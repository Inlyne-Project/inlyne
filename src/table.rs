use crate::{
    text::{Text, TextBox, TextBoxMeasure, TextCache, TextSystem},
    utils::{Point, Rect, Size},
};

use std::{
    default::default,
    sync::{Arc, Mutex},
};

use glyphon::FontSystem;
use taffy::{
    prelude::{auto, length, Display, Layout, Size as TaffySize, Style, Taffy},
    style::AvailableSpace,
    tree::MeasureFunc,
};

pub const TABLE_ROW_GAP: f32 = 20.;
pub const TABLE_COL_GAP: f32 = 20.;

#[derive(Default, Debug)]
pub struct Table {
    pub headers: Vec<TextBox>,
    pub rows: Vec<Vec<TextBox>>,
}

impl Table {
    pub fn new() -> Table {
        Table::default()
    }

    pub fn find_hoverable<'a>(
        &'a self,
        text_system: &mut TextSystem,
        loc: Point,
        pos: Point,
        bounds: Size,
        zoom: f32,
    ) -> Option<&'a Text> {
        let row_heights = self.row_heights(text_system, bounds, zoom);
        let column_widths = self.column_widths(text_system, bounds, zoom);
        let mut x = 0.;
        let mut y = 0.;
        for (i, header) in self.headers.iter().enumerate() {
            let size = header.size(text_system, (bounds.0 - x, bounds.1), zoom);
            if Rect::new((pos.0 + x, pos.1 + y), size).contains(loc) {
                return header.find_hoverable(
                    text_system,
                    loc,
                    (pos.0 + x, pos.1 + y),
                    (bounds.0 - x, bounds.1),
                    zoom,
                );
            }
            x += column_widths.get(i).unwrap() + TABLE_COL_GAP;
        }
        y += row_heights.first().unwrap() + TABLE_ROW_GAP;
        for (row_num, row) in self.rows.iter().enumerate() {
            let mut x = 0.;
            for (i, row_text_box) in row.iter().enumerate() {
                let size = row_text_box.size(text_system, (bounds.0 - x, bounds.1), zoom);
                if Rect::new((pos.0 + x, pos.1 + y), size).contains(loc) {
                    return row_text_box.find_hoverable(
                        text_system,
                        loc,
                        (pos.0 + x, pos.1 + y),
                        (bounds.0 - x, bounds.1),
                        zoom,
                    );
                }
                x += column_widths[i] + TABLE_COL_GAP;
            }
            y += row_heights.get(row_num + 1).unwrap() + TABLE_ROW_GAP;
        }
        None
    }

    pub fn column_widths(&self, text_system: &mut TextSystem, bounds: Size, zoom: f32) -> Vec<f32> {
        let mut max_row_len = self.headers.len();
        for row in &self.rows {
            max_row_len = std::cmp::max(max_row_len, row.len());
        }
        let mut widths = Vec::with_capacity(max_row_len);

        for i in 0..max_row_len {
            let mut max_width: f32 = self
                .headers
                .get(i)
                .map(|h| h.size(text_system, bounds, zoom).0)
                .unwrap_or_default();

            for row in &self.rows {
                let width = row
                    .get(i)
                    .map(|h| h.size(text_system, bounds, zoom).0)
                    .unwrap_or_default();
                if width > max_width {
                    max_width = width;
                }
            }

            widths.push(max_width);
        }

        widths
    }

    pub fn row_heights(&self, text_system: &mut TextSystem, bounds: Size, zoom: f32) -> Vec<f32> {
        let widths = self.column_widths(text_system, bounds, zoom);
        let mut heights = Vec::with_capacity(self.rows.len() + 1);
        let mut max_height = 0.;
        let mut x = 0.;
        for (i, header_text_box) in self.headers.iter().enumerate() {
            let height = header_text_box
                .size(text_system, (bounds.0 - x, bounds.1), zoom)
                .1;
            if height > max_height {
                max_height = height;
            }
            x += widths[i] + TABLE_COL_GAP;
        }
        heights.push(max_height);
        for row in &self.rows {
            let mut x = 0.;
            let mut max_height = 0.;
            for (i, row_text_box) in row.iter().enumerate() {
                let height = row_text_box
                    .size(text_system, (bounds.0 - x, bounds.1), zoom)
                    .1;
                if height > max_height {
                    max_height = height;
                }
                x += widths[i] + TABLE_COL_GAP;
            }
            heights.push(max_height);
        }
        heights
    }

    pub fn layout(
        &self,
        _text_system: &mut TextSystem,
        bounds: Size,
        zoom: f32,
    ) -> anyhow::Result<Vec<Vec<Layout>>> {
        let mut taffy = Taffy::new();
        let max_columns = self.rows.iter().fold(self.headers.len(), |max, row| {
            if row.len() > max {
                row.len()
            } else {
                max
            }
        });

        // Setup the grid
        let root_style = Style {
            display: Display::Grid,
            size: TaffySize {
                width: auto(),
                height: auto(),
            },
            gap: TaffySize {
                width: length(TABLE_COL_GAP),
                height: length(TABLE_ROW_GAP),
            },
            grid_template_columns: vec![auto(); max_columns],
            ..default()
        };

        let font_system = Arc::new(Mutex::new(FontSystem::new()));
        let text_cache = Arc::new(Mutex::new(TextCache::new()));
        let mut nodes = Vec::new();
        let mut node_row = Vec::new();
        // Define the child nodes
        for (_, header) in self.headers.iter().enumerate() {
            node_row.push(taffy.new_leaf_with_measure(
                Style { ..default() },
                MeasureFunc::Boxed(Box::new(TextBoxMeasure {
                    font_system: font_system.clone(),
                    text_cache: text_cache.clone(),
                    textbox: Arc::new(header.clone()),
                    zoom,
                })),
            )?);
        }
        nodes.push(node_row.clone());
        node_row.clear();

        for (_, row) in self.rows.iter().enumerate() {
            for (_, item) in row.iter().enumerate() {
                let item = item.clone();
                node_row.push(taffy.new_leaf_with_measure(
                    Style { ..default() },
                    MeasureFunc::Boxed(Box::new(TextBoxMeasure {
                        font_system: font_system.clone(),
                        text_cache: text_cache.clone(),
                        textbox: Arc::new(item.clone()),
                        zoom,
                    })),
                )?);
            }
            nodes.push(node_row.clone());
            node_row.clear();
        }

        let mut flattened_nodes = Vec::new();
        for row in &nodes {
            flattened_nodes.append(&mut row.clone());
        }

        let root = taffy.new_with_children(root_style, &flattened_nodes)?;

        taffy
            .compute_layout(
                root,
                TaffySize::<AvailableSpace> {
                    width: AvailableSpace::Definite(bounds.0),
                    height: AvailableSpace::Definite(bounds.1),
                },
            )
            .unwrap();
        let mut layouts = Vec::new();

        for row in nodes {
            layouts.push(row.iter().map(|n| *taffy.layout(*n).unwrap()).collect())
        }

        Ok(layouts)
    }

    pub fn push_header(&mut self, header: TextBox) {
        self.headers.push(header);
    }

    pub fn push_row(&mut self, row: Vec<TextBox>) {
        self.rows.push(row);
    }
}
