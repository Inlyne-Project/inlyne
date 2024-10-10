use std::slice;

use crate::{image::Px, opts::ResolvedTheme, utils::Align};

use html5ever::{local_name, Attribute};

pub struct Iter<'attrs>(slice::Iter<'attrs, Attribute>);

impl<'attrs> Iter<'attrs> {
    pub fn new(attrs: &'attrs [Attribute]) -> Self {
        Self(attrs.iter())
    }
}

impl<'attrs> Iterator for Iter<'attrs> {
    type Item = Attr;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Attribute { name, value } = self.0.next()?;
            let attr = match name.local {
                local_name!("align") => Align::new(value).map(Attr::Align),
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
                local_name!("media") => PrefersColorScheme::new(value).map(Attr::Media),
                local_name!("srcset") => Some(Attr::SrcSet(value.to_string())),
                _ => continue,
            };

            if attr.is_some() {
                break attr;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Attr {
    Align(Align),
    Href(String),
    Anchor(String),
    Width(Px),
    Height(Px),
    Src(String),
    Start(usize),
    Style(String),
    IsCheckbox,
    IsChecked,
    Media(PrefersColorScheme),
    SrcSet(String),
}

impl Attr {
    pub fn to_style(&self) -> Option<String> {
        if let Self::Style(style) = self {
            Some(style.to_owned())
        } else {
            None
        }
    }
    pub fn to_align(&self) -> Option<Align> {
        if let Self::Align(align) = self {
            Some(align.to_owned())
        } else {
            None
        }
    }
    pub fn to_anchor(&self) -> Option<String> {
        if let Self::Anchor(name) = self {
            Some(name.to_owned())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PrefersColorScheme(pub ResolvedTheme);

impl PrefersColorScheme {
    pub fn new(s: &str) -> Option<Self> {
        match s {
            "(prefers-color-scheme: dark)" => Some(Self(ResolvedTheme::Dark)),
            "(prefers-color-scheme: light)" => Some(Self(ResolvedTheme::Light)),
            _ => None,
        }
    }
}
