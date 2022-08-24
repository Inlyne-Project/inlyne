use std::collections::HashMap;

use wgpu_glyph::GlyphBrush;

use crate::{
    table::{TABLE_COL_GAP, TABLE_ROW_GAP},
    utils::{Align, Rect},
    Element,
};

pub const DEFAULT_PADDING: f32 = 5.;
pub const DEFAULT_MARGIN: f32 = 100.;

#[derive(Debug)]
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
        let bounds = match &mut element.inner {
            Element::TextBox(text_box) => {
                let indent = text_box.indent;
                let pos = (DEFAULT_MARGIN + indent, self.reserved_height);

                let size = text_box.size(
                    glyph_brush,
                    pos,
                    ((self.screen_size.0 - pos.0 - DEFAULT_MARGIN).max(0.), f32::INFINITY),
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
            Element::Row(row) => {
                let mut reserved_width = DEFAULT_MARGIN;
                let mut inner_reserved_height: f32 = 0.;
                let mut max_height: f32 = 0.;
                let mut max_width: f32 = 0.;
                for element in &mut row.elements {
                    self.position(glyph_brush, element, zoom);
                    let element_bounds = element.bounds.as_mut().expect("already positioned");

                    let target_width = reserved_width
                        + DEFAULT_PADDING * self.hidpi_scale * zoom
                        + element_bounds.size.0;
                    // Row would be too long with this element so add another line
                    if target_width > self.screen_size.0 - DEFAULT_MARGIN {
                        max_width = max_width.max(reserved_width);
                        reserved_width = DEFAULT_MARGIN
                            + DEFAULT_PADDING * self.hidpi_scale * zoom
                            + element_bounds.size.0;
                        inner_reserved_height +=
                            max_height + DEFAULT_PADDING * self.hidpi_scale * zoom;
                        max_height = element_bounds.size.1;
                        element_bounds.pos.0 = DEFAULT_MARGIN;
                    } else {
                        max_height = max_height.max(element_bounds.size.1);
                        element_bounds.pos.0 = reserved_width;
                        reserved_width = target_width;
                    }
                    element_bounds.pos.1 = self.reserved_height + inner_reserved_height;
                }
                max_width = max_width.max(reserved_width);
                inner_reserved_height += max_height + DEFAULT_PADDING * self.hidpi_scale * zoom;
                Rect::new(
                    (DEFAULT_MARGIN, self.reserved_height),
                    (max_width - DEFAULT_MARGIN, inner_reserved_height),
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

#[derive(Debug)]
pub struct Row {
    pub elements: Vec<Positioned<Element>>,
    pub hidpi_scale: f32,
}

impl Row {
    pub fn new(elements: Vec<Positioned<Element>>, hidpi_scale: f32) -> Self {
        Self {
            elements,
            hidpi_scale,
        }
    }
}
