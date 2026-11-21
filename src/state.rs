use std::path::PathBuf;

/// Settings toggled from the panel that survive restarts.
#[derive(Debug, Default)]
pub struct State {
    pub keep_awake: bool,
    pub do_not_disturb: bool,
}

impl State {
    pub fn load() -> Self {
        let state: serde_json::Value = std::fs::read_to_string(path())
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self {
            keep_awake: state["keep_awake"].as_bool().unwrap_or(false),
            do_not_disturb: state["do_not_disturb"].as_bool().unwrap_or(false),
        }
    }

    pub fn save(&self) {
        let path = path();
        let content = serde_json::json!({ "keep_awake": self.keep_awake, "do_not_disturb": self.do_not_disturb }).to_string();
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
