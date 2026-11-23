# Zehll

A lightweight Wayland status bar for Hyprland, written in Rust with [iced](https://github.com/iced-rs/iced) and `wlr-layer-shell`.

Zehll is inspired by the `ii` (illogical-impulse) shell from [end-4/dots-hyprland](https://github.com/end-4/dots-hyprland), built on [Quickshell](https://github.com/quickshell-mirror/quickshell). The goal is a similar look with a much smaller memory footprint.

## Features

- Active window class and title, updated live from the Hyprland event socket.
- Clock with a hover tooltip: full date, system uptime and pending tasks from the `ii` todo list.
- System tray (StatusNotifierItem): icons, left/middle click activation and right-click context menus (DBusMenu) with submenus and toggles.
- Side panel with uptime, reload and power actions, quick settings, notifications, a month calendar and a to-do list shared with ii (`~/.local/state/quickshell/user/todo.json`) and a pomodoro timer (25/5 min, a 15 min break every 4 cycles) that notifies when each phase ends.
- Google Calendar meetings: the next meeting beside the clock, a calendar popover with each day's agenda, and alerts when a meeting is about to start.
- Media player (MPRIS): a play/pause button left of the date opens a player popover, with cover art, seek bar, previous/next, shuffle and repeat.
- App launcher: fuzzy search over apps, session commands, files, clipboard history, Hyprland keybindings and a calculator.
- Screen snip in the style of ii's region selector: freeze the screen, pick a window or drag a zone, copy, save or edit it.
- One bar per monitor.
- Material 3 theme loaded from a matugen-generated file and reloaded live when it changes.

## Requirements

- Hyprland
- A Rust toolchain (edition 2024)
- [Material Symbols Rounded](https://github.com/google/material-design-icons) font for icons
- Optional: [matugen](https://github.com/InioX/matugen) for wallpaper-based colors
- Optional, for cover art that players link over HTTP (such as Spotify): `curl`
- Optional, for the audio spectrum behind the player popover: [cava](https://github.com/karlstav/cava)
- Optional, for meetings: `python3`, a Google OAuth client in `~/.config/calendarz/credentials.json`, `pw-play` (PipeWire) for the alert sound and `xdg-open`
- Optional, for the launcher pages: `qalc` (libqalculate) for the calculator, `fd`, `zoxide`, `file` and `xdg-open` for files, `cliphist` and `wl-copy` for the clipboard
- Optional, for the screen snip: `grim` and `wl-copy`, plus [swappy](https://github.com/jtheoof/swappy) to edit and `xdg-user-dir` to find the Pictures folder

## Build and run

```sh
cargo run --release
```

Debug builds start noticeably slower because the GPU renderer is unoptimized.

## Side panel

Hover the right edge of the bar to open the side panel. Click outside it or press Escape to close it. Zehll registers the Hyprland global shortcut `zehll:panelToggle`, so a keybind can toggle it:

```
bind = SUPER, N, global, zehll:panelToggle
```

To slide the panel in from the right, add Hyprland layer rules for its namespaces:

```
layerrule = animation slide right, ^zehll-panel$
layerrule = noanim, ^zehll-panel-catcher$
```

The power button runs [wlogout](https://github.com/ArtsyMacaw/wlogout).

Quick settings toggle Wi-Fi, Bluetooth, audio output and microphone mute, night light and keep awake:

- Clicking a tile's icon toggles it; clicking the rest of the tile opens its sub-panel: Wi-Fi networks (connect, with a password prompt for new secured ones), Bluetooth devices (connect, pair, disconnect, forget, always connect) or the audio mixer (per-app volume sliders and output device). Details opens KDE's network or Bluetooth settings, or pavucontrol.
- The night light tile opens eye protection: night light intensity and schedule, an anti-flashbang screen shader that dims bright screens, monitor brightness over DDC/CI ([ddcutil](https://www.ddcutil.com/)) and gamma.
- Night light switches [hyprsunset](https://github.com/hyprwm/hyprsunset) between 5000 K and 6000 K. It turns on automatically from 19:00 to 06:30; a manual toggle holds until the next change of the schedule.
- Keep awake holds a logind idle inhibitor, which hypridle honors. It is remembered across restarts in `~/.local/state/zehll/state.json`.

## Notifications

Zehll is a notification daemon (`org.freedesktop.Notifications`); new notifications pop up at the top right (up to three, 5 s by default, critical ones until dismissed) and are listed in the side panel, with do not disturb and clear all. Notifications show their image or app icon and their action buttons; clicking one runs its default action (or hides the popup when it has none), and ✕ dismisses it. If another daemon such as swaync owns the name, Zehll waits in the D-Bus queue and takes over when that daemon exits, so disable the other daemon to use Zehll's.

## Meetings

Zehll reads your Google Calendar with `scripts/gcal.py`, a read-only client that uses only the Python standard library. The script is embedded in the binary and run with `python3`; it reads the OAuth client from `~/.config/calendarz/credentials.json` and keeps its refresh token in `~/.config/calendarz/token.json`, so an existing [calendarz](https://github.com/sergio222-dev/calendarZ) login is reused. To sign in the first time:

```sh
python3 scripts/gcal.py auth
```

Meetings are fetched at startup, every 2 minutes and when the calendar shows another month. All-day, cancelled and declined events are skipped.

- **Pill**: beside the clock, the start time of the next meeting in the next 24 hours, with a dot that is grey more than 15 minutes before it, green from 15 to 5 minutes and yellow in the last 5. Hover it for the title and `10:00 – 10:45 · IN 4 MIN`. When the token expires it reads `SIGN IN` in red; click it to sign in again in the browser.
- **Calendar**: click the date to open the month with ISO week numbers and a dot on days with meetings, and the agenda of the selected day. Past meetings read `DONE`; the next meeting in the next 24 hours is highlighted with `JOIN` (or `OPEN` when it has no Meet link), which opens it in the browser. Click outside it or press Escape to close it.
- **Alerts**: 5 minutes before a meeting, a normal notification says `Starts in 5 minutes`. When it starts, an alert card opens under the clock with `JOIN` (or `OPEN`) and `DISMISS`; it stays until you use one of them and rings every 5 seconds while any card is open.

To try the alert card without a real meeting, trigger the `zehll:meetingTest` global shortcut, for example with a keybind:

```
bind = SUPER SHIFT, M, global, zehll:meetingTest
```

The calendar and the alert cards use the layer namespaces `zehll-calendar`, `zehll-calendar-catcher` and `zehll-meeting-alerts` for layer rules.

## Media player

The button left of the date shows pause while the active player plays and play otherwise; it is hidden when no MPRIS player runs. When playback starts, flames burst out of its edges; when it stops, they burn out as red embers. The flames are a wgpu shader (`src/flame.wgsl`) and are not drawn with iced's software renderer. Like ii, the active player is the one playing, preferring the one already shown; `playerctld` is ignored because it mirrors the others. Hover it for the track and the player name; click it to open the player popover, and click outside it or press Escape to close it. Shuffle and repeat are disabled for players that do not support them. Live streams have no length to seek in: they show a wave that moves while playing and `LIVE`, without seeking or elapsed time. While the popover is open and the player is playing, `cava` draws the audio spectrum behind it, like ii.

The popover uses the layer namespaces `zehll-media` and `zehll-media-catcher` for layer rules.

## Launcher

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

## Screen snip

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

## Theme

The bar uses a tactical HUD style: a fixed navy base with slanted panels and glowing highlights. Its accent colors come from a Material 3 palette in matugen's `colors.json` format, read from `~/.config/zehll/theme.json` (or `$XDG_CONFIG_HOME/zehll/theme.json`). Missing keys fall back to orange and cyan.

| Role | Material 3 token |
|---|---|
| Accent (window tag, clock underline, highlights) | `primary` |
| Text on the accent | `on_primary` |
| Hover glow | `tertiary` |

The Barlow and Barlow Condensed fonts are embedded in the binary (SIL Open Font License, see `assets/fonts/OFL.txt`).

To regenerate it with the wallpaper, add a template to `~/.config/matugen/config.toml` that reuses a `colors.json` template:

```toml
[templates.zehll]
input_path = '~/.config/matugen/templates/colors.json'
output_path = '~/.config/zehll/theme.json'
```

## System tray

Zehll registers as a StatusNotifierHost. It also queues for the `org.kde.StatusNotifierWatcher` name: if another shell already provides the watcher, Zehll uses it, and D-Bus hands the name over to Zehll when that shell exits.

## Built with

- [iced](https://github.com/iced-rs/iced): GUI toolkit
- [iced_layershell / layershellev](https://github.com/waycrate/exwlshelleventloop): layer-shell integration for iced
- [system-tray](https://github.com/jakestanger/system-tray): StatusNotifierItem and DBusMenu client
- [freedesktop-icons](https://github.com/oknozor/freedesktop-icons): icon theme lookup
- [tokio](https://github.com/tokio-rs/tokio), [chrono](https://github.com/chronotope/chrono), [serde_json](https://github.com/serde-rs/json)

## How it was made

Zehll was developed with [Claude Code](https://claude.com/claude-code), using [gentle-ai](https://github.com/Gentleman-Programming/gentle-ai) as the agent harness.
