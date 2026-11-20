// SPDX-License-Identifier: GPL-3.0-or-later

use crate::error::Result;
use std::fs::{DirBuilder, Metadata};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Cursor, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use accioshot::config::{Config, HistoryConfig};
use accioshot::editor;
use accioshot::fs::{PrivateDir, write_atomically};
use accioshot::history::{self, Entry};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};

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

    fn check(&self, entry: &Entry) -> Result<()> {
        match self.entries.lock()?.contains(entry) {
            true => Ok(()),
            false => Err("this capture is no longer in the history".into()),
        }
    }

    fn split_known(&self, entries: Vec<Entry>) -> (Vec<Entry>, Vec<String>) {
        let (known, stale): (Vec<Entry>, Vec<Entry>) = entries
            .into_iter()
            .partition(|entry| self.check(entry).is_ok());
        let failures = stale
            .iter()
            .map(|entry| format!("{} is no longer in the history", label(entry)))
            .collect();
        (known, failures)
    }

    pub fn holds_files(&self, files: &[PathBuf]) -> bool {
        let Ok(entries) = self.entries.lock() else {
            return false;
        };
        files.iter().all(|file| {
            entries
                .iter()
                .any(|entry| entry.path.as_ref() == Some(file))
        })
    }
}

fn file(entry: &Entry) -> Result<&Path> {
    Ok(entry.path.as_deref().ok_or("this capture has no file")?)
}

fn label(entry: &Entry) -> String {
    entry
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .or_else(|| entry.url.clone())
        .unwrap_or_else(|| entry.time.clone())
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

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    run: impl FnOnce(&Library) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || run(&app.state::<Library>())).await?
}

#[tauri::command]
pub async fn history_items(app: AppHandle) -> Result<Items> {
    blocking(app, items).await
}

