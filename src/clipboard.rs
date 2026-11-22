//! Clipboard access for the launcher: `wl-copy`, and the `cliphist` history with Zehll's pins.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use iced::widget::image;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(5);
/// Lines of a text entry shown in the preview.
const PREVIEW_LINES: usize = 40;
const PREVIEW_CHARS: usize = 4000;
/// Points cliphist at another database instead of the user's, for testing Zehll by hand.
const DB_VAR: &str = "ZEHLL_CLIPHIST_DB";

/// One `cliphist list` line: its id and the preview cliphist keeps of the content.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub preview: String,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text,
    /// From cliphist's `[[ binary data 4 KiB png 412x150 ]]` preview.
    Binary { size: String, format: String, dimensions: Option<String> },
}

impl Entry {
    pub fn is_image(&self) -> bool {
        matches!(&self.kind, Kind::Binary { dimensions: Some(_), .. })
    }

    /// The line as `cliphist list` printed it; `cliphist delete` reads its id from it.
    pub fn line(&self) -> String {
        format!("{}\t{}", self.id, self.preview)
    }

    pub fn mime(&self) -> String {
        match &self.kind {
            Kind::Text => String::from("text/plain"),
            Kind::Binary { format, dimensions: Some(_), .. } => format!("image/{format}"),
            Kind::Binary { format, .. } => format!("application/{format}"),
        }
    }

    /// `Text`, or `Image` with its dimensions and size.
    pub fn type_label(&self) -> String {
        match &self.kind {
            Kind::Text => String::from("Text"),
            Kind::Binary { size, dimensions: Some(dimensions), .. } => format!("Image {dimensions}, {size}"),
            Kind::Binary { size, .. } => format!("Binary, {size}"),
        }
    }

    /// What a pin remembers. cliphist gives content copied again a new id, but the same
    /// preview, so pins are keyed by the preview.
    pub fn pin_key(&self) -> &str {
        &self.preview
    }
}

/// Reads one `cliphist list` line: id, tab, preview.
pub fn parse_line(line: &str) -> Option<Entry> {
    let (id, preview) = line.split_once('\t')?;
    let id = id.trim().parse().ok()?;
    Some(Entry { id, preview: preview.to_string(), kind: binary_kind(preview).unwrap_or(Kind::Text) })
}

fn binary_kind(preview: &str) -> Option<Kind> {
    let inner = preview.strip_prefix("[[ binary data ")?.strip_suffix(" ]]")?;
    let words: Vec<&str> = inner.split(' ').collect();
    match words.as_slice() {
        [amount, unit, format] => Some(Kind::Binary { size: format!("{amount} {unit}"), format: format.to_string(), dimensions: None }),
        [amount, unit, format, dimensions] => Some(Kind::Binary {
            size: format!("{amount} {unit}"),
            format: format.to_string(),
            dimensions: Some(dimensions.to_string()),
        }),
        _ => None,
    }
}

/// History newest first, as `cliphist list` prints it.
pub fn parse_list(output: &str) -> Vec<Entry> {
    output.lines().filter_map(parse_line).collect()
}

/// Whether every word of `query` is in `preview`, ignoring case.
pub fn matches(query: &str, preview: &str) -> bool {
    let preview = preview.to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| preview.contains(word))
}

/// Short FNV-1a hash of an entry's content, shown in its details.
pub fn content_hash(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

/// A decoded entry for the preview.
#[derive(Clone, Debug)]
pub struct Decoded {
    pub hash: String,
    /// The start of a text entry.
    pub text: Option<String>,
    pub image: Option<image::Handle>,
}

impl Decoded {
    pub fn new(entry: &Entry, bytes: &[u8]) -> Self {
        let hash = content_hash(bytes);
        match entry.kind {
            Kind::Text => {
                let text: String = String::from_utf8_lossy(bytes).lines().take(PREVIEW_LINES).collect::<Vec<_>>().join("\n").chars().take(PREVIEW_CHARS).collect();
                Self { hash, text: Some(text), image: None }
            }
            Kind::Binary { .. } if entry.is_image() => Self { hash, text: None, image: Some(image::Handle::from_bytes(bytes.to_vec())) },
            Kind::Binary { .. } => Self { hash, text: None, image: None },
        }
    }
}

/// A cliphist run: its arguments and what it reads on stdin.
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    pub args: Vec<String>,
    pub stdin: Option<String>,
}

