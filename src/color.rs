use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::Context;
use serde::Deserialize;
use syntect::highlighting::{
    Color as SyntectColor, Theme as SyntectTheme, ThemeSet as SyntectThemeSet,
};
use two_face::theme::EmbeddedThemeName;
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
        static CACHED_CODE_HIGHLIGHTER: OnceLock<SyntectTheme> = OnceLock::new();
        // Initializing this is non-trivial. Cache so it only runs once
        let code_highlighter = CACHED_CODE_HIGHLIGHTER
            .get_or_init(|| ThemeDefaults::Base16OceanDark.into())
            .to_owned();
        Self {
            text_color: 0x9DACBB,
            background_color: 0x1A1D22,
            code_color: 0xB38FAC,
            quote_block_color: 0x1D2025,
            link_color: 0x4182EB,
            select_color: 0x3675CB,
            checkbox_color: 0x0A5301,
            code_highlighter,
        }
    }

    pub fn light_default() -> Self {
        static CACHED_CODE_HIGHLIGHTER: OnceLock<SyntectTheme> = OnceLock::new();
        // Initializing this is non-trivial. Cache so it only runs once
        let code_highlighter = CACHED_CODE_HIGHLIGHTER
            .get_or_init(|| ThemeDefaults::Github.into())
            .to_owned();
        Self {
            text_color: 0x000000,
            background_color: 0xFFFFFF,
            code_color: 0x95114E,
            quote_block_color: 0xEEF9FE,
            link_color: 0x5466FF,
            select_color: 0xCDE8F0,
            checkbox_color: 0x96ECAE,
            code_highlighter,
        }
    }

    pub fn code_highlighter(mut self, theme: SyntectTheme) -> Self {
        self.code_highlighter = theme;
        self
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
            // multiple times. We can't so now we have to fall back to other means to give a good
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
    Base16EightiesDark,
    Base16MochaDark,
    Base16OceanDark,
    Base16OceanLight,
    ColdarkCold,
    ColdarkDark,
    DarkNeon,
    Dracula,
    Github,
    GruvboxDark,
    GruvboxLight,
    Leet,
    MonokaiExtended,
    MonokaiExtendedLight,
    Nord,
    OneHalfDark,
    OneHalfLight,
    SolarizedDark,
    SolarizedLight,
    SublimeSnazzy,
    TwoDark,
    VisualStudioDarkPlus,
    Zenburn,
}

impl ThemeDefaults {
    fn kebab_pairs() -> &'static [(&'static str, Self)] {
        &[
            ("base16-eighties-dark", Self::Base16EightiesDark),
            ("base16-mocha-dark", Self::Base16MochaDark),
            ("base16-ocean-dark", Self::Base16OceanDark),
            ("base16-ocean-light", Self::Base16OceanLight),
            ("coldark-cold", Self::ColdarkCold),
            ("coldark-dark", Self::ColdarkDark),
            ("dark-neon", Self::DarkNeon),
            ("dracula", Self::Dracula),
            ("github", Self::Github),
            ("gruvbox-dark", Self::GruvboxDark),
            ("gruvbox-light", Self::GruvboxLight),
            ("leet", Self::Leet),
            ("monokai-extended", Self::MonokaiExtended),
            ("monokai-extended-light", Self::MonokaiExtendedLight),
            ("nord", Self::Nord),
            ("one-half-dark", Self::OneHalfDark),
            ("one-half-light", Self::OneHalfLight),
            ("solarized-dark", Self::SolarizedDark),
            ("solarized-light", Self::SolarizedLight),
            ("sublime-snazzy", Self::SublimeSnazzy),
            ("two-dark", Self::TwoDark),
            ("visual-studio-dark-plus", Self::VisualStudioDarkPlus),
            ("zenburn", Self::Zenburn),
        ]
    }

    fn from_kebab(kebab: &str) -> Option<Self> {
        Self::kebab_pairs()
            .iter()
            .find_map(|&(hay, var)| (kebab == hay).then_some(var))
    }

    pub fn as_syntect_name(self) -> &'static str {
        EmbeddedThemeName::from(self).as_name()
    }
}

impl From<ThemeDefaults> for EmbeddedThemeName {
    fn from(default: ThemeDefaults) -> Self {
        match default {
            ThemeDefaults::Base16EightiesDark => Self::Base16EightiesDark,
            ThemeDefaults::Base16MochaDark => Self::Base16MochaDark,
            ThemeDefaults::Base16OceanDark => Self::Base16OceanDark,
            ThemeDefaults::Base16OceanLight => Self::Base16OceanLight,
            ThemeDefaults::ColdarkCold => Self::ColdarkCold,
            ThemeDefaults::ColdarkDark => Self::ColdarkDark,
            ThemeDefaults::DarkNeon => Self::DarkNeon,
            ThemeDefaults::Dracula => Self::Dracula,
            ThemeDefaults::Github => Self::Github,
            ThemeDefaults::GruvboxDark => Self::GruvboxDark,
            ThemeDefaults::GruvboxLight => Self::GruvboxLight,
            ThemeDefaults::Leet => Self::Leet,
            ThemeDefaults::MonokaiExtended => Self::MonokaiExtended,
            ThemeDefaults::MonokaiExtendedLight => Self::MonokaiExtendedLight,
            ThemeDefaults::Nord => Self::Nord,
            ThemeDefaults::OneHalfDark => Self::OneHalfDark,
            ThemeDefaults::OneHalfLight => Self::OneHalfLight,
            ThemeDefaults::SolarizedDark => Self::SolarizedDark,
            ThemeDefaults::SolarizedLight => Self::SolarizedLight,
            ThemeDefaults::SublimeSnazzy => Self::SublimeSnazzy,
            ThemeDefaults::TwoDark => Self::TwoDark,
            ThemeDefaults::VisualStudioDarkPlus => Self::VisualStudioDarkPlus,
            ThemeDefaults::Zenburn => Self::Zenburn,
        }
    }
}

impl From<ThemeDefaults> for SyntectTheme {
    fn from(default: ThemeDefaults) -> Self {
        let default_themes = two_face::theme::extra();
        let mut theme = default_themes.get(default.into()).to_owned();

        // Github's background color is 0xfff which is the same as the default light theme
        // background. We match GitHub's website light theme code blocks instead to distinguish
        // code blocks from the background
        if default == ThemeDefaults::Github {
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
