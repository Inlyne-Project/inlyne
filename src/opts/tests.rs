use std::{ffi::OsString, path::PathBuf};

use super::{cli, config, Opts, ResolvedTheme, ThemeType};
use crate::color::{SyntaxTheme, Theme, ThemeDefaults};
use crate::keybindings::Keybindings;
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
            keybindings: Keybindings::default(),
        }
    }
}

impl ResolvedTheme {
    fn as_theme(&self) -> Theme {
        match &self {
            Self::Dark => Theme::dark_default(),
            Self::Light => Theme::light_default(),
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
            Args::try_parse_from(gen_args(vec!["file.md"])).unwrap(),
            config::Config::default(),
            ResolvedTheme::Light,
        )
        .unwrap(),
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
            Args::try_parse_from(gen_args(vec!["file.md"])).unwrap(),
            config,
            ResolvedTheme::Light,
        )
        .unwrap(),
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
            Args::try_parse_from(gen_args(vec!["file.md"])).unwrap(),
            config,
            ResolvedTheme::Dark,
        )
        .unwrap(),
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
            Args::try_parse_from(gen_args(vec!["file.md"])).unwrap(),
            config,
            ResolvedTheme::Light,
        )
        .unwrap(),
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
            Args::try_parse_from(gen_args(vec!["--theme", "dark", "file.md"])).unwrap(),
            config::Config::default(),
            ResolvedTheme::Light,
        )
        .unwrap(),
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
            Args::try_parse_from(gen_args(vec!["--scale", "1.5", "file.md"])).unwrap(),
            config,
            ResolvedTheme::Light,
        )
        .unwrap(),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            scale: Some(1.5),
            ..Opts::mostly_default("file.md")
        }
    );
}

#[test]
fn cli_kitchen_sink() {
    #[rustfmt::skip]
    let args = gen_args(vec![
        "--theme", "dark",
        "--scale", "1.5",
        "--config", "/path/to/file.toml",
        "--page-width", "500",
        "file.md",
    ]);
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Args::try_parse_from(args).unwrap(),
            config::Config::default(),
            ResolvedTheme::Light,
        )
        .unwrap(),
        Opts {
            page_width: Some(500.0),
            scale: Some(1.5),
            theme: ResolvedTheme::Dark.as_theme(),
            ..Opts::mostly_default("file.md")
        }
    );
}

#[test]
fn builtin_syntax_theme() {
    let mut config = config::Config::default();
    config.light_theme = Some(config::OptionalTheme {
        code_highlighter: Some(SyntaxTheme::Defaults(ThemeDefaults::SolarizedLight)),
        ..Default::default()
    });

    let opts = Opts::parse_and_load_with_system_theme(
        Args::try_parse_from(gen_args(vec!["file.md"])).unwrap(),
        config,
        ResolvedTheme::Light,
    )
    .unwrap();

    assert_eq!(
        opts.theme.code_highlighter.name.unwrap(),
        "Solarized (light)"
    );
}

#[test]
fn custom_syntax_theme() {
    fn config_with_theme_at(path: PathBuf) -> config::Config {
        let mut config = config::Config::default();
        config.light_theme = Some(config::OptionalTheme {
            code_highlighter: Some(SyntaxTheme::Custom { path }),
            ..Default::default()
        });
        config
    }

    let args = Args::try_parse_from(gen_args(vec!["file.md"])).unwrap();

    let res = Opts::parse_and_load_with_system_theme(
        args.clone(),
        config_with_theme_at(PathBuf::from("this_path_doesnt_exist")),
        ResolvedTheme::Light,
    );
    assert!(res.is_err());

    let opts = Opts::parse_and_load_with_system_theme(
        args,
        config_with_theme_at(
            PathBuf::new()
                .join("tests")
                .join("assets")
                .join("sample.tmTheme"),
        ),
        ResolvedTheme::Light,
    )
    .unwrap();
    assert_eq!(
        opts.theme.code_highlighter.name.unwrap(),
        "Example Color Scheme"
    );
}

#[test]
fn missing_file_arg() {
    // A file arg should be required
    assert!(Args::try_parse_from(gen_args(Vec::new())).is_err());
}
