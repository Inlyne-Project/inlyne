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

use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, channel};
use std::sync::Arc;
use std::time::Instant;

use file_watcher::Watcher;
use image::{Image, ImageData};
use interpreter::HtmlInterpreter;
use keybindings::action::{Action, HistDirection, VertDirection, Zoom};
use keybindings::{Key, KeyCombos, ModifiedKey};
use metrics::{histogram, HistTag};
use opts::{Cli, Config, Opts};
use parking_lot::Mutex;
use positioner::{Positioned, Row, Section, Spacer, DEFAULT_MARGIN, DEFAULT_PADDING};
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
use taffy::TaffyTree;
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window};

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

#[derive(Debug, PartialEq)]
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
    renderer: Renderer,
    element_queue: Arc<Mutex<Vec<Element>>>,
    elements: Vec<Positioned<Element>>,
    lines_to_scroll: f32,
    image_cache: ImageCache,
    interpreter_sender: mpsc::Sender<String>,
    keycombos: KeyCombos,
    need_repositioning: bool,
    watcher: Watcher,
    selection: Selection,
    // Run-time state (moved from closures)
    pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
    scrollbar_held: Option<f32>,
    mouse_down: bool,
    modifiers: ModifiersState,
    mouse_position: Point,
    event_loop_proxy: EventLoopProxy<InlyneEvent>,
    clipboard: clipboard::Clipboard,
}

