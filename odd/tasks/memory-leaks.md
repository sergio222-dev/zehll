# Memory leaks and process hygiene

Objective: fix the four confirmed memory/process problems from the performance audit: `gsettings` spawned per icon lookup, the tray stream dying on a lagged broadcast, the unbounded notification history with full-size images, and zombie processes from detached spawns. Branch: `fix/memory-leaks` (from main).
Delivery: one commit per task. Test runner: `cargo test`.

## Specs

- S1 "podemos buscar memory leaks o posibles problemas de memorias? oportunidades de mejora para velocidad o mejor manejo de memoria".
- S2 "arranca con los puntos 1 a 4" (points 1 to 4 of the audit report: gsettings, tray lag, notification history, zombies).

## Assumptions

- A1 The notification history keeps the newest 50; dropped ones are reported to clients as `NotificationClosed` with reason 4 (undefined), since they were neither dismissed nor expired.
- A2 `image-data` pictures are stored at most 64 px on their largest side (the panel draws them at 32 px). `image-path` icons stay as path handles: they hold no pixels until drawn.
- A3 After a lagged tray broadcast, the bar drops every item and rebuilds them from the client's snapshot, closing an open tray menu.
- A4 Out of scope (later slices): the IO on the UI thread (Hyprland socket, `/proc` reads), view-time ranking in the launcher, the snip buffer copy, thumbnail stream cancellation and disk cache pruning.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S2 | inline | done | `fix: read the GTK icon theme once` |
| T2 | S2 | inline | done | `fix: resync the tray after a lagged broadcast` |
| T3 | S2 | inline | done | `fix: cap the notification history and shrink its images` |
| T4 | S2 | inline | done | `fix: reap detached programs` |

## Log

- L1 (2026-11-23) Diagnosis, measured on main (2f39dad, release build, Zehll otherwise stopped). Idle 3 min on the real bus: RSS 452 MB (anon 45, file 331, shmem 77), CPU 0.36 %, 25 threads, 99 fds, no zombies, flat memory. A `gsettings` wrapper first in `PATH` logged 194 spawns in the first 3 s (one per `.desktop` icon lookup in `apps::load`, also without a tray) and 100 spawns for 100 icon-name notifications: `freedesktop_icons::default_theme_gtk` runs `gsettings` on every call (`tray.rs:118`). On a private session bus (swaync owns the real name, and Zehll queues behind it): 200 notifications with a 256x256 `image-data` raised the heap from 38 to 179 MB, nothing freed 30 s later, 400 reached 292 MB heap / 684 MB RSS. Tray lag (`broadcast::channel(32)` in system-tray 0.8.9, `while let Ok` ends the stream on `Lagged`) and the zombies (`std::process::Command::spawn` without `wait` at `main.rs:944,1559,1606`) are confirmed in the code only.
- L2 T1-T4 done. T1 `tray::icon_theme` caches the theme in a `OnceLock`. T2 the tray loop maps each `recv` through the pure `step`: `Lagged` sends `TrayEvent::Reset` plus a fresh `snapshot` of `Add`/menu events (the bar clears its items and closes an open tray menu), only `Closed` ends the stream; tokio's `sync` feature is now listed (it was already enabled by system-tray). T3 pure `notifications::push` (replace in place or insert at the top, drop past `MAX_HISTORY` = 50, return the dropped ids, reported with reason 4) and `shrink` (`image::imageops::thumbnail` to at most 64 px). T4 `system::spawn` runs the program and waits for it on a thread; SubpanelDetails, PowerMenu and `open_link` use it. Tests RED (4 failed, 212 passed) then GREEN (216). Clippy: only the 2 existing `chunks_exact` warnings. Live, same release build and scripts as L1: `gsettings` 194 to 1 at startup and 100 to 0 for 100 icon-name notifications; idle 90 s RSS 444 MB, CPU 0.30 %, no zombies; 1000 notifications with a 256x256 `image-data` in bursts of 200 kept the heap between 109 and 172 MB (vs linear 38 to 292 MB for 400 before): what remains is allocator memory kept after each burst, not growth. NOT tested live: a lagged tray broadcast (needs more than 32 tray events queued) and the reaping of xdg-open/wlogout/DETAILS from a real click (covered by the `spawned_programs_are_reaped` unit test).
