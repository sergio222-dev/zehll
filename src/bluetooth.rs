use std::collections::HashMap;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use zbus::Connection;
use zbus::fdo::ObjectManagerProxy;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::dbus;

const SERVICE: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct Bluetooth {
    pub adapter: OwnedObjectPath,
    pub powered: bool,
    /// Alias of a connected device.
    pub device: Option<String>,
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
    let objects = objects
        .into_iter()
        .map(|(path, interfaces)| (path, interfaces.into_iter().map(|(name, properties)| (name.to_string(), properties)).collect()))
        .collect();
    summarize(&objects)
}

/// Summarizes BlueZ's managed objects: the first adapter and a connected device.
fn summarize(objects: &ManagedObjects) -> Option<Bluetooth> {
    let flag = |properties: &HashMap<String, OwnedValue>, key: &str| {
        properties.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false)
    };
    let (adapter, properties) = objects
        .iter()
        .filter_map(|(path, interfaces)| Some((path, interfaces.get(ADAPTER)?)))
        .min_by_key(|(path, _)| path.as_str())?;
    let device = objects
        .iter()
        .filter_map(|(path, interfaces)| Some((path, interfaces.get(DEVICE)?)))
        .filter(|(_, device)| flag(device, "Connected"))
        .min_by_key(|(path, _)| path.as_str())
        .and_then(|(_, device)| device.get("Alias"))
        .and_then(|alias| String::try_from(alias.try_clone().ok()?).ok());
    Some(Bluetooth { adapter: adapter.clone(), powered: flag(properties, "Powered"), device })
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
    fn no_adapter_means_no_bluetooth() {
        assert_eq!(summarize(&ManagedObjects::new()), None);
    }
}
