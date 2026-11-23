# Emoji search keywords

Objective: the emoji search finds emojis by the words ii found them with (e.g. `thunder` finds ⚡). Branch: `fix/emoji-keywords` (from main).
Delivery: one commit. Test runner: `cargo test`.

## Specs

- S1 "estoy viendo que en el buscador de emoji no salen algunos emojis, por ejemplo si busco thunder no sale el rayo, esto no pasaba cuando usaba II". Fix approved: "dale".

## Assumptions

- A1 Keywords come from emojilib (MIT), the list behind ii's `fuzzel-emoji.sh`; CLDR annotations were left out (they add 0.5 points of coverage for about 350 KB more).
- A2 A keyword match scores 0.8 of a name or shortcode match, above the group match (0.5), so name matches still rank first.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | inline | done | `fix: search emojis by their emojilib keywords` |

## Log

- L1 (2026-11-23) Diagnosis: `emoji::search` scored only the Unicode name and the gemoji shortcodes from the `emojis` crate; ⚡ is "high voltage" / `zap`. ii searches `~/.config/hypr/hyprland/scripts/fuzzel-emoji.sh`, whose lines carry keywords (`⚡ high voltage thunder weather lightning bolt fast`). Over ii's 10,432 emoji/keyword pairs (1,928 emojis), Zehll matched 40.5 %; with gemoji tags 44.2 %, with CLDR 55.3 %, with emojilib 98.9 %, with emojilib and CLDR 99.4 %.
- L2 T1 done: `assets/emoji/keywords.tsv` generated from emojilib's `dist/emoji-en-US.json` (commit b4da69426d5d; lowercased, underscores to spaces, variation selectors dropped, 4 keywords containing commas dropped), its license next to it; `include_str!`, parsed once into a table by the pure `parse_keywords`, looked up by the default tone. Tests RED (5 failed, 221 passed: the 2 new ones and the existing tests that search) then GREEN (226). Clippy: only the 2 existing `chunks_exact` warnings. Release-mode timing of one search (temporary bench, removed): 4.7 to 6.8 ms against 1.2 to 2.4 ms before, per keystroke; `thunder` now returns ⛈️ 🌩️ ⚡ 🫩 instead of only ⛈️. NOT tested live: typing in the Emojis page (it grabs the keyboard on the user's monitor).