impl Invocation {
    fn new(db: Option<&str>, args: &[&str], stdin: Option<String>) -> Self {
        let db = db.into_iter().flat_map(|db| ["-db-path", db]);
        Self { args: db.chain(args.iter().copied()).map(String::from).collect(), stdin }
    }
}

pub fn list_command(db: Option<&str>) -> Invocation {
    Invocation::new(db, &["list"], None)
}

pub fn decode_command(db: Option<&str>, entry: &Entry) -> Invocation {
    Invocation::new(db, &["decode", &entry.id.to_string()], None)
}

pub fn delete_command(db: Option<&str>, entry: &Entry) -> Invocation {
    Invocation::new(db, &["delete"], Some(format!("{}\n", entry.line())))
}

pub fn wipe_command(db: Option<&str>) -> Invocation {
    Invocation::new(db, &["wipe"], None)
}

fn db_override() -> Option<String> {
    std::env::var(DB_VAR).ok().filter(|db| !db.is_empty())
}

/// Runs cliphist and returns its stdout; `None` when it fails, is missing or takes too long.
async fn cliphist(invocation: Invocation) -> Option<Vec<u8>> {
    let run = async {
        let mut child = Command::new("cliphist")
            .args(&invocation.args)
            .stdin(if invocation.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        if let (Some(input), Some(mut stdin)) = (&invocation.stdin, child.stdin.take()) {
            stdin.write_all(input.as_bytes()).await.ok()?;
        }
        child.wait_with_output().await.ok()
    };
    let output = tokio::time::timeout(TIMEOUT, run).await.ok()??;
    if !output.status.success() {
        eprintln!("zehll: cliphist {} failed: {}", invocation.args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
        return None;
    }
    Some(output.stdout)
}

pub async fn list() -> Vec<Entry> {
    let output = cliphist(list_command(db_override().as_deref())).await.unwrap_or_default();
    parse_list(&String::from_utf8_lossy(&output))
}

pub async fn decode(entry: Entry) -> Option<Decoded> {
    let bytes = cliphist(decode_command(db_override().as_deref(), &entry)).await?;
    Some(Decoded::new(&entry, &bytes))
}

pub async fn delete(entry: Entry) {
    cliphist(delete_command(db_override().as_deref(), &entry)).await;
}

pub async fn wipe() {
    cliphist(wipe_command(db_override().as_deref())).await;
}

/// Puts `entry` back on the clipboard: its decoded bytes into `wl-copy`, with the mime type
/// for binary entries so images stay images.
pub async fn copy_entry(entry: Entry) {
    let Some(bytes) = cliphist(decode_command(db_override().as_deref(), &entry)).await else {
        return;
    };
    let mut command = Command::new("wl-copy");
    if !matches!(entry.kind, Kind::Text) {
        command.args(["--type", &entry.mime()]);
    }
    let Ok(mut child) = command.stdin(Stdio::piped()).spawn() else {
        eprintln!("zehll: cannot run wl-copy");
        return;
    };
    // The pipe closes when this `if` ends, so wl-copy sees the end of the data.
    if let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(&bytes).await
    {
        eprintln!("zehll: cannot write to wl-copy: {err}");
    }
    // Awaited so the exited wl-copy is reaped; it forks to keep serving the selection.
    if let Err(err) = child.wait().await {
        eprintln!("zehll: wl-copy failed: {err}");
    }
}

/// Puts `text` on the clipboard with `wl-copy`.
pub async fn copy(text: String) {
    // Awaited so the exited wl-copy is reaped; it forks to keep serving the selection.
    match Command::new("wl-copy").arg("--").arg(&text).status().await {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("zehll: wl-copy failed: {status}"),
        Err(err) => eprintln!("zehll: cannot run wl-copy: {err}"),
    }
}

/// Pins `key`, or unpins it when pinned.
pub fn toggle_pin(pins: &mut Vec<String>, key: &str) {
    match pins.iter().position(|pin| pin == key) {
        Some(at) => {
            pins.remove(at);
        }
        None => pins.insert(0, key.to_string()),
    }
}

/// Pins from `$XDG_STATE_HOME/zehll/clipboard.json`.
pub fn load_pins() -> Vec<String> {
    load_pins_from(&pins_path())
}

pub fn save_pins(pins: &[String]) {
    save_pins_to(&pins_path(), pins);
}

fn load_pins_from(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| json["pins"].as_array().map(|pins| pins.iter().filter_map(|pin| pin.as_str().map(String::from)).collect()))
        .unwrap_or_default()
}

fn save_pins_to(path: &Path, pins: &[String]) {
    let json = serde_json::json!({ "pins": pins });
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, json.to_string()));
    if let Err(err) = written {
        eprintln!("zehll: cannot save {}: {err}", path.display());
    }
}

