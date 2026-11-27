//! Desktop wallpaper drawn by Zehll, like ii's background: one layer per monitor below every
//! window shows the wallpaper zoomed to cover it, and the view slides across it as the monitor's
//! workspace changes. matugen recolors the desktop (and Zehll's theme) on every new wallpaper.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use ::image::ImageEncoder;
use ::image::imageops::FilterType;
use iced::widget::canvas::{self, Canvas};
use iced::widget::image::Handle;
use iced::{Element, Length, Rectangle, Renderer, Theme, mouse};

use crate::{Message, cache, hyprland, snip, workspaces};

/// ii's `background.parallax.workspaceZoom`: the wallpaper is this much larger than the screen.
pub const ZOOM: f32 = 1.07;
/// ii's parallax animation: 600 ms, ease-out cubic.
pub const PAN: Duration = Duration::from_millis(600);

/// The wallpaper and its accent, kept in `~/.local/state/zehll/wallpapers.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub path: Option<PathBuf>,
    /// Index into matugen's source colors of the wallpaper (`--source-color-index`).
    pub accent: usize,
}

impl State {
    pub fn parse(json: &serde_json::Value) -> Self {
        Self {
            path: json["path"].as_str().filter(|path| !path.is_empty()).map(PathBuf::from),
            accent: json["accent"].as_u64().unwrap_or(0) as usize,
        }
    }

    /// `previous` is the file as it is on disk, so keys owned by other features survive.
    pub fn merge_into(&self, previous: serde_json::Value) -> serde_json::Value {
        let mut file = match previous {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        let path = self.path.as_ref().map(|path| path.to_string_lossy().into_owned());
        file.insert(String::from("path"), path.map_or(serde_json::Value::Null, serde_json::Value::from));
        file.insert(String::from("accent"), self.accent.into());
        serde_json::Value::Object(file)
    }
}

/// Size of an `image` scaled to cover a `monitor` and zoomed by `zoom`, never smaller than the
/// monitor.
pub fn cover(image: (u32, u32), monitor: (u32, u32), zoom: f32) -> (u32, u32) {
    let (width, height) = (image.0.max(1) as f32, image.1.max(1) as f32);
    let scale = (monitor.0 as f32 / width).max(monitor.1 as f32 / height) * zoom;
    ((width * scale).round().max(monitor.0 as f32) as u32, (height * scale).round().max(monitor.1 as f32) as u32)
}

/// Where a wallpaper of `size` is drawn on a `monitor` on workspace `slot` of `slots`: its
/// left-top corner, at or left of and above the monitor's. The view moves from the wallpaper's
/// left edge (first slot) to its right edge (last slot) and stays centered vertically.
pub fn offset(size: (u32, u32), monitor: (u32, u32), slot: usize, slots: usize) -> (f32, f32) {
    let fraction = match slots {
        0 | 1 => 0.5,
        _ => slot.min(slots - 1) as f32 / (slots - 1) as f32,
    };
    let spare = |size: u32, screen: u32| size.saturating_sub(screen) as f32;
    (-spare(size.0, monitor.0) * fraction, -spare(size.1, monitor.1) / 2.0)
}

/// The horizontal position of a wallpaper, sliding to each new target like ii's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pan {
    from: f32,
    to: f32,
    started: Option<Instant>,
}

impl Pan {
    pub fn new(x: f32) -> Self {
        Self { from: x, to: x, started: None }
    }

    /// The position at `now`.
    pub fn at(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.to;
        };
        let t = (now.saturating_duration_since(started).as_secs_f32() / PAN.as_secs_f32()).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    /// Slides to `x` from where it is drawn at `now`.
    pub fn move_to(&mut self, x: f32, now: Instant) {
        self.from = self.at(now);
        self.to = x;
        self.started = Some(now);
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.started.is_some_and(|started| now.saturating_duration_since(started) < PAN)
    }
}

/// Scaled wallpapers kept in the cache (a few wallpapers for each monitor); the oldest go first.
const MAX_SCALED: usize = 12;

