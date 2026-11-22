# Workspace indicator

Objective: show the workspaces of each monitor left of the clock, with the current one marked and a shader trail when it moves (artboard Main, "Bar in context", of the Zehll Bar Design canvas). Branch: `feat/workspaces` (from `main`).
Delivery: stacked PRs to main, one per task (T2, T3, T4), each based on the previous branch.
Test runner: `cargo test`.

## Specs

- S1 "agregar a la izquierda de la fecha los workspaces y en cual se esta ... 4 indicadores de workspaces con 1 indicador que diga en cual se esta parado ... se usa un plugin para dividir workspaces por monitor, el indicador deberia mostrar el workspace por monitor": four dots left of the clock; the current workspace is an accent disc with a dark core (the user's reference image). Each bar shows its own monitor's workspaces (hyprsplit, `num_workspaces = 4`: DP-1 1–4, HDMI-A-2 5–8), not the focused monitor's.
- S2 "habria que actualizar el artifact del diseño antes de implementar": design first in the artifact; approved ("se ve bien el diseño").
- S3 "que dejo una estela cuando el indicador se desplaza a otra posicion, hoy usamos shaders asi que seria bueno tambien usarlos": the disc slides to the new slot (ease-out, 160 ms) and a tapered accent streak follows it, fading by 450 ms, drawn with a wgpu shader like the flame. Fixed 22 px slots. Without wgpu the disc slides with no trail.
- Assumption (A1): the slot of a workspace is `(id - 1) % 4`, which matches hyprsplit's ranges for any monitor order; 4 is a constant (`workspaces::PER_MONITOR`), not read from the Hyprland config.
- Assumption (A2): bars are opened by Zehll, one per Hyprland monitor (`OutputOption::OutputName`), instead of `StartMode::AllScreens`, because iced_layershell does not tell which output an `AllScreens` surface is on. Monitors added later get a bar after 500 ms (the wl_output name must reach layershellev first); removed monitors close theirs. Without Hyprland no bar opens.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S2, S3 | inline | done | artifact Main.dc.html v15 (no commit), approved |
| T2 | S1, A1, A2 | inline | done | `feat: show each monitor's workspaces in its bar` (branch feat/workspaces-monitors) |
| T3 | S3 | inline | done | `feat: trail the workspace indicator with a shader` (branch feat/workspaces-trail) |

## Log

- L1 (2026-11-22) Request S1 with a reference image; artifact v14 added the dots and a per-monitor panel. The Hyprland config had `num_workspace` (typo, fixed by the user) and ii's SUPER+0..9 binds (groups of 10) fired with the hyprsplit binds; ii's binds removed in `custom/keybinds.lua`.
- L2 (2026-11-22) S3; artifact v15: fixed 22 px slots and the trail frames (0/90/160/300/450 ms). "empieza la implementacion".
- L3 T2 and T3 done. `hyprland.rs`: `Monitor`, `monitors()` (`j/monitors`), `monitor_events()` re-reads the monitors on `workspace`, `focusedmon`, `moveworkspace`, `monitoradded` and `monitorremoved` (v1 and v2) and emits only changes; the socket2 connection moved to `event_lines()`, shared with `active_window_events`. `main.rs`: `StartMode::Background`, bars opened per monitor (`bar_settings`, namespace `zehll`, Top layer, 44 px exclusive zone), `bars: Id -> monitor`, `set_monitors` moves the indicators and opens or closes bars, a 16 ms `WorkspaceFrame` tick only while a disc moves. `workspaces.rs`: slot `(id - 1) % 4`, timing (head ease-out cubic over 160 ms, tail `t^1.6` over 320 ms and never ahead of the head, fade smoothstep from 55 % to 450 ms), a new move starts where the disc is drawn; view = dots (`theme.bracket`) + trail shader + disc (accent with `bar_background` core and glow) placed by left padding. `trail.rs`/`trail.wgsl`: tapered capsule tail→head (half width 2 → 8 px), glow 4 px, hot end mixed from the accent toward white, premultiplied; same pipeline pattern as the flame. RED 7/7, GREEN `cargo test` 148/148, clippy clean on the touched files (2 old warnings in notifications.rs and tray.rs). Live (Zehll was not running; started from target/release and stopped after): one `zehll` layer per monitor on its output; 8 hyprsplit switches across both monitors, each bar matched its monitor's slot; frames show the disc sliding 2 slots in ~150 ms with the streak behind it both ways. The first switch tried (HDMI 5 → 7) did not show in Zehll in one capture and was not reproduced afterwards. Not tested: monitor hotplug, the software fallback. The clock tooltip stays centered on the screen, now ~50 px left of the clock (same as with the media button).
