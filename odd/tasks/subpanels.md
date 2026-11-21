# Quick settings sub-panels

Objective: HUD sub-panels for Wi-Fi, Bluetooth and audio, opened from the quick settings tiles (artboard Subpanels of the Zehll Bar Design canvas). Branch: `feat/subpanels` (from `feat/side-panel`).
Delivery: one commit per sub-panel. Test runner: `cargo test`.

## Specs

- S1 "para la red al hacer click aparece esta sub panel flotante, para elegir red, en details abre una interfaz de wifi". Details runs ii's `kcmshell6 kcm_networkmanagement`.
- S2 "en bluetooth muesta ese subpanel flotante, en detail tambien abre una interfaz de bluetooth". Details runs ii's `kcmshell6 kcm_bluetooth`.
- S3 "el audio, detail abre pulseaudio control". Details runs `pavucontrol`.
- S4 Opening like ii: left click on a tile toggles, right click opens its sub-panel. Sub-panels float over the dimmed panel inside the panel surface; DONE, Escape or a click outside closes them.
- Constraint: data is read only while a sub-panel is open.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S3, S4 | inline | done | `feat: add the audio sub-panel` |
| T2 | S2 | inline | pending | |
| T3 | S1 | inline | pending | |

## Log

- L1 (2026-11-21) Request: "los subpaneles: habria que hacer los diseños ... por ahora esos 3", then "se ven bien, implementemos".
- L2 ii runs `kcmshell6 kcm_networkmanagement`, `kcmshell6 kcm_bluetooth` and `pavucontrol-qt`/`pavucontrol` for Details (`apps` in ii's config). System: wlan0 present (disconnected, user is wired); pactl 17 has `-f json`; streams play through the EasyEffects sink; BlueZ exposes Battery1.
- L3 T1 done: sub-panels render inside the panel surface over a dimmed layer (click closes), opened with a right click on a tile; Escape closes the sub-panel before the panel. Audio: `src/mixer.rs` reads `pactl -f json` sink inputs and sinks plus the default sink, refreshed through the shared `audio::watch` (pactl subscribe, only while the audio sub-panel is open). Rows show the app icon (looked up once by executable), title, percent and a 20-cell segmented volume (click sets, scroll ±5); a pick list sets the default sink; Details runs pavucontrol. Parser tests RED then GREEN (25 total). Verified by screenshot: 4 real streams, Firefox icon, SOUNDCORE LIFE Q20 selected. Not verified on purpose: changing volumes or the default sink (would alter the user's audio routing through EasyEffects); clicks.
