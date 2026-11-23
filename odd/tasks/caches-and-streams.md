# Caches and background downloads

Objective: the remaining confirmed points of the performance audit, after measuring which ones matter: automatic pruning of Zehll's disk caches, thumbnail streams that are never cancelled (and race on the same partial file), Hyprland requests without a timeout, the snip frame copied twice, and the meeting alerts memory without a bound. Branch: `fix/caches-and-streams` (from main, after #51).
Delivery: one commit per task. Test runner: `cargo test`.

## Specs

- S1 "podemos seguir con los otros puntos?"
- S2 "si, avanza con esos 5 puntos, con respecto de el punto 1, podemos hacer la poda automatica? para que no haya que podarlo a mano".

## Assumptions

- A1 Scaled wallpapers: keep the 12 most recently used files (a few wallpapers for two monitors); the current wallpaper's files are always kept, and a reused file is touched so it counts as recent.
- A2 Wallhaven: keep the last 3 applied-but-not-saved wallpapers in `wallhaven/full` (the current one is the newest) and the 300 most recently shown thumbnails. Gallery thumbnails of images no longer in the folder are deleted when the Wallpapers page lists it (never on an empty listing, which may be a read error).
- A3 Partial files are unique per write (`<file>.<pid>-<n>.part`); prune deletes partial files older than an hour.
- A4 Hyprland requests give up after 500 ms per read or write (replies take well under 1 ms).
- A5 Out of scope, measured as negligible on this machine: `/proc` reads every 2 s (about 0.4 ms), Hyprland requests (0.05 to 0.5 ms), the launcher ranking in `view` (0.06 to 0.3 ms per call with 207 apps), nvidia-smi (about 20 ms, only with the popover open). Not measured, left out: mpris re-sending players, canvases without `canvas::Cache`.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S2 | inline | done | `fix: prune the caches automatically` |
| T2 | S2 | inline | done | `fix: stop thumbnail downloads when a search changes or the launcher closes` |
| T3 | S2 | inline | done | `fix: time out Hyprland requests` |
| T4 | S2 | inline | done | `fix: share the snip frame's pixels instead of copying them` |
| T5 | S2 | inline | done | `fix: forget meeting alerts that can no longer fire` |

## Log

- L1 (2026-11-23) Measured before coding: `~/.cache/zehll/wallpaper` held 58 PNGs (175 MB) and grew by one per wallpaper per monitor; `wallhaven/full` 21 MB, `wallhaven/thumbs` 5.2 MB (195 files), `thumbs` 772 KB. The costs listed in A5 were measured with a Python timing loop, a temporary release-mode bench test of `launcher::items` (removed) and `nvidia-smi` timings.
- L2 T1 new `cache` module: pure `stale` (newest first, kept files always stay), `partial`, `touch`, `prune`. `wallpaper::prepare_blocking` touches reused files and prunes to 12; `wallhaven::download` touches cached files, writes through a unique partial file and runs curl with `--max-filesize` and `kill_on_drop`; `fetch` prunes `full` to 3, `thumbnails` prunes `thumbs` to 300; `gallery::prune_thumbs` drops orphans. Tests RED 3 failed / 216 passed, GREEN 219.
- L3 T2 `Gallery::track/stop` and `Browser::track/stop` keep `abort_on_drop` handles of `Task::abortable` thumbnail tasks: a new listing replaces the gallery task, a new search (`begin`) drops the Wallhaven tasks and the thumbnail map, `close_launcher` stops both, and entering Wallhaven again restarts the missing thumbnails. RED 2 failed / 219 passed, GREEN 221.
- L4 T3 `hyprland::request_at` sets read and write timeouts; a test serves an answering socket and a stuck one (gives up in under 1 s). RED 1 / GREEN 222.
- L5 T4 the snip keeps only the `Handle`, built from the frame's own buffer (`into_raw`, no clone); `snip::dimensions` and `cropped` read the handle's pixels. RED 1 / GREEN 223.
- L6 T5 `meetings::Fired` maps each alert to its meeting's start and `due_alerts` forgets those past the late window. RED 1 / GREEN 224.
- L7 Each commit tested alone in a worktree (219, 221, 222, 223, 224). Clippy: only the 2 existing `chunks_exact` warnings. Live (release build, Zehll otherwise stopped): at startup the scaled cache went from 58 files / 175 MB to 12 files / 37 MB, the current wallpaper's two files kept (newest) and both wallpaper layers open; idle 30 s RSS 443 MB, CPU 0.33 %, no zombies. NOT tested live: the Wallhaven and gallery pruning and stream cancelling (need the launcher open on the user's monitor), the snip (freezes every monitor), a stuck Hyprland (covered by the socket test).
