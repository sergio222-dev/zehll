# MonoLisa for numbers and code

Objective: use MonoLisa Code for values that update on their own and for code or data in the launcher, keeping Barlow for the rest (artboard Typography of the Zehll Bar Design canvas). Branch: `feat/monolisa-numbers` (from main).
Delivery: one commit. Test runner: `cargo test`.

## Specs

- S1 "agregue @file:MonoLisaCode.zip para usar las fuentes, puedes usar esas fuentes para zehll?" and "instala los ttf tambien". Placement chosen on the recommendation ("donde crees que seria mejor usarla?"); board approved ("aprobado").

## Assumptions

- A1 MonoLisa's license (personal) forbids sharing it and app embedding needs the commercial license; the repository is public. Zehll asks for `MonoLisaCode` by name from the system fonts, the TTFs are installed in `~/.local/share/fonts/MonoLisa` and never committed, and `MonoLisa*.zip` is git-ignored.
- A2 Two constants: `MONO` (Regular) for code and data, `MONO_VALUE` (SemiBold) for values; values drop 1 to 2 px against Barlow Condensed because MonoLisa is wider (advance 0.64 em).
- A3 Stays in Barlow: tags, titles, the clock, prose, labels, the big ring percentage of the system popover (its headline).
- A4 Out of scope: other changing numbers not on the board (volume and brightness values, meeting countdowns, pomodoro).

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | inline | done | `feat: use MonoLisa for numbers and code` |

## Log

- L1 (2026-11-23) The zip holds two variable TTFs (Upright, Italic; weights Thin to Black). cosmic-text 0.15 applies the `wght` axis of variable fonts (`font/mod.rs:142`, `swash.rs:25-40`), so no static instances were needed. Barlow's digits are proportional (at 100 px, '1' 35.2 and '0' 56.6 units), MonoLisa's are all 64. Board Typography designed and published (artifact version 30), approved; on the board the next element shifts 14 px from 1% to 100% with Barlow and 0 with MonoLisa.
- L2 T1 done: `MONO`, `MONO_VALUE`, `MONO_ADVANCE` and the pure `mono_width` in `main.rs`. System indicator values in `MONO_VALUE` 14, right-aligned in `mono_width(4, 14)`; tooltip values, ring temperature/clock/memory values, disk sizes, network axis (width for "124.0 KB") and legend in `MONO`; launcher calculator card, clipboard and history titles, keybind combos and the former `Font::MONOSPACE` texts in MonoLisa. Docs: `docs/theme.md` and the README rows. Tests RED (1 failed, 224 passed) then GREEN (225). Clippy: only the 2 existing `chunks_exact` warnings. Live on DP-1 (release build, the user's instance stopped with permission and replaced by this one): the bar indicator matches MonoLisa's shapes (compared against MonoLisa and Noto Sans Mono renders), the system tooltip and popover show the values in MonoLisa with labels in Barlow; the first popover capture showed the network axis labels wrapping in 44 px, fixed by widening the axis and keeping USED in Barlow, then re-checked. NOT tested live: the launcher pages (calculator, clipboard, keybinds), which grab the keyboard on the user's monitor.
