// SPDX-License-Identifier: GPL-3.0-or-later

use crate::Result;

const CMD_KEY: u32 = 256;
const SHIFT_KEY: u32 = 512;
const OPTION_KEY: u32 = 2048;
const CONTROL_KEY: u32 = 4096;

/// Carbon's virtual key codes, which follow the physical ANSI layout and not the alphabet.
const KEYS: &[(&str, u32)] = &[
    ("a", 0x00),
    ("s", 0x01),
    ("d", 0x02),
    ("f", 0x03),
    ("h", 0x04),
    ("g", 0x05),
    ("z", 0x06),
    ("x", 0x07),
    ("c", 0x08),
    ("v", 0x09),
    ("b", 0x0B),
    ("q", 0x0C),
    ("w", 0x0D),
    ("e", 0x0E),
    ("r", 0x0F),
    ("y", 0x10),
    ("t", 0x11),
    ("1", 0x12),
    ("2", 0x13),
    ("3", 0x14),
    ("4", 0x15),
    ("6", 0x16),
    ("5", 0x17),
    ("9", 0x19),
    ("7", 0x1A),
    ("8", 0x1C),
    ("0", 0x1D),
    ("o", 0x1F),
    ("u", 0x20),
    ("i", 0x22),
    ("p", 0x23),
    ("l", 0x25),
    ("j", 0x26),
    ("k", 0x28),
    ("n", 0x2D),
    ("m", 0x2E),
    ("f1", 0x7A),
    ("f2", 0x78),
    ("f3", 0x63),
    ("f4", 0x76),
    ("f5", 0x60),
    ("f6", 0x61),
    ("f7", 0x62),
    ("f8", 0x64),
    ("f9", 0x65),
    ("f10", 0x6D),
    ("f11", 0x67),
    ("f12", 0x6F),
];

/// Reads `Shift+Cmd+4` into Carbon's key code and modifier mask. Compiled everywhere, so the settings
/// window checks the same config on any system.
pub fn parse(text: &str) -> Result<(u32, u32)> {
    let invalid = || format!("invalid region_hotkey in [tray]: {text}");
    let mut parts: Vec<String> = text.split('+').map(|p| p.trim().to_lowercase()).collect();
    let key = parts.pop().ok_or_else(invalid)?;
    let key = KEYS
        .iter()
        .find(|(name, _)| *name == key)
        .ok_or_else(invalid)?
        .1;
    let mut modifiers = 0;
    for part in &parts {
        modifiers |= match part.as_str() {
            "cmd" | "command" => CMD_KEY,
            "shift" => SHIFT_KEY,
            "alt" | "option" => OPTION_KEY,
            "ctrl" | "control" => CONTROL_KEY,
            _ => return Err(invalid().into()),
        };
    }
    Ok((key, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shortcut_keeps_its_codes() {
        assert_eq!(parse("Shift+Cmd+4").unwrap(), (21, CMD_KEY | SHIFT_KEY));
    }

    #[test]
    fn names_ignore_case_and_spaces() {
        assert_eq!(
            parse(" control + OPTION + f12 ").unwrap(),
            (0x6F, CONTROL_KEY | OPTION_KEY)
        );
    }

    #[test]
    fn unknown_names_fail() {
        assert!(parse("Hyper+4").is_err());
        assert!(parse("Cmd+Space").is_err());
        assert!(parse("Cmd+").is_err());
    }
}
