use crate::renderer::{Align, Rect};
use wgpu_glyph::{ab_glyph::Font, FontId, GlyphCruncher, HorizontalAlign, Layout, Section};
use winit::window::CursorIcon;

pub const DEFAULT_TEXT_COLOR: [f32; 4] = [0.5840785, 0.63759696, 0.6938719, 1.0];

#[derive(Clone, Debug)]
pub struct TextBox {
    pub indent: f32,
    pub texts: Vec<Text>,
    pub is_code_block: bool,
    pub align: Align,
}

impl TextBox {
    pub fn new(texts: Vec<Text>) -> TextBox {
        TextBox {
            indent: 0.0,
            texts,
            is_code_block: false,
            align: Align::Left,
        }
    }

    pub fn set_code_block(&mut self, is_code_block: bool) {
        self.is_code_block = is_code_block;
    }

    pub fn make_code_block(mut self, is_code_block: bool) -> Self {
        self.is_code_block = is_code_block;
        self
    }

    pub fn with_indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    pub fn with_align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn set_align(&mut self, align: Align) {
        self.align = align;
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
        for glyph in glyph_brush.glyphs(self.glyph_section(
            screen_position,
            bounds,
            hidpi_scale,
            [0., 0., 0., 0.],
        )) {
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
        for glyph in glyph_brush.glyphs(self.glyph_section(
            screen_position,
            bounds,
            hidpi_scale,
            [0., 0., 0., 0.],
        )) {
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
        if let Some(bounds) = glyph_brush.glyph_bounds(self.glyph_section(
            screen_position,
            bounds,
            hidpi_scale,
            [0., 0., 0., 0.],
        )) {
            (bounds.width(), bounds.height())
        } else {
            (0., 0.)
        }
    }

    pub fn glyph_section(
        &self,
        mut screen_position: (f32, f32),
        bounds: (f32, f32),
        hidpi_scale: f32,
        default_color: [f32; 4],
    ) -> Section {
        let texts = self
            .texts
            .iter()
            .map(|t| t.glyph_text(hidpi_scale, default_color))
            .collect();

        let horizontal_align = match self.align {
            Align::Center => {
                screen_position = (screen_position.0 + bounds.0 / 2., screen_position.1);
                HorizontalAlign::Center
            }
            Align::Left => HorizontalAlign::Left,
            Align::Right => {
                screen_position = (bounds.0 + screen_position.0, screen_position.1);
                HorizontalAlign::Right
            }
            Align::Justify => HorizontalAlign::Center,
        };
        Section {
            screen_position,
            bounds,
            text: texts,
            ..wgpu_glyph::Section::default()
                .with_layout(Layout::default().h_align(horizontal_align))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub size: f32,
    pub color: Option<[f32; 4]>,
    pub link: Option<String>,
    pub is_bold: bool,
    pub font: usize,
}

impl Text {
    pub fn new(text: String) -> Self {
        Self {
            text,
            size: 16.,
            color: None,
            link: None,
            is_bold: false,
            font: 0,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
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

    pub fn with_font(mut self, font_index: usize) -> Self {
        self.font = font_index;
        self
    }

    fn glyph_text(&self, hidpi_scale: f32, default_color: [f32; 4]) -> wgpu_glyph::Text {
        let font = if self.is_bold {
            FontId(self.font * 2 + 1)
        } else {
            FontId(self.font * 2)
        };
        wgpu_glyph::Text::new(self.text.as_str())
            .with_scale(self.size * hidpi_scale)
            .with_color(self.color.unwrap_or(default_color))
            .with_font_id(font)
    }
}
