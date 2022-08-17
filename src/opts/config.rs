use std::fs;

use super::ThemeType;

use anyhow::Context;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct Config {
    pub theme: Option<ThemeType>,
    pub scale: Option<f32>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir().context("Failed detecting config dir")?;
        let config_path = config_dir.join("inlyne").join("inlyne.toml");
        if config_path.is_file() {
            let text = fs::read_to_string(&config_path).context("Failed reading config file")?;
            let config = toml::from_str(&text)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
}
