// SPDX-License-Identifier: GPL-3.0-or-later

use crate::error::Result;
use std::path::{Path, PathBuf};

use accioshot::COPY_ONLY_EXTENSION;
use accioshot::config::EditorConfig;
use serde_json::Value;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, State};

pub struct Paths {
    pub input: PathBuf,
    pub output: PathBuf,
}

#[tauri::command]
pub fn read_image(paths: State<Paths>) -> Result<Response> {
    Ok(std::fs::read(&paths.input)
        .map(Response::new)
        .map_err(|error| format!("cannot read the capture: {error}"))?)
}

#[tauri::command]
pub fn settings(settings: State<EditorConfig>) -> EditorConfig {
    settings.inner().clone()
}

#[tauri::command]
pub fn load_state() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(state_path()?).ok()?).ok()
}

#[tauri::command]
pub fn save_state(state: Value) -> Result<()> {
    let path = state_path().ok_or("cannot find the state directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(std::fs::write(path, state.to_string())?)
}

fn state_path() -> Option<PathBuf> {
    Some(accioshot::state_dir()?.join("editor.json"))
}

#[tauri::command]
pub fn save(request: Request, paths: State<Paths>, app: AppHandle) -> Result<()> {
    write_output(&request, &paths.output)?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn copy(request: Request, paths: State<Paths>, app: AppHandle) -> Result<()> {
    let marker = paths.output.with_extension(COPY_ONLY_EXTENSION);
    std::fs::write(&marker, "").map_err(|error| format!("cannot copy: {error}"))?;
    if let Err(error) = write_output(&request, &paths.output) {
        std::fs::remove_file(&marker).ok();
        return Err(error);
    }
    app.exit(0);
    Ok(())
}

fn write_output(request: &Request, output: &Path) -> Result<()> {
    let InvokeBody::Raw(png) = request.body() else {
        return Err("expected the image as raw bytes".into());
    };
    Ok(accioshot::fs::write_atomically(output, png, 0o600)
        .map_err(|error| format!("cannot save: {error}"))?)
}

#[tauri::command]
pub fn discard(loaded: bool, app: AppHandle) {
    app.exit(if loaded { 0 } else { 1 });
}
