//! Hyprland keybindings that have a description, for the launcher's Keybinds page.

#[derive(Clone, Debug, PartialEq)]
pub struct Keybind {
    /// Modifiers and key, e.g. `SUPER + SHIFT + E`.
    pub keys: String,
    pub description: String,
}

/// Binds from `hyprctl binds -j` that have a description, outside submaps and not on the mouse.
pub fn parse(binds: &serde_json::Value) -> Vec<Keybind> {
    let Some(binds) = binds.as_array() else {
        return Vec::new();
    };
    binds
        .iter()
        .filter(|bind| bind["has_description"].as_bool() == Some(true) && bind["submap"].as_str().unwrap_or_default().is_empty() && bind["mouse"].as_bool() != Some(true))
        .filter_map(|bind| {
            let description = bind["description"].as_str().filter(|description| !description.is_empty())?;
            Some(Keybind { keys: keys(bind), description: description.to_string() })
        })
        .collect()
}

// Hyprland modifier mask bits, in the order they are shown.
const MODIFIERS: [(u64, &str); 4] = [(64, "SUPER"), (4, "CONTROL"), (8, "ALT"), (1, "SHIFT")];

fn keys(bind: &serde_json::Value) -> String {
    let modmask = bind["modmask"].as_u64().unwrap_or_default();
    let key = match bind["key"].as_str().unwrap_or_default() {
        "" => format!("CODE:{}", bind["keycode"].as_u64().unwrap_or_default()),
        key => key.to_uppercase(),
    };
    MODIFIERS
        .iter()
        .filter(|(bit, _)| modmask & bit != 0)
        .map(|(_, name)| (*name).to_string())
        .chain(std::iter::once(key))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Every described bind of the running Hyprland, in its order.
pub fn load() -> Vec<Keybind> {
    crate::hyprland::binds().map(|binds| parse(&binds)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bind(modmask: u64, key: &str, keycode: u64, description: &str) -> serde_json::Value {
        json!({
            "modmask": modmask, "key": key, "keycode": keycode, "description": description,
            "has_description": !description.is_empty(), "submap": "", "mouse": false,
        })
    }

    #[test]
    fn modifiers_come_before_the_key_in_capitals() {
        let binds = json!([
            bind(64, "Return", 0, "Launch terminal"),
            bind(68, "Return", 0, "Launch floating terminal"),
            bind(65, "e", 0, "Browse files"),
            bind(12, "Delete", 0, "Task manager"),
            bind(73, "s", 0, "Screenshot"),
            bind(0, "XF86AudioMute", 0, "Mute"),
        ]);
        let keys: Vec<String> = parse(&binds).into_iter().map(|bind| bind.keys).collect();
        assert_eq!(
            keys,
            ["SUPER + RETURN", "SUPER + CONTROL + RETURN", "SUPER + SHIFT + E", "CONTROL + ALT + DELETE", "SUPER + ALT + SHIFT + S", "XF86AUDIOMUTE"]
        );
        assert_eq!(parse(&binds)[0].description, "Launch terminal");
    }

    #[test]
    fn keycode_binds_show_their_code() {
        assert_eq!(parse(&json!([bind(64, "", 10, "Workspace 1")]))[0].keys, "SUPER + CODE:10");
    }

    #[test]
    fn only_described_binds_outside_submaps_and_off_the_mouse_are_listed() {
        let mut submap = bind(64, "R", 0, "Resize");
        submap["submap"] = json!("resize");
        let mut mouse = bind(64, "mouse:272", 0, "Move window");
        mouse["mouse"] = json!(true);
        let binds = json!([bind(64, "Q", 0, ""), submap, mouse, bind(64, "W", 0, "Close window")]);
        assert_eq!(parse(&binds), [Keybind { keys: String::from("SUPER + W"), description: String::from("Close window") }]);
        assert!(parse(&json!({})).is_empty());
    }
}
