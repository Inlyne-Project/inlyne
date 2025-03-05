use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser};
use pretty_assertions::assert_eq;
use tempfile::NamedTempFile;

use crate::color::{SyntaxTheme, Theme, ThemeDefaults};
use crate::history::History;
use crate::opts::config::{self, FontOptions, LinesToScroll};
use crate::opts::{Cli, Opts, Position, ResolvedTheme, Size, ThemeType};
use crate::test_utils::log;

fn gen_args(args: Vec<&str>) -> Vec<OsString> {
    std::iter::once("inlyne")
        .chain(args)
        .map(OsString::from)
        .collect()
}

fn temp_md_file() -> (NamedTempFile, String) {
    let temp_file = tempfile::Builder::new()
        .prefix("inlyne-tests-")
        .suffix(".md")
        .tempfile()
        .unwrap();
    let path = temp_file.path().to_str().unwrap().to_owned();
    (temp_file, path)
}

impl Opts {
    fn mostly_default(file_path: impl AsRef<Path>) -> Self {
        Self {
            history: History::new(file_path.as_ref()).unwrap(),
            theme: ResolvedTheme::Light.as_theme(),
            decorations: None,
            scale: None,
            page_width: None,
            font_opts: FontOptions::default(),
            lines_to_scroll: LinesToScroll::default().0,
            keybindings: Default::default(),
            color_scheme: None,
            metrics: Default::default(),
            size: None,
            position: None,
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
    log::init();

    Cli::command().debug_assert();
}

#[test]
fn defaults() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(gen_args(vec![&md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config::Config::default(),
            None,
        )
        .unwrap(),
        Opts::mostly_default(&md_file)
    );
}

#[test]
fn config_overrides_default() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    // Light system theme with dark in config
    let config = config::Config {
        theme: Some(ThemeType::Dark),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(gen_args(vec![&md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config,
            Some(ResolvedTheme::Light),
        )
        .unwrap(),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            color_scheme: Some(ResolvedTheme::Dark),
            ..Opts::mostly_default(&md_file)
        }
    );

    // Dark system theme with light in config
    let config = config::Config {
        theme: Some(ThemeType::Light),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(gen_args(vec![&md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config,
            Some(ResolvedTheme::Dark),
        )
        .unwrap(),
        Opts {
            theme: ResolvedTheme::Light.as_theme(),
            color_scheme: Some(ResolvedTheme::Light),
            ..Opts::mostly_default(&md_file)
        }
    );

    let config = config::Config {
        scale: Some(1.5),
        ..Default::default()
    };
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(gen_args(vec![&md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config,
            None,
        )
        .unwrap(),
        Opts {
            scale: Some(1.5),
            ..Opts::mostly_default(&md_file)
        }
    );
}

#[test]
fn from_cli() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(gen_args(vec!["--theme", "dark", &md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config::Config::default(),
            Some(ResolvedTheme::Light),
        )
        .unwrap(),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            color_scheme: Some(ResolvedTheme::Dark),
            ..Opts::mostly_default(&md_file)
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
            Cli::try_parse_from(gen_args(vec!["--scale", "1.5", &md_file]))
                .unwrap()
                .into_view()
                .unwrap(),
            config,
            Some(ResolvedTheme::Light),
        )
        .unwrap(),
        Opts {
            theme: ResolvedTheme::Dark.as_theme(),
            scale: Some(1.5),
            color_scheme: Some(ResolvedTheme::Dark),
            ..Opts::mostly_default(&md_file)
        }
    );
}

#[test]
fn cli_kitchen_sink() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    #[rustfmt::skip]
    let args = gen_args(vec![
        "--theme", "dark",
        "--scale", "1.5",
        "--config", "/path/to/file.toml",
        "--page-width", "500",
        &md_file,
    ]);
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(args).unwrap().into_view().unwrap(),
            config::Config::default(),
            Some(ResolvedTheme::Light),
        )
        .unwrap(),
        Opts {
            page_width: Some(500.0),
            scale: Some(1.5),
            theme: ResolvedTheme::Dark.as_theme(),
            color_scheme: Some(ResolvedTheme::Dark),
            ..Opts::mostly_default(&md_file)
        }
    );
}

#[test]
fn builtin_syntax_theme() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    let mut config = config::Config::default();
    config.light_theme = Some(config::OptionalTheme {
        code_highlighter: Some(SyntaxTheme::Defaults(ThemeDefaults::SolarizedLight)),
        ..Default::default()
    });

    let opts = Opts::parse_and_load_with_system_theme(
        Cli::try_parse_from(gen_args(vec![&md_file]))
            .unwrap()
            .into_view()
            .unwrap(),
        config,
        Some(ResolvedTheme::Light),
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
            code_highlighter: Some(SyntaxTheme::custom(path)),
            ..Default::default()
        });
        config
    }

    log::init();

    let (_tmp, md_file) = temp_md_file();

    let args = Cli::try_parse_from(gen_args(vec![&md_file]))
        .unwrap()
        .into_view()
        .unwrap();

    let res = Opts::parse_and_load_with_system_theme(
        args.clone(),
        config_with_theme_at(PathBuf::from("this_path_doesnt_exist")),
        Some(ResolvedTheme::Light),
    );
    assert!(res.is_err());

    let opts = Opts::parse_and_load_with_system_theme(
        args,
        config_with_theme_at(
            PathBuf::new()
                .join("assets")
                .join("test_data")
                .join("sample.tmTheme"),
        ),
        Some(ResolvedTheme::Light),
    )
    .unwrap();
    assert_eq!(
        opts.theme.code_highlighter.name.unwrap(),
        "Example Color Scheme"
    );
}

#[test]
fn missing_file_arg() {
    log::init();

    // A file arg should be required
    assert!(Cli::try_parse_from(gen_args(Vec::new())).is_err());
}

#[test]
fn win_pos_and_size() {
    log::init();

    let (_tmp, md_file) = temp_md_file();

    let args = gen_args(vec![
        "--win-pos",
        "100,200",
        "--win-size",
        "300x400",
        &md_file,
    ]);
    assert_eq!(
        Opts::parse_and_load_with_system_theme(
            Cli::try_parse_from(args).unwrap().into_view().unwrap(),
            config::Config::default(),
            None,
        )
        .unwrap(),
        Opts {
            position: Some(Position { x: 100, y: 200 }),
            size: Some(Size {
                width: 300,
                height: 400
            }),
            ..Opts::mostly_default(&md_file)
        }
    );
}
