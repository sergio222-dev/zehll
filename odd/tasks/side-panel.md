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
| T3 | S5 | inline | done | `feat: add network and Bluetooth toggles` |
| T4 | S6 | inline | done | `feat: add night light and keep awake toggles` |
| T5a | S7 | inline | done | `feat: serve notifications and list them in the panel` |
| T5b | S7 | inline | pending | |
| T5c | S7 | inline | pending | |
| T6 | S8 | inline | pending | |

## Log

- L1 (2026-11-21) Request: "haz un plan para encarar las tareas de implementacion de esto" (side panel mockup, artboard SidePanel of the Zehll Bar Design canvas), then "dale" to start phase 1.
- L2 Decisions: open like ii (S1); notifications option B (S7). swaync currently owns `org.freedesktop.Notifications` and exposes only count/DND/clear over D-Bus.
- L3 Engram mirror pending: Engram does not know the `zehll` project yet.
- L4 T1 done: panel layer (460 px, Overlay, OnDemand keyboard) plus a transparent full-screen catcher created only while open; SIGUSR1 toggles; Escape closes; hover zone on the bar's right edge opens. Surfaces now live in a registry until `WindowClosed`, so closing surfaces never render as a bar. Calendar grid tests RED (todo!) then GREEN (4 tests pass). Verified with `pkill -USR1 -x zehll`: panel and catcher layers appear and disappear; screenshot matches the mockup. Not verified: hover, Escape and click-outside (need real input).
- L5 (2026-11-21) User: "no podemos hacer que lo anime hyprland? el atajo lo podemos dejar registrado? no quiero agregar el shortcut todavia para no molestar a qs". S1 updated: the toggle is the Hyprland global shortcut `zehll:panelToggle` (protocol hyprland-global-shortcuts-v1), replacing SIGUSR1; the user's Super+N keeps opening ii for now. Slide-in comes from Hyprland layer rules for `^zehll-panel$` (slide right) and `^zehll-panel-catcher$` (no_anim), added to the user's rules.lua. Verified: `hyprctl globalshortcuts` lists `zehll:panelToggle`; `hyprctl dispatch 'hl.dsp.global("zehll:panelToggle")'` opens and closes the panel; config reload reports no errors.
- L6 T2 done: `src/audio.rs` reads default sink/source with `wpctl` and listens to `pactl subscribe`, coalescing each event burst (50 ms) into one read and emitting only on change. The subscription runs only while the panel is open (`kill_on_drop` stops pactl on close). Parser tests RED (todo!) then GREEN. Verified: tiles render, an external `wpctl set-mute` on the mic updates the tile live, and Zehll's pactl process exists only while the panel is open. Not verified: clicking a tile (needs real input).
- L7 (2026-11-21) User: "si le hago click varias veces en la esquina parece como si abriera varias veces la misma". Cause: the catcher covers the bar too, so a click in the corner closes the panel, the catcher disappears, the pointer re-enters the bar's hover zone and `on_enter` reopens it; fade-out and slide-in overlap. Fix: hovering does not reopen the panel within 400 ms of closing (`PANEL_REOPEN_DELAY`); the global shortcut is unaffected. Not verified with real clicks.
- L8 T3 done: `src/network.rs` (NetworkManager) and `src/bluetooth.rs` (BlueZ ObjectManager) read state over the system bus with zbus 5 (already a dependency of system-tray) and re-read on any signal from each service, coalescing bursts (`src/dbus.rs`). Proxies do not cache properties, so zbus adds no extra subscriptions. Internet click toggles the Wi-Fi radio like ii (`nmcli radio wifi`); Bluetooth click toggles the adapter's Powered. Subscriptions run only while the panel is open. Tests RED (todo!) then GREEN (9 total). Verified: tiles show WIRED and SOUNDCORE LIFE Q20. Not verified on purpose: toggling Wi-Fi/Bluetooth (would disrupt the user's session).
- L9 T4 done: night light uses `hyprctl hyprsunset temperature` with ii's values (5000 K on, 6000 K off), read when the panel opens and after each toggle; hyprsunset's own socket speaks a binary protocol, so it is not queried directly. Keep awake holds a logind `Inhibit("idle")` lock (honored by hypridle 0.1.8), released by dropping the descriptor. Tests RED (todo!) then GREEN (11 total). Verified: tiles show `ON · 5000K` and `OFF`; with keep awake on, `systemd-inhibit --list` shows `Zehll idle Keep awake block`, and the lock disappears when Zehll exits. Not ported: ii's automatic night light schedule (19:00–06:30).
- L10 (2026-11-21) User: "podemos hacer que persista el keep awake? se puede portar el modo automatico de nigh light?". Added `src/state.rs` (`~/.local/state/zehll/state.json`, `keep_awake`), restored at startup. Ported ii's automatic night light with the user's ii values (19:00–06:30, 5000 K) as constants: the tick applies the schedule at startup and on each boundary; manual toggles hold until the next boundary; the tile shows `AUTO` while following the schedule. Schedule test RED then GREEN (12 total). Verified: a state file with `keep_awake: true` makes Zehll take the logind lock at startup; tile shows `AUTO · ON · 5000K` at 03:xx. Test state file removed afterwards.
- L11 (2026-11-21) User: "por ahora pasaremos a la fase 5" (settings sub-panels come later). T5 split: T5a server + panel list (close, clear, count, DND); T5b popups; T5c actions and images. Tested inside `dbus-run-session` so swaync keeps the real name; Zehll queues for `org.freedesktop.Notifications` and takes it when swaync exits.
- L12 T5a done: `src/notifications.rs` serves org.freedesktop.Notifications with zbus (Notify, CloseNotification, GetCapabilities, GetServerInformation, NotificationClosed); the panel lists notifications newest first with dismiss, clear all, count and do not disturb (persisted in state.json). Markup test RED then GREEN. Gotcha: `request_name_with_flags(NAME, Default::default())` inferred zbus's `RequestNameFlags` default (DoNotQueue) and failed with "name already taken"; an explicit empty `BitFlags` queues correctly. Verified in `dbus-run-session`: three `notify-send` notifications listed (critical has accent border, body markup stripped), client `CloseNotification` removes one; on the real bus swaync stays owner and Zehll is queued second. Not verified: dismiss/clear/DND clicks.
