# Meetings calendar and alerts

Objective: port calendarz (Google Calendar meetings pill, calendar popover and alerts) to Zehll without Quickshell, following the artboards Calendar and MeetingAlert of the Zehll Bar Design canvas. Branch: `feat/meetings` (from `main`).
Delivery: one commit per task. Test runner: `cargo test`.

## Specs

- S1 "se quiere implementar la vista de calendarios de el artifact de zhell, usando como base @~/code/calendarz, la idea es implementarlo sin usar quickshell". Artboard Calendar: meeting pill next to the clock (grey > 15 min, green 15 to 5 min, yellow ≤ 5 min, red `SIGN IN` when the token expired), hover tooltip (`10:00 – 10:45 · IN 4 MIN`), calendar popover opened from the date (month with week numbers and meeting dots, agenda of the selected day: `DONE`, next with `JOIN`, `IN 5H 34M`).
- S2 Data comes from calendarz's `gcal.py` (standard library only), vendored as `scripts/gcal.py`, reusing the existing login in `~/.config/calendarz/`. `events <from> <to>` (ISO 8601) returns the events in that range with `end`; JSON output and exit codes unchanged (0 ok, 2 reauth, 1 error).
- S3 "incluyamos la fase 2 de alertas". Artboard MeetingAlert ("diseña la tarjeta", approved): 5 min before, a normal notification `Starts in 5 minutes`, no sound; at start time, a persistent alert card below the clock with `JOIN` (`OPEN` without a Meet link) and `DISMISS`, ringing every 5 s while any card is open; a test trigger without a real meeting.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S2 | inline | done | `feat: vendor gcal.py with a date range and end times` |
| T2 | S1 | inline | done | `feat: add the meetings model and pure helpers` |
| T8 | S3 | inline | pending | |
| T3 | S1, S2 | inline | pending | |
| T4 | S1 | inline | pending | |
| T5 | S1 | inline | pending | |
| T6 | S1 | inline | pending | |
| T9 | S3 | inline | pending | |
| T10 | S3 | inline | pending | |
| T11 | S3 | inline | pending | |
| T12 | S3 | inline | pending | |
| T7, T13 | S1, S3 | inline | pending | |

T1 `gcal.py` range + `end`; T2 `meetings.rs` model and pure helpers; T8 `due_alerts`; T3 fetch subscription; T4 bar pill; T5 pill tooltip; T6 calendar popover; T9 5 min notification; T10 alert card surface; T11 ringing; T12 `zehll:meetingTest` shortcut; T7/T13 README. Route: inline, no writer trigger (sequential units sharing `main.rs`).

## Log

- L1 (2026-11-21) "se quiere implementar la vista de calendarios de el artifact de zhell, usando como base @~/code/calendarz, la idea es implementarlo sin usar quickshell, hacer un plan de implementacion"; then "incluyamos la fase 2 de alertas", "diseña la tarjeta", "aprobado, vamos con T1".
- L2 Plan: reuse `gcal.py` as a subprocess instead of OAuth in Rust (no HTTP dependency in Zehll; costs a `python3` runtime dependency). Alert card is its own layer surface, not a critical notification, since notification actions go back to the D-Bus sender.
- L3 T1 done: `scripts/gcal.py` copied from calendarz; `events FROM TO` replaces the fixed 24 h horizon, adds `end`, raises `maxResults` to 250 per calendar; `events` without a range prints the usage (exit 1). No Python test runner in the repo: checked with `py_compile` and a real fetch of the November 2026 grid (2026-10-26 to 2026-12-07): exit 0, 68 events, every one with `end`.
- L4 T2 done: `src/meetings.rs` with `Meeting` (times converted to local `NaiveDateTime` at parse time so helpers stay pure), `Status` from the `gcal.py` exit code, pill (`SIGN IN` on reauth, nothing on error), urgency thresholds, countdown, tooltip, day grouping, agenda states and Meet-link detection. RED 10/10, GREEN `cargo test` 40/40; 17 `dead_code` warnings until T3/T4 wire the module. Assumptions: the pill only looks 24 h ahead (calendarz's old horizon); a meeting in progress reads `NOW` in the agenda; countdowns of a day or more read `IN 2D 3H`; minutes round up. Built with `CARGO_TARGET_DIR` on tmpfs because `/home` was full.
