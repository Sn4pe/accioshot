// SPDX-License-Identifier: GPL-3.0-or-later

//! File name patterns, a subset of ShareX's NameParser. Copyright (c) 2007-2026 ShareX Team.
//!
//! Unlike ShareX this reads the pattern once from left to right, so a window title holding `%y`
//! stays text. The auto-increment tokens are left out, they need a persistent counter.

use std::fs::File;
use std::io::Read;

use chrono::{DateTime, Local};

const FALLBACK_NAME: &str = "accioshot";
/// Room for the extension and the suffix of a repeated name in the usual 255 byte limit.
const MAX_NAME_BYTES: usize = 200;
const ALPHANUMERIC: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
const HEXADECIMAL: &[u8] = b"0123456789abcdef";
const MAX_RANDOM_LENGTH: usize = 64;
/// Longer tokens first, `%y` must not win over `%yy`.
const TOKENS: &[&str] = &[
    "height", "width", "guid", "mon2", "unix", "mon", "cn", "mi", "mo", "ms", "pm", "pn", "ra",
    "rn", "rx", "un", "wy", "yy", "d", "h", "s", "t", "y",
];

#[derive(Clone, Debug, Default)]
pub struct Window {
    pub title: String,
    pub process: String,
}

pub struct Source<'a> {
    pub now: DateTime<Local>,
    pub width: u32,
    pub height: u32,
    pub window: Option<&'a Window>,
}

pub fn needs_window(pattern: &str) -> bool {
    pattern.contains("%t") || pattern.contains("%pn")
}

pub fn file_name(pattern: &str, source: &Source) -> String {
    let name = sanitize(&expand(pattern, source));
    let name = if name.is_empty() {
        FALLBACK_NAME
    } else {
        &name
    };
    format!("{name}.png")
}

/// Reads the size from the PNG header, the name does not justify decoding the image.
pub fn png_size(png: &[u8]) -> (u32, u32) {
    let number = |range: std::ops::Range<usize>| {
        png.get(range)
            .and_then(|bytes| bytes.try_into().ok())
            .map_or(0, u32::from_be_bytes)
    };
    (number(16..20), number(20..24))
}

fn expand(pattern: &str, source: &Source) -> String {
    let twelve_hours = pattern.contains("%pm");
    let mut output = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('%') {
        output.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        match TOKENS.iter().find(|token| rest.starts_with(**token)) {
            Some(token) => {
                rest = &rest[token.len()..];
                match *token {
                    "rn" | "ra" | "rx" => {
                        let (length, after) = random_length(rest);
                        rest = after;
                        output.push_str(&random_token(token, length));
                    }
                    _ => output.push_str(&value(token, source, twelve_hours)),
                }
            }
            None => output.push('%'),
        }
    }
    output.push_str(rest);
    output
}

fn value(token: &str, source: &Source, twelve_hours: bool) -> String {
    let date = |format: &str| source.now.format(format).to_string();
    let window = |read: fn(&Window) -> &str| {
        source
            .window
            .map_or_else(String::new, |window| window_text(read(window)))
    };
    let dimension = |value: u32| match value {
        0 => String::new(),
        value => value.to_string(),
    };
    match token {
        "y" => date("%Y"),
        "yy" => date("%y"),
        "mo" => date("%m"),
        // shortcut: English names only, ShareX's %mon is localized. Add chrono's locales if asked.
        "mon" | "mon2" => date("%B"),
        "d" => date("%d"),
        "h" if twelve_hours => date("%I"),
        "h" => date("%H"),
        "mi" => date("%M"),
        "s" => date("%S"),
        "ms" => date("%3f"),
        "pm" => date("%p"),
        "wy" => date("%V"),
        "unix" => source.now.timestamp().to_string(),
        "guid" => guid(),
        "width" => dimension(source.width),
        "height" => dimension(source.height),
        "un" => std::env::var("USER").unwrap_or_default(),
        "cn" => std::fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|name| name.trim().to_string())
            .unwrap_or_default(),
        "t" => window(|window| &window.title),
        "pn" => window(|window| &window.process),
        _ => unreachable!("every token in TOKENS has a value"),
    }
}

/// ShareX's `%ra{10}`: repeats a random token.
fn random_length(rest: &str) -> (usize, &str) {
    let parsed = rest.strip_prefix('{').and_then(|inner| {
        let (number, after) = inner.split_once('}')?;
        Some((number.parse::<usize>().ok()?, after))
    });
    match parsed {
        Some((length, after)) => (length.min(MAX_RANDOM_LENGTH), after),
        None => (1, rest),
    }
}

