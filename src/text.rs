use crate::{
    color::Theme,
    utils::{Align, Rect},
};
use wgpu_glyph::{
    ab_glyph::{Font, FontArc, PxScale},
    Extra, FontId, GlyphCruncher, HorizontalAlign, Layout, Section,
};
use winit::window::CursorIcon;

pub const DEFAULT_TEXT_COLOR: [f32; 4] = [0.5840785, 0.63759696, 0.6938719, 1.0];

#[derive(Clone, Debug)]
pub struct TextBox {
    pub indent: f32,
    pub texts: Vec<Text>,
    pub is_code_block: bool,
    pub align: Align,
    pub hidpi_scale: f32,
    pub default_text_color: [f32; 4],
}

impl TextBox {
    pub fn new(texts: Vec<Text>, hidpi_scale: f32, theme: &Theme) -> TextBox {
        TextBox {
            indent: 0.0,
            texts,
            is_code_block: false,
            align: Align::Left,
            hidpi_scale,
            default_text_color: theme.text_color.clone(),
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
        click: bool,
    ) -> CursorIcon {
        let fonts: Vec<FontArc> = glyph_brush.fonts().iter().map(|f| f.clone()).collect();
        for glyph in glyph_brush.glyphs(&self.glyph_section(screen_position, bounds)) {
            let bounds = Rect::from((fonts[glyph.font_id.0]).glyph_bounds(&glyph.glyph));
            if bounds.contains(loc) {
                let text = &self.texts[glyph.section_index];
                let cursor = if let Some(ref link) = text.link {
                    if click {
                        open::that(link).unwrap();
                    }
                    CursorIcon::Hand
                } else {
                    CursorIcon::Text
                };
                return cursor;
            }
        }
        CursorIcon::Default
    }

    pub fn size<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
    ) -> (f32, f32) {
        if self.texts.is_empty() {
            return (0., 0.);
        }
        if let Some(bounds) = glyph_brush.glyph_bounds(&self.glyph_section(screen_position, bounds))
        {
            (bounds.width(), bounds.height())
        } else {
            (0., 0.)
        }
    }

    pub fn glyph_section<'a>(
        &'a self,
        mut screen_position: (f32, f32),
        bounds: (f32, f32),
    ) -> Section {
        let texts = self.texts.iter().map(|t| t.wgpu_text()).collect();

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
    pub is_italic: bool,
    pub font: usize,
    pub hidpi_scale: f32,
    pub default_color: [f32; 4],
}

impl Text {
    pub fn new(text: String, hidpi_scale: f32, default_text_color: [f32; 4]) -> Self {
        Self {
            text,
            size: 16.,
            color: None,
            link: None,
            is_bold: false,
            is_italic: false,
            font: 0,
            hidpi_scale,
            default_color: default_text_color,
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

    pub fn make_italic(mut self, italic: bool) -> Self {
        self.is_italic = italic;
        self
    }

    pub fn with_font(mut self, font_index: usize) -> Self {
        self.font = font_index;
        self
    }

    fn font_id(&self) -> FontId {
        let base = self.font * 4;
        let font = if self.is_bold {
            if self.is_italic {
                base + 3
            } else {
                base + 2
            }
        } else {
            if self.is_italic {
                base + 1
            } else {
                base
            }
        };
        FontId(font)
    }

    fn color(&self) -> [f32; 4] {
        if let Some(color) = self.color {
            color
        } else {
            self.default_color
        }
    }

    pub fn wgpu_text(&self) -> wgpu_glyph::Text {
        wgpu_glyph::Text {
            text: &self.text,
            scale: PxScale::from(self.size * self.hidpi_scale),
            font_id: self.font_id(),
            extra: Extra {
                color: self.color(),
                z: 0.0,
            },
        }
    }
}
