// SPDX-License-Identifier: GPL-3.0-or-later

//! What `accioshot` and `accioshot-ui` share.

pub mod config;
pub mod history;
pub mod hotkey;

use std::error::Error;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const OPENER: &str = if cfg!(target_os = "macos") {
    "open"
} else {
    "xdg-open"
};
