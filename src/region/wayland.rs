// SPDX-License-Identifier: GPL-3.0-or-later

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

use super::{Gesture, Magnifier, View, dim, paint, render_screen};
use crate::Result;
use crate::capture::Area;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

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
pub fn select(
    image: &RgbaImage,
    windows: Vec<Area>,
    magnifier: Option<Magnifier>,
) -> Result<Option<(Area, Vec<Area>)>> {
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
        magnifier,
        keyboard: None,
        pointer: None,
        gesture: Gesture::default(),
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
        if let Some(result) = overlay.gesture.result.take() {
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
    magnifier: Option<Magnifier>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    gesture: Gesture,
    failure: Option<String>,
}

impl Overlay {
    fn mark_dirty(&mut self, qh: &QueueHandle<Self>) {
        for index in 0..self.screens.len() {
            self.screens[index].dirty = true;
            if !self.screens[index].frame_pending && self.screens[index].width > 0 {
                self.draw(index, qh);
            }
        }
    }

    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
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
        let view = View {
            area: screen.area,
            width: screen.width,
            height: screen.height,
            bright: &screen.bright,
        };
        paint(
            canvas,
            &view,
            &self.gesture,
            &self.windows,
            self.magnifier,
            &self.image,
            &self.layout,
        );

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
        self.gesture.result = Some(None);
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
            self.gesture.result = Some(None);
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
                    self.gesture.cursor = Some(position);
                }
                PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } => self.gesture.pressed(position),
                PointerEventKind::Press {
                    button: BTN_RIGHT, ..
                } => {
                    self.gesture.result = Some(None);
                }
                PointerEventKind::Release {
                    button: BTN_LEFT, ..
                } => self.gesture.released_at(position, &self.windows),
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
