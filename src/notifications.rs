use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Local};
use enumflags2::BitFlags;
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use zbus::fdo::RequestNameFlags;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, interface};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// `NotificationClosed` reasons from the specification.
pub const DISMISSED: u32 = 2;
const CLOSED_BY_CALL: u32 = 3;
// How long a popup stays when the client leaves it to the server (`expire_timeout` of -1).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub critical: bool,
    pub time: DateTime<Local>,
    /// How long the popup stays on screen; `None` keeps it until dismissed.
    pub timeout: Option<Duration>,
}

/// Popup lifetime from the client's `expire_timeout` in milliseconds: -1 leaves it to the
/// server, 0 never expires. Critical notifications never expire, as the specification suggests.
fn popup_timeout(expire_timeout: i32, critical: bool) -> Option<Duration> {
    match expire_timeout {
        _ if critical => None,
        -1 => Some(DEFAULT_TIMEOUT),
        0 => None,
        milliseconds => Some(Duration::from_millis(milliseconds.unsigned_abs().into())),
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Handle),
    Notify(Notification),
    Close(u32),
}

/// Lets the bar tell clients that the user dismissed their notifications.
#[derive(Debug, Clone)]
pub struct Handle {
    connection: Connection,
}

impl Handle {
    pub async fn closed(self, ids: Vec<u32>, reason: u32) {
        let result = async {
            let server = self.connection.object_server().interface::<_, Server>(PATH).await?;
            for id in ids {
                Server::notification_closed(server.signal_emitter(), id, reason).await?;
            }
            zbus::Result::Ok(())
        };
        if let Err(err) = result.await {
            eprintln!("zehll: cannot report closed notifications: {err}");
        }
    }
}

/// Serves `org.freedesktop.Notifications` on the session bus and streams what clients send.
pub fn events() -> impl Stream<Item = Event> {
    stream::channel(16, async |mut output| {
        let (sender, mut receiver) = mpsc::unbounded();
        let server = Server { next_id: 1, events: sender };
        let connection = async {
            let connection = zbus::connection::Builder::session()?.serve_at(PATH, server)?.build().await?;
            // Queue for the name: a running daemon keeps it, and D-Bus hands it over when that daemon exits.
            connection.request_name_with_flags(NAME, BitFlags::<RequestNameFlags>::empty()).await?;
            zbus::Result::Ok(connection)
        };
        let connection = match connection.await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!("zehll: cannot serve notifications: {err}");
                return;
            }
        };
        if output.send(Event::Ready(Handle { connection })).await.is_err() {
            return;
        }
        while let Some(event) = receiver.next().await {
            if output.send(event).await.is_err() {
                return;
            }
        }
    })
}

struct Server {
    next_id: u32,
    events: mpsc::UnboundedSender<Event>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = match replaces_id {
            0 => {
                let id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1).max(1);
                id
            }
            id => id,
        };
        let critical = hints.get("urgency").and_then(|urgency| u8::try_from(urgency).ok()) == Some(2);
        let notification = Notification {
            id,
            app_name,
            summary,
            body: strip_markup(&body),
            critical,
            time: Local::now(),
            timeout: popup_timeout(expire_timeout, critical),
        };
        let _ = self.events.unbounded_send(Event::Notify(notification));
        id
    }

    async fn close_notification(&self, id: u32, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let _ = self.events.unbounded_send(Event::Close(id));
        Self::notification_closed(&emitter, id, CLOSED_BY_CALL).await?;
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["body", "body-markup"]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("Zehll", "zehll", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(signal)]
    async fn notification_closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;
}

/// Plain text of a notification body: drops the markup tags the spec allows and decodes entities.
fn strip_markup(body: &str) -> String {
    let mut text = String::with_capacity(body.len());
    let mut in_tag = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_timeouts() {
        assert_eq!(popup_timeout(-1, false), Some(DEFAULT_TIMEOUT));
        assert_eq!(popup_timeout(0, false), None);
        assert_eq!(popup_timeout(3000, false), Some(Duration::from_secs(3)));
        assert_eq!(popup_timeout(3000, true), None);
    }

    #[test]
    fn strips_markup_and_entities() {
        assert_eq!(strip_markup("<b>Build</b> finished in <i>3s</i>"), "Build finished in 3s");
        assert_eq!(strip_markup("<a href=\"https://x\">link</a> &amp; more &lt;3"), "link & more <3");
        assert_eq!(strip_markup("plain text"), "plain text");
    }
}
