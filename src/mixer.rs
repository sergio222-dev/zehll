use iced::futures::Stream as FuturesStream;
use tokio::process::Command;

use crate::audio;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mixer {
    pub streams: Vec<Stream>,
    pub sinks: Vec<Sink>,
    pub default_sink: String,
}

/// Streams the mixer state while the audio sub-panel is open.
pub fn events() -> impl FuturesStream<Item = Mixer> {
    audio::watch(read)
}

async fn read() -> Mixer {
    let streams = pactl(&["-f", "json", "list", "sink-inputs"]).await.map(|json| parse_streams(&json)).unwrap_or_default();
    let sinks = pactl(&["-f", "json", "list", "sinks"]).await.map(|json| parse_sinks(&json)).unwrap_or_default();
    let default_sink = pactl(&["get-default-sink"]).await.map(|name| name.trim().to_owned()).unwrap_or_default();
    Mixer { streams, sinks, default_sink }
}

pub async fn set_volume(index: u32, percent: u32) {
    run(&["set-sink-input-volume", &index.to_string(), &format!("{percent}%")]).await;
}

pub async fn set_default_sink(name: String) {
    run(&["set-default-sink", &name]).await;
}

async fn pactl(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl").args(args).output().await.ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn run(args: &[&str]) {
    if let Err(err) = Command::new("pactl").args(args).status().await {
        eprintln!("zehll: pactl {} failed: {err}", args.join(" "));
    }
}

/// An application playing audio (a PulseAudio sink input).
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub index: u32,
    pub app: String,
    pub title: String,
    /// Percent of the first channel; PipeWire keeps channels together for apps.
    pub volume: u32,
    /// Executable name, used to look the app icon up.
    pub binary: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sink {
    pub name: String,
    pub description: String,
}

/// Parses `pactl -f json list sink-inputs`.
fn parse_streams(json: &str) -> Vec<Stream> {
    let inputs: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    inputs
        .iter()
        .filter_map(|input| {
            let properties = &input["properties"];
            let media = properties["media.name"].as_str().unwrap_or_default();
            let (app, title) = match properties["application.name"].as_str() {
                Some(app) => (app.to_owned(), media.to_owned()),
                None => (media.to_owned(), String::new()),
            };
            let volume = input["volume"].as_object()?.values().next()?["value_percent"].as_str()?;
            Some(Stream {
                index: input["index"].as_u64()? as u32,
                app,
                title,
                volume: volume.trim_end_matches('%').parse().ok()?,
                binary: properties["application.process.binary"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

/// Parses `pactl -f json list sinks`.
fn parse_sinks(json: &str) -> Vec<Sink> {
    let sinks: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    sinks
        .iter()
        .filter_map(|sink| Some(Sink { name: sink["name"].as_str()?.to_owned(), description: sink["description"].as_str()?.to_owned() }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAMS: &str = r#"[
        {"index": 37, "sink": 123, "mute": false, "volume": {"mono": {"value": 65536, "value_percent": "100%", "db": "0.00 dB"}},
         "properties": {"media.name": "Virtual Source output"}},
        {"index": 17748, "sink": 123, "mute": false, "volume": {"front-left": {"value": 47024, "value_percent": "72%", "db": "-8.65 dB"}, "front-right": {"value": 47024, "value_percent": "72%", "db": "-8.65 dB"}},
         "properties": {"application.name": "Firefox", "application.process.binary": "firefox", "media.name": "Home / X"}}
    ]"#;

    #[test]
    fn parses_sink_inputs() {
        let streams = parse_streams(STREAMS);
        assert_eq!(
            streams[1],
            Stream { index: 17748, app: String::from("Firefox"), title: String::from("Home / X"), volume: 72, binary: Some(String::from("firefox")) }
        );
        // Without an application name, the media name stands in for it.
        assert_eq!((streams[0].app.as_str(), streams[0].title.as_str(), streams[0].volume), ("Virtual Source output", "", 100));
        assert!(parse_streams("not json").is_empty());
    }

    #[test]
    fn parses_sinks() {
        let sinks = parse_sinks(r#"[{"index": 18372, "name": "bluez_output.C0_EA_C3_18_57_B8.1", "description": "Soundcore Life Q20"}]"#);
        assert_eq!(sinks, vec![Sink { name: String::from("bluez_output.C0_EA_C3_18_57_B8.1"), description: String::from("Soundcore Life Q20") }]);
    }
}
