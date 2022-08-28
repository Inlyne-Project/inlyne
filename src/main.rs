pub mod color;
pub mod fonts;
pub mod image;
pub mod interpreter;
mod keybindings;
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

use keybindings::{Action, Key, KeyCombos, ModifiedKey};
use opts::Args;
use opts::Config;
use positioner::Positioned;
use positioner::Row;
use positioner::Spacer;
use positioner::DEFAULT_MARGIN;
use positioner::DEFAULT_PADDING;
use renderer::Renderer;
use text::TextBox;
use utils::{ImageCache, MaybeImageData, Point, Rect, Size};

use anyhow::Context;
use copypasta::{ClipboardContext, ClipboardProvider};
use notify::op::Op;
use notify::{raw_watcher, RecursiveMode, Watcher};

use winit::event::ModifiersState;
use winit::event::{ElementState, MouseButton};
use winit::{
    event::{Event, KeyboardInput, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug)]
pub enum InlyneEvent {
    LoadedImage(String, MaybeImageData),
    FileReload,
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
    lines_to_scroll: f32,
    args: Args,
    image_cache: ImageCache,
    interpreter_sender: mpsc::Sender<String>,
    interpreter_should_queue: Arc<AtomicBool>,
    keycombos: KeyCombos,
}

/// Gets a relative path extending from the repo root falling back to the full path
fn root_filepath_to_vcs_dir(path: &Path) -> Option<PathBuf> {
    let mut full_path = path.canonicalize().ok()?;
    let mut parts = vec![full_path.file_name()?.to_owned()];

    full_path.pop();
    loop {
        full_path.push(".git");
        let is_git = full_path.exists();
        full_path.pop();
        full_path.push(".hg");
        let is_mercurial = full_path.exists();
        full_path.pop();

        let is_vcs_dir = is_git || is_mercurial;

        match full_path.file_name() {
            Some(name) => parts.push(name.to_owned()),
            // We've seached the full path and didn't find a vcs dir
            None => return Some(path.to_owned()),
        }
        if is_vcs_dir {
            let mut rooted = PathBuf::new();
            for part in parts.into_iter().rev() {
                rooted.push(part);
            }
            return Some(rooted);
        }

        full_path.pop();
    }
}

impl Inlyne {
    pub fn spawn_watcher(&self) {
        // Create a channel to receive the events.
        let (watch_tx, watch_rx) = channel();

        // Create a watcher object, delivering raw events.
        // The notification back-end is selected based on the platform.
        let mut watcher = raw_watcher(watch_tx).unwrap();

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        let root_folder = self
            .args
            .file_path
            .parent()
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| Path::new("."))
            .to_owned();

