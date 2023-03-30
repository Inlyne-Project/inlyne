use std::{ffi::OsString, path::PathBuf};

use super::{cli, config, Opts, ResolvedTheme, ThemeType};
use crate::color::{self, Theme};
use crate::keybindings;
use crate::opts::config::{FontOptions, LinesToScroll};
use crate::opts::Args;

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
            theme: ResolvedTheme::from(ThemeType::default()).as_theme(),
            scale: None,
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
    cli::command(
        "Factor to scale rendered file by [default: Window's scale factor]".to_string(),
        ThemeType::Dark,
    )
    .debug_assert();
}

#[test]
fn defaults() {
    let config = config::Config::default();
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["file.md"]), &config),
            config::Config::default()
        ),
        Opts::mostly_default("file.md")
    );
}

#[test]
fn config_overrides_default() {
    let config = config::Config {
        lines_to_scroll: LinesToScroll(12.0),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["file.md"]), &config),
            config
        ),
        Opts {
            lines_to_scroll: 12.0,
            ..Opts::mostly_default("file.md")
        }
    );

    let config = config::Config {
        scale: Some(1.5),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["file.md"]), &config),
            config,
        ),
        Opts {
            scale: Some(1.5),
            ..Opts::mostly_default("file.md")
        }
    );
}

#[test]
fn from_cli() {
    let config = config::Config::default();
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["--theme", "dark", "file.md"]), &config),
            config::Config::default()
        ),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            ..Opts::mostly_default("file.md")
        }
    );

    // CLI takes precedence over config
    let config = config::Config {
        theme: ThemeType::Dark,
        scale: Some(0.1),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["--scale", "1.5", "file.md"]), &config),
            config
        ),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            scale: Some(1.5),
            ..Opts::mostly_default("file.md")
        }
    );
}
