use std::{
    borrow::BorrowMut,
    collections::hash_map,
    hash::{BuildHasher, Hash, Hasher},
};

use fxhash::{FxHashMap, FxHashSet};
use glyphon::{
    cosmic_text::Align as TextAlign, Affinity, Attrs, AttrsList, BufferLine, Color, Cursor,
    FamilyOwned, FontSystem, Style, SwashCache, TextArea, TextBounds, Weight,
};

use crate::utils::{Align, Line, Point, Rect, Selection, Size};

type KeyHash = u64;
type HashBuilder = twox_hash::RandomXxHashBuilder64;

#[derive(Clone, Debug, Default)]
pub struct TextBox {
    pub indent: f32,
    pub font_size: f32,
    pub texts: Vec<Text>,
    pub is_code_block: bool,
    pub is_quote_block: Option<usize>,
    pub is_checkbox: Option<bool>,
    pub is_anchor: Option<String>,
    pub align: Align,
    pub hidpi_scale: f32,
    pub padding_height: f32,
    pub background_color: Option<[f32; 4]>,
}

pub struct CachedTextArea {
    key: KeyHash,
    left: i32,
    top: i32,
    bounds: TextBounds,
    default_color: Color,
}

impl CachedTextArea {
    pub fn text_area<'a>(&self, cache: &'a TextCache) -> TextArea<'a> {
        TextArea {
            buffer: cache.get(&self.key).expect("Get cached buffer"),
            left: self.left,
            top: self.top,
            bounds: self.bounds,
            default_color: self.default_color,
        }
    }
}

impl TextBox {
    pub fn new(texts: Vec<Text>, hidpi_scale: f32) -> TextBox {
        TextBox {
            texts,
            hidpi_scale,
            font_size: 16.,
            ..Default::default()
        }
    }

    pub fn set_code_block(&mut self, is_code_block: bool) {
        self.is_code_block = is_code_block;
    }

    pub fn set_quote_block(&mut self, nest: Option<usize>) {
        self.is_quote_block = nest;
    }

    pub fn set_checkbox(&mut self, is_checked: Option<bool>) {
        self.is_checkbox = is_checked;
    }

    pub fn set_anchor(&mut self, anchor: Option<String>) {
        self.is_anchor = anchor;
    }

    pub fn set_background_color(&mut self, color: Option<[f32; 4]>) {
        self.background_color = color;
    }

    pub fn with_padding(mut self, padding_height: f32) -> Self {
        self.padding_height = padding_height;
        self
    }

    pub fn set_align(&mut self, align: Align) {
        self.align = align;
    }

    pub fn line_height(&self, zoom: f32) -> f32 {
        self.font_size * 1.1 * self.hidpi_scale * zoom
    }

    pub fn key(&self, bounds: Size, zoom: f32) -> Key<'_> {
        let mut lines = Vec::new();
        let mut sections = Vec::new();
        for (i, text) in self.texts.iter().enumerate() {
            sections.append(&mut text.section_keys(i));
            if text.text.ends_with('\n') {
                lines.push(sections.clone());
                sections.clear();
            }
        }
        if !sections.is_empty() {
            lines.push(sections.clone());
            sections.clear();
        }

        let align = match self.align {
            Align::Left => TextAlign::Left,
            Align::Center => TextAlign::Center,
            Align::Right => TextAlign::Right,
        };

