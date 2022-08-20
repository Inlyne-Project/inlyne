use std::collections::HashMap;

use wgpu_glyph::GlyphBrush;

use crate::{
    table::{TABLE_COL_GAP, TABLE_ROW_GAP},
    utils::{Align, Rect},
    Element,
};

pub const DEFAULT_PADDING: f32 = 5.;
pub const DEFAULT_MARGIN: f32 = 100.;

pub struct Positioned<T> {
    pub inner: T,
    pub bounds: Option<Rect>,
}

impl<T> Positioned<T> {
    pub fn contains(&self, loc: (f32, f32)) -> bool {
        self.bounds.as_ref().unwrap().contains(loc)
    }
}

impl<T> Positioned<T> {
    pub fn new(item: T) -> Positioned<T> {
        Positioned {
            inner: item,
            bounds: None,
        }
    }
}

#[derive(Default)]
pub struct Positioner {
    pub screen_size: (f32, f32),
    pub reserved_height: f32,
    pub hidpi_scale: f32,
    pub anchors: HashMap<String, f32>,
}

impl Positioner {
    pub fn new(screen_size: (f32, f32), hidpi_scale: f32) -> Self {
        Self {
            reserved_height: DEFAULT_PADDING * hidpi_scale,
            hidpi_scale,
            screen_size,
            anchors: HashMap::new(),
        }
    }

    // Positions the element but does not update reserved_height
    pub fn position(
        &mut self,
        glyph_brush: &mut GlyphBrush<()>,
        element: &mut Positioned<Element>,
        zoom: f32,
    ) {
        let bounds = match &element.inner {
            Element::TextBox(text_box) => {
                let indent = text_box.indent;
                let pos = (DEFAULT_MARGIN + indent, self.reserved_height);

                let size = text_box.size(
                    glyph_brush,
                    pos,
                    (
                        self.screen_size.0 - pos.0 - DEFAULT_MARGIN,
                        self.screen_size.1,
                    ),
                    zoom,
                );

                if let Some(ref anchor_name) = text_box.is_anchor {
                    let _ = self.anchors.insert(anchor_name.clone(), pos.1);
                }

                Rect::new(pos, size)
            }
            Element::Spacer(spacer) => Rect::new(
                (0., self.reserved_height),
                (0., spacer.space * self.hidpi_scale * zoom),
            ),
            Element::Image(image) => {
                let size = image.size(self.screen_size, zoom);
                match image.is_aligned {
                    Some(Align::Center) => Rect::new(
                        (self.screen_size.0 / 2. - size.0 / 2., self.reserved_height),
                        size,
                    ),
                    _ => Rect::new((DEFAULT_MARGIN, self.reserved_height), size),
                }
            }
            Element::Table(table) => {
                let pos = (DEFAULT_MARGIN, self.reserved_height);
                let width = table
                    .column_widths(
                        glyph_brush,
                        pos,
                        (self.screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                        zoom,
                    )
                    .iter()
                    .fold(0., |acc, x| acc + x);
                let height = table
                    .row_heights(
                        glyph_brush,
                        pos,
                        (self.screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                        zoom,
                    )
                    .iter()
                    .fold(0., |acc, x| acc + x);
                Rect::new(
                    pos,
                    (
                        width * (TABLE_COL_GAP * table.headers.len() as f32),
                        height + (TABLE_ROW_GAP * (table.rows.len() + 1) as f32),
                    ),
                )
            }
        };
        element.bounds = Some(bounds);
    }

    // Resets reserved height and positions every element again
    pub fn reposition(
        &mut self,
        glyph_brush: &mut GlyphBrush<()>,
        elements: &mut [Positioned<Element>],
        zoom: f32,
    ) {
        self.reserved_height = DEFAULT_PADDING * self.hidpi_scale * zoom;

        for element in elements {
            self.position(glyph_brush, element, zoom);
            self.reserved_height += DEFAULT_PADDING * self.hidpi_scale * zoom
                + element.bounds.as_ref().expect("already positioned").size.1;
        }
    }
}

#[derive(Debug)]
pub struct Spacer {
    pub space: f32,
}

impl Spacer {
    pub fn new(space: f32) -> Spacer {
        Spacer { space }
    }
}
