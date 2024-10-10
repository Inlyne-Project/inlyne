pub struct Iter<'style>(std::str::Split<'style, char>);

impl<'style> Iter<'style> {
    pub fn new(style: &'style str) -> Self {
        Self(style.split(';'))
    }
}

impl<'style> Iterator for Iter<'style> {
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
            } else if let Some(d) = part
                .strip_prefix("text-decoration:")
                .and_then(TextDecoration::new)
            {
                return Some(Style::TextDecoration(d));
            }
        }
    }
}

pub enum Style {
    BackgroundColor(u32),
    Color(u32),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    TextDecoration(TextDecoration),
}

#[derive(Default, PartialEq, Eq, Copy, Clone, Debug)]
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

#[derive(Default, PartialEq, Eq, Copy, Clone, Debug)]
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

#[derive(Default, PartialEq, Eq, Copy, Clone, Debug)]
pub enum TextDecoration {
    #[default]
    Normal,
    Underline,
}

impl TextDecoration {
    pub fn new(s: &str) -> Option<Self> {
        match s {
            "underline" => Some(Self::Underline),
            _ => None,
        }
    }
}
