//! The launcher's Wallpapers page: the images in the wallpaper folder as a grid of cached
//! thumbnails, and the accent colors matugen can take from the selected one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use iced::futures::{SinkExt, Stream};
use iced::stream;

use crate::{cache, fuzzy, wallpaper};

pub const COLUMNS: usize = 4;
pub const ROWS: usize = 3;
const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
/// Thumbnails cover this size, a 16:9 tile at twice the grid's tile width.
const THUMB: (u32, u32) = (480, 270);
/// Thumbnails made at the same time.
const THUMB_BATCH: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// Width and height from the file's header; `None` when it cannot be read.
    pub size: Option<(u32, u32)>,
    /// Seconds since the epoch.
    pub modified: u64,
    pub len: u64,
}

/// The page's state, kept in the launcher between openings.
#[derive(Debug, Default)]
pub struct Gallery {
    /// The folder's images, newest first.
    pub entries: Vec<Entry>,
    /// Cached thumbnail of each image, by path.
    pub thumbs: HashMap<PathBuf, PathBuf>,
    /// matugen's source colors of each image, by path.
    pub colors: HashMap<PathBuf, Vec<String>>,
    /// Accent picked for the selected image, an index into its colors.
    pub accent: usize,
    /// First row shown.
    pub first_row: usize,
}

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// Matugen's `--show-source-colors` output: one `#rrggbb` per line.
pub fn parse_source_colors(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.len() == 7 && line.starts_with('#') && line[1..].chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_owned)
        .collect()
}

/// Indices of the entries matching `query`, in the entries' order (newest first).
pub fn filter(entries: &[Entry], query: &str) -> Vec<usize> {
    (0..entries.len()).filter(|&index| query.is_empty() || fuzzy::score(query, &entries[index].name).is_some()).collect()
}

/// The selection after moving `delta` tiles in a grid of `len`, kept inside it.
pub fn step(selected: usize, delta: i32, len: usize) -> usize {
    (selected as i64 + delta as i64).clamp(0, len.saturating_sub(1) as i64) as usize
}

/// The first row to show so that `selected` stays among the `rows` rows shown.
pub fn first_row(selected: usize, first_row: usize, rows: usize, columns: usize) -> usize {
    let row = selected / columns.max(1);
    first_row.min(row).max((row + 1).saturating_sub(rows))
}

/// `previous` moved by `delta` among `count` colors, wrapping.
pub fn cycle(previous: usize, delta: i32, count: usize) -> usize {
    match count {
        0 => 0,
        _ => (previous as i64 + delta as i64).rem_euclid(count as i64) as usize,
    }
}

/// A random one of `count` items other than the current one, from `seed`.
pub fn pick_other(count: usize, is_current: impl Fn(usize) -> bool, seed: usize) -> Option<usize> {
    let others: Vec<usize> = (0..count).filter(|&at| !is_current(at)).collect();
    match others.len() {
        0 => None,
        len => Some(others[seed % len]),
    }
}

/// A seed from the clock, for `pick_other`.
pub fn seed() -> usize {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.subsec_nanos() as usize)
}

/// `$(xdg-user-dir PICTURES)/Wallpapers`, like ii.
pub fn folder() -> PathBuf {
    let pictures = std::process::Command::new("xdg-user-dir")
        .arg("PICTURES")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Pictures"));
    pictures.join("Wallpapers")
}

/// The folder's images (not hidden ones), newest first, with their size from the header.
pub async fn list() -> Vec<Entry> {
    tokio::task::spawn_blocking(|| list_blocking(&folder())).await.unwrap_or_default()
}

