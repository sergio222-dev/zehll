use std::collections::HashMap;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::dbus;

const SERVICE: &str = "org.freedesktop.NetworkManager";
const PATH: &str = "/org/freedesktop/NetworkManager";
const ACTIVE_CONNECTION: &str = "org.freedesktop.NetworkManager.Connection.Active";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const SETTINGS_CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const DEVICE_TYPE_WIFI: u32 = 2;
// NM_STATE_CONNECTED_LOCAL: at least one connection is up.
const CONNECTED_LOCAL: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wired,
    Wireless,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub wifi_enabled: bool,
    pub connected: bool,
    pub kind: Kind,
    pub name: String,
}

impl Network {
    /// Short status for the quick settings tile.
    pub fn label(&self) -> String {
        match (self.connected, self.kind) {
            (false, _) => String::from("DISCONNECTED"),
            (true, Kind::Wired) => String::from("WIRED"),
            (true, _) => self.name.to_uppercase(),
        }
    }
}

/// A Wi-Fi network as listed in the sub-panel: the strongest access point of each SSID.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessPoint {
    pub path: OwnedObjectPath,
    pub ssid: String,
    /// Signal quality, 0 to 100.
    pub strength: u8,
    pub frequency: u32,
    pub secure: bool,
    pub active: bool,
    /// A saved connection profile exists for this SSID.
    pub known: bool,
}

impl AccessPoint {
    /// Band label from the frequency in MHz.
    pub fn band(&self) -> &'static str {
        match self.frequency {
            ..3000 => "2.4G",
            3000..5925 => "5G",
            _ => "6G",
        }
    }

    /// Signal bars to light, 1 to 4.
    pub fn bars(&self) -> u8 {
        match self.strength {
            76.. => 4,
            51..=75 => 3,
            26..=50 => 2,
            _ => 1,
        }
    }
}

/// Keeps the strongest access point per SSID, drops hidden ones, and sorts the active network
/// first, then by signal.
fn merge(access_points: Vec<AccessPoint>) -> Vec<AccessPoint> {
    let mut networks: Vec<AccessPoint> = Vec::new();
    for access_point in access_points.into_iter().filter(|access_point| !access_point.ssid.is_empty()) {
        match networks.iter_mut().find(|network| network.ssid == access_point.ssid) {
            Some(network) => {
                let active = network.active || access_point.active;
                if access_point.strength > network.strength {
                    *network = access_point;
                }
                network.active = active;
            }
            None => networks.push(access_point),
        }
    }
    networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.strength.cmp(&a.strength)));
    networks
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wifi {
    pub device: OwnedObjectPath,
    pub networks: Vec<AccessPoint>,
    /// A scan requested when the sub-panel opened has not finished yet.
    pub scanning: bool,
}

// NetworkManager may refuse a scan without saying so later; stop showing it as scanning after this.
const SCAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Streams nearby Wi-Fi networks while the Wi-Fi sub-panel is open, after asking for a fresh scan.
pub fn wifi_events() -> impl Stream<Item = Wifi> {
    stream::channel(4, async |mut output| {
        let connection = match Connection::system().await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!("zehll: no system bus: {err}");
                return;
            }
        };
        let mut signals = match dbus::signals(&connection, SERVICE).await {
            Ok(signals) => signals,
            Err(err) => {
                eprintln!("zehll: cannot watch NetworkManager: {err}");
                return;
            }
        };
        // `LastScan` changes when a scan finishes; remember it to know when the requested one is done.
        let mut scan_since = None;
        if let Ok(Some(device)) = wifi_device(&connection).await {
            let scan = async {
                let wireless = dbus::proxy(&connection, SERVICE, device.as_str(), WIRELESS).await?;
                let last_scan: i64 = wireless.get_property("LastScan").await?;
                let options: HashMap<&str, Value<'_>> = HashMap::new();
                wireless.call_method("RequestScan", &(options,)).await?;
                zbus::Result::Ok(last_scan)
            };
            // NetworkManager refuses scans that come too soon after the last one; the cached list is fine then.
            scan_since = scan.await.ok().map(|last_scan| (last_scan, std::time::Instant::now()));
        }
        let mut last = None;
        loop {
            let wifi = read_wifi(&connection, scan_since).await.ok().flatten();
            if wifi != last {
                last = wifi.clone();
                if let Some(wifi) = wifi
                    && output.send(wifi).await.is_err()
                {
                    return;
                }
            }
            if !dbus::next_change(&mut signals).await {
                return;
            }
        }
    })
}

