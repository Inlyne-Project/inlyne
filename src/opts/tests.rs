use std::{ffi::OsString, path::PathBuf};

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
    assert_eq!(
        Opts::parse_and_load_from(gen_args(vec!["file.md"]), config::Config::default()),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::default().as_theme(),
            scale: None,
        }
    );
}

#[test]
fn config_overrides_default() {
    assert_eq!(
        Opts::parse_and_load_from(
            gen_args(vec!["file.md"]),
            config::Config {
                theme: Some(ThemeType::Dark),
                scale: None,
            }
        ),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: None,
        }
    );
    assert_eq!(
        Opts::parse_and_load_from(
            gen_args(vec!["file.md"]),
            config::Config {
                theme: None,
                scale: Some(1.5),
            }
        ),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::default().as_theme(),
            scale: Some(1.5),
        }
    );
}

#[test]
fn from_cli() {
    assert_eq!(
        Opts::parse_and_load_from(
            gen_args(vec!["--theme", "dark", "file.md"]),
            config::Config::default()
        ),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: None,
        }
    );

    // CLI takes precedence over config
    assert_eq!(
        Opts::parse_and_load_from(
            gen_args(vec!["--scale", "1.5", "file.md"]),
            config::Config {
                theme: Some(ThemeType::Dark),
                scale: Some(0.1)
            },
        ),
        Opts {
            file_path: PathBuf::from("file.md"),
            theme: ThemeType::Dark.as_theme(),
            scale: Some(1.5),
        }
    );
}
