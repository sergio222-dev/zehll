# Zehll

A lightweight Wayland status bar for Hyprland, written in Rust with [iced](https://github.com/iced-rs/iced) and `wlr-layer-shell`.

Zehll is inspired by the `ii` (illogical-impulse) shell from [end-4/dots-hyprland](https://github.com/end-4/dots-hyprland), built on [Quickshell](https://github.com/quickshell-mirror/quickshell). The goal is a similar look with a much smaller memory footprint.

## Features

- Active window class and title, updated live from the Hyprland event socket.
- Clock with a hover tooltip: full date, system uptime and pending tasks from the `ii` todo list.
- System tray (StatusNotifierItem): icons, left/middle click activation and right-click context menus (DBusMenu) with submenus and toggles.
- Side panel with uptime, reload and power actions, quick settings, notifications, a month calendar and a to-do list shared with ii (`~/.local/state/quickshell/user/todo.json`) and a pomodoro timer (25/5 min, a 15 min break every 4 cycles) that notifies when each phase ends.
- Google Calendar meetings: the next meeting beside the clock, a calendar popover with each day's agenda, and alerts when a meeting is about to start.
- One bar per monitor.
- Material 3 theme loaded from a matugen-generated file and reloaded live when it changes.

## Requirements

- Hyprland
- A Rust toolchain (edition 2024)
- [Material Symbols Rounded](https://github.com/google/material-design-icons) font for icons
- Optional: [matugen](https://github.com/InioX/matugen) for wallpaper-based colors
- Optional, for meetings: `python3`, a Google OAuth client in `~/.config/calendarz/credentials.json`, `pw-play` (PipeWire) for the alert sound and `xdg-open`

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
