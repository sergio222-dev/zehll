use std::collections::HashMap;
use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use zbus::Connection;
use zbus::fdo::ObjectManagerProxy;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::{bluetooth_agent, dbus};

const SERVICE: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";
/// How long a device whose bond went stale has to show up again (in pairing mode) to be re-paired.
const REDISCOVER: Duration = Duration::from_secs(30);

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct Bluetooth {
    pub adapter: OwnedObjectPath,
    pub powered: bool,
    /// Alias of a connected device.
    pub device: Option<String>,
    /// The adapter is searching for devices.
    pub discovering: bool,
    /// Every device BlueZ knows, like ii: connected, then paired, then the rest.
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub path: OwnedObjectPath,
    pub alias: String,
    /// BlueZ's icon name, such as `audio-headset`.
    pub kind: String,
    pub connected: bool,
    pub paired: bool,
    /// BlueZ connects trusted devices without asking ("always connect").
    pub trusted: bool,
    pub battery: Option<u8>,
}

fn devices(objects: &ManagedObjects) -> Vec<Device> {
    let mut devices: Vec<Device> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let device = interfaces.get(DEVICE)?;
            Some(Device {
                path: path.clone(),
                alias: string(device, "Alias").unwrap_or_default(),
                kind: string(device, "Icon").unwrap_or_default(),
                connected: flag(device, "Connected"),
                paired: flag(device, "Paired"),
                trusted: flag(device, "Trusted"),
                battery: interfaces.get(BATTERY).and_then(|battery| battery.get("Percentage")).and_then(|value| u8::try_from(value).ok()),
            })
        })
        .collect();
    devices.sort_by(|a, b| (b.connected, b.paired).cmp(&(a.connected, a.paired)).then_with(|| a.alias.cmp(&b.alias)));
    devices
}

fn flag(properties: &HashMap<String, OwnedValue>, key: &str) -> bool {
    properties.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false)
}

fn string(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    String::try_from(properties.get(key)?.try_clone().ok()?).ok()
}

#[derive(Debug, Clone)]
pub enum DeviceAction {
    Connect(OwnedObjectPath),
    Pair(OwnedObjectPath),
    Disconnect(OwnedObjectPath),
    Forget { adapter: OwnedObjectPath, device: OwnedObjectPath },
    Trust(OwnedObjectPath, bool),
}

pub async fn run(action: DeviceAction) {
    let result = async {
        let connection = Connection::system().await?;
        match &action {
            DeviceAction::Connect(path) => match dbus::proxy(&connection, SERVICE, path.as_str(), DEVICE).await?.call_method("Connect", &()).await {
                Err(zbus::Error::MethodError(_, Some(reason), _)) if is_key_missing(&reason) => {
                    eprintln!("zehll: Bluetooth {} lost its pairing key; pairing again", path.as_str());
                    repair(&connection, path).await
                }
                result => result.map(drop),
            },
            DeviceAction::Pair(path) => pair(&connection, path).await,
            DeviceAction::Disconnect(path) => dbus::proxy(&connection, SERVICE, path.as_str(), DEVICE).await?.call_method("Disconnect", &()).await.map(drop),
            DeviceAction::Forget { adapter, device } => {
                dbus::proxy(&connection, SERVICE, adapter.as_str(), ADAPTER).await?.call_method("RemoveDevice", &(device,)).await.map(drop)
            }
            DeviceAction::Trust(path, trusted) => {
                dbus::proxy(&connection, SERVICE, path.as_str(), DEVICE).await?.set_property("Trusted", *trusted).await.map_err(Into::into)
            }
        }
    };
    if let Err(err) = result.await {
        eprintln!("zehll: Bluetooth {action:?} failed: {err}");
    }
}

/// Like ii and blueman: pair, trust (so the device's profiles connect without asking), connect.
async fn pair(connection: &Connection, path: &OwnedObjectPath) -> zbus::Result<()> {
    let device = dbus::proxy(connection, SERVICE, path.as_str(), DEVICE).await?;
    let pairing = bluetooth_agent::pairing(path.as_str());
    device.call_method("Pair", &()).await?;
    device.set_property("Trusted", true).await?;
    let connected = device.call_method("Connect", &()).await.map(drop);
    drop(pairing);
    connected
}

