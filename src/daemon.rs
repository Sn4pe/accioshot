// SPDX-License-Identifier: GPL-3.0-or-later

//! The tray. Captures still run as `accioshot capture`, here and from key bindings, so both write
//! the same history and a crash in a capture cannot take the tray down.

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem, SubMenu};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

use crate::Result;
use crate::config::Config;
use crate::history::{self, Entry};

const RECENT_COUNT: usize = 10;
const HISTORY_POLL: Duration = Duration::from_secs(2);
// Otherwise the frozen capture can include the tray menu while it closes.
const MENU_CLOSE_DELAY: Duration = Duration::from_millis(250);

pub fn run() -> Result<()> {
    let _lock = single_instance()?;
    let config = Config::load()?;
    let tray = Tray {
        recent: history::recent(RECENT_COUNT),
        pictures_dir: config.pictures_dir()?,
    };
    let handle = tray.spawn()?;

    // shortcut: polls the file because captures are separate processes, use inotify if a two
    // second delay in the menu ever matters.
    let mut seen = history_modified();
    loop {
        std::thread::sleep(HISTORY_POLL);
        let modified = history_modified();
        if modified != seen {
            seen = modified;
            let recent = history::recent(RECENT_COUNT);
            if handle.update(|tray| tray.recent = recent).is_none() {
                return Err("the tray stopped".into());
            }
        }
    }
}

/// Held until the process exits, the kernel releases it even after a crash.
fn single_instance() -> Result<File> {
    let dir = dirs::runtime_dir().ok_or("XDG_RUNTIME_DIR is not set")?;
    let file = File::create(dir.join("accioshot.lock"))?;
    file.try_lock()
        .map_err(|_| "accioshot daemon is already running")?;
    Ok(file)
}

fn history_modified() -> Option<SystemTime> {
    std::fs::metadata(history::path().ok()?)
        .ok()?
        .modified()
        .ok()
}

struct Tray {
    recent: Vec<Entry>,
    pictures_dir: PathBuf,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "accioshot".into()
    }

    fn title(&self) -> String {
        "accioshot".into()
    }

    fn icon_name(&self) -> String {
        "camera-photo".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        spawn_capture("region", MENU_CLOSE_DELAY);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu: Vec<MenuItem<Self>> = [
            ("Region", "region"),
            ("Active window", "window"),
            ("Focused monitor", "monitor"),
            ("All monitors", "screen"),
        ]
        .into_iter()
        .map(|(label, mode)| {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |_: &mut Self| spawn_capture(mode, MENU_CLOSE_DELAY)),
                ..Default::default()
            }
            .into()
        })
        .collect();

        menu.push(MenuItem::Separator);
        menu.push(
            SubMenu {
                label: "Recent".into(),
                enabled: !self.recent.is_empty(),
                submenu: self.recent.iter().map(recent_item).collect(),
                ..Default::default()
            }
            .into(),
        );
        let dir = self.pictures_dir.clone();
        menu.push(
            StandardItem {
                label: "Open folder".into(),
                activate: Box::new(move |_: &mut Self| open(dir.as_os_str())),
                ..Default::default()
            }
            .into(),
        );
        menu.push(MenuItem::Separator);
        menu.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );
        menu
    }
}

fn recent_item(entry: &Entry) -> MenuItem<Tray> {
    let text = entry
        .url
        .clone()
        .or_else(|| entry.path.as_ref().map(|path| path.display().to_string()))
        .unwrap_or_default();
    StandardItem {
        label: menu_label(&entry_label(entry)),
        activate: Box::new(move |_: &mut Tray| {
            // In process, the daemon lives on to serve the clipboard.
            let source = Source::Bytes(text.clone().into_bytes().into());
            if let Err(error) = Options::new().copy(source, MimeType::Text) {
                eprintln!("accioshot: {error}");
            }
        }),
        ..Default::default()
    }
    .into()
}

fn entry_label(entry: &Entry) -> String {
    let name = entry
        .path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    name.or_else(|| entry.url.clone())
        .unwrap_or_else(|| entry.time.clone())
}

/// Single underscores are mnemonics in menu labels, and capture names are full of them.
fn menu_label(text: &str) -> String {
    text.replace('_', "__")
}

fn spawn_capture(mode: &'static str, delay: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let result = std::env::current_exe().and_then(|exe| {
            Command::new(exe)
                .args(["capture", mode])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .status()
        });
        if let Err(error) = result {
            eprintln!("accioshot: cannot start a capture: {error}");
        }
    });
}

fn open(target: &std::ffi::OsStr) {
    let target = target.to_os_string();
    std::thread::spawn(move || {
        if let Err(error) = Command::new("xdg-open").arg(target).status() {
            eprintln!("accioshot: cannot run xdg-open: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: Option<&str>, url: Option<&str>) -> Entry {
        Entry {
            time: "t".to_string(),
            path: path.map(PathBuf::from),
            url: url.map(str::to_string),
            deletion_url: None,
        }
    }

    #[test]
    fn labels_prefer_the_file_name() {
        assert_eq!(
            entry_label(&entry(Some("/a/b.png"), Some("https://x"))),
            "b.png"
        );
        assert_eq!(entry_label(&entry(None, Some("https://x"))), "https://x");
        assert_eq!(entry_label(&entry(None, None)), "t");
    }

    #[test]
    fn underscores_survive_menus() {
        assert_eq!(menu_label("accioshot_2026_a.png"), "accioshot__2026__a.png");
    }
}
