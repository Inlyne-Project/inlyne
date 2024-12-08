//! A whole load of custom debug impls to keep the output more succinct
//!
//! Mostly to reduce noise for snapshot tests, but also good in general

use std::fmt;

use crate::positioner::Spacer;
use crate::text::Text;

use glyphon::FamilyOwned;

pub struct DebugInlineMaybeF32Color<'a>(pub &'a Option<[f32; 4]>);

impl fmt::Debug for DebugInlineMaybeF32Color<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => f.write_str("None"),
            Some(rgba) => f.write_fmt(format_args!("Some({:?})", DebugF32Color(*rgba))),
        }
    }
}

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

pub struct DebugInline<'inner, T>(pub &'inner T);

impl<T: fmt::Debug> fmt::Debug for DebugInline<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self.0))
    }
}

fn debug_inline_some<T: fmt::Debug>(
    debug: &mut fmt::DebugStruct<'_, '_>,
    name: &'static str,
    maybe_t: &Option<T>,
) {
    if maybe_t.is_some() {
        debug.field(name, &DebugInline(maybe_t));
    }
}

pub struct DebugBytesPrefix<'a>(pub &'a [u8]);

impl fmt::Debug for DebugBytesPrefix<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            [x, y, z, _, ..] => {
                let len = self.0.len();
                f.write_fmt(format_args!("{{ len: {len}, data: [{x}, {y}, {z}, ..] }}"))
            }
            three_or_less => f.write_fmt(format_args!("{three_or_less:?}")),
        }
    }
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
        debug.field("color", &DebugInline(&color));
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
    debug_inline_some(&mut debug, "link", link);

    debug.finish_non_exhaustive()
}

pub fn spacer(spacer: &Spacer, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Spacer { space, visible } = spacer;

    if *visible {
        f.write_fmt(format_args!("VisibleSpacer({space})"))
    } else {
        f.write_fmt(format_args!("InvisibleSpacer({space})"))
    }
}
