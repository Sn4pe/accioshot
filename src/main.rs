// SPDX-License-Identifier: GPL-3.0-or-later

mod capture;
mod config;
mod daemon;
mod editor;
mod history;
mod notify;
mod region;
mod sxcu;
mod syntax;

use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use clap::{Args, Parser, Subcommand};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

use config::{AfterCapture, AfterUpload, Config};

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
    /// Run the tray icon
    Daemon,
}

#[derive(Args)]
struct CaptureArgs {
    mode: capture::Mode,
    /// Save to this directory. Any flag replaces the actions in the config
    #[arg(long, value_name = "DIR")]
    save: Option<PathBuf>,
    /// Copy to the clipboard: the URL if uploading, otherwise the image
    #[arg(long)]
    copy: bool,
    /// Open the capture in the editor first, the other actions get the edited image
    #[arg(long)]
    edit: bool,
    /// Upload with a .sxcu file: a path, or a name in ~/.config/accioshot/uploaders
    #[arg(long, value_name = "UPLOADER")]
    upload: Option<String>,
}

fn main() {
    let (result, is_capture) = match Cli::parse().command {
        Command::Capture(args) => (run(args), true),
        Command::Daemon => (daemon::run(), false),
    };
    if let Err(error) = result {
        let message = error.to_string();
        eprintln!("accioshot: {message}");
        if is_capture && message != capture::CANCELLED {
            notify_failure(&message);
        }
        std::process::exit(1);
    }
}

/// From a key binding nobody sees stderr.
fn notify_failure(message: &str) {
    let enabled = Config::load().map_or(true, |config| config.notifications);
    if enabled
        && let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
    {
        let _ = runtime.block_on(notify::send("Capture failed", message, None));
    }
}

/// What to do with a capture. Any flag replaces the whole config, so a script gets exactly what
/// it asked for.
#[derive(Debug, PartialEq)]
struct Actions {
    edit: bool,
    save: Option<PathBuf>,
    copy_image: bool,
    upload: Option<String>,
    copy_url: bool,
    open_url: bool,
}

impl Actions {
    fn new(args: &CaptureArgs, config: &Config) -> Result<Actions> {
        let mut actions = if args.save.is_some() || args.copy || args.edit || args.upload.is_some()
        {
            Actions {
                edit: args.edit,
                save: args.save.clone(),
                copy_image: args.copy && args.upload.is_none(),
                upload: args.upload.clone(),
                copy_url: args.copy && args.upload.is_some(),
                open_url: false,
            }
        } else {
            let wants = |action| config.after_capture.contains(&action);
            let upload = match wants(AfterCapture::UploadImageToHost) {
                true => Some(
                    config
                        .uploader
                        .clone()
                        .ok_or("UploadImageToHost needs `uploader` in the config")?,
                ),
                false => None,
            };
            Actions {
                edit: wants(AfterCapture::AnnotateImage),
                save: wants(AfterCapture::SaveImageToFile)
                    .then(|| config.pictures_dir())
                    .transpose()?,
                copy_image: wants(AfterCapture::CopyImageToClipboard),
                copy_url: upload.is_some()
                    && config
                        .after_upload
                        .contains(&AfterUpload::CopyUrlToClipboard),
                open_url: upload.is_some() && config.after_upload.contains(&AfterUpload::OpenUrl),
                upload,
            }
        };
        // A capture nothing happens to would be lost.
        if actions.save.is_none() && !actions.copy_image && actions.upload.is_none() {
            actions.save = Some(config.pictures_dir()?);
        }
        Ok(actions)
    }

    /// The editor's copy key: to the clipboard and nothing else.
    fn only_copy(self) -> Actions {
        Actions {
            edit: self.edit,
            save: None,
            copy_image: true,
            upload: None,
            copy_url: false,
            open_url: false,
        }
    }
}

