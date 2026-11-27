# Migrate to iced_exwlshell

Objective: replace iced_layershell 0.19.1 with iced_exwlshell 0.21.4, its renamed successor, to pick up the present-error recovery that froze DP-1's bar. Branch: `chore/iced-exwlshell` (from `main`). Supersedes #59/#60 (vendoring 0.19.1 with the fix backported).
Test runner: `cargo test`.

## Specs

- S1 "cambie de wallpaper desde wallhaven y me desaparecio el indicador de titulo en la barra" / "veo que se congelo la barra del monitor DP-1": DP-1's bar kept its last frame (clock at 13:52, empty title) while HDMI-A-2's bar kept updating.
- S2 "podemos migrar a iced_exwlshell?": migrate instead of vendoring or forking.
- Assumption (A1): the freeze is waycrate/exwlshelleventloop#360. iced_layershell 0.19.1 only `tracing`-logs `SurfaceError::Outdated`/`Lost` (Zehll installs no subscriber) and never reconfigures the swapchain, so every later present of that surface fails. This is inferred from the symptoms, not seen in a log: the freeze could not be reproduced. The fix (#401) shipped in 0.20+, where the crate is iced_exwlshell; iced_layershell stopped at 0.19.1 on crates.io.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1, S2, A1 | inline | done | `chore: migrate to iced_exwlshell 0.21` |

## Log

- L1 (2026-11-27) Diagnosis on the frozen instance (up 12 h): the main thread sat in `epoll_wait`; HDMI-A-2's bar drew the current time; hovering DP-1's bar did not redraw it; Hyprland's event sockets were connected; no leftover catcher; no NVRM/Xid. The clock froze before the Wallhaven wallpapers were applied (14:15-14:17), so the wallpaper change is not the trigger. A 4 s DPMS cycle did not reproduce it.
- L2 T1 done, following upstream's MIGRATION-19.1->20.0 and 20.0->21.0 guides. `daemon` takes `.layer_settings(...)` and an `iced::Settings`. Sizes go through `layer_size`, where 0 still means "compositor picks" (`Extent::Fill`), and margins through `margin(top, right, bottom, left)`. Every `NewLayerShellSettings` gets `blur_option`. `SizeChange` became `LayoutChange { id, anchor, size }` with each surface's own anchor. The tray popup takes `PixelSize`/`Position`. The `winit-core` pin was dropped (it was unused). RED: the 2 helper tests were written with their code. GREEN `cargo test` 235/235. clippy clean apart from the 2 old warnings in notifications.rs and tray.rs. Live on HDMI-A-2, `hyprctl layers -j`: bars 1920x44 and 2560x44, wallpapers full-screen, launcher centered at 720x454 and resized to 1040x640 on the gallery page, side panel 460x1036 under the bar, clock tooltip 320x182 under the pointer, calendar 840x390, all with their catchers, which close. NOT tested: the alert card, the OSD, notification popups, the snip overlay, monitor hotplug, and the recovery path itself.
- L3 Known, not caused by the migration: resizing the tray menu popup is a no-op (it was in 0.19.1 too; `set_size`/`set_layout` only act on layer surfaces). Opening a submenu (e.g. Discord's "Troubleshooting") draws its rows over the old size; 0.19.1 closed the menu instead. Follow-up: `PopUpReposition`.
