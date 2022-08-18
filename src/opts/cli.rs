use std::{env, ffi::OsString, path::PathBuf};

use crate::color::{self, Theme};

use super::{config::Config, ThemeType};

use clap::{command, value_parser, Arg, Command, PossibleValue, ValueEnum};

impl ThemeType {
    pub fn as_theme(&self) -> Theme {
        match &self {
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

impl ValueEnum for ThemeType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Dark, Self::Light]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue<'a>> {
        Some(PossibleValue::new(self.as_str()))
    }
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub file_path: PathBuf,
    pub theme: Option<ThemeType>,
    pub scale: Option<f32>,
}

pub fn command(scale_help: &str, default_theme: ThemeType) -> Command {
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
    pub fn parse_from(args: Vec<OsString>, config: &Config) -> Self {
        let scale_help = format!(
            "Factor to scale rendered file by [default: {}]",
            match config.scale {
                Some(scale) => scale.to_string(),
                None => String::from("Window's scale factor"),
            }
        );

        let command = command(&scale_help, config.theme);
        let matches = command.get_matches_from(args);

        let file_path = matches.get_one("file").cloned().expect("required");
        let theme = matches.get_one("theme").cloned();
        let scale = matches.get_one("scale").cloned();

        Self {
            file_path,
            theme,
            scale,
        }
    }
}
