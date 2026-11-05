// SPDX-License-Identifier: GPL-3.0-or-later

//! Region selection over a frozen capture, like ShareX: whatever was on screen when the key was
//! pressed stays visible and in the result, even if the overlay makes panels or popups close.

use image::RgbaImage;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

use crate::Result;
use crate::capture::Area;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const DIM_PERCENT: u16 = 45;
/// Same as ShareX's MinimumSize: a smaller drag is a click that moved a little.
const MIN_DRAG: f64 = 5.0;

/// False on compositors without layer-shell, such as GNOME.
pub fn available() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, queue)) = registry_queue_init::<Overlay>(&connection) else {
        return false;
    };
    LayerShell::bind(&globals, &queue.handle()).is_ok()
}

/// Returns the selected area and the monitor layout it refers to, or None if cancelled. `windows`
/// can be clicked instead of dragging, topmost first.
pub fn select(image: &RgbaImage, windows: Vec<Area>) -> Result<Option<(Area, Vec<Area>)>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Overlay>(&connection)?;
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let pool = SlotPool::new(1024 * 1024, &shm)?;

    let mut overlay = Overlay {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        screens: Vec::new(),
        layout: Vec::new(),
        image: image.clone(),
        windows,
        keyboard: None,
        pointer: None,
        cursor: None,
        drag_start: None,
        result: None,
        failure: None,
    };

    // Outputs and their logical geometry arrive in separate rounds.
    queue.roundtrip(&mut overlay)?;
    queue.roundtrip(&mut overlay)?;

    for output in overlay.output_state.outputs() {
        let info = overlay
            .output_state
            .info(&output)
            .ok_or("monitor without information")?;
        let (Some((x, y)), Some((width, height))) = (info.logical_position, info.logical_size)
        else {
            return Err("monitor without logical geometry".into());
        };
        let area = Area {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
        };

        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("accioshot"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();

        overlay.layout.push(area);
        overlay.screens.push(Screen {
            layer,
            area,
            width: 0,
            height: 0,
            bright: Vec::new(),
            dimmed: Vec::new(),
            frame_pending: false,
            dirty: false,
        });
    }
    if overlay.screens.is_empty() {
        return Err("no monitors found".into());
    }

    loop {
        queue.blocking_dispatch(&mut overlay)?;
        if let Some(failure) = overlay.failure.take() {
            return Err(failure.into());
        }
        if let Some(result) = overlay.result.take() {
            return Ok(result.map(|area| (area, overlay.layout.clone())));
        }
    }
}

struct Screen {
    layer: LayerSurface,
    area: Area,
    width: u32,
    height: u32,
    bright: Vec<u8>,
    dimmed: Vec<u8>,
    frame_pending: bool,
    dirty: bool,
}

struct Overlay {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    screens: Vec<Screen>,
    layout: Vec<Area>,
    image: RgbaImage,
    windows: Vec<Area>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    /// Last pointer position in global logical coordinates.
    cursor: Option<(f64, f64)>,
    drag_start: Option<(f64, f64)>,
    /// Some(None) means cancelled.
    result: Option<Option<Area>>,
    failure: Option<String>,
}

impl Overlay {
    fn selection(&self) -> Option<Area> {
        Some(rect_between(self.drag_start?, self.cursor?))
    }

    fn mark_dirty(&mut self, qh: &QueueHandle<Self>) {
        for index in 0..self.screens.len() {
            self.screens[index].dirty = true;
            if !self.screens[index].frame_pending && self.screens[index].width > 0 {
                self.draw(index, qh);
            }
        }
    }

    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let selection = self.selection();
        let cursor = self.cursor;
        let hovered = cursor.and_then(|cursor| window_at(&self.windows, cursor));
        let screen = &mut self.screens[index];
        let (width, height) = (screen.width as i32, screen.height as i32);

