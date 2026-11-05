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

Or run it directly with `nix run github:Sn4pe/accioshot -- capture region`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Parts of `src/syntax.rs` and `src/sxcu.rs` are ported from ShareX, Copyright (c) 2007-2026 ShareX Team,
licensed under the GPL.
