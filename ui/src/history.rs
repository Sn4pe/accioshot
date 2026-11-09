// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::{DirBuilder, Metadata};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Cursor, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use accioshot::config::HistoryConfig;
use accioshot::history::{self, Entry};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Response;

/// The entries last shown. Commands only act on these: what a page asks to read, copy, open or
/// delete must be something the history really holds.
pub struct Library {
    entries: Mutex<Vec<Entry>>,
    config: HistoryConfig,
}

impl Library {
    pub fn new(config: HistoryConfig) -> Library {
        Library {
            entries: Mutex::new(Vec::new()),
            config,
        }
    }

    fn check(&self, entry: &Entry) -> Result<(), String> {
        match self
            .entries
            .lock()
            .map_err(|e| e.to_string())?
            .contains(entry)
        {
            true => Ok(()),
            false => Err("this capture is no longer in the history".to_string()),
        }
    }
}

#[derive(Serialize)]
pub struct Items {
    items: Vec<Item>,
    thumbnail_width: u32,
    thumbnail_height: u32,
}

#[derive(Serialize)]
pub struct Item {
    entry: Entry,
    /// Whether the saved file is still on disk.
    exists: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Image,
    Path,
    Url,
    DeletionUrl,
    File,
    Folder,
}

#[tauri::command]
pub fn history_items(library: State<Library>) -> Result<Items, String> {
    let entries = history::recent(library.config.max_items);
    let items = visible(entries, library.config.filter_missing_files);
    *library.entries.lock().map_err(|e| e.to_string())? =
        items.iter().map(|item| item.entry.clone()).collect();
    Ok(Items {
        items,
        thumbnail_width: library.config.thumbnail_width,
        thumbnail_height: library.config.thumbnail_height,
    })
}

fn visible(entries: Vec<Entry>, filter_missing: bool) -> Vec<Item> {
    entries
        .into_iter()
        .map(|entry| Item {
            exists: entry.path.as_deref().is_some_and(Path::exists),
            entry,
        })
        .filter(|item| !filter_missing || item.exists || item.entry.path.is_none())
        .collect()
}

/// Decoding a big capture takes a while, so this runs off the main thread.
#[tauri::command(async)]
pub fn thumbnail(entry: Entry, library: State<Library>) -> Result<Response, String> {
    library.check(&entry)?;
    let path = entry.path.as_deref().ok_or("this capture has no file")?;
    let size = (
        library.config.thumbnail_width,
        library.config.thumbnail_height,
    );
    cached_thumbnail(path, size)
        .map(Response::new)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn cached_thumbnail(path: &Path, size: (u32, u32)) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = std::fs::metadata(path)?;
    let cached = thumbnail_path(path, &file, size);
    if let Some(bytes) = cached
        .as_ref()
        .and_then(|cached| std::fs::read(cached).ok())
    {
        return Ok(bytes);
    }
    let bytes = render_thumbnail(path, size)?;
    if let Some(cached) = cached
        && let Err(error) = store(&cached, &bytes)
    {
        eprintln!("accioshot-ui: cannot cache a thumbnail: {error}");
    }
    Ok(bytes)
}

fn render_thumbnail(path: &Path, size: (u32, u32)) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Cursor::new(Vec::new());
    image::open(path)?
        .thumbnail(size.0, size.1)
        .write_to(&mut bytes, ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

/// Keyed by what makes it stale, so an edited file or another size gets a new one.
fn thumbnail_path(path: &Path, file: &Metadata, size: (u32, u32)) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    (path, file.modified().ok(), file.len(), size).hash(&mut hasher);
    let dir = dirs::cache_dir()?.join("accioshot/thumbs");
    Some(dir.join(format!("{:016x}.png", hasher.finish())))
}

