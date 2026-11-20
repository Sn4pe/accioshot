// SPDX-License-Identifier: GPL-3.0-or-later

use crate::error::Result;
use accioshot::config::{self, Config};
use serde::Serialize;

#[derive(Serialize)]
pub struct Preferences {
    config: Config,
    uploaders: Vec<String>,
    has_hotkey: bool,
}

#[tauri::command]
pub fn load_preferences() -> Result<Preferences> {
    Ok(Preferences {
        config: Config::load()?,
        uploaders: config::uploader_names(),
        has_hotkey: cfg!(target_os = "macos"),
    })
}

#[tauri::command]
pub fn save_preferences(config: Config) -> Result<()> {
    Ok(config.save()?)
}

#[tauri::command]
pub fn open_config() -> Result<()> {
    let path = config::path()?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, "")?;
    }
    Ok(accioshot::open(path.as_os_str())?)
}