impl Inlyne {
    pub fn new(opts: Opts, event_loop: &ActiveEventLoop, event_loop_proxy: EventLoopProxy<InlyneEvent>) -> anyhow::Result<Self> {
        let keycombos = KeyCombos::new(opts.keybindings.clone())?;

        let file_path = opts.history.get_path().to_owned();

        let window = {
            let mut wa = Window::default_attributes().with_title(utils::format_title(&file_path));

            if let Some(decorations) = opts.decorations {
                wa = wa.with_decorations(decorations);
            }
            if let Some(ref pos) = opts.position {
                wa = wa.with_position(winit::dpi::PhysicalPosition::new(pos.x, pos.y));
            }
            if let Some(ref size) = opts.size {
                wa = wa.with_inner_size(winit::dpi::PhysicalSize::new(size.width, size.height));
            }
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            {
                use winit::platform::wayland::WindowAttributesExtWayland;
                wa = wa.with_name("inlyne", "");
            }

            Arc::new(event_loop.create_window(wa).unwrap())
        };

        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            opts.theme.clone(),
            opts.scale.unwrap_or(window.scale_factor() as f32),
            opts.page_width.unwrap_or(f32::MAX),
            opts.font_opts.clone(),
        ))?;

        let element_queue = Arc::new(Mutex::new(Vec::new()));
        let image_cache = Arc::new(Mutex::new(HashMap::new()));
        let md_string = read_to_string(&file_path)
            .with_context(|| format!("Could not read file at '{}'", file_path.display()))?;

        let interpreter = HtmlInterpreter::new(
            window.clone(),
            element_queue.clone(),
            renderer.theme.clone(),
            renderer.surface_format,
            renderer.hidpi_scale,
            image_cache.clone(),
            event_loop_proxy.clone(),
            opts.color_scheme,
        );

        let (interpreter_sender, interpreter_receiver) = channel();
        std::thread::spawn(move || interpreter.interpret_md(interpreter_receiver));

        interpreter_sender.send(md_string)?;

        let lines_to_scroll = opts.lines_to_scroll;

        let watcher = Watcher::spawn(event_loop_proxy.clone(), file_path.clone());

        let _ = file_path.parent().map(std::env::set_current_dir);

        let clipboard = clipboard::Clipboard::default();

        Ok(Self {
            opts,
            window,
            renderer,
            element_queue,
            elements: Vec::new(),
            lines_to_scroll,
            interpreter_sender,
            image_cache,
            keycombos,
            need_repositioning: false,
            watcher,
            selection: Selection::new(),
            pending_resize: None,
            scrollbar_held: None,
            mouse_down: false,
            modifiers: ModifiersState::empty(),
            mouse_position: Point::default(),
            event_loop_proxy,
            clipboard,
        })
    }

    pub fn position_queued_elements(
        element_queue: &Arc<Mutex<Vec<Element>>>,
        renderer: &mut Renderer,
        elements: &mut Vec<Positioned<Element>>,
    ) {
        let positioning_start = Instant::now();

        for element in element_queue.lock().drain(..) {
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

    fn load_file(&mut self, contents: String) {
        self.element_queue.lock().clear();
        self.elements.clear();
        self.renderer.positioner.reserved_height = DEFAULT_PADDING * self.renderer.hidpi_scale;
        self.renderer.positioner.anchors.clear();
        self.interpreter_sender.send(contents).unwrap();
    }

    fn update_file(&mut self, path: &Path, contents: String) {
        self.window.set_title(&utils::format_title(path));
        self.watcher.update_file(path, contents);
        self.renderer.set_scroll_y(0.0);
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
        taffy: &mut TaffyTree<()>,
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

    fn handle_user_event(&mut self, _event_loop: &ActiveEventLoop, inlyne_event: InlyneEvent) {
        match inlyne_event {
            InlyneEvent::LoadedImage(src, image_data) => {
                self.image_cache.lock().insert(src, image_data);
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
        }
    }

    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                let redraw_start = Instant::now();
                Self::position_queued_elements(
                    &self.element_queue,
                    &mut self.renderer,
                    &mut self.elements,
                );
                self.renderer.set_scroll_y(self.renderer.scroll_y);
                if let Err(err) = self
                    .renderer
                    .redraw(&mut self.elements, &mut self.selection)
                    .context("Renderer failed to redraw the screen")
                {
                    tracing::warn!("{}", err);
                }

                histogram!(HistTag::Redraw).record(redraw_start.elapsed());
            }
            WindowEvent::Resized(size) => self.pending_resize = Some(size),
            WindowEvent::CloseRequested => event_loop.exit(),
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
                        _some_link => CursorIcon::Pointer,
                    }
                } else {
                    CursorIcon::Default
                };
                self.window.set_cursor(cursor_icon);

                if self.scrollbar_held.is_some()
                    || (Rect::new(
                        (screen_size.0 - DEFAULT_MARGIN / 4., 0.),
                        (DEFAULT_MARGIN / 4., screen_size.1),
                    )
                    .contains(position.into())
                        && self.mouse_down)
                {
                    let scrollbar_height = self.renderer.scrollbar_height();
                    if self.scrollbar_held.is_none() {
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
                            self.scrollbar_held = Some(
                                position.y as f32
                                    - (((self.renderer.scroll_y
                                        / self.renderer.positioner.reserved_height)
                                        * screen_size.1)
                                        + scrollbar_height / 2.),
                            );
                        } else {
                            self.scrollbar_held = Some(0.);
                        }
                    }

                    let pos_y = if let Some(diff) = self.scrollbar_held {
                        position.y as f32 - diff
                    } else {
                        position.y as f32
                    };
                    let target_scroll = ((pos_y - scrollbar_height / 2.) / screen_size.1)
                        * self.renderer.positioner.reserved_height;
                    self.renderer.set_scroll_y(target_scroll);
                    self.window.request_redraw();
                } else if self.mouse_down && self.selection.handle_drag(loc) {
                    self.window.request_redraw();
                }
                self.mouse_position = loc;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    // Try to click a link
                    let screen_size = self.renderer.screen_size();

                    let y = self.mouse_position.1 - self.renderer.scroll_y;
                    if Rect::new(
                        (screen_size.0 - DEFAULT_MARGIN / 4., 0.),
                        (DEFAULT_MARGIN / 4., screen_size.1),
                    ).contains((self.mouse_position.0, y)) {
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
                        self.mouse_position,
                        screen_size,
                        self.renderer.zoom,
                    ) {
                        match hoverable {
                            Hoverable::Image(Image { is_link: Some(link), .. }) |
                            Hoverable::Text(Text { link: Some(link), .. }) => {
                                let path = PathBuf::from(link);

                                if  path.extension().is_some_and(|ext| ext == "md")
                                    && !path.to_str().is_some_and(|s| s.starts_with("http")) {
                                    // Open them in a new window, akin to what a browser does
                                    let modifiers = self.modifiers;
                                    if modifiers.shift_key() {
                                        std::thread::spawn(move || {
                                            Command::new(
                                                std::env::current_exe()
                                                    .unwrap_or_else(|_| "inlyne".into()),
                                            )
                                                .args(Opts::program_args(&path))
                                                .spawn()
                                                .expect("Couldn't spawn inlyne instance")
                                                .wait()
                                                .expect("Failed waiting on child");
                                        });
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
                                    self.window.set_cursor(CursorIcon::Default);
                                } else if let Err(e) = open::that(link) {
                                    tracing::error!("Could not open link: {e} from {:?}", std::env::current_dir())
                                }
                            },
                            Hoverable::Summary(summary) => {
                                let mut hidden = summary.hidden.borrow_mut();
                                *hidden = !*hidden;
                                self.event_loop_proxy
                                    .send_event(InlyneEvent::Reposition)
                                    .unwrap();
                                self.selection.add_position(self.mouse_position);
                            },
                            _ => {
                                self.selection.add_position(self.mouse_position);
                                self.window.request_redraw();
                            }
                        };
                    } else {
                        self.selection.add_position(self.mouse_position);
                        self.window.request_redraw()
                    }
                    self.mouse_down = true;
                }
                ElementState::Released => {
                    self.scrollbar_held = None;
                    self.mouse_down = false;
                }
            },
            WindowEvent::ModifiersChanged(new_state) => self.modifiers = new_state.state(),
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    state: ElementState::Pressed,
                    logical_key,
                    physical_key,
                    ..
                },
                ..
            } => {
                let key = Key::from_winit_key(&logical_key, &physical_key);
                let modified_key = ModifiedKey(key, self.modifiers);
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
                        Action::Copy => self.clipboard
                            .set_contents(self.selection.text.trim().to_owned()),
                        Action::Quit => event_loop.exit(),
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
        }
    }

    fn handle_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // We lazily store the size and only reposition elements and request a redraw when
        // we receive about_to_wait. This prevents us from clogging up the queue
        // with a bunch of costly resizes. (https://github.com/Inlyne-Project/inlyne/issues/25)
        if let Some(size) = self.pending_resize.take() {
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
}

