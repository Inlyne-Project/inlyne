use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};
use std::{env, thread};

use super::{HtmlInterpreter, ImageCallback, WindowInteractor};
use crate::color::{Theme, ThemeDefaults};
use crate::image::{Image, ImageData};
use crate::opts::ResolvedTheme;
use crate::test_utils::init_test_log;
use crate::utils::Align;
use crate::{Element, ImageCache};

use base64::prelude::*;
use syntect::highlighting::Theme as SyntectTheme;
use wgpu::TextureFormat;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

// We use a dummy window with an internal counter that keeps track of when rendering a single md
// document is finished
#[derive(Clone)]
struct AtomicCounter(Arc<AtomicU32>);

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

struct InterpreterOpts {
    theme: Theme,
    fail_after: Duration,
    color_scheme: Option<ResolvedTheme>,
}

impl Default for InterpreterOpts {
    fn default() -> Self {
        Self {
            theme: Theme::light_default(),
            fail_after: Duration::from_secs(8),
            color_scheme: None,
        }
    }
}

impl InterpreterOpts {
    fn new() -> Self {
        Self::default()
    }

    fn theme<T: Into<Theme>>(mut self, theme: T) -> Self {
        self.theme = theme.into();
        self
    }

    fn set_color_scheme(&mut self, color_scheme: ResolvedTheme) {
        self.color_scheme = Some(color_scheme);
    }

    fn finish(self, counter: AtomicCounter) -> (HtmlInterpreter, Arc<Mutex<VecDeque<Element>>>) {
        let Self {
            theme,
            fail_after: _,
            color_scheme,
        } = self;
        let element_queue = Arc::default();
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
            color_scheme,
        );

        (interpreter, element_queue)
    }
}

#[derive(Default)]
struct ThemeOpts {
    code_highlighter: Option<SyntectTheme>,
}

impl From<ThemeDefaults> for ThemeOpts {
    fn from(default: ThemeDefaults) -> Self {
        Self {
            code_highlighter: Some(default.into()),
        }
    }
}

impl From<ThemeOpts> for Theme {
    fn from(opts: ThemeOpts) -> Self {
        let ThemeOpts { code_highlighter } = opts;
        let mut theme = Theme::light_default();
        if let Some(code_highlighter) = code_highlighter {
            theme.code_highlighter = code_highlighter;
        }
        theme
    }
}

impl From<ThemeDefaults> for Theme {
    fn from(default: ThemeDefaults) -> Self {
        ThemeOpts::from(default).into()
    }
}

fn interpret_md(text: &str) -> VecDeque<Element> {
    interpret_md_with_opts(text, InterpreterOpts::new())
}

fn interpret_md_with_opts(text: &str, opts: InterpreterOpts) -> VecDeque<Element> {
    let fail_after = opts.fail_after;

    let counter = AtomicCounter::new();
    let (interpreter, element_queue) = opts.finish(counter.clone());
    let (md_tx, md_rx) = mpsc::channel();
    md_tx.send(text.to_owned()).unwrap();
    let interpreter_handle = std::thread::spawn(|| {
        interpreter.interpret_md(md_rx);
    });

    let start = Instant::now();
    while !counter.is_finished() {
        if interpreter_handle.is_finished() {
            panic!("The interpreter died >:V");
        } else if start.elapsed() > fail_after {
            panic!("The interpreter appeared to hang. Some task probably panicked");
        }
        thread::sleep(Duration::from_millis(1));
    }

    let mut elements_queue = element_queue.lock().unwrap();
    std::mem::take(&mut *elements_queue)
}

#[macro_export]
macro_rules! snapshot_interpreted_elements {
    ( $( ($test_name:ident, $md_text:ident) ),* $(,)? ) => {
        $crate::snapshot_interpreted_elements!(
            InterpreterOpts::new(),
            $(
                ($test_name, $md_text),
            )*
        );
    };
    ( $opts:expr, $( ($test_name:ident, $md_text:ident) ),* $(,)? ) => {
        $(
            #[test]
            fn $test_name() {
                $crate::test_utils::init_test_log();

                let text = $md_text;
                let opts = $opts;

                let htmlified = $crate::utils::markdown_to_html(
                    text,
                    opts.theme.code_highlighter.clone(),
                );
                let description = format!(" --- md\n\n{text}\n\n --- html\n\n{htmlified}");

                ::insta::with_settings!({
                    description => description,
                }, {
                    insta::assert_debug_snapshot!(interpret_md_with_opts(text, opts));
                });
            }
        )*
    }
}

#[allow(unused)]
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

