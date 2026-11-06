// SPDX-License-Identifier: GPL-3.0-or-later

//! One borderless window per monitor, with its own event loop so `select` can return.

use std::sync::Arc;

use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
use foreign_types::ForeignType;
use image::RgbaImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSCursor, NSEvent,
    NSEventMask, NSEventType, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize};

use super::{Gesture, Magnifier, View, dim, paint, render_screen};
use crate::Result;
use crate::capture::{self, Area};

/// NSScreenSaverWindowLevel: above the menu bar, the Dock and full screen apps.
const OVERLAY_LEVEL: isize = 1000;
const KEY_ESCAPE: u16 = 53;
/// kCGBitmapByteOrder32Little | kCGImageAlphaNoneSkipFirst: the buffers hold B, G, R, X.
const BITMAP_INFO: u32 = (2 << 12) | 6;
const RENDERING_INTENT_DEFAULT: u32 = 0;

define_class!(
    // A borderless window refuses the keyboard by default, and Esc must reach it.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "AccioshotOverlayWindow"]
    struct OverlayWindow;

    impl OverlayWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

/// The painting code works in one unit per buffer pixel, so everything here is multiplied by scale.
struct Screen {
    window: Retained<OverlayWindow>,
    scale: f64,
    area: Area,
    width: u32,
    height: u32,
    bright: Vec<u8>,
    dimmed: Vec<u8>,
    windows: Vec<Area>,
    layout: Vec<Area>,
    magnifier: Option<Magnifier>,
}

fn scaled(area: Area, scale: f64) -> Area {
    Area {
        x: area.x * scale,
        y: area.y * scale,
        width: area.width * scale,
        height: area.height * scale,
    }
}

/// Top left logical coordinates to AppKit's bottom left ones.
fn cocoa_rect(area: Area, primary_height: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(area.x, primary_height - area.y - area.height),
        NSSize::new(area.width, area.height),
    )
}

fn cursor_position(primary_height: f64) -> (f64, f64) {
    let point = NSEvent::mouseLocation();
    (point.x, primary_height - point.y)
}

/// Returns the selected area and the monitor layout it refers to, or None if cancelled. `windows`
/// can be clicked instead of dragging, topmost first.
pub fn select(
    image: &RgbaImage,
    windows: Vec<Area>,
    magnifier: Option<Magnifier>,
) -> Result<Option<(Area, Vec<Area>)>> {
    let mtm = MainThreadMarker::new().ok_or("the selector must run on the main thread")?;
    let layout = capture::layout()?;
    let primary_height = layout
        .iter()
        .find(|area| area.x == 0.0 && area.y == 0.0)
        .map_or(layout[0].height, |area| area.height);

    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, and a capture started from a terminal or the tray must still take the keyboard.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    NSCursor::crosshairCursor().set();

    let mut screens = Vec::new();
    for &area in &layout {
        screens.push(open_screen(
            mtm,
            area,
            primary_height,
            image,
            &windows,
            magnifier,
            &layout,
        ));
    }

    let mut gesture = Gesture {
        cursor: Some(cursor_position(primary_height)),
        ..Default::default()
    };
    for screen in &screens {
        show(screen, &gesture, image);
    }

    let result = loop {
        let mut wait = true;
        // Handles everything queued before painting, a frame per mouse event falls behind.
        while let Some(event) = next_event(&app, wait) {
            wait = false;
            handle(&app, &event, &mut gesture, &windows, primary_height);
        }
        if let Some(result) = gesture.result.take() {
            break result;
        }
        for screen in &screens {
            show(screen, &gesture, image);
        }
    };

    for screen in &screens {
        screen.window.orderOut(None);
    }
    flush();
    Ok(result.map(|area| (area, layout)))
}