fn list_blocking(folder: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_image(path) && !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')))
        .filter_map(|path| {
            let metadata = std::fs::metadata(&path).ok()?;
            let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(Entry {
                name: path.file_name()?.to_string_lossy().into_owned(),
                size: ::image::image_dimensions(&path).ok(),
                modified,
                len: metadata.len(),
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    entries
}

fn thumbs_dir() -> PathBuf {
    cache::dir("thumbs")
}

/// The cached thumbnail of `entry`: named after its path, size and modification time.
fn thumb_file(entry: &Entry) -> PathBuf {
    thumbs_dir().join(wallpaper::cache_name(&entry.path.to_string_lossy(), entry.len, entry.modified, THUMB).replace(".png", ".jpg"))
}

/// Deletes thumbnails of images no longer in the folder, or of older versions of them.
pub fn prune_thumbs(entries: &[Entry]) {
    let keep: Vec<PathBuf> = entries.iter().map(thumb_file).collect();
    cache::prune(&thumbs_dir(), &["jpg"], &keep, keep.len());
}

/// Thumbnails of `entries`, made (or found in the cache) a few at a time, each sent as soon as
/// it is ready with the path it belongs to.
pub fn thumbnails(entries: Vec<Entry>) -> impl Stream<Item = (PathBuf, Option<PathBuf>)> {
    stream::channel(THUMB_BATCH, async move |mut output| {
        for batch in entries.chunks(THUMB_BATCH) {
            let made = batch.iter().cloned().map(|entry| async move {
                let path = entry.path.clone();
                (path, tokio::task::spawn_blocking(move || thumbnail(&entry)).await.ok().flatten())
            });
            for result in iced::futures::future::join_all(made).await {
                if output.send(result).await.is_err() {
                    return;
                }
            }
        }
    })
}

fn thumbnail(entry: &Entry) -> Option<PathBuf> {
    let dir = thumbs_dir();
    let file = thumb_file(entry);
    if file.exists() {
        return Some(file);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let image = ::image::open(&entry.path).ok()?;
    let thumb = image.resize_to_fill(THUMB.0, THUMB.1, ::image::imageops::FilterType::Triangle).to_rgb8();
    let partial = cache::partial(&file);
    thumb.save_with_format(&partial, ::image::ImageFormat::Jpeg).ok()?;
    std::fs::rename(&partial, &file).ok()?;
    Some(file)
}

/// The colors matugen can build the theme from, for the accent strip (`--dry-run` writes nothing).
pub async fn source_colors(path: PathBuf) -> Vec<String> {
    let output = tokio::process::Command::new("matugen")
        .arg("image")
        .arg(&path)
        .args(["--dry-run", "--show-source-colors", "--source-color-index", "0"])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => parse_source_colors(&String::from_utf8_lossy(&output.stdout)),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> Entry {
        Entry { path: PathBuf::from(format!("/w/{name}")), name: name.into(), size: None, modified: 0, len: 0 }
    }

    #[test]
    fn images_are_known_by_extension() {
        assert!(is_image(Path::new("/w/a.PNG")));
        assert!(is_image(Path::new("/w/b.jpeg")));
        assert!(!is_image(Path::new("/w/c.mp4")));
        assert!(!is_image(Path::new("/w/noext")));
    }

    #[test]
    fn source_colors_are_the_hex_lines() {
        assert_eq!(parse_source_colors("#fa3703\n#fa9a16\nnoise\n #ffe433 \n#12345\n"), ["#fa3703", "#fa9a16", "#ffe433"]);
    }

    #[test]
    fn the_search_keeps_matching_entries_in_order() {
        let entries = [entry("wallhaven-9512z8.png"), entry("random_wallpaper.jpg"), entry("wallhaven-76195y.png")];
        assert_eq!(filter(&entries, ""), [0, 1, 2]);
        assert_eq!(filter(&entries, "wallhaven"), [0, 2]);
        assert_eq!(filter(&entries, "random"), [1]);
        assert!(filter(&entries, "zzz").is_empty());
    }

    #[test]
    fn moving_stays_inside_the_grid() {
        assert_eq!(step(0, 1, 10), 1);
        assert_eq!(step(0, -1, 10), 0);
        assert_eq!(step(2, 4, 10), 6);
        assert_eq!(step(7, 4, 10), 9);
        assert_eq!(step(5, -4, 10), 1);
        assert_eq!(step(1, -4, 10), 0);
        assert_eq!(step(3, 1, 0), 0);
    }

    #[test]
    fn the_view_scrolls_by_rows_to_keep_the_selection() {
        // 4 columns, 3 rows shown.
        assert_eq!(first_row(5, 0, 3, 4), 0);
        assert_eq!(first_row(12, 0, 3, 4), 1);
        assert_eq!(first_row(20, 1, 3, 4), 3);
        assert_eq!(first_row(2, 3, 3, 4), 0);
        assert_eq!(first_row(9, 1, 3, 4), 1);
    }

    #[test]
    fn a_random_pick_skips_the_current_one() {
        assert_eq!(pick_other(3, |at| at == 0, 0), Some(1));
        assert_eq!(pick_other(3, |at| at == 0, 1), Some(2));
        assert_eq!(pick_other(3, |at| at == 0, 2), Some(1));
        assert_eq!(pick_other(3, |_| false, 5), Some(2));
        assert_eq!(pick_other(1, |at| at == 0, 7), None);
        assert_eq!(pick_other(0, |_| false, 7), None);
    }

    #[test]
    fn the_accent_cycles_through_the_colors() {
        assert_eq!(cycle(0, 1, 4), 1);
        assert_eq!(cycle(3, 1, 4), 0);
        assert_eq!(cycle(0, -1, 4), 3);
        assert_eq!(cycle(2, 1, 0), 0);
    }
}
