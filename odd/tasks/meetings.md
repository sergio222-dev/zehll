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
| T2 | S1 | inline | done | `feat: add the meetings model and pure helpers` (a856c50) |
| T8 | S3 | inline | done | `feat: compute due meeting alerts` (5e7d683) |
| T3 | S1, S2 | inline | done | `feat: fetch meetings from gcal.py` (820db3b) |
| T4 | S1 | inline | done (visual check pending) | `feat: show the next meeting pill in the bar` |
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
- L5 T8 done: `due_alerts(meetings, now, fired)` in `src/meetings.rs` returns the `Warning` (start within the next 5 min) and `Start` (from the start up to 5 min late, as calendarz) alerts due at `now`, each once per kind; `fired` keys on kind, id and start, so a rescheduled meeting alerts again. RED 3/3, GREEN `cargo test` 43/43.
- L6 T3 done: `meetings::fetch` runs the script embedded with `include_str!` through `python3 -c`, so the binary needs no install path; fetched at startup, every 2 min (calendarz's interval), on month change and when the panel reopens on another month. Range: the shown month grid plus today and tomorrow (`fetch_range`; `calendar::grid_start` now shared with `month_grid`). A reply for a range no longer current is dropped; on error the previous meetings and status stay. Fixed a T2 bug: `parse_events` expected a bare array but `gcal.py` prints `{"events": [...]}`. RED 2/2 (parse with the real shape, `fetch_range`), GREEN `cargo test` 44/44. Real check: the embedded script through `python3 -c` with the November grid in RFC 3339 returned exit 0 and 68 events with the same keys as the test.
- L7 T4 done: `meeting_pill` beside the clock (30 px, `#0d1430` with the border colour, glowing diamond from `shape::Kind::Diamond`, `calendar_today` icon, condensed 16 px label); `Urgency::color` holds the artboard colours. Only `SIGN IN` is clickable: it runs the embedded `gcal.py auth` (browser flow, stopped after 5 min, one at a time) and fetches again. RED 1/1 (`urgency_colors_follow_the_artboard`), GREEN `cargo test` 45/45. Visual check pending: ii (Quickshell) is running and its 63 px `quickshell:bar` layer covers Zehll's bar, so a screenshot showed ii; not stopped without asking.
