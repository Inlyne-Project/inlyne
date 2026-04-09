
# Inlyne Crate API Usage Report for Migration
# From: wgpu 0.16, glyphon 0.3, winit 0.28, taffy 0.3, resvg 0.39
# To:   wgpu 28, glyphon 0.10, winit 0.30, taffy 0.9, resvg 0.45+

================================================================================
## FILE: src/renderer.rs
================================================================================

### IMPORTS:
  - glyphon::{Resolution, SwashCache, TextArea, TextAtlas, TextRenderer}
  - wgpu::util::DeviceExt
  - wgpu::{BindGroup, Buffer, IndexFormat, MultisampleState, TextureFormat}
  - winit::window::Window

### WGPU USAGE (HEAVY - primary GPU setup):

  TYPES USED:
  - wgpu::SurfaceConfiguration (stored as field)
  - wgpu::Surface (stored as field)
  - wgpu::Device (stored as field)
  - wgpu::RenderPipeline (stored as field)
  - wgpu::Queue (stored as field)
  - wgpu::TextureFormat (stored as field)
  - wgpu::BindGroup (used in image_bindgroups)
  - wgpu::Buffer (used for vertex/index buffers)
  - wgpu::IndexFormat (IndexFormat::Uint16)
  - wgpu::MultisampleState
  - wgpu::Color (for background clear color)
  - wgpu::SurfaceError (Lost, Outdated, Timeout, OutOfMemory variants)
  - wgpu::BufferAddress (as cast target)

  INSTANCE/ADAPTER/DEVICE CREATION PATTERN (Renderer::new):
    1. wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), dx12_shader_compiler: wgpu::Dx12Compiler::Fxc })
       MIGRATION: InstanceDescriptor changed significantly in wgpu 28
    2. unsafe { instance.create_surface(window) }
       MIGRATION: In wgpu 28, create_surface takes impl Into<SurfaceTarget<'window>>; no longer unsafe
    3. instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference, force_fallback_adapter, compatible_surface })
    4. adapter.request_device(&wgpu::DeviceDescriptor { label, features: wgpu::Features::empty(), limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()) }, None)
       MIGRATION: DeviceDescriptor changed - may need required_features/required_limits fields

  SHADER MODULE:
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label, source: wgpu::ShaderSource::Wgsl(...) })

  PIPELINE CREATION:
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label, bind_group_layouts, push_constant_ranges })
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label, layout,
      vertex: wgpu::VertexState { module, entry_point: "vs_main", buffers },
      fragment: Some(wgpu::FragmentState { module, entry_point: "fs_main", targets }),
      primitive: wgpu::PrimitiveState::default(),
      depth_stencil: None,
      multisample: wgpu::MultisampleState::default(),
      multiview: None
    })
    MIGRATION: entry_point changed from &str to Option<&str> in newer wgpu

  VERTEX BUFFER LAYOUT:
    wgpu::VertexBufferLayout { array_stride, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![...] }

  SURFACE CONFIGURATION:
    wgpu::SurfaceConfiguration { usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format, width, height, present_mode: wgpu::PresentMode::Fifo, alpha_mode, view_formats: vec![] }
    surface.get_capabilities(&adapter) -> caps.formats, caps.alpha_modes
    surface.configure(&device, &config)

  RENDER PASS:
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, resolve_target, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: true } })], depth_stencil_attachment: None })
    MIGRATION: store changed from bool to wgpu::StoreOp enum in newer wgpu

  BUFFER CREATION:
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label, contents, usage: wgpu::BufferUsages::VERTEX/INDEX })

  RENDER PASS COMMANDS:
    rpass.set_pipeline(&pipeline)
    rpass.set_vertex_buffer(0, buf.slice(..))
    rpass.set_index_buffer(buf.slice(..), wgpu::IndexFormat::Uint16)
    rpass.draw_indexed(range, 0, 0..1)
    rpass.set_bind_group(0, bindgroup, &[])

  FRAME PRESENTATION:
    surface.get_current_texture() -> frame
    frame.texture.create_view(&wgpu::TextureViewDescriptor::default())
    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label })
    queue.submit(Some(encoder.finish()))
    frame.present()

