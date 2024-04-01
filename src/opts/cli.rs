use clap::{
    builder::PossibleValue, command, value_parser, Args as ClapArgs, Parser, Subcommand, ValueEnum,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;

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

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl FromStr for Position {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = input.split(',').collect();
        if parts.len() != 2 {
            return Err("Invalid format for Position: expected format <x>,<y>");
        }
        let x = parts[0]
            .parse::<i32>()
            .map_err(|_| "Invalid x-coordinate: not a valid integer")?;
        let y = parts[1]
            .parse::<i32>()
            .map_err(|_| "Invalid y-coordinate: not a valid integer")?;
        Ok(Position { x, y })
    }
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}
impl FromStr for Size {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = input.split('x').collect();
        if parts.len() != 2 {
            return Err("Invalid format for Size: expected format <width>x<height>");
        }
        let width = parts[0]
            .parse::<u32>()
            .map_err(|_| "Invalid width: not a valid integer")?;
        let height = parts[1]
            .parse::<u32>()
            .map_err(|_| "Invalid height: not a valid integer")?;
        Ok(Size { width, height })
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
    #[arg(long = "win-size", value_parser = value_parser!(Size))]
    pub size: Option<Size>,
}

/// Configuration related things
#[derive(Subcommand, PartialEq, Clone, Debug)]
pub enum ConfigCmd {
    /// Opens the configuration file in the default text editor
    Open,
}
