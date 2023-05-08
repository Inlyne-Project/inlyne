use std::{ffi::OsString, path::PathBuf};

use super::{cli, config, Opts, ResolvedTheme, ThemeType};
use crate::color::{self, Theme};
use crate::keybindings;
use crate::opts::config::{FontOptions, LinesToScroll};
use crate::opts::Args;

use pretty_assertions::assert_eq;

fn gen_args(args: Vec<&str>) -> Vec<OsString> {
    std::iter::once("inlyne")
        .chain(args.into_iter())
        .map(OsString::from)
        .collect()
}

impl Opts {
    fn mostly_default(file_path: impl Into<PathBuf>) -> Self {
        Self {
            file_path: file_path.into(),
            theme: ResolvedTheme::Light.as_theme(),
            scale: None,
            page_width: None,
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
            keybindings: keybindings::defaults(),
        }
    }
}

impl ResolvedTheme {
    fn as_theme(&self) -> Theme {
        match &self {
            Self::Dark => color::DARK_DEFAULT,
            Self::Light => color::LIGHT_DEFAULT,
        }
    }
}

#[test]
fn debug_assert() {
    cli::command().debug_assert();
}

#[test]
fn defaults() {
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["file.md"])),
            config::Config::default(),
            ResolvedTheme::Light,
        ),
        Opts::mostly_default("file.md")
    );
}

#[test]
fn config_overrides_default() {
    // Light system theme with dark in config
    let config = config::Config {
        theme: Some(ThemeType::Dark),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["file.md"])),
            config,
            ResolvedTheme::Light,
        ),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            ..Opts::mostly_default("file.md")
        }
    );

    // Dark system theme with light in config
    let config = config::Config {
        theme: Some(ThemeType::Light),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["file.md"])),
            config,
            ResolvedTheme::Dark,
        ),
        Opts {
            theme: ResolvedTheme::Light.as_theme(),
            ..Opts::mostly_default("file.md")
        }
    );

    let config = config::Config {
        scale: Some(1.5),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["file.md"])),
            config,
            ResolvedTheme::Light,
        ),
        Opts {
            scale: Some(1.5),
            ..Opts::mostly_default("file.md")
        }
    );
}

#[test]
fn from_cli() {
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["--theme", "dark", "file.md"])),
            config::Config::default(),
            ResolvedTheme::Light,
        ),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            ..Opts::mostly_default("file.md")
        }
    );

    // CLI takes precedence over config
    let config = config::Config {
        theme: Some(ThemeType::Dark),
        scale: Some(0.1),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::parse_from(gen_args(vec!["--scale", "1.5", "file.md"])),
            config,
            ResolvedTheme::Light,
        ),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            scale: Some(1.5),
            ..Opts::mostly_default("file.md")
        }
    );
}
