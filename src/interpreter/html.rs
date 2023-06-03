use crate::{
    positioner::{Positioned, Section},
    table::Table,
    utils::Align,
};

pub enum HeaderType {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl HeaderType {
    pub fn text_size(&self) -> f32 {
        match &self {
            Self::H1 => 32.,
            Self::H2 => 24.,
            Self::H3 => 18.72,
            Self::H4 => 16.,
            Self::H5 => 13.28,
            Self::H6 => 10.72,
        }
    }
}

pub struct Header {
    pub header_type: HeaderType,
    pub align: Option<Align>,
}

#[derive(Debug)]
pub enum ListType {
    Ordered(usize),
    Unordered,
}

pub struct List {
    pub list_type: ListType,
}

// Represents the number of parent text option tags the current element is a child of
#[derive(Default)]
pub struct TextOptions {
    pub underline: usize,
    pub bold: usize,
    pub italic: usize,
    pub strike_through: usize,
    pub small: usize,
    pub code: usize,
    pub pre_formatted: usize,
    pub block_quote: usize,
    pub link: Vec<String>,
}

pub enum Element {
    List(List),
    Input,
    Table(Table),
    TableRow(Vec<Positioned<crate::Element>>),
    Header(Header),
    Paragraph(Option<Align>),
    Div(Option<Align>),
    Details(Section),
    Summary,
}
