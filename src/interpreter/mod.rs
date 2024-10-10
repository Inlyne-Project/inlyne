mod ast;
mod hir;
mod html;
#[cfg(test)]
mod tests;

use std::str::FromStr;
use std::sync::{mpsc, Arc};

use crate::color::Theme;
use crate::image::ImageData;
use crate::opts::ResolvedTheme;
use crate::utils::markdown_to_html;
use crate::{Element, ImageCache, InlyneEvent};
use html::style::{FontStyle, FontWeight, TextDecoration};

use crate::interpreter::ast::{Ast, AstOpts};
use crate::interpreter::hir::Hir;
use html5ever::tendril::*;
use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
use parking_lot::Mutex;
use wgpu::TextureFormat;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

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
    fn loaded_image(&self, src: String, image_data: Arc<std::sync::Mutex<Option<ImageData>>>);
}

// External state from the interpreter that we want to stub out for testing
trait WindowInteractor {
    fn finished_single_doc(&self);
    fn request_redraw(&self);
    fn image_callback(&self) -> Box<dyn ImageCallback + Send>;
}

struct EventLoopCallback(EventLoopProxy<InlyneEvent>);

impl ImageCallback for EventLoopCallback {
    fn loaded_image(&self, src: String, image_data: Arc<std::sync::Mutex<Option<ImageData>>>) {
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
    window: Arc<Mutex<dyn WindowInteractor + Send>>,
    theme: Theme,
    ast: Ast,
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
            image_cache,
            Arc::new(Mutex::new(live_window)),
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
        image_cache: ImageCache,
        window: Arc<Mutex<dyn WindowInteractor + Send>>,
        color_scheme: Option<ResolvedTheme>,
    ) -> Self {
        let ast = Ast::new(
            AstOpts {
                anchorizer: Default::default(),
                theme: theme.clone(),
                surface_format,
                hidpi_scale,
                image_cache,
                window: Arc::clone(&window),
                color_scheme,
            },
            element_queue,
        );

        Self { theme, window, ast }
    }

    pub fn interpret_md(self, receiver: mpsc::Receiver<String>) {
        let mut input = BufferQueue::default();

        let code_highlighter = self.theme.code_highlighter.clone();
        let mut tok = Tokenizer::new(Hir::new(), TokenizerOpts::default());

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

            self.ast.interpret(std::mem::take(&mut tok.sink));
            self.window.lock().finished_single_doc();
        }
    }
}
