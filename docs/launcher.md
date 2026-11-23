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

Type to search; the list is fuzzy matched. Up/Down, Tab/Shift+Tab, Ctrl+J/K and Ctrl+N/P move the selection, Ctrl+1–9 picks a visible row, Enter runs the selected row and Ctrl+Enter its secondary action. Escape closes the launcher; Shift+Escape clears the search, else goes back to Home. Clicking outside closes it too.

- **Home** lists apps from their desktop entries, the other pages and the session commands Lock, Suspend, Logout, Restart and Shutdown (the last three need a second Enter). Apps you launch often come first: each launch adds to the app's score, which halves every 14 days, kept in `~/.local/state/zehll/launcher.json`. Enter focuses an open window of the app or starts it (terminal apps in `$TERMINAL`, else kitty); Ctrl+Enter starts its first desktop action. A green diamond marks apps with an open window.
- **Files** lists recent folders from zoxide and searches your home folder with fd as you type; a search that is a path (`~/code`, `/etc`) lists that folder. The selected entry shows a preview with its type, size, path and item count. Enter opens it with `xdg-open`; Ctrl+Enter puts its path in the search to look inside.
- **Clipboard** lists the cliphist history with a preview of the selected entry (text or image), its type, MIME type and a hash of its content. Enter copies it back; Ctrl+Enter pins it to the top (pins are kept in `~/.local/state/zehll/clipboard.json`); the trash button deletes the entry; the pin button shows only pinned entries; the clear button wipes the whole history after a second press.
- **Keybinds** lists every Hyprland keybinding that has a description, searchable by keys or description.
- **Calculator** evaluates the search with qalc as you type: math, units and currencies. Enter copies the result and keeps it in a history of 25 (`~/.local/state/zehll/calculator.json`); Ctrl+Enter puts `(result)` back in the search; Tab walks the history and Enter on a past calculation reuses it.

The launcher uses the layer namespaces `zehll-launcher` and `zehll-launcher-catcher`. Setting `ZEHLL_CLIPHIST_DB` to a file makes the clipboard page use that cliphist database instead of the default one, which is useful to try deleting and wiping without touching your history.

## Requirements

- `qalc` (libqalculate) for the calculator
- `fd`, `zoxide`, `file` and `xdg-open` for files
- `cliphist` and `wl-copy` for the clipboard
