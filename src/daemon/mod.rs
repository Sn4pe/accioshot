// SPDX-License-Identifier: GPL-3.0-or-later

//! The tray. Captures still run as `accioshot capture`, here and from key bindings, so both write
//! the same history and a crash in a capture cannot take the tray down.

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
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
    lock("accioshot.lock")?.ok_or_else(|| "accioshot daemon is already running".into())
}

pub(crate) fn lock(name: &str) -> Result<Option<File>> {
    // macOS has no runtime directory, and its temporary one is already per user.
    let dir = match cfg!(target_os = "macos") {
        true => std::env::temp_dir(),
        false => dirs::runtime_dir().ok_or("XDG_RUNTIME_DIR is not set")?,
    };
    let file = File::create(dir.join(name))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
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

static OPEN_WINDOWS: Mutex<Vec<(&'static str, u32)>> = Mutex::new(Vec::new());

/// `window` is `history` or `settings`.
fn spawn_ui(exe: &Path, window: &'static str) {
    let mut open = OPEN_WINDOWS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(&(_, pid)) = open.iter().find(|(name, _)| *name == window) {
        raise(pid);
        return;
    }
    let mut child = match Command::new(crate::editor::ui_binary(exe))
        .arg(window)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return eprintln!("accioshot: cannot open the {window} window: {error}"),
    };
    let pid = child.id();
    open.push((window, pid));
    std::thread::spawn(move || {
        let _ = child.wait();
        OPEN_WINDOWS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|&(_, open_pid)| open_pid != pid);
    });
}

#[cfg(target_os = "macos")]
fn raise(pid: u32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    let Ok(pid) = i32::try_from(pid) else { return };
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }
}

#[cfg(not(target_os = "macos"))]
fn raise(_pid: u32) {}

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
