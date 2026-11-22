//! Installed applications from their desktop entries, and how often each is launched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::tray;

#[derive(Clone, Debug)]
pub struct App {
    /// Desktop file id, e.g. `org.gnome.Nautilus.desktop`.
    pub id: String,
    pub name: String,
    /// Comment, else GenericName.
    pub description: String,
    pub exec: String,
    pub icon_name: String,
    pub icon: Option<tray::Icon>,
    pub terminal: bool,
    pub wm_class: Option<String>,
    /// Desktop actions as (name, exec), in file order.
    pub actions: Vec<(String, String)>,
}

/// Parses a desktop entry; `None` for anything that should not be listed.
pub fn parse(id: &str, content: &str) -> Option<App> {
    // Unlocalized keys of each group, by group name.
    let mut groups: HashMap<&str, HashMap<&str, &str>> = HashMap::new();
    let mut group = "";
    for line in content.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            group = name;
        } else if let Some((key, value)) = line.split_once('=')
            && !line.starts_with('#')
        {
            groups.entry(group).or_default().entry(key.trim()).or_insert(value.trim());
        }
    }
    let entry = groups.get("Desktop Entry")?;
    let is_true = |key: &str| entry.get(key).is_some_and(|value| *value == "true");
    if entry.get("Type") != Some(&"Application") || is_true("NoDisplay") || is_true("Hidden") {
        return None;
    }
    let description = entry.get("Comment").or_else(|| entry.get("GenericName")).copied().unwrap_or_default();
    let actions = entry
        .get("Actions")
        .into_iter()
        .flat_map(|actions| actions.split(';'))
        .filter(|action| !action.is_empty())
        .filter_map(|action| {
            let group = groups.get(format!("Desktop Action {action}").as_str())?;
            Some((group.get("Name")?.to_string(), group.get("Exec")?.to_string()))
        })
        .collect();
    Some(App {
        id: id.to_string(),
        name: entry.get("Name")?.to_string(),
        description: description.to_string(),
        exec: entry.get("Exec")?.to_string(),
        icon_name: entry.get("Icon").copied().unwrap_or_default().to_string(),
        icon: None,
        terminal: is_true("Terminal"),
        wm_class: entry.get("StartupWMClass").map(|class| class.to_string()),
        actions,
    })
}

/// The command of an `Exec` key without its field codes (`%f`, `%U`...); `%%` becomes `%`.
pub fn command(exec: &str) -> String {
    let mut words = Vec::new();
    for word in exec.split_whitespace() {
        if word.len() == 2 && word.starts_with('%') && word != "%%" {
            continue;
        }
        words.push(word.replace("%%", "%"));
    }
    words.join(" ")
}

/// Whether a window of class `class` belongs to `app`.
pub fn owns_window(app: &App, class: &str) -> bool {
    if class.is_empty() {
        return false;
    }
    let class = class.to_lowercase();
    let id = app.id.strip_suffix(".desktop").unwrap_or(&app.id).to_lowercase();
    // Not by binary: entries sharing one (browser profiles, wrappers) would all claim its windows.
    app.wm_class.as_ref().is_some_and(|wm_class| wm_class.to_lowercase() == class) || id == class || id.rsplit('.').next() == Some(class.as_str())
}

/// Shell command starting `app`, or its first desktop action when `secondary`.
/// Terminal apps run inside `terminal`.
pub fn launch_command(app: &App, secondary: bool, terminal: &str) -> String {
    let exec = match (secondary, app.actions.first()) {
        (true, Some((_, exec))) => exec,
        _ => &app.exec,
    };
    let command = command(exec);
    if app.terminal { format!("{terminal} -e {command}") } else { command }
}

/// `$TERMINAL`, else kitty.
pub fn terminal() -> String {
    std::env::var("TERMINAL").ok().filter(|terminal| !terminal.is_empty()).unwrap_or_else(|| String::from("kitty"))
}

/// Every listed app, sorted by name. Earlier data directories win for the same desktop id.
pub fn load() -> Vec<App> {
    let mut apps: HashMap<String, App> = HashMap::new();
    for dir in data_dirs() {
        let root = dir.join("applications");
        let mut files = Vec::new();
        collect(&root, &mut files);
        for file in files {
            // Subdirectories become part of the id: kde/okular.desktop is kde-okular.desktop.
            let Some(id) = file.strip_prefix(&root).ok().and_then(|path| path.to_str()).map(|path| path.replace('/', "-")) else {
                continue;
            };
            if apps.contains_key(&id) {
                continue;
            }
            // An entry that is hidden still shadows the same id further down.
            let parsed = std::fs::read_to_string(&file).ok().and_then(|content| parse(&id, &content));
            apps.insert(id.clone(), parsed.unwrap_or_else(|| shadowed(&id)));
        }
    }
    let mut apps: Vec<App> = apps.into_values().filter(|app| !app.exec.is_empty()).collect();
    for app in &mut apps {
        app.icon = Some(app.icon_name.as_str()).filter(|name| !name.is_empty()).and_then(|name| tray::lookup(name, None));
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

// Placeholder for a hidden entry, dropped after loading.
fn shadowed(id: &str) -> App {
    App {
        id: id.to_string(),
        name: String::new(),
        description: String::new(),
        exec: String::new(),
        icon_name: String::new(),
        icon: None,
        terminal: false,
        wm_class: None,
        actions: Vec::new(),
    }
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "desktop") {
            files.push(path);
        }
    }
}

