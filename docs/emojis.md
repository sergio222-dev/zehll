# Emojis

[← Back to the README](../README.md)

The launcher's **Emojis** page searches every Unicode emoji and types the one you pick into the window you were using. Open it from Home, or with the global shortcut `zehll:emojis`, which opens the launcher straight on this page (and closes it when it is there). ii puts its emoji picker on Super+.; to hand that combo to Zehll with a Lua config:

```lua
hl.unbind("SUPER + Period")
hl.bind("SUPER + Period", hl.dsp.global("zehll:emojis"), { description = "Utilities: Emoji picker" })
```

`hl.unbind` also drops ii's fuzzel fallback, so the combo does nothing while Zehll is not running.

With an empty search the grid shows **MOST USED** (the six emojis you picked most, with their counts) and then every Unicode group under its header, six emojis per row. Typing drops the headers and lists the matches, best first, fuzzy matched over each emoji's name and GitHub shortcodes, then (weaker) its keywords from [emojilib](https://github.com/muan/emojilib), the same list ii searches, so `thunder` finds ⚡, and last its group; equal matches go to the one you pick more. The footer shows the selected emoji's name and shortcode.

| Key | Action |
|---|---|
| Arrows | Move: Up and Down by rows, across headers |
| Enter, or a click | Close the launcher, type the emoji into the window that had the focus and copy it |
| Ctrl+Enter | Copy it and keep the launcher open |
| Alt+Left / Alt+Right | Pick the skin tone |
| Shift+Escape | Back to Home |

A small + marks emojis that come in skin tones. The tone you pick applies to all of them, on the tiles and when typed. Emoji 18.0's 🫹 and 🫺 stay yellow: Noto Color Emoji has no toned glyphs for them yet.

The use counts and the tone are kept in `~/.local/state/zehll/emojis.json`. The emoji list is built into Zehll (the `emojis` crate), so nothing is read at run time. Emojis are drawn with the Noto Color Emoji font.

## Requirements

- Noto Color Emoji
- `wtype` to type the emoji and `wl-copy` to copy it