/// For a failure that would lose the only copy of the capture.
fn keep_capture(config: &Config, file_name: &str, png: &[u8], error: &dyn Error) -> Box<dyn Error> {
    match config
        .pictures_dir()
        .and_then(|dir| save(&dir, file_name, png))
    {
        Ok(path) => format!("{error}, capture kept at {}", path.display()).into(),
        Err(keep_error) => format!("{error}, and the capture is lost: {keep_error}").into(),
    }
}

fn run(args: CaptureArgs) -> Result<()> {
    let config = Config::load()?;
    let mut actions = Actions::new(&args, &config)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    // Load first, a broken uploader should fail before the user picks a region.
    let mut uploader = actions.upload.as_deref().map(sxcu::load).transpose()?;
    let mut png = runtime.block_on(capture::capture(args.mode))?;
    let now = chrono::Local::now();
    let file_name = now.format("accioshot_%Y-%m-%d_%H-%M-%S.png").to_string();
    if actions.edit {
        let edited = editor::edit(&config.editor.command, &png)
            .map_err(|error| keep_capture(&config, &file_name, &png, error.as_ref()))?
            .ok_or(capture::CANCELLED)?;
        png = edited.png;
        if edited.copy_only {
            actions = actions.only_copy();
            uploader = None;
        }
    }

    let saved = actions
        .save
        .as_deref()
        .map(|dir| save(dir, &file_name, &png))
        .transpose()?;

    let uploaded = match &uploader {
        Some(uploader) => {
            match runtime.block_on(sxcu::upload(uploader, png.clone(), &file_name)) {
                Ok(uploaded) => Some(uploaded),
                Err(error) => {
                    // Don't lose the capture if it only existed for the upload.
                    return Err(match saved {
                        Some(_) => error,
                        None => keep_capture(&config, &file_name, &png, error.as_ref()),
                    });
                }
            }
        }
        None => None,
    };

    let url = uploaded.as_ref().map(|uploaded| uploaded.url.as_str());
    let deletion_url = uploaded.as_ref().and_then(|u| u.deletion_url.as_deref());
    let output = url
        .map(str::to_string)
        .or_else(|| saved.as_ref().map(|path| path.display().to_string()));
    if let Some(deletion_url) = deletion_url {
        eprintln!("deletion URL: {deletion_url}");
    }
    if let Some(output) = &output {
        println!("{output}");
        io::stdout().flush()?;
    }

    if saved.is_some() || uploaded.is_some() {
        let entry = history::Entry {
            time: now.to_rfc3339(),
            path: saved.clone(),
            url: url.map(str::to_string),
            deletion_url: deletion_url.map(str::to_string),
        };
        // The capture is safe already, a broken history must not fail it.
        if let Err(error) = history::append(&entry) {
            eprintln!("accioshot: cannot write the history: {error}");
        }
    }

    if actions.open_url
        && let Some(url) = url
    {
        open_url(url);
    }
    if config.notifications {
        let (summary, body) = match (url, &saved) {
            (Some(url), _) => ("Capture uploaded", url.to_string()),
            (None, Some(path)) => ("Capture saved", path.display().to_string()),
            (None, None) => ("Capture copied", String::new()),
        };
        let notified = runtime.block_on(notify::send(summary, &body, saved.as_deref()));
        if let Err(error) = notified {
            eprintln!("accioshot: cannot show the notification: {error}");
        }
    }

    // The fork below needs a single thread.
    drop(runtime);
    if actions.copy_url
        && let Some(uploaded) = uploaded
    {
        copy_to_clipboard(uploaded.url.into_bytes(), MimeType::Text)?;
    } else if actions.copy_image
        && let Err(error) = copy_to_clipboard(png.clone(), MimeType::Specific("image/png".into()))
    {
        // After the editor's copy key the clipboard was the only destination.
        return Err(match saved {
            Some(_) => error,
            None => keep_capture(&config, &file_name, &png, error.as_ref()),
        });
    }
    Ok(())
}

