mod cli;
mod config;
#[cfg(test)]
mod tests;

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::color;
pub use cli::{Cli, Commands, ConfigCmd, Position, Size, ThemeType, View};
pub use config::{Config, DebugSection, FontOptions, KeybindingsSection, MetricsExporter};

use crate::history::History;
use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use smart_debug::SmartDebug;

static RENDER_ELEMENT_BOUNDS: AtomicBool = AtomicBool::new(false);

#[must_use]
pub fn get_render_element_bounds() -> bool {
    RENDER_ELEMENT_BOUNDS.load(Ordering::SeqCst)
}

pub fn set_render_element_bounds(b: bool) {
    RENDER_ELEMENT_BOUNDS.store(b, Ordering::SeqCst);
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResolvedTheme {
    Dark,
    #[default]
    Light,
}

impl ResolvedTheme {
    fn new(theme_ty: ThemeType) -> Option<Self> {
        match theme_ty {
            ThemeType::Auto => Self::try_detect(),
            ThemeType::Dark => Some(Self::Dark),
            ThemeType::Light => Some(Self::Light),
        }
    }

    fn try_detect() -> Option<Self> {
        match dark_light::detect() {
            dark_light::Mode::Default => None,
            dark_light::Mode::Dark => Some(Self::Dark),
            dark_light::Mode::Light => Some(Self::Light),
        }
    }
}

#[derive(SmartDebug, PartialEq)]
pub struct Opts {
    pub history: History,
    #[debug(skip)]
    pub theme: color::Theme,
    pub decorations: Option<bool>,
    pub scale: Option<f32>,
    pub page_width: Option<f32>,
    pub lines_to_scroll: f32,
    pub font_opts: FontOptions,
    pub keybindings: KeybindingsSection,
    pub color_scheme: Option<ResolvedTheme>,
    pub metrics: Option<MetricsExporter>,
    pub position: Option<Position>,
    pub size: Option<Size>,
}

impl Opts {
    pub fn parse_and_load_from(args: View, config: Config) -> Result<Self> {
        #[cfg(test)]
        {
            // "Use" the unused params
            let (_, _) = (args, config);
            panic!("Use `Opts::parse_and_load_with_system_theme()`");
        }
        #[cfg(not(test))]
        {
            let system_color_scheme = ResolvedTheme::try_detect();
            Self::parse_and_load_inner(args, config, system_color_scheme)
        }
    }

    #[cfg(test)]
    pub fn parse_and_load_with_system_theme(
        args: View,
        config: Config,
        theme: Option<ResolvedTheme>,
    ) -> Result<Self> {
        Self::parse_and_load_inner(args, config, theme)
    }

    fn parse_and_load_inner(
        args: View,
        config: Config,
        fallback_theme: Option<ResolvedTheme>,
    ) -> Result<Self> {
        let Config {
            theme: config_theme,
            decorations: config_decorations,
            scale: config_scale,
            page_width: config_page_width,
            lines_to_scroll,
            light_theme,
            dark_theme,
            font_options,
            keybindings,
            debug,
            window,
        } = config;

        let View {
            file_path,
            theme: args_theme,
            decorations,
            scale: args_scale,
            config: _,
            page_width: args_page_width,
            size: v_size,
            position: v_position,
        } = args;

        let DebugSection {
            metrics,
            render_element_bounds,
        } = debug;

        set_render_element_bounds(render_element_bounds);

        let history = History::new(&file_path)?;
        let resolved_theme = args_theme
            .or(config_theme)
            .and_then(ResolvedTheme::new)
            .or(fallback_theme);
        let theme = {
            let (maybe_theme, fallback_values) = match resolved_theme {
                Some(ResolvedTheme::Dark) => (dark_theme, color::Theme::dark_default()),
                None | Some(ResolvedTheme::Light) => (light_theme, color::Theme::light_default()),
            };

            match maybe_theme {
                Some(theme) => theme.merge(fallback_values)?,
                None => fallback_values,
            }
        };

        let decorations = decorations.or(config_decorations);
        let scale = args_scale.or(config_scale);
        let font_opts = font_options.unwrap_or_default();
        let page_width = args_page_width.or(config_page_width);
        let lines_to_scroll = lines_to_scroll.into();

        let (position, size) = if let Some(window) = window {
            (v_position.or(window.position), v_size.or(window.size))
        } else {
            (v_position, v_size)
        };

        Ok(Self {
            history,
            theme,
            decorations,
            scale,
            page_width,
            lines_to_scroll,
            font_opts,
            keybindings,
            color_scheme: resolved_theme,
            metrics,
            position,
            size,
        })
    }

    /// Arguments to supply to program that are opened externally.
    pub fn program_args(file_path: &Path) -> Vec<String> {
        let current_args = Cli::parse().into_view().expect("Should contain an view!");

        let mut args = Vec::new();

        args.push(file_path.display().to_string());
        if let Some(theme) = current_args.theme {
            args.push("--theme".to_owned());
            args.push(theme.as_str().to_owned());
        }

        if let Some(scale) = current_args.scale {
            args.push("--scale".to_owned());
            args.push(scale.to_string());
        }

        if let Some(config) = current_args.config {
            args.push("--config".to_owned());
            args.push(config.display().to_string());
        }

        if let Some(page_width) = current_args.page_width {
            args.push("-w".to_owned());
            args.push(page_width.to_string());
        }

        args
    }
}