        let (buffer, canvas) =
            match self
                .pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)
            {
                Ok(created) => created,
                Err(error) => {
                    self.failure = Some(format!("cannot create the overlay buffer: {error}"));
                    return;
                }
            };
        canvas.copy_from_slice(&screen.dimmed);
        match (selection, cursor) {
            (Some(selection), _) => paint_selection(canvas, &screen.bright, screen, selection),
            (None, Some(cursor)) => {
                if let Some(window) = hovered {
                    paint_selection(canvas, &screen.bright, screen, window);
                }
                paint_crosshair(canvas, screen, cursor);
            }
            (None, None) => {}
        }

        let surface = screen.layer.wl_surface();
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, FrameCallbackData(surface.clone()));
        if let Err(error) = buffer.attach_to(surface) {
            self.failure = Some(format!("cannot show the overlay: {error}"));
            return;
        }
        screen.layer.commit();
        screen.frame_pending = true;
        screen.dirty = false;
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
    let min_x = layout.iter().map(|a| a.x).fold(f64::INFINITY, f64::min);
    let min_y = layout.iter().map(|a| a.y).fold(f64::INFINITY, f64::min);
    let max_x = layout
        .iter()
        .map(|a| a.x + a.width)
        .fold(f64::MIN, f64::max);
    let factor = f64::from(image.width()) / (max_x - min_x);

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

fn paint_selection(canvas: &mut [u8], bright: &[u8], screen: &Screen, selection: Area) {
    let width = screen.width;
    let Some([left, top, right, bottom]) =
        visible_part(selection, screen.area, width, screen.height)
    else {
        return;
    };
    for y in top..bottom {
        let start = (y as usize * width as usize + left as usize) * 4;
        let end = (y as usize * width as usize + right as usize) * 4;
        canvas[start..end].copy_from_slice(&bright[start..end]);
    }
    for x in left..right {
        put(canvas, width, x, top, WHITE);
        put(canvas, width, x, bottom - 1, WHITE);
    }
    for y in top..bottom {
        put(canvas, width, left, y, WHITE);
        put(canvas, width, right - 1, y, WHITE);
    }
}

fn paint_crosshair(canvas: &mut [u8], screen: &Screen, cursor: (f64, f64)) {
    let x = cursor.0 - screen.area.x;
    let y = cursor.1 - screen.area.y;
    if x < 0.0 || y < 0.0 || x >= f64::from(screen.width) || y >= f64::from(screen.height) {
        return;
    }
    for column in 0..screen.width {
        put(canvas, screen.width, column, y as u32, WHITE);
    }
    for row in 0..screen.height {
        put(canvas, screen.width, x as u32, row, WHITE);
    }
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let found = self
            .screens
            .iter()
            .position(|screen| screen.layer.wl_surface() == surface);
        if let Some(index) = found {
            self.screens[index].frame_pending = false;
            if self.screens[index].dirty {
                self.draw(index, qh);
            }
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Overlay {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.result = Some(None);
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let found = self
            .screens
            .iter()
            .position(|screen| screen.layer.wl_surface() == layer.wl_surface());
        let Some(index) = found else {
            return;
        };
        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            self.failure = Some("the compositor gave the overlay no size".to_string());
            return;
        }
        let bright = render_screen(
            &self.image,
            &self.layout,
            self.screens[index].area,
            width,
            height,
        );
        let screen = &mut self.screens[index];
        screen.dimmed = dim(&bright);
        screen.bright = bright;
        screen.width = width;
        screen.height = height;
        self.draw(index, qh);
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Overlay {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.result = Some(None);
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for Overlay {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let found = self
                .screens
                .iter()
                .find(|screen| screen.layer.wl_surface() == &event.surface);
            let Some(screen) = found else {
                continue;
            };
            let position = (
                screen.area.x + event.position.0,
                screen.area.y + event.position.1,
            );
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.cursor = Some(position);
                }
                PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } => {
                    self.cursor = Some(position);
                    self.drag_start = Some(position);
                }
                PointerEventKind::Press {
                    button: BTN_RIGHT, ..
                } => {
                    self.result = Some(None);
                }
                PointerEventKind::Release {
                    button: BTN_LEFT, ..
                } => {
                    self.cursor = Some(position);
                    // A click with no window under it selects nothing, keep waiting.
                    match released(self.selection(), &self.windows, position) {
                        Some(area) => self.result = Some(Some(area)),
                        None => self.drag_start = None,
                    }
                }
                _ => {}
            }
        }
        self.mark_dirty(qh);
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(Overlay);

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(Overlay);

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
