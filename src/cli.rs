use std::{env, ffi::OsString, fs, path::PathBuf};

use crate::color;

use anyhow::Context;
use clap::{command, value_parser, Arg, Command, PossibleValue, ValueEnum};
use serde::Deserialize;

#[derive(Deserialize, Clone, Copy, Debug)]
enum ThemeType {
    Dark,
    Light,
}

impl ThemeType {
    pub fn as_theme(&self) -> color::Theme {
        match self {
            Self::Dark => color::DARK_DEFAULT,
            Self::Light => color::LIGHT_DEFAULT,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

impl Default for ThemeType {
    fn default() -> Self {
        Self::Light
    }
}

impl ValueEnum for ThemeType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Dark, Self::Light]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue<'a>> {
        Some(PossibleValue::new(self.as_str()))
    }
}

#[derive(Deserialize, Default)]
pub struct Config {
    theme: Option<ThemeType>,
    scale: Option<f32>,
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

#[derive(Debug, PartialEq)]
pub struct Args {
    pub file_path: PathBuf,
    pub theme: color::Theme,
    pub scale: Option<f32>,
}

fn command<'a>(scale_help: &'a str, default_theme: ThemeType) -> Command<'a> {
    let file_arg = Arg::new("file")
        .required(true)
        .takes_value(true)
        .value_name("FILE")
        .value_parser(value_parser!(PathBuf))
        .help("Path to the markdown file");
    let theme_arg = Arg::new("theme")
        .short('t')
        .long("theme")
        .takes_value(true)
        .value_parser(value_parser!(ThemeType))
        .default_value(default_theme.as_str())
        .help("Theme to use when rendering");

    let scale_arg = Arg::new("scale")
        .short('s')
        .long("scale")
        .takes_value(true)
        .value_parser(value_parser!(f32))
        .help(scale_help);

    command!().arg(file_arg).arg(theme_arg).arg(scale_arg)
}

impl Args {
    pub fn parse() -> Self {
        let args = env::args_os().collect();
        let config = match Config::load() {
            Ok(config) => config,
            Err(err) => {
                // TODO: switch to logging
                eprintln!(
                    "WARN: Failed reading config file. Falling back to defaults. Error: {}",
                    err
                );
                Config::default()
            }
        };

        Self::parse_from(args, config)
    }

    fn parse_from(args: Vec<OsString>, config: Config) -> Self {
        let scale_help = format!(
            "Factor to scale rendered file by [default: {}]",
            match config.scale {
                Some(scale) => scale.to_string(),
                None => String::from("Window's scale factor"),
            }
        );

        let command = command(&scale_help, config.theme.unwrap_or_default());
        let matches = command.get_matches_from(args);

        let file_path = matches.get_one("file").cloned().expect("required");
        let theme = matches
            .get_one::<ThemeType>("theme")
            .cloned()
            .unwrap_or_default()
            .as_theme();
        let scale = matches.get_one("scale").cloned().or(config.scale);

        Self {
            file_path,
            theme,
            scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_args(args: Vec<&str>) -> Vec<OsString> {
        std::iter::once("inlyne")
            .chain(args.into_iter())
            .map(OsString::from)
            .collect()
    }

    #[test]
    fn debug_assert() {
        command("1.5", ThemeType::Dark).debug_assert();
        command("Window's scale factor", ThemeType::Dark).debug_assert();
    }

    #[test]
    fn defaults() {
        assert_eq!(
            Args::parse_from(gen_args(vec!["file.md"]), Config::default()),
            Args {
                file_path: PathBuf::from("file.md"),
                theme: ThemeType::default().as_theme(),
                scale: None,
            }
        );
    }

    #[test]
    fn config_overrides_default() {
        assert_eq!(
            Args::parse_from(
                gen_args(vec!["file.md"]),
                Config {
                    theme: Some(ThemeType::Dark),
                    scale: None,
                }
            ),
            Args {
                file_path: PathBuf::from("file.md"),
                theme: ThemeType::Dark.as_theme(),
                scale: None,
            }
        );
        assert_eq!(
            Args::parse_from(
                gen_args(vec!["file.md"]),
                Config {
                    theme: None,
                    scale: Some(1.5),
                }
            ),
            Args {
                file_path: PathBuf::from("file.md"),
                theme: ThemeType::default().as_theme(),
                scale: Some(1.5),
            }
        );
    }

    #[test]
    fn from_cli() {
        assert_eq!(
            Args::parse_from(
                gen_args(vec!["--theme", "dark", "file.md"]),
                Config::default()
            ),
            Args {
                file_path: PathBuf::from("file.md"),
                theme: ThemeType::Dark.as_theme(),
                scale: None,
            }
        );

        // CLI takes precedence over config
        assert_eq!(
            Args::parse_from(
                gen_args(vec!["--scale", "1.5", "file.md"]),
                Config {
                    theme: Some(ThemeType::Dark),
                    scale: Some(0.1)
                },
            ),
            Args {
                file_path: PathBuf::from("file.md"),
                theme: ThemeType::Dark.as_theme(),
                scale: Some(1.5),
            }
        );
    }
}