        Key {
            lines,
            size: self.font_size * self.hidpi_scale * zoom,
            line_height: self.line_height(zoom),
            bounds,
            align,
        }
    }

    pub fn find_hoverable<'a>(
        &'a self,
        text_system: &mut TextSystem,
        loc: Point,
        screen_position: Point,
        bounds: Size,
        zoom: f32,
    ) -> Option<&'a Text> {
        if screen_position.1 > loc.1 || screen_position.1 + bounds.1 < loc.1 {
            return None;
        }
        let cache = text_system.text_cache.borrow_mut();

        let (_, buffer) =
            cache.allocate(text_system.font_system.borrow_mut(), self.key(bounds, zoom));

        if let Some(cursor) = buffer.hit(loc.0 - screen_position.0, loc.1 - screen_position.1) {
            let line = &buffer.lines[cursor.line];
            let text = &self.texts[line.attrs_list().get_span(cursor.index).metadata];
            Some(text)
        } else {
            None
        }
    }

    pub fn size(&self, text_system: &mut TextSystem, bounds: Size, zoom: f32) -> Size {
        if self.texts.is_empty() {
            return (0., self.padding_height * self.hidpi_scale * zoom);
        }

        let cache = text_system.text_cache.borrow_mut();

        let line_height = self.line_height(zoom);

        let (_, paragraph) =
            cache.allocate(text_system.font_system.borrow_mut(), self.key(bounds, zoom));

        let (total_lines, max_width) = paragraph
            .layout_runs()
            .enumerate()
            .fold((0, 0.0), |(_, max), (i, buffer)| {
                (i + 1, buffer.line_w.max(max))
            });

        (
            max_width,
            total_lines as f32 * line_height + self.padding_height * self.hidpi_scale * zoom,
        )
    }

    pub fn text_areas(
        &self,
        text_system: &mut TextSystem,
        screen_position: Point,
        bounds: Size,
        zoom: f32,
        scroll_y: f32,
    ) -> CachedTextArea {
        let cache = text_system.text_cache.borrow_mut();

        let (key, _) = cache.allocate(text_system.font_system.borrow_mut(), self.key(bounds, zoom));

        CachedTextArea {
            key,
            left: screen_position.0 as i32,
            top: (screen_position.1 - scroll_y) as i32,
            bounds: TextBounds::default(),
            default_color: Color::rgb(255, 255, 255),
        }
    }

    pub fn render_lines(
        &self,
        text_system: &mut TextSystem,
        screen_position: Point,
        bounds: Size,
        zoom: f32,
    ) -> Vec<Line> {
        let mut has_lines = false;
        for text in &self.texts {
            if text.is_striked || text.is_underlined {
                has_lines = true;
                break;
            }
        }
        if !has_lines {
            return Vec::new();
        }

        let line_height = self.line_height(zoom);
        let mut lines = Vec::new();

        let cache = text_system.text_cache.borrow_mut();

        let (_, buffer) =
            cache.allocate(text_system.font_system.borrow_mut(), self.key(bounds, zoom));

        let mut y = screen_position.1 + line_height;
        for line in buffer.layout_runs() {
            let mut underline_ranges = Vec::new();
            let mut underline_range = None;
            let mut strike_ranges = Vec::new();
            let mut strike_range = None;
            for glyph in line.glyphs {
                let text = &self.texts[glyph.metadata];
                if text.is_underlined {
                    let mut range = underline_range.unwrap_or(glyph.start..glyph.end);
                    range.end = glyph.end;
                    underline_range = Some(range);
                } else if let Some(range) = underline_range.clone() {
                    underline_ranges.push(range);
                }
                if text.is_striked {
                    let mut range = strike_range.unwrap_or(glyph.start..glyph.end);
                    range.end = glyph.end;
                    strike_range = Some(range);
                } else if let Some(range) = strike_range.clone() {
                    strike_ranges.push(range);
                }
            }
            if let Some(range) = underline_range.clone() {
                underline_ranges.push(range);
            }
            if let Some(range) = strike_range.clone() {
                strike_ranges.push(range);
            }
            for underline_range in &underline_ranges {
                let start_cursor = Cursor::new(line.line_i, underline_range.start);
                let end_cursor = Cursor::new(line.line_i, underline_range.end);
                if let Some((highlight_x, highlight_w)) = line.highlight(start_cursor, end_cursor) {
                    let x = screen_position.0 + highlight_x;
                    lines.push(((x.floor(), y), ((x + highlight_w).ceil(), y)));
                }
            }
            for strike_range in &strike_ranges {
                let start_cursor = Cursor::new(line.line_i, strike_range.start);
                let end_cursor = Cursor::new(line.line_i, strike_range.end);
                if let Some((highlight_x, highlight_w)) = line.highlight(start_cursor, end_cursor) {
                    let x = screen_position.0 + highlight_x;
                    let y = y - (line_height / 2.);
                    lines.push(((x.floor(), y), ((x + highlight_w).ceil(), y)));
                }
            }
            y += line_height;
        }

        lines
    }

    pub fn render_selection(
        &self,
        text_system: &mut TextSystem,
        screen_position: Point,
        bounds: Size,
        zoom: f32,
        selection: Selection,
    ) -> (Vec<Rect>, String) {
        let (mut select_start, mut select_end) = selection;
        if select_start.1 > select_end.1 || select_start.0 > select_end.0 {
            std::mem::swap(&mut select_start, &mut select_end);
        }
        if screen_position.1 > select_end.1 || screen_position.1 + bounds.1 < select_start.1 {
            return (vec![], String::new());
        }

        let mut rects = Vec::new();
        let mut selected_text = String::new();

        let line_height = self.line_height(zoom);
        let cache = text_system.text_cache.borrow_mut();

        let (_, buffer) =
            cache.allocate(text_system.font_system.borrow_mut(), self.key(bounds, zoom));

        if let Some(start_cursor) = buffer.hit(
            select_start.0 - screen_position.0,
            select_start.1 - screen_position.1,
        ) {
            if let Some(end_cursor) = buffer.hit(
                select_end.0 - screen_position.0,
                select_end.1 - screen_position.1,
            ) {
                let mut y = screen_position.1;
                for line in buffer.layout_runs() {
                    let line_contains =
                        move |y_point: f32| y_point >= y && y_point <= y + line_height;
                    if line_contains(select_start.1)
                        || line_contains(select_end.1)
                        || (select_start.1 < y && select_end.1 > y + line_height)
                    {
                        if let Some((highlight_x, highlight_w)) =
                            line.highlight(start_cursor, end_cursor)
                        {
                            let x = screen_position.0 + highlight_x;
                            rects.push(Rect::from_min_max(
                                (x.floor(), y),
                                ((x + highlight_w).ceil(), y + line_height),
                            ));
                        }
                    }

                    // See https://docs.rs/cosmic-text/0.8.0/cosmic_text/struct.LayoutRun.html#method.highlight implementation
                    for glyph in line.glyphs.iter() {
                        let left_glyph_cursor = if line.rtl {
                            Cursor::new_with_affinity(line.line_i, glyph.end, Affinity::Before)
                        } else {
                            Cursor::new_with_affinity(line.line_i, glyph.start, Affinity::After)
                        };
                        let right_glyph_cursor = if line.rtl {
                            Cursor::new_with_affinity(line.line_i, glyph.start, Affinity::After)
                        } else {
                            Cursor::new_with_affinity(line.line_i, glyph.end, Affinity::Before)
                        };
                        if (left_glyph_cursor >= start_cursor && left_glyph_cursor <= end_cursor)
                            && (right_glyph_cursor >= start_cursor
                                && right_glyph_cursor <= end_cursor)
                        {
                            selected_text.push_str(&line.text[glyph.start..glyph.end]);
                        }
                    }
                    if select_end.1 > y + line_height {
                        selected_text.push(' ')
                    }
                    y += line_height;
                }
            }
        }

        (rects, selected_text)
    }
}

