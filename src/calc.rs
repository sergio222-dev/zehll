//! Calculator page of the launcher: `qalc` does the math, units and currencies.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

/// Past calculations kept.
pub const HISTORY_LIMIT: usize = 25;
const TIMEOUT: Duration = Duration::from_secs(3);

/// A calculation as qalc reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    /// The expression as qalc understood it, e.g. `900 days`.
    pub expression: String,
    /// `=`, or `≈` for a rounded result.
    pub sign: &'static str,
    pub result: String,
}

/// A remembered calculation: what was typed and its result.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub input: String,
    pub result: String,
}

/// Reads qalc's output: warnings are skipped, errors give no answer.
pub fn parse(output: &str) -> Option<Answer> {
    let mut lines = output.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.clone().any(|line| line.starts_with("error:")) {
        return None;
    }
    let line = lines.rfind(|line| !line.starts_with("warning:"))?;
    // The last sign splits them: the expression itself may hold `=` in a comparison.
    let split = [" = ", " ≈ "].into_iter().filter_map(|sign| line.rfind(sign).map(|at| (at, sign))).max_by_key(|(at, _)| *at);
    Some(match split {
        Some((at, sign)) => Answer {
            expression: line[..at].to_string(),
            sign: sign.trim(),
            result: line[at + sign.len()..].to_string(),
        },
        None => Answer { expression: line.to_string(), sign: "=", result: line.to_string() },
    })
}

/// Adds `entry` as the newest, dropping an older one with the same input and the oldest past the limit.
pub fn remember(history: &mut Vec<Entry>, entry: Entry) {
    history.retain(|old| old.input != entry.input);
    history.insert(0, entry);
    history.truncate(HISTORY_LIMIT);
}

/// Evaluates `input` with qalc; `None` when qalc fails, is missing or takes too long.
pub async fn evaluate(input: String) -> Option<Answer> {
    // Without this qalc stops to ask about updating old exchange rates.
    let run = Command::new("qalc").args(["-s", "upxrates 0", &input]).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    let output = tokio::time::timeout(TIMEOUT, run).await.ok()?.ok()?;
    parse(&String::from_utf8_lossy(&output.stdout))
}

/// History from `$XDG_STATE_HOME/zehll/calculator.json`, newest first.
pub fn load_history() -> Vec<Entry> {
    std::fs::read_to_string(history_path())
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .map(|json| history_from_json(&json))
        .unwrap_or_default()
}

pub fn save_history(history: &[Entry]) {
    let path = history_path();
    let json = serde_json::json!({
        "history": history.iter().map(|entry| serde_json::json!({ "input": entry.input, "result": entry.result })).collect::<Vec<_>>()
    });
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, json.to_string()));
    if let Err(err) = written {
        eprintln!("zehll: cannot save {}: {err}", path.display());
    }
}

fn history_from_json(json: &serde_json::Value) -> Vec<Entry> {
    let Some(history) = json["history"].as_array() else {
        return Vec::new();
    };
    history
        .iter()
        .filter_map(|entry| Some(Entry { input: entry["input"].as_str()?.to_string(), result: entry["result"].as_str()?.to_string() }))
        .take(HISTORY_LIMIT)
        .collect()
}

fn history_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("zehll/calculator.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(expression: &str, sign: &'static str, result: &str) -> Option<Answer> {
        Some(Answer { expression: expression.to_string(), sign, result: result.to_string() })
    }

    #[test]
    fn splits_the_expression_from_the_result() {
        assert_eq!(parse("900 days = 7.776E16 ns\n"), answer("900 days", "=", "7.776E16 ns"));
        assert_eq!(parse("(4 × 22) + 1 = 89\n"), answer("(4 × 22) + 1", "=", "89"));
        assert_eq!(parse("sqrt(2) ≈ 1.414213562\n"), answer("sqrt(2)", "≈", "1.414213562"));
    }

    #[test]
    fn warnings_are_skipped() {
        let output = "warning: It has been 55 days since the exchange rates last were updated.\n(2 × 1000) USD ≈ €1757.7782\n";
        assert_eq!(parse(output), answer("(2 × 1000) USD", "≈", "€1757.7782"));
    }

    #[test]
    fn errors_and_empty_output_give_no_answer() {
        assert_eq!(parse("error: Unknown function \"foo\".\n"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("warning: only a warning\n"), None);
    }

    #[test]
    fn a_bare_value_is_its_own_result() {
        assert_eq!(parse("42\n"), answer("42", "=", "42"));
    }

    fn entry(input: &str, result: &str) -> Entry {
        Entry { input: input.to_string(), result: result.to_string() }
    }

    #[test]
    fn history_is_newest_first_without_repeats() {
        let mut history = vec![entry("1+1", "2"), entry("2*3", "6")];
        remember(&mut history, entry("2*3", "6"));
        remember(&mut history, entry("10/4", "2.5"));
        assert_eq!(history, [entry("10/4", "2.5"), entry("2*3", "6"), entry("1+1", "2")]);
    }

    #[test]
    fn history_keeps_the_last_twenty_five() {
        let mut history = Vec::new();
        for n in 0..30 {
            remember(&mut history, entry(&n.to_string(), &n.to_string()));
        }
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!((history[0].input.as_str(), history[24].input.as_str()), ("29", "5"));
    }

    #[test]
    fn history_is_read_back_skipping_malformed_entries() {
        let json = serde_json::json!({ "history": [{ "input": "1+1", "result": "2" }, { "input": 3 }] });
        assert_eq!(history_from_json(&json), [entry("1+1", "2")]);
    }
}
