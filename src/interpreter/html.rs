use std::slice;

use html5ever::{local_name, Attribute};

use crate::{positioner::Section, table::Table, text::TextBox, utils::Align};

pub fn find_align(attrs: &[Attribute]) -> Option<Align> {
    AttrIter::new(attrs).find_map(|attr| {
        if let Attr::Align(align) = attr {
            Some(align)
        } else {
            None
        }
    })
}

pub fn find_style(attrs: &[Attribute]) -> Option<String> {
    AttrIter::new(attrs).find_map(|attr| {
        if let Attr::Style(style) = attr {
            Some(style)
        } else {
            None
        }
    })
}

pub struct AttrIter<'attrs>(slice::Iter<'attrs, Attribute>);

impl<'attrs> AttrIter<'attrs> {
    pub fn new(attrs: &'attrs [Attribute]) -> Self {
        Self(attrs.iter())
    }
}

impl<'attrs> Iterator for AttrIter<'attrs> {
    type Item = Attr;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Attribute { name, value } = self.0.next()?;
            let attr = match name.local {
                local_name!("align") => Align::new(&value).map(Attr::Align),
                local_name!("href") => Some(Attr::Href(value.to_string())),
                local_name!("id") => Some(Attr::Anchor(format!("#{value}"))),
                local_name!("width") => value.parse().ok().map(Attr::Width),
                local_name!("height") => value.parse().ok().map(Attr::Height),
                local_name!("src") => Some(Attr::Src(value.to_string())),
                local_name!("start") => value.parse().ok().map(Attr::Start),
                local_name!("style") => Some(Attr::Style(value.to_string())),
                local_name!("type") => {
                    (value.to_string() == "checkbox").then_some(Attr::IsCheckbox)
                }
                local_name!("checked") => Some(Attr::IsChecked),
                _ => continue,
            };

            if attr.is_some() {
                break attr;
            }
        }
    }
}

pub enum Attr {
    Align(Align),
    Href(String),
    Anchor(String),
    Width(u32),
    Height(u32),
    Src(String),
    Start(usize),
    Style(String),
    IsCheckbox,
    IsChecked,
}

pub struct StyleIter<'style>(std::str::Split<'style, char>);

impl<'style> StyleIter<'style> {
    pub fn new(style: &'style str) -> Self {
        Self(style.split(';'))
    }
}

impl<'style> Iterator for StyleIter<'style> {
    type Item = Style;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let part = self.0.next()?;

            if let Some(bg_color) = part
                .strip_prefix("background-color:#")
                .and_then(|hex_str| u32::from_str_radix(hex_str, 16).ok())
            {
                return Some(Style::BackgroundColor(bg_color));
            } else if let Some(color) = part
                .strip_prefix("color:#")
                .and_then(|hex_str| u32::from_str_radix(hex_str, 16).ok())
            {
                return Some(Style::Color(color));
            } else if let Some(w) = part.strip_prefix("font-weight:").and_then(FontWeight::new) {
                return Some(Style::FontWeight(w));
            } else if let Some(s) = part.strip_prefix("font-style:").and_then(FontStyle::new) {
                return Some(Style::FontStyle(s));
            }
        }
    }
}

pub enum Style {
    BackgroundColor(u32),
    Color(u32),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
}

#[derive(Default, PartialEq, Eq)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

impl FontWeight {
    pub fn new(s: &str) -> Option<Self> {
        match s {
            "bold" => Some(Self::Bold),
            _ => None,
        }
    }
}

#[derive(Default, PartialEq, Eq)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

impl FontStyle {
    pub fn new(s: &str) -> Option<Self> {
        match s {
            "italic" => Some(Self::Italic),
            _ => None,
        }
    }
}

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
