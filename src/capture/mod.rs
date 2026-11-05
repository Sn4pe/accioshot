// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use image::{DynamicImage, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};

pub use platform::active_window_names;
#[cfg(target_os = "macos")]
pub use platform::layout;

use crate::config::RegionConfig;
use crate::{Result, region};

pub const CANCELLED: &str = "capture cancelled";

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    /// Select an area on a frozen capture of the screen
    Region,
    /// The last selected area, asks for one if there is none or the screen size changed
    LastRegion,
    /// Active window (Hyprland only on Linux)
    Window,
    /// Focused monitor (Hyprland only on Linux)
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
            let image = decode(&platform::screen().await?)?;
            match last.fits(image.width(), image.height()) {
                true => encode_crop(&image, last.rect),
                false => select_region(options).await,
            }
        }
        Mode::Screen => platform::screen().await,
        Mode::Window => platform::window().await,
        Mode::Monitor => platform::monitor().await,
    }
}

/// shortcut: the desktop's own picker does not tell us the area, so only our selector remembers it.
async fn select_region(options: &RegionConfig) -> Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    if !region::available() {
        return platform::picker().await;
    }
    // Before the capture, so they match what gets frozen. Off Hyprland on Linux there are none.
    let windows = match options.snap_windows {
        true => platform::visible_windows().unwrap_or_default(),
        false => Vec::new(),
    };
    let image = decode(&platform::screen().await?)?;
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
        // macOS has no state directory.
        let dir = dirs::state_dir().or_else(dirs::data_dir)?;
        Some(dir.join("accioshot/last-region.json"))
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

pub(crate) fn decode(png: &[u8]) -> Result<DynamicImage> {
    Ok(ImageReader::with_format(Cursor::new(png), ImageFormat::Png).decode()?)
}

pub(crate) fn encode_crop(
    image: &DynamicImage,
    [x, y, width, height]: [u32; 4],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    image
        .crop_imm(x, y, width, height)
        .write_to(&mut Cursor::new(&mut output), ImageFormat::Png)?;
    Ok(output)
}

/// Maps a logical area to pixels in the image of all monitors. The factor comes from the image
/// because with mixed scales the capture uses the largest one.
pub(crate) fn pixel_rect(
    area: Area,
    areas: &[Area],
    image_width: u32,
    image_height: u32,
) -> Result<[u32; 4]> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
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
        let layout = [
            area(0.0, 0.0, 1920.0, 1080.0),
            area(1920.0, 0.0, 1920.0, 1080.0),
        ];
        let rect = pixel_rect(layout[1], &layout, 3840, 1080).unwrap();
        assert_eq!(rect, [1920, 0, 1920, 1080]);
    }

    #[test]
    fn negative_origin_is_normalized() {
        let layout = [
            area(-1920.0, -100.0, 1920.0, 1080.0),
            area(0.0, 0.0, 1920.0, 1080.0),
        ];
        let rect = pixel_rect(area(10.0, 20.0, 100.0, 50.0), &layout, 3840, 1180).unwrap();
        assert_eq!(rect, [1930, 120, 100, 50]);
    }

    #[test]
    fn scaled_monitor_uses_image_factor() {
        let layout = [area(0.0, 0.0, 1920.0, 1080.0)];
        let rect = pixel_rect(area(100.0, 100.0, 200.0, 100.0), &layout, 3840, 2160).unwrap();
        assert_eq!(rect, [200, 200, 400, 200]);
    }

    #[test]
    fn window_past_the_edge_is_clamped() {
        let layout = [area(0.0, 0.0, 1920.0, 1080.0)];
        let rect = pixel_rect(area(1900.0, 1000.0, 100.0, 200.0), &layout, 1920, 1080).unwrap();
        assert_eq!(rect, [1900, 1000, 20, 80]);
    }

    #[test]
    fn area_outside_the_screen_fails() {
        let layout = [area(0.0, 0.0, 1920.0, 1080.0)];
        assert!(pixel_rect(area(3000.0, 0.0, 100.0, 100.0), &layout, 1920, 1080).is_err());
    }
}
