use std::fs::read_to_string;
use std::path::Path;

use super::ThemeType;
use crate::{color, keybindings::Keybindings};

use anyhow::Context;
use serde::Deserialize;
use syntect::highlighting::Theme as SyntectTheme;

#[derive(Deserialize, Debug, PartialEq, Eq, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct FontOptions {
    #[serde(default)]
    pub regular_font: Option<String>,
    #[serde(default)]
    pub monospace_font: Option<String>,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct OptionalTheme {
    pub text_color: Option<u32>,
    pub background_color: Option<u32>,
    pub code_color: Option<u32>,
    pub code_block_color: Option<u32>,
    pub quote_block_color: Option<u32>,
    pub link_color: Option<u32>,
    pub select_color: Option<u32>,
    pub checkbox_color: Option<u32>,
    pub code_highlighter: Option<color::SyntaxTheme>,
}

impl OptionalTheme {
    pub fn merge(self, other: color::Theme) -> anyhow::Result<color::Theme> {
        let code_highlighter = match self.code_highlighter {
            Some(theme) => SyntectTheme::try_from(theme)?,
            None => other.code_highlighter,
        };

        Ok(color::Theme {
            text_color: self.text_color.unwrap_or(other.text_color),
            background_color: self.background_color.unwrap_or(other.background_color),
            code_color: self.code_color.unwrap_or(other.code_color),
            code_block_color: self.code_block_color.unwrap_or(other.code_block_color),
            quote_block_color: self.quote_block_color.unwrap_or(other.quote_block_color),
            link_color: self.link_color.unwrap_or(other.link_color),
            select_color: self.select_color.unwrap_or(other.select_color),
            checkbox_color: self.checkbox_color.unwrap_or(other.checkbox_color),
            code_highlighter,
        })
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct LinesToScroll(pub f32);

impl From<LinesToScroll> for f32 {
    fn from(value: LinesToScroll) -> Self {
        value.0
    }
}

impl Default for LinesToScroll {
    fn default() -> Self {
        Self(3.0)
    }
}

#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct KeybindingsSection {
    pub base: Option<Keybindings>,
    pub extra: Option<Keybindings>,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub theme: Option<ThemeType>,
    pub scale: Option<f32>,
    pub page_width: Option<f32>,
    pub lines_to_scroll: LinesToScroll,
    pub light_theme: Option<OptionalTheme>,
    pub dark_theme: Option<OptionalTheme>,
    pub font_options: Option<FontOptions>,
    pub keybindings: KeybindingsSection,
}

impl Config {
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let config_content = read_to_string(path).context(format!(
            "Failed to read configuration file at '{}'",
            path.display()
        ))?;

        Ok(toml::from_str(&config_content)?)
    }

    pub fn load_from_system() -> anyhow::Result<Self> {
        let config_dir =
            dirs::config_dir().context("Failed to find the configuration directory")?;

        let config_path = config_dir.join("inlyne").join("inlyne.toml");

        if !config_path.is_file() {
            return Ok(Self::default());
        }

        Self::load_from_file(&config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_file_is_in_sync() {
        // Load the provided default toml file and compare with what we generate to make sure the
        // defaults stay in sync
        let mut config = Config::load_from_file(Path::new("inlyne.default.toml")).unwrap();

        // Swap out some of the values to compare
        let theme = config.theme.take().unwrap();
        let _ = config.font_options.take();
        let dark_theme = config.dark_theme.take().unwrap();
        let light_theme = config.light_theme.take().unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(theme, ThemeType::Auto);
        assert_eq!(
            dark_theme.merge(color::Theme::dark_default()).unwrap(),
            color::Theme::dark_default()
        );
        assert_eq!(
            light_theme.merge(color::Theme::light_default()).unwrap(),
            color::Theme::light_default()
        );
    }
}
