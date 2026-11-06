// SPDX-License-Identifier: GPL-3.0-or-later

//! Region selection over a frozen capture, like ShareX: whatever was on screen when the key was
//! pressed stays visible and in the result, even if the overlay makes panels or popups close.

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "wayland.rs"]
mod platform;

use image::RgbaImage;

#[cfg(target_os = "linux")]
pub use platform::available;
pub use platform::select;

use crate::capture::Area;

const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const BLACK: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];
const MAGNIFIER_GAP: u32 = 24;
const DIM_PERCENT: u16 = 45;
/// Same as ShareX's MinimumSize: a smaller drag is a click that moved a little.
const MIN_DRAG: f64 = 5.0;

/// Shows the real pixels around the cursor, also on scaled monitors where the overlay is soft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Magnifier {
    /// Pixels of the capture per side, odd so one of them is the center.
    pixels: u32,
    /// Overlay pixels per capture pixel.
    cell: u32,
}

impl Magnifier {
    /// Limits from ShareX's RegionCaptureOptions.
    pub fn new(size: u32, pixels: u32) -> Magnifier {
        let pixels = pixels.clamp(3, 35) | 1;
        Magnifier {
            pixels,
            cell: (size.clamp(50, 500) / pixels).max(3),
        }
    }

    fn side(self) -> u32 {
        self.pixels * self.cell
    }
}

/// What the pointer has done so far, in global logical coordinates.
#[derive(Default)]
struct Gesture {
    cursor: Option<(f64, f64)>,
    drag_start: Option<(f64, f64)>,
    /// Some(None) means cancelled.
    result: Option<Option<Area>>,
}

impl Gesture {
    fn selection(&self) -> Option<Area> {
        Some(rect_between(self.drag_start?, self.cursor?))
    }

    fn pressed(&mut self, position: (f64, f64)) {
        self.cursor = Some(position);
        self.drag_start = Some(position);
    }

    /// A click with no window under it selects nothing, keep waiting.
    fn released_at(&mut self, position: (f64, f64), windows: &[Area]) {
        self.cursor = Some(position);
        match released(self.selection(), windows, position) {
            Some(area) => self.result = Some(Some(area)),
            None => self.drag_start = None,
        }
    }
}

struct View<'a> {
    area: Area,
    width: u32,
    height: u32,
    bright: &'a [u8],
}

/// Paints the selection, crosshair and magnifier on a canvas that already holds the dimmed image.
fn paint(
    canvas: &mut [u8],
    view: &View,
    gesture: &Gesture,
    windows: &[Area],
    magnifier: Option<Magnifier>,
    image: &RgbaImage,
    layout: &[Area],
) {
    let cursor = gesture.cursor;
    match (gesture.selection(), cursor) {
        (Some(selection), _) => paint_selection(canvas, view, selection),
        (None, Some(cursor)) => {
            if let Some(window) = window_at(windows, cursor) {
                paint_selection(canvas, view, window);
            }
            paint_crosshair(canvas, view, cursor);
        }
        (None, None) => {}
    }
    if let (Some(magnifier), Some(cursor)) = (magnifier, cursor) {
        let view = (view.area, view.width, view.height);
        paint_magnifier(canvas, view, cursor, magnifier, image, layout);
    }
}

/// Copies the part of the frozen capture under a monitor, as ARGB8888.
///
/// shortcut: one buffer pixel per logical pixel, so the preview is soft on scaled monitors. The
/// result is cropped from the full capture and is not affected. Use wp_viewporter if it bothers.
fn render_screen(
    image: &RgbaImage,
    layout: &[Area],
    area: Area,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let (min_x, min_y, factor) = image_mapping(image, layout);

    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let source_y = ((area.y - min_y + f64::from(y)) * factor) as u32;
        let source_y = source_y.min(image.height() - 1);
        for x in 0..width {
            let source_x = ((area.x - min_x + f64::from(x)) * factor) as u32;
            let [r, g, b, _] = image.get_pixel(source_x.min(image.width() - 1), source_y).0;
            pixels.extend_from_slice(&[b, g, r, 0xFF]);
        }
    }
    pixels
}

/// The layout's top left corner and the capture pixels per logical pixel.
fn image_mapping(image: &RgbaImage, layout: &[Area]) -> (f64, f64, f64) {
    let min_x = layout.iter().map(|a| a.x).fold(f64::INFINITY, f64::min);
    let min_y = layout.iter().map(|a| a.y).fold(f64::INFINITY, f64::min);
    let max_x = layout
        .iter()
        .map(|a| a.x + a.width)
        .fold(f64::MIN, f64::max);
    (min_x, min_y, f64::from(image.width()) / (max_x - min_x))
}

fn dim(pixels: &[u8]) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let shade = |c: u8| (u16::from(c) * DIM_PERCENT / 100) as u8;
            [shade(p[0]), shade(p[1]), shade(p[2]), 0xFF]
        })
        .collect()
}