#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub color: Option<[f32; 4]>,
    pub link: Option<String>,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub is_striked: bool,
    pub font_family: FamilyOwned,
    pub hidpi_scale: f32,
    pub default_color: [f32; 4],
}

impl Text {
    pub fn new(text: String, hidpi_scale: f32, default_text_color: [f32; 4]) -> Self {
        Self {
            text,
            hidpi_scale,
            default_color: default_text_color,
            color: None,
            link: None,
            is_bold: false,
            is_italic: false,
            is_underlined: false,
            is_striked: false,
            font_family: FamilyOwned::SansSerif,
        }
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_link(mut self, link: String) -> Self {
        self.link = Some(link);
        self
    }

    pub fn make_bold(mut self, bold: bool) -> Self {
        self.is_bold = bold;
        self
    }

    pub fn make_italic(mut self, italic: bool) -> Self {
        self.is_italic = italic;
        self
    }

    pub fn make_underlined(mut self, underlined: bool) -> Self {
        self.is_underlined = underlined;
        self
    }

    pub fn make_striked(mut self, striked: bool) -> Self {
        self.is_striked = striked;
        self
    }

    pub fn with_family(mut self, family: FamilyOwned) -> Self {
        self.font_family = family;
        self
    }

    fn color(&self) -> [f32; 4] {
        self.color.unwrap_or(self.default_color)
    }

    fn style(&self) -> Style {
        if self.is_italic {
            Style::Italic
        } else {
            Style::Normal
        }
    }

    fn weight(&self) -> Weight {
        if self.is_bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        }
    }