fn store(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        // 0700: these are small copies of screenshots, they may hold anything.
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let temporary = path.with_extension(format!("{}.partial", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)
}

#[tauri::command(async)]
pub fn image(entry: Entry, library: State<Library>) -> Result<Response, String> {
    library.check(&entry)?;
    let path = entry.path.as_deref().ok_or("this capture has no file")?;
    std::fs::read(path)
        .map(Response::new)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

#[tauri::command(async)]
pub fn copy_entry(entry: Entry, part: Part, library: State<Library>) -> Result<(), String> {
    library.check(&entry)?;
    match part {
        Part::Image => {
            let path = entry.path.as_deref().ok_or("this capture has no file")?;
            copy(&[std::ffi::OsStr::new("--image"), path.as_os_str()], "")
        }
        Part::Path => {
            let path = entry.path.as_deref().ok_or("this capture has no file")?;
            copy(&[], &path.to_string_lossy())
        }
        Part::Url => copy(&[], entry.url.as_deref().ok_or("this capture has no URL")?),
        Part::DeletionUrl | Part::File | Part::Folder => Err("nothing to copy".to_string()),
    }
}

/// Through accioshot: on Wayland only a process without threads can keep serving the clipboard.
fn copy(args: &[&std::ffi::OsStr], text: &str) -> Result<(), String> {
    let mut child = Command::new(crate::sibling("accioshot"))
        .arg("copy")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run accioshot: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    match output.status.success() {
        true => Ok(()),
        false => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
    }
}

static EDITS: AtomicUsize = AtomicUsize::new(0);

#[tauri::command(async)]
pub fn edit_entry(entry: Entry, library: State<Library>) -> Result<(), String> {
    library.check(&entry)?;
    let path = entry
        .path
        .as_deref()
        .filter(|path| path.exists())
        .ok_or("this capture has no file")?;
    let name = path.file_name().ok_or("this capture has no file")?;
    let edit = EDITS.fetch_add(1, Ordering::Relaxed);
    let output = path.with_file_name(format!(".{}.{edit}.editing.png", name.to_string_lossy()));
    let copy_only = output.with_extension("copy-only");
    Command::new(crate::sibling("accioshot-ui"))
        .arg("edit")
        .arg(path)
        .arg(&output)
        .status()
        .map_err(|error| format!("cannot open the editor: {error}"))?;
    let result = if !output.exists() {
        Ok(())
    } else if library.check(&entry).is_err() || !path.exists() {
        Err("the capture was removed while it was being edited".to_string())
    } else if copy_only.exists() {
        copy(&[std::ffi::OsStr::new("--image"), output.as_os_str()], "")
    } else {
        std::fs::rename(&output, path)
            .map_err(|error| format!("cannot save {}: {error}", path.display()))
    };
    let _ = std::fs::remove_file(&output);
    let _ = std::fs::remove_file(&copy_only);
    result
}

#[tauri::command]
pub fn open_entry(entry: Entry, part: Part, library: State<Library>) -> Result<(), String> {
    library.check(&entry)?;
    match part {
        Part::File => crate::open(
            entry
                .path
                .as_deref()
                .ok_or("this capture has no file")?
                .as_os_str(),
        ),
        Part::Folder => {
            let dir = entry.path.as_deref().and_then(Path::parent);
            crate::open(dir.ok_or("this capture has no file")?.as_os_str())
        }
        Part::Url => open_url(entry.url.as_deref()),
        Part::DeletionUrl => open_url(entry.deletion_url.as_deref()),
        Part::Image | Part::Path => Err("nothing to open".to_string()),
    }
}

/// These came from a remote server's response, the desktop must not run anything else.
fn open_url(url: Option<&str>) -> Result<(), String> {
    let url = url.ok_or("this capture has no such link")?;
    if !is_web_url(url) {
        return Err(format!("not a web address: {url}"));
    }
    crate::open(url.as_ref())
}

fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The file goes first: if that fails the entry stays, so the delete can be tried again.
#[tauri::command]
pub fn delete_entries(
    entries: Vec<Entry>,
    delete_file: bool,
    library: State<Library>,
) -> Result<Vec<String>, String> {
    for entry in &entries {
        library.check(entry)?;
    }
    let size = (
        library.config.thumbnail_width,
        library.config.thumbnail_height,
    );
    let mut failures = Vec::new();
    let mut removable = Vec::new();
    for entry in entries {
        if let Some(path) = entry.path.as_deref() {
            if let Ok(file) = std::fs::metadata(path)
                && let Some(cached) = thumbnail_path(path, &file, size)
            {
                let _ = std::fs::remove_file(cached);
            }
            if delete_file {
                match std::fs::remove_file(path) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        failures.push(format!("cannot delete {}: {error}", path.display()));
                        continue;
                    }
                    _ => {}
                }
            }
        }
        removable.push(entry);
    }
    history::remove(&removable).map_err(|error| error.to_string())?;
    library
        .entries
        .lock()
        .map_err(|e| e.to_string())?
        .retain(|e| !removable.contains(e));
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: Option<PathBuf>, url: Option<&str>) -> Entry {
        Entry {
            time: "t".to_string(),
            path,
            url: url.map(str::to_string),
            deletion_url: None,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("accioshot-ui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filtering_hides_missing_files_but_not_uploads() {
        let entries = vec![
            entry(Some("/nonexistent/a.png".into()), None),
            entry(None, Some("https://x")),
        ];
        let shown = visible(entries.clone(), false);
        assert_eq!(shown.len(), 2);
        assert!(!shown[0].exists);
        let shown = visible(entries, true);
        assert_eq!(shown.len(), 1);
        assert!(shown[0].entry.path.is_none());
    }

    #[test]
    fn only_entries_on_show_can_be_used() {
        let library = Library::new(HistoryConfig::default());
        let known = entry(None, Some("https://x"));
        assert!(library.check(&known).is_err());
        library.entries.lock().unwrap().push(known.clone());
        assert!(library.check(&known).is_ok());
        assert!(
            library
                .check(&entry(Some("/etc/passwd".into()), None))
                .is_err()
        );
    }

    #[test]
    fn only_web_links_open() {
        assert!(is_web_url("https://x/delete/1"));
        assert!(is_web_url("HTTP://x"));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("--help"));
        assert!(!is_web_url("javascript:alert(1)"));
    }

    #[test]
    fn thumbnails_fit_the_box_and_change_with_the_file() {
        let dir = temp_dir("thumb");
        let path = dir.join("a.png");
        image::RgbImage::new(800, 200).save(&path).unwrap();
        let small = image::load_from_memory(&render_thumbnail(&path, (250, 150)).unwrap()).unwrap();
        assert_eq!((small.width(), small.height()), (250, 63));

        let before = thumbnail_path(&path, &std::fs::metadata(&path).unwrap(), (250, 150)).unwrap();
        let other = thumbnail_path(&path, &std::fs::metadata(&path).unwrap(), (100, 100)).unwrap();
        image::RgbImage::new(10, 10).save(&path).unwrap();
        let after = thumbnail_path(&path, &std::fs::metadata(&path).unwrap(), (250, 150)).unwrap();
        assert_ne!(before, other);
        assert_ne!(before, after);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stored_thumbnails_leave_no_partial_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("store").join("thumbs");
        let path = dir.join("a.png");
        store(&path, b"png").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"png");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
