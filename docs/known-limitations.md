# Known limitations

Shortcuts taken on purpose, and when to revisit them.

## Capture

- Active window on Hyprland crops the full capture, so anything on top of the window shows up. Upgrade to `hyprland_toplevel_export_v1` if that matters. (`src/capture/linux.rs`, `window`)
- Hyprland does not report the stacking order, so clickable windows assume floating ones are on top and otherwise the most recently focused wins. Overlapping windows can pick the wrong one. (`src/capture/linux.rs`, `stack_windows`)
- Only accioshot's own selector remembers the last region. The desktop's picker does not report the area it captured. (`src/capture/mod.rs`, `select_region`)
- On macOS the screen capture assumes the files and `-D` follow the active display list. Untested with two monitors. (`src/capture/macos.rs`, `screen_in`)
- On macOS the active window is the topmost normal window, so a floating window of another app can win over the focused one. (`src/capture/macos.rs`, `frontmost_window`)

## Region selector

- The overlay uses one buffer pixel per logical pixel, so the preview is soft on scaled monitors. The result is cropped from the full capture and is not affected. Use `wp_viewporter` if it bothers. (`src/region/mod.rs`, `render_screen`)

## File names

- Month names are English only, while ShareX's `%mon` is localized. Add chrono's locales if asked. (`src/names.rs`, `value`)
- Random tokens use modulo, which is slightly biased. Fine for names, these are not secrets. (`src/names.rs`, `random_bytes`)
- The auto-increment tokens are left out, they need a persistent counter. (`src/names.rs`)

## Uploaders

- Response paths support `a.b[0]['c']`, not JSONPath filters or wildcards. Add them if a real uploader needs them. (`src/syntax.rs`, `select`)

## Editor

- Editor command templates split on whitespace, with no quoting. The paths are replaced after splitting, so they can have spaces. Use a real parser if an editor needs quoted arguments. (`src/editor.rs`, `plan`)

## Tray

- The tray polls the history file because captures run as separate processes. Use inotify (Linux) or FSEvents (macOS) if a two second delay in the menu ever matters. (`src/daemon/linux.rs`, `src/daemon/macos.rs`)