/// The device paired with another host and dropped this computer's link key, so BlueZ's bond is
/// stale: forget it, wait for the device to show up again in a scan (it must be in pairing mode)
/// and pair it from scratch.
async fn repair(connection: &Connection, path: &OwnedObjectPath) -> zbus::Result<()> {
    let adapter = dbus::proxy(connection, SERVICE, adapter_of(path.as_str()), ADAPTER).await?;
    let mut signals = dbus::signals(connection, SERVICE).await?;
    adapter.call_method("RemoveDevice", &(path,)).await?;
    // Discovery belongs to this connection; another client's (the sub-panel's) is unaffected.
    adapter.call_method("StartDiscovery", &()).await?;
    let found = tokio::time::timeout(REDISCOVER, async {
        while !has_device(connection, path).await {
            if !dbus::next_change(&mut signals).await {
                return false;
            }
        }
        true
    })
    .await
    .unwrap_or(false);
    let _ = adapter.call_method("StopDiscovery", &()).await;
    if !found {
        return Err(zbus::Error::Failure(format!("{} did not show up again; put it in pairing mode and press PAIR", path.as_str())));
    }
    pair(connection, path).await
}

async fn has_device(connection: &Connection, path: &OwnedObjectPath) -> bool {
    managed_objects(connection).await.is_some_and(|objects| objects.get(path).is_some_and(|interfaces| interfaces.contains_key(DEVICE)))
}

/// BlueZ's reason when the device no longer has this computer's link key (BR/EDR or LE).
fn is_key_missing(reason: &str) -> bool {
    matches!(reason, "br-connection-key-missing" | "le-connection-key-missing")
}

/// The adapter a device object hangs from: `/org/bluez/hci0/dev_X` is on `/org/bluez/hci0`.
fn adapter_of(device: &str) -> &str {
    device.rsplit_once('/').map_or(device, |(adapter, _)| adapter)
}

impl Bluetooth {
    pub fn label(&self) -> String {
        match (&self.device, self.powered) {
            (_, false) => String::from("OFF"),
            (Some(device), true) => device.to_uppercase(),
            (None, true) => String::from("ON"),
        }
    }
}