// XDG_DATA_HOME, then XDG_DATA_DIRS.
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|dirs| !dirs.is_empty()).unwrap_or_else(|| String::from("/usr/local/share:/usr/share"));
    std::iter::once(data_home).chain(data_dirs.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from)).collect()
}

/// Launch score of an app: grows with each launch and halves every `HALF_LIFE` days without one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub value: f64,
    /// Unix seconds of the last launch.
    pub last_used: i64,
}

pub const LAUNCH_INCREMENT: f64 = 0.01;
pub const HALF_LIFE_DAYS: f64 = 14.0;

impl Score {
    /// Value at `now` (unix seconds), decayed since the last launch.
    pub fn at(&self, now: i64) -> f64 {
        let days = (now - self.last_used).max(0) as f64 / (24.0 * 60.0 * 60.0);
        self.value * 0.5f64.powf(days / HALF_LIFE_DAYS)
    }
}

/// Records a launch at `now`.
pub fn bump(scores: &mut HashMap<String, Score>, id: &str, now: i64) {
    let value = scores.get(id).map_or(0.0, |score| score.at(now)) + LAUNCH_INCREMENT;
    scores.insert(id.to_string(), Score { value, last_used: now });
}

/// Launch scores by desktop id, from `$XDG_STATE_HOME/zehll/launcher.json`.
pub fn load_scores() -> HashMap<String, Score> {
    std::fs::read_to_string(scores_path())
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .map(|json| scores_from_json(&json))
        .unwrap_or_default()
}

pub fn save_scores(scores: &HashMap<String, Score>) {
    let path = scores_path();
    let json = serde_json::json!({
        "scores": scores.iter().map(|(id, score)| (id.clone(), serde_json::json!({ "value": score.value, "last_used": score.last_used }))).collect::<serde_json::Map<_, _>>()
    });
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, json.to_string()));
    if let Err(err) = written {
        eprintln!("zehll: cannot save {}: {err}", path.display());
    }
}

fn scores_from_json(json: &serde_json::Value) -> HashMap<String, Score> {
    let Some(scores) = json["scores"].as_object() else {
        return HashMap::new();
    };
    scores
        .iter()
        .filter_map(|(id, score)| Some((id.clone(), Score { value: score["value"].as_f64()?, last_used: score["last_used"].as_i64()? })))
        .collect()
}

