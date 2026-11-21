//! Media players over MPRIS, like ii's `MprisController`: the active player that the bar's
//! media button and the player popover show and control.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream, stream};
use iced::widget::image;
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream};

use crate::dbus;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
// playerctld mirrors other players; showing it would duplicate them.
const PLAYERCTLD: &str = "org.mpris.MediaPlayer2.playerctld";
// Cover art downloads give up after this long.
const ART_TIMEOUT: &str = "10";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loop {
    None,
    Track,
    Playlist,
}

impl Loop {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "None" => Some(Self::None),
            "Track" => Some(Self::Track),
            "Playlist" => Some(Self::Playlist),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Track => "Track",
            Self::Playlist => "Playlist",
        }
    }

    /// Order of the repeat button: off, the playlist, the track.
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Playlist,
            Self::Playlist => Self::Track,
            Self::Track => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    /// D-Bus name, such as `org.mpris.MediaPlayer2.spotify`.
    pub service: String,
    /// Name the player gives itself, such as `Spotify`.
    pub identity: String,
    pub title: String,
    pub artist: String,
    pub art_url: String,
    track_id: Option<OwnedObjectPath>,
    /// None for live streams.
    pub length: Option<Duration>,
    pub playing: bool,
    /// None when the player does not support shuffling.
    pub shuffle: Option<bool>,
    /// None when the player does not support repeating.
    pub loop_status: Option<Loop>,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play_pause: bool,
    pub can_seek: bool,
    // MPRIS does not signal position changes while playing; it is read once and extrapolated.
    position: Duration,
    read_at: Instant,
}

impl Player {
    /// Playback position at `now`, extrapolated from the last read while playing.
    pub fn position(&self, now: Instant) -> Duration {
        let position = match self.playing {
            true => self.position + now.saturating_duration_since(self.read_at),
            false => self.position,
        };
        match self.length {
            Some(length) => position.min(length),
            None => position,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    PlayPause,
    Next,
    Previous,
    Seek(Duration),
    Shuffle(bool),
    Loop(Loop),
}

/// Streams the active player, reading every player again when one changes or appears.
pub fn events() -> impl Stream<Item = Option<Player>> {
    iced::stream::channel(4, async |mut output| {
        let connection = match Connection::session().await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!("zehll: no session bus: {err}");
                return;
            }
        };
        let mut signals = match watch(&connection).await {
            Ok(signals) => signals,
            Err(err) => {
                eprintln!("zehll: cannot watch media players: {err}");
                return;
            }
        };
        let mut active: Option<Player> = None;
        loop {
            let players = read(&connection).await;
            let next = pick(players, active.as_ref().map(|player| player.service.as_str()));
            // Every read restarts the position clock, so equal players are still sent.
            if (next.is_some() || active.is_some()) && output.send(next.clone()).await.is_err() {
                return;
            }
            active = next;
            if !dbus::next_change(&mut signals).await {
                return;
            }
        }
    })
}

/// Property changes and seeks of every player, and players appearing or leaving the bus.
async fn watch(connection: &Connection) -> zbus::Result<impl Stream<Item = zbus::Result<zbus::Message>> + Unpin> {
    let players = MatchRule::builder().msg_type(Type::Signal).path(PATH)?.build();
    let owners = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg0ns(ROOT)?
        .build();
    Ok(stream::select(
        MessageStream::for_match_rule(players, connection, None).await?,
        MessageStream::for_match_rule(owners, connection, None).await?,
    ))
}

/// Like ii: the playing player, preferring the one already shown; then the one shown; then the first.
pub fn pick(players: Vec<Player>, shown: Option<&str>) -> Option<Player> {
    let is_shown = |player: &Player| Some(player.service.as_str()) == shown;
    let index = players
        .iter()
        .position(|player| player.playing && is_shown(player))
        .or_else(|| players.iter().position(|player| player.playing))
        .or_else(|| players.iter().position(is_shown))
        .or((!players.is_empty()).then_some(0))?;
    players.into_iter().nth(index)
}

