// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Result;

/// Action names come from ShareX's AfterCaptureTasks and AfterUploadTasks.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum AfterCapture {
    AnnotateImage,
    SaveImageToFile,
    CopyImageToClipboard,
    UploadImageToHost,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum AfterUpload {
    #[serde(rename = "CopyURLToClipboard")]
    CopyUrlToClipboard,
    #[serde(rename = "OpenURL")]
    OpenUrl,
}

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    pub save_dir: Option<PathBuf>,
    pub uploader: Option<String>,
    pub after_capture: Vec<AfterCapture>,
    pub after_upload: Vec<AfterUpload>,
    pub notifications: bool,
    pub delay: f64,
    pub editor: EditorConfig,
}

/// The built-in editor reads the rest of the [editor] table itself.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    pub command: String,
}

impl Default for EditorConfig {
    fn default() -> Self {
        EditorConfig {
            command: crate::editor::BUILTIN.to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            save_dir: None,
            uploader: None,
            after_capture: vec![AfterCapture::SaveImageToFile],
            after_upload: vec![AfterUpload::CopyUrlToClipboard],
            notifications: true,
            delay: 0.0,
            editor: EditorConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Config> {
        let dir = dirs::config_dir().ok_or("cannot find the config directory")?;
        Config::load_from(&dir.join("accioshot/config.toml"))
    }

    fn load_from(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()).into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(format!("cannot read {}: {error}", path.display()).into()),
        }
    }

    pub fn pictures_dir(&self) -> Result<PathBuf> {
        match &self.save_dir {
            Some(dir) => Ok(expand_home(dir)),
            None => default_dir(),
        }
    }
}

fn default_dir() -> Result<PathBuf> {
    let pictures = dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Pictures")))
        .ok_or("cannot find the pictures directory")?;
    Ok(pictures.join("accioshot"))
}

fn expand_home(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), dirs::home_dir()) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_the_defaults() {
        let config = Config::load_from(Path::new("/nonexistent/config.toml")).unwrap();
        assert_eq!(config.after_capture, [AfterCapture::SaveImageToFile]);
        assert!(config.notifications);
        assert_eq!(config.editor.command, "builtin");
    }

    #[test]
    fn parses_sharex_action_names() {
        let config: Config = toml::from_str(
            r#"
            uploader = "imgur"
            after_capture = ["SaveImageToFile", "UploadImageToHost"]
            after_upload = ["CopyURLToClipboard", "OpenURL"]
            notifications = false
            "#,
        )
        .unwrap();
        assert_eq!(config.uploader.as_deref(), Some("imgur"));
        assert_eq!(config.after_capture[1], AfterCapture::UploadImageToHost);
        assert_eq!(config.after_upload[1], AfterUpload::OpenUrl);
        assert!(!config.notifications);
    }

    #[test]
    fn editor_table_leaves_builtin_settings_alone() {
        let config: Config =
            toml::from_str("[editor]\ncommand = \"satty\"\ntool = \"Arrow\"").unwrap();
        assert_eq!(config.editor.command, "satty");
    }

    #[test]
    fn typos_are_rejected() {
        assert!(toml::from_str::<Config>("after_capture = [\"Nope\"]").is_err());
        assert!(toml::from_str::<Config>("save_directory = \"/tmp\"").is_err());
    }

    #[test]
    fn save_dir_expands_home() {
        let config = Config {
            save_dir: Some("~/shots".into()),
            ..Config::default()
        };
        let home = dirs::home_dir().unwrap();
        assert_eq!(config.pictures_dir().unwrap(), home.join("shots"));
    }
}
