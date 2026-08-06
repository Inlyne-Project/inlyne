use std::fs::{create_dir_all, read_to_string};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{Position, Size, ThemeType};
use crate::color;
use crate::keybindings::Keybindings;

use anyhow::Context;
use serde::Deserialize;
use syntect::highlighting::Theme as SyntectTheme;

#[derive(Deserialize, Debug, PartialEq, Eq, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct FontOptions {
    #[serde(default)]
    pub regular_font: Option<String>,
    #[serde(default)]
    pub monospace_font: Option<String>,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct OptionalTheme {
    pub text_color: Option<u32>,
    pub background_color: Option<u32>,
    pub code_color: Option<u32>,
    pub quote_block_color: Option<u32>,
    pub link_color: Option<u32>,
    pub select_color: Option<u32>,
    pub checkbox_color: Option<u32>,
    pub code_highlighter: Option<color::SyntaxTheme>,
}

impl OptionalTheme {
    pub fn merge(self, other: color::Theme) -> anyhow::Result<color::Theme> {
        let code_highlighter = match self.code_highlighter {
            Some(theme) => SyntectTheme::try_from(theme)?,
            None => other.code_highlighter,
        };

        Ok(color::Theme {
            text_color: self.text_color.unwrap_or(other.text_color),
            background_color: self.background_color.unwrap_or(other.background_color),
            code_color: self.code_color.unwrap_or(other.code_color),
            quote_block_color: self.quote_block_color.unwrap_or(other.quote_block_color),
            link_color: self.link_color.unwrap_or(other.link_color),
            select_color: self.select_color.unwrap_or(other.select_color),
            checkbox_color: self.checkbox_color.unwrap_or(other.checkbox_color),
            code_highlighter,
        })
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct LinesToScroll(pub f32);

impl From<LinesToScroll> for f32 {
    fn from(value: LinesToScroll) -> Self {
        value.0
    }
}

impl Default for LinesToScroll {
    fn default() -> Self {
        Self(3.0)
    }
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct KeybindingsSection {
    #[serde(default)]
    pub base: Keybindings,
    pub extra: Option<Keybindings>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MetricsExporter {
    Log,
    #[cfg(inlyne_tcp_metrics)]
    Tcp,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct DebugSection {
    pub metrics: Option<MetricsExporter>,
    pub render_element_bounds: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Window {
    pub position: Option<Position>,
    pub size: Option<Size>,
}

const CONFIG_DIR_NAME: &str = "inlyne";
const CONFIG_FILE_NAME: &str = "config.toml";
/// Kept working indefinitely for anyone who set up a config before the rename
const LEGACY_CONFIG_FILE_NAME: &str = "inlyne.toml";

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub theme: Option<ThemeType>,
    pub decorations: Option<bool>,
    pub scale: Option<f32>,
    pub page_width: Option<f32>,
    pub lines_to_scroll: LinesToScroll,
    pub light_theme: Option<OptionalTheme>,
    pub dark_theme: Option<OptionalTheme>,
    pub font_options: Option<FontOptions>,
    pub keybindings: KeybindingsSection,
    pub debug: DebugSection,
    pub window: Option<Window>,
}

impl Config {
    pub fn load_from_str(s: &str) -> anyhow::Result<Self> {
        let config = toml::from_str(s)?;
        Ok(config)
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let config_content = read_to_string(path).context(format!(
            "Failed to read configuration file at '{}'",
            path.display()
        ))?;

        Self::load_from_str(&config_content)
    }

    /// Picks the config file within our config directory, preferring
    /// `config.toml` and falling back to the legacy `inlyne.toml`.
    ///
    /// Returns the `config.toml` path when neither exists, so a fresh install
    /// creates the current name. Never modifies the filesystem.
    fn resolve_in(config_dir: &Path) -> PathBuf {
        let path = config_dir.join(CONFIG_FILE_NAME);
        let legacy_path = config_dir.join(LEGACY_CONFIG_FILE_NAME);

        if path.is_file() {
            if legacy_path.is_file() {
                tracing::warn!(
                    "Ignoring '{}' in favor of '{}'",
                    legacy_path.display(),
                    path.display()
                );
            }
            path
        } else if legacy_path.is_file() {
            legacy_path
        } else {
            path
        }
    }

    /// The config file to read from and write back to.
    ///
    /// Shared so that loading at startup and `inlyne config open` can never
    /// disagree on which file is the user's.
    pub fn system_path() -> anyhow::Result<PathBuf> {
        let config_dir =
            dirs::config_dir().context("Failed to find the configuration directory")?;

        Ok(Self::resolve_in(&config_dir.join(CONFIG_DIR_NAME)))
    }

    pub fn load_from_system() -> anyhow::Result<Self> {
        let config_path = Self::system_path()?;

        if !config_path.is_file() {
            Self::create_default_config(&config_path)?
        }

        Self::load_from_file(&config_path)
    }

    pub fn create_default_config(path: &PathBuf) -> anyhow::Result<()> {
        create_dir_all(
            path.parent()
                .context("Could not find parent directory of path.")?,
        )?;

        let mut file = std::fs::File::create(path)?;
        file.write_all(Self::default_config().as_bytes())?;
        Ok(())
    }

    pub const fn default_config() -> &'static str {
        include_str!("../../inlyne.default.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_file_is_in_sync() {
        // Load the provided default toml file and compare with what we generate to make sure the
        // defaults stay in sync
        let mut config = Config::load_from_file(Path::new("inlyne.default.toml")).unwrap();

        // Swap out some of the values to compare
        let theme = config.theme.take().unwrap();
        let _ = config.font_options.take();
        let dark_theme = config.dark_theme.take().unwrap();
        let light_theme = config.light_theme.take().unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(theme, ThemeType::Auto);
        assert_eq!(
            dark_theme.merge(color::Theme::dark_default()).unwrap(),
            color::Theme::dark_default()
        );
        assert_eq!(
            light_theme.merge(color::Theme::light_default()).unwrap(),
            color::Theme::light_default()
        );
    }

    fn touch(path: &Path) {
        std::fs::File::create(path).unwrap();
    }

    #[test]
    fn resolves_to_current_name_when_neither_exists() {
        let dir = tempfile::tempdir().unwrap();

        let resolved = Config::resolve_in(dir.path());

        assert_eq!(resolved, dir.path().join(CONFIG_FILE_NAME));
        // Resolving is only a lookup. Creating the file is `load_from_system`'s job
        assert!(!resolved.exists());
    }

    #[test]
    fn resolves_to_legacy_name_when_it_is_the_only_one() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join(LEGACY_CONFIG_FILE_NAME);
        touch(&legacy_path);

        assert_eq!(Config::resolve_in(dir.path()), legacy_path);
    }

    #[test]
    fn resolves_to_current_name_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        touch(&path);

        assert_eq!(Config::resolve_in(dir.path()), path);
    }

    #[test]
    fn current_name_wins_over_legacy_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        touch(&path);
        touch(&dir.path().join(LEGACY_CONFIG_FILE_NAME));

        assert_eq!(Config::resolve_in(dir.path()), path);
    }

    #[test]
    fn a_directory_does_not_shadow_the_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        let legacy_path = dir.path().join(LEGACY_CONFIG_FILE_NAME);
        touch(&legacy_path);

        assert_eq!(Config::resolve_in(dir.path()), legacy_path);
    }
}
