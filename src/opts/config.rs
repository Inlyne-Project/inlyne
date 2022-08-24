use std::fs;

use super::ThemeType;
use crate::color;

use anyhow::Context;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug, PartialEq, Default, Clone)]
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
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub text_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub background_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub code_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub code_block_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub quote_block_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub link_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub select_color: Option<[f32; 4]>,
    #[serde(default, deserialize_with = "deserialize_hex_to_linear_rgba")]
    pub checkbox_color: Option<[f32; 4]>,
    #[serde(default)]
    pub code_highlighter: Option<color::SyntaxTheme>,
}

fn deserialize_hex_to_linear_rgba<'de, D>(deserializer: D) -> Result<Option<[f32; 4]>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_hex = <Option<u32>>::deserialize(deserializer)?;
    Ok(maybe_hex.map(color::hex_to_linear_rgba))
}

impl OptionalTheme {
    pub fn merge(self, other: color::Theme) -> color::Theme {
        color::Theme {
            text_color: self.text_color.unwrap_or(other.text_color),
            background_color: self
                .background_color
                .map(|[r, g, b, a]| wgpu::Color {
                    r: r as f64,
                    g: g as f64,
                    b: b as f64,
                    a: a as f64,
                })
                .unwrap_or(other.background_color),
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
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default)]
    pub theme: ThemeType,
    pub scale: Option<f32>,
    #[serde(default)]
    pub lines_to_scroll: LinesToScroll,
    pub light_theme: Option<OptionalTheme>,
    pub dark_theme: Option<OptionalTheme>,
    pub font_options: Option<FontOptions>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir().context("Failed detecting config dir")?;
        let config_path = config_dir.join("inlyne").join("inlyne.toml");
        if config_path.is_file() {
            let text = fs::read_to_string(&config_path).context("Failed reading config file")?;
            let config = toml::from_str(&text)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
}
