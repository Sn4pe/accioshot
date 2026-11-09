// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entry {
    pub time: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    pub deletion_url: Option<String>,
}

pub fn path() -> Result<PathBuf> {
    let dir = dirs::data_dir().ok_or("cannot find the data directory")?;
    Ok(dir.join("accioshot/history.jsonl"))
}

/// Removing rewrites the file, so a capture appending at that moment has to wait for it.
fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))?;
    file.lock()?;
    Ok(file)
}

pub fn append(entry: &Entry) -> Result<()> {
    append_to(&path()?, entry)
}

fn append_to(path: &Path, entry: &Entry) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _lock = lock(path)?;
    let mut line = serde_json::to_string(entry)?;
    line.push('\n');
    // 0600: deletion URLs let anyone remove the upload.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(())
}

/// Removes the entries still there. Damaged lines stay, they may be someone's only record.
pub fn remove(entries: &[Entry]) -> Result<bool> {
    remove_from(&path()?, entries)
}

fn remove_from(path: &Path, entries: &[Entry]) -> Result<bool> {
    let _lock = lock(path)?;
    let text = std::fs::read_to_string(path)?;
    let kept: String = text
        .split_inclusive('\n')
        .filter(|line| {
            serde_json::from_str::<Entry>(line).map_or(true, |entry| !entries.contains(&entry))
        })
        .collect();
    if kept.len() == text.len() {
        return Ok(false);
    }
    let temporary = path.with_extension("partial");
    let _ = std::fs::remove_file(&temporary);
    // 0600: deletion URLs let anyone remove the upload.
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?
        .write_all(kept.as_bytes())?;
    std::fs::rename(temporary, path)?;
    Ok(true)
}

/// The last `count` entries, newest first. A missing file is an empty history and damaged lines
/// are skipped, a half written line should not hide the rest.
pub fn recent(count: usize) -> Vec<Entry> {
    path().map_or_else(|_| Vec::new(), |path| recent_from(&path, count))
}

fn recent_from(path: &Path, count: usize) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .rev()
        .filter_map(|line| serde_json::from_str(line).ok())
        .take(count)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn entry(url: &str) -> Entry {
        Entry {
            time: "2026-11-05T10:00:00+01:00".to_string(),
            path: None,
            url: Some(url.to_string()),
            deletion_url: None,
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("accioshot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("history.jsonl")
    }

    #[test]
    fn recent_is_newest_first_and_limited() {
        let file = temp_file("order");
        for url in ["a", "b", "c"] {
            append_to(&file, &entry(url)).unwrap();
        }
        let urls: Vec<_> = recent_from(&file, 2)
            .into_iter()
            .filter_map(|e| e.url)
            .collect();
        assert_eq!(urls, ["c", "b"]);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn damaged_lines_are_skipped() {
        let file = temp_file("damaged");
        append_to(&file, &entry("a")).unwrap();
        let mut handle = OpenOptions::new().append(true).open(&file).unwrap();
        handle.write_all(b"{\"time\":\"cut\n").unwrap();
        assert_eq!(recent_from(&file, 5), [entry("a")]);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn removing_keeps_the_rest_and_damaged_lines() {
        let file = temp_file("remove");
        for url in ["a", "b"] {
            append_to(&file, &entry(url)).unwrap();
        }
        let mut handle = OpenOptions::new().append(true).open(&file).unwrap();
        handle.write_all(b"{\"time\":\"cut\n").unwrap();
        assert!(remove_from(&file, &[entry("a")]).unwrap());
        assert!(!remove_from(&file, &[entry("a")]).unwrap());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.ends_with("{\"time\":\"cut\n") && !text.contains("\"url\":\"a\""));
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn appends_during_a_removal_are_not_lost() {
        let file = temp_file("race");
        append_to(&file, &entry("gone")).unwrap();
        let writer = {
            let file = file.clone();
            std::thread::spawn(move || {
                for index in 0..200 {
                    append_to(&file, &entry(&index.to_string())).unwrap();
                }
            })
        };
        for index in 0..100 {
            let _ = remove_from(&file, &[entry(&format!("x{index}"))]);
        }
        remove_from(&file, &[entry("gone")]).unwrap();
        writer.join().unwrap();
        assert_eq!(recent_from(&file, usize::MAX).len(), 200);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(recent_from(Path::new("/nonexistent/history.jsonl"), 5).is_empty());
    }

    #[test]
    fn file_is_private() {
        let file = temp_file("mode");
        append_to(&file, &entry("a")).unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
}
