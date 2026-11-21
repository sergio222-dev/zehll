use std::path::PathBuf;

use crate::nightlight;

/// Settings toggled from the panel that survive restarts.
#[derive(Debug)]
pub struct State {
    pub keep_awake: bool,
    pub do_not_disturb: bool,
    /// Color temperature night light uses, in kelvin.
    pub night_temperature: u32,
    /// Night light follows its schedule.
    pub night_automatic: bool,
    /// The anti-flashbang screen shader is on.
    pub content_adjustment: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            keep_awake: false,
            do_not_disturb: false,
            night_temperature: nightlight::NIGHT,
            night_automatic: true,
            content_adjustment: false,
        }
    }
}

impl State {
    pub fn load() -> Self {
        let state: serde_json::Value = std::fs::read_to_string(path())
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        let default = Self::default();
        let flag = |key: &str, fallback: bool| state[key].as_bool().unwrap_or(fallback);
        Self {
            keep_awake: flag("keep_awake", default.keep_awake),
            do_not_disturb: flag("do_not_disturb", default.do_not_disturb),
            night_temperature: state["night_temperature"].as_u64().map_or(default.night_temperature, |kelvin| kelvin as u32),
            night_automatic: flag("night_automatic", default.night_automatic),
            content_adjustment: flag("content_adjustment", default.content_adjustment),
        }
    }

    pub fn save(&self) {
        let path = path();
        let content = serde_json::json!({
            "keep_awake": self.keep_awake,
            "do_not_disturb": self.do_not_disturb,
            "night_temperature": self.night_temperature,
            "night_automatic": self.night_automatic,
            "content_adjustment": self.content_adjustment,
        })
        .to_string();
        let result = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, content));
        if let Err(err) = result {
            eprintln!("zehll: cannot save {}: {err}", path.display());
        }
    }
}

fn path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("zehll/state.json")
}
