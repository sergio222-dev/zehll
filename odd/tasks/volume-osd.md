# Volume indicator

Objective: show a volume indicator when the default output's volume or mute changes from the keyboard, a peripheral or an app, hidden after a moment (artboard VolumeOsd of the Zehll Bar Design canvas). Branch: `feat/volume-osd` (from `fix/bluetooth-agent`, PR #48, which edits the same subscription list).
Delivery: one commit. Test runner: `cargo test`.

## Specs

- S1 "cuando se modifica el volumen desde el teclado o un periferico que se muetre un indicador del volumen, pasado un tiempo este se esconde, en este caso no hay problema que este indicador tenga efectos de hyprland". Board approved: "aprobado".

## Assumptions

- A1 Defaults on the approved board: top center of the focused monitor under the bar, 320 px, hides 1.5 s after the last change (ii uses 1 s), hovering hides it at once (like ii), above 100 % BOOST in yellow, muted shows a red MUTED chip with the value struck through.
- A2 Not shown at startup, on a switch of the default device, or while the side panel is open.
- A3 Out of scope: microphone and brightness indicators.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | inline | done | `feat: add a volume indicator` |

## Log

- L1 (2026-11-23) Reference: ii's `onScreenDisplay` opens on the focused monitor, under the bar, on `Audio.sink` volume or mute changes, hides after `osd.timeout` (1000 ms) and when hovered. Board VolumeOsd designed and published (artifact version 28), approved.
- L2 T1 done: `src/osd.rs` with pure `changed`, `lit`, `percent` and `mode`, and the view (slanted chip, device name, value, 20-cell gauge, accent brackets; the whole card is a `mouse_area` that hides on enter). `audio::events` now runs always instead of only with the panel open, and `Message::Audio` drives the indicator: a `zehll-osd` Overlay surface anchored top with a 14 px margin, a 100 ms tick only while it is open, and the reading kept until the compositor closes the surface so it fades out showing it. No `no_anim` rule. Tests RED (4 failed, 208 passed) then GREEN (212). Clippy clean on touched files. Live on DP-1 (focused): the surface opened at x 3040 (centered on the 2560 px monitor), y 58, 320x84; screenshots of 72 % (14 cells) and muted (red chip, struck through, dim gauge); hid about 1.5 s after the last change; hovering hid it in 6 ms; with the panel open a change showed nothing, and after closing it the next change showed it. The volume was restored to its value before each test. NOT tested: the boost state live (would play at 125 %), a real key press or headset button (wpctl changes go through the same PipeWire events), and a hot-plugged monitor.
