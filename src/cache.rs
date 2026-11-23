//! Zehll's files under `~/.cache/zehll`: unique partial files and automatic pruning.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// `~/.cache/zehll/<name>` (or under `$XDG_CACHE_HOME`).
pub fn dir(name: &str) -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cache")).join("zehll").join(name)
}

/// A partial file next to `file`, unique per call: two writers of the same file never share it.
pub fn partial(file: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    file.with_file_name(format!("{name}.{}-{}.part", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Files to delete so at most `max` remain: every file in `keep` stays, then the most recently
/// used ones (newest modification time first) fill the rest.
pub fn stale(mut files: Vec<(PathBuf, SystemTime)>, keep: &[PathBuf], max: usize) -> Vec<PathBuf> {
    files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    let room = max.saturating_sub(files.iter().filter(|(path, _)| keep.contains(path)).count());
    files.into_iter().filter(|(path, _)| !keep.contains(path)).skip(room).map(|(path, _)| path).collect()
}

/// Marks `file` as just used, so pruning keeps it longer.
pub fn touch(file: &Path) {
    if let Ok(handle) = std::fs::File::options().append(true).open(file) {
        let _ = handle.set_modified(SystemTime::now());
    }
}

/// Deletes the least recently used files with one of `extensions` in `dir` beyond `max`, never
/// those in `keep`. Leftover partial files go too.
pub fn prune(dir: &Path, extensions: &[&str], keep: &[PathBuf], max: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let modified = entry.metadata().and_then(|metadata| metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(found) if extensions.contains(&found) => files.push((path, modified)),
            // A partial file still being written is young; an hour-old one was abandoned.
            Some("part") if modified.elapsed().is_ok_and(|age| age.as_secs() > 3600) => {
                let _ = std::fs::remove_file(&path);
            }
            _ => {}
        }
    }
    for path in stale(files, keep, max) {
        if let Err(err) = std::fs::remove_file(&path) {
            eprintln!("zehll: cannot prune {}: {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn partial_files_are_unique_and_next_to_the_file() {
        let file = Path::new("/cache/thumbs/a.jpg");
        let (first, second) = (partial(file), partial(file));
        assert_ne!(first, second);
        for partial in [first, second] {
            assert_eq!(partial.parent(), file.parent());
            assert_eq!(partial.extension().and_then(|extension| extension.to_str()), Some("part"));
            assert!(partial.file_name().unwrap().to_string_lossy().starts_with("a.jpg."));
        }
    }

    #[test]
    fn stale_files_are_the_oldest_beyond_the_limit_and_never_the_kept_ones() {
        let at = |seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        let files = vec![
            (PathBuf::from("old"), at(1)),
            (PathBuf::from("current"), at(2)),
            (PathBuf::from("newest"), at(5)),
            (PathBuf::from("newer"), at(4)),
            (PathBuf::from("middle"), at(3)),
        ];
        let keep = [PathBuf::from("current")];
        let mut deleted = stale(files.clone(), &keep, 3);
        deleted.sort();
        assert_eq!(deleted, [PathBuf::from("middle"), PathBuf::from("old")]);
        assert!(stale(files.clone(), &keep, 5).is_empty());
        // Kept files stay even past the limit.
        let mut deleted = stale(files, &keep, 0);
        deleted.sort();
        assert_eq!(deleted, ["middle", "newer", "newest", "old"].map(PathBuf::from));
    }

    #[test]
    fn prune_deletes_on_disk_and_touch_renews_a_file() {
        let dir = std::env::temp_dir().join(format!("zehll-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, age: u64| {
            let path = dir.join(name);
            std::fs::write(&path, b"x").unwrap();
            let handle = std::fs::File::options().append(true).open(&path).unwrap();
            handle.set_modified(SystemTime::now() - Duration::from_secs(age)).unwrap();
            path
        };
        let (a, b, c) = (file("a.png", 30), file("b.png", 20), file("c.png", 10));
        let other = file("notes.txt", 40);
        let abandoned = file("d.png.7.part", 7200);
        let writing = file("e.png.8.part", 5);
        touch(&a);
        prune(&dir, &["png"], &[], 2);
        assert!(a.exists() && c.exists() && !b.exists());
        assert!(other.exists() && writing.exists() && !abandoned.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
