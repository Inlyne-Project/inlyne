mod ast;
mod hir;
mod html;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::slice;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

use crate::color::{native_color, Theme};
use crate::image::ImageData;
use crate::opts::ResolvedTheme;
use crate::positioner::Row;
use crate::text::TextBox;
use crate::utils::markdown_to_html;
use crate::{Element, ImageCache, InlyneEvent};
use html::{
    style::{FontStyle, FontWeight, TextDecoration},
    Element as InterpreterElement,
};

use crate::interpreter::ast::{Ast, AstOpts};
use crate::interpreter::hir::Hir;
use comrak::Anchorizer;
use html5ever::tendril::*;
use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
use parking_lot::Mutex;
use wgpu::TextureFormat;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

struct State {
    global_indent: f32,
    element_stack: Vec<InterpreterElement>,
    text_options: html::TextOptions,
    span: Span,
    // Stores the row and a counter of newlines after each image
    inline_images: Option<(Row, usize)>,
    pending_anchor: Option<String>,
    pending_list_prefix: Option<String>,
    anchorizer: Anchorizer,
}

impl State {
    fn with_span_color(span_color: [f32; 4]) -> Self {
        Self {
            global_indent: 0.0,
            element_stack: Vec::new(),
            text_options: Default::default(),
            span: Span::with_color(span_color),
            inline_images: None,
            pending_anchor: None,
            pending_list_prefix: None,
            anchorizer: Default::default(),
        }
    }

    fn element_iter_mut(&mut self) -> slice::IterMut<'_, InterpreterElement> {
        self.element_stack.iter_mut()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Span {
    color: [f32; 4],
    weight: FontWeight,
    style: FontStyle,
    decor: TextDecoration,
}

impl Span {
    fn with_color(color: [f32; 4]) -> Self {
        Self {
            color,
            weight: Default::default(),
            style: Default::default(),
            decor: Default::default(),
        }
    }
}

// Images are loaded in a separate thread and use a callback to indicate when they're finished
pub trait ImageCallback {
    fn loaded_image(&self, src: String, image_data: Arc<Mutex<Option<ImageData>>>);
}

// External state from the interpreter that we want to stub out for testing
trait WindowInteractor {
    fn finished_single_doc(&self);
    fn request_redraw(&self);
    fn image_callback(&self) -> Box<dyn ImageCallback + Send>;
}

struct EventLoopCallback(EventLoopProxy<InlyneEvent>);

impl ImageCallback for EventLoopCallback {
    fn loaded_image(&self, src: String, image_data: Arc<Mutex<Option<ImageData>>>) {
        let event = InlyneEvent::LoadedImage(src, image_data);
        self.0.send_event(event).unwrap();
    }
}

// A real interactive window that is being used with `HtmlInterpreter`
struct LiveWindow {
    window: Arc<Window>,
    event_proxy: EventLoopProxy<InlyneEvent>,
}

impl WindowInteractor for LiveWindow {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn image_callback(&self) -> Box<dyn ImageCallback + Send> {
        Box::new(EventLoopCallback(self.event_proxy.clone()))
    }

    fn finished_single_doc(&self) {
        self.event_proxy
            .send_event(InlyneEvent::PositionQueue)
            .unwrap();
    }
}

pub struct HtmlInterpreter {
    element_queue: Arc<Mutex<Vec<Element>>>,
    current_textbox: TextBox,
    hidpi_scale: f32,
    theme: Theme,
    surface_format: TextureFormat,
    state: State,
    file_path: PathBuf,
    // Whether the interpreters is allowed to queue elements
    pub should_queue: Arc<AtomicBool>,
    // Whether interpreter should stop queuing till next received file
    stopped: bool,
    first_pass: bool,
    image_cache: ImageCache,
    window: Arc<parking_lot::Mutex<dyn WindowInteractor + Send>>,
    color_scheme: Option<ResolvedTheme>,
}

impl HtmlInterpreter {
    // FIXME: clippy is probably right here, but I didn't want to hold up setting up clippy for the
    // rest of the repo just because of here
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: Arc<Window>,
        element_queue: Arc<Mutex<Vec<Element>>>,
        theme: Theme,
        surface_format: TextureFormat,
        hidpi_scale: f32,
        file_path: PathBuf,
        image_cache: ImageCache,
        event_proxy: EventLoopProxy<InlyneEvent>,
        color_scheme: Option<ResolvedTheme>,
    ) -> Self {
        let live_window = LiveWindow {
            window,
            event_proxy,
        };
        Self::new_with_interactor(
            element_queue,
            theme,
            surface_format,
            hidpi_scale,
            file_path,
            image_cache,
            Arc::new(parking_lot::Mutex::new(live_window)),
            color_scheme,
        )
    }

    // TODO: fix in a later refactor (consolidate a lot of junk)
    #[allow(clippy::too_many_arguments)]
    fn new_with_interactor(
        element_queue: Arc<Mutex<Vec<Element>>>,
        theme: Theme,
        surface_format: TextureFormat,
        hidpi_scale: f32,
        file_path: PathBuf,
        image_cache: ImageCache,
        window: Arc<parking_lot::Mutex<dyn WindowInteractor + Send>>,
        color_scheme: Option<ResolvedTheme>,
    ) -> Self {
        Self {
            window,
            element_queue,
            current_textbox: TextBox::new(Vec::new(), hidpi_scale),
            hidpi_scale,
            surface_format,
            state: State::with_span_color(native_color(theme.code_color, &surface_format)),
            theme,
            file_path,
            should_queue: Arc::new(AtomicBool::new(true)),
            stopped: false,
            first_pass: true,
            image_cache,
            color_scheme,
        }
    }

    pub fn interpret_md(self, receiver: mpsc::Receiver<String>) {
        let mut input = BufferQueue::default();

        let code_highlighter = self.theme.code_highlighter.clone();
        let mut tok = Tokenizer::new(Hir::new(), TokenizerOpts::default());

        let ast = Ast::new(AstOpts {
            anchorizer: parking_lot::Mutex::new(Anchorizer::default()),
            hidpi_scale: self.hidpi_scale,
            surface_format: self.surface_format,
            theme: self.theme,
            color_scheme: self.color_scheme,
            image_cache: Arc::clone(&self.image_cache),
            window: Arc::clone(&self.window),
        });
        for md_string in receiver {
            tracing::debug!(
                "Received markdown for interpretation: {} bytes",
                md_string.len()
            );

            let htmlified = markdown_to_html(&md_string, code_highlighter.clone());

            input.push_back(
                Tendril::from_str(&htmlified)
                    .unwrap()
                    .try_reinterpret::<fmt::UTF8>()
                    .unwrap(),
            );

            let _ = tok.feed(&mut input);
            assert!(input.is_empty());
            tok.end();

            *self.element_queue.lock() = ast.interpret(std::mem::take(&mut tok.sink)).into();
            self.window.lock().finished_single_doc();
        }
    }
}
