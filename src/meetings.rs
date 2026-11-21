//! Google Calendar meetings fetched by `scripts/gcal.py`, and the pure rules the bar
//! pill, its tooltip and the calendar popover are drawn from.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};

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

/// Parses the JSON array printed by `gcal.py events`, converting times into `tz`.
/// Malformed entries are skipped; malformed JSON yields no meetings.
pub fn parse_events<Tz: TimeZone>(json: &str, tz: &Tz) -> Vec<Meeting> {
    let items: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
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

/// `10:00 – 10:45 · IN 4 MIN`
pub fn tooltip(meeting: &Meeting, now: NaiveDateTime) -> String {
    format!("{} – {} · {}", meeting.start.format("%H:%M"), meeting.end.format("%H:%M"), countdown(meeting, now))
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

pub fn has_meet_link(meeting: &Meeting) -> bool {
    meeting.link.starts_with("https://meet.google.com/")
}

/// Minutes before the start at which the heads-up notification fires.
pub const LEAD_MINUTES: i64 = 5;
/// How long after the start a missed alert card still opens (e.g. after a late fetch).
pub const LATE_MINUTES: i64 = 5;

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
        let json = r#"[
            {"id": "a", "title": "Design review", "start": "2026-11-21T13:00:00Z",
             "end": "2026-11-21T13:45:00Z", "calendar": "Work", "link": "https://meet.google.com/x"},
            {"id": "broken", "title": "No end", "start": "2026-11-21T14:00:00Z"}
        ]"#;
        let buenos_aires = FixedOffset::west_opt(3 * 3600).unwrap();
        let meetings = parse_events(json, &buenos_aires);
        assert_eq!(meetings.len(), 1);
        let m = &meetings[0];
        assert_eq!((m.id.as_str(), m.title.as_str(), m.calendar.as_str()), ("a", "Design review", "Work"));
        assert_eq!((m.start, m.end), (at(21, 10, 0), at(21, 10, 45)));
        assert_eq!(m.link, "https://meet.google.com/x");
        assert!(parse_events("not json", &buenos_aires).is_empty());
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
    fn tooltip_text() {
        let m = meeting("review", at(21, 10, 0), at(21, 10, 45));
        assert_eq!(tooltip(&m, at(21, 9, 56)), "10:00 – 10:45 · IN 4 MIN");
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
