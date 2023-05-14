use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use comrak::{markdown_to_html_with_plugins, ComrakOptions};
use wgpu_glyph::ab_glyph;
use winit::window::CursorIcon;

use crate::{color::SyntaxTheme, image::ImageData};

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

impl From<ab_glyph::Rect> for Rect {
    fn from(other_rect: ab_glyph::Rect) -> Self {
        let ab_glyph::Rect {
            min: ab_glyph::Point { x: min_x, y: min_y },
            max: ab_glyph::Point { x: max_x, y: max_y },
        } = other_rect;
        Self::from_min_max((min_x, min_y), (max_x, max_y))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
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

pub fn markdown_to_html(md: &str, syntax_theme: SyntaxTheme) -> String {
    let mut options = ComrakOptions::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    options.parse.smart = true;
    options.render.unsafe_ = true;

    let mut plugins = comrak::ComrakPlugins::default();
    let adapter = comrak::plugins::syntect::SyntectAdapter::new(syntax_theme.as_syntect_name());
    plugins.render.codefence_syntax_highlighter = Some(&adapter);

    let htmlified = markdown_to_html_with_plugins(&md, &options, &plugins);

    htmlified
}
