// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::Command;

use ashpd::desktop::ResponseError;
use ashpd::desktop::screenshot::Screenshot;
use reqwest::Url;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{Area, CANCELLED, decode, encode_crop, pixel_rect};
use crate::Result;
use crate::names::Window;

pub async fn screen() -> Result<Vec<u8>> {
    portal(false).await
}

/// The desktop's own selector, for compositors without layer-shell.
pub async fn picker() -> Result<Vec<u8>> {
    portal(true).await
}

// shortcut: crops the full capture, so anything on top of the window shows up.
// Upgrade to hyprland_toplevel_export_v1 if that matters.
pub async fn window() -> Result<Vec<u8>> {
    let (area, layout) = (active_window()?, monitor_areas(&monitors()?));
    crop(&portal(false).await?, area, &layout)
}

pub async fn monitor() -> Result<Vec<u8>> {
    let monitors = monitors()?;
    let area = monitors
        .iter()
        .find(|monitor| monitor.focused)
        .ok_or("no focused monitor")?
        .area();
    crop(&portal(false).await?, area, &monitor_areas(&monitors))
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

fn crop(png: &[u8], area: Area, layout: &[Area]) -> Result<Vec<u8>> {
    let image = decode(png)?;
    let rect = pixel_rect(area, layout, image.width(), image.height())?;
    encode_crop(&image, rect)
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
pub fn visible_windows() -> Result<Vec<Area>> {
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
    fn rotated_monitor_swaps_sides() {
        let rotated = monitor(0, 0, 1920, 1080, 1.0, 1);
        assert_eq!(rotated.area(), area(0.0, 0.0, 1080.0, 1920.0));
    }
}