### GLYPHON USAGE:
  - SwashCache::new()
  - TextAtlas::new(&device, &queue, surface_format)
    MIGRATION: TextAtlas::new signature changed in glyphon 0.10
  - TextRenderer::new(&mut text_atlas, &device, MultisampleState::default(), None)
    MIGRATION: TextRenderer::new signature changed
  - Resolution { width, height } (used in text_renderer.prepare)
  - text_renderer.prepare(&device, &queue, &mut font_system, &mut text_atlas, Resolution{..}, text_areas, &mut swash_cache)
    MIGRATION: prepare() signature changed significantly
  - text_renderer.render(&text_atlas, &mut rpass)
  - text_atlas.trim()
  - glyphon::Color::rgb(255, 0, 255) (used for debug bounds)

### WINIT USAGE:
  - winit::window::Window (passed to Renderer::new, used for inner_size())

================================================================================
## FILE: src/main.rs
================================================================================

### IMPORTS:
  - taffy::Taffy
  - winit::event::{ElementState, Event, KeyboardInput, ModifiersState, MouseButton, MouseScrollDelta, WindowEvent}
  - winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder}
  - winit::window::{CursorIcon, Window, WindowBuilder}
  - raw_window_handle::HasRawDisplayHandle

### WINIT USAGE (HEAVY - entire event loop):

  EVENT LOOP CREATION:
    EventLoopBuilder::<InlyneEvent>::with_user_event().build()
    MIGRATION: In winit 0.30, EventLoopBuilder API changed. with_user_event() removed; use EventLoop::with_user_event()

  WINDOW CREATION:
    WindowBuilder::new().with_title(...)
      .with_decorations(bool)
      .with_position(winit::dpi::PhysicalPosition::new(x, y))
      .with_inner_size(winit::dpi::PhysicalSize::new(w, h))
      .build(&event_loop)
    MIGRATION: In winit 0.30, windows are created differently via ActiveEventLoop::create_window()
    
    Wayland platform extension:
      winit::platform::wayland::WindowBuilderExtWayland -> wb.with_name("inlyne", "")
      MIGRATION: WindowBuilderExtWayland renamed/changed in winit 0.30

  CUSTOM EVENT / EVENT LOOP PROXY:
    event_loop.create_proxy() -> EventLoopProxy<InlyneEvent>
    proxy.send_event(InlyneEvent::...) 
    Event::UserEvent(inlyne_event) pattern matching

  EVENT LOOP RUN PATTERN:
    event_loop.run(move |event, _, control_flow| {
      *control_flow = ControlFlow::Wait;
      match event {
        Event::UserEvent(e) => ...
        Event::RedrawRequested(_) => ...
        Event::WindowEvent { event, .. } => match event {
          WindowEvent::Resized(size) => ...
          WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit
          WindowEvent::MouseWheel { delta, .. } => match delta { PixelDelta, LineDelta }
          WindowEvent::CursorMoved { position, .. } => ...
          WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => ...
          WindowEvent::ModifiersChanged(new_state) => ...
          WindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode, .. }, .. } => ...
        }
        Event::MainEventsCleared => ...
      }
    });
    MIGRATION: In winit 0.30, event loop is trait-based (ApplicationHandler). Events restructured:
      - Event::MainEventsCleared -> about_to_wait()
      - Event::RedrawRequested -> ApplicationHandler::window_event with WindowEvent::RedrawRequested
      - ControlFlow::Exit -> elwt.exit()
      - Keyboard input changed from KeyboardInput{virtual_keycode, scancode} to KeyEvent{physical_key, logical_key}
      - ModifiersChanged now carries Modifiers not ModifiersState
      - MouseScrollDelta variants unchanged but accessed differently

  WINDOW METHODS:
    window.inner_size()
    window.scale_factor()
    window.set_cursor_icon(CursorIcon::Default/Text/Hand)
    window.request_redraw()
    window.set_title(...)
    window.raw_display_handle()

  RAW WINDOW HANDLE:
    raw_window_handle::HasRawDisplayHandle -> event_loop.raw_display_handle()
    MIGRATION: In winit 0.30, uses raw-window-handle 0.6 with HasDisplayHandle trait

### TAFFY USAGE:
  - taffy::Taffy (type alias used for find_hoverable parameter)

================================================================================
## FILE: src/text.rs
================================================================================

### IMPORTS:
  - glyphon::{Affinity, Attrs, AttrsList, BufferLine, Color, Cursor, FamilyOwned, FontSystem, LayoutGlyph, Shaping, Style, SwashCache, TextArea, TextBounds, Weight}
  - taffy::prelude::{AvailableSpace, Size as TaffySize}

