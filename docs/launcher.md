# Launcher

[← Back to the README](../README.md)

Zehll registers the Hyprland global shortcut `zehll:launcherToggle`, which opens a launcher in the middle of the focused monitor:

```
bind = SUPER, SPACE, global, zehll:launcherToggle
```

or, with a Lua config:

```lua
hl.bind("SUPER + SPACE", hl.dsp.global("zehll:launcherToggle"))
```

### Open it with a SUPER tap

To open the launcher by tapping SUPER alone, like ii's search, while SUPER + any key keeps doing its own thing, send `zehll:launcherTap` from a release bind. Zehll acts on that shortcut only when it is released, so the config decides whether the tap counts: SUPER arms it, any other key or click while SUPER is held cancels it.

```lua
local armed = false
local function fire()
    if armed then hl.dispatch(hl.dsp.global("zehll:launcherTap")) end
    armed = false
end
for _, key in ipairs({ "SUPER_L", "SUPER_R" }) do
    hl.bind(key, function() armed = true end, { ignore_mods = true, transparent = true })
    hl.bind(key, fire, { ignore_mods = true, transparent = true, release = true })
end
-- Cancel on every other key and click held with SUPER (non-consuming, so their binds still run).
local keys = { "Tab", "Return", "space", "Escape", "mouse:272", "mouse:273", "mouse_up", "mouse_down" }
for c = string.byte("a"), string.byte("z") do table.insert(keys, string.char(c)) end
for i = 0, 9 do table.insert(keys, tostring(i)) end
for _, key in ipairs(keys) do
    hl.bind("SUPER + " .. key, function() armed = false end, { non_consuming = true, ignore_mods = true, transparent = true })
end
```

Add any other key you use with SUPER to the list; a key missing from it does not cancel the tap. `zehll:launcherToggle` from a release bind does nothing, because Hyprland sends it as released.

Type to search; the list is fuzzy matched. Up/Down, Tab/Shift+Tab, Ctrl+J/K and Ctrl+N/P move the selection, Ctrl+1–9 picks a visible row, Enter runs the selected row and Ctrl+Enter its secondary action. Escape closes the launcher; Shift+Escape clears the search, else goes back to Home. Clicking outside closes it too.

- **Home** lists apps from their desktop entries, the other pages and the session commands Lock, Suspend, Logout, Restart and Shutdown (the last three need a second Enter). Apps you launch often come first: each launch adds to the app's score, which halves every 14 days, kept in `~/.local/state/zehll/launcher.json`. Enter focuses an open window of the app or starts it (terminal apps in `$TERMINAL`, else kitty); Ctrl+Enter starts its first desktop action. A green diamond marks apps with an open window.
- **Files** lists recent folders from zoxide and searches your home folder with fd as you type; a search that is a path (`~/code`, `/etc`) lists that folder. The selected entry shows a preview with its type, size, path and item count. Enter opens it with `xdg-open`; Ctrl+Enter puts its path in the search to look inside.
- **Clipboard** lists the cliphist history with a preview of the selected entry (text or image), its type, MIME type and a hash of its content. Enter copies it back; Ctrl+Enter pins it to the top (pins are kept in `~/.local/state/zehll/clipboard.json`); the trash button deletes the entry; the pin button shows only pinned entries; the clear button wipes the whole history after a second press. The global shortcut `zehll:clipboard` opens the launcher straight on this page (and closes it when it is there); to put it on ii's Super+V with a Lua config (this also drops ii's fuzzel fallback):

  ```lua
  hl.unbind("SUPER + V")
  hl.bind("SUPER + V", hl.dsp.global("zehll:clipboard"), { description = "Utilities: Clipboard history" })
  ```

- **Keybinds** lists every Hyprland keybinding that has a description, searchable by keys or description.
- **Calculator** evaluates the search with qalc as you type: math, units and currencies. Enter copies the result and keeps it in a history of 25 (`~/.local/state/zehll/calculator.json`); Ctrl+Enter puts `(result)` back in the search; Tab walks the history and Enter on a past calculation reuses it.
- **Wallpapers** shows the wallpaper folder as a grid and applies the selected one with its accent color; see [Wallpapers](wallpapers.md).
- **Wallhaven** searches wallhaven.cc with its filters and applies or keeps a result; see [Wallpapers](wallpapers.md#the-wallhaven-page).
- **Emojis** searches every emoji and types or copies the one you pick; see [Emojis](emojis.md).

The launcher uses the layer namespaces `zehll-launcher` and `zehll-launcher-catcher`. Hyprland fades and slides layers in and out by default; to open and close the launcher instantly, turn that off for both:

```lua
hl.layer_rule({ match = { namespace = "^zehll-launcher$" }, no_anim = true })
hl.layer_rule({ match = { namespace = "^zehll-launcher-catcher$" }, no_anim = true })
```

Setting `ZEHLL_CLIPHIST_DB` to a file makes the clipboard page use that cliphist database instead of the default one, which is useful to try deleting and wiping without touching your history.

## Requirements

- `qalc` (libqalculate) for the calculator
- `fd`, `zoxide`, `file` and `xdg-open` for files
- `cliphist` and `wl-copy` for the clipboard
