use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
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
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

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
    let window = Box::new(DummyWindow(counter));
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

                let syntect_theme = $crate::color::Theme::light_default().code_highlighter;
                let htmlified = $crate::utils::markdown_to_html(text, syntect_theme);
                let description = format!(" --- md\n\n{text}\n\n --- html\n\n{htmlified}");

                ::insta::with_settings!({
                    description => description,
                }, {
                    insta::assert_debug_snapshot!(interpret_md(text));
                });
            }
        )*
    }
}

const FOOTNOTES_LIST_PREFIX: &str = "\
This sentence[^1] has two footnotes[^2]

[^1]: 1st footnote
[^2]: 2nd footnote";

const CHECKLIST_HAS_NO_TEXT_PREFIX: &str = "\
- [x] Completed task
- [ ] Incomplete task";

const CODE_BLOCK_BG_COLOR: &str = "\
```
Fenced code block with no language tag
```

```rust
// Rust code
fn main() {}
```";

const BARE_LINK_GETS_AUTOLINKED: &str = "\
In a paragraph https://example.org

- In a list https://example.org
";

const TOML_GETS_HIGHLIGHTED: &str = "\
```toml
key = 123
```
";

const HANDLES_COMMA_IN_INFO_STR: &str = "\
```rust,ignore
let v = 1;
```
";

snapshot_interpreted_elements!(
    (footnotes_list_prefix, FOOTNOTES_LIST_PREFIX),
    (checklist_has_no_text_prefix, CHECKLIST_HAS_NO_TEXT_PREFIX),
    (code_block_bg_color, CODE_BLOCK_BG_COLOR),
    (bare_link_gets_autolinked, BARE_LINK_GETS_AUTOLINKED),
    (toml_gets_highlighted, TOML_GETS_HIGHLIGHTED),
    (handles_comma_in_info_str, HANDLES_COMMA_IN_INFO_STR),
);

/// Spin up a server, so we can test network requests without external services
fn mock_file_server(url_path: &str, mime: &str, file_path: &Path) -> (MockServer, String) {
    let bytes = fs::read(file_path).unwrap();
    let setup_server = async {
        let mock_server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(url_path))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes, mime))
            .mount(&mock_server)
            .await;
        mock_server
    };
    let server = pollster::block_on(setup_server);

    let full_url = format!("{}{}", server.uri(), url_path);
    (server, full_url)
}

#[test]
fn centered_image_with_size_align_and_link() {
    let logo_path = Path::new("tests").join("assets").join("bun_logo.png");
    let (_server, logo_url) = mock_file_server("/bun_logo.png", "image/png", &logo_path);

    let text = format!(
        r#"
<p align="center">
  <a href="https://bun.sh"><img src="{logo_url}" alt="Logo" height=170></a>
</p>"#,
    );

    insta::with_settings!({
        // The port for the URL here is non-deterministic, but the description changing doesn't
        // invalidate the snapshot, so that's okay
        description => &text,
    }, {
        insta::assert_debug_snapshot!(interpret_md(&text));
    });
}
