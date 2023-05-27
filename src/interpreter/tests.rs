use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

use super::{HtmlInterpreter, ImageCallback, WindowInteractor};
use crate::{color::Theme, image::ImageData, Element, ImageCache};

use wgpu::TextureFormat;

// We use a dummy window with an internal counter that keeps track of when rendering a single md
// document is finished
#[derive(Default)]
struct AtomicCounter(Arc<AtomicU32>);

impl Clone for AtomicCounter {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl AtomicCounter {
    fn new() -> Self {
        Self(Arc::new(AtomicU32::from(1)))
    }

    fn is_finished(&self) -> bool {
        let counter = self.0.load(Ordering::SeqCst);
        counter == 0
    }

    fn inc(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn dec(&self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct DummyWindow(AtomicCounter);

impl From<AtomicCounter> for DummyWindow {
    fn from(counter: AtomicCounter) -> Self {
        Self(counter)
    }
}

impl WindowInteractor for DummyWindow {
    fn finished_single_doc(&self) {
        self.0.dec();
    }

    fn request_redraw(&self) {}

    // The counter is inc'd for each callback we create and internally dec's when it's called
    fn image_callback(&self) -> Box<dyn ImageCallback + Send> {
        self.0.inc();
        Box::new(DummyCallback(self.0.clone()))
    }
}

struct DummyCallback(AtomicCounter);

impl ImageCallback for DummyCallback {
    fn loaded_image(&self, _: String, _: Arc<Mutex<Option<ImageData>>>) {
        self.0.dec();
    }
}

fn dummy_interpreter(counter: AtomicCounter) -> (HtmlInterpreter, Arc<Mutex<VecDeque<Element>>>) {
    let element_queue = Arc::default();
    let theme = Theme::light_default();
    let surface_format = TextureFormat::Bgra8UnormSrgb;
    let hidpi_scale = 1.0;
    let file_path = PathBuf::from("does_not_exist");
    let image_cache = ImageCache::default();
    let window = Box::new(DummyWindow::from(counter));
    let interpreter = HtmlInterpreter::new_with_interactor(
        Arc::clone(&element_queue),
        theme,
        surface_format,
        hidpi_scale,
        file_path,
        image_cache,
        window,
    );

    (interpreter, element_queue)
}

fn interpret_md(text: &str) -> VecDeque<Element> {
    let counter = AtomicCounter::new();
    let (interpreter, element_queue) = dummy_interpreter(counter.clone());
    let (md_tx, md_rx) = mpsc::channel();
    md_tx.send(text.to_owned()).unwrap();
    let _ = std::thread::spawn(|| {
        interpreter.interpret_md(md_rx);
    });

    while !counter.is_finished() {
        thread::sleep(Duration::from_millis(1));
    }

    let mut elements_queue = element_queue.lock().unwrap();
    std::mem::take(&mut *elements_queue)
}

macro_rules! snapshot_interpreted_elements {
    ( $( ($test_name:ident, $md_text:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                let text = $md_text;
                ::insta::with_settings!({
                    description => text,
                }, {
                    insta::assert_debug_snapshot!(interpret_md(text));
                });
            }
        )*
    }
}

const NUMBERED_LIST_PREFIX: &str = "\
This sentence[^1] has two footnotes[^2] followed by a list.

1. Ordered list

[^1]: 1st footnote
[^2]: 2nd footnote";

snapshot_interpreted_elements!((numbered_list_prefix, NUMBERED_LIST_PREFIX));
