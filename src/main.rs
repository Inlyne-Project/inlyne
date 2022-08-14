pub mod cli;
pub mod color;
pub mod image;
pub mod interpreter;
pub mod renderer;
pub mod table;
pub mod text;
pub mod utils;

use crate::cli::Args;
use crate::image::Image;
use crate::interpreter::HtmlInterpreter;
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
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                            // Arbitrarily pick x30 as the number of pixels to shift per line
                            MouseScrollDelta::LineDelta(_, y_delta) => {
                                y_delta as f32 * 32.0 * self.renderer.hidpi_scale
                            }
                        };

                        let screen_height = self.renderer.screen_height();
                        if self.renderer.reserved_height > screen_height {
                            self.renderer.scroll_y -= y_pixel_shift;

                            if self.renderer.scroll_y.is_sign_negative() {
                                self.renderer.scroll_y = 0.;
                            } else if self.renderer.scroll_y
                                >= (self.renderer.reserved_height - screen_height)
                            {
                                self.renderer.scroll_y =
                                    self.renderer.reserved_height - screen_height;
                            }
                        }
                        self.window.request_redraw();
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let mut over_link = false;
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
                                        let cursor = text_box.hovering_over(
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(cursor);
                                        over_link = true;
                                        break;
                                    }
                                    Element::Table(ref table) => {
                                        let bounds = element.bounds.as_ref().unwrap();
                                        let cursor = table.hovering_over(
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(cursor);
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
                            || (Rect::new(
                                (screen_size.0 * (49. / 50.), 0.),
                                (screen_size.0 * (1. / 50.), screen_size.1),
                            )
                            .contains(position.into())
                                && click_scheduled)
                        {
                            let target_scroll = ((position.y as f32 / screen_size.1)
                                * self.renderer.reserved_height)
                                - (screen_size.1 / self.renderer.reserved_height * screen_size.1);
                            self.renderer.scroll_y = if target_scroll <= 0. {
                                0.
                            } else if target_scroll >= self.renderer.reserved_height - screen_size.1
                            {
                                self.renderer.reserved_height - screen_size.1
                            } else {
                                target_scroll
                            };
                            self.window.request_redraw();
                            if !scrollbar_held {
                                scrollbar_held = true;
                            }
                        } else {
                            if click_scheduled {
                                self.renderer.selection = Some((loc, loc));
                            }
                            if let Some(ref mut selection) = self.renderer.selection {
                                if mouse_down {
                                    selection.1 = loc;
                                    self.window.request_redraw();
                                }
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
                        if input.virtual_keycode == Some(VirtualKeyCode::C) {
                            let copy = (cfg!(target_os = "macos") && modifiers.logo())
                                || (!cfg!(target_os = "macos") && modifiers.ctrl());
                            if copy {
                                self.clipboard
                                    .set_contents(self.renderer.selection_text.trim().to_owned())
                                    .unwrap()
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
    let args = Args::parse();
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
