use serde::Deserialize;
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
    pub code_block_color: u32,
    pub quote_block_color: u32,
    pub link_color: u32,
    pub select_color: u32,
    pub checkbox_color: u32,
    pub code_highlighter: SyntaxTheme,
}

pub const DARK_DEFAULT: Theme = Theme {
    text_color: 0x9DACBB,
    background_color: 0x1A1D22,
    code_color: 0xB38FAC,
    code_block_color: 0x181C21,
    quote_block_color: 0x1D2025,
    link_color: 0x4182EB,
    select_color: 0x3675CB,
    checkbox_color: 0x0A5301,
    code_highlighter: SyntaxTheme::Base16OceanDark,
};

pub const LIGHT_DEFAULT: Theme = Theme {
    text_color: 0x000000,
    background_color: 0xFFFFFF,
    code_color: 0x95114E,
    code_block_color: 0xEAEDF3,
    quote_block_color: 0xEEF9FE,
    link_color: 0x5466FF,
    select_color: 0xCDE8F0,
    checkbox_color: 0x96ECAE,
    code_highlighter: SyntaxTheme::InspiredGithub,
};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SyntaxTheme {
    Base16OceanDark,
    Base16EightiesDark,
    Base16MochaDark,
    Base16OceanLight,
    InspiredGithub,
    SolarizedDark,
    SolarizedLight,
}

impl SyntaxTheme {
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
