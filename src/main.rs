// SPDX-License-Identifier: GPL-3.0-or-later

mod capture;
mod region;
mod sxcu;
mod syntax;

use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Take a screenshot. Prints the URL if uploaded, otherwise the saved path.
    Capture(CaptureArgs),
}

#[derive(Args)]
struct CaptureArgs {
    mode: capture::Mode,
    /// Save to this directory. Default when no other action is given: accioshot/ in your pictures directory
    #[arg(long, value_name = "DIR")]
    save: Option<PathBuf>,
    /// Copy to the clipboard: the URL if uploading, otherwise the image
    #[arg(long)]
    copy: bool,
    /// Upload with a .sxcu file: a path, or a name in ~/.config/accioshot/uploaders
    #[arg(long, value_name = "UPLOADER")]
    upload: Option<String>,
}

fn main() {
    let Command::Capture(args) = Cli::parse().command;
    if let Err(error) = run(args) {
        eprintln!("accioshot: {error}");
        std::process::exit(1);
    }
}

fn run(args: CaptureArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    // Load first, a broken uploader should fail before the user picks a region.
    let uploader = args.upload.as_deref().map(sxcu::load).transpose()?;
    let png = runtime.block_on(capture::capture(args.mode))?;
    let file_name = chrono::Local::now()
        .format("accioshot_%Y-%m-%d_%H-%M-%S.png")
        .to_string();

    let keep_by_default = !args.copy && uploader.is_none();
    let dir = match args.save {
        Some(dir) => Some(dir),
        None if keep_by_default => Some(default_dir()?),
        None => None,
    };
    let saved = dir
        .as_deref()
        .map(|dir| save(dir, &file_name, &png))
        .transpose()?;

    let uploaded = match &uploader {
        Some(uploader) => {
            match runtime.block_on(sxcu::upload(uploader, png.clone(), &file_name)) {
                Ok(uploaded) => Some(uploaded),
                Err(error) => {
                    // Don't lose the capture if it only existed for the upload.
                    if saved.is_none() {
                        let path = save(&default_dir()?, &file_name, &png)?;
                        eprintln!("accioshot: capture kept at {}", path.display());
                    }
                    return Err(error);
                }
            }
        }
        None => None,
    };

    let output = match (&uploaded, &saved) {
        (Some(uploaded), _) => Some(uploaded.url.clone()),
        (None, Some(path)) => Some(path.display().to_string()),
        (None, None) => None,
    };
    if let Some(deletion_url) = uploaded.as_ref().and_then(|u| u.deletion_url.as_deref()) {
        eprintln!("deletion URL: {deletion_url}");
    }
    if let Some(output) = &output {
        println!("{output}");
        io::stdout().flush()?;
    }

    // The fork below needs a single thread.
    drop(runtime);
    if args.copy {
        let (source, mime) = match uploaded {
            Some(uploaded) => (uploaded.url.into_bytes(), MimeType::Text),
            None => (png, MimeType::Specific("image/png".into())),
        };
        copy_to_clipboard(source, mime)?;
    }
    Ok(())
}

/// On Wayland the clipboard is served by whoever set it, so a detached child keeps doing that
/// after we exit, until something else takes the clipboard. Same approach as wl-copy.
fn copy_to_clipboard(data: Vec<u8>, mime: MimeType) -> Result<()> {
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

fn default_dir() -> Result<PathBuf> {
    let pictures = dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Pictures")))
        .ok_or("cannot find the pictures directory")?;
    Ok(pictures.join("accioshot"))
}

/// Never overwrites: two captures in the same second get a numeric suffix.
fn save(dir: &Path, file_name: &str, png: &[u8]) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stem = file_name.trim_end_matches(".png");
    for attempt in 0..1000 {
        let path = match attempt {
            0 => dir.join(file_name),
            n => dir.join(format!("{stem}-{n}.png")),
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(png)?;
                return Ok(fs::canonicalize(path)?);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(format!("too many captures named {file_name}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_does_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("accioshot-test-{}", std::process::id()));
        let first = save(&dir, "a.png", b"first").unwrap();
        let second = save(&dir, "a.png", b"second").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"first");
        assert_eq!(fs::read(&second).unwrap(), b"second");
        fs::remove_dir_all(dir).unwrap();
    }
}
