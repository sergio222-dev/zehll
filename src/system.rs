use std::process::Command;
use std::time::Duration;

use crate::todo;

const MAX_TODOS: usize = 5;

/// Starts a program that outlives the call and reaps it when it exits, so it never stays a zombie.
pub fn spawn(program: &str, args: &[&str]) -> std::io::Result<()> {
    let mut child = Command::new(program).args(args).spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// States of this process's direct children, from `/proc/self/task/*/children`.
    fn child_states() -> Vec<char> {
        let tasks = std::fs::read_dir("/proc/self/task").unwrap();
        let children = tasks.flatten().filter_map(|task| std::fs::read_to_string(task.path().join("children")).ok());
        let pids: Vec<String> = children.flat_map(|line| line.split_whitespace().map(String::from).collect::<Vec<_>>()).collect();
        pids.iter()
            .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
            .filter_map(|stat| stat.rsplit_once(") ").and_then(|(_, rest)| rest.chars().next()))
            .collect()
    }

    #[test]
    fn spawned_programs_are_reaped() {
        spawn("true", &[]).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!child_states().contains(&'Z'), "a spawned program stayed a zombie");
        assert!(spawn("zehll-no-such-program", &[]).is_err());
    }
}
