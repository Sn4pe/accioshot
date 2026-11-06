// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::Result;

pub const BUILTIN_EDITOR: &str = "builtin";
pub const DEFAULT_NAME_PATTERN: &str = "accioshot_%y-%mo-%d_%h-%mi-%s";

/// Action names come from ShareX's AfterCaptureTasks and AfterUploadTasks.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum AfterCapture {
    AnnotateImage,
    SaveImageToFile,
    CopyImageToClipboard,
    UploadImageToHost,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum AfterUpload {
    #[serde(rename = "CopyURLToClipboard")]
    CopyUrlToClipboard,
    #[serde(rename = "OpenURL")]
    OpenUrl,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    pub after_capture: Vec<AfterCapture>,
    pub after_upload: Vec<AfterUpload>,
    pub notifications: bool,
    pub delay: f64,
    pub name_pattern: String,
    pub region: RegionConfig,
    pub editor: EditorConfig,
    pub tray: TrayConfig,
    pub history: HistoryConfig,
    pub ui: UiConfig,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub theme: Theme,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryConfig {
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub max_items: usize,
    pub filter_missing_files: bool,
}

/// ShareX's defaults, from ImageHistorySettings.
impl Default for HistoryConfig {
    fn default() -> Self {
        HistoryConfig {
            thumbnail_width: 250,
            thumbnail_height: 150,
            max_items: 500,
            filter_missing_files: false,
        }
    }
}

/// Read on macOS only, on Linux the compositor owns global shortcuts.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrayConfig {
    pub region_hotkey: String,
}

