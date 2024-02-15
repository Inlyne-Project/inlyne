pub mod attr;
mod element;
pub mod picture;
pub mod style;
mod tag_name;

pub use attr::Attr;
pub use element::Element;
pub use picture::Picture;
pub use tag_name::TagName;

use crate::utils::Align;

use html5ever::Attribute;

pub fn find_align(attrs: &[Attribute]) -> Option<Align> {
    attr::Iter::new(attrs).find_map(|attr| {
        if let Attr::Align(align) = attr {
            Some(align)
        } else {
            None
        }
    })
}

pub fn find_style(attrs: &[Attribute]) -> Option<String> {
    attr::Iter::new(attrs).find_map(|attr| {
        if let Attr::Style(style) = attr {
            Some(style)
        } else {
            None
        }
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub ty: HeaderType,
    pub align: Option<Align>,
}

impl Header {
    pub fn new(ty: HeaderType, align: Option<Align>) -> Self {
        Self { ty, align }
    }
}

#[derive(Debug)]
pub enum ListType {
    Ordered(usize),
    Unordered,
}

pub struct List {
    pub ty: ListType,
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
