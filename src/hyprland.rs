use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::PathBuf;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
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

/// Every keybinding (`hyprctl binds -j`).
pub fn binds() -> Option<serde_json::Value> {
    request("j/binds")
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

/// Monitors, windows and layers (`j/monitors`, `j/clients`, `j/layers`), read together.
pub fn layout() -> Option<(serde_json::Value, serde_json::Value, serde_json::Value)> {
    Some((request("j/monitors")?, request("j/clients")?, request("j/layers")?))
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

/// Lines of the Hyprland event socket (`EVENT>>DATA`).
async fn event_lines() -> Option<Lines<BufReader<UnixStream>>> {
    let Some(path) = socket_dir().map(|dir| dir.join(".socket2.sock")) else {
        eprintln!("zehll: not running under Hyprland");
        return None;
    };
    match UnixStream::connect(&path).await {
        Ok(socket) => Some(BufReader::new(socket).lines()),
        Err(err) => {
            eprintln!("zehll: cannot connect to {}: {err}", path.display());
            None
        }
    }
}

/// Streams focus and title changes from the Hyprland event socket.
pub fn active_window_events() -> impl Stream<Item = ActiveWindow> {
    stream::channel(16, async |mut output| {
        let Some(mut lines) = event_lines().await else {
            return;
        };
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

/// A monitor, the id of the workspace it shows and the ids of its workspaces with windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub workspace: i64,
    pub occupied: Vec<i64>,
}

/// Every enabled monitor.
pub fn monitors() -> Vec<Monitor> {
    let (Some(monitors), Some(workspaces)) = (request("j/monitors"), request("j/workspaces")) else {
        return Vec::new();
    };
    parse_monitors(&monitors, &workspaces)
}

fn parse_monitors(json: &serde_json::Value, workspaces: &serde_json::Value) -> Vec<Monitor> {
    let (Some(monitors), Some(workspaces)) = (json.as_array(), workspaces.as_array()) else {
        return Vec::new();
    };
    // Special workspaces have negative ids and are not part of the indicator.
    let occupied = |name: &str| {
        workspaces
            .iter()
            .filter(|workspace| workspace["monitor"].as_str() == Some(name) && workspace["windows"].as_u64().unwrap_or(0) > 0)
            .filter_map(|workspace| workspace["id"].as_i64().filter(|id| *id > 0))
            .collect()
    };
    monitors
        .iter()
        .filter_map(|monitor| {
            let name = monitor["name"].as_str()?;
            Some(Monitor {
                name: name.to_owned(),
                workspace: monitor["activeWorkspace"]["id"].as_i64()?,
                occupied: occupied(name),
            })
        })
        .collect()
}

/// Whether an event socket line can change the monitors, the workspace one of them shows or
/// which workspaces have windows.
fn changes_monitors(line: &str) -> bool {
    let Some((event, _)) = line.split_once(">>") else {
        return false;
    };
    matches!(
        event.trim_end_matches("v2"),
        "workspace"
            | "focusedmon"
            | "moveworkspace"
            | "monitoradded"
            | "monitorremoved"
            | "openwindow"
            | "closewindow"
            | "movewindow"
            | "destroyworkspace"
    )
}

/// Streams the monitors, once on connecting and again whenever they or their workspaces change.
pub fn monitor_events() -> impl Stream<Item = Vec<Monitor>> {
    stream::channel(16, async |mut output| {
        let Some(mut lines) = event_lines().await else {
            return;
        };
        let mut last = monitors();
        if output.send(last.clone()).await.is_err() {
            return;
        }
        while let Ok(Some(line)) = lines.next_line().await {
            if !changes_monitors(&line) {
                continue;
            }
            let current = monitors();
            if current != last {
                last = current;
                if output.send(last.clone()).await.is_err() {
                    return;
                }
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

/// Every monitor as `hyprctl monitors -j` lists it.
pub fn monitors_json() -> Option<serde_json::Value> {
    request("j/monitors")
}

/// Name of the monitor Hyprland currently considers focused.
pub fn focused_monitor() -> Option<String> {
    focused_monitor_json()?["name"].as_str().map(str::to_owned)
}

/// Width of the focused monitor in logical pixels, the unit of layer surface margins.
pub fn focused_monitor_width() -> Option<i32> {
    let monitor = focused_monitor_json()?;
    let scale = monitor["scale"].as_f64().filter(|scale| *scale > 0.0).unwrap_or(1.0);
    Some((monitor["width"].as_f64()? / scale).round() as i32)
}

/// Shows the workspace `id` on the focused monitor.
pub fn focus_workspace(id: i64) -> bool {
    dispatch(&format!("hl.dsp.focus({{ workspace = \"{id}\", on_current_monitor = true }})"), &format!("workspace {id}"))
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

    #[test]
    fn monitors_keep_their_name_and_active_workspace() {
        let json = serde_json::json!([
            { "id": 0, "name": "DP-1", "activeWorkspace": { "id": 2, "name": "2" }, "focused": false },
            { "id": 1, "name": "HDMI-A-2", "activeWorkspace": { "id": 5, "name": "5" }, "focused": true },
            { "id": 2, "name": "broken" },
        ]);
        let workspaces = serde_json::json!([
            { "id": 1, "monitor": "DP-1", "windows": 2 },
            { "id": 2, "monitor": "DP-1", "windows": 0 },
            { "id": 4, "monitor": "DP-1", "windows": 1 },
            { "id": 6, "monitor": "HDMI-A-2", "windows": 3 },
            { "id": -98, "name": "special:special", "monitor": "HDMI-A-2", "windows": 1 },
        ]);
        assert_eq!(
            parse_monitors(&json, &workspaces),
            [
                Monitor { name: "DP-1".into(), workspace: 2, occupied: vec![1, 4] },
                Monitor { name: "HDMI-A-2".into(), workspace: 5, occupied: vec![6] },
            ]
        );
    }

    #[test]
    fn workspace_and_monitor_events_refresh_the_monitors() {
        for line in [
            "workspacev2>>6,6",
            "workspace>>6",
            "focusedmonv2>>DP-1,2",
            "moveworkspacev2>>3,3,HDMI-A-2",
            "monitoraddedv2>>2,DP-2,Dell",
            "monitorremovedv2>>2,DP-2,Dell",
            "openwindow>>5a1b,6,kitty,zsh",
            "closewindow>>5a1b",
            "movewindowv2>>5a1b,4,4",
            "destroyworkspacev2>>3,3",
        ] {
            assert!(changes_monitors(line), "{line}");
        }
        for line in ["activewindow>>kitty,workspacev2>>1", "windowtitlev2>>5a1b,zsh", "workspacev"] {
            assert!(!changes_monitors(line), "{line}");
        }
    }
}
