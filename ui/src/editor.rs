// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, State};

use crate::settings::Settings;

const COPY_ONLY_EXTENSION: &str = "copy-only";

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
    // macOS has no state directory.
    let dir = dirs::state_dir().or_else(dirs::data_dir)?;
    Some(dir.join("accioshot/editor.json"))
}

/// Takes raw bytes, JSON would make a large image many times bigger.
#[tauri::command]
pub fn save(request: Request, paths: State<Paths>, app: AppHandle) -> Result<(), String> {
    // A copy that failed halfway leaves its marker, and this save must still reach the disk.
    let _ = std::fs::remove_file(paths.output.with_extension(COPY_ONLY_EXTENSION));
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
    write_atomically(output, png).map_err(|error| format!("cannot save: {error}"))
}

/// accioshot treats a missing output as discarded, so it must never see a partial file.
fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("partial");
    std::fs::write(&temporary, data)?;
    std::fs::rename(temporary, path)
}

/// Closing before the image appeared is a failure, not a choice: accioshot must keep the capture.
#[tauri::command]
pub fn discard(loaded: bool, app: AppHandle) {
    app.exit(if loaded { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_leaves_only_the_result() {
        let dir = std::env::temp_dir().join(format!("accioshot-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.png");
        write_atomically(&path, b"data").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
