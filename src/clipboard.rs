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

    fn copy(data: Vec<u8>, mime: MimeType) -> Result<()> {
        let mut options = Options::new();
        options.foreground(true);
        let prepared = options.prepare_copy(Source::Bytes(data.into()), mime)?;
        let dev_null = File::options().read(true).write(true).open("/dev/null")?;

        match unsafe { libc::fork() } {
            -1 => Err(io::Error::last_os_error().into()),
            0 => {
                unsafe {
                    libc::setsid();
                    for fd in 0..=2 {
                        libc::dup2(dev_null.as_raw_fd(), fd);
                    }
                }
                std::env::set_current_dir("/").ok();
                prepared.serve().ok();
                unsafe { libc::_exit(0) }
            }
            _ => Ok(()),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use accioshot::fs::PrivateDir;

    use crate::Result;

    pub fn copy_text(text: String) -> Result<()> {
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
        let dir = PrivateDir::new("accioshot-clipboard")?;
        let path = dir.path().join("capture.png");
        std::fs::write(&path, png)?;
        crate::notify::osascript(
            "set the clipboard to (read (POSIX file (item 1 of argv)) as «class PNGf»)",
            &[path.as_os_str()],
        )
        .map_err(|error| format!("cannot copy the image: {error}").into())
    }
}