### GLYPHON/COSMIC-TEXT USAGE (HEAVY - text rendering pipeline):

  TYPES USED:
  - FontSystem (stored in Arc<Mutex<FontSystem>>)
  - SwashCache (stored in TextSystem)
  - glyphon::TextRenderer (stored in TextSystem)
  - glyphon::TextAtlas (stored in TextSystem)
  - glyphon::Buffer (stored in TextCache entries map)
  - glyphon::Metrics (created as Metrics::new(size, line_height))
  - BufferLine (created with BufferLine::new(text, attrs_list, Shaping::Advanced))
  - Attrs (Attrs::new().family(...).weight(...).style(...).color(...).metadata(...))
  - AttrsList (AttrsList::new(Attrs::new()), then .add_span(range, attrs))
  - Color (Color::rgb(255,255,255), Color::rgba(r,g,b,a))
  - Cursor (Cursor::new(line, index), Cursor::new_with_affinity(line, index, affinity))
  - Affinity (Affinity::Before, Affinity::After)
  - FamilyOwned (FamilyOwned::SansSerif, .as_family() -> Family<'_>)
  - Style (Style::Italic, Style::Normal)
  - Weight (Weight::BOLD, Weight::NORMAL)
  - Shaping (Shaping::Advanced)
  - TextArea { buffer, left, top, bounds, default_color, scale }
    MIGRATION: TextArea fields may change in glyphon 0.10
  - TextBounds (TextBounds::default())
  - LayoutGlyph (accessed as glyph.start, glyph.end, glyph.metadata)
  - glyphon::Family<'a> (used in Font struct)

  BUFFER OPERATIONS:
  - glyphon::Buffer::new(font_system, metrics)
  - buffer.set_size(font_system, width, height)
    MIGRATION: set_size signature changed to take Option<f32> in newer cosmic-text
  - buffer.lines.clear()
  - buffer.lines.push(buffer_line)
  - buffer.shape_until_scroll(font_system)
  - buffer.hit(x, y) -> Option<Cursor>
  - buffer.layout_runs() -> iterator of LayoutRun
    - run.line_w, run.line_i, run.glyphs, run.rtl, run.text
    - run.highlight(start_cursor, end_cursor) -> Option<(x, w)>

  TEXT SYSTEM STRUCT:
    pub struct TextSystem {
      pub font_system: Arc<Mutex<FontSystem>>,
      pub text_renderer: glyphon::TextRenderer,
      pub text_atlas: glyphon::TextAtlas,
      pub text_cache: Arc<Mutex<TextCache>>,
      pub swash_cache: SwashCache,
    }

  TEXT CACHE PATTERN:
    TextCache stores FxHashMap<KeyHash, glyphon::Buffer>
    allocate() creates Buffer, sets size, pushes BufferLines, calls shape_until_scroll
    trim() retains only recently_used entries

### TAFFY USAGE:
  - TaffySize<Option<f32>> (for known_dimensions in measure)
  - TaffySize<taffy::style::AvailableSpace> (for available_space)
  - AvailableSpace::Definite, AvailableSpace::MinContent, AvailableSpace::MaxContent
  - Returns TaffySize<f32> from measure

================================================================================
## FILE: src/fonts.rs
================================================================================

### IMPORTS:
  - glyphon::FontSystem

### GLYPHON USAGE:
  - FontSystem::new()
  - font_system.db_mut().set_sans_serif_family(name)
  - font_system.db_mut().set_monospace_family(name)
  MIGRATION: db_mut() returns &mut fontdb::Database; API should be similar

================================================================================
## FILE: src/image/mod.rs
================================================================================

### IMPORTS:
  - resvg::{tiny_skia, usvg}
  - usvg::fontdb
  - wgpu::util::DeviceExt
  - wgpu::{BindGroup, Device, TextureFormat}

### WGPU USAGE (image texture pipeline):

  IMAGE BIND GROUP CREATION (Image::create_bind_group):
    wgpu::Extent3d { width, height, depth_or_array_layers: 1 }
    device.create_texture(&wgpu::TextureDescriptor {
      size, mip_level_count: 1, sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8UnormSrgb,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      label, view_formats: &[]
    })
    queue.write_texture(
      wgpu::ImageCopyTexture { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
      &rgba_image,
      wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
      texture_size
    )
    texture.create_view(&wgpu::TextureViewDescriptor::default())
    device.create_bind_group(&wgpu::BindGroupDescriptor {
      layout, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
      ], label
    })

  IMAGE RENDERER (ImageRenderer::new):
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      entries: &[
        wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } }, count: None },
        wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
      ], label
    })
    device.create_pipeline_layout(...)
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label, source: wgpu::ShaderSource::Wgsl(...) })
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      ..., fragment: Some(wgpu::FragmentState { ..., targets: &[Some(wgpu::ColorTargetState {
        format, blend: Some(wgpu::BlendState { color: wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha }, alpha: wgpu::BlendComponent::REPLACE }),
        write_mask: wgpu::ColorWrites::ALL
      })] })
    })
    device.create_buffer_init(...)
    device.create_sampler(&wgpu::SamplerDescriptor { address_mode_u/v/w: wgpu::AddressMode::ClampToEdge, mag/min/mipmap_filter: wgpu::FilterMode::Linear, ..Default::default() })

  STORED FIELDS:
    render_pipeline: wgpu::RenderPipeline
    index_buf: wgpu::Buffer
    bindgroup_layout: wgpu::BindGroupLayout
    sampler: wgpu::Sampler
    bind_group: Option<Arc<wgpu::BindGroup>> (on Image struct)

