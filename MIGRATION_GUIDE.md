# Migration Guide: Inlyne Dependency Upgrades

## Table of Contents
1. [wgpu 0.16 → 28.0](#1-wgpu-016--280)
2. [winit 0.28 → 0.30](#2-winit-028--030)
3. [glyphon 0.3 → 0.10](#3-glyphon-03--010)
4. [cosmic-text 0.9 → 0.15+](#4-cosmic-text-09--015)
5. [taffy 0.3 → 0.9](#5-taffy-03--09)
6. [resvg 0.39 → 0.45+](#6-resvg-039--045)
7. [Cross-cutting Migration Patterns](#7-cross-cutting-migration-patterns)

---

## 1. WGPU: 0.16 → 28.0

The jump spans many major releases: 0.17, 0.18, 0.19, 22.0 (first
semver release after "arcanization"), 23, 24, 25, 26, 27, 28.

### A) Instance Creation

```rust
// BEFORE (0.16):
let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::all(),
    dx12_shader_compiler: Default::default(),
});

// AFTER (28.0):
let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::all(),
    ..Default::default()  // many new fields added over versions
});
// NOTE: Pass by reference now (&InstanceDescriptor)
// NOTE: In v29+, Default is removed; use Instance::new_with_display_handle()
```

### B) Surface Creation (Major change in 0.19+)

```rust
// BEFORE (0.16):
// unsafe, no lifetime param
let surface = unsafe { instance.create_surface(&window) }.unwrap();

// AFTER (22.0+):
// Safe, but Surface now has a lifetime param Surface<'window>
// Use Arc<Window> for Surface<'static>:
let window = Arc::new(window);
let surface = instance.create_surface(window.clone()).unwrap();

// Surface<'static> is needed to store in structs easily.
// Window must outlive the Surface — Arc<Window> guarantees this.
```

### C) Device Request

```rust
// BEFORE (0.16):
let (device, queue) = adapter.request_device(
    &wgpu::DeviceDescriptor {
        features: wgpu::Features::empty(),
        limits: wgpu::Limits::default(),
        label: None,
    },
    None,
).await.unwrap();

// AFTER (28.0):
let (device, queue) = adapter.request_device(
    &wgpu::DeviceDescriptor {
        label: None,
        required_features: wgpu::Features::empty(),  // renamed
        required_limits: wgpu::Limits::default(),     // renamed
        memory_hints: wgpu::MemoryHints::default(),   // new field
    },
    None,
).await.unwrap();
```

### D) Surface Configuration

```rust
// BEFORE (0.16):
let caps = surface.get_capabilities(&adapter);
// get_supported_formats, present_modes were separate calls in earlier versions

// AFTER (28.0):
let caps = surface.get_capabilities(&adapter);
// Unified: caps.formats, caps.present_modes, caps.alpha_modes
let config = wgpu::SurfaceConfiguration {
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    format: caps.formats[0],         // or find srgb preferred
    width,
    height,
    present_mode: wgpu::PresentMode::Fifo,
    alpha_mode: caps.alpha_modes[0],
    view_formats: vec![],
    desired_maximum_frame_latency: 2,  // new field in later versions
};
```

### E) RenderPassDescriptor (changed in 0.18)

```rust
// BEFORE (0.16):
encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("pass"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: &view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(color),
            store: true,  // was bool
        },
    })],
    depth_stencil_attachment: None,
});

// AFTER (28.0):
encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    label: Some("pass"),
    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: &view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(color),
            store: wgpu::StoreOp::Store,  // StoreOp enum
        },
    })],
    depth_stencil_attachment: None,
    timestamp_writes: None,        // new in 0.18
    occlusion_query_set: None,     // new in 0.18
});
```

### F) Resource Cloning ("Arcanization" in 22.0)

Starting with wgpu 22.0, Device, Buffer, Texture, Queue, etc. implement
`Clone` via internal `Arc`. No more ID-based handles. `RenderPass` and
`ComputePass` no longer impose lifetime constraints on resources.

```rust
// BEFORE: Had to carefully manage resource lifetimes
// AFTER: Just clone handles freely
let device2 = device.clone();  // cheap Arc clone
```

### G) request_adapter return type

```rust
// BEFORE (0.16):
let adapter: Option<Adapter> = instance.request_adapter(&options).await;
let adapter = adapter.unwrap();

// AFTER (recent):
let adapter: Result<Adapter, _> = instance.request_adapter(&options).await;
let adapter = adapter?;
```

### H) SurfaceTexture Error Handling

```rust
// BEFORE (0.16):
match surface.get_current_texture() {
    Ok(frame) => { /* render */ }
    Err(wgpu::SurfaceError::Lost) => { /* reconfigure */ }
    Err(wgpu::SurfaceError::Outdated) => { /* reconfigure */ }
    Err(e) => { eprintln!("{:?}", e); }
}

// AFTER (28.0 - same pattern, but v29 changes to enum):
// v28: Still Result-based, same as above
// v29+: Returns CurrentSurfaceTexture enum instead of Result
// match surface.get_current_texture() {
//     wgpu::CurrentSurfaceTexture::Success(frame) => { /* render */ }
//     wgpu::CurrentSurfaceTexture::Outdated => { /* reconfigure */ }
//     ...
// }
```

### I) Push Constants → Immediates (v28)

```rust
// BEFORE: "Push Constants" terminology
// AFTER (v28): Renamed to "Immediates"
// WGSL: var<immediate> my_imm: MyImmediate;
```

---

## 2. WINIT: 0.28 → 0.30

This is the most architecturally significant change. The closure-based
event loop is replaced with the `ApplicationHandler` trait.

### BEFORE (0.28) — Closure-based event loop

```rust
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("My App")
        .build(&event_loop)
        .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested, ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size), ..
            } => {
                // resize surface
            }
            Event::RedrawRequested(_) => {
                // render
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}
```

### AFTER (0.30) — ApplicationHandler trait

```rust
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use std::sync::Arc;

struct App {
    window: Option<Arc<Window>>,
    // ... other state (wgpu device, surface, etc.) as Option<T>
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Called when the platform is ready for render surfaces.
        // Create the window HERE, not before run_app().
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("My App")
            ).unwrap()
        );

        // Create wgpu surface HERE (needs the window to exist)
        // let surface = instance.create_surface(window.clone()).unwrap();

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Render here
                // (was Event::RedrawRequested in 0.28)
            }
            WindowEvent::Resized(size) => {
                // Resize surface
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Replaces Event::MainEventsCleared
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // App is going to background (mobile platforms)
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        // Must drop wgpu surfaces here (Android)
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();  // now returns Result
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App { window: None };
    event_loop.run_app(&mut app).unwrap();       // run() -> run_app()
}
```

### Event Name Mapping Table

| winit 0.28                          | winit 0.30                                 |
|-------------------------------------|--------------------------------------------|
| `Event::RedrawRequested(_)`         | `WindowEvent::RedrawRequested`             |
| `Event::MainEventsCleared`          | `about_to_wait()`                          |
| `Event::Resumed`                    | `can_create_surfaces()` / `resumed()`      |
| `Event::Suspended`                  | `suspended()`                              |
| `Event::WindowEvent { event, .. }`  | `window_event(.., event)`                  |
| `WindowBuilder::new()`             | `Window::default_attributes()`             |
| `.build(&event_loop)`              | `event_loop.create_window(attrs)`          |
| `*control_flow = ControlFlow::Exit` | `event_loop.exit()`                        |
| `event_loop.run(closure)`          | `event_loop.run_app(&mut handler)`         |
| `EventLoop::new()`                 | `EventLoop::new().unwrap()` (returns Result)|

### Critical Notes

- **Window must be created inside `can_create_surfaces()`**, not before
  `run_app()`. On Android, native window handles don't exist until this
  callback fires.
- **Use `Arc<Window>`** for wgpu `Surface<'static>` compatibility.
- **WASM note:** You cannot use `pollster::block_on` inside
  `can_create_surfaces()` on WASM. Use `wasm_bindgen_futures::spawn_local`
  for async wgpu initialization.
- **macOS bug in 0.30.0:** Panic on exit fixed in 0.30.1+. Workaround:
  manually `self.window.take()` in `CloseRequested` before `exit()`.

### Looking ahead to 0.31 (beta)

- `Window::inner_size()` → `Window::surface_size()`
- `ActiveEventLoop` and `Window` become traits (returns `Box<dyn Window>`)
- `Cursor*`/`Touch` events unified into `Pointer*` events

---

## 3. GLYPHON: 0.3 → 0.10

Glyphon 0.3 depended on `cosmic-text ^0.9` + `wgpu ^0.16`.
Glyphon 0.10 depends on `cosmic-text ^0.15` + `wgpu ^28.0`.

### A) New types: Cache and Viewport

```rust
// NEW in later glyphon versions — must be created during init:
use glyphon::{Cache, Viewport};

let cache = Cache::new(&device);
let viewport = Viewport::new(&device, &cache);
```

### B) TextAtlas::new

```rust
// BEFORE (0.3):
let mut atlas = TextAtlas::new(&device, &queue, swapchain_format);

// AFTER (0.10):
let cache = Cache::new(&device);
let mut atlas = TextAtlas::new(
    &device,
    &queue,
    &cache,            // NEW: Cache parameter
    swapchain_format,
);
```

### C) TextRenderer::new — unchanged

```rust
// Same in both versions:
let text_renderer = TextRenderer::new(
    &mut atlas,
    &device,
    MultisampleState::default(),
    None,  // depth_stencil
);
```

### D) TextRenderer::prepare — Viewport replaces Resolution

```rust
// BEFORE (0.3):
text_renderer.prepare(
    &device,
    &queue,
    &mut font_system,
    &mut atlas,
    Resolution { width, height },  // screen_resolution directly
    [TextArea {
        buffer: &buffer,
        left: 10.0,
        top: 10.0,
        scale: 1.0,
        bounds: TextBounds { left: 0, top: 0, right: 600, bottom: 160 },
        default_color: Color::rgb(255, 255, 255),
    }],
    &mut swash_cache,
)?;

// AFTER (0.10):
// First, update the viewport each frame with current resolution:
viewport.update(
    &queue,
    Resolution { width: config.width, height: config.height },
);

text_renderer.prepare(
    &device,
    &queue,
    &mut font_system,
    &mut atlas,
    &viewport,          // Viewport object instead of Resolution
    [TextArea {
        buffer: &buffer,
        left: 10.0,
        top: 10.0,
        scale: 1.0,
        bounds: TextBounds { left: 0, top: 0, right: 600, bottom: 160 },
        default_color: Color::rgb(255, 255, 255),
        custom_glyphs: &[],  // NEW required field
    }],
    &mut swash_cache,
)?;
```

### E) TextRenderer::render — gains Viewport parameter

```rust
// BEFORE (0.3):
text_renderer.render(&atlas, &mut render_pass)?;

// AFTER (0.10):
text_renderer.render(&atlas, &viewport, &mut render_pass)?;
//                           ^^^^^^^^^  NEW parameter
```

### F) TextArea struct — new field

```rust
// BEFORE (0.3):
TextArea {
    buffer: &buffer,
    left: 10.0,
    top: 10.0,
    scale: 1.0,
    bounds: TextBounds { .. },
    default_color: Color::rgb(255, 255, 255),
}

// AFTER (0.10):
TextArea {
    buffer: &buffer,
    left: 10.0,
    top: 10.0,
    scale: 1.0,
    bounds: TextBounds { .. },
    default_color: Color::rgb(255, 255, 255),
    custom_glyphs: &[],  // NEW: required, use empty slice if no custom glyphs
}
```

### G) Post-frame cleanup

```rust
// Call after each frame to free unused atlas entries:
atlas.trim();
```

### H) Additional prepare variants (0.10)

```rust
// prepare_with_depth — maps metadata to f32 depth values
// prepare_with_custom — custom glyph rasterization callback
// prepare_with_depth_and_custom — both combined
```

---

## 4. COSMIC-TEXT: 0.9 → 0.15+

### A) FontSystem::new — unchanged

```rust
// Same in both versions:
let mut font_system = FontSystem::new();

// New constructors available in 0.15+:
// FontSystem::new_with_fonts(fonts)
// FontSystem::new_with_locale_and_db(locale, db)
```

### B) Buffer::new — unchanged

```rust
// Same in both versions:
let mut buffer = Buffer::new(&mut font_system, Metrics::new(30.0, 42.0));

// Also available:
let mut buffer = Buffer::new_empty(Metrics::new(30.0, 42.0));

// NOTE: Panics if line_height or font_size is 0 (enforced in both versions)
```

### C) Buffer::set_text — new alignment parameter

```rust
// BEFORE (0.9):
buffer.set_text(
    &mut font_system,
    "Hello world",
    Attrs::new(),
    Shaping::Advanced,
);

// AFTER (0.15+):
buffer.set_text(
    &mut font_system,
    "Hello world",
    Attrs::new(),
    Shaping::Advanced,
    None,              // NEW: alignment parameter (Option<Align>)
);

// To set alignment:
use cosmic_text::Align;
buffer.set_text(
    &mut font_system,
    "Hello world",
    Attrs::new(),
    Shaping::Advanced,
    Some(Align::Center),
);
```

### D) Buffer::set_size — Option<f32> instead of f32

```rust
// BEFORE (0.9):
buffer.set_size(&mut font_system, 800.0, 600.0);

// AFTER (0.15+):
buffer.set_size(&mut font_system, Some(800.0), Some(600.0));
// Use None for unbounded dimension:
buffer.set_size(&mut font_system, Some(800.0), None);
```

### E) Buffer::set_rich_text — reference + alignment

```rust
// BEFORE (0.9):
buffer.set_rich_text(
    &mut font_system,
    [
        ("hello, ", attrs.clone()),
        ("world", attrs.clone().family(Family::Monospace)),
    ],
    Attrs::new(),           // owned
    Shaping::Advanced,
);

// AFTER (0.15+):
buffer.set_rich_text(
    &mut font_system,
    [
        ("hello, ", attrs.clone()),
        ("world", attrs.clone().family(Family::Monospace)),
    ],
    &Attrs::new(),          // now takes reference
    Shaping::Advanced,
    None,                   // NEW: alignment parameter
);
```

### F) shape_until_scroll — new prune parameter

```rust
// BEFORE (0.9):
buffer.shape_until_scroll(&mut font_system);

// AFTER (0.15+):
buffer.shape_until_scroll(&mut font_system, false);
// prune: bool — whether to discard shaped data for off-screen lines
// Use `true` for memory savings on very long documents
```

### G) Buffer::draw — unchanged pattern

```rust
// Same callback pattern in both versions:
buffer.draw(
    &mut font_system,
    &mut swash_cache,
    cosmic_text::Color::rgb(255, 255, 255),
    |x, y, w, h, color| {
        // pixel placement callback
    },
);
```

### H) New Renderer trait (0.16+)

```rust
// 0.16+ adds a Renderer trait for more flexible rendering:
buffer.render(&mut font_system, &mut my_renderer, color);
// Alternative to buffer.draw() for custom renderers
```

### I) New features in 0.15+

```rust
// Variable font support (0.15)
// Hinting configuration (0.16+):
buffer.set_hinting(Hinting::Full);    // Full, Light, None

// Ellipsizing (0.18+):
buffer.set_ellipsize(Ellipsize::End);   // Start, Middle, End

// Attrs::matches() removed in 0.17 (incompatible with new fallback logic)
```

### J) Dependency changes

```
0.9:  rustybuzz for shaping
0.15: Replaced rustybuzz with HarfRust
0.15: fontdb updated to 0.23
0.15: skrifa 0.37+ for font scaling
```

---

## 5. TAFFY: 0.3 → 0.9

### A) Taffy → TaffyTree\<T\> (0.4)

```rust
// BEFORE (0.3):
use taffy::prelude::*;
let mut taffy = Taffy::new();

// AFTER (0.4+):
use taffy::prelude::*;
let mut taffy: TaffyTree<()> = TaffyTree::new();

// With a user-defined context type:
let mut taffy: TaffyTree<MyContext> = TaffyTree::new();
```

### B) points() → length() (0.4)

```rust
// BEFORE (0.3):
use taffy::prelude::*;
let style = Style {
    size: Size {
        width: points(800.0),
        height: points(100.0),
    },
    ..Default::default()
};

// AFTER (0.4+):
let style = Style {
    size: Size {
        width: length(800.0),
        height: length(100.0),
    },
    ..Default::default()
};
```

### C) Measure Functions — Major Rework (0.4)

```rust
// BEFORE (0.3):
// Per-node measure closures
let leaf = tree.new_leaf_with_measure(
    Style::DEFAULT,
    |known_dimensions: Size<Option<f32>>,
     available_space: Size<AvailableSpace>| {
        Size { width: 100.0, height: 50.0 }
    },
)?;
tree.compute_layout(root, Size::MAX_CONTENT)?;

// AFTER (0.4+):
// Per-node context + single global measure function
#[derive(Clone)]
struct TextMeasure {
    width: f32,
    height: f32,
}

let leaf = tree.new_leaf_with_context(
    Style::DEFAULT,
    TextMeasure { width: 100.0, height: 50.0 },
)?;

tree.compute_layout_with_measure(
    root,
    Size::MAX_CONTENT,
    |known_dimensions, available_space, _node_id, node_context, _style| {
        let ctx = node_context.unwrap();
        Size { width: ctx.width, height: ctx.height }
    },
);

// Without measure (pure layout):
tree.compute_layout(root, Size::MAX_CONTENT);
```

### D) Node creation renames

```rust
// BEFORE (0.3):
tree.new_leaf(style)                         // leaf without measure
tree.new_leaf_with_measure(style, closure)   // leaf with measure
tree.new_with_children(style, &[child1, child2])

// AFTER (0.4+):
tree.new_leaf(style)                         // same
tree.new_leaf_with_context(style, context)   // context replaces closure
tree.new_with_children(style, &[child1, child2])  // same
```

### E) Style changes (0.4+)

```rust
// AlignContent and JustifyContent merged:
// JustifyContent is now an alias for AlignContent

// New display mode:
display: Display::Block,  // NEW in 0.4 (was only Flex and Grid)

// New overflow property:
overflow: Point {
    x: Overflow::Scroll,
    y: Overflow::Scroll,
},

// New scrollbar_width:
scrollbar_width: 15.0,
```

### F) compute_layout return type

```rust
// BEFORE (0.3):
tree.compute_layout(root, size)?;  // returns TaffyResult<()>

// AFTER (0.4+):
tree.compute_layout(root, size);   // returns () (no Result)
```

### G) Style is Generic in 0.9 (Named Grid Lines)

```rust
// 0.9: Style struct is generic over a string type (CheapCloneStr trait)
// For basic use, this is transparent:
let style = Style::DEFAULT;  // still works

// Named grid lines and areas (new in 0.9):
// TrackSizingFunction renamed to GridTemplateComponent
// NonRepeatedTrackSizingFunction renamed to TrackSizingFunction
// New field: Style::grid_template_areas
```

### H) Layout access — unchanged

```rust
// Same in both:
let layout = tree.layout(node)?;
println!("x={}, y={}, w={}, h={}", layout.location.x, layout.location.y,
         layout.size.width, layout.size.height);
```

### I) Rounding control

```rust
// Available since 0.3.3:
tree.enable_rounding();
tree.disable_rounding();

// Unrounded layout access (0.7.1+):
let raw = tree.get_unrounded_layout(node);
```

---

## 6. RESVG: 0.39 → 0.45+

### A) Core Rendering — Mostly Stable

```rust
// Both versions (basic pattern unchanged):
let opt = usvg::Options::default();
let tree = usvg::Tree::from_data(&svg_data, &opt)?;
let size = tree.size().to_int_size();
let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
```

### B) Immutability (0.40) — Major Architectural Change

```rust
// BEFORE (0.39):
// usvg::Tree was mutable after creation, used Rc<RefCell>
let mut tree = usvg::Tree::from_data(&svg_data, &opt)?;
// Could modify nodes after parsing

// AFTER (0.40+):
// All usvg types are immutable after creation
let tree = usvg::Tree::from_data(&svg_data, &opt)?;
// tree is Send + Sync (uses Arc instead of Rc<RefCell>)
// Cannot modify after parse — do transformations during parse via Options
```

### C) Tree Structure Change (0.38+)

```rust
// BEFORE: Tree used rctree crate (Rc<RefCell<Node>>)
// Had risk of RefCell borrow panics at runtime

// AFTER (0.38+): Regular Rust enum tree
// Standard Rust mutability rules apply
// Node::abs_transform() is O(1) — precalculated
// Bounding boxes precalculated in object and canvas coordinates
```

### D) Visibility → bool (0.42)

```rust
// BEFORE (0.39):
match node.visibility() {
    usvg::Visibility::Visible => { /* ... */ }
    usvg::Visibility::Hidden => { /* ... */ }
    usvg::Visibility::Collapse => { /* ... */ }
}

// AFTER (0.42+):
if node.is_visible() {
    // ...
}
// Simple bool replaces the enum
```

### E) Font Control (0.42+)

```rust
// NEW: Custom font resolver
let mut opt = usvg::Options::default();
opt.font_resolver = usvg::FontResolver {
    select_font: Box::new(|font_family, properties, db| {
        // Custom font matching logic
        db.query(&fontdb::Query { .. })
    }),
    select_fallback: Box::new(|c, properties, db| {
        // Custom fallback logic
        None
    }),
};

// Color font support: COLRv0, COLRv1, sbix, CBDT, SVG tables (emoji)
```

### F) License Change (0.45)

```
BEFORE: MPL-2.0
AFTER:  Apache-2.0 OR MIT
```

### G) tiny-skia Version Compatibility

```
resvg 0.39-0.44: tiny-skia 0.11.x
resvg 0.45-0.46: tiny-skia 0.11.x
resvg 0.47:      tiny-skia 0.12.x   // BREAKING if you use tiny-skia directly
```

### H) New Features

```rust
// 0.42: Color fonts (emoji), viewbox flattening
// 0.45: background_color attribute, !important CSS flag, Luma JPEG
// 0.46: Nested SVGs, glyph outline caching, nested embedded images
// 0.47: Variable fonts (font-variation-settings), radial gradient focal radius
```

---

## 7. Cross-cutting Migration Patterns

### Typical App Struct (Before → After)

```rust
// BEFORE (winit 0.28 + wgpu 0.16 + glyphon 0.3):
struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    buffer: Buffer,
    window: Window,
}

// AFTER (winit 0.30 + wgpu 28.0 + glyphon 0.10):
struct App {
    state: Option<State>,  // None until can_create_surfaces
}

struct State {
    surface: wgpu::Surface<'static>,   // lifetime param added
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    font_system: FontSystem,
    swash_cache: SwashCache,
    cache: glyphon::Cache,             // NEW
    viewport: glyphon::Viewport,       // NEW
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    buffer: Buffer,
    window: Arc<Window>,               // Arc for Surface<'static>
}
```

### Initialization Order (After)

```rust
impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // 1. Create window
        let window = Arc::new(event_loop.create_window(
            Window::default_attributes().with_title("inlyne")
        ).unwrap());

        // 2. Create wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        // 3. Create surface from Arc<Window>
        let surface = instance.create_surface(window.clone()).unwrap();

        // 4. Request adapter (compatible with surface)
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
        ).unwrap();

        // 5. Request device
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }, None)
        ).unwrap();

        // 6. Configure surface
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration { /* ... */ };
        surface.configure(&device, &config);

        // 7. Set up glyphon
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let viewport = glyphon::Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer = TextRenderer::new(
            &mut atlas, &device, MultisampleState::default(), None,
        );

        // 8. Create text buffer
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 20.0));
        buffer.set_size(&mut font_system, Some(800.0), Some(600.0));
        buffer.set_text(
            &mut font_system, "Hello", Attrs::new(), Shaping::Advanced, None,
        );
        buffer.shape_until_scroll(&mut font_system, false);

        self.state = Some(State {
            surface, device, queue, config,
            font_system, swash_cache, cache, viewport,
            atlas, text_renderer, buffer, window,
        });
    }
}
```

### Render Loop (After)

```rust
WindowEvent::RedrawRequested => {
    let state = self.state.as_mut().unwrap();

    // Update viewport
    state.viewport.update(
        &state.queue,
        Resolution {
            width: state.config.width,
            height: state.config.height,
        },
    );

    // Prepare text
    state.text_renderer.prepare(
        &state.device,
        &state.queue,
        &mut state.font_system,
        &mut state.atlas,
        &state.viewport,
        [TextArea {
            buffer: &state.buffer,
            left: 10.0,
            top: 10.0,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: state.config.width as i32,
                bottom: state.config.height as i32,
            },
            default_color: Color::rgb(255, 255, 255),
            custom_glyphs: &[],
        }],
        &mut state.swash_cache,
    ).unwrap();

    // Render
    let frame = state.surface.get_current_texture().unwrap();
    let view = frame.texture.create_view(&Default::default());
    let mut encoder = state.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor { label: Some("encoder") }
    );
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        state.text_renderer.render(
            &state.atlas, &state.viewport, &mut pass,
        ).unwrap();
    }
    state.queue.submit(std::iter::once(encoder.finish()));
    frame.present();
    state.atlas.trim();
}
```

### Taffy Layout Integration (After)

```rust
use taffy::prelude::*;

// Create tree
let mut taffy: TaffyTree<()> = TaffyTree::new();

// Build layout
let header = taffy.new_leaf(Style {
    size: Size { width: percent(1.0), height: length(60.0) },
    ..Default::default()
})?;

let content = taffy.new_leaf(Style {
    size: Size { width: percent(1.0), height: auto() },
    flex_grow: 1.0,
    ..Default::default()
})?;

let root = taffy.new_with_children(
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        size: Size { width: length(800.0), height: length(600.0) },
        ..Default::default()
    },
    &[header, content],
)?;

taffy.compute_layout(root, Size::MAX_CONTENT);

let layout = taffy.layout(root)?;
```

### SVG Rendering with resvg (After)

```rust
let opt = usvg::Options::default();
let tree = usvg::Tree::from_data(&svg_data, &opt)?;
// tree is now immutable and Send+Sync

let size = tree.size().to_int_size();
let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

// Convert to RGBA bytes for wgpu texture upload:
let rgba = pixmap.data();  // &[u8] in premultiplied RGBA
```