/// Cache file of a scaled wallpaper: a stable hash of the source (path, size, modification
/// time) with the scaled size, so a changed file or monitor gets a new one.
pub fn cache_name(source: &str, len: u64, modified: u64, size: (u32, u32)) -> String {
    // FNV-1a: stable across builds, unlike the standard library's hasher.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in source.bytes().chain(len.to_le_bytes()).chain(modified.to_le_bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}-{}x{}.png", size.0, size.1)
}

/// Slots per monitor, from the workspace indicator.
pub fn slots() -> usize {
    workspaces::PER_MONITOR as usize
}

/// The wallpaper scaled for one monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Scaled {
    pub monitor: String,
    /// The monitor's physical size.
    pub screen: (u32, u32),
    pub size: (u32, u32),
    pub file: PathBuf,
}

/// One monitor's wallpaper layer.
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: iced::window::Id,
    pub scaled: Scaled,
    pub handle: Handle,
    pub pan: Pan,
}

impl Layer {
    pub fn new(id: iced::window::Id, scaled: Scaled, slot: usize) -> Self {
        let x = offset(scaled.size, scaled.screen, slot, slots()).0;
        Self { id, handle: Handle::from_path(&scaled.file), scaled, pan: Pan::new(x) }
    }

    /// A new wallpaper keeps the layer and jumps to its place.
    pub fn replace(&mut self, scaled: Scaled, slot: usize) {
        *self = Self::new(self.id, scaled, slot);
    }

    pub fn slide_to(&mut self, slot: usize, now: Instant) {
        let x = offset(self.scaled.size, self.scaled.screen, slot, slots()).0;
        if x != self.pan.to {
            self.pan.move_to(x, now);
        }
    }
}

/// The layer's surface: the whole monitor, below every window, without input or keyboard.
pub fn layer_settings(output: String) -> iced_exwlshell::reexport::NewLayerShellSettings {
    use iced_exwlshell::reexport::{Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption};
    NewLayerShellSettings {
        size: iced_exwlshell::reexport::LayerSize::FILL,
        layer: Layer::Background,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        // Covers the whole output, bars included.
        exclusive_zone: Some(-1),
        margin: None,
        keyboard_interactivity: KeyboardInteractivity::None,
        output_option: OutputOption::OutputName(output),
        events_transparent: true,
        namespace: Some(String::from("zehll-wallpaper")),
        blur_option: Default::default(),
    }
}

pub fn view(layer: &Layer, now: Instant) -> Element<'_, Message> {
    let y = offset(layer.scaled.size, layer.scaled.screen, 0, 1).1;
    let picture = Picture { handle: layer.handle.clone(), screen: layer.scaled.screen, size: layer.scaled.size, at: (layer.pan.at(now), y) };
    Canvas::new(picture).width(Length::Fill).height(Length::Fill).into()
}

/// The wallpaper drawn at its place; physical pixels are mapped to the surface's size.
struct Picture {
    handle: Handle,
    screen: (u32, u32),
    size: (u32, u32),
    at: (f32, f32),
}

impl canvas::Program<Message> for Picture {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let scale = bounds.width / self.screen.0.max(1) as f32;
        let rectangle = Rectangle {
            x: self.at.0 * scale,
            y: self.at.1 * scale,
            width: self.size.0 as f32 * scale,
            height: self.size.1 as f32 * scale,
        };
        frame.draw_image(rectangle, canvas::Image::new(self.handle.clone()));
        vec![frame.into_geometry()]
    }
}

fn state_file() -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/state")).join("zehll/wallpapers.json")
}