/// Connects to `ssid`: activates its saved profile, or creates one (with `password` for secured networks).
pub async fn connect(device: OwnedObjectPath, access_point: OwnedObjectPath, ssid: String, password: Option<String>) {
    let result = async {
        let connection = Connection::system().await?;
        let manager = dbus::proxy(&connection, SERVICE, PATH, SERVICE).await?;
        let root = ObjectPath::try_from("/")?;
        if let Some(profile) = saved_profiles(&connection).await?.remove(&ssid) {
            manager.call_method("ActivateConnection", &(profile, &device, root)).await?;
            return zbus::Result::Ok(());
        }
        let mut settings: HashMap<&str, HashMap<&str, Value<'_>>> = HashMap::new();
        if let Some(password) = &password {
            settings.insert(
                "802-11-wireless-security",
                HashMap::from([("key-mgmt", Value::from("wpa-psk")), ("psk", Value::from(password.as_str()))]),
            );
        }
        manager.call_method("AddAndActivateConnection", &(settings, &device, &access_point)).await?;
        Ok(())
    };
    if let Err(err) = result.await {
        eprintln!("zehll: cannot connect to {ssid}: {err}");
    }
}

async fn wifi_device(connection: &Connection) -> zbus::Result<Option<OwnedObjectPath>> {
    let manager = dbus::proxy(connection, SERVICE, PATH, SERVICE).await?;
    let devices: Vec<OwnedObjectPath> = manager.call("GetDevices", &()).await?;
    for device in devices {
        let kind: u32 = dbus::proxy(connection, SERVICE, device.as_str(), DEVICE).await?.get_property("DeviceType").await?;
        if kind == DEVICE_TYPE_WIFI {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

async fn read_wifi(connection: &Connection, scan_since: Option<(i64, std::time::Instant)>) -> zbus::Result<Option<Wifi>> {
    let Some(device) = wifi_device(connection).await? else {
        return Ok(None);
    };
    let wireless = dbus::proxy(connection, SERVICE, device.as_str(), WIRELESS).await?;
    let active: OwnedObjectPath = wireless.get_property("ActiveAccessPoint").await?;
    let last_scan: i64 = wireless.get_property("LastScan").await?;
    let scanning = scan_since.is_some_and(|(before, requested)| last_scan == before && requested.elapsed() < SCAN_TIMEOUT);
    let paths: Vec<OwnedObjectPath> = wireless.call("GetAllAccessPoints", &()).await?;
    let known = saved_profiles(connection).await?;

    let mut access_points = Vec::new();
    for path in paths {
        let properties = get_all(connection, path.as_str(), ACCESS_POINT).await?;
        let number = |key: &str| properties.get(key).and_then(|value| u32::try_from(value).ok()).unwrap_or(0);
        let ssid = properties.get("Ssid").and_then(|value| Vec::<u8>::try_from(value.try_clone().ok()?).ok()).unwrap_or_default();
        let ssid = String::from_utf8_lossy(&ssid).into_owned();
        access_points.push(AccessPoint {
            active: path == active,
            known: known.contains_key(&ssid),
            ssid,
            strength: properties.get("Strength").and_then(|value| u8::try_from(value).ok()).unwrap_or(0),
            frequency: number("Frequency"),
            // The privacy flag or any WPA/RSN flag means the network needs credentials.
            secure: number("Flags") & 1 != 0 || number("WpaFlags") != 0 || number("RsnFlags") != 0,
            path,
        });
    }
    Ok(Some(Wifi { device, networks: merge(access_points), scanning }))
}

/// Saved Wi-Fi connection profiles by SSID.
async fn saved_profiles(connection: &Connection) -> zbus::Result<HashMap<String, OwnedObjectPath>> {
    let settings = dbus::proxy(connection, SERVICE, SETTINGS_PATH, SETTINGS).await?;
    let paths: Vec<OwnedObjectPath> = settings.call("ListConnections", &()).await?;
    let mut profiles = HashMap::new();
    for path in paths {
        let profile = dbus::proxy(connection, SERVICE, path.as_str(), SETTINGS_CONNECTION).await?;
        let config: HashMap<String, HashMap<String, OwnedValue>> = profile.call("GetSettings", &()).await?;
        let ssid = config.get("802-11-wireless").and_then(|wireless| wireless.get("ssid")).and_then(|ssid| Vec::<u8>::try_from(ssid.try_clone().ok()?).ok());
        if let Some(ssid) = ssid {
            profiles.insert(String::from_utf8_lossy(&ssid).into_owned(), path);
        }
    }
    Ok(profiles)
}

async fn get_all(connection: &Connection, path: &str, interface: &str) -> zbus::Result<HashMap<String, OwnedValue>> {
    let properties = dbus::proxy(connection, SERVICE, path, "org.freedesktop.DBus.Properties").await?;
    properties.call("GetAll", &(interface,)).await
}

/// Streams the NetworkManager state, reading it again whenever NetworkManager signals a change.
pub fn events() -> impl Stream<Item = Network> {
    stream::channel(4, async |mut output| {
        let connection = match Connection::system().await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!("zehll: no system bus: {err}");
                return;
            }
        };
        let mut signals = match dbus::signals(&connection, SERVICE).await {
            Ok(signals) => signals,
            Err(err) => {
                eprintln!("zehll: cannot watch NetworkManager: {err}");
                return;
            }
        };
        let mut last = None;
        loop {
            let network = read(&connection).await.ok();
            if network != last {
                last = network.clone();
                if let Some(network) = network
                    && output.send(network).await.is_err()
                {
                    return;
                }
            }
            if !dbus::next_change(&mut signals).await {
                return;
            }
        }
    })
}

/// Turns the Wi-Fi radio on or off, like ii's Internet toggle.
pub async fn set_wifi(enabled: bool) {
    let result = async {
        let connection = Connection::system().await?;
        dbus::proxy(&connection, SERVICE, PATH, SERVICE).await?.set_property("WirelessEnabled", enabled).await?;
        zbus::Result::Ok(())
    };
    if let Err(err) = result.await {
        eprintln!("zehll: cannot toggle Wi-Fi: {err}");
    }
}

async fn read(connection: &Connection) -> zbus::Result<Network> {
    let manager = dbus::proxy(connection, SERVICE, PATH, SERVICE).await?;
    let wifi_enabled: bool = manager.get_property("WirelessEnabled").await?;
    let state: u32 = manager.get_property("State").await?;
    let primary: OwnedObjectPath = manager.get_property("PrimaryConnection").await?;

    let (kind, name) = if primary.as_str() == "/" {
        (Kind::Other, String::new())
    } else {
        let active = dbus::proxy(connection, SERVICE, primary.as_str(), ACTIVE_CONNECTION).await?;
        let kind: String = active.get_property("Type").await?;
        let kind = match kind.as_str() {
            "802-3-ethernet" => Kind::Wired,
            "802-11-wireless" => Kind::Wireless,
            _ => Kind::Other,
        };
        (kind, active.get_property("Id").await?)
    };
    Ok(Network { wifi_enabled, connected: state >= CONNECTED_LOCAL, kind, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(connected: bool, kind: Kind, name: &str) -> Network {
        Network { wifi_enabled: true, connected, kind, name: name.to_owned() }
    }

    fn access_point(ssid: &str, strength: u8, frequency: u32, active: bool) -> AccessPoint {
        AccessPoint {
            path: OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap(),
            ssid: ssid.to_owned(),
            strength,
            frequency,
            secure: true,
            active,
            known: false,
        }
    }

    #[test]
    fn bands_and_bars() {
        assert_eq!(access_point("a", 90, 2412, false).band(), "2.4G");
        assert_eq!(access_point("a", 90, 5180, false).band(), "5G");
        assert_eq!(access_point("a", 90, 5955, false).band(), "6G");
        let bars: Vec<u8> = [90, 60, 30, 5].iter().map(|strength| access_point("a", *strength, 2412, false).bars()).collect();
        assert_eq!(bars, [4, 3, 2, 1]);
    }

    #[test]
    fn merges_access_points() {
        let merged = merge(vec![
            access_point("Cafe", 40, 2412, false),
            access_point("Home", 50, 2412, false),
            access_point("Cafe", 70, 5180, false),
            access_point("", 99, 2412, false),
            access_point("Office", 30, 2412, true),
        ]);
        let summary: Vec<(&str, u8)> = merged.iter().map(|ap| (ap.ssid.as_str(), ap.strength)).collect();
        assert_eq!(summary, [("Office", 30), ("Cafe", 70), ("Home", 50)]);
    }

    #[test]
    fn labels() {
        assert_eq!(network(true, Kind::Wired, "Wired connection 1").label(), "WIRED");
        assert_eq!(network(true, Kind::Wireless, "Home 5G").label(), "HOME 5G");
        assert_eq!(network(true, Kind::Other, "tun0").label(), "TUN0");
        assert_eq!(network(false, Kind::Other, "").label(), "DISCONNECTED");
    }
}