/// The URL comes from a remote server, so only web links get opened.
fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        eprintln!("accioshot: not opening a non-web URL: {url}");
        return;
    }
    let spawned = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(error) = spawned {
        eprintln!("accioshot: cannot run xdg-open: {error}");
    }
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

    fn args(save: Option<&str>, copy: bool, upload: Option<&str>) -> CaptureArgs {
        CaptureArgs {
            mode: capture::Mode::Screen,
            save: save.map(PathBuf::from),
            copy,
            edit: false,
            upload: upload.map(str::to_string),
        }
    }

    fn config(toml_text: &str) -> Config {
        let mut config: Config = toml::from_str(toml_text).unwrap();
        config.save_dir = Some("/shots".into());
        config
    }

    #[test]
    fn flags_replace_the_config() {
        let config = config("after_capture = [\"UploadImageToHost\"]\nuploader = \"a\"");
        let actions = Actions::new(&args(None, true, None), &config).unwrap();
        assert!(actions.copy_image && actions.save.is_none() && actions.upload.is_none());
    }

    #[test]
    fn config_can_open_the_editor() {
        let config = config("after_capture = [\"AnnotateImage\", \"CopyImageToClipboard\"]");
        let actions = Actions::new(&args(None, false, None), &config).unwrap();
        assert!(actions.edit && actions.copy_image);
    }

    #[test]
    fn edit_flag_alone_still_keeps_the_result() {
        let mut flags = args(None, false, None);
        flags.edit = true;
        let actions = Actions::new(&flags, &config("")).unwrap();
        assert!(actions.edit);
        assert_eq!(actions.save.as_deref(), Some(Path::new("/shots")));
    }

    #[test]
    fn copy_means_the_url_when_uploading() {
        let actions = Actions::new(&args(None, true, Some("a")), &config("")).unwrap();
        assert!(actions.copy_url && !actions.copy_image);
    }

    #[test]
    fn config_drives_a_bare_capture() {
        let config = config(
            "after_capture = [\"SaveImageToFile\", \"UploadImageToHost\"]\n\
             uploader = \"a\"\nafter_upload = [\"OpenURL\"]",
        );
        let actions = Actions::new(&args(None, false, None), &config).unwrap();
        assert_eq!(actions.save.as_deref(), Some(Path::new("/shots")));
        assert_eq!(actions.upload.as_deref(), Some("a"));
        assert!(actions.open_url && !actions.copy_url);
    }

    #[test]
    fn upload_without_an_uploader_fails() {
        let config = config("after_capture = [\"UploadImageToHost\"]");
        assert!(Actions::new(&args(None, false, None), &config).is_err());
    }

    #[test]
    fn empty_actions_still_keep_the_capture() {
        let actions =
            Actions::new(&args(None, false, None), &config("after_capture = []")).unwrap();
        assert_eq!(actions.save.as_deref(), Some(Path::new("/shots")));
    }

    #[test]
    fn only_copy_drops_every_other_action() {
        let config = config("after_upload = [\"OpenURL\"]");
        let actions = Actions::new(&args(Some("/x"), true, Some("a")), &config).unwrap();
        let actions = actions.only_copy();
        assert!(actions.copy_image && !actions.copy_url && !actions.open_url);
        assert!(actions.save.is_none() && actions.upload.is_none());
    }

    #[test]
    fn failure_keeps_the_capture() {
        let dir = std::env::temp_dir().join(format!("accioshot-keep-{}", std::process::id()));
        let mut config = config("");
        config.save_dir = Some(dir.clone());
        let broken: Box<dyn Error> = "broken".into();
        let error = keep_capture(&config, "a.png", b"capture", broken.as_ref());
        assert!(error.to_string().starts_with("broken, capture kept at"));
        assert_eq!(fs::read(dir.join("a.png")).unwrap(), b"capture");
        fs::remove_dir_all(dir).unwrap();
    }

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
