// SPDX-License-Identifier: GPL-3.0-or-later

//! What `accioshot` and `accioshot-ui` share.

pub mod config;
pub mod editor;
pub mod fs;
pub mod history;
pub mod hotkey;

use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const OPENER: &str = if cfg!(target_os = "macos") {
    "open"
} else {
    "xdg-open"
};
pub const UI_BINARY: &str = "accioshot-ui";
pub const COPY_ONLY_EXTENSION: &str = "copy-only";

pub fn sibling_binary(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(name)))
        .filter(|path| path.exists())
        .unwrap_or_else(|| name.into())
}

pub fn open(target: &OsStr) -> Result<()> {
    let mut child = Command::new(OPENER)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot run {OPENER}: {error}"))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

pub fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_links_open() {
        assert!(is_web_url("https://x/delete/1"));
        assert!(is_web_url("HTTP://x"));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("--help"));
        assert!(!is_web_url("javascript:alert(1)"));
    }
}
