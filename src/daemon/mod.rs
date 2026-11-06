// SPDX-License-Identifier: GPL-3.0-or-later

//! The tray. Captures still run as `accioshot capture`, here and from key bindings, so both write
//! the same history and a crash in a capture cannot take the tray down.

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(any(target_os = "macos", test))]
mod hotkey;

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

pub use platform::run;

use crate::Result;
use crate::history::{self, Entry};

const RECENT_COUNT: usize = 10;
const HISTORY_POLL: Duration = Duration::from_secs(2);
// Otherwise the frozen capture can include the tray menu while it closes.
const MENU_CLOSE_DELAY: Duration = Duration::from_millis(250);
const MODES: [(&str, &str); 5] = [
    ("Region", "region"),
    ("Last region", "last-region"),
    ("Active window", "window"),
    ("Focused monitor", "monitor"),
    ("All monitors", "screen"),
];

/// Held until the process exits, the kernel releases it even after a crash.
fn single_instance() -> Result<File> {
    // macOS has no runtime directory, and its temporary one is already per user.
    let dir = match cfg!(target_os = "macos") {
        true => std::env::temp_dir(),
        false => dirs::runtime_dir().ok_or("XDG_RUNTIME_DIR is not set")?,
    };
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

fn entry_label(entry: &Entry) -> String {
    let name = entry
        .path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    name.or_else(|| entry.url.clone())
        .unwrap_or_else(|| entry.time.clone())
}

/// What clicking a recent capture copies.
fn entry_text(entry: &Entry) -> String {
    entry
        .url
        .clone()
        .or_else(|| entry.path.as_ref().map(|path| path.display().to_string()))
        .unwrap_or_default()
}

fn spawn_capture(exe: PathBuf, mode: &'static str, delay: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let result = Command::new(exe)
            .args(["capture", mode])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status();
        if let Err(error) = result {
            eprintln!("accioshot: cannot start a capture: {error}");
        }
    });
}

fn open(target: &std::ffi::OsStr) {
    let target = target.to_os_string();
    std::thread::spawn(move || {
        if let Err(error) = Command::new(crate::OPENER).arg(target).status() {
            eprintln!("accioshot: cannot run {}: {error}", crate::OPENER);
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
}
