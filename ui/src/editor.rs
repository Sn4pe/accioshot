// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

use accioshot::COPY_ONLY_EXTENSION;
use serde_json::Value;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, State};

use crate::settings::Settings;

pub struct Paths {
    pub input: PathBuf,
    pub output: PathBuf,
}

#[tauri::command]
pub fn read_image(paths: State<Paths>) -> Result<Response, String> {
    std::fs::read(&paths.input)
        .map(Response::new)
        .map_err(|error| format!("cannot read the capture: {error}"))
}

#[tauri::command]
pub fn settings(settings: State<Settings>) -> Settings {
    settings.inner().clone()
}

#[tauri::command]
pub fn load_state() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(state_path()?).ok()?).ok()
}

#[tauri::command]
pub fn save_state(state: Value) -> Result<(), String> {
    let path = state_path().ok_or("cannot find the state directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, state.to_string()).map_err(|error| error.to_string())
}

fn state_path() -> Option<PathBuf> {
    Some(accioshot::state_dir()?.join("editor.json"))
}

/// Takes raw bytes, JSON would make a large image many times bigger.
#[tauri::command]
pub fn save(request: Request, paths: State<Paths>, app: AppHandle) -> Result<(), String> {
    // A copy that failed halfway leaves its marker, and this save must still reach the disk.
    std::fs::remove_file(paths.output.with_extension(COPY_ONLY_EXTENSION)).ok();
    write_output(&request, &paths.output)?;
    app.exit(0);
    Ok(())
}

/// Same as save, the marker tells accioshot the image is only for the clipboard.
#[tauri::command]
pub fn copy(request: Request, paths: State<Paths>, app: AppHandle) -> Result<(), String> {
    std::fs::write(paths.output.with_extension(COPY_ONLY_EXTENSION), "")
        .map_err(|error| format!("cannot copy: {error}"))?;
    write_output(&request, &paths.output)?;
    app.exit(0);
    Ok(())
}

fn write_output(request: &Request, output: &Path) -> Result<(), String> {
    let InvokeBody::Raw(png) = request.body() else {
        return Err("expected the image as raw bytes".to_string());
    };
    accioshot::fs::write_atomically(output, png, 0o600)
        .map_err(|error| format!("cannot save: {error}"))
}

/// Closing before the image appeared is a failure, not a choice: accioshot must keep the capture.
#[tauri::command]
pub fn discard(loaded: bool, app: AppHandle) {
    app.exit(if loaded { 0 } else { 1 });
}
