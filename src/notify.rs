// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use crate::Result;

#[cfg(target_os = "macos")]
pub async fn send(summary: &str, body: &str, _image: Option<&Path>) -> Result<()> {
    osascript(
        "display notification (item 2 of argv) with title \"accioshot\" subtitle (item 1 of argv)",
        &[summary.as_ref(), body.as_ref()],
    )
}

#[cfg(target_os = "macos")]
pub fn osascript(script: &str, args: &[&std::ffi::OsStr]) -> Result<()> {
    let output = std::process::Command::new("osascript")
        .env("LC_ALL", "en_US.UTF-8")
        .args(["-e", "on run argv", "-e", script, "-e", "end run"])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
const EXPIRE_DEFAULT: i32 = -1;

#[cfg(target_os = "linux")]
pub async fn send(summary: &str, body: &str, image: Option<&Path>) -> Result<()> {
    use std::collections::HashMap;
    use zbus::Variant;

    let connection = zbus::Connection::session().await?;
    let mut hints: HashMap<&str, Variant> = HashMap::new();
    if let Some(image) = image {
        hints.insert("image-path", Variant::from(image.display().to_string()));
    }
    let actions: &[&str] = &[];
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "accioshot",
                0u32,
                "",
                summary,
                escape_markup(body),
                actions,
                hints,
                EXPIRE_DEFAULT,
            ),
        )
        .await?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped() {
        assert_eq!(escape_markup("a<b>&c"), "a&lt;b&gt;&amp;c");
    }
}
