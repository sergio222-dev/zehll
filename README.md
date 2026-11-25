# Zehll

A lightweight Wayland status bar for Hyprland, written in Rust with [iced](https://github.com/iced-rs/iced) and `wlr-layer-shell`.

Zehll is inspired by the `ii` (illogical-impulse) shell from [end-4/dots-hyprland](https://github.com/end-4/dots-hyprland), built on [Quickshell](https://github.com/quickshell-mirror/quickshell). The goal is a similar look with a much smaller memory footprint.

## Features

- Active window class and title, updated live from the Hyprland event socket.
- Clock with a hover tooltip: full date, system uptime and pending tasks from the `ii` todo list.
- [System tray](docs/system-tray.md) (StatusNotifierItem): icons, left/middle click activation and right-click context menus (DBusMenu) with submenus and toggles.
- [Side panel](docs/side-panel.md) with uptime, reload and power actions, quick settings, [notifications](docs/notifications.md), a month calendar and a to-do list shared with ii (`~/.local/state/quickshell/user/todo.json`) and a pomodoro timer (25/5 min, a 15 min break every 4 cycles) that notifies when each phase ends.
- [Google Calendar meetings](docs/meetings.md): the next meeting beside the clock, a calendar popover with each day's agenda, and alerts when a meeting is about to start.
- [Agent indicator](docs/agents.md): one logo per live Claude Code or Copilot CLI session right of the meetings pill, colored by what the session is doing.
- [Media player](docs/media-player.md) (MPRIS): a play/pause button left of the date opens a player popover, with cover art, seek bar, previous/next, shuffle and repeat.
- [App launcher](docs/launcher.md): fuzzy search over apps, session commands, files, clipboard history, Hyprland keybindings and a calculator.
- [Emoji picker](docs/emojis.md): a launcher page that searches emojis and types the one you pick, with most used and skin tones.
- [Screen snip](docs/screen-snip.md) in the style of ii's region selector: freeze the screen, pick a window or drag a zone, copy, save or edit it.
- [Wallpapers](docs/wallpapers.md) with ii's workspace parallax, launcher pages to pick them with their accent color and to search Wallhaven, and colors from matugen.
- [Volume indicator](docs/volume-osd.md): shows the volume for a moment when it changes from the keyboard, a headset or an app.
- One bar per monitor.
- [Material 3 theme](docs/theme.md) loaded from a matugen-generated file and reloaded live when it changes.

## Documentation

| Page | Covers |
|---|---|
| [Side panel](docs/side-panel.md) | Opening it, quick settings and their sub-panels, night light, keep awake |
| [Notifications](docs/notifications.md) | The notification daemon, popups and the list in the side panel |
| [Meetings](docs/meetings.md) | Google Calendar sign-in, the pill, the calendar popover and the alerts |
| [Agent indicator](docs/agents.md) | The ai_alert hooks it reads, the colors and the tooltip |
| [Media player](docs/media-player.md) | The media button, its popover and live streams |
| [Launcher](docs/launcher.md) | The shortcut, keys and every page |
| [Emojis](docs/emojis.md) | The emoji page, its shortcut, keys and skin tones |
| [Screen snip](docs/screen-snip.md) | The shortcut, picking and dragging zones, copy, save and edit |
| [Wallpapers](docs/wallpapers.md) | The parallax, the Wallpapers and Wallhaven pages and the accent color |
| [Volume indicator](docs/volume-osd.md) | When it shows and hides, boost and mute |
| [Theme](docs/theme.md) | Colors from matugen, the embedded fonts and MonoLisa |
| [System tray](docs/system-tray.md) | How Zehll shares the tray watcher with other shells |

## Requirements

- Hyprland
- A Rust toolchain (edition 2024)
- [Material Symbols Rounded](https://github.com/google/material-design-icons) font for icons

Some features use optional tools; each page lists its own:

| Feature | Optional tools |
|---|---|
| [Side panel](docs/side-panel.md#requirements) | wlogout, hyprsunset, ddcutil |
| [Meetings](docs/meetings.md#requirements) | python3, a Google OAuth client, pw-play, xdg-open |
| [Agent indicator](docs/agents.md#requirements) | ai_alert's hooks |
| [Media player](docs/media-player.md#requirements) | curl, cava |
| [Launcher](docs/launcher.md#requirements) | qalc, fd, zoxide, file, xdg-open, cliphist, wl-copy |
| [Emojis](docs/emojis.md#requirements) | Noto Color Emoji, wtype, wl-copy |
| [Screen snip](docs/screen-snip.md#requirements) | grim, wl-copy, swappy, xdg-user-dir |
| [Wallpapers](docs/wallpapers.md#requirements) | matugen, xdg-user-dir, curl |
| [Volume indicator](docs/volume-osd.md#requirements) | pactl, wpctl |
| [Theme](docs/theme.md#requirements) | matugen, MonoLisa Code |

## Build and run

```sh
cargo run --release
```

Debug builds start noticeably slower because the GPU renderer is unoptimized.

## Built with

- [iced](https://github.com/iced-rs/iced): GUI toolkit
- [iced_layershell / layershellev](https://github.com/waycrate/exwlshelleventloop): layer-shell integration for iced
- [system-tray](https://github.com/jakestanger/system-tray): StatusNotifierItem and DBusMenu client
- [freedesktop-icons](https://github.com/oknozor/freedesktop-icons): icon theme lookup
- [emojilib](https://github.com/muan/emojilib): emoji search keywords (MIT, see [`assets/emoji/emojilib-LICENSE.txt`](assets/emoji/emojilib-LICENSE.txt))
- [tokio](https://github.com/tokio-rs/tokio), [chrono](https://github.com/chronotope/chrono), [serde_json](https://github.com/serde-rs/json)

## How it was made

Zehll was developed with [Claude Code](https://claude.com/claude-code), using [gentle-ai](https://github.com/Gentleman-Programming/gentle-ai) as the agent harness.
