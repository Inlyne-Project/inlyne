use std::{env, ffi::OsString, io, path::PathBuf};

use crate::color::{self, Theme};

use super::{config::Config, ThemeType};

use clap::builder::PossibleValue;
use clap::{command, value_parser, Arg, Command, ValueEnum};
use clap_complete::{generate, Generator, Shell};

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

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.as_str()))
    }
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Args {
    pub file_path: PathBuf,
    pub theme: Option<ThemeType>,
    pub scale: Option<f32>,
}

pub fn command(scale_help: String, default_theme: ThemeType) -> Command {
    let file_arg = Arg::new("file")
        .required_unless_present("shell")
        .number_of_values(1)
        .value_name("FILE")
        .value_parser(value_parser!(PathBuf))
        .help("Path to the markdown file");
    let theme_arg = Arg::new("theme")
        .short('t')
        .long("theme")
        .number_of_values(1)
        .value_parser(value_parser!(ThemeType))
        .default_value(default_theme.as_str())
        .help("Theme to use when rendering");

    let scale_arg = Arg::new("scale")
        .short('s')
        .long("scale")
        .number_of_values(1)
        .value_parser(value_parser!(f32))
        .help(scale_help);

    let gen_comp_arg = Arg::new("shell")
        .long("gen-completions")
        .help("Generate shell completions")
        .number_of_values(1)
        .value_parser(value_parser!(Shell));

    command!()
        .arg(file_arg)
        .arg(theme_arg)
        .arg(scale_arg)
        .arg(gen_comp_arg)
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
        let scale_help = Self::scale_help(config.scale);

        let c = command(scale_help.clone(), config.theme);
        let matches = c.get_matches_from(args);

        // Shell completions exit early so handle them first
        if let Some(shell) = matches.get_one::<Shell>("shell").copied() {
            let mut c = command(Self::scale_help(None), ThemeType::default());
            Self::print_completions(shell, &mut c);
            std::process::exit(0);
        }

        let file_path = matches.get_one("file").cloned().expect("required");
        let theme = matches.get_one("theme").cloned();
        let scale = matches.get_one("scale").cloned();

        Self {
            file_path,
            theme,
            scale,
        }
    }

    fn scale_help(maybe_scale: Option<f32>) -> String {
        format!(
            "Factor to scale rendered file by [default: {}]",
            match maybe_scale {
                Some(scale) => scale.to_string(),
                None => String::from("Window's scale factor"),
            }
        )
    }

    fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
        generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
    }
}
