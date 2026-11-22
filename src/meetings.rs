//! Google Calendar meetings fetched by `scripts/gcal.py`, and the pure rules the bar
//! pill, its tooltip and the calendar popover are drawn from.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Days, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone};
use iced::Color;
use tokio::process::Command;

use crate::calendar;

/// A timed meeting, with its times already converted to the local time zone.
#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub calendar: String,
    /// Google Meet link when the event has one, otherwise the event page.
    pub link: String,
}

/// Outcome of the last `gcal.py events` run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// The stored token expired: the user must sign in again.
    Reauth,
    Error,
}

/// Colour class of the pill dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urgency {
    /// More than 15 minutes away.
    Far,
    /// 15 to 5 minutes away.
    Soon,
    /// 5 minutes or less.
    Imminent,
    /// Token expired.
    Expired,
}

impl Urgency {
    /// Dot colour from the Calendar artboard: grey, green, yellow, red.
    pub fn color(self) -> Color {
        match self {
            Urgency::Far => Color::from_rgb8(0x5d, 0x6b, 0x8f),
            Urgency::Soon => Color::from_rgb8(0x5f, 0xe3, 0x9a),
            Urgency::Imminent => Color::from_rgb8(0xff, 0xd2, 0x3f),
            Urgency::Expired => Color::from_rgb8(0xff, 0x4d, 0x5e),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pill {
    pub label: String,
    pub urgency: Urgency,
}

/// Row state in the agenda of the selected day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgendaState {
    Done,
    Now,
    /// The next meeting overall: highlighted, with `JOIN`.
    Next,
    Later,
}

pub fn status_from_exit(code: Option<i32>) -> Status {
    match code {
        Some(0) => Status::Ok,
        Some(2) => Status::Reauth,
        _ => Status::Error,
    }
}

/// Parses the `{"events": [...]}` object printed by `gcal.py events`, converting times into `tz`.
/// Malformed entries are skipped; malformed JSON yields no meetings.
pub fn parse_events<Tz: TimeZone>(json: &str, tz: &Tz) -> Vec<Meeting> {
    let output: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let items = output.get("events").and_then(|events| events.as_array()).map(Vec::as_slice).unwrap_or_default();
    let time = |item: &serde_json::Value, key: &str| {
        let parsed = DateTime::parse_from_rfc3339(item.get(key)?.as_str()?).ok()?;
        Some(parsed.with_timezone(tz).naive_local())
    };
    let text = |item: &serde_json::Value, key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    items
        .iter()
        .filter_map(|item| {
            Some(Meeting {
                id: item.get("id")?.as_str()?.to_string(),
                title: text(item, "title"),
                start: time(item, "start")?,
                end: time(item, "end")?,
                calendar: text(item, "calendar"),
                link: text(item, "link"),
            })
        })
        .collect()
}

/// First meeting starting at or after `now` and within the next 24 hours.
pub fn next_meeting(meetings: &[Meeting], now: NaiveDateTime) -> Option<&Meeting> {
    meetings
        .iter()
        .filter(|m| m.start >= now && m.start - now <= TimeDelta::days(1))
        .min_by_key(|m| m.start)
}

pub fn urgency(meeting: &Meeting, now: NaiveDateTime) -> Urgency {
    let left = meeting.start - now;
    if left > TimeDelta::minutes(15) {
        Urgency::Far
    } else if left > TimeDelta::minutes(5) {
        Urgency::Soon
    } else {
        Urgency::Imminent
    }
}

/// `IN 4 MIN`, `IN 5H 34M`, `IN 2D 3H`; minutes round up so a pending meeting never reads 0.
pub fn countdown(meeting: &Meeting, now: NaiveDateTime) -> String {
    let minutes = ((meeting.start - now).num_seconds().max(0) + 59) / 60;
    match minutes {
        m if m < 60 => format!("IN {m} MIN"),
        m if m < 24 * 60 => format!("IN {}H {}M", m / 60, m % 60),
        m => format!("IN {}D {}H", m / (24 * 60), m % (24 * 60) / 60),
    }
}

/// The bar pill, or `None` when there is nothing to show.
pub fn pill(status: Status, meetings: &[Meeting], now: NaiveDateTime) -> Option<Pill> {
    match status {
        Status::Reauth => Some(Pill { label: "SIGN IN".into(), urgency: Urgency::Expired }),
        Status::Error => None,
        Status::Ok => next_meeting(meetings, now)
            .map(|m| Pill { label: m.start.format("%H:%M").to_string(), urgency: urgency(m, now) }),
    }
}

/// `10:00 – 10:45`
pub fn time_range(meeting: &Meeting) -> String {
    format!("{} – {}", meeting.start.format("%H:%M"), meeting.end.format("%H:%M"))
}

/// Meetings starting on `date`, in start order.
pub fn meetings_on(meetings: &[Meeting], date: NaiveDate) -> Vec<&Meeting> {
    let mut on_day: Vec<_> = meetings.iter().filter(|m| m.start.date() == date).collect();
    on_day.sort_by_key(|m| m.start);
    on_day
}

pub fn has_meetings(meetings: &[Meeting], date: NaiveDate) -> bool {
    meetings.iter().any(|m| m.start.date() == date)
}

pub fn agenda_state(meeting: &Meeting, meetings: &[Meeting], now: NaiveDateTime) -> AgendaState {
    if meeting.end <= now {
        AgendaState::Done
    } else if meeting.start < now {
        AgendaState::Now
    } else if next_meeting(meetings, now) == Some(meeting) {
        AgendaState::Next
    } else {
        AgendaState::Later
    }
}

/// Badge text of an agenda row: `DONE`, `NOW` or the countdown.
pub fn agenda_label(meeting: &Meeting, meetings: &[Meeting], now: NaiveDateTime) -> String {
    match agenda_state(meeting, meetings, now) {
        AgendaState::Done => "DONE".into(),
        AgendaState::Now => "NOW".into(),
        AgendaState::Next | AgendaState::Later => countdown(meeting, now),
    }
}

/// Calendar name, plus `Google Meet` when the meeting has a Meet link: `Work · Google Meet`.
pub fn source(meeting: &Meeting) -> String {
    match has_meet_link(meeting) {
        true => format!("{} · Google Meet", meeting.calendar),
        false => meeting.calendar.clone(),
    }
}

/// Second line of an alert card: `10:00 – 10:45 · Work · Google Meet`.
pub fn alert_meta(meeting: &Meeting) -> String {
    format!("{} – {} · {}", meeting.start.format("%H:%M"), meeting.end.format("%H:%M"), source(meeting))
}

/// Status of an alert card: `STARTING NOW`, `STARTED 1 MIN AGO`, `STARTED 4 MIN AGO`.
pub fn started_label(meeting: &Meeting, now: NaiveDateTime) -> String {
    match (now - meeting.start).num_minutes().max(0) {
        0 => "STARTING NOW".into(),
        minutes => format!("STARTED {minutes} MIN AGO"),
    }
}

/// Agenda header: `NO MEETINGS`, `1 MEETING`, `3 MEETINGS`.
pub fn meeting_count(count: usize) -> String {
    match count {
        0 => "NO MEETINGS".into(),
        1 => "1 MEETING".into(),
        count => format!("{count} MEETINGS"),
    }
}

pub fn has_meet_link(meeting: &Meeting) -> bool {
    meeting.link.starts_with("https://meet.google.com/")
}

/// Dates to fetch, end exclusive: the month grid of `month` plus today and tomorrow, which
/// the pill and the alerts need whatever month the calendar shows.
pub fn fetch_range(month: NaiveDate, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = calendar::grid_start(month);
    let end = start + Days::new(calendar::GRID_DAYS);
    (start.min(today), end.max(today + Days::new(2)))
}

const GCAL: &str = include_str!("../scripts/gcal.py");

/// Runs `gcal.py events` for the dates `from..to` in the local time zone. The script is
/// embedded in the binary, so it needs only `python3` and the calendarz login.
pub async fn fetch(from: NaiveDate, to: NaiveDate) -> (Status, Vec<Meeting>) {
    let bound = |date: NaiveDate| {
        Local.from_local_datetime(&date.and_time(NaiveTime::MIN)).earliest().map(|time| time.to_rfc3339())
    };
    let (Some(from), Some(to)) = (bound(from), bound(to)) else {
        return (Status::Error, Vec::new());
    };
    let output = match Command::new("python3").args(["-c", GCAL, "events", &from, &to]).output().await {
        Ok(output) => output,
        Err(err) => {
            eprintln!("zehll: cannot run python3 for the calendar: {err}");
            return (Status::Error, Vec::new());
        }
    };
    let status = status_from_exit(output.status.code());
    if status == Status::Error {
        eprintln!("zehll: calendar fetch failed: {}", String::from_utf8_lossy(&output.stdout).trim());
    }
    (status, parse_events(&String::from_utf8_lossy(&output.stdout), &Local))
}

/// Time allowed to finish the browser sign-in before `gcal.py auth` is stopped.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Runs `gcal.py auth`, which opens the browser and waits for Google's redirect.
pub async fn sign_in() {
    let child = Command::new("python3").args(["-c", GCAL, "auth"]).kill_on_drop(true).status();
    match tokio::time::timeout(SIGN_IN_TIMEOUT, child).await {
        Ok(Ok(status)) if status.success() => {}
        Ok(Ok(status)) => eprintln!("zehll: calendar sign-in failed: {status}"),
        Ok(Err(err)) => eprintln!("zehll: cannot run python3 for the calendar sign-in: {err}"),
        Err(_) => eprintln!("zehll: calendar sign-in timed out"),
    }
}

/// Fake meeting starting at `now`, for trying the alert card without a real one.
pub fn test_meeting(now: NaiveDateTime) -> Meeting {
    Meeting {
        // Nanoseconds keep two quick presses apart, so dismissing one card leaves the other.
        id: format!("test-{}", now.and_utc().timestamp_nanos_opt().unwrap_or_default()),
        title: "Test meeting".into(),
        start: now,
        end: now + TimeDelta::minutes(30),
        calendar: "Test".into(),
        link: "https://meet.google.com/".into(),
    }
}

/// Minutes before the start at which the heads-up notification fires.
pub const LEAD_MINUTES: i64 = 5;
/// How long after the start a missed alert card still opens (e.g. after a late fetch).
pub const LATE_MINUTES: i64 = 5;

/// Summary and body of the heads-up notification: the title and `Starts in 5 minutes`.
pub fn warning_text(meeting: &Meeting) -> (String, String) {
    (meeting.title.clone(), format!("Starts in {LEAD_MINUTES} minutes"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertKind {
    /// `Starts in 5 minutes` notification, no sound.
    Warning,
    /// Persistent ringing card.
    Start,
}

/// Alerts that became due at `now`, each meeting at most once per kind.
/// `fired` remembers what already fired; a rescheduled meeting fires again.
pub fn due_alerts<'a>(
    meetings: &'a [Meeting],
    now: NaiveDateTime,
    fired: &mut HashSet<String>,
) -> Vec<(AlertKind, &'a Meeting)> {
    meetings
        .iter()
        .filter_map(|meeting| {
            let left = meeting.start - now;
            let kind = if left > TimeDelta::zero() && left <= TimeDelta::minutes(LEAD_MINUTES) {
                AlertKind::Warning
            } else if left <= TimeDelta::zero() && left >= -TimeDelta::minutes(LATE_MINUTES) {
                AlertKind::Start
            } else {
                return None;
            };
            fired.insert(format!("{kind:?}|{}|{}", meeting.id, meeting.start)).then_some((kind, meeting))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn meeting(id: &str, start: NaiveDateTime, end: NaiveDateTime) -> Meeting {
        Meeting {
            id: id.into(),
            title: id.into(),
            start,
            end,
            calendar: "Work".into(),
            link: "https://meet.google.com/abc-defg-hij".into(),
        }
    }

    fn day() -> Vec<Meeting> {
        vec![
            meeting("standup", at(21, 8, 30), at(21, 9, 0)),
            meeting("review", at(21, 10, 0), at(21, 10, 45)),
            meeting("sync", at(21, 15, 30), at(21, 16, 0)),
        ]
    }

    #[test]
    fn exit_codes_map_to_status() {
        assert_eq!(status_from_exit(Some(0)), Status::Ok);
        assert_eq!(status_from_exit(Some(2)), Status::Reauth);
        assert_eq!(status_from_exit(Some(1)), Status::Error);
        assert_eq!(status_from_exit(None), Status::Error);
    }

    #[test]
    fn parses_gcal_output_into_the_given_time_zone() {
        let json = r#"{"events": [
            {"id": "a", "title": "Design review", "start": "2026-11-21T13:00:00Z",
             "end": "2026-11-21T13:45:00Z", "calendar": "Work", "link": "https://meet.google.com/x"},
            {"id": "broken", "title": "No end", "start": "2026-11-21T14:00:00Z"}
        ]}"#;
        let buenos_aires = FixedOffset::west_opt(3 * 3600).unwrap();
        let meetings = parse_events(json, &buenos_aires);
        assert_eq!(meetings.len(), 1);
        let m = &meetings[0];
        assert_eq!((m.id.as_str(), m.title.as_str(), m.calendar.as_str()), ("a", "Design review", "Work"));
        assert_eq!((m.start, m.end), (at(21, 10, 0), at(21, 10, 45)));
        assert_eq!(m.link, "https://meet.google.com/x");
        assert!(parse_events("not json", &buenos_aires).is_empty());
        assert!(parse_events(r#"{"error": "reauth"}"#, &buenos_aires).is_empty());
    }

    #[test]
    fn fetch_range_covers_the_month_grid_and_the_next_two_days() {
        let date = |month, day| NaiveDate::from_ymd_opt(2026, month, day).unwrap();
        assert_eq!(fetch_range(date(11, 1), date(11, 21)), (date(10, 26), date(12, 7)));
        // Browsing a past month still fetches what the pill and the alerts need.
        assert_eq!(fetch_range(date(9, 1), date(11, 21)), (date(8, 31), date(11, 23)));
        assert_eq!(fetch_range(date(11, 1), date(12, 6)), (date(10, 26), date(12, 8)));
    }

    #[test]
    fn next_meeting_is_the_first_pending_one_within_a_day() {
        let meetings = day();
        assert_eq!(next_meeting(&meetings, at(21, 9, 56)).map(|m| m.id.as_str()), Some("review"));
        assert_eq!(next_meeting(&meetings, at(21, 10, 0)).map(|m| m.id.as_str()), Some("review"));
        assert_eq!(next_meeting(&meetings, at(21, 10, 1)).map(|m| m.id.as_str()), Some("sync"));
        assert_eq!(next_meeting(&meetings, at(20, 8, 29)), None);
        assert_eq!(next_meeting(&meetings, at(21, 16, 0)), None);
    }

    #[test]
    fn urgency_thresholds() {
        let m = meeting("review", at(21, 10, 0), at(21, 10, 45));
        assert_eq!(urgency(&m, at(21, 9, 44)), Urgency::Far);
        assert_eq!(urgency(&m, at(21, 9, 45)), Urgency::Soon);
        assert_eq!(urgency(&m, at(21, 9, 54)), Urgency::Soon);
        assert_eq!(urgency(&m, at(21, 9, 55)), Urgency::Imminent);
        assert_eq!(urgency(&m, at(21, 10, 0)), Urgency::Imminent);
    }

    #[test]
    fn countdown_formats() {
        let m = meeting("sync", at(21, 15, 30), at(21, 16, 0));
        assert_eq!(countdown(&m, at(21, 15, 26)), "IN 4 MIN");
        assert_eq!(countdown(&m, at(21, 15, 25) + TimeDelta::seconds(30)), "IN 5 MIN");
        assert_eq!(countdown(&m, at(21, 15, 30)), "IN 0 MIN");
        assert_eq!(countdown(&m, at(21, 14, 30)), "IN 1H 0M");
        assert_eq!(countdown(&m, at(21, 9, 56)), "IN 5H 34M");
        assert_eq!(countdown(&m, at(19, 12, 30)), "IN 2D 3H");
    }

    #[test]
    fn pill_states() {
        let meetings = day();
        let pill_at = |status, now| pill(status, &meetings, now);
        assert_eq!(
            pill_at(Status::Ok, at(21, 9, 56)),
            Some(Pill { label: "10:00".into(), urgency: Urgency::Imminent })
        );
        assert_eq!(
            pill_at(Status::Ok, at(21, 11, 0)),
            Some(Pill { label: "15:30".into(), urgency: Urgency::Far })
        );
        assert_eq!(
            pill_at(Status::Reauth, at(21, 11, 0)),
            Some(Pill { label: "SIGN IN".into(), urgency: Urgency::Expired })
        );
        assert_eq!(pill_at(Status::Ok, at(21, 17, 0)), None);
        assert_eq!(pill_at(Status::Error, at(21, 9, 56)), None);
    }

    #[test]
    fn urgency_colors_follow_the_artboard() {
        let rgb = |u: Urgency| u.color().into_rgba8();
        assert_eq!(rgb(Urgency::Far), [0x5d, 0x6b, 0x8f, 0xff]);
        assert_eq!(rgb(Urgency::Soon), [0x5f, 0xe3, 0x9a, 0xff]);
        assert_eq!(rgb(Urgency::Imminent), [0xff, 0xd2, 0x3f, 0xff]);
        assert_eq!(rgb(Urgency::Expired), [0xff, 0x4d, 0x5e, 0xff]);
    }

    #[test]
    fn tooltip_text() {
        let m = meeting("review", at(21, 10, 0), at(21, 10, 45));
        assert_eq!(time_range(&m), "10:00 – 10:45");
    }

    #[test]
    fn meetings_by_day() {
        let mut meetings = day();
        meetings.push(meeting("late", at(22, 0, 0), at(22, 1, 0)));
        meetings.push(meeting("early", at(21, 7, 0), at(21, 7, 30)));
        let ids: Vec<_> = meetings_on(&meetings, at(21, 0, 0).date()).iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["early", "standup", "review", "sync"]);
        assert!(has_meetings(&meetings, at(22, 0, 0).date()));
        assert!(!has_meetings(&meetings, at(23, 0, 0).date()));
    }

    #[test]
    fn alert_card_texts() {
        let mut m = meeting("review", at(21, 10, 0), at(21, 10, 45));
        assert_eq!(source(&m), "Work · Google Meet");
        assert_eq!(alert_meta(&m), "10:00 – 10:45 · Work · Google Meet");
        m.link = "https://www.google.com/calendar/event?eid=abc".into();
        assert_eq!(source(&m), "Work");
        assert_eq!(alert_meta(&m), "10:00 – 10:45 · Work");

        assert_eq!(started_label(&m, at(21, 10, 0)), "STARTING NOW");
        assert_eq!(started_label(&m, at(21, 10, 0) + TimeDelta::seconds(59)), "STARTING NOW");
        assert_eq!(started_label(&m, at(21, 10, 1)), "STARTED 1 MIN AGO");
        assert_eq!(started_label(&m, at(21, 10, 4) + TimeDelta::seconds(30)), "STARTED 4 MIN AGO");
    }

    #[test]
    fn meeting_count_header() {
        assert_eq!(meeting_count(0), "NO MEETINGS");
        assert_eq!(meeting_count(1), "1 MEETING");
        assert_eq!(meeting_count(3), "3 MEETINGS");
    }

    #[test]
    fn agenda_rows() {
        let meetings = day();
        let now = at(21, 9, 56);
        let rows: Vec<_> = meetings
            .iter()
            .map(|m| (agenda_state(m, &meetings, now), agenda_label(m, &meetings, now)))
            .collect();
        assert_eq!(
            rows,
            [
                (AgendaState::Done, "DONE".to_string()),
                (AgendaState::Next, "IN 4 MIN".to_string()),
                (AgendaState::Later, "IN 5H 34M".to_string()),
            ]
        );
        let during = at(21, 10, 20);
        assert_eq!(agenda_state(&meetings[1], &meetings, during), AgendaState::Now);
        assert_eq!(agenda_label(&meetings[1], &meetings, during), "NOW");
        assert_eq!(agenda_state(&meetings[2], &meetings, during), AgendaState::Next);
    }

    #[test]
    fn meet_link_decides_join_or_open() {
        let mut m = meeting("review", at(21, 10, 0), at(21, 10, 45));
        assert!(has_meet_link(&m));
        m.link = "https://www.google.com/calendar/event?eid=abc".into();
        assert!(!has_meet_link(&m));
    }

    fn due(meetings: &[Meeting], now: NaiveDateTime, fired: &mut HashSet<String>) -> Vec<(AlertKind, String)> {
        due_alerts(meetings, now, fired).into_iter().map(|(kind, m)| (kind, m.id.clone())).collect()
    }

    #[test]
    fn test_meeting_starts_now_with_a_meet_link() {
        let m = test_meeting(at(21, 10, 0));
        assert_eq!((m.title.as_str(), m.start, m.end), ("Test meeting", at(21, 10, 0), at(21, 10, 30)));
        assert!(has_meet_link(&m));
        assert_eq!(started_label(&m, at(21, 10, 0)), "STARTING NOW");
        // Each press is a separate card, even within the same minute.
        assert_ne!(m.id, test_meeting(at(21, 10, 0) + TimeDelta::seconds(1)).id);
    }

    #[test]
    fn warning_notification_text() {
        let m = meeting("Design review", at(21, 10, 0), at(21, 10, 45));
        assert_eq!(warning_text(&m), ("Design review".to_string(), "Starts in 5 minutes".to_string()));
    }

    #[test]
    fn warning_fires_once_inside_the_lead_window() {
        let meetings = day();
        let mut fired = HashSet::new();
        assert!(due(&meetings, at(21, 9, 54), &mut fired).is_empty());
        assert_eq!(due(&meetings, at(21, 9, 55), &mut fired), [(AlertKind::Warning, "review".to_string())]);
        assert!(due(&meetings, at(21, 9, 57), &mut fired).is_empty());
    }

    #[test]
    fn start_card_fires_once_at_start_and_while_late() {
        let meetings = day();
        let mut fired = HashSet::new();
        due(&meetings, at(21, 9, 56), &mut fired);
        assert_eq!(due(&meetings, at(21, 10, 0), &mut fired), [(AlertKind::Start, "review".to_string())]);
        assert!(due(&meetings, at(21, 10, 1), &mut fired).is_empty());

        let mut late = HashSet::new();
        assert_eq!(due(&meetings, at(21, 10, 5), &mut late), [(AlertKind::Start, "review".to_string())]);
        let mut too_late = HashSet::new();
        assert!(due(&meetings, at(21, 10, 5) + TimeDelta::seconds(1), &mut too_late).is_empty());
    }

    #[test]
    fn overlapping_meetings_alert_together_and_reschedules_fire_again() {
        let mut meetings = day();
        meetings.push(meeting("one-on-one", at(21, 10, 0), at(21, 10, 30)));
        let mut fired = HashSet::new();
        assert_eq!(
            due(&meetings, at(21, 10, 0), &mut fired),
            [(AlertKind::Start, "review".to_string()), (AlertKind::Start, "one-on-one".to_string())]
        );

        meetings[1].start = at(21, 11, 0);
        assert_eq!(due(&meetings, at(21, 10, 56), &mut fired), [(AlertKind::Warning, "review".to_string())]);
    }
}
