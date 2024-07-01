use std::sync::Arc;

use crate::text::{Text, TextBox, TextBoxMeasure, TextSystem};
use crate::utils::{default, Point, Rect, Size};

use taffy::node::MeasureFunc;
use taffy::prelude::{
    auto, line, points, AvailableSpace, Display, Layout, Size as TaffySize, Style, Taffy,
};
use taffy::style::JustifyContent;

pub const TABLE_ROW_GAP: f32 = 20.;
pub const TABLE_COL_GAP: f32 = 20.;

#[derive(Debug)]
pub struct TableLayout {
    pub rows: Vec<Vec<Layout>>,
    pub size: Size,
}

#[derive(Default, Debug)]
pub struct Table {
    pub rows: Vec<Vec<TextBox>>,
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

        for (row, row_layout) in self.rows.iter().zip(table_layout.rows.iter()) {
            for (item, layout) in row.iter().zip(row_layout.iter()) {
                if Rect::new(
                    (pos.0 + layout.location.x, pos.1 + layout.location.y),
                    (layout.size.width, layout.size.height),
                )
                .contains(loc)
                {
                    return item.find_hoverable(
                        text_system,
                        loc,
                        (pos.0 + layout.location.x, pos.1 + layout.location.y),
                        (layout.size.width, layout.size.height),
                        zoom,
                    );
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
        let max_columns = self
            .rows
            .iter()
            .fold(0, |max, row| std::cmp::max(row.len(), max));

        // Setup the grid
        let root_style = Style {
            display: Display::Flex,
            size: TaffySize {
                width: points(bounds.0),
                height: auto(),
            },
            justify_content: Some(JustifyContent::Start),
            ..default()
        };

        let grid_style = Style {
            display: Display::Grid,
            gap: TaffySize {
                width: points(TABLE_COL_GAP),
                height: points(TABLE_ROW_GAP),
            },
            grid_template_columns: vec![auto(); max_columns],
            ..default()
        };

        let mut nodes = Vec::new();
        let mut node_row = Vec::new();

        for (y, row) in self.rows.iter().enumerate() {
            for (x, item) in row.iter().enumerate() {
                let item = item.clone();
                let textbox_measure = TextBoxMeasure {
                    font_system: text_system.font_system.clone(),
                    text_cache: text_system.text_cache.clone(),
                    textbox: Arc::new(item.clone()),
                    zoom,
                };
                node_row.push(taffy.new_leaf_with_measure(
                    Style {
                        grid_row: line(1 + y as i16 + 1),
                        grid_column: line(x as i16 + 1),
                        ..default()
                    },
                    MeasureFunc::Boxed(Box::new(move |known_dimensions, available_space| {
                        textbox_measure.measure(known_dimensions, available_space)
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

        let grid = taffy.new_with_children(grid_style, &flattened_nodes)?;
        let root = taffy.new_with_children(root_style, &[grid])?;

        taffy.compute_layout(
            root,
            TaffySize::<AvailableSpace> {
                width: AvailableSpace::Definite(bounds.0),
                height: AvailableSpace::MaxContent,
            },
        )?;

        let rows_layout: Vec<Vec<Layout>> = nodes
            .into_iter()
            .map(|row| row.iter().map(|n| *taffy.layout(*n).unwrap()).collect())
            .collect();
        let size = taffy.layout(root)?.size;

        Ok(TableLayout {
            rows: rows_layout,
            size: (size.width, size.height),
        })
    }
    pub fn push_row(&mut self, row: Vec<TextBox>) {
        self.rows.push(row);
    }
}