const UNORDERED_LIST_IN_ORDERED: &str = "\
1. 1st outer
    - bullet
2. 2nd outer
";

const NESTED_ORDERED_LIST: &str = "\
1. 1st outer
    1. 1st inner
2. 2nd outer
";

const ORDERED_LIST_IN_UNORDERED: &str = "\
- bullet
    1. 1st inner
- bullet
";

const PARA_IN_ORDERED_LIST: &str = "\
1. 1st item

    Nested paragraph

2. 2nd item
";

const CODE_IN_ORDERED_LIST: &str = "\
1. 1st item

    ```rust
    fn main() {}
    ```

2. 2nd item
";

const YAML_FRONTMATTER: &str = "\
---
date: 2018-05-01
tags:
  - another tag
---
# Markdown h1 header
";

const ALIGNED_TABLE: &str = "\
| left default | left forced | centered | right | left default |
| ------------ | :---------- | :------: | ----: | ------------ |
| text         | text        |   text   |  text | text         |
";

const UNIQUE_ANCHORS: &str = "\
# Foo
# Foo
";

// TODO: this snapshot splits `Text` up a lot more than needed
const KBD_TAG: &str = "\
Keyboard text: <kbd>Alt-\\<num\\></kbd>
";

const BLOCKQUOTE: &str = "\
> blockquote
";

const HORIZONTAL_RULER: &str = "\
horizontal ruler vv

---
";

const SMALL_TEXT: &str = "\
<small>small</small>
";

const TEXT_STYLES: &str = "\
**bold**

_italic_

~~strikethrough~~

<u>underline</u>
";

// TODO: this still has all sorts of issues (the anchor and extra whitespace)
const HEADER_INHERIT_ALIGN: &str = r##"
<div align="center">
  <h4>
    <a href="#install">
      Install
    </a>
    <span> | </span>
    <a href="#usage">
      Usage
    </a>
  </h4>
</div>"##;

const COLLAPSED_SECTION: &str = "\
<details>
<summary>summary</summary>

collapsed text
</details>
";

snapshot_interpreted_elements!(
    // (footnotes_list_prefix, FOOTNOTES_LIST_PREFIX),
    (checklist_has_no_text_prefix, CHECKLIST_HAS_NO_TEXT_PREFIX),
    (code_block_bg_color, CODE_BLOCK_BG_COLOR),
    (bare_link_gets_autolinked, BARE_LINK_GETS_AUTOLINKED),
    (toml_gets_highlighted, TOML_GETS_HIGHLIGHTED),
    (handles_comma_in_info_str, HANDLES_COMMA_IN_INFO_STR),
    (unordered_list_in_ordered, UNORDERED_LIST_IN_ORDERED),
    (nested_ordered_list, NESTED_ORDERED_LIST),
    (ordered_list_in_unordered, ORDERED_LIST_IN_UNORDERED),
    (para_in_ordered_list, PARA_IN_ORDERED_LIST),
    (code_in_ordered_list, CODE_IN_ORDERED_LIST),
    (yaml_frontmatter, YAML_FRONTMATTER),
    (aligned_table, ALIGNED_TABLE),
    (unique_anchors, UNIQUE_ANCHORS),
    (kbd_tag, KBD_TAG),
    (blockquote, BLOCKQUOTE),
    (horizontal_ruler, HORIZONTAL_RULER),
    (small_text, SMALL_TEXT),
    (text_styles, TEXT_STYLES),
    (header_inherit_align, HEADER_INHERIT_ALIGN),
    (collapsed_section, COLLAPSED_SECTION),
);