fn scores_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("zehll/launcher.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "[Desktop Entry]
Type=Application
Name=Firefox
Name[es]=Firefox ES
GenericName=Web Browser
Comment=Browse the Web
Exec=/usr/lib/firefox/firefox %u
Icon=firefox
StartupWMClass=firefox
Actions=new-window;new-private-window;

[Desktop Action new-window]
Name=New Window
Exec=/usr/lib/firefox/firefox --new-window %u

[Desktop Action new-private-window]
Name=New Private Window
Exec=/usr/lib/firefox/firefox --private-window %u
";

    #[test]
    fn parses_name_description_exec_icon_and_actions() {
        let app = parse("firefox.desktop", FIREFOX).unwrap();
        assert_eq!((app.id.as_str(), app.name.as_str(), app.description.as_str()), ("firefox.desktop", "Firefox", "Browse the Web"));
        assert_eq!((app.exec.as_str(), app.icon_name.as_str(), app.terminal), ("/usr/lib/firefox/firefox %u", "firefox", false));
        assert_eq!(app.wm_class.as_deref(), Some("firefox"));
        assert_eq!(
            app.actions,
            [
                (String::from("New Window"), String::from("/usr/lib/firefox/firefox --new-window %u")),
                (String::from("New Private Window"), String::from("/usr/lib/firefox/firefox --private-window %u")),
            ]
        );
    }

    #[test]
    fn falls_back_to_the_generic_name_without_a_comment() {
        let app = parse("btop.desktop", "[Desktop Entry]\nType=Application\nName=btop++\nGenericName=System Monitor\nExec=btop\nTerminal=true\n").unwrap();
        assert_eq!((app.description.as_str(), app.terminal), ("System Monitor", true));
    }

    #[test]
    fn hidden_entries_and_non_applications_are_skipped() {
        assert!(parse("a.desktop", "[Desktop Entry]\nType=Application\nName=A\nExec=a\nNoDisplay=true\n").is_none());
        assert!(parse("b.desktop", "[Desktop Entry]\nType=Application\nName=B\nExec=b\nHidden=true\n").is_none());
        assert!(parse("c.desktop", "[Desktop Entry]\nType=Link\nName=C\nURL=https://example.com\n").is_none());
        assert!(parse("d.desktop", "[Desktop Entry]\nType=Application\nName=D\n").is_none());
    }

    #[test]
    fn keys_of_other_groups_are_ignored() {
        let app = parse("x.desktop", "[Other]\nName=Wrong\n[Desktop Entry]\nType=Application\nName=Right\nExec=x\n").unwrap();
        assert_eq!(app.name, "Right");
    }

    #[test]
    fn field_codes_are_removed_from_commands() {
        assert_eq!(command("/usr/lib/firefox/firefox %u"), "/usr/lib/firefox/firefox");
        assert_eq!(command("code --new-window %F"), "code --new-window");
        assert_eq!(command("printf 100%%"), "printf 100%");
        assert_eq!(command("env A=1 app %i %c %k --flag"), "env A=1 app --flag");
    }

    fn app(id: &str, exec: &str, wm_class: Option<&str>) -> App {
        App {
            id: id.to_string(),
            name: String::new(),
            description: String::new(),
            exec: exec.to_string(),
            icon_name: String::new(),
            icon: None,
            terminal: false,
            wm_class: wm_class.map(str::to_string),
            actions: Vec::new(),
        }
    }

    #[test]
    fn windows_are_matched_by_class_or_id() {
        assert!(owns_window(&app("zen.desktop", "zen-browser %u", Some("zen")), "zen"));
        assert!(owns_window(&app("org.gnome.Nautilus.desktop", "nautilus --new-window", None), "org.gnome.Nautilus"));
        assert!(owns_window(&app("com.mitchellh.ghostty.desktop", "ghostty", None), "com.mitchellh.ghostty"));
        assert!(owns_window(&app("spotify.desktop", "/usr/bin/spotify --uri=%U", None), "Spotify"));
        assert!(!owns_window(&app("kitty.desktop", "kitty", None), "firefox"));
        assert!(!owns_window(&app("x.desktop", "x", None), ""));
    }

    #[test]
    fn entries_sharing_a_binary_do_not_own_its_windows() {
        // A Firefox profile launcher opens its own window instead of focusing the default one.
        assert!(!owns_window(&app("firefox-work.desktop", "firefox -P work", None), "firefox"));
    }

    #[test]
    fn launch_commands_use_the_first_action_and_the_terminal() {
        let firefox = parse("firefox.desktop", FIREFOX).unwrap();
        assert_eq!(launch_command(&firefox, false, "kitty"), "/usr/lib/firefox/firefox");
        assert_eq!(launch_command(&firefox, true, "kitty"), "/usr/lib/firefox/firefox --new-window");
        let btop = parse("btop.desktop", "[Desktop Entry]\nType=Application\nName=btop++\nExec=btop %f\nTerminal=true\n").unwrap();
        assert_eq!(launch_command(&btop, false, "foot"), "foot -e btop");
        // Without actions the secondary launch is the app itself.
        assert_eq!(launch_command(&btop, true, "foot"), "foot -e btop");
    }

    #[test]
    fn launches_add_up_and_decay_by_half_every_two_weeks() {
        let day = 24 * 60 * 60;
        let mut scores = HashMap::new();
        bump(&mut scores, "firefox.desktop", 0);
        bump(&mut scores, "firefox.desktop", 0);
        let score = scores["firefox.desktop"];
        assert!((score.at(0) - 0.02).abs() < 1e-9);
        assert!((score.at(14 * day) - 0.01).abs() < 1e-9);
        bump(&mut scores, "firefox.desktop", 14 * day);
        assert!((scores["firefox.desktop"].at(14 * day) - 0.02).abs() < 1e-9);
        assert_eq!(scores["firefox.desktop"].last_used, 14 * day);
    }

    #[test]
    fn scores_are_read_back_skipping_malformed_entries() {
        let json = serde_json::json!({ "scores": { "a.desktop": { "value": 0.03, "last_used": 5 }, "b.desktop": { "value": "x" } } });
        let scores = scores_from_json(&json);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores["a.desktop"], Score { value: 0.03, last_used: 5 });
        assert!(scores_from_json(&serde_json::json!([])).is_empty());
    }
}
