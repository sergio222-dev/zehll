# Zehll

A lightweight Wayland status bar for Hyprland, written in Rust with [iced](https://github.com/iced-rs/iced) and `wlr-layer-shell`.

Zehll is inspired by the `ii` (illogical-impulse) shell from [end-4/dots-hyprland](https://github.com/end-4/dots-hyprland), built on [Quickshell](https://github.com/quickshell-mirror/quickshell). The goal is a similar look with a much smaller memory footprint.

## Features

- Active window class and title, updated live from the Hyprland event socket.
- Clock with a hover tooltip: full date, system uptime and pending tasks from the `ii` todo list.
- System tray (StatusNotifierItem): icons, left/middle click activation and right-click context menus (DBusMenu) with submenus and toggles.
- One bar per monitor.
- Material 3 theme loaded from a matugen-generated file and reloaded live when it changes.

## Requirements

- Hyprland
- A Rust toolchain (edition 2024)
- [Material Symbols Rounded](https://github.com/google/material-design-icons) font for icons
- Optional: [matugen](https://github.com/InioX/matugen) for wallpaper-based colors

## Build and run

```sh
cargo run --release
```

Debug builds start noticeably slower because the GPU renderer is unoptimized.

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
