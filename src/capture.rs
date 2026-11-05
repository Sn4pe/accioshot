// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use ashpd::desktop::ResponseError;
use ashpd::desktop::screenshot::Screenshot;
use clap::ValueEnum;
use image::{DynamicImage, ImageFormat, ImageReader};
use reqwest::Url;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{Result, region};

pub const CANCELLED: &str = "capture cancelled";

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    /// Select an area on a frozen capture of the screen
    Region,
    /// Active window (Hyprland only)
    Window,
    /// Focused monitor (Hyprland only)
    Monitor,
    /// All monitors
    Screen,
}

pub async fn capture(mode: Mode) -> Result<Vec<u8>> {
    match mode {
        Mode::Region if region::available() => {
            let png = portal(false).await?;
            let image = decode(&png)?.to_rgba8();
            let (area, layout) = region::select(&image)?.ok_or(CANCELLED)?;
            crop(&png, area, &layout)
        }
        Mode::Region => portal(true).await,
        Mode::Screen => portal(false).await,
        // shortcut: crops the full capture, so anything on top of the window shows up.
        // Upgrade to hyprland_toplevel_export_v1 if that matters.
        Mode::Window => {
            let (area, layout) = (active_window()?, monitor_areas(&monitors()?));
            crop(&portal(false).await?, area, &layout)
        }
        Mode::Monitor => {
            let monitors = monitors()?;
            let area = monitors
                .iter()
                .find(|monitor| monitor.focused)
                .ok_or("no focused monitor")?
                .area();
            crop(&portal(false).await?, area, &monitor_areas(&monitors))
        }
    }
}

async fn portal(interactive: bool) -> Result<Vec<u8>> {
    let response = Screenshot::request()
        .interactive(interactive)
        .modal(false)
        .send()
        .await?
        .response()
        .map_err(|error| match error {
            ashpd::Error::Response(ResponseError::Cancelled) => CANCELLED.into(),
            error => Box::<dyn std::error::Error>::from(format!("screenshot portal: {error}")),
        })?;
    let path = Url::parse(response.uri().as_str())?
        .to_file_path()
        .map_err(|_| "the portal did not return a local file")?;
    let png = std::fs::read(&path)?;
    remove_if_temporary(&path);
    Ok(png)
}

/// xdph leaves captures in the runtime dir. Other backends may save to the user's pictures,
/// and those files are not ours to delete.
fn remove_if_temporary(path: &Path) {
    if dirs::runtime_dir().is_some_and(|runtime| path.starts_with(runtime)) {
        let _ = std::fs::remove_file(path);
    }
}

fn decode(png: &[u8]) -> Result<DynamicImage> {
    Ok(ImageReader::with_format(Cursor::new(png), ImageFormat::Png).decode()?)
}

fn crop(png: &[u8], area: Area, layout: &[Area]) -> Result<Vec<u8>> {
    let image = decode(png)?;
    let [x, y, width, height] = pixel_rect(area, layout, image.width(), image.height())?;
    let mut output = Vec::new();
    image
        .crop_imm(x, y, width, height)
        .write_to(&mut Cursor::new(&mut output), ImageFormat::Png)?;
    Ok(output)
}

/// Maps a logical area to pixels in the image of all monitors. The factor comes from the image
/// because with mixed scales the capture uses the largest one.
fn pixel_rect(area: Area, areas: &[Area], image_width: u32, image_height: u32) -> Result<[u32; 4]> {
    let min_x = areas.iter().map(|a| a.x).fold(f64::INFINITY, f64::min);
    let min_y = areas.iter().map(|a| a.y).fold(f64::INFINITY, f64::min);
    let max_x = areas.iter().map(|a| a.x + a.width).fold(f64::MIN, f64::max);
    if areas.is_empty() || max_x <= min_x {
        return Err("no monitors found".into());
    }
    let factor = f64::from(image_width) / (max_x - min_x);

    let left = (((area.x - min_x) * factor).round()).max(0.0) as u32;
    let top = (((area.y - min_y) * factor).round()).max(0.0) as u32;
    let right = (((area.x + area.width - min_x) * factor).round()).max(0.0) as u32;
    let bottom = (((area.y + area.height - min_y) * factor).round()).max(0.0) as u32;
    let (right, bottom) = (right.min(image_width), bottom.min(image_height));
    if left >= right || top >= bottom {
        return Err("the area is outside the screen".into());
    }
    Ok([left, top, right - left, bottom - top])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize)]
