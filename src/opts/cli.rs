use std::{env, ffi::OsString, path::PathBuf};

use super::{config::Config, ThemeType};

use clap::builder::PossibleValue;
use clap::{command, value_parser, Arg, Command, ValueEnum};

impl ThemeType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

impl ValueEnum for ThemeType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Auto, Self::Dark, Self::Light]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.as_str()))
    }
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Args {
    pub file_path: PathBuf,
    pub theme: Option<ThemeType>,
    pub scale: Option<f32>,
    pub page_width: Option<f32>,
}

pub fn command(scale_help: String, default_theme: Option<ThemeType>) -> Command {
    let file_arg = Arg::new("file")
        .required(true)
        .number_of_values(1)
        .value_name("FILE")
        .value_parser(value_parser!(PathBuf))
        .help("Path to the markdown file");
    let mut theme_arg = Arg::new("theme")
        .short('t')
        .long("theme")
        .number_of_values(1)
        .value_parser(value_parser!(ThemeType))
        .help("Theme to use when rendering");
    if let Some(theme) = default_theme {
        theme_arg = theme_arg.default_value(theme.as_str());
    }

    let scale_arg = Arg::new("scale")
        .short('s')
        .long("scale")
        .number_of_values(1)
        .value_parser(value_parser!(f32))
        .help(scale_help);

    let page_width_arg = Arg::new("page_width")
        .short('w')
        .long("page width")
        .number_of_values(1)
        .value_parser(value_parser!(f32))
        .help("Maximum width of page in pixels");

    command!()
        .arg(file_arg)
        .arg(theme_arg)
        .arg(scale_arg)
        .arg(page_width_arg)
}

impl Args {
    pub fn new(config: &Config) -> Self {
        let program_args = std::env::args_os().collect();
        Self::parse_from(program_args, config)
    }

    pub fn program_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        args.push(self.file_path.as_os_str().to_str().unwrap().to_string());
        if let Some(theme) = self.theme {
            args.push("--theme".to_owned());
            args.push(theme.as_str().to_owned());
        }
        if let Some(scale) = self.scale {
            args.push("--scale".to_owned());
            args.push(scale.to_string());
        }
        args
    }

    pub fn parse_from(args: Vec<OsString>, config: &Config) -> Self {
        let scale_help = format!(
            "Factor to scale rendered file by [default: {}]",
            match config.scale {
                Some(scale) => scale.to_string(),
                None => String::from("Window's scale factor"),
            }
        );

        let command = command(scale_help, config.theme);
        let matches = command.get_matches_from(args);

        let file_path = matches.get_one("file").cloned().expect("required");
        let theme = matches.get_one("theme").cloned();
        let scale = matches.get_one("scale").cloned();
        let page_width = matches.get_one("page_width").cloned();

        Self {
            file_path,
            theme,
            scale,
            page_width,
        }
    }
}
