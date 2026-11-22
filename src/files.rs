//! Files page of the launcher: recent folders from `zoxide`, search with `fd`.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

use crate::fuzzy;

/// Rows the page lists at most.
pub const LIMIT: usize = 100;
/// Results `fd` returns at most.
const FD_RESULTS: usize = 300;
/// How deep below home a search looks.
const FD_DEPTH: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(3);
// zoxide checks every folder it knows exists; a cold disk cache took 6.7 s here.
const ZOXIDE_TIMEOUT: Duration = Duration::from_secs(15);
/// Bytes of a readable file shown in the preview.
const PREVIEW_BYTES: usize = 8192;
const PREVIEW_LINES: usize = 40;
const READABLE_MIMES: [&str; 10] = [
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-shellscript",
    "application/x-sh",
    "application/x-yaml",
    "application/toml",
    "application/x-ruby",
    "application/x-python",
    "application/x-perl",
];

/// A file or folder of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// zoxide's frecency, 0 for paths it does not know.
    pub zoxide: f64,
}

impl Entry {
    pub fn new(path: &str, is_dir: bool, zoxide: f64) -> Self {
        let name = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or("/");
        Self { path: path.to_string(), name: name.to_string(), is_dir, zoxide }
    }
}

/// What the preview shows of the selected entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    pub mime: String,
    pub size: u64,
    /// Entries of a folder.
    pub items: Option<usize>,
    /// First lines of a readable file.
    pub preview: Option<String>,
}

/// Reads `zoxide query -ls`: one `score path` per line, best first.
pub fn parse_zoxide(output: &str) -> Vec<Entry> {
    output
        .lines()
        .filter_map(|line| {
            // Only the leading whitespace is padding: paths may hold spaces.
            let (score, path) = line.trim_start().split_once(char::is_whitespace)?;
            Some(Entry::new(path, true, score.parse().ok()?))
        })
        .collect()
}

/// Reads `fd` output, where folders end with `/`.
pub fn parse_fd(output: &str) -> Vec<Entry> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| match line.strip_suffix('/') {
            Some(path) => Entry::new(if path.is_empty() { "/" } else { path }, true, 0.0),
            None => Entry::new(line, false, 0.0),
        })
        .collect()
}

/// `~` and `~/x` expanded with `home`.
fn expand(query: &str, home: &str) -> String {
    match query.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
        _ => query.to_string(),
    }
}

/// The folder to list and the name prefix to keep, when the search is an absolute path.
pub fn path_query(query: &str, home: &str) -> Option<(String, String)> {
    let query = expand(query.trim(), home);
    if !query.starts_with('/') {
        return None;
    }
    if query.ends_with('/') || query == home {
        let dir = query.trim_end_matches('/');
        return Some((if dir.is_empty() { "/" } else { dir }.to_string(), String::new()));
    }
    let (dir, prefix) = query.rsplit_once('/')?;
    Some((if dir.is_empty() { "/" } else { dir }.to_string(), prefix.to_string()))
}

/// Arguments of the `fd` run for `query`, searching `home` unless the query is a path.
pub fn fd_args(query: &str, home: &str) -> Vec<String> {
    let mut args: Vec<String> = ["--type", "f", "--type", "d", "--exclude", ".git"].map(String::from).to_vec();
    args.extend([String::from("--max-results"), FD_RESULTS.to_string()]);
    match path_query(query, home) {
        Some((dir, prefix)) => {
            args.extend(["--max-depth", "1", "--no-ignore", "--fixed-strings", "--ignore-case"].map(String::from));
            if prefix.starts_with('.') {
                args.push(String::from("--hidden"));
            }
            args.extend([prefix, String::from("--search-path"), dir]);
        }
        None => {
            args.extend([String::from("--full-path"), String::from("--max-depth"), FD_DEPTH.to_string()]);
            args.extend(["--exclude", "node_modules", "--exclude", ".local/share/Trash"].map(String::from));
            // Words in order anywhere in the path; the ranking does the fuzzy part.
            let words: Vec<String> = query.split_whitespace().map(regex_escape).collect();
            args.extend([words.join(".*"), home.to_string()]);
        }
    }
    args
}

