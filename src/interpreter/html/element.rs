use super::{picture, Header, List, ListType};
use crate::utils::Align;
use crate::{Section, Table, TextBox};

pub enum Element {
    List(List),
    Input,
    Table(Table),
    TableRow(Vec<TextBox>),
    Header(Header),
    Paragraph(Option<Align>),
    Div(Option<Align>),
    Details(Section),
    Summary,
    Picture(picture::Builder),
}

impl From<picture::Builder> for Element {
    fn from(pic: picture::Builder) -> Self {
        Self::Picture(pic)
    }
}

impl Element {
    pub fn table() -> Self {
        Self::Table(Table::new())
    }

    pub fn table_row() -> Self {
        Self::TableRow(Vec::new())
    }

    pub fn unordered_list() -> Self {
        Self::List(List {
            ty: ListType::Unordered,
        })
    }

    pub fn ordered_list(start_index: usize) -> Self {
        Self::List(List {
            ty: ListType::Ordered(start_index),
        })
    }

    pub fn is_picture(&self) -> bool {
        matches!(self, Self::Picture(_))
    }

    pub fn as_mut_list(&mut self) -> Option<&mut List> {
        if let Self::List(list) = self {
            Some(list)
        } else {
            None
        }
    }

    pub fn as_mut_table(&mut self) -> Option<&mut Table> {
        if let Self::Table(table) = self {
            Some(table)
        } else {
            None
        }
    }
}