### RESVG/USVG/TINY_SKIA USAGE (SVG rendering):
  - usvg::Options::default()
  - usvg::Tree::from_data(&image_data, &opt)
  - tree.size -> usvg::Size
  - tree.size.width(), tree.size.height()
  - tree.size.scale_to(tiny_skia::Size)
    MIGRATION: In resvg 0.45+, usvg::Tree API changed significantly:
      - Tree::from_data returns different type
      - tree.size may be accessed differently
      - scale_to may need different approach
  - tree.postprocess(Default::default(), fontdb)
    MIGRATION: postprocess() removed in newer resvg; text is resolved during parsing
  - tiny_skia::Size::from_wh(w, h)
  - tiny_skia::Pixmap::new(w, h)
  - tiny_skia::Transform::default()
  - resvg::render(&tree, transform, &mut pixmap.as_mut())
    MIGRATION: In resvg 0.45+, render() signature changed; it's now tree.render(transform, &mut pixmap.as_mut())
  - pixmap.width(), pixmap.height(), pixmap.data()
  - usvg::fontdb::Database::new()
  - db.load_system_fonts()
    MIGRATION: fontdb is now a separate crate; Database API may differ

================================================================================
## FILE: src/positioner.rs
================================================================================

### IMPORTS:
  - taffy::Taffy

### TAFFY USAGE:
  - Taffy::new()
  - taffy.disable_rounding()
  - Stored as field: pub taffy: Taffy
  - Passed to table.layout() and find_hoverable()
  MIGRATION: In taffy 0.9:
    - Taffy renamed to TaffyTree
    - disable_rounding() API may have changed
    - Node handling changed

================================================================================
## FILE: src/table.rs
================================================================================

### IMPORTS:
  - taffy::node::MeasureFunc
  - taffy::prelude::{auto, line, points, AvailableSpace, Display, Layout, Size as TaffySize, Style, Taffy}
  - taffy::style::JustifyContent

### TAFFY USAGE (HEAVY - table layout):

  TYPES/ENUMS:
  - Style { display: Display::Flex/Display::Grid, size, justify_content, gap, grid_template_columns, grid_row, grid_column, .. }
  - TaffySize { width, height }
  - Layout (accessed as layout.location.x, layout.location.y, layout.size.width, layout.size.height)
  - MeasureFunc::Boxed(Box::new(closure))
  - AvailableSpace::Definite, AvailableSpace::MaxContent
  - JustifyContent::Start
  
  HELPER FUNCTIONS:
  - auto() (for dimensions)
  - line(i16) (for grid placement)
  - points(f32) (for fixed dimensions)

  TAFFY TREE OPERATIONS:
  - taffy.new_leaf_with_measure(style, MeasureFunc::Boxed(...))
  - taffy.new_with_children(style, &children)
  - taffy.compute_layout(root, TaffySize::<AvailableSpace>{...})
  - taffy.layout(node) -> &Layout
  
  MIGRATION: In taffy 0.9:
    - Taffy -> TaffyTree<T>
    - MeasureFunc replaced with NodeContext generic
    - new_leaf_with_measure -> new_leaf_with_context
    - Style field changes (justify_content no longer Option)
    - points() -> length()
    - line() -> may have changed
    - auto() still exists
    - compute_layout takes different args

================================================================================
## OTHER FILES WITH RELEVANT USAGE
================================================================================

### src/color.rs:
  - wgpu::TextureFormat (for native_color function that converts colors based on format)
  - Matches on TextureFormat::Rgba8UnormSrgb, Bgra8UnormSrgb variants

