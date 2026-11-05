// SPDX-License-Identifier: GPL-3.0-or-later

pub use platform::{copy_image, copy_text};

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::File;
    use std::io;
    use std::os::fd::AsRawFd;

    use wl_clipboard_rs::copy::{MimeType, Options, Source};

    use crate::Result;

    pub fn copy_text(text: String) -> Result<()> {
        copy(text.into_bytes(), MimeType::Text)
    }

    pub fn copy_image(png: &[u8]) -> Result<()> {
        copy(png.to_vec(), MimeType::Specific("image/png".into()))
    }

    /// On Wayland the clipboard is served by whoever set it, so a detached child keeps doing that
    /// after we exit, until something else takes the clipboard. Same approach as wl-copy.
    fn copy(data: Vec<u8>, mime: MimeType) -> Result<()> {
        let mut options = Options::new();
        options.foreground(true);
        let prepared = options.prepare_copy(Source::Bytes(data.into()), mime)?;
        let dev_null = File::options().read(true).write(true).open("/dev/null")?;

        // SAFETY: the process is single threaded here, and the child only makes plain syscalls
        // before serving.
        match unsafe { libc::fork() } {
            -1 => Err(io::Error::last_os_error().into()),
            0 => {
                // Inherited stdio would keep `url=$(accioshot ...)` waiting for the child.
                unsafe {
                    libc::setsid();
                    for fd in 0..=2 {
                        libc::dup2(dev_null.as_raw_fd(), fd);
                    }
                }
                let _ = std::env::set_current_dir("/");
                let _ = prepared.serve();
                std::process::exit(0);
            }
            _ => Ok(()),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::{Command, Stdio};

    use crate::Result;

    pub fn copy_text(text: String) -> Result<()> {
        // pbcopy decodes its input by locale, and launchd starts us without one.
        let mut child = Command::new("pbcopy")
            .env("LC_ALL", "en_US.UTF-8")
            .stdin(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("cannot write to pbcopy")?
            .write_all(text.as_bytes())?;
        match child.wait()?.success() {
            true => Ok(()),
            false => Err("pbcopy failed".into()),
        }
    }

    pub fn copy_image(png: &[u8]) -> Result<()> {
        let path =
            std::env::temp_dir().join(format!("accioshot-clipboard-{}.png", std::process::id()));
        // A killed run with this same pid leaves its file behind.
        let _ = std::fs::remove_file(&path);
        // 0600: this is a screenshot, it may hold anything.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?
            .write_all(png)?;
        let output = Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "set the clipboard to (read (POSIX file (item 1 of argv)) as «class PNGf»)",
                "-e",
                "end run",
            ])
            .arg(&path)
            .output();
        let _ = std::fs::remove_file(&path);
        let output = output?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(format!("cannot copy the image: {}", error.trim()).into());
        }
        Ok(())
    }
}
