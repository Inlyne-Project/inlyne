use super::HeaderType;

use html5ever::{local_name, LocalNameStaticSet};
use string_cache::Atom;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagName {
    Anchor,
    BlockQuote,
    BoldOrStrong,
    Break,
    Code,
    Details,
    Div,
    EmphasisOrItalic,
    Header(HeaderType),
    HorizontalRuler,
    Picture,
    Source,
    Image,
    Input,
    ListItem,
    OrderedList,
    Paragraph,
    PreformattedText,
    Section,
    Small,
    Span,
    Strikethrough,
    Summary,
    Table,
    TableBody,
    TableDataCell,
    TableHead,
    TableHeader,
    TableRow,
    Underline,
    UnorderedList,
    Root,
}

impl TryFrom<&Atom<LocalNameStaticSet>> for TagName {
    type Error = Atom<LocalNameStaticSet>;

    fn try_from(atom: &Atom<LocalNameStaticSet>) -> Result<Self, Self::Error> {
        let tag_name = match atom {
            &local_name!("a") => Self::Anchor,
            &local_name!("blockquote") => Self::BlockQuote,
            &local_name!("b") | &local_name!("strong") => Self::BoldOrStrong,
            &local_name!("br") => Self::Break,
            &local_name!("code") | &local_name!("kbd") => Self::Code,
            &local_name!("details") => Self::Details,
            &local_name!("div") => Self::Div,
            &local_name!("em") | &local_name!("i") => Self::EmphasisOrItalic,
            &local_name!("h1") => Self::Header(HeaderType::H1),
            &local_name!("h2") => Self::Header(HeaderType::H2),
            &local_name!("h3") => Self::Header(HeaderType::H3),
            &local_name!("h4") => Self::Header(HeaderType::H4),
            &local_name!("h5") => Self::Header(HeaderType::H5),
            &local_name!("h6") => Self::Header(HeaderType::H6),
            &local_name!("hr") => Self::HorizontalRuler,
            &local_name!("picture") => Self::Picture,
            &local_name!("source") => Self::Source,
            &local_name!("img") => Self::Image,
            &local_name!("input") => Self::Input,
            &local_name!("li") => Self::ListItem,
            &local_name!("ol") => Self::OrderedList,
            &local_name!("p") => Self::Paragraph,
            &local_name!("pre") => Self::PreformattedText,
            &local_name!("section") => Self::Section,
            &local_name!("small") => Self::Small,
            &local_name!("span") => Self::Span,
            &local_name!("s") | &local_name!("del") => Self::Strikethrough,
            &local_name!("summary") => Self::Summary,
            &local_name!("table") => Self::Table,
            &local_name!("tbody") => Self::TableBody,
            &local_name!("td") => Self::TableDataCell,
            &local_name!("th") => Self::TableHeader,
            &local_name!("thead") => Self::TableHead,
            &local_name!("tr") => Self::TableRow,
            &local_name!("u") | &local_name!("ins") => Self::Underline,
            &local_name!("ul") => Self::UnorderedList,
            _ => return Err(atom.to_owned()),
        };

        Ok(tag_name)
    }
}
