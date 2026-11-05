// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The [editor] table of config.toml. `command` belongs to accioshot and is only accepted here so
/// typos in the rest can be rejected.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[serde(skip_serializing)]
    command: Option<String>,
    pub tool: String,
    pub colors: Vec<String>,
    pub highlight_color: String,
    pub stroke_width: f64,
    pub font_family: String,
    pub font_size: f64,
    pub step_size: f64,
    pub blur_strength: f64,
    pub pixel_size: f64,
    pub remember_last: bool,
    pub floating: bool,
    pub width: u32,
    pub height: u32,
    /// Action name to key combination, merged over the defaults.
    pub keys: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            command: None,
            tool: "Arrow".to_string(),
            colors: [
                "#e5484d", "#f5a524", "#3fa66b", "#3b82c4", "#f4f1ea", "#1b1916",
            ]
            .map(str::to_string)
            .to_vec(),
            highlight_color: "#ffe066".to_string(),
            stroke_width: 4.0,
            font_family: "sans-serif".to_string(),
            font_size: 28.0,
            step_size: 32.0,
            blur_strength: 12.0,
            pixel_size: 14.0,
            remember_last: true,
            floating: true,
            width: 1280,
            height: 800,
            keys: HashMap::new(),
        }
    }
}

#[derive(Deserialize, Default)]
struct File {
    editor: Option<toml::Value>,
}

impl Settings {
    pub fn load() -> Result<Settings, String> {
        let dir = dirs::config_dir().ok_or("cannot find the config directory")?;
        Settings::load_from(&dir.join("accioshot/config.toml"))
    }

    fn load_from(path: &Path) -> Result<Settings, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        Settings::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    fn parse(text: &str) -> Result<Settings, String> {
        let file: File = toml::from_str(text).map_err(|error| error.to_string())?;
        let Some(table) = file.editor else {
            return Ok(Settings::default());
        };
        let settings: Settings = table
            .try_into()
            .map_err(|error: toml::de::Error| error.to_string())?;
        if settings.colors.is_empty() {
            return Err("[editor] colors cannot be empty".to_string());
        }
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_table_gives_the_defaults() {
        let settings = Settings::parse("uploader = \"imgur\"").unwrap();
        assert_eq!(settings.tool, "Arrow");
        assert!(settings.floating);
    }

    #[test]
    fn table_overrides_only_what_it_sets() {
        let text = "[editor]\ncommand = \"builtin\"\ntool = \"Blur\"\n[editor.keys]\nsave = \"ctrl+enter\"";
        let settings = Settings::parse(text).unwrap();
        assert_eq!(settings.tool, "Blur");
        assert_eq!(settings.keys["save"], "ctrl+enter");
        assert_eq!(settings.font_size, 28.0);
    }

    #[test]
    fn typos_are_rejected() {
        assert!(Settings::parse("[editor]\nfont_sizes = 3").is_err());
    }

    #[test]
    fn empty_colors_are_rejected() {
        assert!(Settings::parse("[editor]\ncolors = []").is_err());
    }
}
