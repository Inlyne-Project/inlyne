use crate::{
    positioner::Positioned,
    text::{Text, TextBoxMeasure, TextCache, TextSystem},
    utils::{default, Point, Rect, Size},
    Element,
};

use std::sync::{Arc, Mutex};

use glyphon::FontSystem;
use taffy::{
    prelude::{
        auto, length, line, AvailableSpace, Display, Layout, Size as TaffySize, Style, Taffy,
    },
    style::JustifyContent,
    tree::{Measurable, MeasureFunc},
};

pub const TABLE_ROW_GAP: f32 = 20.;
pub const TABLE_COL_GAP: f32 = 20.;

pub struct TableMeasure {
    pub table: Arc<Table>,
    pub text_cache: Arc<Mutex<TextCache>>,
    pub font_system: Arc<Mutex<FontSystem>>,
    pub zoom: f32,
}

impl Measurable for TableMeasure {
    fn measure(
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
        let available_height = match available_space.height {
            AvailableSpace::Definite(space) => space,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::MAX,
        };
        let height_bound = known_dimensions.height.unwrap_or(available_height);

        let size = self
            .table
            .layout_internal(
                self.text_cache.clone(),
                self.font_system.clone(),
                &mut Taffy::new(),
                (width_bound, height_bound),
                self.zoom,
            )
            .unwrap()
            .size;
        TaffySize {
            width: known_dimensions.width.unwrap_or(size.0),
            height: known_dimensions.height.unwrap_or(size.1),
        }
    }
}

#[derive(Debug)]
pub struct TableLayout {
    pub headers: Vec<Layout>,
    pub rows: Vec<Vec<Layout>>,
    pub size: Size,
}

#[derive(Default, Debug, Clone)]
pub struct Table {
    pub headers: Vec<Positioned<Element>>,
    pub rows: Vec<Vec<Positioned<Element>>>,
}

impl Table {
    pub fn new() -> Table {
        Table::default()
    }

