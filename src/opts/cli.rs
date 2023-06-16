use std::{env, ffi::OsString, path::PathBuf};

use clap::builder::PossibleValue;
use clap::{command, value_parser, Arg, Command, ValueEnum, ValueHint};
use serde::Deserialize;

const SCALE_HELP: &str =
    "Factor to scale rendered file by [default: OS defined window scale factor]";

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeType {
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemeType {
    pub fn as_str(&self) -> &'static str {
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
    pub config: Option<PathBuf>,
    pub page_width: Option<f32>,
}

pub fn command() -> Command {
    let file_arg = Arg::new("file")
        .required(true)
        .number_of_values(1)
        .value_name("FILE")
        .value_parser(value_parser!(PathBuf))
        .value_hint(ValueHint::AnyPath)
        .help("Path to the markdown file");

    let theme_arg = Arg::new("theme")
        .short('t')
        .long("theme")
        .number_of_values(1)
        .value_parser(value_parser!(ThemeType))
        .help("Theme to use when rendering");

    let scale_arg = Arg::new("scale")
        .short('s')
        .long("scale")
        .number_of_values(1)
        .value_parser(value_parser!(f32))
        .help(SCALE_HELP);

    let config_arg = Arg::new("config")
        .short('c')
        .long("config")
        .number_of_values(1)
        .value_parser(value_parser!(PathBuf))
        .help("Configuration file to use");

    let page_width_arg = Arg::new("page_width")
        .short('w')
        .long("page-width")
        .number_of_values(1)
        .value_parser(value_parser!(f32))
        .help("Maximum width of page in pixels");

    command!()
        .arg(file_arg)
        .arg(theme_arg)
        .arg(scale_arg)
        .arg(config_arg)
        .arg(page_width_arg)
}

impl Args {
    pub fn new() -> Self {
        let program_args = std::env::args_os().collect();
        Self::parse_from(program_args)
    }

    pub fn parse_from(args: Vec<OsString>) -> Self {
        #[cfg(test)]
        {
            let _ = args;
            panic!("Use `Args::try_parse_from()` in tests");
        }
        #[cfg(not(test))]
        match Self::try_parse_from(args) {
            Ok(args) => args,
            // Expose clap error normally
            Err(clap_err) => clap_err.exit(),
        }
    }

    pub fn try_parse_from(args: Vec<OsString>) -> Result<Self, clap::Error> {
        let c = command();
        let matches = c.try_get_matches_from(args)?;

        let file_path = matches.get_one("file").cloned().unwrap();
        let theme = matches.get_one("theme").cloned();
        let scale = matches.get_one("scale").cloned();
        let config = matches.get_one("config").cloned();
        let page_width = matches.get_one("page_width").cloned();

        Ok(Self {
            file_path,
            theme,
            scale,
            config,
            page_width,
        })
    }
}
