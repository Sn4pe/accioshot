// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem, SubMenu};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

use super::{
    HISTORY_POLL, MENU_CLOSE_DELAY, MODES, RECENT_COUNT, entry_label, entry_text, history_modified,
    open, single_instance, spawn_capture, spawn_ui,
};
use crate::Result;
use crate::config::Config;
use crate::history::{self, Entry};

pub fn run() -> Result<()> {
    let _lock = single_instance()?;
    let config = Config::load()?;
    let tray = Tray {
        recent: history::recent(RECENT_COUNT),
        pictures_dir: config.pictures_dir()?,
        // Asked now: once an update replaces the binary, the kernel reports it as "(deleted)".
        exe: std::env::current_exe()?,
    };
    let handle = tray.spawn()?;

    // shortcut: polls the file because captures are separate processes, use inotify if a two
    // second delay in the menu ever matters.
    let mut seen = history_modified();
    loop {
        std::thread::sleep(HISTORY_POLL);
        let modified = history_modified();
        if modified != seen {
            seen = modified;
            let recent = history::recent(RECENT_COUNT);
            if handle.update(|tray| tray.recent = recent).is_none() {
                return Err("the tray stopped".into());
            }
        }
    }
}

struct Tray {
    recent: Vec<Entry>,
    pictures_dir: PathBuf,
    exe: PathBuf,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "accioshot".into()
    }

    fn title(&self) -> String {
        "accioshot".into()
    }

    fn icon_name(&self) -> String {
        "camera-photo".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        spawn_capture(self.exe.clone(), "region", MENU_CLOSE_DELAY);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu: Vec<MenuItem<Self>> = MODES
            .into_iter()
            .map(|(label, mode)| {
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| {
                        spawn_capture(tray.exe.clone(), mode, MENU_CLOSE_DELAY)
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();

        menu.push(MenuItem::Separator);
        menu.push(
            SubMenu {
                label: "Recent".into(),
                enabled: !self.recent.is_empty(),
                submenu: self.recent.iter().map(recent_item).collect(),
                ..Default::default()
            }
            .into(),
        );
        for (label, window) in [("History", "history"), ("Settings", "settings")] {
            menu.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| spawn_ui(&tray.exe, window)),
                    ..Default::default()
                }
                .into(),
            );
        }
        let dir = self.pictures_dir.clone();
        menu.push(
            StandardItem {
                label: "Open folder".into(),
                activate: Box::new(move |_: &mut Self| open(dir.as_os_str())),
                ..Default::default()
            }
            .into(),
        );
        menu.push(MenuItem::Separator);
        menu.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );
        menu
    }
}

fn recent_item(entry: &Entry) -> MenuItem<Tray> {
    let text = entry_text(entry);
    StandardItem {
        label: menu_label(&entry_label(entry)),
        activate: Box::new(move |_: &mut Tray| {
            // In process, the daemon lives on to serve the clipboard.
            let source = Source::Bytes(text.clone().into_bytes().into());
            if let Err(error) = Options::new().copy(source, MimeType::Text) {
                eprintln!("accioshot: {error}");
            }
        }),
        ..Default::default()
    }
    .into()
}

/// Single underscores are mnemonics in menu labels, and capture names are full of them.
fn menu_label(text: &str) -> String {
    text.replace('_', "__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscores_survive_menus() {
        assert_eq!(menu_label("accioshot_2026_a.png"), "accioshot__2026__a.png");
    }
}
