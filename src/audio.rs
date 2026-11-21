use std::process::Stdio;
use std::time::Duration;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub const OUTPUT: &str = "@DEFAULT_AUDIO_SINK@";
pub const INPUT: &str = "@DEFAULT_AUDIO_SOURCE@";
// A single change emits a burst of events; wait this long for it to end before reading once.
const SETTLE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Device {
    pub name: String,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    pub output: Device,
    pub input: Device,
}

/// Streams the default output and input devices, reading them again when PipeWire reports a change.
pub fn events() -> impl Stream<Item = Audio> {
    watch(read)
}

/// Runs `read` now and again after each burst of PipeWire changes, emitting only new values.
pub fn watch<T, F>(read: fn() -> F) -> impl Stream<Item = T>
where
    T: Clone + PartialEq + Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    stream::channel(4, async move |mut output| {
        let child = Command::new("pactl").arg("subscribe").stdout(Stdio::piped()).kill_on_drop(true).spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(err) => {
                eprintln!("zehll: cannot run pactl subscribe: {err}");
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        let mut lines = BufReader::new(stdout).lines();

        let mut last = read().await;
        if output.send(last.clone()).await.is_err() {
            return;
        }
        while let Ok(Some(line)) = lines.next_line().await {
            if !(line.contains(" on sink ") || line.contains(" on source ") || line.contains(" on server ")) {
                continue;
            }
            while let Ok(Ok(Some(_))) = tokio::time::timeout(SETTLE, lines.next_line()).await {}
            let audio = read().await;
            if audio != last {
                last = audio.clone();
                if output.send(audio).await.is_err() {
                    return;
                }
            }
        }
    })
}

pub async fn toggle_mute(target: &str) {
    if let Err(err) = Command::new("wpctl").args(["set-mute", target, "toggle"]).status().await {
        eprintln!("zehll: cannot toggle mute: {err}");
    }
}

async fn read() -> Audio {
    Audio { output: device(OUTPUT).await, input: device(INPUT).await }
}

async fn device(target: &str) -> Device {
    let (volume, muted) = wpctl(&["get-volume", target]).await.as_deref().and_then(parse_volume).unwrap_or_default();
    let name = wpctl(&["inspect", target]).await.as_deref().and_then(parse_description).unwrap_or_default();
    Device { name, volume, muted }
}

async fn wpctl(args: &[&str]) -> Option<String> {
    let output = Command::new("wpctl").args(args).output().await.ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parses `wpctl get-volume` output: `Volume: 0.69` or `Volume: 0.69 [MUTED]`.
fn parse_volume(output: &str) -> Option<(f32, bool)> {
    let mut parts = output.trim().strip_prefix("Volume: ")?.split_whitespace();
    let volume = parts.next()?.parse().ok()?;
    Some((volume, parts.any(|part| part == "[MUTED]")))
}

/// Extracts `node.description` from `wpctl inspect` output.
fn parse_description(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.trim_start_matches([' ', '*']).strip_prefix("node.description = "))
        .map(|value| value.trim_matches('"').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_volume_and_mute() {
        assert_eq!(parse_volume("Volume: 0.69\n"), Some((0.69, false)));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some((1.0, true)));
        assert_eq!(parse_volume("not a volume"), None);
    }

    #[test]
    fn parses_node_description() {
        let output = "id 79, type PipeWire:Interface:Node\n    node.name = \"alsa_input\"\n  * node.description = \"HyperX QuadCast S Analog Stereo\"\n";
        assert_eq!(parse_description(output).as_deref(), Some("HyperX QuadCast S Analog Stereo"));
        assert_eq!(parse_description("id 1\n"), None);
    }
}