impl Default for TrayConfig {
    fn default() -> Self {
        TrayConfig {
            region_hotkey: "Shift+Cmd+4".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RegionConfig {
    pub snap_windows: bool,
    pub magnifier: bool,
    pub magnifier_size: u32,
    pub magnifier_pixels: u32,
}

/// ShareX's defaults, from RegionCaptureOptions.
impl Default for RegionConfig {
    fn default() -> Self {
        RegionConfig {
            snap_windows: true,
            magnifier: true,
            magnifier_size: 150,
            magnifier_pixels: 15,
        }
    }
}

/// The built-in editor reads the rest of the [editor] table itself.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct EditorConfig {
    pub command: String,
}

impl Default for EditorConfig {
    fn default() -> Self {
        EditorConfig {
            command: BUILTIN_EDITOR.to_string(),
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
            name_pattern: DEFAULT_NAME_PATTERN.to_string(),
            region: RegionConfig::default(),
            editor: EditorConfig::default(),
            tray: TrayConfig::default(),
            history: HistoryConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

/// `~/.config` on macOS too, so the same files work on both systems.
pub fn base_dir() -> Option<PathBuf> {
    match cfg!(target_os = "macos") {
        true => dirs::home_dir().map(|home| home.join(".config")),
        false => dirs::config_dir(),
    }
}

pub fn path() -> Result<PathBuf> {
    let dir = base_dir().ok_or("cannot find the config directory")?;
    Ok(dir.join("accioshot/config.toml"))
}

/// Only the names: `.sxcu` files can hold API keys.
pub fn uploader_names() -> Vec<String> {
    let Some(dir) = base_dir() else {
        return Vec::new();
    };
    let Ok(files) = std::fs::read_dir(dir.join("accioshot/uploaders")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = files
        .flatten()
        .filter_map(|file| {
            let path = file.path();
            (path.extension()? == "sxcu").then(|| path.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort();
    names
}

/// Applies `config` to the file's text. Comments, unknown tables and keys left at their
/// value stay as they are, and a default nobody set stays out of the file.
fn patched(text: &str, config: &Config) -> Result<String> {
    let mut document: DocumentMut = text.parse()?;
    let old: toml::Table = toml::from_str(text)?;
    let new = toml::Table::try_from(config)?;
    merge(
        document.as_table_mut(),
        &old,
        &new,
        &toml::Table::try_from(Config::default())?,
    )?;
    for key in ["save_dir", "uploader"] {
        if !new.contains_key(key) {
            document.remove(key);
        }
    }
    Ok(document.to_string())
}

fn merge(
    document: &mut dyn TableLike,
    old: &toml::Table,
    new: &toml::Table,
    default: &toml::Table,
) -> Result<()> {
    let empty = toml::Table::new();
    for (key, value) in new {
        let was = old.get(key);
        if let toml::Value::Table(value) = value {
            let default = default
                .get(key)
                .and_then(toml::Value::as_table)
                .unwrap_or(&empty);
            if was.is_none() && value == default {
                continue;
            }
            if !document.contains_key(key) {
                document.insert(key, toml_edit::table());
            }
            let table = document
                .get_mut(key)
                .and_then(Item::as_table_like_mut)
                .ok_or_else(|| format!("{key} must be a table"))?;
            let was = was.and_then(toml::Value::as_table).unwrap_or(&empty);
            merge(table, was, value, default)?;
        } else if was.or(default.get(key)) != Some(value) {
            let mut replacement: toml_edit::Value = value.to_string().parse()?;
            // In place: inserting would replace the key too, and the comment above it lives there.
            match document.get_mut(key) {
                Some(current) => {
                    if let Some(value) = current.as_value() {
                        *replacement.decor_mut() = value.decor().clone();
                    }
                    *current = Item::Value(replacement);
                }
                None => {
                    document.insert(key, Item::Value(replacement));
                }
            }
        }
    }
    Ok(())
}

impl Config {
    pub fn load() -> Result<Config> {
        Config::load_from(&path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("cannot read {}: {error}", path.display()).into()),
        };
        let text = patched(&text, self).map_err(|error| format!("{}: {error}", path.display()))?;
        toml::from_str::<Config>(&text)?;

        // Dotfile managers link this file, and a rename would replace the link.
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let permissions = std::fs::metadata(&target).map(|file| file.permissions());
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = target.with_extension("toml.partial");
        std::fs::write(&temporary, text)
            .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
        if let Ok(permissions) = permissions {
            std::fs::set_permissions(&temporary, permissions)?;
        }
        std::fs::rename(temporary, &target)?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if !self.delay.is_finite() || self.delay < 0.0 {
            return Err("the delay cannot be negative".into());
        }
        if self.name_pattern.trim().is_empty() {
            return Err("the file name cannot be empty".into());
        }
        if self.editor.command.trim().is_empty() {
            return Err("the editor command cannot be empty".into());
        }
        if self.history.thumbnail_width == 0
            || self.history.thumbnail_height == 0
            || self.history.max_items == 0
        {
            return Err("history sizes must be above zero".into());
        }
        if !self.tray.region_hotkey.is_empty() {
            crate::hotkey::parse(&self.tray.region_hotkey)?;
        }
        Ok(())
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
    fn hotkey_can_change_or_be_empty() {
        assert_eq!(Config::default().tray.region_hotkey, "Shift+Cmd+4");
        let config: Config = toml::from_str("[tray]\nregion_hotkey = \"\"").unwrap();
        assert_eq!(config.tray.region_hotkey, "");
    }

    fn save(text: &str, change: impl FnOnce(&mut Config)) -> String {
        let mut config: Config = toml::from_str(text).unwrap();
        change(&mut config);
        patched(text, &config).unwrap()
    }

    #[test]
    fn theme_is_read_and_stays_out_until_changed() {
        let config: Config = toml::from_str("[ui]\ntheme = \"frappe\"").unwrap();
        assert_eq!(config.ui.theme, Theme::Frappe);
        assert!(toml::from_str::<Config>("[ui]\ntheme = \"blue\"").is_err());
        assert_eq!(save("", |_| {}), "");
        let text = save("", |config| config.ui.theme = Theme::Mocha);
        assert_eq!(text.trim(), "[ui]\ntheme = \"mocha\"");
    }

    #[test]
    fn saving_nothing_keeps_the_file_as_it_was() {
        let text = "# mine\nsave_dir = \"~/shots\" # here\n\n[editor]\ntool = \"Text\"\ncolors = [\n  \"#fff\", # white\n]\n[region]\nmagnifier = true\n";
        assert_eq!(save(text, |_| {}), text);
    }

    #[test]
    fn a_new_file_only_gets_what_changed() {
        let text = save("", |config| config.region.magnifier = false);
        assert_eq!(text.trim(), "[region]\nmagnifier = false");
    }

    #[test]
    fn changes_keep_comments_and_the_rest_of_the_editor_table() {
        let text = save(
            "# mine\ndelay = 1.0 # wait\n[editor]\ntool = \"Text\"\n",
            |config| {
                config.delay = 2.5;
                config.editor.command = "satty {input}".to_string();
            },
        );
        assert!(text.starts_with("# mine\ndelay = 2.5 # wait\n"));
        assert!(text.contains("tool = \"Text\""));
        assert!(text.contains("command = \"satty {input}\""));
        assert_eq!(
            toml::from_str::<Config>(&text).unwrap().editor.command,
            "satty {input}"
        );
    }

    #[test]
    fn clearing_an_option_removes_its_key() {
        let text = save("save_dir = \"/a\"\nuploader = \"b\"\n", |config| {
            config.save_dir = None;
            config.uploader = None;
        });
        assert_eq!(text.trim(), "");
    }

    #[test]
    fn actions_keep_their_sharex_names() {
        let text = save("", |config| {
            config.after_upload = vec![AfterUpload::OpenUrl];
        });
        assert!(text.contains("after_upload = [\"OpenURL\"]"));
    }

    #[test]
    fn inline_tables_can_be_patched() {
        let text = save("region = { magnifier = true }\n", |config| {
            config.region.magnifier = false
        });
        assert!(!toml::from_str::<Config>(&text).unwrap().region.magnifier);
    }

    #[test]
    fn invalid_values_are_not_saved() {
        let dir = std::env::temp_dir().join(format!("accioshot-config-{}", std::process::id()));
        let path = dir.join("config.toml");
        let config = Config {
            delay: -1.0,
            ..Config::default()
        };
        assert!(config.save_to(&path).is_err());
        let config = Config {
            tray: TrayConfig {
                region_hotkey: "Hyper+4".to_string(),
            },
            ..Config::default()
        };
        assert!(config.save_to(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn saving_writes_through_a_link_and_keeps_its_mode() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("accioshot-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (real, link) = (dir.join("real.toml"), dir.join("config.toml"));
        std::fs::write(&real, "delay = 1.0\n").unwrap();
        std::fs::set_permissions(&real, PermissionsExt::from_mode(0o640)).unwrap();
        symlink(&real, &link).unwrap();
        let config = Config {
            delay: 3.0,
            ..Config::default()
        };
        config.save_to(&link).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "delay = 3.0\n");
        let mode = std::fs::metadata(&real).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broken_files_are_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("accioshot-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "delay = [").unwrap();
        assert!(Config::default().save_to(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "delay = [");
        std::fs::remove_dir_all(dir).unwrap();
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