async fn read(connection: &Connection) -> Vec<Player> {
    let names = match zbus::fdo::DBusProxy::new(connection).await {
        Ok(proxy) => proxy.list_names().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut players = Vec::new();
    for name in names.iter().map(|name| name.as_str()).filter(|name| name.starts_with(PREFIX) && *name != PLAYERCTLD) {
        // A player leaving while it is read is skipped.
        if let Ok(player) = read_player(connection, name).await {
            players.push(player);
        }
    }
    players
}

async fn read_player(connection: &Connection, service: &str) -> zbus::Result<Player> {
    let properties = dbus::proxy(connection, service, PATH, PROPERTIES).await?;
    let values: HashMap<String, OwnedValue> = properties.call("GetAll", &(PLAYER,)).await?;
    let identity = match properties.call::<_, _, OwnedValue>("Get", &(ROOT, "Identity")).await {
        Ok(identity) => identity.downcast_ref::<String>().ok(),
        Err(_) => None,
    };
    let identity = identity.unwrap_or_else(|| service.trim_start_matches(PREFIX).split('.').next().unwrap_or_default().to_owned());
    Ok(parse(service, identity, &values, Instant::now()))
}

fn parse(service: &str, identity: String, values: &HashMap<String, OwnedValue>, now: Instant) -> Player {
    let metadata: HashMap<String, OwnedValue> = values
        .get("Metadata")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| HashMap::try_from(value).ok())
        .unwrap_or_default();
    let artist = match metadata.get("xesam:artist").and_then(|value| value.try_clone().ok()) {
        Some(value) => Vec::<String>::try_from(value).unwrap_or_default().join(", "),
        None => String::new(),
    };
    Player {
        service: service.to_owned(),
        identity,
        title: string(&metadata, "xesam:title").unwrap_or_default(),
        artist,
        art_url: string(&metadata, "mpris:artUrl").unwrap_or_default(),
        track_id: metadata.get("mpris:trackid").and_then(|value| value.downcast_ref::<ObjectPath>().ok()).map(Into::into),
        length: micros(&metadata, "mpris:length").filter(|length| !length.is_zero()),
        playing: string(values, "PlaybackStatus").as_deref() == Some("Playing"),
        shuffle: values.get("Shuffle").and_then(|value| value.downcast_ref::<bool>().ok()),
        loop_status: string(values, "LoopStatus").as_deref().and_then(Loop::parse),
        can_go_next: flag(values, "CanGoNext"),
        can_go_previous: flag(values, "CanGoPrevious"),
        can_play_pause: flag(values, "CanPlay") || flag(values, "CanPause"),
        can_seek: flag(values, "CanSeek"),
        position: micros(values, "Position").unwrap_or_default(),
        read_at: now,
    }
}

fn string(values: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    values.get(key)?.downcast_ref::<String>().ok()
}

fn flag(values: &HashMap<String, OwnedValue>, key: &str) -> bool {
    values.get(key).and_then(|value| value.downcast_ref::<bool>().ok()).unwrap_or(false)
}

/// MPRIS times are microseconds, signed by the spec but sent unsigned by some players.
fn micros(values: &HashMap<String, OwnedValue>, key: &str) -> Option<Duration> {
    let value = values.get(key)?;
    let micros = value.downcast_ref::<i64>().ok().or_else(|| value.downcast_ref::<u64>().ok().and_then(|value| i64::try_from(value).ok()))?;
    Some(Duration::from_micros(micros.max(0) as u64))
}

pub async fn run(player: Player, command: Command) {
    let result = async {
        let connection = Connection::session().await?;
        let proxy = dbus::proxy(&connection, &player.service, PATH, PLAYER).await?;
        match command {
            Command::PlayPause => proxy.call_method("PlayPause", &()).await.map(drop),
            Command::Next => proxy.call_method("Next", &()).await.map(drop),
            Command::Previous => proxy.call_method("Previous", &()).await.map(drop),
            Command::Seek(target) => {
                let target = i64::try_from(target.as_micros()).unwrap_or(i64::MAX);
                match &player.track_id {
                    Some(track) => proxy.call_method("SetPosition", &(track, target)).await.map(drop),
                    // Without a track id only a relative seek is possible.
                    None => {
                        let current = i64::try_from(player.position(Instant::now()).as_micros()).unwrap_or(i64::MAX);
                        proxy.call_method("Seek", &(target - current,)).await.map(drop)
                    }
                }
            }
            Command::Shuffle(shuffle) => proxy.set_property("Shuffle", shuffle).await.map_err(Into::into),
            Command::Loop(status) => proxy.set_property("LoopStatus", status.as_str()).await.map_err(Into::into),
        }
    };
    if let Err(err) = result.await {
        eprintln!("zehll: media command {command:?} failed: {err}");
    }
}

/// Cover art from a local file or downloaded with curl, which players such as Spotify link to.
pub async fn art(url: String) -> Option<image::Handle> {
    let bytes = match file_path(&url) {
        Some(path) => tokio::fs::read(path).await.ok()?,
        None if url.starts_with("https://") || url.starts_with("http://") => {
            let output = tokio::process::Command::new("curl")
                .args(["--silent", "--fail", "--location", "--max-time", ART_TIMEOUT, "--", &url])
                .output()
                .await
                .ok()?;
            output.status.success().then_some(output.stdout)?
        }
        None => return None,
    };
    Some(image::Handle::from_bytes(bytes))
}