fn random_token(token: &str, length: usize) -> String {
    let alphabet = match token {
        "rn" => DIGITS,
        "ra" => ALPHANUMERIC,
        _ => HEXADECIMAL,
    };
    random_bytes(length)
        .iter()
        .map(|byte| char::from(alphabet[usize::from(*byte) % alphabet.len()]))
        .collect()
}

/// shortcut: modulo is slightly biased and fine for names, these are not secrets.
fn random_bytes(length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    let read = File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes));
    if read.is_err() {
        // Still a usable name, the save step never overwrites.
        bytes.fill(0);
    }
    bytes
}

fn guid() -> String {
    let mut bytes = random_bytes(16);
    bytes[6] = bytes[6] & 0x0F | 0x40;
    bytes[8] = bytes[8] & 0x3F | 0x80;
    let hex = |range: std::ops::Range<usize>| -> String {
        bytes[range]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    format!(
        "{}-{}-{}-{}-{}",
        hex(0..4),
        hex(4..6),
        hex(6..8),
        hex(8..10),
        hex(10..16)
    )
}

/// Same as ShareX's SanitizeInput, a title reads better without spaces.
fn window_text(text: &str) -> String {
    text.trim().replace(' ', "_")
}

/// The name reaches disks and uploaders, so nothing that means something in a path survives.
fn sanitize(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.');
    let mut end = clean.len().min(MAX_NAME_BYTES);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn source(window: Option<&Window>) -> Source<'_> {
        Source {
            now: Local.with_ymd_and_hms(2026, 3, 9, 15, 4, 5).unwrap(),
            width: 800,
            height: 600,
            window,
        }
    }

    fn name(pattern: &str) -> String {
        file_name(pattern, &source(None))
    }

    #[test]
    fn default_pattern_keeps_the_old_names() {
        assert_eq!(
            name(crate::config::DEFAULT_NAME_PATTERN),
            "accioshot_2026-03-09_15-04-05.png"
        );
    }

    #[test]
    fn date_and_image_tokens() {
        assert_eq!(name("%yy|%mon|%wy"), "26_March_11.png");
        assert_eq!(name("%h %pm"), "03 PM.png");
        assert_eq!(name("%widthx%height"), "800x600.png");
        assert_eq!(name("%ms"), "000.png");
    }

    #[test]
    fn unknown_tokens_stay_as_text() {
        assert_eq!(name("100%-%q%"), "100%-%q%.png");
    }

    #[test]
    fn random_tokens_repeat_and_are_capped() {
        let digits = name("%rn{8}");
        assert_eq!(digits.len(), "12345678.png".len());
        assert!(digits[..8].bytes().all(|byte| byte.is_ascii_digit()));
        assert_eq!(name("%ra").len(), "x.png".len());
        assert_eq!(name("%rx{9999}").len(), MAX_RANDOM_LENGTH + ".png".len());
    }

    #[test]
    fn guid_has_the_right_shape() {
        let value = guid();
        assert_eq!(value.len(), 36);
        assert_eq!(&value[14..15], "4");
    }

    #[test]
    fn window_title_cannot_escape_or_expand() {
        let window = Window {
            title: " ../../etc/passwd %y ".to_string(),
            process: "fire fox".to_string(),
        };
        let title = file_name("%t", &source(Some(&window)));
        assert_eq!(title, "_.._etc_passwd_%y.png");
        assert_eq!(file_name("%pn", &source(Some(&window))), "fire_fox.png");
    }

    #[test]
    fn missing_window_leaves_nothing() {
        assert_eq!(name("%t"), "accioshot.png");
        assert!(needs_window("a %t") && needs_window("%pn") && !needs_window("%y"));
    }

    #[test]
    fn hidden_and_path_names_are_cleaned() {
        assert_eq!(name("../x/y"), "_x_y.png");
        assert_eq!(name("."), "accioshot.png");
        assert_eq!(name("a\nb"), "a_b.png");
    }

    #[test]
    fn long_names_are_cut_on_a_character() {
        let long = "é".repeat(150);
        let cut = name(&long);
        assert!(cut.len() <= MAX_NAME_BYTES + ".png".len());
        assert!(cut.ends_with("é.png"));
    }

    #[test]
    fn png_size_reads_the_header() {
        let mut png = vec![0; 16];
        png.extend_from_slice(&800u32.to_be_bytes());
        png.extend_from_slice(&600u32.to_be_bytes());
        assert_eq!(png_size(&png), (800, 600));
        assert_eq!(png_size(b"short"), (0, 0));
    }
}
