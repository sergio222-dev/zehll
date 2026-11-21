use tokio::process::Command;

/// Temperatures ii uses for night light on and off, so both shells agree on the state.
pub const NIGHT: u32 = 5000;
pub const DAY: u32 = 6000;

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
