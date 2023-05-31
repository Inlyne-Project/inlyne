//! A whole load of custom debug impls to keep the output more succinct
//!
//! Mostly to reduce noise for snapshot tests, but also good in general

use std::fmt;

use crate::{
    positioner::Spacer,
    text::{Text, TextBox},
};

use glyphon::FamilyOwned;

pub struct DebugF32Color(pub [f32; 4]);

impl fmt::Debug for DebugF32Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == [0.0, 0.0, 0.0, 1.0] {
            f.write_str("Color(BLACK)")
        } else {
            let Self([r, g, b, a]) = self;

            if *a == 1.0 {
                f.write_fmt(format_args!("Color {{ r: {r:.2}, g: {g:.2}, b: {b:.2} }}"))
            } else {
                f.write_fmt(format_args!(
                    "Color {{ r: {r:.2}, g: {g:.2}, b: {b:.2}, a: {a:.2} }}"
                ))
            }
        }
    }
}

pub struct DebugInlineOption<'inner, T>(pub &'inner Option<T>);

impl<'inner, T: fmt::Debug> fmt::Debug for DebugInlineOption<'inner, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(t) = &self.0 {
            f.write_fmt(format_args!("Some({t:?})"))
        } else {
            f.write_str("None")
        }
    }
}

pub fn text_box(text_box: &TextBox, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let TextBox {
        indent,
        font_size,
        texts,
        is_code_block,
        is_quote_block,
        is_checkbox,
        is_anchor,
        align,
        // Globally consistent so avoid displaying as noise
        hidpi_scale: _,
        padding_height,
        background_color,
    } = text_box;

    let mut debug = f.debug_struct("TextBox");

    let default = TextBox::default();

    // Fields that we only display when set to unique values
    if *font_size != default.font_size {
        debug.field("font_size", font_size);
    }
    if align != &default.align {
        debug.field("align", align);
    }
    if *indent != default.indent {
        debug.field("indent", indent);
    }
    if *padding_height != default.padding_height {
        debug.field("padding_height", padding_height);
    }
    if background_color.is_some() {
        let background_color = background_color.map(DebugF32Color);
        let debug_background_color = DebugInlineOption(&background_color);
        debug.field("background_color", &debug_background_color);
    }
    if *is_code_block {
        debug.field("is_code_block", &true);
    }
    if is_quote_block.is_some() {
        debug.field("is_quote_block", &DebugInlineOption(is_quote_block));
    }
    if is_checkbox.is_some() {
        debug.field("is_checkbox", &DebugInlineOption(is_checkbox));
    }
    if is_anchor.is_some() {
        debug.field("is_anchor", &DebugInlineOption(is_anchor));
    }

    // Texts at the end so all the smaller fields for text box are easily visible
    debug.field("texts", texts);

    debug.finish()
}

pub fn text(text: &Text, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    #[derive(Copy, Clone)]
    struct StyleWrapper {
        is_bold: bool,
        is_italic: bool,
        is_underlined: bool,
        is_striked: bool,
    }

    impl StyleWrapper {
        fn is_regular(self) -> bool {
            let Self {
                is_bold,
                is_italic,
                is_underlined,
                is_striked,
            } = self;

            ![is_bold, is_italic, is_underlined, is_striked].contains(&true)
        }
    }

    impl fmt::Debug for StyleWrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Self {
                is_bold,
                is_italic,
                is_underlined,
                is_striked,
            } = *self;

            if self.is_regular() {
                f.write_str("REGULAR")?;
            } else {
                if is_bold {
                    f.write_str("BOLD ")?;
                }
                if is_italic {
                    f.write_str("ITALIC ")?;
                }
                if is_underlined {
                    f.write_str("UNDERLINED ")?;
                }
                if is_striked {
                    f.write_str("STRIKED ")?;
                }
            }

            Ok(())
        }
    }

    let Text {
        text,
        color,
        link,
        is_bold,
        is_italic,
        is_underlined,
        is_striked,
        font_family,
        // Globally consistent so avoid displaying as noise
        hidpi_scale: _,
        default_color,
    } = text;

    let mut debug = f.debug_struct("Text");

    // Fields that we will always display
    debug.field("text", text);

    // Fields that we only display when set to unique values
    if font_family != &FamilyOwned::SansSerif {
        debug.field("font_family", font_family);
    }
    if color.is_none() {
        debug.field("default_color", &DebugF32Color(*default_color));
    } else {
        let color = color.map(DebugF32Color);
        let debug_color = DebugInlineOption(&color);
        debug.field("color", &debug_color);
    }
    let style = StyleWrapper {
        is_bold: *is_bold,
        is_italic: *is_italic,
        is_underlined: *is_underlined,
        is_striked: *is_striked,
    };
    if !style.is_regular() {
        debug.field("style", &style);
    }
    if link.is_some() {
        debug.field("link", &DebugInlineOption(link));
    }

    debug.finish()
}

pub fn spacer(spacer: &Spacer, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Spacer { space, visible } = spacer;

    if *visible {
        f.write_fmt(format_args!("VisibleSpacer({space})"))
    } else {
        f.write_fmt(format_args!("InvisibleSpacer({space})"))
    }
}
