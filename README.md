# accioshot

Screenshot capture and upload tool for Linux and macOS: capture, annotate, save, copy, upload.

## Usage

```sh
accioshot capture region                                # pick an area, save to <pictures>/accioshot
accioshot capture last-region                           # the same area again, without asking
accioshot capture screen --save ~/shots                 # all monitors
accioshot capture window --copy                         # active window, image to the clipboard
accioshot capture monitor --upload imgur --copy         # focused monitor, URL to the clipboard
accioshot capture screen --delay 3                      # wait 3 seconds first, for menus and tooltips
```

stdout only holds the URL (if uploaded) or the saved path, so it works in scripts:

```sh
url=$(accioshot capture region --upload imgur) && notify-send "$url"
```

Bind it to a key from your compositor. For Hyprland:

```
bind = SUPER SHIFT, S, exec, accioshot capture region --upload imgur --copy
```

If an upload fails and the capture was not being saved, it is kept in the default directory.

## Editor

`--edit`, or `AnnotateImage` in `after_capture`, opens the capture in an editor before the other
actions run. Discarding the edit cancels them.

The built-in editor has select, rectangle, ellipse, line, arrow, freehand, text, numbered steps,
highlight, blur, pixelate and crop, with undo. Press the tool's key (`v r e l a f t s h b p c`), hold
`Shift` for squares and 45 degree lines, `Ctrl+S` to save, `Esc` to cancel what is in progress and
then to close. `Ctrl+C` or the Copy button copies the image and closes: nothing is saved or uploaded,
whatever the other actions say. Blur and pixelate can leave hidden text partly recoverable, so crop
secrets out instead.

If the editor fails, or the image never loads and you close it, the original capture is saved to the
default directory.

Everything is set under `[editor]`. These are the defaults:

```toml
[editor]
command = "builtin"     # or another program, see below
tool = "Arrow"
colors = ["#e5484d", "#f5a524", "#3fa66b", "#3b82c4", "#f4f1ea", "#1b1916"]
highlight_color = "#ffe066"
stroke_width = 4
font_family = "sans-serif"
font_size = 28
step_size = 32
blur_strength = 12
pixel_size = 14
remember_last = true    # reopen with the last tool, colors and sizes
initial_zoom = "fit"    # or a scale, 1 is 100 %
zoom_step = 1.25        # factor per zoom step
max_zoom = 16
floating = true         # ask the compositor to float the window
width = 1280
height = 800

[editor.keys]           # only what you want to change, "" turns a key off
save = "ctrl+enter"
redo = "ctrl+shift+z, ctrl+y"
```

Actions in `[editor.keys]` are the tool names plus `undo`, `redo`, `delete`, `save`, `copy`,
`discard`, `zoom_in`, `zoom_out`, `zoom_fit`, `zoom_actual`, `pan`, `move_left`, `move_right`,
`move_up`, `move_down`, `duplicate` (Ctrl+D), `select_all` (Ctrl+A), `bring_forward` (Page Up),
`send_backward` (Page Down), `bring_to_front` (Home) and `send_to_back` (End).

Ctrl and the wheel zoom around the pointer, the wheel scrolls and Shift scrolls sideways. The middle
button drags the view, and so does the left one while `pan` (Space) is held. The move keys nudge the
selection by 1 pixel, 10 with Shift, and a run of them is one undo step.

With Select, drag over empty space to select every annotation fully inside the box, and Shift-click
to add or drop one. Moving, the move keys, Delete, duplicating, layers and the color apply to the
whole selection, the size only to annotations of the first one's kind. Steps keep the number they
were placed with when they change layer.

To use another editor, set `command` to a program line with `{input}` and `{output}`. No output means
discarded and a failed exit is an error. Without `{output}` it must edit `{input}` in place, and then
it cannot discard: closing it runs the other actions on the file as it is.

```toml
[editor]
command = "satty --filename {input} --output-filename {output} --early-exit"
```

## History

The tray's History opens every capture as a sheet of thumbnails, grouped by day. Search looks in names,
links and dates. Select one to copy the image, path or URL, or open it. `Delete upload` opens the
host's deletion page in your browser and nothing is sent by itself. `Remove` takes it out of the history,
and it asks first whether to delete the file too. `Edit` reopens it in the built-in editor: saving
replaces the file, copying leaves it as it was.

Ctrl or Cmd+click adds to the selection, Shift+click selects a range and Ctrl or Cmd+A selects every
capture shown, then `Remove` or Delete acts on all of them. `Clean up N missing` selects the entries whose
file is gone. Drag captures to another app to drop the files there. Space shows a quick preview and a double click opens the viewer.
Both zoom like the editor: Ctrl or Cmd and the wheel around the pointer, Ctrl or Cmd `+`, `-` and `0`,
a click switches between fitting and 100 %, and dragging or the wheel pans. The arrows go to the
previous or next capture and every action and shortcut keeps working. Thumbnails are kept in `~/.cache/accioshot/thumbs`, or
`~/Library/Caches/accioshot/thumbs` on macOS. These are the defaults:

```toml
[history]
thumbnail_width = 250
thumbnail_height = 150
max_items = 500
filter_missing_files = false
```

## Settings

The tray's Settings edits `config.toml` and keeps your comments and every key it does not show, such as
the editor's colors and keys. Uploaders appear by name, and their files are never read. New captures
use a saved change at once. Restart the tray for its folder and, on macOS, its shortcut.

The editor, history and settings windows follow the theme:

```toml
[ui]
theme = "system"   # or Catppuccin's "latte", "frappe", "macchiato", "mocha"
```