/// Path of a `file://` URL, percent-decoded.
fn file_path(url: &str) -> Option<PathBuf> {
    let encoded = url.strip_prefix("file://")?.as_bytes();
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        let escaped = (encoded[index] == b'%')
            .then(|| encoded.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match escaped {
            Some(byte) => {
                bytes.push(byte);
                index += 3;
            }
            None => {
                bytes.push(encoded[index]);
                index += 1;
            }
        }
    }
    Some(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
}

/// `1:44`, or `1:02:03` past an hour.
pub fn format_time(time: Duration) -> String {
    let seconds = time.as_secs();
    match seconds / 3600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(service: &str, playing: bool) -> Player {
        Player {
            service: service.to_owned(),
            identity: String::new(),
            title: String::new(),
            artist: String::new(),
            art_url: String::new(),
            track_id: None,
            length: Some(Duration::from_secs(104)),
            playing,
            shuffle: None,
            loop_status: None,
            can_go_next: true,
            can_go_previous: true,
            can_play_pause: true,
            can_seek: true,
            position: Duration::from_secs(2),
            read_at: Instant::now(),
        }
    }

    fn picked(players: Vec<Player>, shown: Option<&str>) -> Option<String> {
        pick(players, shown).map(|player| player.service)
    }

    #[test]
    fn picks_the_playing_player_and_keeps_the_shown_one() {
        assert_eq!(picked(vec![], None), None);
        assert_eq!(picked(vec![player("a", false), player("b", true)], Some("a")).as_deref(), Some("b"));
        assert_eq!(picked(vec![player("a", true), player("b", true)], Some("b")).as_deref(), Some("b"));
        assert_eq!(picked(vec![player("a", false), player("b", false)], Some("b")).as_deref(), Some("b"));
        assert_eq!(picked(vec![player("a", false), player("b", false)], Some("gone")).as_deref(), Some("a"));
    }

    #[test]
    fn position_runs_only_while_playing_and_stops_at_the_end() {
        let paused = player("a", false);
        let later = paused.read_at + Duration::from_secs(10);
        assert_eq!(paused.position(later), Duration::from_secs(2));
        let playing = player("a", true);
        assert_eq!(playing.position(playing.read_at + Duration::from_secs(10)), Duration::from_secs(12));
        assert_eq!(playing.position(playing.read_at + Duration::from_secs(500)), Duration::from_secs(104));
    }

    #[test]
    fn repeat_cycles_off_playlist_track() {
        assert_eq!(Loop::None.next(), Loop::Playlist);
        assert_eq!(Loop::Playlist.next(), Loop::Track);
        assert_eq!(Loop::Track.next(), Loop::None);
        assert_eq!(Loop::parse("Track"), Some(Loop::Track));
        assert_eq!(Loop::parse("bogus"), None);
    }

    #[test]
    fn formats_times() {
        assert_eq!(format_time(Duration::from_secs(2)), "0:02");
        assert_eq!(format_time(Duration::from_secs(104)), "1:44");
        assert_eq!(format_time(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn decodes_file_urls() {
        assert_eq!(file_path("file:///tmp/my%20cover.png"), Some(PathBuf::from("/tmp/my cover.png")));
        assert_eq!(file_path("file:///tmp/100%"), Some(PathBuf::from("/tmp/100%")));
        assert_eq!(file_path("https://example.com/a.png"), None);
    }

    #[test]
    fn parses_player_properties() {
        use zbus::zvariant::Value;
        let metadata: HashMap<&str, Value> = HashMap::from([
            ("xesam:title", Value::from("window seats")),
            ("xesam:artist", Value::from(vec!["driftø blues", "guest"])),
            ("mpris:artUrl", Value::from("file:///tmp/cover.png")),
            ("mpris:length", Value::from(104_000_000_i64)),
            ("mpris:trackid", Value::from(ObjectPath::try_from("/track/1").unwrap())),
        ]);
        let owned = |value: Value| OwnedValue::try_from(value).unwrap();
        let values = HashMap::from([
            (String::from("Metadata"), owned(Value::from(metadata))),
            (String::from("PlaybackStatus"), owned(Value::from("Playing"))),
            (String::from("LoopStatus"), owned(Value::from("Playlist"))),
            (String::from("Shuffle"), owned(Value::from(true))),
            (String::from("Position"), owned(Value::from(2_000_000_i64))),
            (String::from("CanGoNext"), owned(Value::from(true))),
            (String::from("CanPause"), owned(Value::from(true))),
        ]);
        let now = Instant::now();
        let player = parse("org.mpris.MediaPlayer2.spotify", String::from("Spotify"), &values, now);
        assert_eq!(player.title, "window seats");
        assert_eq!(player.artist, "driftø blues, guest");
        assert_eq!(player.art_url, "file:///tmp/cover.png");
        assert_eq!(player.length, Some(Duration::from_secs(104)));
        assert_eq!(player.track_id.as_ref().map(|track| track.as_str()), Some("/track/1"));
        assert!(player.playing && player.can_go_next && player.can_play_pause);
        assert!(!player.can_go_previous && !player.can_seek);
        assert_eq!(player.shuffle, Some(true));
        assert_eq!(player.loop_status, Some(Loop::Playlist));
        assert_eq!(player.position(now), Duration::from_secs(2));
    }

    #[test]
    fn missing_properties_mean_unsupported() {
        let player = parse("org.mpris.MediaPlayer2.firefox", String::from("Firefox"), &HashMap::new(), Instant::now());
        assert_eq!((player.shuffle, player.loop_status, player.length), (None, None, None));
        assert!(!player.playing);
    }
}
