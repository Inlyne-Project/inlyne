#![allow(
    // I don't really care enough about the names here to fix things atm
    clippy::enum_variant_names,
)]
#![warn(
    // Generally we don't want this sneaking into `main`
    clippy::todo,
    // This should be used very sparingly compared between logging and clap
    clippy::print_stdout, clippy::print_stderr,
)]

mod clipboard;
pub mod color;
mod debug_impls;
mod file_watcher;
pub mod fonts;
pub mod history;
pub mod image;
pub mod interpreter;
mod keybindings;
mod metrics;
pub mod opts;
mod panic_hook;
pub mod positioner;
pub mod renderer;
pub mod selection;
pub mod table;
#[cfg(test)]
pub mod test_utils;
pub mod text;
pub mod utils;

use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use file_watcher::Watcher;
use image::{Image, ImageData};
use interpreter::HtmlInterpreter;
use keybindings::action::{Action, HistDirection, VertDirection, Zoom};
use keybindings::{Key, KeyCombos, ModifiedKey};
use metrics::{histogram, HistTag};
use opts::{Cli, Config, Opts};
use positioner::{Positioned, Row, Section, Spacer, DEFAULT_MARGIN, DEFAULT_PADDING};
use raw_window_handle::HasRawDisplayHandle;
use renderer::Renderer;
use table::Table;
use text::{Text, TextBox, TextSystem};
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::SubscriberInitExt;
use utils::{ImageCache, Point, Rect, Size};

use crate::opts::{Commands, ConfigCmd, MetricsExporter};
use crate::selection::Selection;
use anyhow::Context;
use clap::Parser;
use taffy::Taffy;
use winit::event::{
    ElementState, Event, KeyboardInput, ModifiersState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use winit::window::{CursorIcon, Window, WindowBuilder};

pub enum InlyneEvent {
    LoadedImage(String, Arc<Mutex<Option<ImageData>>>),
    FileReload,
    FileChange { contents: String },
    Reposition,
    PositionQueue,
}

impl Debug for InlyneEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Inlyne Event")
    }
}