/// Wrapper that holds an optional Inlyne (initialized on resumed) and
/// the configuration needed to create it.
struct App {
    /// Only `Some` before initialization; taken on first `resumed`.
    opts: Option<Opts>,
    event_loop_proxy: EventLoopProxy<InlyneEvent>,
    inlyne: Option<Inlyne>,
}

impl ApplicationHandler<InlyneEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        if self.inlyne.is_none() {
            if let Some(opts) = self.opts.take() {
                match Inlyne::new(opts, event_loop, self.event_loop_proxy.clone()) {
                    Ok(inlyne) => self.inlyne = Some(inlyne),
                    Err(err) => {
                        tracing::error!("Failed to initialize Inlyne: {}", err);
                        event_loop.exit();
                    }
                }
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: InlyneEvent) {
        if let Some(inlyne) = &mut self.inlyne {
            inlyne.handle_user_event(event_loop, event);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let Some(inlyne) = &mut self.inlyne {
            inlyne.handle_window_event(event_loop, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(inlyne) = &mut self.inlyne {
            inlyne.handle_about_to_wait(event_loop);
        }
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

            let event_loop = EventLoop::<InlyneEvent>::with_user_event().build()?;
            let event_loop_proxy = event_loop.create_proxy();

            let mut app = App {
                opts: Some(opts),
                event_loop_proxy,
                inlyne: None,
            };

            event_loop.run_app(&mut app)?;
        }
        Commands::Config(ConfigCmd::Open) => {
            let config_path = dirs::config_dir()
                .context("Failed to find the configuration directory")?
                .join("inlyne")
                .join("inlyne.toml");

            let config = std::fs::read_to_string(&config_path)
                .unwrap_or_else(|_| Config::default_config().to_string());

            let new_config = edit::edit_with_builder(
                &config,
                edit::Builder::new()
                    .prefix("inlyne_temp")
                    .suffix(".toml")
                    .keep(true),
            )?;

            _ = Config::load_from_str(&new_config)?;

            std::fs::write(config_path, new_config)?;
        }
    }

    Ok(())
}
