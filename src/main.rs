pub mod color;
pub mod image;
pub mod interpreter;
pub mod opts;
pub mod renderer;
pub mod table;
pub mod text;
pub mod utils;

use crate::image::Image;
use crate::interpreter::HtmlInterpreter;
use crate::opts::Opts;
use crate::table::Table;

use color::Theme;
use renderer::{Renderer, Spacer};
use utils::Rect;

use anyhow::Context;
use copypasta::{ClipboardContext, ClipboardProvider};
use text::TextBox;
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;
use winit::event::{ElementState, MouseButton};
use winit::{
    event::{Event, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};

use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug)]
pub enum InlyneEvent {
    Reposition,
}

pub enum Element {
    TextBox(TextBox),
    Spacer(Spacer),
    Image(Image),
    Table(Table),
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
}

impl Inlyne {
    pub async fn new(theme: Theme, scale: Option<f32>) -> Self {
        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Arc::new(Window::new(&event_loop).unwrap());
        window.set_title("Inlyne");
        let renderer = Renderer::new(
            &window,
            event_loop.create_proxy(),
            theme,
            scale.unwrap_or(window.scale_factor() as f32),
        )
        .await;
        let clipboard = ClipboardContext::new().unwrap();

        Self {
            window,
            event_loop,
            renderer,
            element_queue: Arc::new(Mutex::new(VecDeque::new())),
            clipboard,
        }
    }

    pub fn push<T: Into<Element>>(&mut self, element: T) {
        let element = element.into();
        self.renderer.push(element);
    }

    pub fn run(mut self) {
        let mut click_scheduled = false;
        let mut scrollbar_held = false;
        let mut mouse_down = false;
        let mut modifiers = ModifiersState::empty();
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::Reposition => {
                        self.renderer.reposition();
                        self.window.request_redraw()
                    }
                },
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    self.renderer.config.width = size.width;
                    self.renderer.config.height = size.height;
                    self.renderer
                        .surface
                        .configure(&self.renderer.device, &self.renderer.config);
                    self.renderer.reposition();
                    self.window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    let queued_elements =
                        if let Ok(mut element_queue) = self.element_queue.try_lock() {
                            Some(element_queue.drain(0..).collect::<Vec<Element>>())
                        } else {
                            None
                        };
                    if let Some(queue) = queued_elements {
                        for element in queue {
                            self.renderer.push(element);
                        }
                    }
                    self.renderer.redraw();
                }
                Event::WindowEvent { event, .. } => match event {
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
                        let mut over_link = false;
                        let mut jumped = false;
                        let screen_size = self.renderer.screen_size();
                        let loc = (
                            position.x as f32,
                            position.y as f32 + self.renderer.scroll_y,
                        );
                        for element in self.renderer.elements.iter() {
                            if element.contains(loc) {
                                match element.deref() {
                                    Element::TextBox(ref text_box) => {
                                        let bounds = element.bounds.as_ref().unwrap();
                                        let hover_info = text_box.hovering_over(
                                            &self.renderer.anchors,
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            self.renderer.zoom,
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(hover_info.cursor_icon);
                                        if let Some(jump_pos) = hover_info.jump {
                                            jumped = true;
                                            self.renderer.set_scroll_y(jump_pos);
                                            self.window.request_redraw();
                                        }
                                        over_link = true;
                                        break;
                                    }
                                    Element::Table(ref table) => {
                                        let bounds = element.bounds.as_ref().unwrap();
                                        let hover_info = table.hovering_over(
                                            &self.renderer.anchors,
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            self.renderer.zoom,
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(hover_info.cursor_icon);
                                        if let Some(jump_pos) = hover_info.jump {
                                            jumped = true;
                                            self.renderer.set_scroll_y(jump_pos);
                                            self.window.request_redraw();
                                        }
                                        over_link = true;
                                        break;
                                    }
                                    Element::Image(image) => {
                                        if let Some(ref link) = image.is_link {
                                            if click_scheduled && open::that(link).is_err() {
                                                eprintln!("Could not open link");
                                            }
                                            self.window.set_cursor_icon(CursorIcon::Hand);
                                            over_link = true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if scrollbar_held
                            || (Rect::new((screen_size.0 - 25., 0.), (25., screen_size.1))
                                .contains(position.into())
                                && click_scheduled)
                        {
                            let target_scroll = ((position.y as f32 / screen_size.1)
                                * self.renderer.reserved_height)
                                - (screen_size.1 / self.renderer.reserved_height * screen_size.1);
                            self.renderer.set_scroll_y(target_scroll);
                            self.window.request_redraw();
                            if !scrollbar_held {
                                scrollbar_held = true;
                            }
                        } else if click_scheduled && !jumped {
                            self.renderer.selection = Some((loc, loc));
                        } else if let Some(ref mut selection) = self.renderer.selection {
                            if mouse_down {
                                selection.1 = loc;
                                self.window.request_redraw();
                            }
                        }

                        if !over_link {
                            self.window.set_cursor_icon(CursorIcon::Default);
                        }
                        click_scheduled = false;
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => match state {
                        ElementState::Pressed => {
                            self.renderer.selection = None;
                            mouse_down = true;
                            click_scheduled = true;
                            self.window.request_redraw();
                        }
                        ElementState::Released => {
                            click_scheduled = false;
                            scrollbar_held = false;
                            mouse_down = false;
                        }
                    },
                    WindowEvent::ModifiersChanged(modifier_state) => modifiers = modifier_state,
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let ElementState::Pressed = input.state {
                            match input.virtual_keycode {
                                Some(VirtualKeyCode::C) => {
                                    let copy = (cfg!(target_os = "macos") && modifiers.logo())
                                        || (!cfg!(target_os = "macos") && modifiers.ctrl());
                                    if copy {
                                        self.clipboard
                                            .set_contents(
                                                self.renderer.selection_text.trim().to_owned(),
                                            )
                                            .unwrap()
                                    }
                                }
                                Some(VirtualKeyCode::Equals) => {
                                    let zoom = ((cfg!(target_os = "macos") && modifiers.logo())
                                        || (!cfg!(target_os = "macos") && modifiers.ctrl()))
                                        && modifiers.shift();
                                    if zoom {
                                        self.renderer.zoom *= 1.1;
                                        self.renderer.reposition();
                                        self.renderer.set_scroll_y(self.renderer.scroll_y);
                                        self.window.request_redraw();
                                    }
                                }
                                Some(VirtualKeyCode::Minus) => {
                                    let zoom = ((cfg!(target_os = "macos") && modifiers.logo())
                                        || (!cfg!(target_os = "macos") && modifiers.ctrl()))
                                        && modifiers.shift();
                                    if zoom {
                                        self.renderer.zoom *= 0.9;
                                        self.renderer.reposition();
                                        self.renderer.set_scroll_y(self.renderer.scroll_y);
                                        self.window.request_redraw();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        });
    }
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse_and_load();
    let theme = args.theme;
    let md_string = std::fs::read_to_string(&args.file_path)
        .with_context(|| format!("Could not read file at {:?}", args.file_path))?;
    let inlyne = pollster::block_on(Inlyne::new(theme, args.scale));

    let hidpi_scale = args.scale.unwrap_or(inlyne.window.scale_factor() as f32);
    let interpreter = HtmlInterpreter::new(
        inlyne.window.clone(),
        inlyne.element_queue.clone(),
        inlyne.renderer.theme.clone(),
        hidpi_scale,
    );

    std::thread::spawn(move || interpreter.intepret_md(md_string.as_str()));
    inlyne.run();

    Ok(())
}
