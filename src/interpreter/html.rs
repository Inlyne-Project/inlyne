use crate::{positioner::Section, table::Table, text::TextBox, utils::Align};

pub enum HeaderType {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl HeaderType {
    pub fn new(s: &str) -> Option<Self> {
        let header_type = match s {
            "h1" => Self::H1,
            "h2" => Self::H2,
            "h3" => Self::H3,
            "h4" => Self::H4,
            "h5" => Self::H5,
            "h6" => Self::H6,
            _ => return None,
        };

        Some(header_type)
    }

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
    TableRow(Vec<TextBox>),
    Header(Header),
    Paragraph(Option<Align>),
    Div(Option<Align>),
    Details(Section),
    Summary,
}