pub enum Hoverable<'a> {
    Image(&'a Image),
    Text(&'a Text),
    Summary(&'a Section),
}

#[derive(Debug)]
pub enum Element {
    TextBox(TextBox),
    Spacer(Spacer),
    Image(Image),
    Table(Table),
    Row(Row),
    Section(Section),
}

impl From<Section> for Element {
    fn from(section: Section) -> Self {
        Element::Section(section)
    }
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
    opts: Opts,
    window: Arc<Window>,
    // HACK: `Option<_>` is used here to keep `Inlyne` valid while running the event loop. Consider
    // splitting this out from the rest of the state
    event_loop: Option<EventLoop<InlyneEvent>>,
    renderer: Renderer,
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    elements: Vec<Positioned<Element>>,
    lines_to_scroll: f32,
    image_cache: ImageCache,
    interpreter_sender: mpsc::Sender<String>,
    interpreter_should_queue: Arc<AtomicBool>,
    keycombos: KeyCombos,
    need_repositioning: bool,
    watcher: Watcher,
    selection: Selection,
}

impl Inlyne {
    pub fn new(opts: Opts) -> anyhow::Result<Self> {
        let keycombos = KeyCombos::new(opts.keybindings.clone())?;

        let file_path = opts.history.get_path().to_owned();

        let event_loop = EventLoopBuilder::<InlyneEvent>::with_user_event().build();

        let window = {
            let mut wb = WindowBuilder::new().with_title(utils::format_title(&file_path));

            if let Some(ref pos) = opts.position {
                wb = wb.with_position(winit::dpi::PhysicalPosition::new(pos.x, pos.y))
            }
            if let Some(ref size) = opts.size {
                wb = wb.with_inner_size(winit::dpi::PhysicalSize::new(size.width, size.height))
            }

            Arc::new(wb.build(&event_loop).unwrap())
        };

        let renderer = pollster::block_on(Renderer::new(
            &window,
            opts.theme.clone(),
            opts.scale.unwrap_or(window.scale_factor() as f32),
            opts.page_width.unwrap_or(f32::MAX),
            opts.font_opts.clone(),
        ))?;

        let element_queue = Arc::new(Mutex::new(VecDeque::new()));
        let image_cache = Arc::new(Mutex::new(HashMap::new()));
        let md_string = read_to_string(&file_path)
            .with_context(|| format!("Could not read file at '{}'", file_path.display()))?;

        let interpreter = HtmlInterpreter::new(
            window.clone(),
            element_queue.clone(),
            renderer.theme.clone(),
            renderer.surface_format,
            renderer.hidpi_scale,
            file_path.clone(),
            image_cache.clone(),
            event_loop.create_proxy(),
            opts.color_scheme,
        );

        let (interpreter_sender, interpreter_receiver) = channel();
        let interpreter_should_queue = interpreter.should_queue.clone();
        std::thread::spawn(move || interpreter.interpret_md(interpreter_receiver));

        interpreter_sender.send(md_string)?;

        let lines_to_scroll = opts.lines_to_scroll;

        let watcher = Watcher::spawn(event_loop.create_proxy(), file_path.clone());

        let _ = file_path.parent().map(std::env::set_current_dir);

        Ok(Self {
            opts,
            window,
            event_loop: Some(event_loop),
            renderer,
            element_queue,
            elements: Vec::new(),
            lines_to_scroll,
            interpreter_sender,
            interpreter_should_queue,
            image_cache,
            keycombos,
            need_repositioning: false,
            watcher,
            selection: Selection::new(),
        })
    }

    pub fn position_queued_elements(
        element_queue: &Arc<Mutex<VecDeque<Element>>>,
        renderer: &mut Renderer,
        elements: &mut Vec<Positioned<Element>>,
    ) {
        let queue = {
            element_queue
                .try_lock()
                .map(|mut queue| queue.drain(..).collect::<Vec<Element>>())
        };
        if let Ok(queue) = queue {
            let positioning_start = Instant::now();

            for element in queue {
                // Position element and add it to elements
                let mut positioned_element = Positioned::new(element);
                renderer
                    .positioner
                    .position(
                        &mut renderer.text_system,
                        &mut positioned_element,
                        renderer.zoom,
                    )
                    .unwrap();
                renderer.positioner.reserved_height +=
                    DEFAULT_PADDING * renderer.hidpi_scale * renderer.zoom
                        + positioned_element.bounds.as_ref().unwrap().size.1;
                elements.push(positioned_element);
            }

            histogram!(HistTag::Positioner).record(positioning_start.elapsed());
        }
    }

    fn load_file(&mut self, contents: String) {
        self.interpreter_should_queue
            .store(false, Ordering::Relaxed);
        self.element_queue.lock().unwrap().clear();
        self.elements.clear();
        self.renderer.positioner.reserved_height = DEFAULT_PADDING * self.renderer.hidpi_scale;
        self.renderer.positioner.anchors.clear();
        self.interpreter_should_queue.store(true, Ordering::Relaxed);
        self.interpreter_sender.send(contents).unwrap();
    }

    fn update_file(&mut self, path: &Path, contents: String) {
        self.window.set_title(&utils::format_title(path));
        self.watcher.update_file(path, contents);
        self.renderer.set_scroll_y(0.0);
    }

    pub fn run(mut self) {
        let mut pending_resize = None;
        let mut scrollbar_held = None;
        let mut mouse_down = false;
        let mut modifiers = ModifiersState::empty();
        let mut mouse_position: Point = Point::default();

        let event_loop = self.event_loop.take().unwrap();
        let event_loop_proxy = event_loop.create_proxy();
        // SAFETY: Since this takes a pointer to the winit event loop, it MUST be dropped first,
        // which is done by `move` into event loop.
        let mut clipboard = unsafe { clipboard::Clipboard::new(event_loop.raw_display_handle()) };
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::LoadedImage(src, image_data) => {
                        self.image_cache.lock().unwrap().insert(src, image_data);
                        self.need_repositioning = true;
                    }
                    InlyneEvent::FileReload => match read_to_string(self.opts.history.get_path()) {
                        Ok(contents) => self.load_file(contents),
                        Err(err) => {
                            tracing::warn!(
                                "Failed reloading file at {}\nError: {}",
                                self.opts.history.get_path().display(),
                                err
                            );
                        }
                    },
                    InlyneEvent::FileChange { contents } => self.load_file(contents),
                    InlyneEvent::Reposition => {
                        self.need_repositioning = true;
                    }
                    InlyneEvent::PositionQueue => {
                        Self::position_queued_elements(
                            &self.element_queue,
                            &mut self.renderer,
                            &mut self.elements,
                        );
                        self.window.request_redraw()
                    }
                },
                Event::RedrawRequested(_) => {
                    let redraw_start = Instant::now();
                    Self::position_queued_elements(
                        &self.element_queue,
                        &mut self.renderer,
                        &mut self.elements,
                    );
                    self.renderer.set_scroll_y(self.renderer.scroll_y);
                    self.renderer
                        .redraw(&mut self.elements, &mut self.selection)
                        .context("Renderer failed to redraw the screen")
                        .unwrap();

                    histogram!(HistTag::Redraw).record(redraw_start.elapsed());
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => pending_resize = Some(size),
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::MouseWheel { delta, .. } => match delta {
                        MouseScrollDelta::PixelDelta(pos) => {
                            Self::scroll_pixels(&mut self.renderer, &self.window, pos.y as f32)
                        }
                        MouseScrollDelta::LineDelta(_, y_delta) => Self::scroll_lines(
                            &mut self.renderer,
                            &self.window,
                            self.lines_to_scroll,
                            y_delta,
                        ),
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        let screen_size = self.renderer.screen_size();
                        let loc = (
                            position.x as f32,
                            position.y as f32 + self.renderer.scroll_y,
                        );

                        let cursor_icon = if let Some(hoverable) = Self::find_hoverable(
                            &mut self.renderer.text_system,
                            &mut self.renderer.positioner.taffy,
                            &self.elements,
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

                        if scrollbar_held.is_some()
                            || (Rect::new(
                                (screen_size.0 - DEFAULT_MARGIN / 4., 0.),
                                (DEFAULT_MARGIN / 4., screen_size.1),
                            )
                            .contains(position.into())
                                && mouse_down)
                        {
                            let scrollbar_height = self.renderer.scrollbar_height();
                            if scrollbar_held.is_none() {
                                if Rect::new(
                                    (
                                        screen_size.0 - DEFAULT_MARGIN / 4.,
                                        ((self.renderer.scroll_y
                                            / self.renderer.positioner.reserved_height)
                                            * screen_size.1),
                                    ),
                                    (DEFAULT_MARGIN / 4., scrollbar_height),
                                )
                                .contains(position.into())
                                {
                                    // If we click in the bounds of the scrollbar, maintain the difference between the
                                    // center of the scrollbar and the mouse
                                    scrollbar_held = Some(
                                        position.y as f32
                                            - (((self.renderer.scroll_y
                                                / self.renderer.positioner.reserved_height)
                                                * screen_size.1)
                                                + scrollbar_height / 2.),
                                    );
                                } else {
                                    scrollbar_held = Some(0.);
                                }
                            }

                            let pos_y = if let Some(diff) = scrollbar_held {
                                position.y as f32 - diff
                            } else {
                                position.y as f32
                            };
                            let target_scroll = ((pos_y - scrollbar_height / 2.) / screen_size.1)
                                * self.renderer.positioner.reserved_height;
                            self.renderer.set_scroll_y(target_scroll);
                            self.window.request_redraw();
                        } else if mouse_down && self.selection.handle_drag(loc) {
                            self.window.request_redraw();
                        }
                        mouse_position = loc;
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => match state {
                        ElementState::Pressed => {
                            // Try to click a link
                            let screen_size = self.renderer.screen_size();

                            let y = mouse_position.1 - self.renderer.scroll_y;
                            if Rect::new(
                                (screen_size.0 - DEFAULT_MARGIN / 4., 0.),
                                (DEFAULT_MARGIN / 4., screen_size.1),
                            ).contains((mouse_position.0, y)) {
                                let scrollbar_height = self.renderer.scrollbar_height();

                                let target_scroll = ((y - scrollbar_height / 2.) / screen_size.1)
                                    * self.renderer.positioner.reserved_height;

                                self.renderer.set_scroll_y(target_scroll);
                                self.window.request_redraw();
                            }

                            if let Some(hoverable) = Self::find_hoverable(
                                &mut self.renderer.text_system,
                                &mut self.renderer.positioner.taffy,
                                &self.elements,
                                mouse_position,
                                screen_size,
                                self.renderer.zoom,
                            ) {
                                match hoverable {
                                    Hoverable::Image(Image { is_link: Some(link), .. }) |
                                    Hoverable::Text(Text { link: Some(link), .. }) => {
                                        let path = PathBuf::from_str(link).unwrap(); // Can't fail

                                        if  path.extension().map_or(false, |ext| ext == "md")
                                            && !path.to_str().map_or(false, |s| s.starts_with("http")) {
                                            // Open them in a new window, akin to what a browser does
                                            if modifiers.shift() {
                                                Command::new(
                                                    std::env::current_exe()
                                                        .unwrap_or_else(|_| "inlyne".into()),
                                                )
                                                    .args(Opts::program_args(&path))
                                                    .spawn()
                                                    .expect("Could not spawn new inlyne instance");
                                            } else {
                                                match read_to_string(&path) {
                                                    Ok(contents) => {
                                                        self.update_file(&path, contents);
                                                        self.opts.history.make_next(path);
                                                    }
                                                    Err(err) => {
                                                        tracing::warn!(
                                                        "Failed loading markdown file at {}\nError: {}",
                                                        path.display(),
                                                        err,
                                                    );
                                                    }
                                                }
                                            }
                                        } else if let Some(anchor_pos) =
                                            self.renderer.positioner.anchors.get(&link.to_lowercase())
                                        {
                                            self.renderer.set_scroll_y(*anchor_pos);
                                            self.window.request_redraw();
                                            self.window.set_cursor_icon(CursorIcon::Default);
                                        } else if let Err(e) = open::that(link) {
                                            tracing::error!("Could not open link: {e} from {:?}", std::env::current_dir())
                                        }
                                    },
                                    Hoverable::Summary(summary) => {
                                        let mut hidden = summary.hidden.borrow_mut();
                                        *hidden = !*hidden;
                                        event_loop_proxy
                                            .send_event(InlyneEvent::Reposition)
                                            .unwrap();
                                        self.selection.add_position(mouse_position);
                                    },
                                    _ => {
                                        self.selection.add_position(mouse_position);
                                        self.window.request_redraw();
                                    }
                                };
                            } else {
                                self.selection.add_position(mouse_position);
                            }
                            mouse_down = true;
                        }
                        ElementState::Released => {
                            scrollbar_held = None;
                            mouse_down = false;
                        }
                    },
                    WindowEvent::ModifiersChanged(new_state) => modifiers = new_state,
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode,
                                scancode,
                                ..
                            },
                        ..
                    } => {
                        let key = Key::new(virtual_keycode, scancode);
                        let modified_key = ModifiedKey(key, modifiers);
                        if let Some(action) = self.keycombos.munch(modified_key) {
                            match action {
                                Action::ToEdge(direction) => {
                                    let scroll = match direction {
                                        VertDirection::Up => 0.0,
                                        VertDirection::Down => f32::INFINITY,
                                    };
                                    self.renderer.set_scroll_y(scroll);
                                    self.window.request_redraw();
                                }
                                Action::Scroll(direction) => {
                                    let lines = match direction {
                                        VertDirection::Up => 1.0,
                                        VertDirection::Down => -1.0,
                                    };

                                    Self::scroll_lines(
                                        &mut self.renderer,
                                        &self.window,
                                        self.lines_to_scroll,
                                        lines,
                                    )
                                }
                                Action::Page(direction) => {
                                    // Move 90% of current page height
                                    let scroll_amount = self.renderer.config.height as f32 * 0.9;
                                    let scroll_with_direction = match direction {
                                        VertDirection::Up => scroll_amount,
                                        VertDirection::Down => -scroll_amount,
                                    };

                                    Self::scroll_pixels(
                                        &mut self.renderer,
                                        &self.window,
                                        scroll_with_direction,
                                    );
                                }
                                Action::Zoom(zoom_action) => {
                                    let zoom = match zoom_action {
                                        Zoom::In => self.renderer.zoom * 1.1,
                                        Zoom::Out => self.renderer.zoom * 0.9,
                                        Zoom::Reset => 1.0,
                                    };

                                    self.renderer.zoom = zoom;
                                    let old_reserved = self.renderer.positioner.reserved_height;
                                    self.renderer.reposition(&mut self.elements).unwrap();
                                    let new_reserved = self.renderer.positioner.reserved_height;
                                    self.renderer.set_scroll_y(
                                        self.renderer.scroll_y * (new_reserved / old_reserved),
                                    );
                                    self.window.request_redraw();
                                }
                                Action::Copy => clipboard
                                    .set_contents(self.selection.text.trim().to_owned()),
                                Action::Quit => *control_flow = ControlFlow::Exit,
                                Action::History(hist_dir) => {
                                    let changed_path = match hist_dir {
                                        HistDirection::Next => self.opts.history.next(),
                                        HistDirection::Prev => self.opts.history.previous(),
                                    }.map(ToOwned::to_owned);
                                    let Some(file_path) = changed_path else {
                                        return;
                                    };
                                    match read_to_string(&file_path) {
                                        Ok(contents) => {
                                            self.update_file(&file_path, contents);
                                            let parent = file_path.parent().expect("File should have parent directory");
                                            std::env::set_current_dir(parent).expect("Could not set current directory.");
                                        }
                                        Err(err) => {
                                            tracing::warn!(
                                                "Failed loading markdown file at {}\nError: {}",
                                                file_path.display(),
                                                err,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Event::MainEventsCleared => {
                    // We lazily store the size and only reposition elements and request a redraw when
                    // we receive a `MainEventsCleared`.  This prevents us from clogging up the queue
                    // with a bunch of costly resizes. (https://github.com/trimental/inlyne/issues/25)
                    if let Some(size) = pending_resize.take() {
                        if size.width > 0 && size.height > 0 {
                            self.renderer.config.width = size.width;
                            self.renderer.config.height = size.height;
                            self.renderer.positioner.screen_size = size.into();
                            self.renderer
                                .surface
                                .configure(&self.renderer.device, &self.renderer.config);
                            let old_reserved = self.renderer.positioner.reserved_height;
                            self.renderer.reposition(&mut self.elements).unwrap();
                            let new_reserved = self.renderer.positioner.reserved_height;
                            self.renderer.set_scroll_y(
                                self.renderer.scroll_y * (new_reserved / old_reserved),
                            );
                            self.window.request_redraw();
                        }
                    }

                    if self.need_repositioning {
                        self.renderer.reposition(&mut self.elements).unwrap();
                        self.window.request_redraw();
                        self.need_repositioning = false;
                    }
                }
                _ => {}
            }
        });
    }

    fn scroll_lines(
        renderer: &mut Renderer,
        window: &Window,
        lines_to_scroll: f32,
        num_lines: f32,
    ) {
        let num_pixels = num_lines * 16.0 * lines_to_scroll * renderer.hidpi_scale * renderer.zoom;
        Self::scroll_pixels(renderer, window, num_pixels);
    }

    fn scroll_pixels(renderer: &mut Renderer, window: &Window, num_pixels: f32) {
        renderer.set_scroll_y(renderer.scroll_y - num_pixels);
        window.request_redraw();
    }

    fn find_hoverable<'a>(
        text_system: &mut TextSystem,
        taffy: &mut Taffy,
        elements: &'a [Positioned<Element>],
        loc: Point,
        screen_size: Size,
        zoom: f32,
    ) -> Option<Hoverable<'a>> {
        let screen_pos = |screen_size: Size, bounds_offset: f32| {
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
                            text_system,
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
                            text_system,
                            taffy,
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
                    Self::find_hoverable(text_system, taffy, &row.elements, loc, screen_size, zoom)
                }
                Element::Section(section) => {
                    if let Some(ref summary) = *section.summary {
                        if let Some(ref bounds) = summary.bounds {
                            if bounds.contains(loc) {
                                return Some(Hoverable::Summary(section));
                            }
                        }
                    }
                    if !*section.hidden.borrow() {
                        Self::find_hoverable(
                            text_system,
                            taffy,
                            &section.elements,
                            loc,
                            screen_size,
                            zoom,
                        )
                    } else {
                        None
                    }
                }
            })
    }
}

fn main() -> anyhow::Result<()> {
    setup_panic!();

    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive("inlyne=info".parse()?)
        .with_env_var("INLYNE_LOG")
        .from_env()?;
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().compact())
        .init();

    let command = Cli::parse().into_commands();

    match command {
        Commands::View(view) => {
            let config = match &view.config {
                Some(config_path) => Config::load_from_file(config_path)?,
                None => Config::load_from_system().unwrap_or_else(|err| {
                    tracing::warn!(
                        "Failed reading config file. Falling back to defaults. Error: {}",
                        err
                    );
                    Config::default()
                }),
            };
            let opts = Opts::parse_and_load_from(view, config)?;

            if let Some(exporter) = &opts.metrics {
                match exporter {
                    MetricsExporter::Log => {
                        let recorder = metrics::LogRecorder::default();
                        metrics::set_global_recorder(recorder)
                            .expect("Failed setting metrics recorder");
                    }
                    #[cfg(inlyne_tcp_metrics)]
                    MetricsExporter::Tcp => metrics_exporter_tcp::TcpBuilder::new()
                        .install()
                        .expect("Failed to install TCP metrics server"),
                };
            }

            for tag in HistTag::iter() {
                tag.set_global_description();
            }

            let inlyne = Inlyne::new(opts)?;
            inlyne.run();
        }
        Commands::Config(ConfigCmd::Open) => {
            let config_path = dirs::config_dir()
                .context("Failed to find the configuration directory")?
                .join("inlyne")
                .join("inlyne.toml");

            if !config_path.is_file() {
                tracing::warn!(
                    "No config found. Creating a new config at: {}",
                    config_path.display()
                );
                Config::create_default_config(&config_path)?;
            }

            edit::edit_file(config_path)?;
        }
    }

    Ok(())
}
