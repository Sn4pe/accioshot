// SPDX-License-Identifier: GPL-3.0-or-later

mod drag;
mod editor;
mod error;
mod history;
mod preferences;
mod theme;

use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use accioshot::config::Config;
#[cfg(target_os = "linux")]
use gtk::prelude::GtkSettingsExt;
use tauri::{LogicalSize, Manager, WebviewWindow};

const USAGE: &str = "usage: accioshot-ui edit <input.png> <output.png> | history | settings";
#[cfg(target_os = "linux")]
const DMABUF_VARIABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
const FLOAT_SETTLE: Duration = Duration::from_millis(300);
const HISTORY_SIZE: (u32, u32) = (1120, 760);
const SETTINGS_SIZE: (u32, u32) = (720, 760);
/// GTK counts it in 1024ths of a dot per inch.
#[cfg(target_os = "linux")]
const DEFAULT_FONT_DPI: i32 = 96 * 1024;

enum Mode {
    Edit(editor::Paths),
    History,
    Settings,
}

impl Mode {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Option<Mode> {
        let mut args = args.into_iter();
        let name = args.next()?;
        match (name.to_str()?, args.next(), args.next(), args.next()) {
            ("edit", Some(input), Some(output), None) => Some(Mode::Edit(editor::Paths {
                input: input.into(),
                output: output.into(),
            })),
            ("history", None, _, _) => Some(Mode::History),
            ("settings", None, _, _) => Some(Mode::Settings),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Mode::Edit(_) => "edit",
            Mode::History => "history",
            Mode::Settings => "settings",
        }
    }
}

struct WindowName(&'static str);

#[tauri::command]
fn window_name(name: tauri::State<WindowName>) -> &'static str {
    name.0
}

/// WebKit crashes with Wayland error 71 on NVIDIA unless its DMABUF renderer is off. Setting the
/// variable yourself wins over this.
#[cfg(target_os = "linux")]
fn work_around_nvidia() {
    if std::env::var_os(DMABUF_VARIABLE).is_none() && Path::new("/proc/driver/nvidia").exists() {
        // SAFETY: called first thing in main, before any other thread exists.
        unsafe { std::env::set_var(DMABUF_VARIABLE, "1") };
    }
}

/// GTK leaves this at -1 without the gsettings-desktop-schemas, and WebKit scales the page by it:
/// everything lays out at a negative size and the window looks black.
#[cfg(target_os = "linux")]
fn work_around_missing_font_dpi() {
    if let Some(settings) = gtk::Settings::default()
        && settings.gtk_xft_dpi() <= 0
    {
        settings.set_gtk_xft_dpi(DEFAULT_FONT_DPI);
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
        let resizable = window
            .set_min_size(None::<LogicalSize<u32>>)
            .and_then(|()| window.set_max_size(None::<LogicalSize<u32>>));
        if let Err(error) = resizable {
            eprintln!("accioshot-ui: {error}");
        }
    });
    Ok(())
}

fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("accioshot-ui: {error}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    work_around_nvidia();

    let Some(mode) = Mode::parse(std::env::args_os().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let mut builder = tauri::Builder::default().manage(WindowName(mode.name()));
    let (title, (width, height), floating) =
        match mode {
            Mode::Edit(paths) => {
                let settings = match Config::load() {
                    Ok(config) => config.editor,
                    Err(error) => return fail(error),
                };
                let window = (settings.width, settings.height, settings.floating);
                builder = builder.manage(paths).manage(settings).invoke_handler(
                    tauri::generate_handler![
                        window_name,
                        theme::ui_theme,
                        editor::read_image,
                        editor::settings,
                        editor::load_state,
                        editor::save_state,
                        editor::save,
                        editor::copy,
                        editor::discard,
                    ],
                );
                ("accioshot editor", (window.0, window.1), window.2)
            }
            Mode::History => {
                // A broken config.toml should not hide the captures, the defaults show them fine.
                let config = Config::load().unwrap_or_else(|error| {
                    eprintln!("accioshot-ui: {error}");
                    Config::default()
                });
                builder = builder
                    .plugin(drag::init())
                    .manage(history::Library::new(config.history))
                    .invoke_handler(tauri::generate_handler![
                        window_name,
                        theme::ui_theme,
                        history::history_items,
                        history::thumbnail,
                        history::image,
                        history::copy_entry,
                        history::open_entry,
                        history::edit_entry,
                        history::delete_entries,
                    ]);
                ("accioshot history", HISTORY_SIZE, true)
            }
            Mode::Settings => {
                builder = builder.invoke_handler(tauri::generate_handler![
                    window_name,
                    theme::ui_theme,
                    preferences::load_preferences,
                    preferences::save_preferences,
                    preferences::open_config,
                ]);
                ("accioshot settings", SETTINGS_SIZE, true)
            }
        };

    let result = builder
        .setup(move |app| {
            #[cfg(target_os = "linux")]
            work_around_missing_font_dpi();
            let window = app.get_webview_window("main").ok_or("no main window")?;
            let size = LogicalSize::new(width, height);
            window.set_title(title)?;
            window.set_background_color(Some(theme::background(&window)))?;
            window.set_size(size)?;
            #[cfg(debug_assertions)]
            if let Some(path) = std::env::var_os("ACCIOSHOT_UI_TEST") {
                let script = std::fs::read_to_string(path)?;
                let window = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(800));
                    if let Err(error) = window.eval(script) {
                        eprintln!("accioshot-ui: {error}");
                    }
                });
            }
            if floating {
                show_floating(window, size)?;
            } else {
                window.show()?;
            }
            // Started from a terminal instead of an app bundle, it would open behind everything.
            #[cfg(target_os = "macos")]
            app.get_webview_window("main")
                .ok_or("no main window")?
                .set_focus()?;
            Ok(())
        })
        .run(tauri::generate_context!());

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<&'static str> {
        Mode::parse(args.iter().map(OsString::from)).map(|mode| mode.name())
    }

    #[test]
    fn modes_take_exactly_their_arguments() {
        assert_eq!(parse(&["edit", "in", "out"]), Some("edit"));
        assert_eq!(parse(&["history"]), Some("history"));
        assert_eq!(parse(&["settings"]), Some("settings"));
        assert_eq!(parse(&["edit", "in"]), None);
        assert_eq!(parse(&["edit", "in", "out", "more"]), None);
        assert_eq!(parse(&["history", "more"]), None);
        assert_eq!(parse(&["in.png", "out.png"]), None);
        assert_eq!(parse(&[]), None);
    }
}
