use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

use anyhow::Context;
use taffy::Taffy;

use crate::image::Image;
use crate::text::TextSystem;
use crate::utils::{Align, Point, Rect, Size};
use crate::{debug_impls, Element};

pub const DEFAULT_PADDING: f32 = 5.;
pub const DEFAULT_MARGIN: f32 = 100.;

#[derive(Debug, PartialEq)]
pub struct Positioned<T> {
    pub inner: T,
    pub bounds: Option<Rect>,
}

impl<T> Positioned<T> {
    pub fn contains(&self, loc: Point) -> bool {
        self.bounds
            .as_ref()
            .context("Element not positioned")
            .unwrap()
            .contains(loc)
    }
}

impl<T> Positioned<T> {
    pub fn new<I: Into<T>>(item: I) -> Positioned<T> {
        Positioned {
            inner: item.into(),
            bounds: None,
        }
    }
}

#[derive(Default)]
pub struct Positioner {
    pub screen_size: Size,
    pub reserved_height: f32,
    pub hidpi_scale: f32,
    pub page_width: f32,
    pub anchors: HashMap<String, f32>,
    pub taffy: Taffy,
}

impl Positioner {
    pub fn new(screen_size: Size, hidpi_scale: f32, page_width: f32) -> Self {
        let mut taffy = Taffy::new();
        taffy.disable_rounding();
        Self {
            reserved_height: DEFAULT_PADDING * hidpi_scale,
            hidpi_scale,
            page_width,
            screen_size,
            anchors: HashMap::new(),
            taffy,
        }
    }

