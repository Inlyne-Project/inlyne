use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};

use comrak::{
    adapters::SyntaxHighlighterAdapter,
    markdown_to_html_with_plugins,
    plugins::syntect::{SyntectAdapter, SyntectAdapterBuilder},
    ComrakOptions,
};
use indexmap::IndexMap;
use serde::Deserialize;
use syntect::highlighting::{Theme as SyntectTheme, ThemeSet as SyntectThemeSet};
use winit::window::CursorIcon;

use crate::image::ImageData;

pub(crate) fn default<T: Default>() -> T {
    Default::default()
}

pub fn usize_in_mib(num: usize) -> f32 {
    num as f32 / 1_024.0 / 1_024.0
}

pub type Line = ((f32, f32), (f32, f32));
pub type Selection = ((f32, f32), (f32, f32));
pub type Point = (f32, f32);
pub type Size = (f32, f32);
pub type ImageCache = Arc<Mutex<HashMap<String, Arc<Mutex<Option<ImageData>>>>>>;

#[derive(Debug, Clone)]
pub struct Rect {
    pub pos: Point,
    pub size: Point,
}

impl Rect {
    pub fn new(pos: Point, size: Point) -> Rect {
        Rect { pos, size }
    }

    pub fn from_min_max(min: Point, max: Point) -> Rect {
        Rect {
            pos: min,
            size: (max.0 - min.0, max.1 - min.1),
        }
    }

    pub fn max(&self) -> Point {
        (self.pos.0 + self.size.0, self.pos.1 + self.size.1)
    }

    pub fn contains(&self, loc: Point) -> bool {
        self.pos.0 <= loc.0 && loc.0 <= self.max().0 && self.pos.1 <= loc.1 && loc.1 <= self.max().1
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    pub fn new(s: &str) -> Option<Self> {
        let align = match s {
            "left" => Self::Left,
            "center" => Self::Center,
            "right" => Self::Right,
            _ => return None,
        };

        Some(align)
    }
}

#[derive(Default)]
pub struct HoverInfo {
    pub cursor_icon: CursorIcon,
    pub jump: Option<f32>,
}

impl From<CursorIcon> for HoverInfo {
    fn from(cursor_icon: CursorIcon) -> Self {
        Self {
            cursor_icon,
            ..Default::default()
        }
    }
}

// TODO(cosmic): Remove after `comrak` supports code block info strings that have a comma
//     (like ```rust,ignore)
//     https://github.com/kivikakk/comrak/issues/246
struct CustomSyntectAdapter(SyntectAdapter);

impl SyntaxHighlighterAdapter for CustomSyntectAdapter {
    fn write_highlighted(
        &self,
        output: &mut dyn io::Write,
        lang: Option<&str>,
        code: &str,
    ) -> io::Result<()> {
        let norm_lang = lang.map(|l| l.split_once(',').map(|(lang, _)| lang).unwrap_or(l));
        self.0.write_highlighted(output, norm_lang, code)
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn io::Write,
        attributes: HashMap<String, String>,
    ) -> io::Result<()> {
        self.0.write_pre_tag(output, attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn io::Write,
        attributes: HashMap<String, String>,
    ) -> io::Result<()> {
        self.0.write_code_tag(output, attributes)
    }
}

pub fn markdown_to_html(md: &str, syntax_theme: SyntectTheme) -> String {
    let mut options = ComrakOptions::default();
    options.extension.autolink = true;
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    options.extension.shortcodes = true;
    options.parse.smart = true;
    options.render.unsafe_ = true;

    // TODO(cosmic): gonna send a PR so that a plugin can pass in a single theme too
    let dummy_name = "theme";
    let mut theme_set = SyntectThemeSet::new();
    theme_set
        .themes
        .insert(String::from(dummy_name), syntax_theme);
    let syn_set = two_face::syntax::extra_no_newlines();
    let adapter = SyntectAdapterBuilder::new()
        .syntax_set(syn_set)
        .theme_set(theme_set)
        .theme(dummy_name)
        .build();

    let mut plugins = comrak::ComrakPlugins::default();
    let custom = CustomSyntectAdapter(adapter);
    plugins.render.codefence_syntax_highlighter = Some(&custom);

    let htmlified = markdown_to_html_with_plugins(md, &options, &plugins);

    // Comrak doesn't support converting the front matter to HTML, so we have to convert it to an
    // HTML table ourselves. Front matter is found like so
    // ---
    // {YAML value}
    // ---
    // {Markdown}
    let html_front_matter = if md.starts_with("---") {
        let mut parts = md.split("---");
        let _ = parts.next();
        parts
            .next()
            .and_then(
                |front_matter| match serde_yaml::from_str::<FrontMatter>(front_matter) {
                    Ok(front_matter) => Some(front_matter.to_table()),
                    Err(err) => {
                        log::warn!(
                            "Failed parsing front matter. Error: {}\n{}",
                            err,
                            front_matter
                        );
                        None
                    }
                },
            )
            .unwrap_or_default()
    } else {
        String::new()
    };

    format!("{}{}", html_front_matter, htmlified)
}

#[derive(Deserialize, Debug)]
struct FrontMatter(IndexMap<String, Cell>);

impl FrontMatter {
    fn to_table(&self) -> String {
        let mut table = String::from("<table>\n");

        table.push_str("<thead>\n<tr>\n");
        for key in self.0.keys() {
            table.push_str("<th align=\"center\">");
            html_escape::encode_safe_to_string(key, &mut table);
            table.push_str("</th>\n");
        }
        table.push_str("</tr>\n</thead>\n");

        table.push_str("<tbody>\n<tr>\n");
        for cell in self.0.values() {
            table.push_str("<td align=\"center\">");
            cell.render_into(&mut table);
            table.push_str("</td>\n");
        }
        table.push_str("</tr>\n</tbody>\n");

        table.push_str("</table>\n");
        table
    }
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Cell {
    Str(String),
    Table(Vec<String>),
}

impl Cell {
    fn render_into(&self, buf: &mut String) {
        match self {
            Self::Str(s) => {
                html_escape::encode_safe_to_string(s, buf);
            }
            Self::Table(_v) => {
                log::warn!("Nested tables aren't supported yet. Skipping");
                buf.push_str("{Skipped nested table}");
            }
        }
    }
}
