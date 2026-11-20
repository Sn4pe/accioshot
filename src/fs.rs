// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
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
