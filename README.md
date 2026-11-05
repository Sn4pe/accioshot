# accioshot

Screenshot capture and upload tool for Linux. A Rust port of [ShareX](https://github.com/ShareX/ShareX),
starting with its core loop: capture, save, copy, upload.

## Usage

```sh
accioshot capture region                                # pick an area, save to <pictures>/accioshot
accioshot capture screen --save ~/shots                 # all monitors
accioshot capture window --copy                         # active window, image to the clipboard
accioshot capture monitor --upload imgur --copy         # focused monitor, URL to the clipboard
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
then to close. Blur and pixelate can leave hidden text partly recoverable, so crop secrets out
instead.

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
floating = true         # ask the compositor to float the window
width = 1280
height = 800

[editor.keys]           # only what you want to change, "" turns a key off
save = "ctrl+enter"
redo = "ctrl+shift+z, ctrl+y"
```

Actions in `[editor.keys]` are the tool names plus `undo`, `redo`, `delete`, `save` and `discard`.

To use another editor, set `command` to a program line with `{input}` and `{output}`. Without
`{output}` it must edit `{input}` in place. A failed exit is an error, and no output means discarded:

```toml
[editor]
command = "satty --filename {input} --output-filename {output} --early-exit"
```

## Tray

`accioshot daemon` adds a tray icon: click for a region capture, or use the menu for the other modes,
recent captures (click to copy the URL or path) and the captures folder. Key bindings keep working
without it.

## Config

`~/.config/accioshot/config.toml` sets what a capture without flags does. Any `--save`, `--copy`,
`--upload` or `--edit` flag replaces it. Action names are ShareX's:

```toml
save_dir = "~/Pictures/shots"
uploader = "imgur"
after_capture = ["AnnotateImage", "SaveImageToFile", "CopyImageToClipboard", "UploadImageToHost"]
after_upload = ["CopyURLToClipboard", "OpenURL"]
notifications = true
```

The defaults save to `accioshot/` in your pictures directory and show a notification. Captures are
listed in `~/.local/share/accioshot/history.jsonl`, private because it holds deletion URLs.

## Requirements

- `xdg-desktop-portal` with a backend for your desktop
- `region` freezes the screen and shows its own selector on compositors with layer-shell (Hyprland,
  Sway...): drag to select, `Esc` or right click to cancel. Elsewhere it uses the desktop's picker
- `window` and `monitor` only work on Hyprland for now
- `--copy` needs Wayland

## Uploaders

`--upload` takes ShareX custom uploaders (`.sxcu`): a path, or a name in
`~/.config/accioshot/uploaders/`. For example, `--upload imgur` reads
`~/.config/accioshot/uploaders/imgur.sxcu`.

These files often hold API keys, so only use ones you trust and keep them private.

Supported: `MultipartFormData` and `Binary` bodies, and the `{filename}`, `{input}`, `{base64}`,
`{random}`, `{response}`, `{responseurl}`, `{header}`, `{json}` and `{regex}` functions.

Not supported yet: `{xml}`, the dialog functions (`{inputbox}`, `{outputbox}`, `{select}`), ShareX
name patterns such as `%y`, and JSONPath filters and wildcards.

## Build

```sh
nix develop
cargo build --release
```

That builds `accioshot`. The editor is a separate binary so the CLI and tray do not load WebKit, and it
must sit next to `accioshot`:

```sh
(cd editor/ui && npm ci && npm run build)
cargo build --release --workspace
```

Or run it directly with `nix run github:Sn4pe/accioshot -- capture region`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Parts of `src/syntax.rs` and `src/sxcu.rs` are ported from ShareX, Copyright (c) 2007-2026 ShareX Team,
licensed under the GPL.
