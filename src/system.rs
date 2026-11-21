use std::time::Duration;

use crate::todo;

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

#[derive(Debug, Default)]
pub struct Todos {
    pub pending: usize,
    pub lines: Vec<String>,
}

/// Unfinished tasks of the quickshell `ii` todo list.
pub fn pending_todos() -> Todos {
    let todos = todo::load();
    let pending: Vec<&str> = todos.iter().filter(|todo| !todo.done).map(|todo| todo.content.as_str()).collect();

    if pending.is_empty() {
        return Todos { pending: 0, lines: vec![String::from("No pending tasks")] };
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
    Todos { pending: pending.len(), lines }
}