const UNDERLINE_IN_CODEBLOCK: &str = "\
```rust
use std::io;
```";

const LET_IS_ITALICIZED: &str = "\
```rust
let foo;
```";

snapshot_interpreted_elements!(
    InterpreterOpts::new().theme(ThemeDefaults::Dracula),
    (underline_in_codeblock, UNDERLINE_IN_CODEBLOCK),
    (let_is_italicized, LET_IS_ITALICIZED),
);

const NUM_IS_BOLD: &str = "\
```ts
3000;
```";

snapshot_interpreted_elements!(
    InterpreterOpts::new().theme(ThemeDefaults::Zenburn),
    (num_is_bold, NUM_IS_BOLD),
);

struct File {
    url_path: String,
    mime: String,
    bytes: Vec<u8>,
}

impl File {
    fn new(url_path: &str, mime: &str, bytes: &[u8]) -> Self {
        Self {
            url_path: url_path.to_owned(),
            mime: mime.to_owned(),
            bytes: bytes.to_owned(),
        }
    }
}

/// Spin up a server, so we can test network requests without external services
fn mock_file_server(files: &[File]) -> (MockServer, String) {
    let setup_server = async {
        let mock_server = MockServer::start().await;

        for file in files {
            let File {
                url_path,
                mime,
                bytes,
            } = file;
            Mock::given(matchers::method("GET"))
                .and(matchers::path(url_path))
                .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_owned(), mime))
                .mount(&mock_server)
                .await;
        }

        mock_server
    };
    let server = pollster::block_on(setup_server);

    let server_url = server.uri();
    (server, server_url)
}

#[test]
fn centered_image_with_size_align_and_link() {
    init_test_log();

    let logo = include_bytes!("../../assets/test_data/bun_logo.png");
    let logo_path = "/bun_logo.png";
    let (_server, server_url) = mock_file_server(&[File::new(logo_path, "image/png", logo)]);
    let logo_url = server_url + logo_path;

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

#[test]
fn image_loading_fails_gracefully() {
    init_test_log();

    let json = r#"{"im": "not an image"}"#;
    let json_path = "/snapshot.png";
    let (_server, server_url) =
        mock_file_server(&[File::new(json_path, "application/json", json.as_bytes())]);
    let json_url = server_url + json_path;

    let text = format!("![This actually returns JSON 😈]({json_url})");

    // Windows CI takes a very variable amount of time to handle this test specifically, and I'm
    // not sure why. Bump up the timeout delay to reduce the amount of spurious failures in as
    // tight of a niche as we can specify
    let mut opts = InterpreterOpts::new();
    if env::var("CI").map_or(false, |var| ["true", "1"].contains(&&*var.to_lowercase()))
        && cfg!(target_os = "windows")
    {
        opts.fail_after *= 2;
    }

    insta::with_settings!({
        // The port for the URL here is non-deterministic, but the description changing doesn't
        // invalidate the snapshot, so that's okay
        description => &text,
    }, {
        insta::assert_debug_snapshot!(interpret_md_with_opts(&text, opts));
    });
}

// Check to see that each paths are used for their respective color-schemes
#[test]
fn picture_dark_light() {
    fn find_image(elements: &VecDeque<Element>) -> Option<&Image> {
        elements.iter().find_map(|element| match element {
            crate::Element::Image(image) => Some(image),
            _ => None,
        })
    }

    const B64_SINGLE_PIXEL_WEBP_000: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICICEREJAA==";
    const B64_SINGLE_PIXEL_WEBP_999: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICICzYyIBA==";
    const B64_SINGLE_PIXEL_WEBP_FFF: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICIC/Y+IBA==";

    init_test_log();

    let light_path = "/light.webp";
    let dark_path = "/dark.webp";
    let default_path = "/default.webp";
    let webp_mime = "image/webp";
    let files = [
        (dark_path, B64_SINGLE_PIXEL_WEBP_FFF),
        (light_path, B64_SINGLE_PIXEL_WEBP_000),
        (default_path, B64_SINGLE_PIXEL_WEBP_999),
    ]
    .map(|(path, b64_bytes)| {
        let bytes = BASE64_STANDARD.decode(b64_bytes).unwrap();
        File::new(path, webp_mime, &bytes)
    });
    let (_server, server_url) = mock_file_server(&files);
    let dark_url = format!("{server_url}{dark_path}");
    let light_url = format!("{server_url}{light_path}");
    let default_url = format!("{server_url}{default_path}");

    let text = format!(
        r#"
<p align="center">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="{dark_url}"/>
      <source media="(prefers-color-scheme: light)" srcset="{light_url}"/>
      <img src="{default_url}"/>
    </picture>
</p>
"#,
    );

    for color_scheme in [None, Some(ResolvedTheme::Dark), Some(ResolvedTheme::Light)] {
        let mut opts = InterpreterOpts::new();
        if let Some(color_scheme) = color_scheme {
            opts.set_color_scheme(color_scheme);
        }
        let elements = interpret_md_with_opts(&text, opts);
        let image = find_image(&elements).unwrap();

        // Should pick up align from the enclosing `<p>`
        assert_eq!(image.is_aligned, Some(Align::Center));

        let rgba_data = image
            .image_data
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .to_bytes();
        let byte = match color_scheme {
            Some(ResolvedTheme::Dark) => 0xff,
            Some(ResolvedTheme::Light) => 0x00,
            None => 0x99,
        };
        // Image data stores pixel data in RGBA format
        let expected = [byte, byte, byte, 0xff];
        assert_eq!(
            rgba_data, expected,
            "Failed for color scheme: {color_scheme:?}"
        );
    }
}
