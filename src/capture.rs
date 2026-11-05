// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use ashpd::desktop::ResponseError;
use ashpd::desktop::screenshot::Screenshot;
use clap::ValueEnum;
use image::{DynamicImage, ImageFormat, ImageReader};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::config::RegionConfig;
use crate::names::Window;
use crate::{Result, region};

pub const CANCELLED: &str = "capture cancelled";

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    /// Select an area on a frozen capture of the screen
    Region,
    /// The last selected area, asks for one if there is none or the screen size changed
    LastRegion,
    /// Active window (Hyprland only)
    Window,
    /// Focused monitor (Hyprland only)
    Monitor,
    /// All monitors
    Screen,
}

pub async fn capture(mode: Mode, options: &RegionConfig) -> Result<Vec<u8>> {
    match mode {
        Mode::Region => select_region(options).await,
        Mode::LastRegion => {
            let Some(last) = LastRegion::load() else {
                return select_region(options).await;
            };
            let image = decode(&portal(false).await?)?;
            match last.fits(image.width(), image.height()) {
                true => encode_crop(&image, last.rect),
                false => select_region(options).await,
            }
        }
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

/// shortcut: the desktop's own picker does not tell us the area, so only our selector remembers it.
async fn select_region(options: &RegionConfig) -> Result<Vec<u8>> {
    if !region::available() {
        return portal(true).await;
    }
    // Before the capture, so they match what gets frozen. Off Hyprland there are none to click.
    let windows = match options.snap_windows {
        true => visible_windows().unwrap_or_default(),
        false => Vec::new(),
    };
    let image = decode(&portal(false).await?)?;
    let magnifier = options
        .magnifier
        .then(|| region::Magnifier::new(options.magnifier_size, options.magnifier_pixels));
    let (area, layout) = region::select(&image.to_rgba8(), windows, magnifier)?.ok_or(CANCELLED)?;
    let rect = pixel_rect(area, &layout, image.width(), image.height())?;
    let last = LastRegion {
        rect,
        image: [image.width(), image.height()],
    };
    // The capture is fine without it.
    if let Err(error) = last.save() {
        eprintln!("accioshot: cannot remember the region: {error}");
    }
    encode_crop(&image, rect)
}

/// Pixels instead of the logical area: replaying it needs no monitor layout, and a screen of
/// another size is caught by comparing the image.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct LastRegion {
    rect: [u32; 4],
    image: [u32; 2],
}

impl LastRegion {
    fn path() -> Option<PathBuf> {
        Some(dirs::state_dir()?.join("accioshot/last-region.json"))
    }

    fn load() -> Option<LastRegion> {
        LastRegion::load_from(&LastRegion::path()?)
    }

    fn load_from(path: &Path) -> Option<LastRegion> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    fn save(&self) -> Result<()> {
        let path = LastRegion::path().ok_or("cannot find the state directory")?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
    }

    fn fits(&self, width: u32, height: u32) -> bool {
        let [x, y, rect_width, rect_height] = self.rect;
        self.image == [width, height]
            && rect_width > 0
            && rect_height > 0
            && x.checked_add(rect_width)
                .is_some_and(|right| right <= width)
            && y.checked_add(rect_height)
                .is_some_and(|bottom| bottom <= height)
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
    let rect = pixel_rect(area, layout, image.width(), image.height())?;
    encode_crop(&image, rect)
}

fn encode_crop(image: &DynamicImage, [x, y, width, height]: [u32; 4]) -> Result<Vec<u8>> {
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
    title: Option<String>,
    class: Option<String>,
    pid: Option<i32>,
}

/// None off Hyprland or without a focused window, a name pattern is no reason to fail a capture.
pub fn active_window_names() -> Option<Window> {
    let window: ActiveWindow = hyprctl("activewindow").ok()?;
    let process = window
        .pid
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok())
        .map(|name| name.trim().to_string())
        .or(window.class)?;
    Some(Window {
        title: window.title?,
        process,
    })
}

#[derive(Deserialize)]
struct Workspace {
    id: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitorWorkspaces {
    active_workspace: Workspace,
    special_workspace: Workspace,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Client {
    at: [i32; 2],
    size: [i32; 2],
    workspace: Workspace,
    mapped: bool,
    hidden: bool,
    floating: bool,
    #[serde(rename = "focusHistoryID")]
    focus_history_id: i32,
}

/// The windows on screen, topmost first.
fn visible_windows() -> Result<Vec<Area>> {
    let monitors: Vec<MonitorWorkspaces> = hyprctl("monitors")?;
    let visible: Vec<i32> = monitors
        .iter()
        .flat_map(|monitor| [monitor.active_workspace.id, monitor.special_workspace.id])
        .collect();
    Ok(stack_windows(hyprctl("clients")?, &visible))
}

/// shortcut: Hyprland does not report the stacking order, so this assumes floating windows are on
/// top and otherwise the most recently focused wins. Overlapping windows can pick the wrong one.
fn stack_windows(mut clients: Vec<Client>, workspaces: &[i32]) -> Vec<Area> {
    clients.retain(|client| {
        client.mapped && !client.hidden && workspaces.contains(&client.workspace.id)
    });
    clients.sort_by_key(|client| (!client.floating, client.focus_history_id));
    clients
        .iter()
        .map(|client| Area {
            x: f64::from(client.at[0]),
            y: f64::from(client.at[1]),
            width: f64::from(client.size[0]),
            height: f64::from(client.size[1]),
        })
        .collect()
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
    fn windows_are_stacked_and_filtered() {
        let client = |x, workspace, mapped, hidden, floating, focus| Client {
            at: [x, 0],
            size: [10, 10],
            workspace: Workspace { id: workspace },
            mapped,
            hidden,
            floating,
            focus_history_id: focus,
        };
        let clients = vec![
            client(1, 1, true, false, false, 3),
            client(2, 1, true, false, false, 0),
            client(3, 1, true, false, true, 5),
            client(4, 9, true, false, true, 1),
            client(5, 1, false, false, true, 1),
            client(6, 1, true, true, true, 1),
        ];
        let order: Vec<f64> = stack_windows(clients, &[1, 2])
            .iter()
            .map(|window| window.x)
            .collect();
        assert_eq!(order, [3.0, 2.0, 1.0]);
    }

    #[test]
    fn hyprland_client_json_parses() {
        let text = r#"[{"address":"0x1","mapped":true,"hidden":false,"at":[9,51],"size":[946,1020],
            "workspace":{"id":3,"name":"3"},"floating":false,"monitor":1,"class":"kitty",
            "focusHistoryID":7}]"#;
        let clients: Vec<Client> = serde_json::from_str(text).unwrap();
        assert_eq!(clients[0].focus_history_id, 7);
    }

    #[test]
    fn last_region_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("accioshot-last-{}", std::process::id()));
        let path = dir.join("nested/last-region.json");
        let last = LastRegion {
            rect: [10, 20, 30, 40],
            image: [100, 100],
        };
        last.save_to(&path).unwrap();
        assert_eq!(LastRegion::load_from(&path), Some(last));
        std::fs::write(&path, "damaged").unwrap();
        assert_eq!(LastRegion::load_from(&path), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn last_region_only_fits_the_same_screen() {
        let last = LastRegion {
            rect: [10, 20, 30, 40],
            image: [100, 100],
        };
        assert!(last.fits(100, 100));
        assert!(!last.fits(200, 100));
        let outside = LastRegion {
            rect: [90, 0, 30, 10],
            image: [100, 100],
        };
        assert!(!outside.fits(100, 100));
        let empty = LastRegion {
            rect: [0, 0, 0, 10],
            image: [100, 100],
        };
        assert!(!empty.fits(100, 100));
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