`system` means Latte on a light desktop and Mocha on a dark one. On Linux it looks at the background
color GTK gives its windows, so themes that only recolor GTK, such as Stylix's, count too. A window
keeps its theme until you reopen it.

If the file is managed elsewhere and read-only, such as by Home Manager, saving reports the error and
changes nothing.

## macOS app

`contrib/install-macos.sh` builds `Accioshot.app`, installs it in `/Applications` and starts it now and
at every login. Run it again to update. It needs Rust and Node.

It lives in the menu bar and works like the Linux tray below. It also makes `Shift+Cmd+4` take a region
capture, so turn off whatever else uses that key, such as the system shortcut in System Settings >
Keyboard > Keyboard Shortcuts > Screenshots. Or pick another key, restart the app, and leave it empty
for none:

```toml
[tray]
region_hotkey = "Ctrl+Option+4"   # Shift, Cmd, Ctrl, Option with a letter, a digit or F1 to F12
```

Allow Accioshot in Screen Recording the first time it asks. The first install creates a local signing
certificate in your login keychain, so updates keep the permission. Run it from a terminal in your
session: signing may ask for the keychain password, so choose Always Allow. Errors go to `~/Library/Logs/accioshot.log`. Quit it from the menu until the next
login, or stop it with:

```sh
launchctl bootout gui/$(id -u)/com.sn4pe.accioshot
```

## Tray

`accioshot daemon` adds a tray icon: click for a region capture, or use the menu for the other modes,
recent captures (click to copy the URL or path) and the captures folder. Key bindings keep working
without it.

On Linux, to start it with your session, copy [`contrib/accioshot.service`](contrib/accioshot.service) to
`~/.config/systemd/user/`, adjust `ExecStart` if needed, and run
`systemctl --user enable --now accioshot`. The Nix package installs it with the right path. Your
compositor must reach `graphical-session.target`, which Hyprland only does with its systemd
integration.

## Config

`~/.config/accioshot/config.toml` sets what a capture without flags does. Any `--save`, `--copy`,
`--upload` or `--edit` flag replaces it. Action names are ShareX's:

```toml
save_dir = "~/Pictures/shots"
uploader = "imgur"
after_capture = ["AnnotateImage", "SaveImageToFile", "CopyImageToClipboard", "UploadImageToHost"]
after_upload = ["CopyURLToClipboard", "OpenURL"]
notifications = true
delay = 0               # seconds, `--delay` replaces only this
name_pattern = "accioshot_%y-%mo-%d_%h-%mi-%s"
```

`name_pattern` takes ShareX's tokens: `%y %yy %mo %mon %mon2 %d %h %mi %s %ms %pm %unix %wy` for the
date, `%rn %ra %rx` for random characters (`%ra{10}` repeats it), `%guid`, `%width %height`, `%un %cn`
for user and computer, and `%t %pn` for the active window's title and process, on Hyprland and macOS.
Month names are in English and the `%i` counters are not supported. The name also goes to the
uploader, so `%t` can publish a window title.

The region selector is set under `[region]`:

```toml
[region]
snap_windows = true     # clicking selects the window under the cursor, on Hyprland and macOS
magnifier = true        # zoom on the real pixels around the cursor
magnifier_size = 150    # overlay pixels per side, 50 to 500
magnifier_pixels = 15   # capture pixels per side, 3 to 35, odd
```

The defaults save to `accioshot/` in your pictures directory and show a notification. Captures are
listed in `~/.local/share/accioshot/history.jsonl` (`~/Library/Application Support/accioshot/` on
macOS), private because it holds deletion URLs.

## Requirements

- `xdg-desktop-portal` with a backend for your desktop
- `region` freezes the screen and shows its own selector on compositors with layer-shell (Hyprland,
  Sway...): drag to select, click a window on Hyprland, `Esc` or right click to cancel. Elsewhere it
  uses the desktop's picker, and `last-region` cannot remember what you picked
- `window` and `monitor` only work on Hyprland for now
- `--copy` needs Wayland on Linux

On macOS there is nothing else to install, it uses `screencapture`, `osascript` and `pbcopy`:

- The app that launches accioshot needs Screen Recording in System Settings > Privacy & Security
- `region` shows the same selector, and clicking selects a window
- `window` is the topmost window and `monitor` the one under the cursor
- `%t` and `%pn` work in `name_pattern`
- Notifications come from Script Editor, which needs to be allowed in System Settings
- Setups with several monitors are untested

## Uploaders

`--upload` takes ShareX custom uploaders (`.sxcu`): a path, or a name in
`uploaders/` next to the config. For example, `--upload imgur` reads
`~/.config/accioshot/uploaders/imgur.sxcu`.

These files often hold API keys, so only use ones you trust and keep them private.

Supported: `MultipartFormData` and `Binary` bodies, and the `{filename}`, `{input}`, `{base64}`,
`{random}`, `{response}`, `{responseurl}`, `{header}`, `{json}` and `{regex}` functions.

Not supported yet: `{xml}`, the dialog functions (`{inputbox}`, `{outputbox}`, `{select}`), ShareX
name patterns such as `%y`, and JSONPath filters and wildcards.

## Build

```sh
nix develop     # Linux, macOS only needs Rust and Node
cargo build --release
```

That builds `accioshot`. The windows are a separate binary, `accioshot-ui`, so the CLI and tray do not
load WebKit. It must sit next to `accioshot`:

```sh
(cd ui/web && npm ci && npm run build)
cargo build --release --workspace
```

Or run it directly with `nix run github:Sn4pe/accioshot -- capture region`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Parts of `src/syntax.rs`, `src/sxcu.rs` and `src/names.rs` are ported from ShareX, Copyright (c) 2007-2026 ShareX Team,
licensed under the GPL.
