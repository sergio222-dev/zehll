use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::PathBuf;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Clone, Default)]
pub struct ActiveWindow {
    pub class: String,
    pub title: String,
}

fn socket_dir() -> Option<PathBuf> {
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    Some(PathBuf::from(runtime).join("hypr").join(signature))
}

/// Sends a JSON request (e.g. `j/activewindow`) to the Hyprland request socket.
fn request(command: &str) -> Option<serde_json::Value> {
    let mut socket = StdUnixStream::connect(socket_dir()?.join(".socket.sock")).ok()?;
    socket.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    socket.read_to_string(&mut reply).ok()?;
    serde_json::from_str(&reply).ok()
}

/// Queries the currently focused window.
pub fn active_window() -> Option<ActiveWindow> {
    let json = request("j/activewindow")?;
    Some(ActiveWindow {
        class: json["class"].as_str().unwrap_or_default().to_owned(),
        title: json["title"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Streams focus and title changes from the Hyprland event socket.
pub fn active_window_events() -> impl Stream<Item = ActiveWindow> {
    stream::channel(16, async |mut output| {
        let Some(path) = socket_dir().map(|dir| dir.join(".socket2.sock")) else {
            eprintln!("zehll: not running under Hyprland");
            return;
        };
        let socket = match UnixStream::connect(&path).await {
            Ok(socket) => socket,
            Err(err) => {
                eprintln!("zehll: cannot connect to {}: {err}", path.display());
                return;
            }
        };

        let mut lines = BufReader::new(socket).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            // Format: activewindow>>CLASS,TITLE (the title may contain commas).
            let Some(data) = line.strip_prefix("activewindow>>") else {
                continue;
            };
            let (class, title) = data.split_once(',').unwrap_or((data, ""));
            let window = ActiveWindow {
                class: class.to_owned(),
                title: title.to_owned(),
            };
            if output.send(window).await.is_err() {
                return;
            }
        }
    })
}

fn focused_monitor_json() -> Option<serde_json::Value> {
    request("j/monitors")?
        .as_array()?
        .iter()
        .find(|monitor| monitor["focused"].as_bool() == Some(true))
        .cloned()
}

/// Name of the monitor Hyprland currently considers focused.
pub fn focused_monitor() -> Option<String> {
    focused_monitor_json()?["name"].as_str().map(str::to_owned)
}

/// Cursor x relative to the left edge of the focused monitor, which is where the bar starts.
pub fn cursor_x_on_focused_monitor() -> Option<i32> {
    let cursor = request("j/cursorpos")?["x"].as_i64()?;
    let monitor = focused_monitor_json()?["x"].as_i64()?;
    Some((cursor - monitor) as i32)
}

/// Names of all connected monitors.
pub fn monitors() -> Vec<String> {
    request("j/monitors")
        .and_then(|json| json.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|monitor| monitor["name"].as_str().map(str::to_owned))
        .collect()
}
