use crate::renderer::Rect;
use wgpu_glyph::{ab_glyph::Font, FontId, GlyphCruncher, Section};
use winit::window::CursorIcon;

pub const DEFAULT_TEXT_COLOR: [f32; 4] = [0.5840785,
0.63759696,
0.6938719,
1.0];

#[derive(Clone, Debug)]
pub struct TextBox {
    pub indent: f32,
    pub texts: Vec<Text>,
}

impl TextBox {
    pub fn new(texts: Vec<Text>) -> TextBox {
        TextBox { indent: 0.0, texts }
    }

    pub fn with_indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    pub fn hovering_over<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        loc: (f32, f32),
        screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
    ) -> CursorIcon {
        let font = &glyph_brush.fonts()[0].clone();
        for glyph in glyph_brush.glyphs(self.glyph_section(screen_position, bounds, hidpi_scale)) {
            let bounds = font.glyph_bounds(&glyph.glyph);
            let bounds =
                Rect::from_min_max((bounds.min.x, bounds.min.y), (bounds.max.x, bounds.max.y));
            if bounds.contains(loc) {
                let text = &self.texts[glyph.section_index];
                let cursor = if text.link.is_some() {
                    CursorIcon::Hand
                } else {
                    CursorIcon::Text
                };
                return cursor;
            }
        }
        CursorIcon::Default
    }

    pub fn click<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        loc: (f32, f32),
        screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
    ) {
        let font = &glyph_brush.fonts()[0].clone();
        for glyph in glyph_brush.glyphs(self.glyph_section(screen_position, bounds, hidpi_scale)) {
            let bounds = font.glyph_bounds(&glyph.glyph);
            let bounds =
                Rect::from_min_max((bounds.min.x, bounds.min.y), (bounds.max.x, bounds.max.y));
            if bounds.contains(loc) {
                let text = &self.texts[glyph.section_index];
                if let Some(ref link) = text.link {
                    open::that(link).unwrap()
                }
            }
        }
    }

    pub fn size<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
    ) -> (f32, f32) {
        if self.texts.is_empty() {
            return (0., 0.);
        }
        if let Some(bounds) =
            glyph_brush.glyph_bounds(self.glyph_section(screen_position, bounds, hidpi_scale))
        {
            (bounds.width(), bounds.height())
        } else {
            (0., 0.)
        }
    }

    pub fn glyph_section(
        &self,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
    ) -> Section {
        let texts = self
            .texts
            .iter()
            .map(|t| t.glyph_text(hidpi_scale))
            .collect();
        Section {
            screen_position,
            bounds,
            text: texts,
            ..wgpu_glyph::Section::default()
        }
    }
    
    pub fn code_rects<T: GlyphCruncher>(&self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
    ) -> Vec<Rect> {
        dbg!(&self.texts);
        let mut code_bounds = Vec::new();
        let font = &glyph_brush.fonts()[0].clone();
        let mut glyph_iter = glyph_brush.glyphs(self.glyph_section(screen_position, bounds, hidpi_scale));
        let first_glyph = if let Some(first_glyph) = glyph_iter.next() {
            first_glyph
        } else {
            return Vec::new()
        };
        let first_bounds = font.glyph_bounds(&first_glyph.glyph);
        let mut current_section = first_glyph.section_index;
        let mut is_code = self.texts[current_section].is_code;
        let (mut min, mut max) = (first_bounds.min, first_bounds.max);
        for glyph in  glyph_iter {
            let bounds = font.glyph_bounds(&glyph.glyph);
            if is_code != self.texts[glyph.section_index].is_code {
                if self.texts[current_section].is_code {
                    code_bounds.push(Rect::from_min_max((min.x, min.y), (max.x, max.y)));
                }
                current_section = glyph.section_index;
                is_code = self.texts[glyph.section_index].is_code;
                min = bounds.min;
                max = bounds.max;
            } else if self.texts[glyph.section_index].is_code {
                min.x = min.x.min(bounds.min.x);
                min.y = min.y.min(bounds.min.y);
                max.x = max.x.max(bounds.max.x);
                max.y = max.y.max(bounds.max.y);
            }
        }
        if self.texts[current_section].is_code {
            code_bounds.push(Rect::from_min_max((min.x, min.y), (max.x, max.y)));
        }
        code_bounds
    }
}

#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub size: f32,
    pub color: [f32; 4],
    pub link: Option<String>,
    pub is_code: bool,
    pub is_bold: bool,
    pub font: usize,
}

impl Text {
    pub fn new(text: String) -> Self {
        Self {
            text,
            size: 16.,
            color: DEFAULT_TEXT_COLOR,
            link: None,
            is_bold: false,
            font: 0,
            is_code: false
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
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

    pub fn make_code(mut self, code: bool) -> Self {
        self.is_code = code;
        self
    }

    pub fn with_font(mut self, font_index: usize) -> Self {
        self.font = font_index;
        self
    }

    fn glyph_text(&self, hidpi_scale: f32) -> wgpu_glyph::Text {
        let font = if self.is_bold {
            FontId(self.font * 2 + 1)
        } else {
            FontId(self.font * 2)
        };
        wgpu_glyph::Text::new(self.text.as_str())
            .with_scale(self.size * hidpi_scale)
            .with_color(self.color)
            .with_font_id(font)
    }
}