struct Monitor {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    transform: u32,
    focused: bool,
}

impl Monitor {
    fn area(&self) -> Area {
        // Hyprland reports unrotated pixels, odd transforms swap the sides.
        let (width, height) = match self.transform % 2 {
            0 => (self.width, self.height),
            _ => (self.height, self.width),
        };
        Area {
            x: f64::from(self.x),
            y: f64::from(self.y),
            width: f64::from(width) / self.scale,
            height: f64::from(height) / self.scale,
        }
    }
}

#[derive(Deserialize)]
struct ActiveWindow {
    at: Option<[i32; 2]>,
    size: Option<[i32; 2]>,
}

fn monitor_areas(monitors: &[Monitor]) -> Vec<Area> {
    monitors.iter().map(Monitor::area).collect()
}

fn monitors() -> Result<Vec<Monitor>> {
    hyprctl("monitors")
}

fn active_window() -> Result<Area> {
    let window: ActiveWindow = hyprctl("activewindow")?;
    let (Some([x, y]), Some([width, height])) = (window.at, window.size) else {
        return Err("no active window".into());
    };
    Ok(Area {
        x: f64::from(x),
        y: f64::from(y),
        width: f64::from(width),
        height: f64::from(height),
    })
}

fn hyprctl<T: DeserializeOwned>(command: &str) -> Result<T> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Err("this mode only works on Hyprland, use region or screen".into());
    }
    let output = Command::new("hyprctl").args(["-j", command]).output()?;
    if !output.status.success() {
        return Err(format!("hyprctl {command} failed").into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, width: u32, height: u32, scale: f64, transform: u32) -> Monitor {
        Monitor {
            x,
            y,
            width,
            height,
            scale,
            transform,
            focused: false,
        }
    }

    fn area(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn second_monitor_is_offset() {
        let layout = monitor_areas(&[
            monitor(0, 0, 1920, 1080, 1.0, 0),
            monitor(1920, 0, 1920, 1080, 1.0, 0),
        ]);
        let rect = pixel_rect(layout[1], &layout, 3840, 1080).unwrap();
        assert_eq!(rect, [1920, 0, 1920, 1080]);
    }

    #[test]
    fn negative_origin_is_normalized() {
        let layout = monitor_areas(&[
            monitor(-1920, -100, 1920, 1080, 1.0, 0),
            monitor(0, 0, 1920, 1080, 1.0, 0),
        ]);
        let rect = pixel_rect(area(10.0, 20.0, 100.0, 50.0), &layout, 3840, 1180).unwrap();
        assert_eq!(rect, [1930, 120, 100, 50]);
    }

    #[test]
    fn scaled_monitor_uses_image_factor() {
        let layout = monitor_areas(&[monitor(0, 0, 3840, 2160, 2.0, 0)]);
        let rect = pixel_rect(area(100.0, 100.0, 200.0, 100.0), &layout, 3840, 2160).unwrap();
        assert_eq!(rect, [200, 200, 400, 200]);
    }

    #[test]
    fn rotated_monitor_swaps_sides() {
        let rotated = monitor(0, 0, 1920, 1080, 1.0, 1);
        assert_eq!(rotated.area(), area(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn window_past_the_edge_is_clamped() {
        let layout = monitor_areas(&[monitor(0, 0, 1920, 1080, 1.0, 0)]);
        let rect = pixel_rect(area(1900.0, 1000.0, 100.0, 200.0), &layout, 1920, 1080).unwrap();
        assert_eq!(rect, [1900, 1000, 20, 80]);
    }

    #[test]
    fn area_outside_the_screen_fails() {
        let layout = monitor_areas(&[monitor(0, 0, 1920, 1080, 1.0, 0)]);
        assert!(pixel_rect(area(3000.0, 0.0, 100.0, 100.0), &layout, 1920, 1080).is_err());
    }
}