fn regex_escape(word: &str) -> String {
    word.chars().fold(String::new(), |mut escaped, c| {
        if r"\.+*?()|[]{}^$#&-~".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
        escaped
    })
}

/// Rows for `query`: recent folders when empty; a path lists that folder's children, folders
/// first; anything else ranks recent and found entries by fuzzy match plus zoxide's score.
pub fn rank(query: &str, recent: &[Entry], found: &[Entry], home: &str) -> Vec<Entry> {
    let query = query.trim();
    if query.is_empty() {
        return recent.iter().take(LIMIT).cloned().collect();
    }
    // Recent first, so a found path zoxide knows keeps its score.
    let mut seen = HashSet::new();
    let pool = recent.iter().chain(found).filter(|entry| seen.insert(entry.path.as_str()));
    if let Some((dir, prefix)) = path_query(query, home) {
        let prefix = prefix.to_lowercase();
        let mut children: Vec<Entry> = pool
            .filter(|entry| {
                let parent = entry.path.rsplit_once('/').map(|(parent, _)| if parent.is_empty() { "/" } else { parent });
                parent == Some(dir.as_str()) && entry.name.to_lowercase().starts_with(&prefix)
            })
            .cloned()
            .collect();
        children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        children.truncate(LIMIT);
        return children;
    }
    let mut ranked: Vec<(f64, Entry)> = pool
        .filter_map(|entry| {
            let score = [&entry.name, &entry.path].iter().filter_map(|field| fuzzy::score(query, field)).reduce(f32::max)?;
            Some((score as f64 + entry.zoxide, entry.clone()))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked.into_iter().take(LIMIT).map(|(_, entry)| entry).collect()
}

/// `4.0 KB` style, as upstream shows sizes.
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let size = bytes as f64;
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", size / KB),
        1_048_576..1_073_741_824 => format!("{:.1} MB", size / (KB * KB)),
        _ => format!("{:.1} GB", size / (KB * KB * KB)),
    }
}

/// `path` with the home folder written as `~`.
pub fn tilde(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// What Ctrl+Enter puts in the search: the path, with `/` after a folder to list it.
pub fn path_text(entry: &Entry) -> String {
    match entry.is_dir && !entry.path.ends_with('/') {
        true => format!("{}/", entry.path),
        false => entry.path.clone(),
    }
}

/// Single-quotes `text` for `sh`.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

pub fn is_readable(mime: &str) -> bool {
    mime.starts_with("text/") || READABLE_MIMES.contains(&mime)
}

/// The first lines of a file's start, lossily decoded.
fn preview_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).lines().take(PREVIEW_LINES).collect::<Vec<_>>().join("\n")
}

/// Folders zoxide knows, best first; it leaves out those that no longer exist.
pub async fn recent() -> Vec<Entry> {
    let Some(output) = run("zoxide", &["query".into(), "-ls".into()], ZOXIDE_TIMEOUT).await else {
        return Vec::new();
    };
    parse_zoxide(&output).into_iter().take(LIMIT).collect()
}

/// Files and folders matching `query` with `fd`; empty when the query names a missing folder.
pub async fn search(query: String, home: String) -> Vec<Entry> {
    if let Some((dir, _)) = path_query(&query, &home)
        && !Path::new(&dir).is_dir()
    {
        return Vec::new();
    }
    run("fd", &fd_args(&query, &home), TIMEOUT).await.map(|output| parse_fd(&output)).unwrap_or_default()
}

/// Type, size, item count and preview of `path`; `None` when it cannot be read.
pub async fn details(path: String) -> Option<Details> {
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    if metadata.is_dir() {
        let mut items = 0;
        if let Ok(mut dir) = tokio::fs::read_dir(&path).await {
            while let Ok(Some(_)) = dir.next_entry().await {
                items += 1;
            }
        }
        return Some(Details { mime: String::from("inode/directory"), size: metadata.len(), items: Some(items), preview: None });
    }
    let mime = run("file", &["-b".into(), "--mime-type".into(), "--".into(), path.clone()], TIMEOUT)
        .await
        .map(|output| output.trim().to_string())
        .filter(|mime| !mime.is_empty())
        .unwrap_or_else(|| String::from("application/octet-stream"));
    let preview = match is_readable(&mime) {
        true => read_start(&path).await.map(|bytes| preview_text(&bytes)),
        false => None,
    };
    Some(Details { mime, size: metadata.len(), items: None, preview })
}

