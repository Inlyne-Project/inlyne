use glyphon::FontSystem;

use crate::opts::FontOptions;

pub fn get_fonts(font_opts: &FontOptions) -> FontSystem {
    let mut font_system = FontSystem::new();

    if let Some(regular_name) = &font_opts.regular_font {
        font_system.db_mut().set_sans_serif_family(regular_name)
    }

    if let Some(monospace_name) = &font_opts.monospace_font {
        font_system.db_mut().set_monospace_family(monospace_name)
    }

    font_system
}
