// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum Zoom {
    Scale(f64),
    Named(String),
}

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
    /// "fit" or a scale, 1 is 100 %.
    pub initial_zoom: Zoom,
    /// Factor per zoom step.
    pub zoom_step: f64,
    pub max_zoom: f64,
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
            initial_zoom: Zoom::Named("fit".to_string()),
            zoom_step: 1.25,
            max_zoom: 16.0,
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
        Settings::load_from(&accioshot::config::path().map_err(|error| error.to_string())?)
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
        match &settings.initial_zoom {
            Zoom::Scale(scale) if *scale > 0.0 => {}
            Zoom::Named(name) if name == "fit" => {}
            _ => return Err("[editor] initial_zoom must be \"fit\" or a scale above 0".to_string()),
        }
        if settings.zoom_step <= 1.0 {
            return Err("[editor] zoom_step must be above 1".to_string());
        }
        if settings.max_zoom < 1.0 {
            return Err("[editor] max_zoom cannot be below 1".to_string());
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
    fn initial_zoom_takes_fit_or_a_scale() {
        let settings = Settings::parse("[editor]\ninitial_zoom = 2").unwrap();
        assert_eq!(settings.initial_zoom, Zoom::Scale(2.0));
        assert!(Settings::parse("[editor]\ninitial_zoom = \"fit\"").is_ok());
        assert!(Settings::parse("[editor]\ninitial_zoom = \"big\"").is_err());
        assert!(Settings::parse("[editor]\ninitial_zoom = 0").is_err());
    }

    #[test]
    fn zoom_limits_are_checked() {
        assert!(Settings::parse("[editor]\nzoom_step = 1").is_err());
        assert!(Settings::parse("[editor]\nmax_zoom = 0.5").is_err());
    }

    #[test]
    fn empty_colors_are_rejected() {
        assert!(Settings::parse("[editor]\ncolors = []").is_err());
    }
}