fn rect_between(a: (f64, f64), b: (f64, f64)) -> Area {
    Area {
        x: a.0.min(b.0),
        y: a.1.min(b.1),
        width: (a.0 - b.0).abs(),
        height: (a.1 - b.1).abs(),
    }
}

fn window_at(windows: &[Area], (x, y): (f64, f64)) -> Option<Area> {
    windows
        .iter()
        .find(|window| {
            x >= window.x
                && y >= window.y
                && x < window.x + window.width
                && y < window.y + window.height
        })
        .copied()
}

/// What releasing the button selects: the dragged area, or the window under a click.
fn released(selection: Option<Area>, windows: &[Area], cursor: (f64, f64)) -> Option<Area> {
    selection
        .filter(|area| area.width >= MIN_DRAG && area.height >= MIN_DRAG)
        .or_else(|| window_at(windows, cursor))
}

/// The selection in surface pixels, clamped to the monitor, as (left, top, right, bottom).
fn visible_part(selection: Area, screen: Area, width: u32, height: u32) -> Option<[u32; 4]> {
    let left = (selection.x - screen.x).floor().max(0.0);
    let top = (selection.y - screen.y).floor().max(0.0);
    let right = (selection.x + selection.width - screen.x)
        .ceil()
        .min(f64::from(width));
    let bottom = (selection.y + selection.height - screen.y)
        .ceil()
        .min(f64::from(height));
    (left < right && top < bottom).then_some([left as u32, top as u32, right as u32, bottom as u32])
}

fn put(canvas: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 4]) {
    let start = (y as usize * width as usize + x as usize) * 4;
    canvas[start..start + 4].copy_from_slice(&color);
}

fn paint_selection(canvas: &mut [u8], view: &View, selection: Area) {
    let width = view.width;
    let Some([left, top, right, bottom]) = visible_part(selection, view.area, width, view.height)
    else {
        return;
    };
    for y in top..bottom {
        let start = (y as usize * width as usize + left as usize) * 4;
        let end = (y as usize * width as usize + right as usize) * 4;
        canvas[start..end].copy_from_slice(&view.bright[start..end]);
    }
    outline(canvas, width, [left, top, right, bottom]);
}

fn paint_crosshair(canvas: &mut [u8], view: &View, cursor: (f64, f64)) {
    let x = cursor.0 - view.area.x;
    let y = cursor.1 - view.area.y;
    if x < 0.0 || y < 0.0 || x >= f64::from(view.width) || y >= f64::from(view.height) {
        return;
    }
    for column in 0..view.width {
        put(canvas, view.width, column, y as u32, WHITE);
    }
    for row in 0..view.height {
        put(canvas, view.width, x as u32, row, WHITE);
    }
}

/// Where the magnifier goes along one axis: past the cursor, or before it near the far edge.
fn magnifier_offset(cursor: u32, side: u32, screen: u32) -> u32 {
    if cursor + MAGNIFIER_GAP + side <= screen {
        cursor + MAGNIFIER_GAP
    } else {
        cursor.saturating_sub(MAGNIFIER_GAP + side)
    }
}

/// `view` is the monitor's area and its surface size.
fn paint_magnifier(
    canvas: &mut [u8],
    (area, width, height): (Area, u32, u32),
    cursor: (f64, f64),
    magnifier: Magnifier,
    image: &RgbaImage,
    layout: &[Area],
) {
    let x = cursor.0 - area.x;
    let y = cursor.1 - area.y;
    let side = magnifier.side();
    if x < 0.0 || y < 0.0 || x >= f64::from(width) || y >= f64::from(height) {
        return;
    }
    // Too small a monitor for it, the crosshair is still there.
    if side + 2 > width || side + 2 > height {
        return;
    }
    let left = magnifier_offset(x as u32, side + 2, width);
    let top = magnifier_offset(y as u32, side + 2, height);

    let (min_x, min_y, factor) = image_mapping(image, layout);
    let center_x = ((cursor.0 - min_x) * factor) as i64;
    let center_y = ((cursor.1 - min_y) * factor) as i64;
    let half = i64::from(magnifier.pixels / 2);
    let cell = magnifier.cell;

    for row in 0..side {
        let source_y = center_y + i64::from(row / cell) - half;
        for column in 0..side {
            let source_x = center_x + i64::from(column / cell) - half;
            let color = u32::try_from(source_x)
                .ok()
                .zip(u32::try_from(source_y).ok())
                .and_then(|(source_x, source_y)| image.get_pixel_checked(source_x, source_y))
                .map_or(BLACK, |pixel| {
                    let [r, g, b, _] = pixel.0;
                    [b, g, r, 0xFF]
                });
            put(canvas, width, left + 1 + column, top + 1 + row, color);
        }
    }
    outline(canvas, width, [left, top, left + side + 2, top + side + 2]);
    let center = magnifier.pixels / 2 * cell;
    let (center_left, center_top) = (left + 1 + center, top + 1 + center);
    outline(
        canvas,
        width,
        [
            center_left,
            center_top,
            center_left + cell,
            center_top + cell,
        ],
    );
}

