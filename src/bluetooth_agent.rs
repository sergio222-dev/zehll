//! BlueZ agent. Without one, BlueZ rejects the profiles an untrusted device opens (AVRCP, HFP)
//! and any pairing that needs a confirmation, so devices connect only halfway.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::sync::Mutex;

use iced::futures::{Stream, StreamExt};
use iced::stream;
use zbus::zvariant::ObjectPath;
use zbus::{Connection, interface};

use crate::dbus;

const SERVICE: &str = "org.bluez";
const DEVICE: &str = "org.bluez.Device1";
const PATH: &str = "/org/zehll/bluetooth/agent";
// Zehll has no UI to type or show a PIN.
const CAPABILITY: &str = "NoInputNoOutput";
// Most legacy devices without a keypad (speakers, headsets) use this PIN.
const LEGACY_PIN: &str = "0000";

/// Devices with a pairing started from the sub-panel.
static PAIRING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Marks `device` as being paired from the sub-panel until the returned guard drops.
pub fn pairing(device: &str) -> Pairing {
    PAIRING.lock().unwrap().insert(device.to_owned());
    Pairing(device.to_owned())
}

pub struct Pairing(String);

impl Drop for Pairing {
    fn drop(&mut self) {
        PAIRING.lock().unwrap().remove(&self.0);
    }
}

fn is_pairing(device: &str) -> bool {
    PAIRING.lock().unwrap().contains(device)
}

/// What BlueZ asks the agent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Request {
    /// A legacy PIN code.
    PinCode,
    /// A passkey shown on the other device.
    Passkey,
    /// Confirm a passkey (numeric comparison).
    Confirmation,
    /// Accept a just-works pairing the device started.
    Authorization,
    /// Let the device open a profile (AVRCP, HFP...).
    Service,
}

