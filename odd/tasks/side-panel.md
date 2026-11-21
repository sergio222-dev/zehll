# Side panel

Objective: HUD-styled right side panel replacing ii's right sidebar. Branch: `feat/side-panel`.
Delivery: one commit per phase on the feature branch (ask-on-risk). Test runner: `cargo test`.

## Specs

- S1 Open/close like ii: "al pasar el mouse por el borde derecho superior de la status bar o apretando super+n para togglear, al hacer click fuera se cierra". Toggle via the Hyprland global shortcut `zehll:panelToggle`; Esc also closes; Hyprland layer rules animate the slide-in.
- S2 Header: uptime, reload and power actions (power opens `wlogout`). Edit and settings actions are omitted until they have a target.
- S3 Calendar: month grid starting on Monday, today highlighted, previous/next month navigation; resets to the current month when the panel opens.
- S4 Audio output and microphone toggles (mute), event-driven via `wpctl` / `pactl subscribe`.
- S5 Network and Bluetooth status and toggles via NetworkManager and BlueZ D-Bus.
- S6 Night light (`hyprsunset`) and keep awake (inhibit `hypridle`).
- S7 Notifications: "yo prefiero la B" — Zehll implements its own `org.freedesktop.Notifications` server with list, popups, DND and clear, in HUD style. Requires disabling swaync.
- S8 To Do tab (reads and writes ii's `todo.json`) and Timer tab.
- Constraint: event-driven, no polling; the panel surface exists only while open.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1-S3 | inline | done | `feat: add side panel with calendar` |
| T2 | S4 | inline | done | `feat: add audio toggles to the side panel` |
| T3 | S5 | inline | pending | |
| T4 | S6 | inline | pending | |
| T5 | S7 | inline | pending | |
| T6 | S8 | inline | pending | |

## Log

- L1 (2026-11-21) Request: "haz un plan para encarar las tareas de implementacion de esto" (side panel mockup, artboard SidePanel of the Zehll Bar Design canvas), then "dale" to start phase 1.
- L2 Decisions: open like ii (S1); notifications option B (S7). swaync currently owns `org.freedesktop.Notifications` and exposes only count/DND/clear over D-Bus.
- L3 Engram mirror pending: Engram does not know the `zehll` project yet.
- L4 T1 done: panel layer (460 px, Overlay, OnDemand keyboard) plus a transparent full-screen catcher created only while open; SIGUSR1 toggles; Escape closes; hover zone on the bar's right edge opens. Surfaces now live in a registry until `WindowClosed`, so closing surfaces never render as a bar. Calendar grid tests RED (todo!) then GREEN (4 tests pass). Verified with `pkill -USR1 -x zehll`: panel and catcher layers appear and disappear; screenshot matches the mockup. Not verified: hover, Escape and click-outside (need real input).
- L5 (2026-11-21) User: "no podemos hacer que lo anime hyprland? el atajo lo podemos dejar registrado? no quiero agregar el shortcut todavia para no molestar a qs". S1 updated: the toggle is the Hyprland global shortcut `zehll:panelToggle` (protocol hyprland-global-shortcuts-v1), replacing SIGUSR1; the user's Super+N keeps opening ii for now. Slide-in comes from Hyprland layer rules for `^zehll-panel$` (slide right) and `^zehll-panel-catcher$` (no_anim), added to the user's rules.lua. Verified: `hyprctl globalshortcuts` lists `zehll:panelToggle`; `hyprctl dispatch 'hl.dsp.global("zehll:panelToggle")'` opens and closes the panel; config reload reports no errors.
- L6 T2 done: `src/audio.rs` reads default sink/source with `wpctl` and listens to `pactl subscribe`, coalescing each event burst (50 ms) into one read and emitting only on change. The subscription runs only while the panel is open (`kill_on_drop` stops pactl on close). Parser tests RED (todo!) then GREEN. Verified: tiles render, an external `wpctl set-mute` on the mic updates the tile live, and Zehll's pactl process exists only while the panel is open. Not verified: clicking a tile (needs real input).
- L7 (2026-11-21) User: "si le hago click varias veces en la esquina parece como si abriera varias veces la misma". Cause: the catcher covers the bar too, so a click in the corner closes the panel, the catcher disappears, the pointer re-enters the bar's hover zone and `on_enter` reopens it; fade-out and slide-in overlap. Fix: hovering does not reopen the panel within 400 ms of closing (`PANEL_REOPEN_DELAY`); the global shortcut is unaffected. Not verified with real clicks.
