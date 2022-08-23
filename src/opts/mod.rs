mod cli;
mod config;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::color;

use serde::Deserialize;

pub use self::cli::Args;
pub use self::config::Config;
pub use self::config::FontOptions;

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum ThemeType {
    Dark,
    #[default]
    Light,
}

#[derive(Debug, PartialEq)]
pub struct Opts {
    pub file_path: PathBuf,
    pub theme: color::Theme,
    pub scale: Option<f32>,
    pub lines_to_scroll: f32,
    pub font_opts: FontOptions,
}

impl Opts {
    pub fn parse_and_load_from(args: &Args, config: config::Config) -> Self {
        let config::Config {
            theme: config_theme,
            scale: config_scale,
            lines_to_scroll: config_lines_to_scroll,
            light_theme: config_light_theme,
            dark_theme: config_dark_theme,
            font_options: config_font_options,
        } = config;

        let theme = match args.theme.unwrap_or(config_theme) {
            ThemeType::Dark => match config_dark_theme {
                Some(config_dark_theme) => config_dark_theme.merge(color::DARK_DEFAULT),
                None => color::DARK_DEFAULT,
            },
            ThemeType::Light => match config_light_theme {
                Some(config_light_theme) => config_light_theme.merge(color::LIGHT_DEFAULT),
                None => color::LIGHT_DEFAULT,
            },
        };

        let font_opts = config_font_options.unwrap_or_default();

        Self {
            file_path: args.file_path.clone(),
            theme,
            scale: args.scale.or(config_scale),
            lines_to_scroll: config_lines_to_scroll.0,
            font_opts,
        }
    }
}