    pub fn find_hoverable<'a>(
        &'a self,
        text_system: &mut TextSystem,
        taffy: &mut Taffy,
        loc: Point,
        pos: Point,
        bounds: Size,
        zoom: f32,
    ) -> Option<&'a Text> {
        let table_layout = self.layout(text_system, taffy, bounds, zoom).ok()?;
        for (header, layout) in self.headers.iter().zip(table_layout.headers.iter()) {
            if Rect::new(
                (pos.0 + layout.location.x, pos.1 + layout.location.y),
                (layout.size.width, layout.size.height),
            )
            .contains(loc)
            {
                let layout_bounds = Rect {
                    pos: (pos.0 + layout.location.x, pos.1 + layout.location.y),
                    size: (layout.size.width, layout.size.height),
                };
                let hoverable = match &header.inner {
                    Element::TextBox(textbox) => textbox.find_hoverable(
                        text_system,
                        loc,
                        layout_bounds.pos,
                        layout_bounds.size,
                        zoom,
                    ),
                    Element::Table(table) => table.find_hoverable(
                        text_system,
                        taffy,
                        loc,
                        layout_bounds.pos,
                        layout_bounds.size,
                        zoom,
                    ),
                    _ => None,
                };
                return hoverable;
            }
        }
        for (row, row_layout) in self.rows.iter().zip(table_layout.rows.iter()) {
            for (item, layout) in row.iter().zip(row_layout.iter()) {
                if Rect::new(
                    (pos.0 + layout.location.x, pos.1 + layout.location.y),
                    (layout.size.width, layout.size.height),
                )
                .contains(loc)
                {
                    let layout_bounds = Rect {
                        pos: (pos.0 + layout.location.x, pos.1 + layout.location.y),
                        size: (layout.size.width, layout.size.height),
                    };
                    let hoverable = match &item.inner {
                        Element::TextBox(textbox) => textbox.find_hoverable(
                            text_system,
                            loc,
                            layout_bounds.pos,
                            layout_bounds.size,
                            zoom,
                        ),
                        Element::Table(table) => table.find_hoverable(
                            text_system,
                            taffy,
                            loc,
                            layout_bounds.pos,
                            layout_bounds.size,
                            zoom,
                        ),
                        _ => None,
                    };
                    return hoverable;
                }
            }
        }
        None
    }

    pub fn layout(
        &self,
        text_system: &mut TextSystem,
        taffy: &mut Taffy,
        bounds: Size,
        zoom: f32,
    ) -> anyhow::Result<TableLayout> {
        self.layout_internal(
            text_system.text_cache.clone(),
            text_system.font_system.clone(),
            taffy,
            bounds,
            zoom,
        )
    }

    pub fn layout_internal(
        &self,
        text_cache: Arc<Mutex<TextCache>>,
        font_system: Arc<Mutex<FontSystem>>,
        taffy: &mut Taffy,
        bounds: Size,
        zoom: f32,
    ) -> anyhow::Result<TableLayout> {
        let max_columns = self
            .rows
            .iter()
            .fold(self.headers.len(), |max, row| std::cmp::max(row.len(), max));

        // Setup the grid
        let root_style = Style {
            display: Display::Flex,
            size: TaffySize {
                width: length(bounds.0),
                height: auto(),
            },
            justify_content: Some(JustifyContent::Start),
            ..default()
        };

        let grid_style = Style {
            display: Display::Grid,
            gap: TaffySize {
                width: length(TABLE_COL_GAP),
                height: length(TABLE_ROW_GAP),
            },
            grid_template_columns: vec![auto(); max_columns],
            ..default()
        };

        let mut nodes = Vec::new();
        let mut node_row = Vec::new();
        // Define the child nodes
        for (x, header) in self.headers.iter().enumerate() {
            let measure: Box<dyn Measurable> = match &header.inner {
                Element::TextBox(textbox) => Box::new(TextBoxMeasure {
                    font_system: font_system.clone(),
                    text_cache: text_cache.clone(),
                    textbox: Arc::new(textbox.clone()),
                    zoom,
                }),
                Element::Table(table) => Box::new(TableMeasure {
                    table: Arc::new(table.clone()),
                    font_system: font_system.clone(),
                    text_cache: text_cache.clone(),
                    zoom,
                }),
                _ => unimplemented!(),
            };
            if let Element::Table(_) = header.inner {
                let table = taffy.new_leaf_with_measure(
                    Style {
                        display: Display::Grid,
                        gap: TaffySize {
                            width: length(TABLE_COL_GAP),
                            height: length(TABLE_ROW_GAP),
                        },
                        grid_template_columns: vec![auto(); max_columns],
                        grid_row: line(1),
                        grid_column: line(x as i16 + 1),
                        ..default()
                    },
                    MeasureFunc::Boxed(measure),
                )?;
                let flex_table = taffy.new_with_children(
                    Style {
                        display: Display::Flex,
                        justify_content: Some(JustifyContent::Start),
                        grid_row: line(1),
                        grid_column: line(x as i16 + 1),
                        ..Default::default()
                    },
                    &[table],
                )?;
                node_row.push(flex_table);
            } else {
                node_row.push(taffy.new_leaf_with_measure(
                    Style {
                        grid_row: line(1),
                        grid_column: line(x as i16 + 1),
                        ..default()
                    },
                    MeasureFunc::Boxed(measure),
                )?);
            }
        }
        nodes.push(node_row.clone());
        node_row.clear();

        for (y, row) in self.rows.iter().enumerate() {
            for (x, item) in row.iter().enumerate() {
                let measure: Box<dyn Measurable> = match &item.inner {
                    Element::TextBox(textbox) => Box::new(TextBoxMeasure {
                        font_system: font_system.clone(),
                        text_cache: text_cache.clone(),
                        textbox: Arc::new(textbox.clone()),
                        zoom,
                    }),
                    Element::Table(table) => Box::new(TableMeasure {
                        table: Arc::new(table.clone()),
                        font_system: font_system.clone(),
                        text_cache: text_cache.clone(),
                        zoom,
                    }),
                    e => unimplemented!("{:?}", e),
                };
                if let Element::Table(_) = item.inner {
                    let table = taffy.new_leaf_with_measure(
                        Style {
                            display: Display::Grid,
                            gap: TaffySize {
                                width: length(TABLE_COL_GAP),
                                height: length(TABLE_ROW_GAP),
                            },
                            grid_template_columns: vec![auto(); max_columns],
                            ..default()
                        },
                        MeasureFunc::Boxed(measure),
                    )?;
                    let flex_table = taffy.new_with_children(
                        Style {
                            display: Display::Flex,
                            justify_content: Some(JustifyContent::Start),
                            grid_row: line(1 + y as i16 + (!self.headers.is_empty()) as i16),
                            grid_column: line(x as i16 + 1),
                            ..Default::default()
                        },
                        &[table],
                    )?;
                    node_row.push(flex_table);
                } else {
                    node_row.push(taffy.new_leaf_with_measure(
                        Style {
                            grid_row: line(1 + y as i16 + (!self.headers.is_empty()) as i16),
                            grid_column: line(x as i16 + 1),
                            ..default()
                        },
                        MeasureFunc::Boxed(measure),
                    )?);
                }
            }
            nodes.push(node_row.clone());
            node_row.clear();
        }

        let mut flattened_nodes = Vec::new();
        for row in &nodes {
            flattened_nodes.append(&mut row.clone());
        }

        let grid = taffy.new_with_children(grid_style, &flattened_nodes)?;
        let root = taffy.new_with_children(root_style, &[grid])?;

        taffy.compute_layout(
            root,
            TaffySize::<AvailableSpace> {
                width: AvailableSpace::Definite(bounds.0),
                height: AvailableSpace::MaxContent,
            },
        )?;

        let mut rows = nodes.into_iter();
        let header_layout = rows
            .next()
            .unwrap_or_default()
            .iter()
            .map(|n| *taffy.layout(*n).unwrap())
            .collect();

        let rows_layout: Vec<Vec<Layout>> = rows
            .map(|row| row.iter().map(|n| *taffy.layout(*n).unwrap()).collect())
            .collect();
        let size = taffy.layout(root)?.size;

        Ok(TableLayout {
            headers: header_layout,
            rows: rows_layout,
            size: (size.width, size.height),
        })
    }

    pub fn push_header(&mut self, header: Element) {
        self.headers.push(Positioned::new(header));
    }

    pub fn push_row(&mut self, row: Vec<Positioned<Element>>) {
        self.rows.push(row);
    }
}
