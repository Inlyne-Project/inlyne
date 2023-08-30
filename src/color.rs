use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::Context;
use serde::Deserialize;
use syntect::highlighting::{
    Color as SyntectColor, Theme as SyntectTheme, ThemeSet as SyntectThemeSet,
};
use wgpu::TextureFormat;

fn hex_to_linear_rgba(c: u32) -> [f32; 4] {
    let f = |xu: u32| {
        let x = (xu & 0xff) as f32 / 255.0;
        if x > 0.04045 {
            ((x + 0.055) / 1.055).powf(2.4)
        } else {
            x / 12.92
        }
    };
    [f(c >> 16), f(c >> 8), f(c), 1.0]
}

pub fn native_color(c: u32, format: &TextureFormat) -> [f32; 4] {
    use wgpu::TextureFormat::*;
    let f = |xu: u32| (xu & 0xff) as f32 / 255.0;

    match format {
        Rgba8UnormSrgb | Bgra8UnormSrgb => hex_to_linear_rgba(c),
        _ => [f(c >> 16), f(c >> 8), f(c), 1.0],
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub text_color: u32,
    pub background_color: u32,
    pub code_color: u32,
    pub quote_block_color: u32,
    pub link_color: u32,
    pub select_color: u32,
    pub checkbox_color: u32,
    pub code_highlighter: SyntectTheme,
}

impl Theme {
    pub fn dark_default() -> Self {
        Self {
            text_color: 0x9DACBB,
            background_color: 0x1A1D22,
            code_color: 0xB38FAC,
            quote_block_color: 0x1D2025,
            link_color: 0x4182EB,
            select_color: 0x3675CB,
            checkbox_color: 0x0A5301,
            code_highlighter: SyntectTheme::from(ThemeDefaults::Base16OceanDark),
        }
    }

    pub fn light_default() -> Self {
        Self {
            text_color: 0x000000,
            background_color: 0xFFFFFF,
            code_color: 0x95114E,
            quote_block_color: 0xEEF9FE,
            link_color: 0x5466FF,
            select_color: 0xCDE8F0,
            checkbox_color: 0x96ECAE,
            code_highlighter: SyntectTheme::from(ThemeDefaults::InspiredGithub),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxTheme {
    Defaults(ThemeDefaults),
    Custom(ThemeCustom),
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ThemeCustom {
    path: PathBuf,
}

impl SyntaxTheme {
    pub fn custom(path: PathBuf) -> Self {
        Self::Custom(ThemeCustom { path })
    }
}

impl TryFrom<SyntaxTheme> for SyntectTheme {
    type Error = anyhow::Error;

    fn try_from(syntax_theme: SyntaxTheme) -> Result<Self, Self::Error> {
        match syntax_theme {
            SyntaxTheme::Defaults(default) => Ok(SyntectTheme::from(default)),
            SyntaxTheme::Custom(ThemeCustom { path }) => {
                let mut reader = BufReader::new(File::open(&path).with_context(|| {
                    format!("Failed opening theme from path {}", path.display())
                })?);
                SyntectThemeSet::load_from_reader(&mut reader)
                    .with_context(|| format!("Failed loading theme from path {}", path.display()))
            }
        }
    }
}

// Give better error messages than regular `#[serde(untagged)]`
impl<'de> Deserialize<'de> for SyntaxTheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Untagged {
            Defaults(String),
            Custom(ThemeCustom),
        }

        let Ok(untagged) = Untagged::deserialize(deserializer) else {
            return Err(serde::de::Error::custom(
                "Expects either a default theme name or a path to a custom theme. E.g.\n\
                default: \"inspired-github\"\n\
                custom:  { path = \"/path/to/custom.tmTheme\" }",
            ));
        };

        match untagged {
            // Unfortunately #[serde(untagged)] uses private internals to reuse a deserializer
            // mutliple times. We can't so now we have to fall back to other means to give a good
            // error message ;-;
            Untagged::Defaults(theme_name) => match ThemeDefaults::from_kebab(&theme_name) {
                Some(theme) => Ok(Self::Defaults(theme)),
                None => {
                    let variants = ThemeDefaults::kebab_pairs()
                        .iter()
                        .map(|(kebab, _)| format!("\"{kebab}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let msg = format!(
                        "\"{theme_name}\" didn't match any of the expected variants: [{variants}]"
                    );
                    Err(serde::de::Error::custom(msg))
                }
            },
            Untagged::Custom(custom) => Ok(Self::Custom(custom)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeDefaults {
    Base16OceanDark,
    Base16EightiesDark,
    Base16MochaDark,
    Base16OceanLight,
    InspiredGithub,
    SolarizedDark,
    SolarizedLight,
}

impl ThemeDefaults {
    fn kebab_pairs() -> &'static [(&'static str, Self)] {
        &[
            ("base16-ocean-dark", Self::Base16OceanDark),
            ("base16-eighties-dark", Self::Base16EightiesDark),
            ("base16-mocha-dark", Self::Base16MochaDark),
            ("base16-ocean-light", Self::Base16OceanLight),
            ("inspired-github", Self::InspiredGithub),
            ("solarized-dark", Self::SolarizedDark),
            ("solarized-light", Self::SolarizedLight),
        ]
    }

    fn from_kebab(kebab: &str) -> Option<Self> {
        Self::kebab_pairs()
            .iter()
            .find_map(|&(hay, var)| (kebab == hay).then_some(var))
    }

    pub fn as_syntect_name(self) -> &'static str {
        match self {
            Self::Base16OceanDark => "base16-ocean.dark",
            Self::Base16EightiesDark => "base16-eighties.dark",
            Self::Base16MochaDark => "base16-mocha.dark",
            Self::Base16OceanLight => "base16-ocean.light",
            Self::InspiredGithub => "InspiredGitHub",
            Self::SolarizedDark => "Solarized (dark)",
            Self::SolarizedLight => "Solarized (light)",
        }
    }
}

impl From<ThemeDefaults> for SyntectTheme {
    fn from(default: ThemeDefaults) -> Self {
        let mut default_themes = SyntectThemeSet::load_defaults();
        let mut theme = default_themes
            .themes
            .remove(default.as_syntect_name())
            .expect("Included with defaults");

        // InspiredGitHub's background color is 0xfff which is the same as the default light theme
        // background. We match GitHub's light theme code blocks instead to distinguish code blocks
        // from the background
        if default == ThemeDefaults::InspiredGithub {
            theme.settings.background = Some(SyntectColor {
                r: 0xf6,
                g: 0xf8,
                b: 0xfa,
                a: u8::MAX,
            });
        }

        theme
    }
}
