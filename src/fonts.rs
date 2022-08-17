use anyhow::Context;
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;
use wgpu_glyph::ab_glyph::{FontArc, FontVec};

use std::error::Error;
use std::fmt;

use crate::opts::FontOptions;

#[derive(Debug)]
pub enum FontError {
    CopyingFontData,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            FontError::CopyingFontData => write!(f, "Error copying font data"),
        }
    }
}

impl Error for FontError {}

pub fn get_fonts(font_opts: FontOptions) -> anyhow::Result<Vec<FontArc>> {
    let regular = font_opts
        .regular_font
        .map(|font| [FamilyName::Title(font)])
        .unwrap_or([FamilyName::SansSerif]);
    let monospace = font_opts
        .monospace_font
        .map(|font| [FamilyName::Title(font)])
        .unwrap_or([FamilyName::Monospace]);
    get_family_fonts(&regular, &monospace)
}

pub fn get_family_fonts(
    regular: &[FamilyName],
    monospace: &[FamilyName],
) -> anyhow::Result<Vec<FontArc>> {
    let font_source = SystemSource::new();
    let default_text_reg = font_source
        .select_best_match(regular, Properties::new().style(Style::Normal))
        .with_context(|| "No font found for regular font")?;
    let default_text_reg_italic = font_source
        .select_best_match(regular, Properties::new().style(Style::Italic))
        .with_context(|| "No font found for regular font with italics")?;
    let default_text_bold = font_source
        .select_best_match(regular, Properties::new().weight(Weight::BOLD))
        .with_context(|| "No font found for regular font in bold")?;
    let default_text_bold_italic = font_source
        .select_best_match(
            regular,
            Properties::new().weight(Weight::BOLD).style(Style::Italic),
        )
        .with_context(|| "No font found for regular font in bold with italics")?;
    let monospace_text_reg = font_source
        .select_best_match(monospace, Properties::new().style(Style::Normal))
        .with_context(|| "No font found for monospace font")?;
    let monospace_text_reg_italic = font_source
        .select_best_match(monospace, Properties::new().style(Style::Italic))
        .with_context(|| "No font found for monospace font with italics")?;
    let monospace_text_bold = font_source
        .select_best_match(monospace, Properties::new().weight(Weight::BOLD))
        .with_context(|| "No font found for monospace font in bold")?;
    let monospace_text_bold_italic = font_source
        .select_best_match(
            monospace,
            Properties::new().weight(Weight::BOLD).style(Style::Italic),
        )
        .with_context(|| "No font found for monospace font in bold with italics")?;
    let fonts = [
        default_text_reg,
        default_text_reg_italic,
        default_text_bold,
        default_text_bold_italic,
        monospace_text_reg,
        monospace_text_reg_italic,
        monospace_text_bold,
        monospace_text_bold_italic,
    ];
    fonts
        .iter()
        .map(|font| load_handle(font).map(|font_vec| font_vec.into()))
        .collect::<anyhow::Result<Vec<FontArc>>>()
}

pub fn load_handle(handle: &Handle) -> anyhow::Result<FontVec> {
    match handle {
        Handle::Path { path, font_index } => {
            let buffer = std::fs::read(path)?;
            Ok(FontVec::try_from_vec_and_index(buffer, *font_index)?)
        }
        Handle::Memory { bytes, font_index } => Ok(FontVec::try_from_vec_and_index(
            bytes.to_vec(),
            *font_index,
        )?),
    }
}
