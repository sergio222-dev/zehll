use std::path::PathBuf;
use std::time::Duration;

const MAX_TODOS: usize = 5;

/// Time since boot, from `/proc/uptime`.
pub fn uptime() -> Option<Duration> {
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    let seconds: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(seconds))
}

/// Formats a duration like `3d, 2h, 50m`, omitting leading zero units.
pub fn format_uptime(uptime: Duration) -> String {
    let minutes = uptime.as_secs() / 60;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h, {minutes}m"),
        _ => format!("{days}d, {hours}h, {minutes}m"),
    }
}

/// Lines describing the unfinished tasks of the quickshell `ii` todo list.
pub fn pending_todos() -> Vec<String> {
    let todos: Vec<serde_json::Value> = std::fs::read_to_string(todo_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default();
    let pending: Vec<&str> = todos
        .iter()
        .filter(|todo| todo["done"].as_bool() != Some(true))
        .filter_map(|todo| todo["content"].as_str())
        .collect();

    if pending.is_empty() {
        return vec![String::from("No pending tasks")];
    }
    let mut lines: Vec<String> = pending
        .iter()
        .take(MAX_TODOS)
        .enumerate()
        .map(|(index, content)| format!("{}. {content}", index + 1))
        .collect();
    if pending.len() > MAX_TODOS {
        lines.push(format!("... and {} more", pending.len() - MAX_TODOS));
    }
    lines
}

fn todo_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("quickshell/user/todo.json")
}
