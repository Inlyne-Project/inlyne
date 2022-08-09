use wgpu_glyph::GlyphCruncher;
use winit::window::CursorIcon;

use crate::{text::TextBox, utils::Rect};

pub const TABLE_ROW_GAP: f32 = 20.;
pub const TABLE_COL_GAP: f32 = 20.;

pub struct Table {
    pub headers: Vec<TextBox>,
    pub rows: Vec<Vec<TextBox>>,
}

impl Table {
    pub fn new() -> Table {
        Table {
            headers: Vec::new(),
            rows: Vec::new(),
        }
    }

    pub fn hovering_over<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        loc: (f32, f32),
        pos: (f32, f32),
        bounds: (f32, f32),
        click: bool,
    ) -> CursorIcon {
        let row_heights = self.row_heights(glyph_brush, pos, bounds);
        let column_widths = self.column_widths(glyph_brush, pos, bounds);
        let mut x = 0.;
        let mut y = 0.;
        for (i, header) in self.headers.iter().enumerate() {
            let size = header.size(
                glyph_brush,
                (pos.0 + x, pos.1 + y),
                (bounds.0 - x, bounds.1),
            );
            if Rect::new((pos.0 + x, pos.1 + y), size).contains(loc) {
                return header.hovering_over(
                    glyph_brush,
                    loc,
                    (pos.0 + x, pos.1 + y),
                    (bounds.0 - x, bounds.1),
                    click,
                );
            }
            x += column_widths.get(i).unwrap() + TABLE_COL_GAP;
        }
        y += row_heights.first().unwrap() + TABLE_ROW_GAP;
        for (row_num, row) in self.rows.iter().enumerate() {
            let mut x = 0.;
            for (i, row_text_box) in row.iter().enumerate() {
                let size = row_text_box.size(
                    glyph_brush,
                    (pos.0 + x, pos.1 + y),
                    (bounds.0 - x, bounds.1),
                );
                if Rect::new((pos.0 + x, pos.1 + y), size).contains(loc) {
                    return row_text_box.hovering_over(
                        glyph_brush,
                        loc,
                        (pos.0 + x, pos.1 + y),
                        (bounds.0 - x, bounds.1),
                        click,
                    );
                }
                x += column_widths[i] + TABLE_COL_GAP;
            }
            y += row_heights.get(row_num + 1).unwrap() + TABLE_ROW_GAP;
        }
        CursorIcon::Default
    }

    pub fn column_widths<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
    ) -> Vec<f32> {
        let mut widths = Vec::with_capacity(self.headers.len());
        for (i, header_text_box) in self.headers.iter().enumerate() {
            let mut max_width = header_text_box.size(glyph_brush, screen_position, bounds).0;
            for row in &self.rows {
                if let Some(text_box) = row.get(i) {
                    let width = text_box.size(glyph_brush, screen_position, bounds).0;
                    if width > max_width {
                        max_width = width;
                    }
                }
            }
            widths.push(max_width);
        }
        widths
    }

    pub fn row_heights<T: GlyphCruncher>(
        &self,
        glyph_brush: &mut T,
        screen_position: (f32, f32),
        bounds: (f32, f32),
    ) -> Vec<f32> {
        let widths = self.column_widths(glyph_brush, screen_position, bounds);
        let mut heights = Vec::with_capacity(self.rows.len() + 1);
        let mut max_height = 0.;
        let mut x = 0.;
        let mut y = 0.;
        for (i, header_text_box) in self.headers.iter().enumerate() {
            let height = header_text_box
                .size(
                    glyph_brush,
                    (screen_position.0 + x, screen_position.1 + y),
                    (bounds.0 - x, bounds.1),
                )
                .1;
            if height > max_height {
                max_height = height;
            }
            x += widths[i] + TABLE_COL_GAP;
        }
        y += max_height + TABLE_COL_GAP;
        heights.push(max_height);
        for row in &self.rows {
            let mut x = 0.;
            let mut max_height = 0.;
            for (i, row_text_box) in row.iter().enumerate() {
                let height = row_text_box
                    .size(
                        glyph_brush,
                        (screen_position.0 + x, screen_position.1 + y),
                        (bounds.0 - x, bounds.1),
                    )
                    .1;
                if height > max_height {
                    max_height = height;
                }
                x += widths[i] + TABLE_COL_GAP;
            }
            y += max_height + TABLE_ROW_GAP;
            heights.push(max_height);
        }
        heights
    }

    pub fn push_header(&mut self, header: TextBox) {
        self.headers.push(header);
    }

    pub fn push_row(&mut self, row: Vec<TextBox>) {
        self.rows.push(row);
    }
}