fn read_state_file() -> serde_json::Value {
    std::fs::read_to_string(state_file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// A section of the state file kept by another feature, like the Wallhaven filters.
pub fn load_section(name: &str) -> serde_json::Value {
    read_state_file()[name].take()
}

pub fn save_section(name: &str, value: serde_json::Value) {
    let mut file = match read_state_file() {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    file.insert(name.to_owned(), value);
    write_state_file(serde_json::Value::Object(file));
}

fn write_state_file(content: serde_json::Value) {
    let path = state_file();
    let result = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, content.to_string()));
    if let Err(err) = result {
        eprintln!("zehll: cannot save {}: {err}", path.display());
    }
}

pub fn load() -> State {
    State::parse(&read_state_file())
}

pub fn save(state: &State) {
    write_state_file(state.merge_into(read_state_file()));
}

/// Every monitor's name and physical size.
fn monitor_sizes() -> Vec<(String, (u32, u32))> {
    hyprland::monitors_json()
        .map(|json| snip::parse_monitors(&json).into_iter().map(|monitor| (monitor.name, (monitor.width, monitor.height))).collect())
        .unwrap_or_default()
}

/// Scales `source` for every monitor, reusing cached files: only as large as the monitor needs,
/// so the texture stays small. `None` when the image cannot be read.
pub async fn prepare(source: PathBuf) -> Option<Vec<Scaled>> {
    let monitors = tokio::task::spawn_blocking(monitor_sizes).await.ok()?;
    tokio::task::spawn_blocking(move || prepare_blocking(&source, &monitors)).await.ok()?
}

fn prepare_blocking(source: &Path, monitors: &[(String, (u32, u32))]) -> Option<Vec<Scaled>> {
    let metadata = std::fs::metadata(source).ok()?;
    let modified = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |time| time.as_secs());
    let dir = cache::dir("wallpaper");
    std::fs::create_dir_all(&dir).ok()?;
    let dimensions = match ::image::image_dimensions(source) {
        Ok(dimensions) => dimensions,
        Err(err) => {
            eprintln!("zehll: cannot read wallpaper {}: {err}", source.display());
            return None;
        }
    };
    // Decoded once, and only when some size is missing.
    let mut image: Option<::image::DynamicImage> = None;
    let mut all = Vec::new();
    for (monitor, screen) in monitors {
        let size = cover(dimensions, *screen, ZOOM);
        let file = dir.join(cache_name(&source.to_string_lossy(), metadata.len(), modified, size));
        if file.exists() {
            cache::touch(&file);
        } else {
            if image.is_none() {
                image = match ::image::open(source) {
                    Ok(image) => Some(image),
                    Err(err) => {
                        eprintln!("zehll: cannot read wallpaper {}: {err}", source.display());
                        return None;
                    }
                };
            }
            let scaled = image.as_ref()?.resize_exact(size.0, size.1, FilterType::Triangle).to_rgb8();
            write_png(&file, &scaled)?;
        }
        all.push(Scaled { monitor: monitor.clone(), screen: *screen, size, file });
    }
    let current: Vec<PathBuf> = all.iter().map(|scaled| scaled.file.clone()).collect();
    cache::prune(&dir, &["png"], &current, MAX_SCALED);
    Some(all)
}

/// Fast PNG: a cache file read back once per start.
fn write_png(path: &Path, image: &::image::RgbImage) -> Option<()> {
    let partial = cache::partial(path);
    let written = std::fs::File::create(&partial).map_err(::image::ImageError::IoError).and_then(|file| {
        let encoder = ::image::codecs::png::PngEncoder::new_with_quality(
            std::io::BufWriter::new(file),
            ::image::codecs::png::CompressionType::Fast,
            ::image::codecs::png::FilterType::Adaptive,
        );
        encoder.write_image(image.as_raw(), image.width(), image.height(), ::image::ExtendedColorType::Rgb8)
    });
    match written.and_then(|()| std::fs::rename(&partial, path).map_err(::image::ImageError::IoError)) {
        Ok(()) => Some(()),
        Err(err) => {
            eprintln!("zehll: cannot write {}: {err}", path.display());
            let _ = std::fs::remove_file(&partial);
            None
        }
    }
}