        let event_proxy = self.event_loop.create_proxy();
        let file_path = self.args.file_path.clone();
        std::thread::spawn(move || {
            watcher
                .watch(root_folder, RecursiveMode::Recursive)
                .unwrap();

            loop {
                let event = match watch_rx.recv() {
                    Ok(event) => event,
                    Err(err) => {
                        log::warn!("Config watcher channel dropped unexpectedly: {}", err);
                        break;
                    }
                };

                if event.op.unwrap().intersects(Op::WRITE)
                    && event
                        .path
                        .map(|p| p.file_name() == file_path.file_name())
                        .unwrap_or_default()
                {
                    // Always reload the primary configuration file.
                    let _ = event_proxy.send_event(InlyneEvent::FileReload);
                }
            }
        });
    }

    pub async fn new(opts: &Opts, args: Args) -> anyhow::Result<Self> {
        let keycombos = KeyCombos::new(opts.keybindings.clone())?;

        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Arc::new(Window::new(&event_loop).unwrap());
        match root_filepath_to_vcs_dir(&args.file_path) {
            Some(path) => window.set_title(&format!("Inlyne - {}", path.to_string_lossy())),
            None => window.set_title("Inlyne"),
        }
        let renderer = Renderer::new(
            &window,
            opts.theme.clone(),
            opts.scale.unwrap_or(window.scale_factor() as f32),
            opts.font_opts.clone(),
        )
        .await?;
        let clipboard = ClipboardContext::new().unwrap();

        let element_queue = Arc::new(Mutex::new(VecDeque::new()));
        let image_cache = Arc::new(Mutex::new(HashMap::new()));
        let interpreter = HtmlInterpreter::new(
            window.clone(),
            element_queue.clone(),
            renderer.theme.clone(),
            renderer.hidpi_scale,
            args.file_path.clone(),
            image_cache.clone(),
        );

        let (interpreter_sender, interpreter_reciever) = channel();
        let interpreter_should_queue = interpreter.should_queue.clone();
        std::thread::spawn(move || interpreter.intepret_md(interpreter_reciever));
        let md_string = std::fs::read_to_string(&opts.file_path)
            .with_context(|| format!("Could not read file at {:?}", opts.file_path))?;
        interpreter_sender.send(md_string)?;

        Ok(Self {
            window,
            event_loop,
            renderer,
            element_queue,
            clipboard,
            elements: Vec::new(),
            lines_to_scroll: opts.lines_to_scroll,
            args,
            interpreter_sender,
            interpreter_should_queue,
            image_cache,
            keycombos,
        })
    }

    pub fn run(mut self) {
        let mut pending_resize = None;
        let mut scrollbar_held = false;
        let mut mouse_down = false;
        let mut modifiers = ModifiersState::empty();
        let mut last_loc = (0.0, 0.0);
        let mut selection_cache = String::new();
        let mut selecting = false;
        let event_loop_proxy = self.event_loop.create_proxy();
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::LoadedImage(src, image_data) => {
                        self.image_cache.lock().unwrap().insert(src, image_data);
                        self.renderer.reposition(&mut self.elements).unwrap();
                        self.window.request_redraw()
                    }
                    InlyneEvent::FileReload => {
                        self.interpreter_should_queue
                            .store(false, Ordering::Relaxed);
                        self.element_queue.lock().unwrap().clear();
                        self.elements.clear();
                        self.renderer.positioner.reserved_height =
                            DEFAULT_PADDING * self.renderer.hidpi_scale;
                        self.renderer.positioner.anchors.clear();
                        let md_string = std::fs::read_to_string(&self.args.file_path)
                            .with_context(|| {
                                format!("Could not read file at {:?}", self.args.file_path)
                            })
                            .unwrap();
                        self.interpreter_should_queue.store(true, Ordering::Relaxed);
                        self.interpreter_sender.send(md_string).unwrap();
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
                            self.renderer
                                .positioner
                                .position(
                                    &mut self.renderer.glyph_brush,
                                    &mut positioned_element,
                                    self.renderer.zoom,
                                )
                                .unwrap();
                            self.renderer.positioner.reserved_height +=
                                DEFAULT_PADDING * self.renderer.hidpi_scale * self.renderer.zoom
                                    + positioned_element.bounds.as_ref().unwrap().size.1;
                            self.elements.push(positioned_element);
                        }
                    }
                    self.renderer.set_scroll_y(self.renderer.scroll_y);
                    self.renderer
                        .redraw(&mut self.elements)
                        .context("Renderer failed to redraw the screen")
                        .unwrap();
                    if selecting {
                        selection_cache = self.renderer.selection_text.clone();
                    }
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
                                selecting = true;
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
                                        args.file_path = maybe_path.unwrap();
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
                            selecting = false;
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
                                Action::ToTop => {
                                    self.renderer.set_scroll_y(0.0);
                                    self.window.request_redraw();
                                }
                                Action::ToBottom => {
                                    self.renderer.set_scroll_y(f32::INFINITY);
                                    self.window.request_redraw();
                                }
                                a_scroll @ (Action::ScrollUp | Action::ScrollDown) => {
                                    let lines = match a_scroll {
                                        Action::ScrollUp => 1.0,
                                        Action::ScrollDown => -1.0,
                                        _ => unreachable!("This arm is only for scroll actions"),
                                    };

                                    Self::scroll_lines(
                                        &mut self.renderer,
                                        &self.window,
                                        self.lines_to_scroll,
                                        lines,
                                    )
                                }
                                a_zoom @ (Action::ZoomIn | Action::ZoomOut | Action::ZoomReset) => {
                                    let zoom = match a_zoom {
                                        Action::ZoomIn => self.renderer.zoom * 1.1,
                                        Action::ZoomOut => self.renderer.zoom * 0.9,
                                        Action::ZoomReset => 1.0,
                                        _ => unreachable!("This arm is only for zoom actions"),
                                    };

                                    self.renderer.zoom = zoom;
                                    self.renderer.reposition(&mut self.elements).unwrap();
                                    self.renderer.set_scroll_y(self.renderer.scroll_y);
                                    self.window.request_redraw();
                                }
                                Action::Copy => self
                                    .clipboard
                                    .set_contents(selection_cache.trim().to_owned())
                                    .unwrap(),
                                Action::Quit => *control_flow = ControlFlow::Exit,
                            }
                        }
                    }
                    _ => {}
                },
                Event::MainEventsCleared => {
                    // We lazily store the size and only reposition elements and request a redraw when
                    // we recieve a `MainEventsCleared`.  This prevents us from clogging up the queue
                    // with a bunch of costly resizes. (https://github.com/trimental/inlyne/issues/25)
                    if let Some(size) = pending_resize.take() {
                        self.renderer.config.width = size.width;
                        self.renderer.config.height = size.height;
                        self.renderer.positioner.screen_size = size.into();
                        self.renderer
                            .surface
                            .configure(&self.renderer.device, &self.renderer.config);
                        self.renderer.reposition(&mut self.elements).unwrap();
                        self.renderer.set_scroll_y(self.renderer.scroll_y);
                        self.window.request_redraw();
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
        let num_pixels =
            num_lines as f32 * 16.0 * lines_to_scroll * renderer.hidpi_scale * renderer.zoom;
        Self::scroll_pixels(renderer, window, num_pixels);
    }

    fn scroll_pixels(renderer: &mut Renderer, window: &Window, num_pixels: f32) {
        renderer.set_scroll_y(renderer.scroll_y - num_pixels);
        window.request_redraw();
    }

    fn find_hoverable<'a, T: wgpu_glyph::GlyphCruncher>(
        elements: &'a [Positioned<Element>],
        glyph_brush: &'a mut T,
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
    let inlyne = pollster::block_on(Inlyne::new(&opts, args))?;

    inlyne.spawn_watcher();
    inlyne.run();

    Ok(())
}