    pub fn attrs(&self) -> Attrs {
        let color = self.color();
        let attrs = Attrs::new()
            .color(Color::rgba(
                (color[0] * 255.) as u8,
                (color[1] * 255.) as u8,
                (color[2] * 255.) as u8,
                (color[3] * 255.) as u8,
            ))
            .style(self.style())
            .weight(self.weight());
        attrs
    }

    pub fn section_keys(&self, index: usize) -> Vec<SectionKey<'_>> {
        let color = self.color();
        let color = Color::rgba(
            (color[0] * 255.) as u8,
            (color[1] * 255.) as u8,
            (color[2] * 255.) as u8,
            (color[3] * 255.) as u8,
        );
        let font = Font {
            family: self.font_family.as_family(),
            weight: self.weight(),
        };
        self.text
            .lines()
            .map(|line| SectionKey {
                content: line,
                font,
                color,
                index,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, Hash)]
struct Font<'a> {
    family: glyphon::Family<'a>,
    weight: glyphon::Weight,
}

#[derive(Clone, Copy, Hash)]
pub struct SectionKey<'a> {
    content: &'a str,
    font: Font<'a>,
    color: Color,
    index: usize,
}

#[derive(Clone)]
pub struct Key<'a> {
    lines: Vec<Vec<SectionKey<'a>>>,
    size: f32,
    line_height: f32,
    bounds: Size,
    align: TextAlign,
}

#[derive(Default)]
pub struct TextCache {
    entries: FxHashMap<KeyHash, glyphon::Buffer>,
    recently_used: FxHashSet<KeyHash>,
    hasher: HashBuilder,
}

impl TextCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &KeyHash) -> Option<&glyphon::Buffer> {
        self.entries.get(key)
    }

    fn allocate(
        &mut self,
        font_system: &mut glyphon::FontSystem,
        key: Key<'_>,
    ) -> (KeyHash, &mut glyphon::Buffer) {
        let hash = {
            let mut hasher = self.hasher.build_hasher();

            key.lines.hash(&mut hasher);
            key.size.to_bits().hash(&mut hasher);
            key.line_height.to_bits().hash(&mut hasher);
            key.bounds.0.to_bits().hash(&mut hasher);
            key.bounds.1.to_bits().hash(&mut hasher);

            hasher.finish()
        };

        if let hash_map::Entry::Vacant(entry) = self.entries.entry(hash) {
            let metrics = glyphon::Metrics::new(key.size, key.line_height);
            let mut buffer = glyphon::Buffer::new(font_system, metrics);

            buffer.set_size(font_system, key.bounds.0, key.bounds.1.max(key.line_height));

            buffer.lines.clear();

            for line in key.lines {
                let mut line_str = String::new();
                let mut attrs_list = AttrsList::new(Attrs::new());
                for section in line {
                    let start = line_str.len();
                    line_str.push_str(section.content);
                    let end = line_str.len();
                    attrs_list.add_span(
                        start..end,
                        Attrs::new()
                            .family(section.font.family)
                            .weight(section.font.weight)
                            .color(section.color)
                            .metadata(section.index),
                    )
                }
                let mut buffer_line = BufferLine::new(line_str, attrs_list);
                buffer_line.set_align(Some(key.align));
                buffer.lines.push(buffer_line);
            }

            buffer.shape_until_scroll(font_system);

            let _ = entry.insert(buffer);
        }

        let _ = self.recently_used.insert(hash);

        (hash, self.entries.get_mut(&hash).unwrap())
    }

    pub fn trim(&mut self) {
        self.entries
            .retain(|key, _| self.recently_used.contains(key));

        self.recently_used.clear();
    }
}

pub struct TextSystem {
    pub font_system: FontSystem,
    pub text_renderer: glyphon::TextRenderer,
    pub text_atlas: glyphon::TextAtlas,
    pub text_cache: TextCache,
    pub swash_cache: SwashCache,
}
