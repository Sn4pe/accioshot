// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::Path;

use zbus::Variant;

use crate::Result;

const EXPIRE_DEFAULT: i32 = -1;

/// Shows a desktop notification, with the capture as its image if there is one on disk.
pub async fn send(summary: &str, body: &str, image: Option<&Path>) -> Result<()> {
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

/// Servers may render the body as markup, and URLs and errors come from remote servers.
fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped() {
        assert_eq!(escape_markup("a<b>&c"), "a&lt;b&gt;&amp;c");
    }
}
