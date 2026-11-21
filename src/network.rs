use iced::futures::{SinkExt, Stream};
use iced::stream;
use zbus::Connection;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus;

const SERVICE: &str = "org.freedesktop.NetworkManager";
const PATH: &str = "/org/freedesktop/NetworkManager";
const ACTIVE_CONNECTION: &str = "org.freedesktop.NetworkManager.Connection.Active";
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

    #[test]
    fn labels() {
        assert_eq!(network(true, Kind::Wired, "Wired connection 1").label(), "WIRED");
        assert_eq!(network(true, Kind::Wireless, "Home 5G").label(), "HOME 5G");
        assert_eq!(network(true, Kind::Other, "tun0").label(), "TUN0");
        assert_eq!(network(false, Kind::Other, "").label(), "DISCONNECTED");
    }
}
