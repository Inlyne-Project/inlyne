mod cli;
mod config;
#[cfg(test)]
mod tests;

use std::{env, ffi::OsString, path::PathBuf};

use crate::color;

use serde::Deserialize;

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
    pub font_opts: FontOptions,
}

impl Opts {
    pub fn parse_and_load() -> Self {
        let args = env::args_os().collect();
        let config = match config::Config::load() {
            Ok(config) => config,
            Err(err) => {
                // TODO: switch to logging
                eprintln!(
                    "WARN: Failed reading config file. Falling back to defaults. Error: {}",
                    err
                );
                config::Config::default()
            }
        };

        Self::parse_and_load_from(args, config)
    }

    fn parse_and_load_from(args: Vec<OsString>, config: config::Config) -> Self {
        let cli::Args {
            file_path,
            theme: args_theme,
            scale: args_scale,
        } = cli::Args::parse_from(args, &config);
        let config::Config {
            theme: config_theme,
            scale: config_scale,
            light_theme: config_light_theme,
            dark_theme: config_dark_theme,
            font_options: config_font_options,
        } = config;

        let theme = match args_theme.unwrap_or(config_theme) {
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
            file_path,
            theme,
            scale: args_scale.or(config_scale),
            font_opts,
        }
    }
}
