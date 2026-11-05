// SPDX-License-Identifier: GPL-3.0-or-later

//! Captures run as `accioshot capture`, so a crash in one cannot take the shortcut down.

use std::ffi::c_void;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::OnceLock;

use crate::Result;

// Carbon's only API for global shortcuts, and it needs no Accessibility permission.
const K_VK_ANSI_4: u32 = 21;
const CMD_KEY: u32 = 256;
const SHIFT_KEY: u32 = 512;
const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const EVENT_HOT_KEY_PRESSED: u32 = 5;
const SIGNATURE: u32 = u32::from_be_bytes(*b"acct");

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
    fn RunApplicationEventLoop();
}

static EXE: OnceLock<PathBuf> = OnceLock::new();

pub fn run() -> Result<()> {
    let _lock = single_instance()?;
    // Asked now, an update can replace the binary while this runs.
    EXE.set(std::env::current_exe()?)
        .map_err(|_| "started twice")?;

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
        RunApplicationEventLoop();
    }
    Ok(())
}

/// Held until the process exits, the kernel releases it even after a crash.
fn single_instance() -> Result<File> {
    let file = File::create(std::env::temp_dir().join("accioshot.lock"))?;
    file.try_lock()
        .map_err(|_| "accioshot daemon is already running")?;
    Ok(file)
}

extern "C" fn on_shortcut(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
    std::thread::spawn(|| {
        let Some(exe) = EXE.get() else { return };
        let result = Command::new(exe)
            .args(["capture", "region"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status();
        if let Err(error) = result {
            eprintln!("accioshot: cannot start a capture: {error}");
        }
    });
    0
}