/// Streams the Bluetooth adapter state, reading it again whenever BlueZ signals a change.
pub fn events() -> impl Stream<Item = Bluetooth> {
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
                eprintln!("zehll: cannot watch BlueZ: {err}");
                return;
            }
        };
        let mut last = None;
        loop {
            let bluetooth = read(&connection).await;
            if bluetooth != last {
                last = bluetooth.clone();
                if let Some(bluetooth) = bluetooth
                    && output.send(bluetooth).await.is_err()
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

/// Searches for devices while the stream is alive. BlueZ ties discovery to the D-Bus client that
/// started it, so dropping the stream (closing the sub-panel) ends the search.
pub fn discover() -> impl Stream<Item = ()> {
    stream::channel(1, async |_output| {
        let result = async {
            let connection = Connection::system().await?;
            let Some(bluetooth) = read(&connection).await else {
                return zbus::Result::Ok(None);
            };
            dbus::proxy(&connection, SERVICE, bluetooth.adapter.as_str(), ADAPTER).await?.call_method("StartDiscovery", &()).await?;
            Ok(Some(connection))
        };
        match result.await {
            // Hold the connection until the subscription is dropped.
            Ok(Some(_connection)) => std::future::pending::<()>().await,
            Ok(None) => {}
            Err(err) => eprintln!("zehll: cannot start Bluetooth discovery: {err}"),
        }
    })
}

pub async fn set_powered(adapter: OwnedObjectPath, powered: bool) {
    let result = async {
        let connection = Connection::system().await?;
        dbus::proxy(&connection, SERVICE, adapter.as_str(), ADAPTER).await?.set_property("Powered", powered).await?;
        zbus::Result::Ok(())
    };
    if let Err(err) = result.await {
        eprintln!("zehll: cannot toggle Bluetooth: {err}");
    }
}

async fn read(connection: &Connection) -> Option<Bluetooth> {
    summarize(&managed_objects(connection).await?)
}

async fn managed_objects(connection: &Connection) -> Option<ManagedObjects> {
    let manager = ObjectManagerProxy::builder(connection)
        .destination(SERVICE)
        .ok()?
        .path("/")
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let objects = manager.get_managed_objects().await.ok()?;
    Some(
        objects
            .into_iter()
            .map(|(path, interfaces)| (path, interfaces.into_iter().map(|(name, properties)| (name.to_string(), properties)).collect()))
            .collect(),
    )
}

/// Summarizes BlueZ's managed objects: the first adapter and a connected device.
fn summarize(objects: &ManagedObjects) -> Option<Bluetooth> {
    let (adapter, properties) = objects
        .iter()
        .filter_map(|(path, interfaces)| Some((path, interfaces.get(ADAPTER)?)))
        .min_by_key(|(path, _)| path.as_str())?;
    let devices = devices(objects);
    let device = devices.iter().find(|device| device.connected).map(|device| device.alias.clone());
    Some(Bluetooth { adapter: adapter.clone(), powered: flag(properties, "Powered"), discovering: flag(properties, "Discovering"), device, devices })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn value(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn object(interface: &str, properties: &[(&str, OwnedValue)]) -> HashMap<String, HashMap<String, OwnedValue>> {
        let properties = properties.iter().map(|(key, value)| (key.to_string(), value.try_clone().unwrap())).collect();
        HashMap::from([(interface.to_owned(), properties)])
    }

    fn path(path: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(path).unwrap()
    }

    #[test]
    fn detects_a_missing_link_key() {
        // Seen in zehll.log when the headset had paired with another host.
        assert!(is_key_missing("br-connection-key-missing"));
        assert!(is_key_missing("le-connection-key-missing"));
        assert!(!is_key_missing("br-connection-page-timeout"));
        assert!(!is_key_missing("br-connection-busy"));
    }

    #[test]
    fn finds_the_adapter_of_a_device() {
        assert_eq!(adapter_of("/org/bluez/hci0/dev_C0_EA_C3_18_57_B8"), "/org/bluez/hci0");
        assert_eq!(adapter_of("/org/bluez/hci1/dev_4C_72_74_8F_CB_84"), "/org/bluez/hci1");
    }

    #[test]
    fn finds_adapter_and_connected_device() {
        let objects = ManagedObjects::from([
            (path("/org/bluez/hci0"), object("org.bluez.Adapter1", &[("Powered", value(true.into()))])),
            (
                path("/org/bluez/hci0/dev_A"),
                object("org.bluez.Device1", &[("Connected", value(false.into())), ("Alias", value("MI BT 18I".into()))]),
            ),
            (
                path("/org/bluez/hci0/dev_B"),
                object("org.bluez.Device1", &[("Connected", value(true.into())), ("Alias", value("Soundcore Life Q20".into()))]),
            ),
        ]);
        let bluetooth = summarize(&objects).unwrap();
        assert_eq!(bluetooth.adapter, path("/org/bluez/hci0"));
        assert!(bluetooth.powered);
        assert_eq!(bluetooth.device.as_deref(), Some("Soundcore Life Q20"));
        assert_eq!(bluetooth.label(), "SOUNDCORE LIFE Q20");
    }

    #[test]
    fn lists_paired_devices_connected_first() {
        let mut headset = object(
            "org.bluez.Device1",
            &[
                ("Connected", value(true.into())),
                ("Paired", value(true.into())),
                ("Trusted", value(true.into())),
                ("Alias", value("Soundcore Life Q20".into())),
                ("Icon", value("audio-headset".into())),
            ],
        );
        headset.extend(object("org.bluez.Battery1", &[("Percentage", value(80u8.into()))]));
        let objects = ManagedObjects::from([
            (
                path("/org/bluez/hci0"),
                object("org.bluez.Adapter1", &[("Powered", value(true.into())), ("Discovering", value(true.into()))]),
            ),
            (
                path("/org/bluez/hci0/dev_A"),
                object("org.bluez.Device1", &[("Connected", value(false.into())), ("Paired", value(true.into())), ("Alias", value("MI BT 18I".into()))]),
            ),
            (path("/org/bluez/hci0/dev_B"), headset),
            // Seen during a scan but never paired: listed last.
            (path("/org/bluez/hci0/dev_C"), object("org.bluez.Device1", &[("Alias", value("Another TV".into()))])),
        ]);
        let bluetooth = summarize(&objects).unwrap();
        assert!(bluetooth.discovering);
        let devices = bluetooth.devices;
        let order: Vec<&str> = devices.iter().map(|device| device.alias.as_str()).collect();
        assert_eq!(order, ["Soundcore Life Q20", "MI BT 18I", "Another TV"]);
        assert_eq!(
            devices[0],
            Device {
                path: path("/org/bluez/hci0/dev_B"),
                alias: String::from("Soundcore Life Q20"),
                kind: String::from("audio-headset"),
                connected: true,
                paired: true,
                trusted: true,
                battery: Some(80),
            }
        );
        assert_eq!((devices[1].paired, devices[1].connected, devices[1].battery), (true, false, None));
        assert!(!devices[2].paired);
    }

    #[test]
    fn no_adapter_means_no_bluetooth() {
        assert_eq!(summarize(&ManagedObjects::new()), None);
    }
}
