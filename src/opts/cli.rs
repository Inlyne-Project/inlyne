use super::{Position, Size};
use clap::{
    builder::PossibleValue, command, value_parser, Args as ClapArgs, Parser, Subcommand, ValueEnum,
};
use serde::Deserialize;
use std::path::PathBuf;

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

#[derive(Debug, PartialEq, Clone, Parser)]
#[command(version, about, arg_required_else_help(true))]
#[clap(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub view_file: Option<View>,
}

impl Cli {
    pub fn into_commands(self) -> Commands {
        if let Some(view) = self.view_file {
            Commands::View(view)
        } else {
            self.command.expect("Command should be Some!")
        }
    }
    pub fn into_view(self) -> Result<View, &'static str> {
        Ok(if let Some(view) = self.view_file {
            view
        } else if let Some(Commands::View(view)) = self.command {
            view
        } else {
            return Err("Cli options do not contain an view option");
        })
    }
}

#[derive(Subcommand, Debug, PartialEq, Clone)]
pub enum Commands {
    View(View),
    #[command(subcommand)]
    Config(ConfigCmd),
}

/// View a markdown file with inlyne
#[derive(ClapArgs, PartialEq, Debug, Clone, Default)]
#[command(arg_required_else_help(true))]
pub struct View {
    /// Path to the markdown file
    #[arg(value_name = "FILE", required = true)]
    pub file_path: PathBuf,

    /// Theme to use when rendering
    #[arg(short = 't', long = "theme", value_parser = value_parser!(ThemeType))]
    pub theme: Option<ThemeType>,

    /// Factor to scale rendered file by [default: OS defined window scale factor]
    #[arg(short = 's', long = "scale")]
    pub scale: Option<f32>,

    /// Configuration file to use
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Maximum width of page in pixels
    #[arg(short = 'w', long = "page-width")]
    pub page_width: Option<f32>,

    /// Position of the opened window <x>,<y>
    #[arg(short = 'p', long = "win-pos", value_parser = value_parser!(Position))]
    pub position: Option<Position>,

    /// Size of the opened window <width>x<height>
    #[arg(short = 'g', long = "win-size", value_parser = value_parser!(Size))]
    pub size: Option<Size>,
}

/// Configuration related things
#[derive(Subcommand, PartialEq, Clone, Debug)]
pub enum ConfigCmd {
    /// Opens the configuration file in the default text editor
    Open,
}
