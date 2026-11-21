use std::collections::HashMap;

use tokio::process::Command;

/// A monitor whose brightness is set over DDC/CI.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub bus: String,
    pub percent: u32,
    /// Maximum raw value; `setvcp` takes raw values.
    max: u32,
}

/// Reads the brightness of the monitor named `output` (`DP-1`). DDC is slow (about 0.3 s per
/// call), so this only runs when the eye protection sub-panel opens.
pub async fn read(output: String) -> Option<Monitor> {
    let detect = ddcutil(&["detect", "--brief"]).await?;
    let bus = parse_detect(&detect).remove(&output)?;
    let (current, max) = parse_vcp(&ddcutil(&["-b", &bus, "getvcp", "10", "--brief"]).await?)?;
    Some(Monitor { bus, percent: current * 100 / max, max })
}

pub async fn set(monitor: Monitor, percent: u32) {
    let raw = percent * monitor.max / 100;
    if ddcutil(&["-b", &monitor.bus, "setvcp", "10", &raw.to_string()]).await.is_none() {
        eprintln!("zehll: cannot set brightness on i2c bus {}", monitor.bus);
    }
}

async fn ddcutil(args: &[&str]) -> Option<String> {
    let output = Command::new("ddcutil").args(args).output().await.ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// I2C bus of each monitor from `ddcutil detect --brief`, keyed by output name (`DP-1`).
fn parse_detect(output: &str) -> HashMap<String, String> {
    let mut buses = HashMap::new();
    let mut bus = None;
    for line in output.lines().map(str::trim) {
        if let Some(device) = line.strip_prefix("I2C bus:") {
            bus = device.trim().strip_prefix("/dev/i2c-").map(str::to_owned);
        } else if let Some(connector) = line.strip_prefix("DRM connector:") {
            // `card1-DP-1` names output `DP-1` on card 1.
            let name = connector.trim().split_once('-').map(|(_, name)| name.to_owned());
            if let (Some(name), Some(bus)) = (name, bus.take()) {
                buses.insert(name, bus);
            }
        }
    }
    buses
}

/// Current and maximum raw brightness from `ddcutil getvcp 10 --brief`: `VCP 10 C <current> <max>`.
fn parse_vcp(output: &str) -> Option<(u32, u32)> {
    let mut fields = output.split_whitespace().skip(3);
    let current: u32 = fields.next()?.parse().ok()?;
    let max: u32 = fields.next()?.parse().ok()?;
    (max > 0).then_some((current, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETECT: &str = "Display 1
   I2C bus:          /dev/i2c-3
   DRM connector:    card1-DP-1
   drm_connector_id: 833
   Monitor:          AUS:XG27UCS:T5LMTF045763

Display 2
   I2C bus:          /dev/i2c-4
   DRM connector:    card1-HDMI-A-2
   drm_connector_id: 837
   Monitor:          GSM:LG FULL HD:
";

    #[test]
    fn maps_outputs_to_buses() {
        let buses = parse_detect(DETECT);
        assert_eq!(buses.get("DP-1").map(String::as_str), Some("3"));
        assert_eq!(buses.get("HDMI-A-2").map(String::as_str), Some("4"));
        assert_eq!(buses.len(), 2);
    }

    #[test]
    fn reads_raw_brightness() {
        assert_eq!(parse_vcp("VCP 10 C 20 100\n"), Some((20, 100)));
        // Some monitors use a maximum other than 100.
        assert_eq!(parse_vcp("VCP 10 C 25 50"), Some((25, 50)));
        assert_eq!(parse_vcp("VCP 10 ERR"), None);
    }
}
