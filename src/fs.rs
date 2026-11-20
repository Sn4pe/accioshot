// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::{DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    )
}

fn temporary_sibling(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}.partial", unique_suffix()))
}

pub struct PrivateDir(PathBuf);

impl PrivateDir {
    pub fn new(prefix: &str) -> io::Result<PrivateDir> {
        let dir = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
        PrivateDir::create(dir.join(format!("{prefix}-{}", unique_suffix())))
    }

    fn create(path: PathBuf) -> io::Result<PrivateDir> {
        std::fs::remove_dir_all(&path).ok();
        DirBuilder::new().mode(0o700).create(&path)?;
        Ok(PrivateDir(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

pub fn write_atomically(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let temporary = temporary_sibling(path);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)
        .and_then(|mut file| file.write_all(data))
        .and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        std::fs::remove_file(&temporary).ok();
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("accioshot-fs-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn private_dirs_are_unique_and_removed() {
        let first = PrivateDir::new("accioshot-fs-test").unwrap();
        let second = PrivateDir::new("accioshot-fs-test").unwrap();
        assert_ne!(first.path(), second.path());
        let mode = std::fs::metadata(first.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        let path = first.path().to_path_buf();
        drop(first);
        assert!(!path.exists());
    }

    #[test]
    fn leftover_directory_is_replaced() {
        let path = temp_dir("leftover");
        std::fs::write(path.join("input.png"), "old").unwrap();
        let dir = PrivateDir::create(path.clone()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn atomic_write_leaves_only_the_result() {
        let dir = temp_dir("atomic");
        let path = dir.join("output.png");
        write_atomically(&path, b"old", 0o600).unwrap();
        write_atomically(&path, b"data", 0o600).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_writes_never_mix() {
        let dir = temp_dir("race");
        let path = dir.join("thumb.png");
        let writers: Vec<_> = (0..8u8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let data = vec![index; 256 * 1024];
                    for _ in 0..20 {
                        write_atomically(&path, &data, 0o600).unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let data = std::fs::read(&path).unwrap();
        assert_eq!(data.len(), 256 * 1024);
        assert!(data.iter().all(|byte| *byte == data[0]));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