### src/interpreter/mod.rs:
  - wgpu::TextureFormat (stored/passed for color conversion)
  - winit::event_loop::EventLoopProxy (for sending InlyneEvent)
  - winit::window::Window (Arc<Window> for request_redraw proxy)

### src/interpreter/ast.rs:
  - glyphon::FamilyOwned (for font family selection)
  - wgpu::TextureFormat (for color conversion)

### src/keybindings/mod.rs:
  - winit::event::{ModifiersState, ScanCode, VirtualKeyCode as VirtKey}
  MIGRATION: In winit 0.30:
    - VirtualKeyCode replaced with KeyCode
    - ScanCode replaced with PhysicalKey::Code
    - ModifiersState replaced with Modifiers

### src/keybindings/serialization.rs:
  - winit::event::{ModifiersState, VirtualKeyCode as VirtKey}

### src/keybindings/defaults.rs:
  - winit::event::{ModifiersState, VirtualKeyCode as VirtKey}

### src/keybindings/mappings.rs:
  - winit::event::VirtualKeyCode as VirtKey

### src/utils.rs:
  - winit::window::CursorIcon

### src/debug_impls.rs:
  - glyphon::FamilyOwned

### src/file_watcher/mod.rs:
  - winit::event_loop::EventLoopProxy

================================================================================
## DEPENDENCY VERSIONS (from Cargo.toml)
================================================================================
  Current:
    glyphon = "0.3"
    wgpu = "0.16"
    winit = "0.28.7"
    taffy = "0.3.19"
    resvg = "0.39.0"
    fontdb = "0.14.1"
    raw-window-handle = "0.5.2"

  Target:
    glyphon = "0.10"      (cosmic-text 0.15)
    wgpu = "28"
    winit = "0.30"
    taffy = "0.9"
    resvg = "0.45+"
    fontdb = (version bundled with resvg 0.45)
    raw-window-handle = "0.6" (or use HasDisplayHandle)

================================================================================
## KEY MIGRATION PATTERNS SUMMARY
================================================================================

### WGPU 0.16 -> 28 (Major Changes):
  1. Surface creation: No longer unsafe, takes SurfaceTarget
  2. InstanceDescriptor: fields changed (dx12_shader_compiler removed/renamed)
  3. DeviceDescriptor: field names changed (features -> required_features, limits -> required_limits)
  4. RenderPassDescriptor: ops.store changed from bool to StoreOp enum
  5. entry_point in VertexState/FragmentState: &str -> Option<&str>
  6. TextureDescriptor: may have additional required fields
  7. Pipeline creation: mostly similar but check for new required fields
  8. Surface::get_capabilities -> get_default_config or similar

### GLYPHON 0.3 -> 0.10 (Major Changes):
  1. TextAtlas::new() signature changed (may need ColorMode parameter)
  2. TextRenderer::new() signature changed
  3. TextRenderer::prepare() signature changed significantly
  4. Buffer::set_size() takes Option<f32> instead of f32
  5. Buffer::new() may require different args
  6. TextArea struct may have different fields
  7. Color API may have changed
  8. FamilyOwned, Attrs API changes

### WINIT 0.28 -> 0.30 (Architecture Change):
  1. Event loop is now trait-based (ApplicationHandler)
  2. EventLoopBuilder::with_user_event() changed
  3. Window creation via ActiveEventLoop::create_window() instead of WindowBuilder
  4. VirtualKeyCode -> KeyCode
  5. ScanCode -> PhysicalKey
  6. ModifiersState -> Modifiers
  7. KeyboardInput -> KeyEvent
  8. Event::MainEventsCleared -> about_to_wait()
  9. ControlFlow::Exit -> EventLoopWindowTarget::exit()
  10. raw-window-handle 0.5 -> 0.6 (HasRawDisplayHandle -> HasDisplayHandle)

### TAFFY 0.3 -> 0.9 (API Redesign):
  1. Taffy -> TaffyTree<NodeContext>
  2. MeasureFunc removed; use NodeContext generic + measure callback
  3. new_leaf_with_measure -> new_leaf_with_context
  4. points() -> length()
  5. Style fields renamed/restructured
  6. JustifyContent no longer wrapped in Option
  7. Grid API changes

### RESVG 0.39 -> 0.45+ (API Changes):
  1. tree.postprocess() removed; text resolved during parsing
  2. usvg::Tree::from_data() API changed (takes fontdb as param)
  3. resvg::render() becomes method on tree or changed signature
  4. tree.size access pattern changed
  5. scale_to() approach changed
  6. fontdb integration changed
