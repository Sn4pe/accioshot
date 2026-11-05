// SPDX-License-Identifier: GPL-3.0-or-later

mod settings;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use settings::Settings;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, LogicalSize, Manager, State, WebviewWindow};

const USAGE: &str = "usage: accioshot-editor <input.png> <output.png>";
const DMABUF_VARIABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
const FLOAT_SETTLE: Duration = Duration::from_millis(300);

struct Paths {
    input: PathBuf,
    output: PathBuf,
}

#[tauri::command]
fn read_image(paths: State<Paths>) -> Result<Response, String> {
    std::fs::read(&paths.input)
        .map(Response::new)
        .map_err(|error| format!("cannot read the capture: {error}"))
}

#[tauri::command]
fn settings(settings: State<Settings>) -> Settings {
    settings.inner().clone()
}

#[tauri::command]
fn load_state() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(state_path()?).ok()?).ok()
}

#[tauri::command]
fn save_state(state: Value) -> Result<(), String> {
    let path = state_path().ok_or("cannot find the state directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, state.to_string()).map_err(|error| error.to_string())
}

fn state_path() -> Option<PathBuf> {
    Some(dirs::state_dir()?.join("accioshot/editor.json"))
}

/// Takes raw bytes, JSON would make a large image many times bigger.
#[tauri::command]
fn save(request: Request, paths: State<Paths>, app: AppHandle) -> Result<(), String> {
    let InvokeBody::Raw(png) = request.body() else {
        return Err("expected the image as raw bytes".to_string());
    };
    write_atomically(&paths.output, png).map_err(|error| format!("cannot save: {error}"))?;
    app.exit(0);
    Ok(())
}

/// accioshot treats a missing output as discarded, so it must never see a partial file.
fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("partial");
    std::fs::write(&temporary, data)?;
    std::fs::rename(temporary, path)
}

#[tauri::command]
fn discard(app: AppHandle) {
    app.exit(0);
}

/// WebKit crashes with Wayland error 71 on NVIDIA unless its DMABUF renderer is off. Setting the
/// variable yourself wins over this.
fn work_around_nvidia() {
    if std::env::var_os(DMABUF_VARIABLE).is_none() && Path::new("/proc/driver/nvidia").exists() {
        // SAFETY: called first thing in main, before any other thread exists.
        unsafe { std::env::set_var(DMABUF_VARIABLE, "1") };
    }
}

/// Compositors float a window whose minimum and maximum sizes match, and Wayland clients have no
/// other way to ask for it. Once it is on screen it can go back to being resizable.
fn show_floating(window: WebviewWindow, size: LogicalSize<u32>) -> tauri::Result<()> {
    window.set_min_size(Some(size))?;
    window.set_max_size(Some(size))?;
    window.show()?;
    std::thread::spawn(move || {
        std::thread::sleep(FLOAT_SETTLE);
        let _ = window.set_min_size(None::<LogicalSize<u32>>);
        let _ = window.set_max_size(None::<LogicalSize<u32>>);
    });
    Ok(())
}

fn main() {
    work_around_nvidia();

    let mut args = std::env::args_os().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let loaded = match Settings::load() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("accioshot-editor: {error}");
            std::process::exit(1);
        }
    };
    let (width, height, floating) = (loaded.width, loaded.height, loaded.floating);

    let result = tauri::Builder::default()
        .manage(Paths {
            input: input.into(),
            output: output.into(),
        })
        .manage(loaded)
        .setup(move |app| {
            let window = app.get_webview_window("main").ok_or("no main window")?;
            let size = LogicalSize::new(width, height);
            window.set_size(size)?;
            #[cfg(debug_assertions)]
            if let Some(path) = std::env::var_os("ACCIOSHOT_EDITOR_TEST") {
                let script = std::fs::read_to_string(path)?;
                let window = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(800));
                    let _ = window.eval(script);
                });
            }
            if floating {
                show_floating(window, size)?;
            } else {
                window.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_image, settings, load_state, save_state, save, discard
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("accioshot-editor: {error}");
        std::process::exit(1);
    }
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
