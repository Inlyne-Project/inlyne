use super::ThemeType;
use crate::{color, keybindings::Keybindings};

use anyhow::Context;
use serde::Deserialize;

#[derive(Deserialize, Debug, PartialEq, Eq, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct FontOptions {
    #[serde(default)]
    pub regular_font: Option<String>,
    #[serde(default)]
    pub monospace_font: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct OptionalTheme {
    #[serde(default)]
    pub text_color: Option<u32>,
    #[serde(default)]
    pub background_color: Option<u32>,
    #[serde(default)]
    pub code_color: Option<u32>,
    #[serde(default)]
    pub code_block_color: Option<u32>,
    #[serde(default)]
    pub quote_block_color: Option<u32>,
    #[serde(default)]
    pub link_color: Option<u32>,
    #[serde(default)]
    pub select_color: Option<u32>,
    #[serde(default)]
    pub checkbox_color: Option<u32>,
    #[serde(default)]
    pub code_highlighter: Option<color::SyntaxTheme>,
}

impl OptionalTheme {
    pub fn merge(self, other: color::Theme) -> color::Theme {
        color::Theme {
            text_color: self.text_color.unwrap_or(other.text_color),
            background_color: self.background_color.unwrap_or(other.background_color),
            code_color: self.code_color.unwrap_or(other.code_color),
            code_block_color: self.code_block_color.unwrap_or(other.code_block_color),
            quote_block_color: self.quote_block_color.unwrap_or(other.quote_block_color),
            link_color: self.link_color.unwrap_or(other.link_color),
            select_color: self.select_color.unwrap_or(other.select_color),
            checkbox_color: self.checkbox_color.unwrap_or(other.checkbox_color),
            code_highlighter: self.code_highlighter.unwrap_or(other.code_highlighter),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct LinesToScroll(pub f32);

impl Default for LinesToScroll {
    fn default() -> Self {
        Self(3.0)
    }
}

#[derive(Deserialize, Debug, Default)]
pub struct KeybindingsSection {
    pub base: Option<Keybindings>,
    pub extra: Option<Keybindings>,
}

#[derive(Deserialize, Debug, Default)]
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
    pub async fn load() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir().context("Failed detecting config dir")?;
        let config_path = config_dir.join("inlyne").join("inlyne.toml");
        if config_path.is_file() {
            let text = tokio::fs::read_to_string(&config_path)
                .await
                .context("Failed reading config file")?;
            let config = toml::from_str(&text)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
}
