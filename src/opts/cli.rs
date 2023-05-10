use std::{env, ffi::OsString, io, path::PathBuf};

use super::ThemeType;

use clap::builder::PossibleValue;
use clap::{command, value_parser, Arg, Command, ValueEnum, ValueHint};
use clap_complete::{generate, Generator, Shell};

const SCALE_HELP: &str =
    "Factor to scale rendered file by [default: OS defined window scale factor]";

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
        .required_unless_present("shell")
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

    let gen_comp_arg = Arg::new("shell")
        .long("gen-completions")
        .help("Generate shell completions")
        .number_of_values(1)
        .value_parser(value_parser!(Shell));

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
        .arg(gen_comp_arg)
        .arg(config_arg)
        .arg(page_width_arg)
}

impl Args {
    pub fn new() -> Self {
        let program_args = std::env::args_os().collect();
        Self::parse_from(program_args)
    }

    pub fn parse_from(args: Vec<OsString>) -> Self {
        let c = command();
        let matches = c.get_matches_from(args);

        // Shell completions exit early so handle them first
        if let Some(shell) = matches.get_one::<Shell>("shell").copied() {
            let mut c = command();
            Self::print_completions(shell, &mut c);
            std::process::exit(0);
        }

        let file_path = matches.get_one("file").cloned().expect("required");
        let theme = matches.get_one("theme").cloned();
        let scale = matches.get_one("scale").cloned();
        let config = matches.get_one("config").cloned();
        let page_width = matches.get_one("page_width").cloned();

        Self {
            file_path,
            theme,
            scale,
            config,
            page_width,
        }
    }

    fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
        generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
    }
}