fn items(library: &Library) -> Result<Items> {
    let entries = history::recent(library.config.max_items);
    let items = visible(entries, library.config.filter_missing_files);
    *library.entries.lock()? = items.iter().map(|item| item.entry.clone()).collect();
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

#[tauri::command]
pub async fn thumbnail(entry: Entry, app: AppHandle) -> Result<Response> {
    blocking(app, move |library| thumbnail_of(&entry, library)).await
}

fn thumbnail_of(entry: &Entry, library: &Library) -> Result<Response> {
    library.check(entry)?;
    let path = file(entry)?;
    let size = library.config.thumbnail_size();
    Ok(cached_thumbnail(path, size)
        .map(Response::new)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?)
}

fn cached_thumbnail(path: &Path, size: (u32, u32)) -> accioshot::Result<Vec<u8>> {
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

fn render_thumbnail(path: &Path, size: (u32, u32)) -> accioshot::Result<Vec<u8>> {
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
    write_atomically(path, bytes, 0o600)
}

#[tauri::command]
pub async fn image(entry: Entry, app: AppHandle) -> Result<Response> {
    blocking(app, move |library| image_of(&entry, library)).await
}

fn image_of(entry: &Entry, library: &Library) -> Result<Response> {
    library.check(entry)?;
    let path = file(entry)?;
    Ok(std::fs::read(path)
        .map(Response::new)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?)
}

#[tauri::command]
pub async fn copy_entry(entry: Entry, part: Part, app: AppHandle) -> Result<()> {
    blocking(app, move |library| copy_part(&entry, part, library)).await
}

fn copy_part(entry: &Entry, part: Part, library: &Library) -> Result<()> {
    library.check(entry)?;
    match part {
        Part::Image => {
            let path = file(entry)?;
            copy(&[std::ffi::OsStr::new("--image"), path.as_os_str()], "")
        }
        Part::Path => {
            let path = file(entry)?;
            copy(&[], &path.to_string_lossy())
        }
        Part::Url => copy(&[], entry.url.as_deref().ok_or("this capture has no URL")?),
        Part::DeletionUrl | Part::File | Part::Folder => Err("nothing to copy".into()),
    }
}

/// Through accioshot: on Wayland only a process without threads can keep serving the clipboard.
fn copy(args: &[&std::ffi::OsStr], text: &str) -> Result<()> {
    let mut child = Command::new(accioshot::sibling_binary("accioshot"))
        .arg("copy")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run accioshot: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    match output.status.success() {
        true => Ok(()),
        false => Err(String::from_utf8_lossy(&output.stderr).trim().into()),
    }
}

#[tauri::command]
pub async fn edit_entry(entry: Entry, app: AppHandle) -> Result<()> {
    blocking(app, move |library| edit(&entry, library)).await
}

fn edit(entry: &Entry, library: &Library) -> Result<()> {
    library.check(entry)?;
    let path = entry
        .path
        .as_deref()
        .filter(|path| path.exists())
        .ok_or("this capture has no file")?;
    let command = Config::load()?.editor.command;
    edit_file(path, &command, || library.check(entry).is_ok())
}

fn edit_file(path: &Path, command: &str, still_listed: impl Fn() -> bool) -> Result<()> {
    let png =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let Some(edited) = editor::edit(command, &png)? else {
        return Ok(());
    };
    if !still_listed() || !path.exists() {
        return Err("the capture was removed while it was being edited".into());
    }
    if edited.copy_only {
        let dir = PrivateDir::new("accioshot-copy")?;
        let file = dir.path().join("capture.png");
        std::fs::write(&file, &edited.png)?;
        return copy(&[std::ffi::OsStr::new("--image"), file.as_os_str()], "");
    }
    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok(write_atomically(path, &edited.png, mode)
        .map_err(|error| format!("cannot save {}: {error}", path.display()))?)
}

#[tauri::command]
pub fn open_entry(entry: Entry, part: Part, library: State<Library>) -> Result<()> {
    library.check(&entry)?;
    match part {
        Part::File => open(file(&entry)?.as_os_str()),
        Part::Folder => {
            let dir = file(&entry)?.parent().ok_or("this capture has no folder")?;
            open(dir.as_os_str())
        }
        Part::Url => open_url(entry.url.as_deref()),
        Part::DeletionUrl => open_url(entry.deletion_url.as_deref()),
        Part::Image | Part::Path => Err("nothing to open".into()),
    }
}

/// These came from a remote server's response, the desktop must not run anything else.
fn open_url(url: Option<&str>) -> Result<()> {
    let url = url.ok_or("this capture has no such link")?;
    if !accioshot::is_web_url(url) {
        return Err(format!("not a web address: {url}").into());
    }
    open(url.as_ref())
}

fn open(target: &std::ffi::OsStr) -> Result<()> {
    Ok(accioshot::open(target)?)
}

/// The file goes first: if that fails the entry stays, so the delete can be tried again.
#[tauri::command]
pub fn delete_entries(
    entries: Vec<Entry>,
    delete_file: bool,
    library: State<Library>,
) -> Result<Vec<String>> {
    let (entries, mut failures) = library.split_known(entries);
    let size = library.config.thumbnail_size();
    let mut removable = Vec::new();
    for entry in entries {
        if let Some(path) = entry.path.as_deref() {
            if let Ok(file) = std::fs::metadata(path)
                && let Some(cached) = thumbnail_path(path, &file, size)
            {
                std::fs::remove_file(cached).ok();
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
    if !removable.is_empty() {
        history::remove(&removable)?;
    }
    library.entries.lock()?.retain(|e| !removable.contains(e));
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
        std::fs::remove_dir_all(&dir).ok();
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
    fn stale_entries_fail_alone() {
        let library = Library::new(HistoryConfig::default());
        let known = entry(None, Some("https://x"));
        library.entries.lock().unwrap().push(known.clone());
        let stale = entry(Some("/shots/gone.png".into()), None);
        let (entries, failures) = library.split_known(vec![stale, known.clone()]);
        assert_eq!(entries, [known]);
        assert_eq!(failures, ["/shots/gone.png is no longer in the history"]);
    }

    #[test]
    fn only_files_on_show_can_be_dragged() {
        let library = Library::new(HistoryConfig::default());
        library
            .entries
            .lock()
            .unwrap()
            .push(entry(Some("/shots/a.png".into()), None));
        assert!(library.holds_files(&["/shots/a.png".into()]));
        assert!(!library.holds_files(&["/shots/a.png".into(), "/etc/passwd".into()]));
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
    fn edits_replace_the_file_and_failures_say_so() {
        let dir = temp_dir("edit");
        let path = dir.join("a.png");
        let png = b"\x89PNG\r\n\x1a\nold";
        std::fs::write(&path, png).unwrap();
        std::fs::set_permissions(&path, PermissionsExt::from_mode(0o640)).unwrap();
        edit_file(&path, "cp {input} {output}", || true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), png);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(edit_file(&path, "false {input}", || true).is_err());
        assert!(edit_file(&path, "cp {input} {output}", || false).is_err());
        edit_file(&path, "true {output}", || true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), png);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stored_thumbnails_leave_no_partial_file() {
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
