use std::borrow::BorrowMut;
use std::collections::hash_map;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use fxhash::{FxHashMap, FxHashSet};
use glyphon::{
    Affinity, Attrs, AttrsList, BufferLine, Color, Cursor, FamilyOwned, FontSystem, LayoutGlyph,
    Shaping, Style, SwashCache, TextArea, TextBounds, Weight,
};
use smart_debug::SmartDebug;
use taffy::prelude::{AvailableSpace, Size as TaffySize};

use crate::debug_impls::{self, DebugInline, DebugInlineMaybeF32Color};
use crate::selection::{Selection, SelectionKind, SelectionMode};
use crate::utils::{Align, Line, Point, Rect, Size};

type KeyHash = u64;
type HashBuilder = twox_hash::RandomXxHashBuilder64;

pub struct TextBoxMeasure {
    pub textbox: Arc<TextBox>,
    pub text_cache: Arc<Mutex<TextCache>>,
    pub font_system: Arc<Mutex<FontSystem>>,
    pub zoom: f32,
}

impl TextBoxMeasure {
    fn internal_measure(&self, bounds: (f32, f32)) -> (f32, f32) {
        self.textbox
            .size_without_system(&self.text_cache, &self.font_system, bounds, self.zoom)
    }

    pub fn measure(
        &self,
        known_dimensions: TaffySize<Option<f32>>,
        available_space: TaffySize<taffy::style::AvailableSpace>,
    ) -> TaffySize<f32> {
        let available_width = match available_space.width {
            AvailableSpace::Definite(space) => space,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::MAX,
        };
        let width_bound = known_dimensions.width.unwrap_or(available_width);

        let size = self.internal_measure((width_bound, f32::MAX));
        TaffySize {
            width: known_dimensions.width.unwrap_or(size.0),
            height: known_dimensions.height.unwrap_or(size.1),
        }
    }
}

#[derive(SmartDebug, Clone)]
#[debug(skip_defaults)]
pub struct TextBox {
    pub font_size: f32,
    pub align: Align,
    pub indent: f32,
    pub padding_height: f32,
    #[debug(wrapper = DebugInlineMaybeF32Color)]
    pub background_color: Option<[f32; 4]>,
    pub is_code_block: bool,
    #[debug(wrapper = DebugInline)]
    pub is_quote_block: Option<usize>,
    #[debug(wrapper = DebugInline)]
    pub is_checkbox: Option<bool>,
    #[debug(wrapper = DebugInline)]
    pub is_anchor: Option<String>,
    #[debug(no_skip)]
    pub texts: Vec<Text>,
    #[debug(skip)]
    pub hidpi_scale: f32,
}

impl Default for TextBox {
    fn default() -> Self {
        Self {
            indent: 0.0,
            font_size: 16.0,
            texts: Vec::new(),
            is_code_block: false,
            is_quote_block: None,
            is_checkbox: None,
            is_anchor: None,
            align: Align::default(),
            hidpi_scale: 1.0,
            padding_height: 0.0,
            background_color: None,
        }
    }
}

#[derive(Clone)]
pub struct CachedTextArea {
    key: KeyHash,
    left: f32,
    top: f32,
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
            scale: 1.,
        }
    }
}

impl TextBox {
    pub fn new(texts: Vec<Text>, hidpi_scale: f32) -> TextBox {
        TextBox {
            texts,
            hidpi_scale,
            ..Default::default()
        }
    }

    pub fn set_code_block(&mut self, is_code_block: bool) {
        self.is_code_block = is_code_block;
    }

    pub fn set_quote_block(&mut self, nest: usize) {
        self.is_quote_block = Some(nest);
    }

    pub fn clear_quote_block(&mut self) {
        self.is_quote_block = None;
    }

    pub fn set_checkbox(&mut self, is_checked: Option<bool>) {
        self.is_checkbox = is_checked;
    }
    pub fn set_anchor(&mut self, anchor: String) {
        self.is_anchor = Some(anchor);
    }

    pub fn set_background_color(&mut self, color: [f32; 4]) {
        self.background_color = Some(color);
    }

    pub fn with_padding(mut self, padding_height: f32) -> Self {
        self.padding_height = padding_height;
        self
    }

    pub fn set_align(&mut self, align: Align) {
        self.align = align;
    }

    pub fn set_align_or_default(&mut self, maybe_align: Option<Align>) {
        self.set_align(maybe_align.unwrap_or_default());
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

        Key {
            lines,
            size: self.font_size * self.hidpi_scale * zoom,
            line_height: self.line_height(zoom),
            bounds,
        }
    }

    /// Returns the [`Text`] in the given [`TextSystem`] with the cursor over it, if any.
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

        let mut cache = text_system.text_cache.lock().unwrap();

        let (_, buffer) = cache.allocate(
            text_system.font_system.lock().unwrap().borrow_mut(),
            self.key(bounds, zoom),
        );

