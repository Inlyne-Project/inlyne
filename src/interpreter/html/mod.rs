pub mod attr;
pub mod picture;
pub mod style;
mod tag_name;

pub use attr::Attr;
pub use picture::Picture;
pub use tag_name::TagName;

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
    // https://html.spec.whatwg.org/multipage/rendering.html#sections-and-headings
    pub fn size_multiplier(&self) -> f32 {
        match self {
            HeaderType::H1 => 2.0,
            HeaderType::H2 => 1.5,
            HeaderType::H3 => 1.17,
            HeaderType::H4 => 1.0,
            HeaderType::H5 => 0.83,
            HeaderType::H6 => 0.67,
        }
    }
}
