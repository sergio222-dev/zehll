# Quick settings sub-panels

Objective: HUD sub-panels for Wi-Fi, Bluetooth and audio, opened from the quick settings tiles (artboard Subpanels of the Zehll Bar Design canvas). Branch: `feat/subpanels` (from `feat/side-panel`).
Delivery: one commit per sub-panel. Test runner: `cargo test`.

## Specs

- S1 "para la red al hacer click aparece esta sub panel flotante, para elegir red, en details abre una interfaz de wifi". Details runs ii's `kcmshell6 kcm_networkmanagement`.
- S2 "en bluetooth muesta ese subpanel flotante, en detail tambien abre una interfaz de bluetooth". Details runs ii's `kcmshell6 kcm_bluetooth`.
- S3 "el audio, detail abre pulseaudio control". Details runs `pavucontrol`.
- S4 Opening: "no se abren los sub paneles al hacer click en los quicksettings, deberia abrir el subpanel". Clicking a tile's icon toggles; clicking the rest of the tile (or right-clicking) opens its sub-panel; tiles without a sub-panel toggle anywhere. Sub-panels float over the dimmed panel inside the panel surface; DONE, Escape or a click outside closes them.
- Constraint: data is read only while a sub-panel is open.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S3, S4 | inline | done | `feat: add the audio sub-panel` |
| T2 | S2 | inline | done | `feat: add the Bluetooth sub-panel` |
| T3 | S1 | inline | done | `feat: add the Wi-Fi sub-panel` |

## Log

- L1 (2026-11-21) Request: "los subpaneles: habria que hacer los diseños ... por ahora esos 3", then "se ven bien, implementemos".
- L2 ii runs `kcmshell6 kcm_networkmanagement`, `kcmshell6 kcm_bluetooth` and `pavucontrol-qt`/`pavucontrol` for Details (`apps` in ii's config). System: wlan0 present (disconnected, user is wired); pactl 17 has `-f json`; streams play through the EasyEffects sink; BlueZ exposes Battery1.
- L3 T1 done: sub-panels render inside the panel surface over a dimmed layer (click closes), opened with a right click on a tile; Escape closes the sub-panel before the panel. Audio: `src/mixer.rs` reads `pactl -f json` sink inputs and sinks plus the default sink, refreshed through the shared `audio::watch` (pactl subscribe, only while the audio sub-panel is open). Rows show the app icon (looked up once by executable), title, percent and a 20-cell segmented volume (click sets, scroll ±5); a pick list sets the default sink; Details runs pavucontrol. Parser tests RED then GREEN (25 total). Verified by screenshot: 4 real streams, Firefox icon, SOUNDCORE LIFE Q20 selected. Not verified on purpose: changing volumes or the default sink (would alter the user's audio routing through EasyEffects); clicks.
- L4 T2 done: `bluetooth::devices` lists every device BlueZ knows like ii (connected, then paired, then unpaired) with alias, icon kind, Trusted and Battery1 percentage; actions Connect, Pair, Disconnect, Forget (Adapter1.RemoveDevice) and Trust. The sub-panel shows the connected device with glow and a 10-cell battery gauge. Test RED then GREEN (26 total). Verified by screenshot: Soundcore Life Q20 connected at 80%, MI BT 18I not paired with ALWAYS CONNECT and PAIR. Not verified on purpose: the actions (would disconnect the user's headphones); clicks.
- L5 T3 done: `network::wifi_events` finds the Wi-Fi device, requests a scan, and lists access points (one GetAll per AP) merged per SSID (strongest kept, hidden dropped, active first), marking saved profiles; it runs only while the Wi-Fi sub-panel is open. `network::connect` activates a saved profile or calls AddAndActivateConnection, with a WPA-PSK password for new secured networks (inline prompt). Tests RED then GREEN (28 total). Verified by screenshot: 11 real networks with bars, bands and locks. Not verified on purpose: connecting (the user is wired). All three sub-panels done.
- L6 (2026-11-21) User: "no se abren los sub paneles al hacer click en los quicksettings, deberia abrir el subpanel". Cause: sub-panels opened only on right click (copied from ii's tooltip). S4 rewritten: the icon block is a nested button that toggles, the tile body opens the sub-panel, right click still opens it. Verified the tiles render unchanged; clicks not verifiable here.
- L7 (2026-11-21) User: "los volumenes del sonido deberian ser unos sliders". Audio rows now use HUD-styled iced sliders (6 px accent rail, 4 px handle that glows when hovered or dragged); the dragged value is shown locally and pactl runs once on release; the wheel still steps 5%. Verified by screenshot with live streams; dragging not verifiable here.