        if let Some(cursor) = buffer.hit(loc.0 - screen_position.0, loc.1 - screen_position.1) {
            let line = &buffer.lines[cursor.line];
            let mut index = cursor.index;
            if cursor.affinity == Affinity::Before {
                index = index.saturating_sub(1);
            }
            let text = &self.texts[line.attrs_list().get_span(index).metadata];
            Some(text)
        } else {
            None
        }
    }

    pub fn size(&self, text_system: &mut TextSystem, bounds: Size, zoom: f32) -> Size {
        self.size_without_system(
            &text_system.text_cache,
            &text_system.font_system,
            bounds,
            zoom,
        )
    }

    pub fn size_without_system(
        &self,
        text_cache: &Mutex<TextCache>,
        font_system: &Mutex<FontSystem>,
        bounds: Size,
        zoom: f32,
    ) -> Size {
        if self.texts.is_empty() {
            return (0., self.padding_height * self.hidpi_scale * zoom);
        }

        let mut cache = text_cache.lock().unwrap();

        let line_height = self.line_height(zoom);

        let (_, paragraph) = cache.allocate(
            font_system.lock().unwrap().borrow_mut(),
            self.key(bounds, zoom),
        );

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

        let (key, max_width) = {
            let mut cache = cache.lock().unwrap();
            let (key, paragraph) = cache.allocate(
                text_system.font_system.lock().unwrap().borrow_mut(),
                self.key(bounds, zoom),
            );

            let max_width = paragraph
                .layout_runs()
                .fold(0., |max, buffer| buffer.line_w.max(max));
            (key, max_width)
        };

        let left = match self.align {
            Align::Left => screen_position.0,
            Align::Center => screen_position.0 + (bounds.0 - max_width) / 2.,
            Align::Right => screen_position.0 + bounds.0 - max_width,
        };

        CachedTextArea {
            key,
            left,
            top: (screen_position.1 - scroll_y),
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
        text_area: &CachedTextArea,
    ) -> Vec<Line> {
        fn push_line_segment(
            lines: &mut Vec<ThinLine>,
            current_line: Option<ThinLine>,
            glyph: &LayoutGlyph,
            color: [f32; 4],
        ) -> ThinLine {
            let range = if let Some(current) = current_line {
                if current.color == color {
                    let mut range = current.range;
                    range.end = glyph.end;
                    range
                } else {
                    lines.push(current);
                    glyph.start..glyph.end
                }
            } else {
                glyph.start..glyph.end
            };
            ThinLine { range, color }
        }

        let has_lines = self
            .texts
            .iter()
            .any(|text| text.is_striked || text.is_underlined);
        if !has_lines {
            return Vec::new();
        }

        let line_height = self.line_height(zoom);
        let mut lines = Vec::new();

        let mut cache = text_system.text_cache.lock().unwrap();

        let (_, buffer) = cache.allocate(
            text_system.font_system.lock().unwrap().borrow_mut(),
            self.key(bounds, zoom),
        );

        let mut y = screen_position.1 + line_height;
        for line in buffer.layout_runs() {
            let mut underlines = Vec::new();
            let mut current_underline: Option<ThinLine> = None;
            let mut strikes = Vec::new();
            let mut current_strike: Option<ThinLine> = None;
            // Goes over glyphs and finds the underlines and strikethroughs. The current
            // underline/strikethrough is combined with matching consecutive lines
            for glyph in line.glyphs {
                let text = &self.texts[glyph.metadata];
                let color = text.color.unwrap_or(text.default_color);
                if text.is_underlined {
                    let underline =
                        push_line_segment(&mut underlines, current_underline, glyph, color);
                    current_underline = Some(underline);
                } else if let Some(current) = current_underline.clone() {
                    underlines.push(current);
                }
                if text.is_striked {
                    let strike = push_line_segment(&mut strikes, current_strike, glyph, color);
                    current_strike = Some(strike);
                } else if let Some(current) = current_strike.clone() {
                    strikes.push(current);
                }
            }
            if let Some(current) = current_underline.take() {
                underlines.push(current);
            }
            if let Some(current) = current_strike.take() {
                strikes.push(current);
            }
            for ThinLine { range, color } in &underlines {
                let start_cursor = Cursor::new(line.line_i, range.start);
                let end_cursor = Cursor::new(line.line_i, range.end);
                if let Some((highlight_x, highlight_w)) = line.highlight(start_cursor, end_cursor) {
                    let x = text_area.left + highlight_x;
                    let min = (x.floor(), y);
                    let max = ((x + highlight_w).ceil(), y);
                    let line = Line::with_color(min, max, *color);
                    lines.push(line);
                }
            }
            for ThinLine { range, color } in &strikes {
                let start_cursor = Cursor::new(line.line_i, range.start);
                let end_cursor = Cursor::new(line.line_i, range.end);
                if let Some((highlight_x, highlight_w)) = line.highlight(start_cursor, end_cursor) {
                    let x = screen_position.0 + highlight_x;
                    let y = y - (line_height / 2.);
                    let min = (x.floor(), y);
                    let max = ((x + highlight_w).ceil(), y);
                    let line = Line::with_color(min, max, *color);
                    lines.push(line);
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
        selection: &mut Selection,
    ) -> Option<Vec<Rect>> {
        let mut rects = Vec::new();
        let mut selected_text = String::new();

        let line_height = self.line_height(zoom);
        let mut cache = text_system.text_cache.lock().unwrap();

        let (_, buffer) = cache.allocate(
            text_system.font_system.lock().unwrap().borrow_mut(),
            self.key(bounds, zoom),
        );

        let (start_cursor, end_cursor, start_y, end_y) = match &selection.selection {
            SelectionKind::Drag { mut start, mut end } => {
                if start.1 > end.1 || start.0 > end.0 {
                    std::mem::swap(&mut start, &mut end);
                }
                if screen_position.1 > end.1 || screen_position.1 + bounds.1 < start.1 {
                    return None;
                }

                let start_cursor =
                    buffer.hit(start.0 - screen_position.0, start.1 - screen_position.1)?;
                let end_cursor =
                    buffer.hit(end.0 - screen_position.0, end.1 - screen_position.1)?;
                (start_cursor, end_cursor, start.1, end.1)
            }
            SelectionKind::Click { mode, position, .. } => {
                let mut cursor = buffer.hit(
                    position.0 - screen_position.0,
                    position.1 - screen_position.1,
                )?;

                let line = buffer.lines.get(cursor.line)?;

                match mode {
                    SelectionMode::Word => {
                        let text = line.text();

                        let mut start_index = None;
                        let mut end_index = None;

                        match cursor.affinity {
                            Affinity::Before => {
                                if cursor.index == 0 {
                                    return None;
                                }
                                if text
                                    .get(cursor.index - 1..cursor.index)?
                                    .contains(|c: char| c.is_whitespace())
                                {
                                    cursor.index += 1;
                                } else if cursor.index == text.len()
                                    || text
                                        .get(cursor.index..cursor.index + 1)?
                                        .contains(|c: char| c.is_whitespace())
                                {
                                    end_index = Some(cursor.index);
                                }
                            }
                            Affinity::After => {
                                if text
                                    .get(cursor.index..cursor.index + 1)?
                                    .contains(|c: char| c.is_whitespace())
                                {
                                    cursor.index -= 1;
                                } else if cursor.index == 0
                                    || text
                                        .get(cursor.index - 1..cursor.index)?
                                        .contains(|c: char| c.is_whitespace())
                                {
                                    start_index = Some(cursor.index)
                                }
                            }
                        }

                        if end_index.is_none() {
                            let end_text = text
                                .get(cursor.index..)
                                .and_then(|str| str.split_whitespace().next())?;
                            end_index = Some(end_text.len() + cursor.index);
                        }
                        if start_index.is_none() {
                            let start_text = text
                                .get(..cursor.index)
                                .and_then(|str| str.split_whitespace().next_back())?;
                            start_index = Some(cursor.index - start_text.len());
                        }

                        let start =
                            Cursor::new(cursor.line, start_index.expect("Should have an value"));
                        let end =
                            Cursor::new(cursor.line, end_index.expect("Should have an value"));

                        (start, end, position.1, position.1)
                    }
                    SelectionMode::Line => {
                        let start = Cursor::new(cursor.line, 0);
                        let end = Cursor::new(cursor.line, line.text().len());
                        (start, end, position.1, position.1)
                    }
                }
            }
            _ => {
                return None;
            }
        };

        let mut y = screen_position.1;
        for line in buffer.layout_runs() {
            let line_contains = move |y_point: f32| y_point >= y && y_point <= y + line_height;
            if line_contains(start_y)
                || line_contains(end_y)
                || (start_y < y && end_y > y + line_height)
            {
                if let Some((highlight_x, highlight_w)) = line.highlight(start_cursor, end_cursor) {
                    let x = screen_position.0 + highlight_x;
                    rects.push(Rect::from_min_max(
                        (x.floor(), y),
                        ((x + highlight_w).ceil(), y + line_height),
                    ));
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
                        && (right_glyph_cursor >= start_cursor && right_glyph_cursor <= end_cursor)
                    {
                        selected_text.push_str(&line.text[glyph.start..glyph.end]);
                    }
                }
                if end_y > y + line_height {
                    selected_text.push(' ')
                }
            }
            y += line_height;
        }

        selection.add_line(&selected_text);

        Some(rects)
    }
}

#[derive(Clone)]
struct ThinLine {
    range: Range<usize>,
    color: [f32; 4],
}

#[derive(Clone)]
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

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_impls::text(self, f)
    }
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
            style: self.style(),
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
    style: glyphon::Style,
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
                            .style(section.font.style)
                            .color(section.color)
                            .metadata(section.index),
                    )
                }
                let buffer_line = BufferLine::new(line_str, attrs_list, Shaping::Advanced);
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
    pub font_system: Arc<Mutex<FontSystem>>,
    pub text_renderer: glyphon::TextRenderer,
    pub text_atlas: glyphon::TextAtlas,
    pub text_cache: Arc<Mutex<TextCache>>,
    pub swash_cache: SwashCache,
}