async fn read_start(path: &str) -> Option<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut bytes = Vec::with_capacity(PREVIEW_BYTES);
    file.take(PREVIEW_BYTES as u64).read_to_end(&mut bytes).await.ok()?;
    Some(bytes)
}

/// Stdout of `program`, or `None` when it fails, is missing or takes too long.
async fn run(program: &str, args: &[String], timeout: Duration) -> Option<String> {
    let run = Command::new(program).args(args).stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    let output = tokio::time::timeout(timeout, run).await.ok()?.ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/user";

    fn dir(path: &str, zoxide: f64) -> Entry {
        Entry::new(path, true, zoxide)
    }

    fn file(path: &str) -> Entry {
        Entry::new(path, false, 0.0)
    }

    fn paths(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.path.as_str()).collect()
    }

    #[test]
    fn entries_are_named_after_the_last_component() {
        assert_eq!(dir("/home/user/code/zehll", 0.0).name, "zehll");
        assert_eq!(dir("/", 0.0).name, "/");
    }

    #[test]
    fn zoxide_lines_give_score_and_path() {
        let output = " 208.0 /home/user/sc\n  96.5 /home/user/My Documents/notes\n\nbroken\n   4.0 /tmp\n";
        assert_eq!(parse_zoxide(output), [dir("/home/user/sc", 208.0), dir("/home/user/My Documents/notes", 96.5), dir("/tmp", 4.0)]);
    }

    #[test]
    fn fd_marks_folders_with_a_trailing_slash() {
        let output = "/home/user/a b/\n/home/user/x.txt\n\n";
        assert_eq!(parse_fd(output), [dir("/home/user/a b", 0.0), file("/home/user/x.txt")]);
    }

    #[test]
    fn a_search_that_is_a_path_lists_that_folder() {
        assert_eq!(path_query("zehll", HOME), None);
        assert_eq!(path_query("~", HOME), Some((String::from(HOME), String::new())));
        assert_eq!(path_query("~/co", HOME), Some((String::from(HOME), String::from("co"))));
        assert_eq!(path_query("/etc/", HOME), Some((String::from("/etc"), String::new())));
        assert_eq!(path_query("/", HOME), Some((String::from("/"), String::new())));
        assert_eq!(path_query("/us", HOME), Some((String::from("/"), String::from("us"))));
    }

    #[test]
    fn fd_searches_home_with_bounded_depth_and_results() {
        let args = fd_args("code zeh(", HOME);
        let joined = args.join(" ");
        assert!(joined.contains("--max-depth 8") && joined.contains("--max-results 300"), "{joined}");
        assert!(joined.contains("--full-path") && joined.contains("--exclude node_modules"), "{joined}");
        // Words match in order, with regex characters escaped.
        assert_eq!(args[args.len() - 2..], [String::from(r"code.*zeh\("), String::from(HOME)]);
    }

    #[test]
    fn fd_lists_a_folder_for_a_path_search() {
        let args = fd_args("~/.co", HOME);
        let joined = args.join(" ");
        assert!(joined.contains("--max-depth 1") && joined.contains("--fixed-strings") && joined.contains("--hidden"), "{joined}");
        assert_eq!(args[args.len() - 3..], [String::from(".co"), String::from("--search-path"), String::from(HOME)]);
        assert!(!fd_args("/etc/ho", HOME).contains(&String::from("--hidden")));
    }

    #[test]
    fn an_empty_search_lists_recent_folders_in_order() {
        let recent = [dir("/home/user/sc", 208.0), dir("/home/user/code", 90.0)];
        assert_eq!(paths(&rank("  ", &recent, &[file("/home/user/x")], HOME)), ["/home/user/sc", "/home/user/code"]);
    }

    #[test]
    fn found_entries_are_boosted_by_the_zoxide_score() {
        let recent = [dir("/home/user/code/zehll", 50.0), dir("/home/user/music", 80.0)];
        let found = [file("/home/user/notes/zehll.md"), dir("/home/user/code/zehll", 0.0), dir("/home/user/old/zehll", 0.0)];
        let ranked = rank("zehll", &recent, &found, HOME);
        assert_eq!(paths(&ranked)[0], "/home/user/code/zehll");
        // Listed once, keeping its score.
        assert_eq!(ranked.iter().filter(|entry| entry.path == "/home/user/code/zehll").count(), 1);
        assert_eq!(ranked[0].zoxide, 50.0);
        assert!(!paths(&ranked).contains(&"/home/user/music"));
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn a_path_search_lists_children_folders_first() {
        let found = [file("/home/user/code/b.txt"), dir("/home/user/code/zehll", 0.0), dir("/home/user/code/Alpha", 0.0), file("/home/user/other/a")];
        assert_eq!(paths(&rank("~/code/", &[], &found, HOME)), ["/home/user/code/Alpha", "/home/user/code/zehll", "/home/user/code/b.txt"]);
        assert_eq!(paths(&rank("~/code/ZE", &[], &found, HOME)), ["/home/user/code/zehll"]);
    }

    #[test]
    fn the_list_is_capped() {
        let found: Vec<Entry> = (0..150).map(|n| file(&format!("/home/user/f{n}"))).collect();
        assert_eq!(rank("f", &[], &found, HOME).len(), LIMIT);
    }

    #[test]
    fn sizes_read_like_upstream() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(4096), "4.0 KB");
        assert_eq!(format_size(1_572_864), "1.5 MB");
        assert_eq!(format_size(3 * 1_073_741_824), "3.0 GB");
    }

    #[test]
    fn home_is_shown_as_a_tilde() {
        assert_eq!(tilde("/home/user/code/zehll", HOME), "~/code/zehll");
        assert_eq!(tilde("/home/user", HOME), "~");
        assert_eq!(tilde("/home/username", HOME), "/home/username");
        assert_eq!(tilde("/etc", ""), "/etc");
    }

    #[test]
    fn ctrl_enter_text_ends_folders_with_a_slash() {
        assert_eq!(path_text(&dir("/home/user/code", 0.0)), "/home/user/code/");
        assert_eq!(path_text(&file("/home/user/a.txt")), "/home/user/a.txt");
        assert_eq!(path_text(&dir("/", 0.0)), "/");
    }

    #[test]
    fn paths_are_quoted_for_the_shell() {
        assert_eq!(shell_quote("/home/user/a b.txt"), "'/home/user/a b.txt'");
        assert_eq!(shell_quote("/tmp/it's $(x)"), r"'/tmp/it'\''s $(x)'");
    }

    #[test]
    fn text_and_known_script_types_are_readable() {
        assert!(is_readable("text/plain") && is_readable("application/json") && is_readable("application/x-shellscript"));
        assert!(!is_readable("image/png") && !is_readable("application/octet-stream") && !is_readable(""));
    }

    #[test]
    fn the_preview_keeps_the_first_lines() {
        let text: String = (0..100).map(|n| format!("line {n}\n")).collect();
        let preview = preview_text(text.as_bytes());
        assert_eq!(preview.lines().count(), PREVIEW_LINES);
        assert!(preview.starts_with("line 0\nline 1"));
        assert_eq!(preview_text(b"a\xffb"), "a\u{fffd}b");
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zehll-files-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
    }

    #[test]
    fn folder_details_count_items() {
        let dir = temp_dir("details");
        std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let details = block_on(details(dir.to_string_lossy().into_owned()));
        std::fs::remove_dir_all(&dir).unwrap();
        let details = details.unwrap();
        assert_eq!((details.mime.as_str(), details.items, details.preview), ("inode/directory", Some(2), None));
    }
}
