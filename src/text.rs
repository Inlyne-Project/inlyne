use crate::utils::{Align, Rect};
use wgpu_glyph::{
    ab_glyph::{Font, FontArc, PxScale},
    Extra, FontId, GlyphCruncher, HorizontalAlign, Layout, Section, SectionGlyph,
};

#[derive(Clone, Debug, Default)]
pub struct TextBox {
    pub indent: f32,
    pub texts: Vec<Text>,
    pub is_code_block: bool,
    pub is_quote_block: Option<usize>,
    pub is_anchor: Option<String>,
    pub align: Align,
    pub hidpi_scale: f32,
    pub padding_height: f32,
    pub background_color: Option<[f32; 4]>,
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

    pub fn set_quote_block(&mut self, nest: Option<usize>) {
        self.is_quote_block = nest;
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

    pub fn find_hoverable<'a, T: GlyphCruncher>(
        &'a self,
        glyph_brush: &'a mut T,
        loc: (f32, f32),
        screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
    ) -> Option<&'a Text> {
        let fonts: Vec<FontArc> = glyph_brush.fonts().to_vec();
        glyph_brush
            .glyphs(&self.glyph_section(screen_position, bounds, zoom))
            .find(|glyph| {
                let bounds = Rect::from((fonts[glyph.font_id.0]).glyph_bounds(&glyph.glyph));
                bounds.contains(loc)
            })
            .map(|glyph| &self.texts[glyph.section_index])
    }

    pub fn glyph_bounds<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
    ) -> Vec<(Rect, SectionGlyph)> {
        let mut glyph_bounds = Vec::new();
        let fonts: Vec<FontArc> = glyph_brush.fonts().to_vec();
        for glyph in glyph_brush.glyphs(&self.glyph_section(screen_position, bounds, zoom)) {
            let bounds = Rect::from((fonts[glyph.font_id.0]).glyph_bounds(&glyph.glyph));
            glyph_bounds.push((bounds, glyph.clone()));
        }
        glyph_bounds
    }

    pub fn size<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
    ) -> (f32, f32) {
        if self.texts.is_empty() {
            return (0., self.padding_height * self.hidpi_scale * zoom);
        }

        if let Some(bounds) =
            glyph_brush.glyph_bounds(&self.glyph_section(screen_position, bounds, zoom))
        {
            (
                bounds.width(),
                bounds.height() + self.padding_height * self.hidpi_scale * zoom,
            )
        } else {
            (0., self.padding_height * self.hidpi_scale * zoom)
        }
    }

    pub fn glyph_section(
        &self,
        mut screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
    ) -> Section {
        let texts = self.texts.iter().map(|t| t.wgpu_text(zoom)).collect();

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

    pub fn render_lines<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
    ) -> Option<Vec<((f32, f32), (f32, f32))>> {
        let mut has_lines = false;
        for text in &self.texts {
            if text.is_striked || text.is_underlined {
                has_lines = true;
                break;
            }
        }
        if !has_lines {
            return None;
        }
        let mut lines = Vec::new();
        for (glyph_bounds, glyph) in self.glyph_bounds(glyph_brush, screen_position, bounds, zoom) {
            if self.texts[glyph.section_index].is_underlined {
                lines.push((
                    (glyph_bounds.pos.0, glyph_bounds.max.1),
                    (glyph_bounds.max.0, glyph_bounds.max.1),
                ));
            }
            if self.texts[glyph.section_index].is_striked {
                let mid_height = glyph_bounds.pos.1 + glyph_bounds.size.1 / 2.;
                lines.push((
                    (glyph_bounds.pos.0, mid_height),
                    (glyph_bounds.max.0, mid_height),
                ));
            }
        }

        Some(lines)
    }

    pub fn render_selection<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
        zoom: f32,
        mut selection: ((f32, f32), (f32, f32)),
    ) -> (Vec<Rect>, String) {
        let mut selection_rects = Vec::new();
        let mut selection_text = String::new();
        if selection.0 == selection.1 {
            return (selection_rects, selection_text);
        }
        if selection.0 .1 > selection.1 .1 {
            std::mem::swap(&mut selection.0, &mut selection.1);
        }
        let rect = Rect::new(screen_position, bounds);
        if rect.contains(selection.0) {
            for (glyph_bounds, glyph) in
                self.glyph_bounds(glyph_brush, screen_position, bounds, zoom)
            {
                if (glyph_bounds.pos.1 >= selection.0 .1 && glyph_bounds.max.1 <= selection.1 .1)
                    || (glyph_bounds.max.1 <= selection.1 .1
                        && glyph_bounds.max.1 >= selection.0 .1
                        && glyph_bounds.max.0 >= selection.0 .0)
                    || (glyph_bounds.max.1 >= selection.1 .1
                        && glyph_bounds.pos.1 <= selection.0 .1
                        && glyph_bounds.pos.0 <= selection.0 .0.max(selection.1 .0)
                        && glyph_bounds.max.0 >= selection.0 .0.min(selection.1 .0))
                {
                    selection_rects.push(glyph_bounds);
                    if let Some(char) = self.texts[glyph.section_index]
                        .text
                        .chars()
                        .nth(glyph.byte_index)
                    {
                        selection_text.push(char);
                    }
                }
            }
            selection_text.push('\n');
        }
        if rect.pos.1 >= selection.0 .1.min(selection.1 .1)
            && rect.max.1 <= selection.0 .1.max(selection.1 .1)
        {
            selection_rects.push(rect.clone());
            for text in &self.texts {
                selection_text.push_str(&text.text);
            }
            selection_text.push('\n');
        }
        if rect.contains(selection.1) {
            for (glyph_bounds, glyph) in
                self.glyph_bounds(glyph_brush, screen_position, bounds, zoom)
            {
                if (glyph_bounds.pos.1 >= selection.0 .1 && glyph_bounds.max.1 <= selection.1 .1)
                    || (glyph_bounds.pos.1 <= selection.1 .1
                        && glyph_bounds.pos.1 >= selection.0 .1
                        && glyph_bounds.pos.0 <= selection.1 .0)
                {
                    selection_rects.push(glyph_bounds);
                    if let Some(char) = self.texts[glyph.section_index]
                        .text
                        .chars()
                        .nth(glyph.byte_index)
                    {
                        selection_text.push(char);
                    }
                }
            }
            selection_text.push('\n');
        }
        (selection_rects, selection_text)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Text {
    pub text: String,
    pub size: f32,
    pub color: Option<[f32; 4]>,
    pub link: Option<String>,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub is_striked: bool,
    pub font: usize,
    pub hidpi_scale: f32,
    pub default_color: [f32; 4],
}

impl Text {
    pub fn new(text: String, hidpi_scale: f32, default_text_color: [f32; 4]) -> Self {
        Self {
            text,
            size: 16.,
            hidpi_scale,
            default_color: default_text_color,
            ..Default::default()
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

    pub fn make_underlined(mut self, underlined: bool) -> Self {
        self.is_underlined = underlined;
        self
    }

    pub fn make_striked(mut self, striked: bool) -> Self {
        self.is_striked = striked;
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
        } else if self.is_italic {
            base + 1
        } else {
            base
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

    pub fn wgpu_text(&self, zoom: f32) -> wgpu_glyph::Text {
        wgpu_glyph::Text {
            text: &self.text,
            scale: PxScale::from(self.size * self.hidpi_scale * zoom),
            font_id: self.font_id(),
            extra: Extra {
                color: self.color(),
                z: 0.0,
            },
        }
    }
}
