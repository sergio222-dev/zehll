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

use crate::tray;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// `NotificationClosed` reasons from the specification.
pub const DISMISSED: u32 = 2;
const CLOSED_BY_CALL: u32 = 3;
pub const DEFAULT_ACTION: &str = "default";
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
    pub actions: Vec<Action>,
    pub icon: Option<tray::Icon>,
}

impl Notification {
    pub fn has_default_action(&self) -> bool {
        self.actions.iter().any(|action| action.key == DEFAULT_ACTION)
    }

    /// Actions shown as buttons; the default one is triggered by clicking the notification.
    pub fn buttons(&self) -> impl Iterator<Item = &Action> {
        self.actions.iter().filter(|action| action.key != DEFAULT_ACTION)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// Pairs the flat `[key, label, key, label, ...]` list clients send.
fn parse_actions(actions: &[String]) -> Vec<Action> {
    actions.chunks_exact(2).map(|pair| Action { key: pair[0].clone(), label: pair[1].clone() }).collect()
}

/// RGBA pixels from the `image-data` hint: rows of `rowstride` bytes, RGB or RGBA.
fn image_data_rgba(width: i32, height: i32, rowstride: i32, has_alpha: bool, data: &[u8]) -> Option<Vec<u8>> {
    let (width, height, rowstride) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?, usize::try_from(rowstride).ok()?);
    let channels = if has_alpha { 4 } else { 3 };
    if height == 0 || data.len() < rowstride * (height - 1) + width * channels {
        return None;
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in data.chunks(rowstride).take(height) {
        for pixel in row[..width * channels].chunks_exact(channels) {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], if has_alpha { pixel[3] } else { 255 }]);
        }
    }
    Some(rgba)
}

/// Picks the notification image: raw `image-data`, then `image-path`, then the app icon.
fn notification_icon(app_icon: &str, hints: &HashMap<String, OwnedValue>) -> Option<tray::Icon> {
    let image_data = ["image-data", "image_data", "icon_data"].iter().find_map(|key| hints.get(*key));
    if let Some(value) = image_data
        && let Ok((width, height, rowstride, has_alpha, _bits, _channels, data)) =
            <(i32, i32, i32, bool, i32, i32, Vec<u8>)>::try_from(value.try_clone().ok()?)
        && let Some(rgba) = image_data_rgba(width, height, rowstride, has_alpha, &data)
    {
        return Some(tray::Icon::Raster(iced::widget::image::Handle::from_rgba(width as u32, height as u32, rgba)));
    }
    let path = ["image-path", "image_path"]
        .iter()
        .find_map(|key| hints.get(*key).and_then(|value| String::try_from(value.try_clone().ok()?).ok()))
        .unwrap_or_else(|| app_icon.to_owned());
    let path = path.strip_prefix("file://").unwrap_or(&path);
    (!path.is_empty()).then(|| tray::lookup(path, None)).flatten()
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
    pub async fn invoke(self, id: u32, key: String) {
        let result = async {
            let server = self.connection.object_server().interface::<_, Server>(PATH).await?;
            Server::action_invoked(server.signal_emitter(), id, &key).await
        };
        if let Err(err) = result.await {
            eprintln!("zehll: cannot invoke notification action: {err}");
        }
    }

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

/// Sends a notification to whichever daemon owns the name, as any other client would.
pub async fn send(app: &str, icon: &str, summary: &str, body: &str) {
    let result = async {
        let connection = Connection::session().await?;
        let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        connection
            .call_method(Some(NAME), PATH, Some(NAME), "Notify", &(app, 0u32, icon, summary, body, Vec::<&str>::new(), hints, -1i32))
            .await?;
        zbus::Result::Ok(())
    };
    if let Err(err) = result.await {
        eprintln!("zehll: cannot send notification: {err}");
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
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
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
            actions: parse_actions(&actions),
            icon: notification_icon(&app_icon, &hints),
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
        vec!["actions", "body", "body-markup", "icon-static"]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("Zehll", "zehll", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(signal)]
    async fn notification_closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, action_key: &str) -> zbus::Result<()>;
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
    fn pairs_actions() {
        let actions = ["default", "Open", "reply", "Reply"].map(String::from);
        let parsed = parse_actions(&actions);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], Action { key: String::from("reply"), label: String::from("Reply") });
        assert!(parse_actions(&[String::from("dangling")]).is_empty());
    }

    #[test]
    fn converts_image_data() {
        // 2x1 RGB, padded to an 8-byte rowstride.
        assert_eq!(image_data_rgba(2, 1, 8, false, &[1, 2, 3, 4, 5, 6, 0, 0]), Some(vec![1, 2, 3, 255, 4, 5, 6, 255]));
        assert_eq!(image_data_rgba(1, 1, 4, true, &[9, 8, 7, 6]), Some(vec![9, 8, 7, 6]));
        assert_eq!(image_data_rgba(2, 2, 6, false, &[0; 6]), None);
    }

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
