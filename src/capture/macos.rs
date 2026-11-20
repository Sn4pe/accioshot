// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use accioshot::fs::PrivateDir;
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, ItemRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::display::CGDisplay;
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGRect;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowName,
    kCGWindowNumber, kCGWindowOwnerName,
};
use image::{ImageFormat, ImageReader, RgbaImage, imageops};

use super::Area;
use crate::Result;
use crate::names::Window;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub async fn screen() -> Result<Vec<u8>> {
    in_temporary_dir(screen_in)
}

pub async fn window() -> Result<Vec<u8>> {
    let (id, _) = frontmost_window()?;
    in_temporary_dir(|dir| {
        let file = dir.join("capture.png");
        run(&[format!("-l{id}").as_ref(), "-o".as_ref(), file.as_os_str()])?;
        Ok(std::fs::read(&file)?)
    })
}

pub async fn monitor() -> Result<Vec<u8>> {
    let index = screen_at(cursor()?, &screens()?).unwrap_or(0) + 1;
    in_temporary_dir(|dir| {
        let file = dir.join("capture.png");
        run(&[format!("-D{index}").as_ref(), file.as_os_str()])?;
        Ok(std::fs::read(&file)?)
    })
}

pub fn layout() -> Result<Vec<Area>> {
    Ok(screens()?
        .into_iter()
        .map(|[x, y, width, height]| Area::new(x, y, width, height))
        .collect())
}

pub fn visible_windows() -> Result<Vec<Area>> {
    Ok(normal_windows()?
        .iter()
        .filter_map(|window| {
            let bounds = value(window, unsafe { kCGWindowBounds })?;
            let bounds: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(bounds.as_CFTypeRef().cast()) };
            let field = |name: &'static str| {
                bounds
                    .find(CFString::from_static_string(name))?
                    .downcast::<CFNumber>()?
                    .to_f64()
            };
            Some(Area::new(
                field("X")?,
                field("Y")?,
                field("Width")?,
                field("Height")?,
            ))
        })
        .collect())
}

fn in_temporary_dir(capture: impl FnOnce(&Path) -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        unsafe { CGRequestScreenCaptureAccess() };
        return Err(
            "allow Screen Recording in System Settings > Privacy & Security for the app \
                    that launched accioshot, then restart that app"
                .into(),
        );
    }
    let dir = PrivateDir::new("accioshot-capture")?;
    capture(dir.path())
}

fn screen_in(dir: &Path) -> Result<Vec<u8>> {
    let screens = screens()?;
    let files: Vec<PathBuf> = (0..screens.len())
        .map(|index| dir.join(format!("{index}.png")))
        .collect();
    let args: Vec<&std::ffi::OsStr> = files.iter().map(|file| file.as_os_str()).collect();
    run(&args)?;
    if let [file] = files.as_slice() {
        return Ok(std::fs::read(file)?);
    }
    let parts = files
        .iter()
        .zip(screens)
        .map(|(file, bounds)| {
            let image =
                ImageReader::with_format(Cursor::new(std::fs::read(file)?), ImageFormat::Png)
                    .decode()?;
            Ok((bounds, image.to_rgba8()))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut output = Vec::new();
    stitch(&parts)?.write_to(&mut Cursor::new(&mut output), ImageFormat::Png)?;
    Ok(output)
}

fn run(args: &[&std::ffi::OsStr]) -> Result<()> {
    let output = Command::new("screencapture")
        .arg("-x")
        .args(args)
        .output()?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("screencapture failed: {}", error.trim()).into());
    }
    Ok(())
}

type Bounds = [f64; 4];

fn screens() -> Result<Vec<Bounds>> {
    let ids = CGDisplay::active_displays().map_err(|_| "cannot list the monitors")?;
    if ids.is_empty() {
        return Err("no monitors found".into());
    }
    Ok(ids
        .into_iter()
        .map(|id| bounds(CGDisplay::new(id).bounds()))
        .collect())
}

fn bounds(rect: CGRect) -> Bounds {
    [
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    ]
}

fn cursor() -> Result<(f64, f64)> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "cannot read the cursor position")?;
    let event = CGEvent::new(source).map_err(|_| "cannot read the cursor position")?;
    let point = event.location();
    Ok((point.x, point.y))
}

fn screen_at((x, y): (f64, f64), screens: &[Bounds]) -> Option<usize> {
    screens.iter().position(|&[left, top, width, height]| {
        x >= left && x < left + width && y >= top && y < top + height
    })
}

