// SPDX-License-Identifier: GPL-3.0-or-later

use accioshot::config::{Config, Theme};
#[cfg(target_os = "linux")]
use gtk::prelude::{StyleContextExt, WidgetExt};
use tauri::WebviewWindow;
use tauri::window::Color;

/// What the page shows before its stylesheet loads, the same as `--bg` in app.css.
pub fn background(window: &WebviewWindow) -> Color {
    match resolved(window) {
        Theme::Frappe => Color(0x30, 0x34, 0x46, 0xff),
        Theme::Macchiato => Color(0x24, 0x27, 0x3a, 0xff),
        Theme::Mocha => Color(0x1e, 0x1e, 0x2e, 0xff),
        Theme::Latte | Theme::System => Color(0xef, 0xf1, 0xf5, 0xff),
    }
}

/// Asked again after saving, so the settings window changes along with the file.
#[tauri::command]
pub fn ui_theme(window: WebviewWindow) -> &'static str {
    match resolved(&window) {
        Theme::Frappe => "frappe",
        Theme::Macchiato => "macchiato",
        Theme::Mocha => "mocha",
        Theme::Latte | Theme::System => "latte",
    }
}

fn resolved(window: &WebviewWindow) -> Theme {
    // A broken config.toml should not stop the window that explains it.
    match Config::load().map_or(Theme::System, |config| config.ui.theme) {
        Theme::System if system_is_dark(window) => Theme::Mocha,
        Theme::System => Theme::Latte,
        theme => theme,
    }
}

/// Themes like Stylix's only recolor GTK and keep the theme's name, so the name and what the web
/// view reports say nothing. The color the windows really get does. Stylix redefines only
/// `window_bg_color`, older themes only have `theme_bg_color`.
#[cfg(target_os = "linux")]
fn system_is_dark(window: &WebviewWindow) -> bool {
    let background = window.gtk_window().ok().and_then(|gtk_window| {
        let style = gtk_window.style_context();
        style
            .lookup_color("window_bg_color")
            .or_else(|| style.lookup_color("theme_bg_color"))
    });
    match background {
        Some(color) => is_dark_color(color.red(), color.green(), color.blue()),
        None => reports_dark(window),
    }
}

#[cfg(not(target_os = "linux"))]
fn system_is_dark(window: &WebviewWindow) -> bool {
    reports_dark(window)
}

fn reports_dark(window: &WebviewWindow) -> bool {
    window
        .theme()
        .is_ok_and(|theme| theme == tauri::Theme::Dark)
}

/// Perceived brightness, each channel from 0 to 1.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn is_dark_color(red: f64, green: f64, blue: f64) -> bool {
    0.299 * red + 0.587 * green + 0.114 * blue < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dark(hex: u32) -> bool {
        let channel = |shift: u32| f64::from((hex >> shift) & 0xff) / 255.0;
        is_dark_color(channel(16), channel(8), channel(0))
    }

    #[test]
    fn common_backgrounds_land_on_the_right_side() {
        assert!(dark(0x1e1e2e), "Catppuccin Mocha");
        assert!(dark(0x242424), "Adwaita dark");
        assert!(!dark(0xeff1f5), "Catppuccin Latte");
        assert!(!dark(0xfafafa), "Adwaita");
    }
}
