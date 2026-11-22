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
    serde_json::from_str(&request_text(command)?).ok()
}

fn request_text(command: &str) -> Option<String> {
    let mut socket = StdUnixStream::connect(socket_dir()?.join(".socket.sock")).ok()?;
    socket.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    socket.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// Runs a dispatcher: `lua` for Lua configs, else the classic `legacy` form.
fn dispatch(lua: &str, legacy: &str) -> bool {
    let ok = |command: &str| request_text(&format!("dispatch {command}")).is_some_and(|reply| reply.trim() == "ok");
    ok(lua) || ok(legacy)
}

/// Runs a shell command as a child of Hyprland, so it outlives the shell and gets its rules.
pub fn exec(command: &str) -> bool {
    dispatch(&format!("hl.dsp.exec_cmd({})", lua_string(command)), &format!("exec {command}"))
}

pub fn focus_window(address: &str) -> bool {
    dispatch(&format!("hl.dsp.focus({{ window = \"address:{address}\" }})"), &format!("focuswindow address:{address}"))
}

/// Ends the Hyprland session.
pub fn exit() -> bool {
    dispatch("hl.dsp.exit()", "exit")
}

/// Open windows as (address, class).
pub fn clients() -> Vec<(String, String)> {
    let Some(serde_json::Value::Array(clients)) = request("j/clients") else {
        return Vec::new();
    };
    clients
        .iter()
        .filter_map(|client| Some((client["address"].as_str()?.to_owned(), client["class"].as_str()?.to_owned())))
        .collect()
}

/// A Lua string literal holding `text`.
fn lua_string(text: &str) -> String {
    let mut literal = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            c => literal.push(c),
        }
    }
    literal.push('"');
    literal
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_strings_escape_quotes_backslashes_and_newlines() {
        assert_eq!(lua_string("kitty -e btop"), "\"kitty -e btop\"");
        assert_eq!(lua_string("sh -c \"echo a\\b\"\n"), "\"sh -c \\\"echo a\\\\b\\\"\\n\"");
    }
}
