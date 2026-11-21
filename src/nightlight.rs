use chrono::{NaiveTime, Timelike};
use tokio::process::Command;

/// Temperatures ii uses for night light on and off, so both shells agree on the state.
pub const NIGHT: u32 = 5000;
pub const DAY: u32 = 6000;
/// Automatic schedule, inclusive on both ends like ii's: (hour, minute).
const FROM: (u32, u32) = (19, 0);
const TO: (u32, u32) = (6, 30);

/// Whether the automatic schedule wants night light on at `time`.
pub fn scheduled(time: NaiveTime) -> bool {
    let minutes = |(hour, minute): (u32, u32)| hour * 60 + minute;
    let (now, from, to) = (minutes((time.hour(), time.minute())), minutes(FROM), minutes(TO));
    if from < to { (from..=to).contains(&now) } else { now >= from || now <= to }
}

/// Parses `hyprctl hyprsunset temperature` output, e.g. `5000`.
fn parse_temperature(output: &str) -> Option<u32> {
    output.trim().parse().ok()
}

pub fn is_on(temperature: u32) -> bool {
    temperature < DAY
}

/// Current hyprsunset temperature, or `None` when hyprsunset is not running.
pub async fn temperature() -> Option<u32> {
    let output = Command::new("hyprctl").args(["hyprsunset", "temperature"]).output().await.ok()?;
    parse_temperature(&String::from_utf8_lossy(&output.stdout))
}

/// Switches night light and returns the temperature hyprsunset reports afterwards.
pub async fn set(on: bool) -> Option<u32> {
    let temperature = if on { NIGHT } else { DAY };
    let status = Command::new("hyprctl").args(["hyprsunset", "temperature", &temperature.to_string()]).status().await;
    if let Err(err) = status {
        eprintln!("zehll: cannot set night light: {err}");
    }
    self::temperature().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_wraps_around_midnight() {
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        assert!(!scheduled(at(18, 59)));
        assert!(scheduled(at(19, 0)));
        assert!(scheduled(at(0, 0)));
        assert!(scheduled(at(6, 30)));
        assert!(!scheduled(at(6, 31)));
        assert!(!scheduled(at(12, 0)));
    }

    #[test]
    fn parses_temperature() {
        assert_eq!(parse_temperature("5000\n"), Some(5000));
        assert_eq!(parse_temperature("Couldn't connect to hyprsunset"), None);
    }

    #[test]
    fn night_light_is_on_below_day_temperature() {
        assert!(is_on(NIGHT));
        assert!(!is_on(DAY));
        assert!(!is_on(6500));
    }
}