fn open_screen(
    mtm: MainThreadMarker,
    area: Area,
    primary_height: f64,
    image: &RgbaImage,
    windows: &[Area],
    magnifier: Option<Magnifier>,
    layout: &[Area],
) -> Screen {
    let style = NSWindowStyleMask::Borderless;
    // SAFETY: the standard initializer of NSWindow, with its documented argument types.
    let window: Retained<OverlayWindow> = unsafe {
        msg_send![
            OverlayWindow::alloc(mtm),
            initWithContentRect: cocoa_rect(area, primary_height),
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // SAFETY: the window is kept alive by Retained, so AppKit must not release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setLevel(OVERLAY_LEVEL);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
    window.setAcceptsMouseMovedEvents(true);
    window.setHasShadow(false);
    window.setOpaque(true);
    if let Some(view) = window.contentView() {
        view.setWantsLayer(true);
    }
    window.makeKeyAndOrderFront(None);

    let scale = window.backingScaleFactor();
    let area = scaled(area, scale);
    let (width, height) = (area.width.round() as u32, area.height.round() as u32);
    let layout: Vec<Area> = layout.iter().map(|&a| scaled(a, scale)).collect();
    let bright = render_screen(image, &layout, area, width, height);
    Screen {
        window,
        scale,
        area,
        width,
        height,
        dimmed: dim(&bright),
        bright,
        windows: windows.iter().map(|&a| scaled(a, scale)).collect(),
        layout,
        magnifier: magnifier.map(|magnifier| Magnifier {
            cell: (f64::from(magnifier.cell) * scale).round() as u32,
            ..magnifier
        }),
    }
}

fn next_event(app: &NSApplication, wait: bool) -> Option<Retained<NSEvent>> {
    let date = wait.then(NSDate::distantFuture);
    // SAFETY: the mode is the framework's own constant, and this runs on the main thread.
    unsafe {
        app.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::Any,
            date.as_deref(),
            NSDefaultRunLoopMode,
            true,
        )
    }
}

fn handle(
    app: &NSApplication,
    event: &NSEvent,
    gesture: &mut Gesture,
    windows: &[Area],
    primary_height: f64,
) {
    let position = cursor_position(primary_height);
    match event.r#type() {
        NSEventType::MouseMoved | NSEventType::LeftMouseDragged => gesture.cursor = Some(position),
        NSEventType::LeftMouseDown => gesture.pressed(position),
        NSEventType::LeftMouseUp => gesture.released_at(position, windows),
        NSEventType::RightMouseDown => gesture.result = Some(None),
        NSEventType::KeyDown => {
            if event.keyCode() == KEY_ESCAPE {
                gesture.result = Some(None);
            }
            // Passing keys on would only beep.
            return;
        }
        _ => {}
    }
    app.sendEvent(event);
}

fn show(screen: &Screen, gesture: &Gesture, image: &RgbaImage) {
    let scale = |(x, y): (f64, f64)| (x * screen.scale, y * screen.scale);
    let gesture = Gesture {
        cursor: gesture.cursor.map(scale),
        drag_start: gesture.drag_start.map(scale),
        result: None,
    };
    let view = View {
        area: screen.area,
        width: screen.width,
        height: screen.height,
        bright: &screen.bright,
    };
    let mut canvas = screen.dimmed.clone();
    paint(
        &mut canvas,
        &view,
        &gesture,
        &screen.windows,
        screen.magnifier,
        image,
        &screen.layout,
    );
    let provider = CGDataProvider::from_buffer(Arc::new(canvas));
    let frame = CGImage::new(
        screen.width as usize,
        screen.height as usize,
        8,
        32,
        screen.width as usize * 4,
        &CGColorSpace::create_device_rgb(),
        BITMAP_INFO,
        &provider,
        false,
        RENDERING_INTENT_DEFAULT,
    );
    let Some(content) = screen.window.contentView() else {
        return;
    };
    // SAFETY: CALayer's contents accepts and retains a CGImage, and wantsLayer gives the view a layer.
    unsafe {
        let layer: *mut AnyObject = msg_send![&*content, layer];
        let contents = frame.as_ptr().cast::<AnyObject>();
        let _: () = msg_send![layer, setContents: contents];
    }
    flush();
}

/// Without a run of the system's own loop nothing else commits what was drawn.
fn flush() {
    if let Some(transaction) = objc2::runtime::AnyClass::get(c"CATransaction") {
        // SAFETY: a class method without arguments.
        let _: () = unsafe { msg_send![transaction, flush] };
    }
}
