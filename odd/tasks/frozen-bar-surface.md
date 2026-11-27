# Frozen bar surface

Objective: a bar that stops redrawing on one monitor recovers by itself. Branch: `fix/frozen-bar-surface` (from `main`).
Test runner: `cargo test`.

## Specs

- S1 "cambie de wallpaper desde wallhaven y me desaparecio el indicador de titulo en la barra", then "veo que se congelo la barra del monitor DP-1": DP-1's bar kept its last frame (clock at 13:52, no window title) while HDMI-A-2's bar kept updating.
- Assumption (A1): the cause is iced_layershell 0.19.1 ignoring `SurfaceError::Outdated`/`Lost` from wgpu (waycrate/exwlshelleventloop#360, NVIDIA): the swapchain is never reconfigured, so every later present of that surface fails. It is inferred from the symptoms and the upstream report, not seen in a log: the freeze could not be reproduced, and 0.19.1 logged the error only through `tracing`, which Zehll does not install.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | A1 | inline | done | `chore: vendor iced_layershell 0.19.1` |
| T2 | S1, A1 | inline | done | `fix: recover bar surfaces whose swapchain went stale` |

## Log

- L1 (2026-11-27) Diagnosis on the frozen instance (PID 1777, up 12 h): the main thread sat in `epoll_wait` (1 CPU tick in 5 s); HDMI-A-2's bar drew the current time and title; hovering DP-1's bar did not redraw it; both Hyprland event sockets connected; no leftover catcher layer; no NVRM/Xid in the kernel log. The clock froze at 13:52, before the Wallhaven wallpapers were applied (14:15-14:17), so the wallpaper change is not the trigger; the title was empty because the last frame was drawn while no window had focus. Hyprland's log has no timestamps, so the trigger stays unknown. A 4 s DPMS off/on cycle did not reproduce it (before or after the fix).
- L2 T1, T2 done. `vendor/iced_layershell` (crates.io 0.19.1 + upstream `LICENSE`) through `[patch.crates-io]`. T2 backports upstream PR #401 (`Outdated` reconfigures the surface, `Lost` recreates it, both request a redraw) and prints every present error to stderr, so a new freeze leaves `zehll: presenting a layer surface failed: ...` in `zehll.log`. `cargo test` 233/233. Live: a test instance from the branch drew both bars, and after a DPMS cycle DP-1's clock advanced a minute with no present errors. NOT tested: the recovery path itself (no way found to force `Outdated`/`Lost` on demand). If DP-1 freezes again without that log line, the next suspect is a missing `wl_surface.frame` callback (layershellev waits for it forever) and needs a watchdog.