fn outline(canvas: &mut [u8], width: u32, [left, top, right, bottom]: [u32; 4]) {
    for x in left..right {
        put(canvas, width, x, top, WHITE);
        put(canvas, width, x, bottom - 1, WHITE);
    }
    for y in top..bottom {
        put(canvas, width, left, y, WHITE);
        put(canvas, width, right - 1, y, WHITE);
    }
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
    fn dragging_in_any_direction_gives_the_same_rect() {
        let expected = area(10.0, 20.0, 30.0, 40.0);
        assert_eq!(rect_between((10.0, 20.0), (40.0, 60.0)), expected);
        assert_eq!(rect_between((40.0, 60.0), (10.0, 20.0)), expected);
        assert_eq!(rect_between((40.0, 20.0), (10.0, 60.0)), expected);
    }

    #[test]
    fn magnifier_limits_match_sharex() {
        assert_eq!(
            Magnifier::new(150, 15),
            Magnifier {
                pixels: 15,
                cell: 10
            }
        );
        // Even counts have no center pixel.
        assert_eq!(Magnifier::new(150, 16).pixels, 17);
        assert_eq!(
            Magnifier::new(1, 999),
            Magnifier {
                pixels: 35,
                cell: 3
            }
        );
    }

    #[test]
    fn magnifier_stays_on_screen() {
        assert_eq!(magnifier_offset(100, 152, 1920), 124);
        assert_eq!(magnifier_offset(1900, 152, 1920), 1724);
        assert_eq!(magnifier_offset(10, 152, 160), 0);
    }

    #[test]
    fn magnifier_shows_the_capture_pixel_under_the_cursor() {
        let mut image = RgbaImage::new(40, 40);
        image.put_pixel(5, 6, image::Rgba([10, 20, 30, 255]));
        let layout = [area(0.0, 0.0, 20.0, 20.0)];
        let magnifier = Magnifier::new(50, 3);
        let mut canvas = vec![0u8; 400 * 400 * 4];
        let view = (layout[0], 400, 400);
        paint_magnifier(&mut canvas, view, (2.5, 3.0), magnifier, &image, &layout);
        let cell = magnifier.cell as usize;
        // Top left of the box is 24 px past the cursor, inside the 1 px frame, center cell + 1.
        let (x, y) = (2 + 24 + 1 + cell + 1, 3 + 24 + 1 + cell + 1);
        let start = (y * 400 + x) * 4;
        assert_eq!(canvas[start..start + 4], [30, 20, 10, 0xFF]);
    }

    #[test]
    fn topmost_window_wins() {
        let windows = [area(0.0, 0.0, 100.0, 100.0), area(50.0, 50.0, 100.0, 100.0)];
        assert_eq!(window_at(&windows, (60.0, 60.0)), Some(windows[0]));
        assert_eq!(window_at(&windows, (120.0, 120.0)), Some(windows[1]));
        assert_eq!(window_at(&windows, (150.0, 150.0)), None);
    }

    #[test]
    fn drag_beats_the_window_and_a_small_drag_is_a_click() {
        let windows = [area(0.0, 0.0, 100.0, 100.0)];
        let drag = area(10.0, 10.0, 20.0, 20.0);
        assert_eq!(released(Some(drag), &windows, (30.0, 30.0)), Some(drag));
        let wobble = area(10.0, 10.0, 2.0, 1.0);
        assert_eq!(
            released(Some(wobble), &windows, (12.0, 11.0)),
            Some(windows[0])
        );
        assert_eq!(released(Some(wobble), &[], (12.0, 11.0)), None);
    }

    #[test]
    fn selection_across_monitors_is_clamped_to_each() {
        let selection = area(1900.0, 10.0, 40.0, 20.0);
        let left = area(0.0, 0.0, 1920.0, 1080.0);
        let right = area(1920.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            visible_part(selection, left, 1920, 1080),
            Some([1900, 10, 1920, 30])
        );
        assert_eq!(
            visible_part(selection, right, 1920, 1080),
            Some([0, 10, 20, 30])
        );
    }

    #[test]
    fn selection_on_another_monitor_is_not_visible() {
        let selection = area(2000.0, 10.0, 40.0, 20.0);
        let left = area(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(visible_part(selection, left, 1920, 1080), None);
    }

    #[test]
    fn screen_renders_its_part_of_the_capture() {
        let mut image = RgbaImage::new(4, 1);
        image.put_pixel(2, 0, image::Rgba([10, 20, 30, 255]));
        let layout = [area(0.0, 0.0, 2.0, 1.0), area(2.0, 0.0, 2.0, 1.0)];
        let pixels = render_screen(&image, &layout, layout[1], 2, 1);
        assert_eq!(pixels[..4], [30, 20, 10, 0xFF]);
    }

    #[test]
    fn dim_keeps_alpha_opaque() {
        assert_eq!(dim(&[100, 200, 0, 0xFF]), [45, 90, 0, 0xFF]);
    }
}
