// SPDX-License-Identifier: GPL-3.0-or-later

use accioshot::config::{self, Config};
use serde::Serialize;

#[derive(Serialize)]
pub struct Preferences {
    config: Config,
    uploaders: Vec<String>,
    /// Only macOS needs a shortcut of its own, elsewhere the compositor has them.
    has_hotkey: bool,
}

#[tauri::command]
pub fn load_preferences() -> Result<Preferences, String> {
    Ok(Preferences {
        config: Config::load().map_err(|error| error.to_string())?,
        uploaders: config::uploader_names(),
        has_hotkey: cfg!(target_os = "macos"),
    })
}

#[tauri::command]
pub fn save_preferences(config: Config) -> Result<(), String> {
    config.save().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_config() -> Result<(), String> {
    let path = config::path().map_err(|error| error.to_string())?;
    if !path.exists() {
        // The desktop cannot open a file that is not there yet.
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, "").map_err(|error| error.to_string())?;
    }
    crate::open(path.as_os_str())
}