/// What Zehll knows about the device asking.
#[derive(Debug, Clone, Copy, Default)]
struct Known {
    paired: bool,
    trusted: bool,
    /// A pairing with this device was started from the sub-panel.
    pairing: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Answer {
    Accept,
    Pin(&'static str),
    Reject,
}

/// Accepts requests for a pairing started from the sub-panel and services of known devices;
/// rejects the rest, so nearby devices can't pair on their own.
fn answer(request: Request, known: Known) -> Answer {
    match request {
        Request::PinCode if known.pairing => Answer::Pin(LEGACY_PIN),
        Request::Confirmation | Request::Authorization if known.pairing => Answer::Accept,
        Request::Service if known.pairing || known.paired || known.trusted => Answer::Accept,
        _ => Answer::Reject,
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
}

struct Agent;

impl Agent {
    async fn decide(&self, connection: &Connection, device: &ObjectPath<'_>, request: Request) -> Answer {
        let known = async {
            let proxy = dbus::proxy(connection, SERVICE, device.as_str(), DEVICE).await?;
            zbus::Result::Ok(Known {
                paired: proxy.get_property("Paired").await.unwrap_or(false),
                trusted: proxy.get_property("Trusted").await.unwrap_or(false),
                pairing: is_pairing(device.as_str()),
            })
        };
        let known = known.await.unwrap_or(Known { pairing: is_pairing(device.as_str()), ..Known::default() });
        let answer = answer(request, known);
        if answer == Answer::Reject {
            eprintln!("zehll: Bluetooth agent rejected {request:?} from {device}");
        }
        answer
    }

    async fn accept(&self, connection: &Connection, device: &ObjectPath<'_>, request: Request) -> Result<(), AgentError> {
        match self.decide(connection, device, request).await {
            Answer::Accept => Ok(()),
            _ => Err(rejected()),
        }
    }
}

fn rejected() -> AgentError {
    AgentError::Rejected(String::from("Rejected by Zehll"))
}

#[interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}

    async fn request_pin_code(&self, device: ObjectPath<'_>, #[zbus(connection)] connection: &Connection) -> Result<String, AgentError> {
        match self.decide(connection, &device, Request::PinCode).await {
            Answer::Pin(pin) => Ok(pin.to_owned()),
            _ => Err(rejected()),
        }
    }

    fn display_pin_code(&self, device: ObjectPath<'_>, pincode: String) {
        eprintln!("zehll: Bluetooth PIN for {device}: {pincode}");
    }

    async fn request_passkey(&self, device: ObjectPath<'_>, #[zbus(connection)] connection: &Connection) -> Result<u32, AgentError> {
        self.decide(connection, &device, Request::Passkey).await;
        Err(rejected())
    }

    fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, _entered: u16) {
        eprintln!("zehll: Bluetooth passkey for {device}: {passkey:06}");
    }

    async fn request_confirmation(&self, device: ObjectPath<'_>, _passkey: u32, #[zbus(connection)] connection: &Connection) -> Result<(), AgentError> {
        self.accept(connection, &device, Request::Confirmation).await
    }

    async fn request_authorization(&self, device: ObjectPath<'_>, #[zbus(connection)] connection: &Connection) -> Result<(), AgentError> {
        self.accept(connection, &device, Request::Authorization).await
    }

    async fn authorize_service(&self, device: ObjectPath<'_>, _uuid: String, #[zbus(connection)] connection: &Connection) -> Result<(), AgentError> {
        self.accept(connection, &device, Request::Service).await
    }

    fn cancel(&self) {}
}

async fn register(connection: &Connection) -> zbus::Result<()> {
    let manager = dbus::proxy(connection, SERVICE, "/org/bluez", "org.bluez.AgentManager1").await?;
    let path = ObjectPath::from_static_str_unchecked(PATH);
    manager.call_method("RegisterAgent", &(&path, CAPABILITY)).await?;
    manager.call_method("RequestDefaultAgent", &(&path,)).await?;
    Ok(())
}

/// Serves the agent on the system bus for as long as the subscription lives, registering it
/// again whenever BlueZ restarts. Emits nothing.
pub fn serve() -> impl Stream<Item = Infallible> {
    stream::channel(1, async |_output| {
        let setup = async {
            let connection = zbus::connection::Builder::system()?.serve_at(PATH, Agent)?.build().await?;
            let bus = zbus::fdo::DBusProxy::new(&connection).await?;
            let owners = bus.receive_name_owner_changed_with_args(&[(0, SERVICE)]).await?;
            zbus::Result::Ok((connection, owners))
        };
        let (connection, mut owners) = match setup.await {
            Ok(setup) => setup,
            Err(err) => {
                eprintln!("zehll: cannot serve the Bluetooth agent: {err}");
                return;
            }
        };
        if let Err(err) = register(&connection).await {
            eprintln!("zehll: cannot register the Bluetooth agent: {err}");
        }
        while let Some(signal) = owners.next().await {
            let restarted = signal.args().is_ok_and(|args| args.new_owner().is_some());
            if restarted && let Err(err) = register(&connection).await {
                eprintln!("zehll: cannot register the Bluetooth agent: {err}");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRANGER: Known = Known { paired: false, trusted: false, pairing: false };
    const PAIRING_NOW: Known = Known { paired: false, trusted: false, pairing: true };

    #[test]
    fn accepts_a_pairing_started_from_the_panel() {
        assert_eq!(answer(Request::Confirmation, PAIRING_NOW), Answer::Accept);
        assert_eq!(answer(Request::Authorization, PAIRING_NOW), Answer::Accept);
        assert_eq!(answer(Request::PinCode, PAIRING_NOW), Answer::Pin("0000"));
        assert_eq!(answer(Request::Service, PAIRING_NOW), Answer::Accept);
    }

    #[test]
    fn rejects_pairings_a_stranger_starts() {
        for request in [Request::PinCode, Request::Passkey, Request::Confirmation, Request::Authorization, Request::Service] {
            assert_eq!(answer(request, STRANGER), Answer::Reject, "{request:?}");
        }
    }

    #[test]
    fn lets_paired_or_trusted_devices_open_profiles() {
        assert_eq!(answer(Request::Service, Known { paired: true, ..STRANGER }), Answer::Accept);
        assert_eq!(answer(Request::Service, Known { trusted: true, ..STRANGER }), Answer::Accept);
        // A paired device asking to pair again goes through the sub-panel.
        assert_eq!(answer(Request::Authorization, Known { paired: true, ..STRANGER }), Answer::Reject);
    }

    #[test]
    fn never_accepts_a_passkey_it_cannot_type() {
        assert_eq!(answer(Request::Passkey, PAIRING_NOW), Answer::Reject);
    }

    #[test]
    fn tracks_pairings_until_the_guard_drops() {
        let device = "/org/bluez/hci0/dev_TEST";
        let guard = pairing(device);
        assert!(is_pairing(device));
        drop(guard);
        assert!(!is_pairing(device));
    }
}