/// Regenerates the desktop colors (and Zehll's theme) from `source` with matugen.
pub async fn recolor(source: PathBuf, accent: usize) {
    let output = tokio::process::Command::new("matugen")
        .arg("image")
        .arg(&source)
        .args(["--mode", "dark", "--source-color-index", &accent.to_string(), "--quiet"])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => eprintln!("zehll: matugen failed: {}", String::from_utf8_lossy(&output.stderr).trim()),
        Err(err) => eprintln!("zehll: cannot run matugen: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_reads_its_keys_and_keeps_the_others_when_saved() {
        let file = json!({ "path": "/w/a.png", "accent": 2, "wallhaven": { "apikey": "k" } });
        let state = State::parse(&file);
        assert_eq!(state, State { path: Some(PathBuf::from("/w/a.png")), accent: 2 });
        assert_eq!(State::parse(&json!({})), State::default());
        let saved = State { path: Some(PathBuf::from("/w/b.jpg")), accent: 0 }.merge_into(file);
        assert_eq!(saved, json!({ "path": "/w/b.jpg", "accent": 0, "wallhaven": { "apikey": "k" } }));
        assert_eq!(State::default().merge_into(json!(null)), json!({ "path": null, "accent": 0 }));
    }

    #[test]
    fn the_wallpaper_covers_the_monitor_zoomed() {
        // 3840x2160 on 1920x1080: scale 0.5 * 1.07 = 0.535.
        assert_eq!(cover((3840, 2160), (1920, 1080), 1.07), (2054, 1156));
        // Taller than the monitor: the width decides (2560 / 2592 * 1.07).
        assert_eq!(cover((2592, 1728), (2560, 1440), 1.07), (2739, 1826));
        // A small image is scaled up; rounding never leaves the monitor uncovered.
        assert_eq!(cover((1000, 1000), (1920, 1080), 1.0), (1920, 1920));
        assert_eq!(cover((1919, 1079), (1920, 1080), 1.0), (1921, 1080));
    }

    #[test]
    fn the_view_moves_across_the_slots_and_stays_centered_vertically() {
        // 2054x1156 on 1920x1080: 134 pixels to pan across, 76 to center.
        assert_eq!(offset((2054, 1156), (1920, 1080), 0, 4), (0.0, -38.0));
        assert_eq!(offset((2054, 1156), (1920, 1080), 3, 4), (-134.0, -38.0));
        assert_eq!(offset((2054, 1156), (1920, 1080), 1, 4).0, -134.0 / 3.0);
        // Past the last slot, it stays at the right edge; one slot shows the middle.
        assert_eq!(offset((2054, 1156), (1920, 1080), 7, 4).0, -134.0);
        assert_eq!(offset((2054, 1156), (1920, 1080), 0, 1).0, -67.0);
    }

    #[test]
    fn the_pan_eases_out_over_600_ms_from_where_it_is_drawn() {
        let start = Instant::now();
        let mut pan = Pan::new(0.0);
        assert!(!pan.animating(start));
        pan.move_to(-100.0, start);
        assert!(pan.animating(start));
        assert_eq!(pan.at(start), 0.0);
        // Ease-out cubic: 1 - (1 - 0.5)^3 = 0.875 at half time.
        assert_eq!(pan.at(start + PAN / 2), -87.5);
        assert_eq!(pan.at(start + PAN), -100.0);
        assert!(!pan.animating(start + PAN));
        // A new target mid-way starts from the drawn position.
        let mut pan = Pan::new(0.0);
        pan.move_to(-100.0, start);
        pan.move_to(0.0, start + PAN / 2);
        assert_eq!(pan.at(start + PAN / 2), -87.5);
        assert_eq!(pan.at(start + PAN / 2 + PAN), 0.0);
    }

    #[test]
    fn cache_names_change_with_the_file_and_the_size() {
        let name = cache_name("/w/a.png", 100, 5, (2054, 1156));
        assert!(name.ends_with("-2054x1156.png"), "{name}");
        assert_eq!(name, cache_name("/w/a.png", 100, 5, (2054, 1156)));
        assert_ne!(name, cache_name("/w/a.png", 100, 6, (2054, 1156)));
        assert_ne!(name, cache_name("/w/b.png", 100, 5, (2054, 1156)));
        assert_ne!(name, cache_name("/w/a.png", 100, 5, (2739, 1826)));
    }
}
