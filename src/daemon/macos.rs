// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use core_foundation::date::CFAbsoluteTimeGetCurrent;
use core_foundation::runloop::{
    CFRunLoop, CFRunLoopTimer, CFRunLoopTimerRef, kCFRunLoopCommonModes,
};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{
    HISTORY_POLL, MENU_CLOSE_DELAY, MODES, RECENT_COUNT, entry_label, entry_text, history_modified,
    open, single_instance, spawn_capture,
};
use crate::config::Config;
use crate::history::{self, Entry};
use crate::{Result, clipboard};

// Carbon's only API for global shortcuts, and it needs no Accessibility permission.
const K_VK_ANSI_4: u32 = 21;
const CMD_KEY: u32 = 256;
const SHIFT_KEY: u32 = 512;
const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const EVENT_HOT_KEY_PRESSED: u32 = 5;
const SIGNATURE: u32 = u32::from_be_bytes(*b"acct");
const ICON_SIZE: u32 = 44;

const OPEN_FOLDER: &str = "open-folder";
const QUIT: &str = "quit";
const RECENT_PREFIX: &str = "recent:";

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

type Handler = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: Handler,
        types_count: u32,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(
        key: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        out: *mut *mut c_void,
    ) -> i32;
}

static EXE: OnceLock<PathBuf> = OnceLock::new();

struct State {
    tray: TrayIcon,
    pictures_dir: PathBuf,
    seen: Option<SystemTime>,
}

thread_local! {
    // Menus can only be touched from the main thread, which is where every callback here runs.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn run() -> Result<()> {
    let _lock = single_instance()?;
    let config = Config::load()?;
    // Asked now, an update can replace the binary while this runs.
    EXE.set(std::env::current_exe()?)
        .map_err(|_| "started twice")?;

    let mtm = MainThreadMarker::new().ok_or("the tray must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    register_shortcut()?;

    let recent = history::recent(RECENT_COUNT);
    let tray = TrayIconBuilder::new()
        .with_tooltip("accioshot")
        .with_icon(icon()?)
        .with_menu(Box::new(menu(&recent)?))
        .with_menu_on_left_click(false)
        .build()?;
    STATE.with_borrow_mut(|state| {
        *state = Some(State {
            tray,
            pictures_dir: config.pictures_dir().unwrap_or_default(),
            seen: history_modified(),
        })
    });

    MenuEvent::set_event_handler(Some(|event: MenuEvent| on_menu(event.id.as_ref())));
    TrayIconEvent::set_event_handler(Some(|event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            capture("region", Duration::ZERO);
        }
    }));

    // shortcut: polls the file because captures are separate processes, use FSEvents if a two
    // second delay in the menu ever matters.
    let timer = CFRunLoopTimer::new(
        // SAFETY: a plain clock read.
        unsafe { CFAbsoluteTimeGetCurrent() } + HISTORY_POLL.as_secs_f64(),
        HISTORY_POLL.as_secs_f64(),
        0,
        0,
        refresh_recent,
        ptr::null_mut(),
    );
    // SAFETY: the mode is the framework's own constant.
    CFRunLoop::get_current().add_timer(&timer, unsafe { kCFRunLoopCommonModes });

    app.run();
    Ok(())
}

fn icon() -> Result<Icon> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../../editor/icons/icon.png"),
        image::ImageFormat::Png,
    )?;
    let image = image
        .resize(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (width, height) = image.dimensions();
    Ok(Icon::from_rgba(image.into_raw(), width, height)?)
}

fn menu(recent: &[Entry]) -> Result<Menu> {
    let menu = Menu::new();
    for (label, mode) in MODES {
        menu.append(&MenuItem::with_id(mode, label, true, None))?;
    }
    menu.append(&PredefinedMenuItem::separator())?;
    let submenu = Submenu::new("Recent", !recent.is_empty());
    for entry in recent {
        // The text itself, a refresh can replace the list while the menu is open.
        let id = format!("{RECENT_PREFIX}{}", entry_text(entry));
        submenu.append(&MenuItem::with_id(id, entry_label(entry), true, None))?;
    }
    menu.append(&submenu)?;
    menu.append(&MenuItem::with_id(OPEN_FOLDER, "Open folder", true, None))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(QUIT, "Quit", true, None))?;
    Ok(menu)
}

fn on_menu(id: &str) {
    if let Some((_, mode)) = MODES.iter().find(|(_, mode)| *mode == id) {
        capture(mode, MENU_CLOSE_DELAY);
    } else if id == QUIT {
        std::process::exit(0);
    } else if id == OPEN_FOLDER {
        STATE.with_borrow(|state| {
            if let Some(state) = state {
                open(state.pictures_dir.as_os_str());
            }
        });
    } else if let Some(text) = id.strip_prefix(RECENT_PREFIX)
        && let Err(error) = clipboard::copy_text(text.to_string())
    {
        eprintln!("accioshot: {error}");
    }
}

fn capture(mode: &'static str, delay: Duration) {
    if let Some(exe) = EXE.get() {
        spawn_capture(exe.clone(), mode, delay);
    }
}

extern "C" fn refresh_recent(_: CFRunLoopTimerRef, _: *mut c_void) {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        let modified = history_modified();
        if modified == state.seen {
            return;
        }
        state.seen = modified;
        match menu(&history::recent(RECENT_COUNT)) {
            Ok(menu) => state.tray.set_menu(Some(Box::new(menu))),
            Err(error) => eprintln!("accioshot: cannot update the menu: {error}"),
        }
    });
}

fn register_shortcut() -> Result<()> {
    let pressed = EventTypeSpec {
        event_class: EVENT_CLASS_KEYBOARD,
        event_kind: EVENT_HOT_KEY_PRESSED,
    };
    let id = EventHotKeyId {
        signature: SIGNATURE,
        id: 1,
    };
    // SAFETY: the arguments outlive the calls, and the registration lasts until the process exits.
    unsafe {
        let target = GetApplicationEventTarget();
        let status = InstallEventHandler(
            target,
            on_shortcut,
            1,
            &pressed,
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if status != 0 {
            return Err(format!("cannot listen for the shortcut: error {status}").into());
        }
        let mut hot_key = ptr::null_mut();
        let status = RegisterEventHotKey(
            K_VK_ANSI_4,
            CMD_KEY | SHIFT_KEY,
            id,
            target,
            0,
            &mut hot_key,
        );
        if status != 0 {
            return Err(format!("cannot register Shift+Cmd+4: error {status}").into());
        }
    }
    Ok(())
}

extern "C" fn on_shortcut(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
    capture("region", Duration::ZERO);
    0
}