fn stitch(parts: &[(Bounds, RgbaImage)]) -> Result<RgbaImage> {
    let edge = |edge: fn(&Bounds) -> f64| parts.iter().map(move |(bounds, _)| edge(bounds));
    let left = edge(|b| b[0]).fold(f64::INFINITY, f64::min);
    let top = edge(|b| b[1]).fold(f64::INFINITY, f64::min);
    let right = edge(|b| b[0] + b[2]).fold(f64::NEG_INFINITY, f64::max);
    let bottom = edge(|b| b[1] + b[3]).fold(f64::NEG_INFINITY, f64::max);
    let factor = parts
        .iter()
        .map(|(bounds, image)| f64::from(image.width()) / bounds[2])
        .fold(0.0, f64::max);
    if parts.is_empty() || factor <= 0.0 {
        return Err("no monitors found".into());
    }

    let pixels = |points: f64| (points * factor).round() as u32;
    let mut canvas = RgbaImage::new(pixels(right - left), pixels(bottom - top));
    for ([x, y, width, height], image) in parts {
        let (x, y) = (i64::from(pixels(x - left)), i64::from(pixels(y - top)));
        let (width, height) = (pixels(*width), pixels(*height));
        if image.dimensions() == (width, height) {
            imageops::replace(&mut canvas, image, x, y);
        } else {
            let scaled = imageops::resize(image, width, height, imageops::FilterType::Lanczos3);
            imageops::replace(&mut canvas, &scaled, x, y);
        }
    }
    Ok(canvas)
}

fn normal_windows() -> Result<Vec<CFDictionary<CFString, CFType>>> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let windows: CFArray = copy_window_info(options, kCGNullWindowID).ok_or("no windows found")?;
    Ok(windows
        .get_all_values()
        .into_iter()
        .filter_map(|window| {
            let window: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(window.cast()) };
            let visible = number(&window, unsafe { kCGWindowLayer })? == 0.0
                && number(&window, unsafe { kCGWindowAlpha })? > 0.0;
            visible.then_some(window)
        })
        .collect())
}

fn frontmost_window() -> Result<(i64, Window)> {
    let windows = normal_windows()?;
    let window = windows.first().ok_or("no active window")?;
    let id = number(window, unsafe { kCGWindowNumber }).ok_or("the active window has no number")?;
    let names = Window {
        title: text(window, unsafe { kCGWindowName }).unwrap_or_default(),
        process: text(window, unsafe { kCGWindowOwnerName }).unwrap_or_default(),
    };
    Ok((id as i64, names))
}

fn value(window: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<ItemRef<'_, CFType>> {
    window.find(unsafe { CFString::wrap_under_get_rule(key) })
}

fn number(window: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<f64> {
    value(window, key)?.downcast::<CFNumber>()?.to_f64()
}

fn text(window: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<String> {
    Some(value(window, key)?.downcast::<CFString>()?.to_string())
}

pub fn active_window_names() -> Option<Window> {
    frontmost_window().ok().map(|(_, window)| window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
    const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);

    #[test]
    fn monitors_keep_their_layout_with_a_negative_origin() {
        let parts = [
            ([0.0, 0.0, 2.0, 2.0], RgbaImage::from_pixel(2, 2, RED)),
            ([-2.0, 1.0, 2.0, 1.0], RgbaImage::from_pixel(2, 1, BLUE)),
        ];
        let image = stitch(&parts).unwrap();
        assert_eq!(image.dimensions(), (4, 2));
        assert_eq!(*image.get_pixel(0, 1), BLUE);
        assert_eq!(*image.get_pixel(2, 0), RED);
        assert_eq!(*image.get_pixel(0, 0), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn sharpest_monitor_sets_the_scale() {
        let parts = [
            ([0.0, 0.0, 2.0, 2.0], RgbaImage::from_pixel(4, 4, RED)),
            ([2.0, 0.0, 2.0, 2.0], RgbaImage::from_pixel(2, 2, BLUE)),
        ];
        let image = stitch(&parts).unwrap();
        assert_eq!(image.dimensions(), (8, 4));
        assert_eq!(*image.get_pixel(7, 3), BLUE);
    }

    #[test]
    fn cursor_picks_its_monitor() {
        let screens = [[0.0, 0.0, 100.0, 100.0], [-50.0, 0.0, 50.0, 100.0]];
        assert_eq!(screen_at((-1.0, 10.0), &screens), Some(1));
        assert_eq!(screen_at((99.0, 99.0), &screens), Some(0));
        assert_eq!(screen_at((100.0, 0.0), &screens), None);
    }
}
