pub mod color;
pub mod fonts;
pub mod image;
pub mod interpreter;
pub mod opts;
pub mod positioner;
pub mod renderer;
pub mod table;
pub mod text;
pub mod utils;

use crate::image::Image;
use crate::interpreter::HtmlInterpreter;
use crate::opts::Opts;
use crate::table::Table;
use crate::text::Text;

use opts::Args;
use opts::Config;
use positioner::Positioned;
use positioner::Row;
use positioner::Spacer;
use positioner::DEFAULT_MARGIN;
use positioner::DEFAULT_PADDING;
use renderer::Renderer;
use utils::Rect;

use anyhow::Context;
use copypasta::{ClipboardContext, ClipboardProvider};
use text::TextBox;
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;
use winit::event::{ElementState, MouseButton};
use winit::{
    event::{Event, KeyboardInput, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug)]
pub enum InlyneEvent {
    Reposition,
}

pub enum Hoverable<'a> {
    Image(&'a Image),
    Text(&'a Text),
}

#[derive(Debug)]
pub enum Element {
    TextBox(TextBox),
    Spacer(Spacer),
    Image(Image),
    Table(Table),
    Row(Row),
}

impl From<Row> for Element {
    fn from(row: Row) -> Self {
        Element::Row(row)
    }
}
impl From<Image> for Element {
    fn from(image: Image) -> Self {
        Element::Image(image)
    }
}

impl From<Spacer> for Element {
    fn from(spacer: Spacer) -> Self {
        Element::Spacer(spacer)
    }
}

impl From<TextBox> for Element {
    fn from(text_box: TextBox) -> Self {
        Element::TextBox(text_box)
    }
}

impl From<Table> for Element {
    fn from(table: Table) -> Self {
        Element::Table(table)
    }
}

pub struct Inlyne {
    window: Arc<Window>,
    event_loop: EventLoop<InlyneEvent>,
    renderer: Renderer,
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    clipboard: ClipboardContext,
    elements: Vec<Positioned<Element>>,
    args: Args,
}

impl Inlyne {
    pub async fn new(opts: Opts, args: Args) -> anyhow::Result<Self> {
        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Arc::new(Window::new(&event_loop).unwrap());
        window.set_title("Inlyne");
        let renderer = Renderer::new(
            &window,
            event_loop.create_proxy(),
            opts.theme,
            opts.scale.unwrap_or(window.scale_factor() as f32),
            opts.font_opts,
        )
        .await?;
        let clipboard = ClipboardContext::new().unwrap();

        Ok(Self {
            window,
            event_loop,
            renderer,
            element_queue: Arc::new(Mutex::new(VecDeque::new())),
            clipboard,
            elements: Vec::new(),
            args,
        })
    }

    pub fn run(mut self) {
        // Why do we handle resizes lazily like this?
        //
        // For a bit of background. `winit`'s event loop has separate tiers of events. It won't move
        // on to the next tier until all of the events from the current tier have been processed and
        // (here's the kicker) events can keep coming in while all of this is happening. Here are
        // the tiers for reference
        //
        // 1. Window events, User events, Device events
        // --- `MainEventsCleared` sent ---
        // 2. Redraw windows (goes back to 1. after this)
        //
        // The obvious issue here is that expensive events from one tier continually coming in will
        // block us from moving on to the next tier
        //
        // How does this matter in practice?
        //
        // Dragging to resize a window is represented as a mix of window events
        // (`WindowEvent::Resized` and `WindowEvent::CursorMoved`) followed by a final
        // `MainEventsCleared`. With large READMEs a window resize will be expensive because it has
        // to reposition many elements. This means that dragging a window will queue up a lot of
        // window resizes before we can even redraw a window, but it's pointless to calculate a
        // window resize if there is already another size pending
        //
        // Instead we lazily store the size and only reposition elements and request a redraw when
        // we recieve a `MainEventsCleared` indicating that we've finished recieveing the first tier
        // of events. This prevents us from clogging up the queue with a bunch of costly resizes.
        // For more information take a look at
        // https://github.com/trimental/inlyne/issues/25
        let mut pending_resize = None;
        let mut scrollbar_held = false;
        let mut mouse_down = false;
        let mut modifiers = ModifiersState::empty();
        let mut last_loc = (0.0, 0.0);
        let event_loop_proxy = self.event_loop.create_proxy();
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::Reposition => {
                        self.renderer.reposition(&mut self.elements);
                        self.window.request_redraw()
                    }
                },
                Event::RedrawRequested(_) => {
                    let queue = {
                        self.element_queue
                            .try_lock()
                            .map(|mut queue| queue.drain(..).collect::<Vec<Element>>())
                    };
                    if let Ok(queue) = queue {
                        for mut element in queue {
                            // Adds callback for when image is loaded to reposition and redraw
                            match element {
                                Element::Image(ref mut image) => {
                                    image.add_callback(event_loop_proxy.clone());
                                }
                                Element::Row(ref mut row) => {
                                    for element in &mut row.elements {
                                        if let Element::Image(ref mut image) = element.inner {
                                            image.add_callback(event_loop_proxy.clone());
                                        }
                                    }
                                }
                                _ => {}
                            }
                            // Position element and add it to elements
                            let mut positioned_element = Positioned::new(element);
                            self.renderer.positioner.position(
                                &mut self.renderer.glyph_brush,
                                &mut positioned_element,
                                self.renderer.zoom,
                            );
                            self.renderer.positioner.reserved_height +=
                                DEFAULT_PADDING * self.renderer.hidpi_scale * self.renderer.zoom
                                    + positioned_element
                                        .bounds
                                        .as_ref()
                                        .expect("already positioned")
                                        .size
                                        .1;
                            self.elements.push(positioned_element);
                        }
                    }
                    self.renderer
                        .redraw(&mut self.elements)
                        .with_context(|| "Renderer failed to redraw the screen")
                        .unwrap();
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => pending_resize = Some(size),
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::MouseWheel { delta, .. } => {
                        let y_pixel_shift = match delta {
                            MouseScrollDelta::PixelDelta(pos) => {
                                pos.y as f32 * self.renderer.hidpi_scale * self.renderer.zoom
                            }
                            // Arbitrarily pick x30 as the number of pixels to shift per line
                            MouseScrollDelta::LineDelta(_, y_delta) => {
                                y_delta as f32
                                    * 32.0
                                    * self.renderer.hidpi_scale
                                    * self.renderer.zoom
                            }
                        };

                        self.renderer
                            .set_scroll_y(self.renderer.scroll_y - y_pixel_shift);
                        self.window.request_redraw();
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let screen_size = self.renderer.screen_size();
                        let loc = (
                            position.x as f32,
                            position.y as f32 + self.renderer.scroll_y,
                        );
                        last_loc = loc;

                        let cursor_icon = if let Some(hoverable) = Self::find_hoverable(
                            &self.elements,
                            &mut self.renderer.glyph_brush,
                            loc,
                            screen_size,
                            self.renderer.zoom,
                        ) {
                            match hoverable {
                                Hoverable::Image(Image { is_link: None, .. }) => {
                                    CursorIcon::Default
                                }
                                Hoverable::Text(Text { link: None, .. }) => CursorIcon::Text,
                                _some_link => CursorIcon::Hand,
                            }
                        } else {
                            CursorIcon::Default
                        };
                        self.window.set_cursor_icon(cursor_icon);

                        if scrollbar_held
                            || (Rect::new((screen_size.0 - 25., 0.), (25., screen_size.1))
                                .contains(position.into())
                                && mouse_down)
                        {
                            let target_scroll = ((position.y as f32 / screen_size.1)
                                * self.renderer.positioner.reserved_height)
                                - (screen_size.1 / self.renderer.positioner.reserved_height
                                    * screen_size.1);
                            self.renderer.set_scroll_y(target_scroll);
                            self.window.request_redraw();
                            if !scrollbar_held {
                                scrollbar_held = true;
                            }
                        } else if let Some(selection) = &mut self.renderer.selection {
                            if mouse_down {
                                selection.1 = loc;
                                self.window.request_redraw();
                            }
                        }
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => match state {
                        ElementState::Pressed => {
                            // Reset selection
                            if self.renderer.selection.is_some() {
                                self.renderer.selection = None;
                                self.window.request_redraw();
                            }

                            // Try to click a link
                            let screen_size = self.renderer.screen_size();
                            if let Some(hoverable) = Self::find_hoverable(
                                &self.elements,
                                &mut self.renderer.glyph_brush,
                                last_loc,
                                screen_size,
                                self.renderer.zoom,
                            ) {
                                let maybe_link = match hoverable {
                                    Hoverable::Image(Image { is_link, .. }) => is_link,
                                    Hoverable::Text(Text { link, .. }) => link,
                                };

                                if let Some(link) = maybe_link {
                                    let maybe_path = PathBuf::from_str(link).ok();
                                    let is_md = maybe_path.as_ref().map_or(false, |p| {
                                        p.extension().map_or(false, |ext| ext == "md")
                                    });
                                    if is_md {
                                        // Open markdown files ourselves
                                        let mut args = self.args.clone();
                                        args.file_path =
                                            maybe_path.expect("Already checked path extension");
                                        Command::new(
                                            std::env::current_exe()
                                                .unwrap_or_else(|_| "inlyne".into()),
                                        )
                                        .args(args.program_args())
                                        .spawn()
                                        .expect("Could not spawn new inlyne instance");
                                    } else if open::that(link).is_err() {
                                        if let Some(anchor_pos) =
                                            self.renderer.positioner.anchors.get(link)
                                        {
                                            self.renderer.set_scroll_y(*anchor_pos);
                                            self.window.request_redraw();
                                            self.window.set_cursor_icon(CursorIcon::Default);
                                        }
                                    }
                                } else if self.renderer.selection.is_none() {
                                    // Only set selection when not over link
                                    self.renderer.selection = Some((last_loc, last_loc));
                                }
                            } else if self.renderer.selection.is_none() {
                                self.renderer.selection = Some((last_loc, last_loc));
                            }

                            mouse_down = true;
                        }
                        ElementState::Released => {
                            scrollbar_held = false;
                            mouse_down = false;
                        }
                    },
                    WindowEvent::ModifiersChanged(new_state) => modifiers = new_state,
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(keycode),
                                ..
                            },
                        ..
                    } => match keycode {
                        VirtualKeyCode::C => {
                            let copy = (cfg!(target_os = "macos") && modifiers.logo())
                                || (!cfg!(target_os = "macos") && modifiers.ctrl());
                            if copy {
                                self.clipboard
                                    .set_contents(self.renderer.selection_text.trim().to_owned())
                                    .unwrap()
                            }
                        }
                        VirtualKeyCode::Equals => {
                            let zoom = ((cfg!(target_os = "macos") && modifiers.logo())
                                || (!cfg!(target_os = "macos") && modifiers.ctrl()))
                                && modifiers.shift();
                            if zoom {
                                self.renderer.zoom *= 1.1;
                                self.renderer.reposition(&mut self.elements);
                                self.renderer.set_scroll_y(self.renderer.scroll_y);
                                self.window.request_redraw();
                            }
                        }
                        VirtualKeyCode::Minus => {
                            let zoom = ((cfg!(target_os = "macos") && modifiers.logo())
                                || (!cfg!(target_os = "macos") && modifiers.ctrl()))
                                && modifiers.shift();
                            if zoom {
                                self.renderer.zoom *= 0.9;
                                self.renderer.reposition(&mut self.elements);
                                self.renderer.set_scroll_y(self.renderer.scroll_y);
                                self.window.request_redraw();
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                },
                Event::MainEventsCleared => {
                    if let Some(size) = pending_resize.take() {
                        self.renderer.config.width = size.width;
                        self.renderer.config.height = size.height;
                        self.renderer.positioner.screen_size = size.into();
                        self.renderer
                            .surface
                            .configure(&self.renderer.device, &self.renderer.config);
                        self.renderer.reposition(&mut self.elements);
                        self.renderer.set_scroll_y(self.renderer.scroll_y);
                        self.window.request_redraw();
                    }
                }
                _ => {}
            }
        });
    }

    fn find_hoverable<'a, T: wgpu_glyph::GlyphCruncher>(
        elements: &'a [Positioned<Element>],
        glyph_brush: &'a mut T,
        loc: (f32, f32),
        screen_size: (f32, f32),
        zoom: f32,
    ) -> Option<Hoverable<'a>> {
        let screen_pos = |screen_size: (f32, f32), bounds_offset: f32| {
            (
                screen_size.0 - bounds_offset - DEFAULT_MARGIN,
                screen_size.1,
            )
        };

        elements
            .iter()
            .find(|&e| e.contains(loc) && !matches!(e.inner, Element::Spacer(_)))
            .and_then(|element| match &element.inner {
                Element::TextBox(text_box) => {
                    let bounds = element.bounds.as_ref().unwrap();
                    text_box
                        .find_hoverable(
                            glyph_brush,
                            loc,
                            bounds.pos,
                            screen_pos(screen_size, bounds.pos.0),
                            zoom,
                        )
                        .map(Hoverable::Text)
                }
                Element::Table(table) => {
                    let bounds = element.bounds.as_ref().unwrap();
                    table
                        .find_hoverable(
                            glyph_brush,
                            loc,
                            bounds.pos,
                            screen_pos(screen_size, bounds.pos.0),
                            zoom,
                        )
                        .map(Hoverable::Text)
                }
                Element::Image(image) => Some(Hoverable::Image(image)),
                Element::Spacer(_) => unreachable!("Spacers are filtered"),
                Element::Row(row) => {
                    Self::find_hoverable(&row.elements, glyph_brush, loc, screen_size, zoom)
                }
            })
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Error)
        .filter_module("inlyne", log::LevelFilter::Info)
        .parse_env("INLYNE_LOG")
        .init();

    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            log::warn!(
                "Failed reading config file. Falling back to defaults. Error: {}",
                err
            );
            Config::default()
        }
    };
    let args = Args::new(&config);
    let opts = Opts::parse_and_load_from(&args, config);

    let md_string = std::fs::read_to_string(&opts.file_path)
        .with_context(|| format!("Could not read file at {:?}", opts.file_path))?;
    let inlyne = pollster::block_on(Inlyne::new(opts, args))?;

    let interpreter = HtmlInterpreter::new(
        inlyne.window.clone(),
        inlyne.element_queue.clone(),
        inlyne.renderer.theme.clone(),
        inlyne.renderer.hidpi_scale,
    );

    std::thread::spawn(move || interpreter.intepret_md(md_string.as_str()));
    inlyne.run();

    Ok(())
}
