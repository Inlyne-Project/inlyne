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
use crate::positioner::Spacer;
use crate::test_utils::image::{Sample, SamplePng};
use crate::test_utils::{log, server};
use crate::text::{Text, TextBox};
use crate::utils::Align;
use crate::{Element, ImageCache};

use base64::prelude::*;
use glyphon::FamilyOwned;
use pretty_assertions::assert_eq;
use smart_debug::SmartDebug;
use syntect::highlighting::Theme as SyntectTheme;
use tiny_http::{Header, Response};
use wgpu::TextureFormat;

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

impl From<ThemeDefaults> for InterpreterOpts {
    fn from(theme_default: ThemeDefaults) -> Self {
        Self::new().theme(theme_default)
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
                $crate::test_utils::log::init();

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
    (para_in_ordered_list, PARA_IN_ORDERED_LIST),
    (code_in_ordered_list, CODE_IN_ORDERED_LIST),
    (yaml_frontmatter, YAML_FRONTMATTER),
    (aligned_table, ALIGNED_TABLE),
    (header_inherit_align, HEADER_INHERIT_ALIGN),
    (collapsed_section, COLLAPSED_SECTION),
);

fn elem_as_text_box(elem: &Element) -> Option<&TextBox> {
    if let Element::TextBox(text_box) = elem {
        Some(text_box)
    } else {
        None
    }
}

const UNIQUE_ANCHORS: &str = "\
# Foo
# Foo
";

#[test]
fn identical_anchors_are_unique() {
    log::init();

    let elems = interpret_md(UNIQUE_ANCHORS);
    let anchors: Vec<_> = elems
        .iter()
        .filter_map(|elem| {
            let text_box = elem_as_text_box(elem)?;
            text_box.is_anchor.as_deref()
        })
        .collect();
    insta::assert_debug_snapshot!(anchors, @r###"
    [
        "#foo",
        "#foo-1",
    ]
    "###);
}

const BARE_LINK_GETS_AUTOLINKED: &str = "\
In a paragraph https://example.org/in/para

- In a list https://example.org/in/list
";

#[test]
fn bare_link_gets_autolinked() {
    log::init();

    let elems = interpret_md(BARE_LINK_GETS_AUTOLINKED);
    let links: Vec<_> = elems
        .iter()
        .filter_map(elem_as_text_box)
        .flat_map(|text_box| text_box.texts.iter())
        .filter_map(|text| text.link.as_deref())
        .collect();
    insta::assert_debug_snapshot!(links, @r###"
    [
        "https://example.org/in/para",
        "https://example.org/in/list",
    ]
    "###);
}

const BLOCKQUOTE: &str = r#"
> One level
>
> > Two levels
>
> One level
"#;

#[test]
fn blockquote() {
    log::init();

    let elems = interpret_md(BLOCKQUOTE);
    let quoteblock_indent_to_text: Vec<_> = elems
        .iter()
        .filter_map(|elem| {
            let text_box = elem_as_text_box(elem)?;
            let depth = text_box.is_quote_block?;
            let text: String = text_box.texts.iter().map(|t| t.text.as_str()).collect();
            Some((depth, text))
        })
        .collect();
    insta::assert_debug_snapshot!(quoteblock_indent_to_text, @r###"
    [
        (
            1,
            "One level",
        ),
        (
            2,
            "Two levels",
        ),
        (
            1,
            "One level",
        ),
    ]
    "###);
}

#[test]
fn horizontal_ruler_is_visible_spacer() {
    log::init();

    let elems = interpret_md("---");
    let num_visible_spacers = elems
        .iter()
        .filter(|elem| matches!(elem, Element::Spacer(Spacer { visible: true, .. })))
        .count();
    assert_eq!(num_visible_spacers, 1);
}

fn collect_list_prefixes(elems: &VecDeque<Element>) -> Vec<(&str, f32)> {
    elems
        .iter()
        .filter_map(|elem| {
            let text_box = elem_as_text_box(elem)?;
            let prefix = text_box.texts.first()?.text.as_str();
            let indent = text_box.indent;
            Some((prefix, indent))
        })
        .collect()
}

const UNORDERED_LIST_IN_ORDERED: &str = "\
1. 1st outer
    - bullet
2. 2nd outer
";

#[test]
fn unordered_list_in_ordered() {
    log::init();

    let elems = interpret_md(UNORDERED_LIST_IN_ORDERED);
    let list_prefixes = collect_list_prefixes(&elems);
    insta::assert_debug_snapshot!(list_prefixes, @r###"
    [
        (
            "1. ",
            50.0,
        ),
        (
            "· ",
            100.0,
        ),
        (
            "2. ",
            50.0,
        ),
    ]
    "###);
}

const NESTED_ORDERED_LIST: &str = "\
1. 1st outer
    1. 1st inner
2. 2nd outer
";

#[test]
fn nested_ordered_list() {
    log::init();

    let elems = interpret_md(NESTED_ORDERED_LIST);
    let list_prefixes = collect_list_prefixes(&elems);
    insta::assert_debug_snapshot!(list_prefixes, @r###"
    [
        (
            "1. ",
            50.0,
        ),
        (
            "1. ",
            100.0,
        ),
        (
            "2. ",
            50.0,
        ),
    ]
    "###);
}

const ORDERED_LIST_IN_UNORDERED: &str = "\
- bullet
    1. 1st inner
- bullet
";

#[test]
fn ordered_list_in_unordered() {
    log::init();

    let elems = interpret_md(ORDERED_LIST_IN_UNORDERED);
    let list_prefixes = collect_list_prefixes(&elems);
    insta::assert_debug_snapshot!(list_prefixes, @r###"
    [
        (
            "· ",
            50.0,
        ),
        (
            "1. ",
            100.0,
        ),
        (
            "· ",
            50.0,
        ),
    ]
    "###);
}

#[test]
fn small_text() {
    log::init();

    let md = "<small>small</small>\n\nregular";
    let elems = interpret_md(md);
    let mut font_box_size_it = elems
        .iter()
        .filter_map(|elem| elem_as_text_box(elem).map(|text_box| text_box.font_size));
    let small_size = font_box_size_it.next().expect("Small text");
    let regular_size = font_box_size_it.next().expect("Regular text");
    assert_eq!(font_box_size_it.next(), None);
    assert!(
        small_size < regular_size,
        "Small ({small_size}) text should be smaller than regular ({regular_size})",
    );
}

#[derive(SmartDebug, Default, PartialEq, Eq)]
#[debug(skip_defaults)]
struct Styles {
    bold: bool,
    italic: bool,
    striked: bool,
    underline: bool,
}

impl From<&Text> for Styles {
    fn from(text: &Text) -> Self {
        Self {
            bold: text.is_bold,
            italic: text.is_italic,
            striked: text.is_striked,
            underline: text.is_underlined,
        }
    }
}

impl Styles {
    fn new() -> Self {
        Self::default()
    }

    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    fn striked(mut self) -> Self {
        self.striked = true;
        self
    }

    fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

const TEXT_STYLES: &str = "\
**bold**

_italic_

~~strikethrough~~

<u>underline</u>";

#[test]
fn text_styles() {
    log::init();

    let elems = interpret_md(TEXT_STYLES);
    let styles: Vec<Styles> = elems
        .iter()
        .filter_map(|elem| {
            let text_box = elem_as_text_box(elem)?;
            let style = text_box.texts.first()?.into();
            Some(style)
        })
        .collect();
    assert_eq!(
        styles,
        [
            Styles::new().bold(),
            Styles::new().italic(),
            Styles::new().striked(),
            Styles::new().underline()
        ]
    )
}

#[test]
fn kbd_tag_monospace() {
    log::init();

    let md = "Keyboard text: <kbd>Alt-\\<num\\></kbd>";
    let elems = interpret_md(md);
    let mono_text: String = elems
        .iter()
        .filter_map(elem_as_text_box)
        .flat_map(|text_box| text_box.texts.iter())
        .filter_map(|text| match text.font_family {
            FamilyOwned::Monospace => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    insta::assert_snapshot!(&mono_text, @"Alt-<num>");
}

const UNDERLINE_IN_CODEBLOCK: &str = "\
```rust
use std::io;
```
";

#[test]
fn underline_in_codeblock() {
    log::init();

    let elems = interpret_md_with_opts(UNDERLINE_IN_CODEBLOCK, ThemeDefaults::Dracula.into());
    let underlined_code: Vec<&Text> = elems
        .iter()
        .filter_map(elem_as_text_box)
        .flat_map(|text_box| text_box.texts.iter())
        .filter(|text| text.is_underlined)
        .collect();
    insta::assert_debug_snapshot!(underlined_code, @r###"
    [
        Text {
            text: "std",
            font_family: Monospace,
            color: Some(Color { r: 0.13, g: 0.69, b: 0.86 }),
            style: UNDERLINED ,
            ..
        },
        Text {
            text: "::",
            font_family: Monospace,
            color: Some(Color { r: 1.00, g: 0.19, b: 0.56 }),
            style: UNDERLINED ,
            ..
        },
    ]
    "###)
}

fn find_text_within_elem(text: &str) -> impl Fn(&Element) -> Option<&Text> + '_ {
    move |elem| {
        let text_box = elem_as_text_box(elem)?;
        text_box.texts.iter().find(|t| t.text == text)
    }
}

const ITALICS_IN_CODEBLOCK: &str = "\
```rust
let foo;
```
";

#[test]
fn italics_in_codeblock() {
    log::init();

    let elems = interpret_md_with_opts(ITALICS_IN_CODEBLOCK, ThemeDefaults::Dracula.into());
    let italicized_let = elems.iter().find_map(find_text_within_elem("let")).unwrap();
    assert_eq!(Styles::from(italicized_let), Styles::new().italic());
    insta::assert_debug_snapshot!(italicized_let, @r###"
    Text {
        text: "let",
        font_family: Monospace,
        color: Some(Color { r: 0.26, g: 0.81, b: 0.98 }),
        style: ITALIC ,
        ..
    }
    "###);
}

const BOLD_IN_CODEBLOCK: &str = "\
```ts
3000;
```
";

// This specific theme and code should bold the number `3000` which tests some specific parts of
// our codeblock interpretation
#[test]
fn bold_in_codeblock() {
    log::init();

    let elems = interpret_md_with_opts(BOLD_IN_CODEBLOCK, ThemeDefaults::Zenburn.into());
    let bold_3000 = elems
        .iter()
        .find_map(find_text_within_elem("3000"))
        .unwrap();
    assert_eq!(Styles::from(bold_3000), Styles::new().bold());
    insta::assert_debug_snapshot!(bold_3000, @r###"
    Text {
        text: "3000",
        font_family: Monospace,
        color: Some(Color { r: 0.24, g: 0.67, b: 0.67 }),
        style: BOLD ,
        ..
    }
    "###);
}

const HANDLES_COMMA_IN_INFO_STR: &str = "\
```rust,ignore
let v = 1;
```
";

#[test]
fn handles_comma_in_info_str() {
    log::init();

    let regular = HANDLES_COMMA_IN_INFO_STR.replace(",ignore", "");
    assert_ne!(
        HANDLES_COMMA_IN_INFO_STR, regular,
        "Should have modified the fence block tag"
    );

    let with_comma = interpret_md(HANDLES_COMMA_IN_INFO_STR);
    let regular = interpret_md(&regular);
    assert_eq!(
        with_comma, regular,
        "Should contain identically highlighted text"
    );
}

// TODO: Add a test that verifies the background color is the same for both text boxes
const CODE_BLOCK_BG_COLOR: &str = "\
```
Fenced code block with no language tag
```

```rust
// Rust code
fn main() {}
```";

#[test]
fn code_block_bg_color() {
    log::init();

    let elems = interpret_md(CODE_BLOCK_BG_COLOR);
    let codeblock_bg: Vec<_> = elems
        .iter()
        .filter_map(|elem| {
            let text_box = elem_as_text_box(elem)?;
            text_box.background_color
        })
        .collect();

    match codeblock_bg.as_slice() {
        &[plain_bg, rust_bg] => assert_eq!(plain_bg, rust_bg),
        unexpected => panic!("Expected 2 codeblocks. Found: {}", unexpected.len()),
    }
}

const TOML_GETS_HIGHLIGHTED: &str = "\
```toml
key = 123
```
";

#[test]
fn toml_gets_highlighted() {
    log::init();

    let without_tag = TOML_GETS_HIGHLIGHTED.replace("toml", "");
    assert_ne!(TOML_GETS_HIGHLIGHTED, without_tag, "TOML tag is removed");
    let highlighted_elems = interpret_md(TOML_GETS_HIGHLIGHTED);
    let plain_elems = interpret_md(&without_tag);
    assert_ne!(highlighted_elems, plain_elems, "Highlighting should differ");
}

fn find_image(elements: &VecDeque<Element>) -> Option<&Image> {
    elements.iter().find_map(|element| match element {
        crate::Element::Image(image) => Some(image),
        _ => None,
    })
}

#[test]
fn centered_image_with_size_align_and_link() {
    log::init();

    let logo: Sample = SamplePng::Bun.into();
    let logo_path = "/bun_logo.png";
    let files = vec![server::File::new(
        logo_path,
        logo.content_type(),
        &logo.pre_decode(),
    )];
    let (_server, server_url) = server::mock_file_server(files);
    let logo_url = server_url + logo_path;

    let text = format!(
        r#"
<p align="center">
  <a href="https://bun.sh"><img src="{logo_url}" alt="Logo" height=170></a>
</p>"#,
    );

    let elems = interpret_md(&text);
    let image = find_image(&elems).unwrap();
    insta::assert_debug_snapshot!(image, @r###"
    Image {
        image_data: Mutex {
            data: Some(
                ImageData {
                    lz4_blob: { len: 21244, data: [4, 34, 77, ..] },
                    scale: true,
                    dimensions: (396, 347),
                },
            ),
            poisoned: false,
            ..
        },
        is_aligned: Some(Center),
        size: Some(PxHeight(Px(170))),
        is_link: Some("https://bun.sh"),
        ..
    }
    "###);
}

// TODO: change this to test against the image cache so that we can inspect the error?
#[test]
fn image_loading_fails_gracefully() {
    log::init();

    let json = r#"{"im": "not an image"}"#;
    let json_path = "/snapshot.png";
    let (_server, server_url) = server::mock_file_server(vec![server::File::new(
        json_path,
        "application/json",
        json.as_bytes(),
    )]);
    let json_url = server_url + json_path;

    let text = format!("![This actually returns JSON 😈]({json_url})");

    // Windows CI takes a very variable amount of time to handle this test specifically, and I'm
    // not sure why. Bump up the timeout delay to reduce the amount of spurious failures in as
    // tight of a niche as we can specify
    let mut opts = InterpreterOpts::new();
    let is_ci = env::var("CI").map_or(false, |var| ["true", "1"].contains(&&*var.to_lowercase()));
    let is_windows = cfg!(target_os = "windows");
    if is_ci && is_windows {
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
    const B64_SINGLE_PIXEL_WEBP_000: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICICEREJAA==";
    const B64_SINGLE_PIXEL_WEBP_999: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICICzYyIBA==";
    const B64_SINGLE_PIXEL_WEBP_FFF: &[u8] = b"UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAM1VICIC/Y+IBA==";

    log::init();

    let light_path = "/light.webp";
    let dark_path = "/dark.webp";
    let default_path = "/default.webp";
    let webp_mime = "image/webp";
    let files = [
        (dark_path, B64_SINGLE_PIXEL_WEBP_FFF),
        (light_path, B64_SINGLE_PIXEL_WEBP_000),
        (default_path, B64_SINGLE_PIXEL_WEBP_999),
    ]
    .into_iter()
    .map(|(path, b64_bytes)| {
        let bytes = BASE64_STANDARD.decode(b64_bytes).unwrap();
        server::File::new(path, webp_mime, &bytes)
    })
    .collect();
    let (_server, server_url) = server::mock_file_server(files);
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

#[test]
fn custom_user_agent() {
    log::init();

    let (send_ua, recv_ua) = mpsc::channel();
    let state = server::State::new().send(send_ua);
    let send_ua_server = server::spawn(state, |state, req, _req_url| {
        let maybe_ua = req.headers().iter().find_map(|Header { field, value }| {
            field.equiv("user-agent").then(|| value.as_str().to_owned())
        });
        let _ = state
            .send
            .as_ref()
            .unwrap()
            .send(server::FromServer::UserAgent(maybe_ua));
        let sample_body = Sample::Png(SamplePng::Bun).pre_decode();
        Response::from_data(sample_body).boxed()
    });
    let server_url = send_ua_server.url();

    let text = format!(r"![Show me the UA]({server_url})");
    let _ = interpret_md(&text);

    // TODO: why is this wrapped in an `Option<_>`?
    let server::FromServer::UserAgent(Some(user_agent)) = recv_ua.recv().unwrap() else {
        panic!();
    };
    insta::assert_snapshot!(user_agent, @"inlyne 0.4.1 https://github.com/Inlyne-Project/inlyne");
}