    // Positions the element but does not update reserved_height
    pub fn position(
        &mut self,
        text_system: &mut TextSystem,
        element: &mut Positioned<Element>,
        zoom: f32,
    ) -> anyhow::Result<()> {
        let centering = (self.screen_size.0 - self.page_width).max(0.) / 2.;

        let bounds = match &mut element.inner {
            Element::TextBox(text_box) => {
                let indent = text_box.indent;
                let pos = (DEFAULT_MARGIN + indent + centering, self.reserved_height);

                let size = text_box.size(
                    text_system,
                    (
                        (self.screen_size.0 - pos.0 - DEFAULT_MARGIN - centering).max(0.),
                        f32::INFINITY,
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
                let size = image
                    .size(
                        (self.screen_size.0.min(self.page_width), self.screen_size.1),
                        zoom,
                    )
                    .unwrap_or_default();
                match image.is_aligned {
                    Some(Align::Center) => Rect::new(
                        (self.screen_size.0 / 2. - size.0 / 2., self.reserved_height),
                        size,
                    ),
                    _ => Rect::new((DEFAULT_MARGIN + centering, self.reserved_height), size),
                }
            }
            Element::Table(table) => {
                let pos = (DEFAULT_MARGIN + centering, self.reserved_height);
                let layout = table.layout(
                    text_system,
                    &mut self.taffy,
                    (
                        self.screen_size.0 - pos.0 - DEFAULT_MARGIN - centering,
                        f32::INFINITY,
                    ),
                    zoom,
                )?;
                Rect::new(
                    (DEFAULT_MARGIN + centering, self.reserved_height),
                    layout.size,
                )
            }
            Element::Row(row) => {
                let mut reserved_width = DEFAULT_MARGIN + centering;
                let mut inner_reserved_height: f32 = 0.;
                let mut max_height: f32 = 0.;
                let mut max_width: f32 = 0.;
                for element in &mut row.elements {
                    self.position(text_system, element, zoom)?;
                    let element_bounds = element
                        .bounds
                        .as_mut()
                        .context("Element didn't have bounds")?;

                    let target_width = reserved_width
                        + DEFAULT_PADDING * self.hidpi_scale * zoom
                        + element_bounds.size.0;
                    // Row would be too long with this element so add another line
                    if target_width > self.screen_size.0 - DEFAULT_MARGIN - centering {
                        max_width = max_width.max(reserved_width);
                        reserved_width = DEFAULT_MARGIN
                            + centering
                            + DEFAULT_PADDING * self.hidpi_scale * zoom
                            + element_bounds.size.0;
                        inner_reserved_height +=
                            max_height + DEFAULT_PADDING * self.hidpi_scale * zoom;
                        max_height = element_bounds.size.1;
                        element_bounds.pos.0 = DEFAULT_MARGIN + centering;
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
                    (DEFAULT_MARGIN + centering, self.reserved_height),
                    (
                        max_width - DEFAULT_MARGIN - centering,
                        inner_reserved_height,
                    ),
                )
            }
            Element::Section(section) => {
                let mut section_bounds =
                    Rect::new((DEFAULT_MARGIN + centering, self.reserved_height), (0., 0.));
                if let Some(ref mut summary) = *section.summary {
                    self.position(text_system, summary, zoom)?;
                    let element_size = summary
                        .bounds
                        .as_mut()
                        .context("Element didn't have bounds")?
                        .size;
                    self.reserved_height +=
                        element_size.1 + DEFAULT_PADDING * self.hidpi_scale * zoom;
                    section_bounds.size.1 +=
                        element_size.1 + DEFAULT_PADDING * self.hidpi_scale * zoom;
                    section_bounds.size.0 = section_bounds.size.0.max(element_size.0)
                }
                for element in &mut section.elements {
                    self.position(text_system, element, zoom)?;
                    let element_size = element
                        .bounds
                        .as_mut()
                        .context("Element didn't have bounds")?
                        .size;
                    self.reserved_height +=
                        element_size.1 + DEFAULT_PADDING * self.hidpi_scale * zoom;
                    if !*section.hidden.borrow() {
                        section_bounds.size.1 +=
                            element_size.1 + DEFAULT_PADDING * self.hidpi_scale * zoom;
                        section_bounds.size.0 = section_bounds.size.0.max(element_size.0)
                    }
                }
                self.reserved_height = section_bounds.pos.1;
                section_bounds
            }
        };
        element.bounds = Some(bounds);
        Ok(())
    }

    // Resets reserved height and positions every element again
    pub fn reposition(
        &mut self,
        text_system: &mut TextSystem,
        elements: &mut [Positioned<Element>],
        zoom: f32,
    ) -> anyhow::Result<()> {
        self.reserved_height = DEFAULT_PADDING * self.hidpi_scale * zoom;

        for element in elements {
            self.position(text_system, element, zoom)?;
            self.reserved_height += DEFAULT_PADDING * self.hidpi_scale * zoom
                + element
                    .bounds
                    .as_ref()
                    .context("Element didn't have bounds")?
                    .size
                    .1;
        }
        Ok(())
    }
}

#[derive(PartialEq)]
pub struct Spacer {
    pub space: f32,
    pub visible: bool,
}

impl Spacer {
    pub fn invisible() -> Self {
        Self::new(5.0, false)
    }

    pub fn visible() -> Self {
        Self::new(5.0, true)
    }

    pub fn new(space: f32, visible: bool) -> Self {
        Self { space, visible }
    }
}

impl fmt::Debug for Spacer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_impls::spacer(self, f)
    }
}

#[derive(Debug, PartialEq)]
pub struct Row {
    pub elements: Vec<Positioned<Element>>,
    pub hidpi_scale: f32,
}

impl Row {
    pub fn with_image(image: Image, hidpi_scale: f32) -> Self {
        Self {
            elements: vec![Positioned::new(image)],
            hidpi_scale,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Section {
    pub elements: Vec<Positioned<Element>>,
    pub hidpi_scale: f32,
    pub hidden: RefCell<bool>,
    pub summary: Box<Option<Positioned<Element>>>,
}

impl Section {
    pub fn bare(hidpi_scale: f32) -> Self {
        Self {
            elements: Default::default(),
            hidpi_scale,
            hidden: Default::default(),
            summary: Default::default(),
        }
    }
}
