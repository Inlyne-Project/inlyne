use std::{ffi::OsString, path::PathBuf};

use crate::opts::config::{FontOptions, LinesToScroll};
use crate::opts::Args;

use super::{cli, config, Opts, ThemeType};

fn gen_args(args: Vec<&str>) -> Vec<OsString> {
    std::iter::once("inlyne")
        .chain(args.into_iter())
        .map(OsString::from)
        .collect()
}

#[test]
fn debug_assert() {
    cli::command(
        "Factor to scale rendered file by [default: Window's scale factor]",
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
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::default().as_theme(),
            scale: None,
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
        }
    );
}

#[test]
fn config_overrides_default() {
    let config = config::Config {
        theme: ThemeType::Dark,
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_from(
            &Args::parse_from(gen_args(vec!["file.md"]), &config),
            config
        ),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: None,
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
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
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::default().as_theme(),
            scale: Some(1.5),
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
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
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: None,
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
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
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: Some(1.5),
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
        }
    );
}
