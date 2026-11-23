# Emoji picker

Objective: an emoji search page in the launcher, modelled on the user's screenshot (search field, MOST USED, groups with headers, a 6-column grid), designed first in the Zehll Bar Design canvas (artboard Emojis, "Emoji picker"). Branch: `feat/emoji-picker`, stacked on `feat/wallhaven-shortcut` (#41).
Test runner: `cargo test`.

## Specs

- S1 "vamos a agregar otra feature, un buscador de emojis" (with a screenshot of a picker: "Search emojis...", Most Used, Smileys & Emotion, large emojis in a 6-column grid).
- S2 Scope answer: "Página Emojis del launcher (entrada en Home + Super+. la abre directo, grilla 6 columnas como tu captura, reusa búsqueda/tamaño por página)".
- S3 Design: artifact v22, new board Emojis; approved ("aprobado").
- Assumption (A1): the page is 720 x 560 (`SizeChange`), back to 720 x 454 on leaving. `zehll:emojis` toggles the launcher on it, bound to Super+. in `custom/keybinds.lua` (replacing ii's `quickshell:overviewEmojiToggle` and its fuzzel fallback).
- Assumption (A2): data from the `emojis` crate (Unicode groups, names, GitHub shortcodes, skin tones), compiled in. Search ranks with `fuzzy::score` over the name and shortcodes, the group at half weight; ties by use count, then Unicode order.
- Assumption (A3): Enter closes the launcher, copies with wl-copy, gives the focus back to the window that was focused when the launcher opened (Hyprland otherwise focuses the window under the pointer) and types with wtype; Ctrl+Enter only copies and keeps the launcher open; a click picks like Enter. Counts (by default-tone emoji) and the tone persist in `~/.local/state/zehll/emojis.json`; MOST USED is laid out again on the next opening or search.
- Assumption (A4): Emoji 18.0's toned 🫹/🫺 are not offered in tones (`NEWEST_TONED` = 17.0): Noto Color Emoji 2026-09 has no glyphs for them.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S3 | inline | done | artifact Emojis.dc.html v22 (no commit), approved |
| T2 | S1, S2, A1–A4 | inline | done | `feat: add an Emojis page to the launcher` |

## Log

- L1 (2026-11-23) Scope question answered (S2); board Emojis published as v22 (page, search, 2x skin tone detail, flow, spec notes); approved.
- L2 T2. `emoji.rs`: pure `Line`, `layout`, `most_used`, `search`, `count`, `bump`, `with_tone`, `has_tones`, `cycle_tone`, `line_of`, `vertical`, `sideways`, `first_line`, `scroll`, `parse_state`, `state_json`; `Picker` (load/save/refresh/text); `paste`. RED 9/9 with `todo!()` stubs, GREEN 9/9. `launcher.rs`: `Page::Emojis` in Home, `EMOJI_SIZE`, `emoji_body`/`emoji_tile`/`emoji_name`, `tile_row` shared with `tile_rows`, `emoji_select`/`emoji_vertical`/`emoji_sideways`/`emoji_scroll`, the layout redone in `set_query`; its test was written with the code, and the Home list and size tests were updated. `main.rs`: `emoji_nav`, `pick_emoji`, `EmojiClick`, `EmojiScroll`, `previous_window` read on opening; `hyprland::active_address`; shortcut `zehll:emojis`. `cargo test` 203/203; clippy clean on the touched files. New crates in the lock file: `emojis`, `phf`, `phf_shared`.
- L3 Live on HDMI-A-2 (Zehll was not running; started and stopped; probe kitties recording raw input; ydotool). Found and fixed live: (1) many emojis drew as monochrome outlines (font fallback picked a non-color font), fixed by naming Noto Color Emoji; (2) after the launcher closed, Hyprland focused the window under the pointer and the first emoji went to the user's terminal, fixed by refocusing the window that was active at opening (A3); (3) toned 🫹/🫺 drew as a yellow hand plus a swatch. Measured with cosmic-text 0.15 (iced's shaper) over all 3583 emojis and tones: no missing glyphs, only those 10 shape to two glyphs (A4). Verified: Super+. opened the page at 720 x 560 with SMILEYS & EMOTION; "banana" + Enter typed 🍌 into the probe and copied it, counted in the state file; "thumbs" + Alt+Right x3 + Enter typed 👍🏽 and saved tone 3; Ctrl+Enter copied 🍌 without typing and kept the launcher open; eight Downs kept the selected row in view; Shift+Escape went back to Home at 720 x 454; Escape closed. Also during testing, a manual `wtype` after the probes had exited typed "x🍌" into the user's terminal. Cleanup: probes closed, the state file deleted, focus back on DP-1. Not tested: mouse wheel scrolling and clicks on tiles, fractional scale, typing into XWayland apps (wtype uses the virtual keyboard protocol).
