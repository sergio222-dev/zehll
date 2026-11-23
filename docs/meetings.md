# Meetings

[← Back to the README](../README.md)

Zehll reads your Google Calendar with [`scripts/gcal.py`](../scripts/gcal.py), a read-only client that uses only the Python standard library. The script is embedded in the binary and run with `python3`; it reads the OAuth client from `~/.config/calendarz/credentials.json` and keeps its refresh token in `~/.config/calendarz/token.json`, so an existing [calendarz](https://github.com/sergio222-dev/calendarZ) login is reused. To sign in the first time:

```sh
python3 scripts/gcal.py auth
```

Meetings are fetched at startup, every 2 minutes and when the calendar shows another month. All-day, cancelled and declined events are skipped.

- **Pill**: beside the clock, the start time of the next meeting in the next 24 hours, with a dot that is grey more than 15 minutes before it, green from 15 to 5 minutes and yellow in the last 5. Hover it for the title and `10:00 – 10:45 · IN 4 MIN`. When the token expires it reads `SIGN IN` in red; click it to sign in again in the browser.
- **Calendar**: click the date to open the month with ISO week numbers and a dot on days with meetings, and the agenda of the selected day. Past meetings read `DONE`; the next meeting in the next 24 hours is highlighted with `JOIN` (or `OPEN` when it has no Meet link), which opens it in the browser. Click outside it or press Escape to close it.
- **Alerts**: 5 minutes before a meeting, a normal notification says `Starts in 5 minutes`. When it starts, an alert card opens under the clock with `JOIN` (or `OPEN`) and `DISMISS`; it stays until you use one of them and rings every 5 seconds while any card is open.

To try the alert card without a real meeting, trigger the `zehll:meetingTest` global shortcut, for example with a keybind:

```
bind = SUPER SHIFT, M, global, zehll:meetingTest
```

The calendar and the alert cards use the layer namespaces `zehll-calendar`, `zehll-calendar-catcher` and `zehll-meeting-alerts` for layer rules.

## Requirements

- `python3`
- a Google OAuth client in `~/.config/calendarz/credentials.json`
- `pw-play` (PipeWire) for the alert sound
- `xdg-open`
