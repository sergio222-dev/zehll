# Screen snip

[← Back to the README](../README.md)

Zehll registers the Hyprland global shortcut `zehll:screenshot`, which freezes every monitor with `grim` and opens a zone selector over them, like ii's region selector:

```
bind = SUPER SHIFT, S, global, zehll:screenshot
```

or, with a Lua config:

```lua
hl.bind("SUPER + SHIFT + S", hl.dsp.global("zehll:screenshot"))
```

The screen dims and every window of the active workspace, and every top-level layer that is not a bar, gets an outline; the one under the pointer lights up with its class, and a click selects it. Dragging selects any zone instead, with its size in pixels. Releasing the left button copies the selection as a PNG with `wl-copy`; the right button opens it in swappy. Escape, the close button or the shortcut again cancel. A selection stays on one monitor.

SAVE in the toolbar also writes each copied snip to `$(xdg-user-dir PICTURES)/Screenshots/screenshot-YYYY-MM-DD_HH.MM.SS.png`; it is remembered in `~/.local/state/zehll/state.json`. The selector uses the layer namespace `zehll-snip`; a `noanim` layer rule keeps it from fading in over the frozen frame.

## Requirements

- `grim` and `wl-copy`
- [swappy](https://github.com/jtheoof/swappy) to edit
- `xdg-user-dir` to find the Pictures folder