fn pins_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("zehll/clipboard.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(id: u64, preview: &str) -> Entry {
        Entry { id, preview: preview.to_string(), kind: Kind::Text }
    }

    #[test]
    fn list_lines_are_read_as_text_or_binary_entries() {
        let output = "23182\tclaude-subscription-directsdk\n23177\t[[ binary data 4 KiB png 412x150 ]]\n9\t[[ binary data 12 B octet-stream ]]\nnot a line\n7\ta\ttab\n";
        let entries = parse_list(output);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], text(23182, "claude-subscription-directsdk"));
        let image = &entries[1];
        assert_eq!(image.kind, Kind::Binary { size: String::from("4 KiB"), format: String::from("png"), dimensions: Some(String::from("412x150")) });
        assert!(image.is_image());
        assert_eq!((image.mime().as_str(), image.type_label().as_str()), ("image/png", "Image 412x150, 4 KiB"));
        assert_eq!((entries[2].is_image(), entries[2].type_label().as_str()), (false, "Binary, 12 B"));
        assert_eq!((entries[0].mime().as_str(), entries[0].type_label().as_str()), ("text/plain", "Text"));
        // Only the first tab separates the id.
        assert_eq!(entries[3], text(7, "a\ttab"));
    }

    #[test]
    fn the_filter_needs_every_word_ignoring_case() {
        assert!(matches("", "anything"));
        assert!(matches("SUB claude", "claude-subscription-directsdk"));
        assert!(!matches("claude zzz", "claude-subscription-directsdk"));
    }

    #[test]
    fn destructive_commands_name_the_entry_and_the_database() {
        let entry = text(23182, "claude-subscription-directsdk");
        assert_eq!(delete_command(None, &entry), Invocation { args: vec![String::from("delete")], stdin: Some(String::from("23182\tclaude-subscription-directsdk\n")) });
        assert_eq!(wipe_command(None), Invocation { args: vec![String::from("wipe")], stdin: None });
        assert_eq!(wipe_command(Some("/tmp/db")).args, ["-db-path", "/tmp/db", "wipe"]);
        assert_eq!(decode_command(Some("/tmp/db"), &entry).args, ["-db-path", "/tmp/db", "decode", "23182"]);
        assert_eq!(list_command(None).args, ["list"]);
    }

    #[test]
    fn decoded_text_keeps_its_start_and_a_stable_hash() {
        let entry = text(1, "hello");
        let decoded = Decoded::new(&entry, b"hello");
        assert_eq!(decoded.text.as_deref(), Some("hello"));
        assert_eq!(decoded.hash, content_hash(b"hello"));
        assert_eq!(decoded.hash.len(), 16);
        assert_ne!(content_hash(b"hello"), content_hash(b"hellp"));
        let long: String = (0..100).map(|n| format!("{n}\n")).collect();
        assert_eq!(Decoded::new(&entry, long.as_bytes()).text.unwrap().lines().count(), PREVIEW_LINES);
    }

    #[test]
    fn pins_toggle_and_survive_a_new_id_for_the_same_content() {
        let mut pins = Vec::new();
        toggle_pin(&mut pins, text(5, "token").pin_key());
        // Copied again, the same content comes back with a new id.
        assert!(pins.iter().any(|pin| pin == text(9, "token").pin_key()));
        toggle_pin(&mut pins, text(9, "token").pin_key());
        assert!(pins.is_empty());
    }

    #[test]
    fn pins_are_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("zehll-pins-test-{}", std::process::id()));
        let path = dir.join("zehll/clipboard.json");
        assert_eq!(load_pins_from(&path), Vec::<String>::new());
        let pins = vec![String::from("token"), String::from("a\ttab")];
        save_pins_to(&path, &pins);
        assert_eq!(load_pins_from(&path), pins);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
