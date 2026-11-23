//! Screen snip with a zone selector, like ii's region selector: every monitor is frozen,
//! windows and layers can be picked with a click, and a dragged rectangle selects any zone.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use chrono::NaiveDateTime;
use iced::Point;
use iced::widget::image::Handle;
use ::image::RgbaImage;
use tokio::io::AsyncWriteExt;

use crate::hyprland;

/// A press and release closer than this, in logical pixels, is a click.
const CLICK_SLOP: f32 = 4.0;

/// A rectangle in logical pixels relative to its monitor's top left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, point: Point) -> bool {
        (self.x..=self.x + self.width).contains(&point.x) && (self.y..=self.y + self.height).contains(&point.y)
    }

    /// The part of `self` inside `other`, if any.
    fn intersection(&self, other: &Rect) -> Option<Rect> {
        let (left, top) = (self.x.max(other.x), self.y.max(other.y));
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > left && bottom > top).then_some(Rect { x: left, y: top, width: right - left, height: bottom - top })
    }
}

/// A window or layer the pointer can pick, with the label shown on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub rect: Rect,
    pub label: String,
}

/// A monitor's place in the layout. `width` and `height` are physical pixels, like grim's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub workspace: i64,
    pub focused: bool,
}

impl Monitor {
    fn logical_size(&self) -> (f32, f32) {
        (self.width as f32 / self.scale, self.height as f32 / self.scale)
    }
}

/// Every monitor in `hyprctl monitors -j`.
pub fn parse_monitors(json: &serde_json::Value) -> Vec<Monitor> {
    let Some(monitors) = json.as_array() else {
        return Vec::new();
    };
    monitors
        .iter()
        .filter_map(|monitor| {
            Some(Monitor {
                name: monitor["name"].as_str()?.to_owned(),
                x: monitor["x"].as_f64()? as f32,
                y: monitor["y"].as_f64()? as f32,
                width: monitor["width"].as_u64()? as u32,
                height: monitor["height"].as_u64()? as u32,
                scale: monitor["scale"].as_f64().filter(|scale| *scale > 0.0).unwrap_or(1.0) as f32,
                workspace: monitor["activeWorkspace"]["id"].as_i64()?,
                focused: monitor["focused"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// What a click can pick on `monitor`, in pick order: its top-level layers that are not bars,
/// docks or Zehll's own, then the floating windows of its active workspace (most recently
/// focused first), then the tiled ones. Each is clipped to the monitor.
pub fn regions(monitor: &Monitor, clients: &serde_json::Value, layers: &serde_json::Value) -> Vec<Region> {
    let (width, height) = monitor.logical_size();
    let screen = Rect { x: monitor.x, y: monitor.y, width, height };
    // Global layout coordinates to the monitor's own, dropping what lies outside it.
    let local = |x: f64, y: f64, width: f64, height: f64, label: &str| {
        let rect = Rect { x: x as f32, y: y as f32, width: width as f32, height: height as f32 }.intersection(&screen)?;
        Some(Region { rect: Rect { x: rect.x - monitor.x, y: rect.y - monitor.y, ..rect }, label: label.to_owned() })
    };

    // Level 2 is Hyprland's top layer, where popups and launchers live (ii looks only there).
    let empty = Vec::new();
    let top = layers[&monitor.name]["levels"]["2"].as_array().unwrap_or(&empty);
    let layers = top.iter().filter_map(|layer| {
        let namespace = layer["namespace"].as_str()?;
        let skip = namespace.contains("bar") || namespace.contains("dock") || namespace.starts_with("zehll");
        let (w, h) = (layer["w"].as_f64()?, layer["h"].as_f64()?);
        // Invisible helper surfaces (ii keeps a 1 x 1 one) are not something to pick.
        if skip || w < 8.0 || h < 8.0 {
            return None;
        }
        local(layer["x"].as_f64()?, layer["y"].as_f64()?, w, h, namespace)
    });

    let mut windows: Vec<&serde_json::Value> = clients
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|client| {
            client["workspace"]["id"].as_i64() == Some(monitor.workspace)
                && client["mapped"].as_bool() != Some(false)
                && client["hidden"].as_bool() != Some(true)
        })
        .collect();
    // Floating windows sit above tiled ones; among them the most recently focused is on top.
    windows.sort_by_key(|client| (client["floating"].as_bool() != Some(true), client["focusHistoryID"].as_i64().unwrap_or(i64::MAX)));
    let windows = windows.into_iter().filter_map(|client| {
        let (at, size) = (&client["at"], &client["size"]);
        local(at[0].as_f64()?, at[1].as_f64()?, size[0].as_f64()?, size[1].as_f64()?, client["class"].as_str()?)
    });
    layers.chain(windows).collect()
}

/// The rectangle between a drag's start and end, whichever way it went.
pub fn selection(start: Point, end: Point) -> Rect {
    Rect { x: start.x.min(end.x), y: start.y.min(end.y), width: (end.x - start.x).abs(), height: (end.y - start.y).abs() }
}

pub fn is_click(start: Point, end: Point) -> bool {
    (end.x - start.x).abs() < CLICK_SLOP && (end.y - start.y).abs() < CLICK_SLOP
}

/// The region a click at `point` picks.
pub fn target(regions: &[Region], point: Point) -> Option<&Region> {
    regions.iter().find(|region| region.rect.contains(point))
}

/// `rect` in the physical pixels of a `frame` captured at `scale`, as (x, y, width, height),
/// clamped to the frame; `None` when nothing is left.
pub fn crop(rect: Rect, scale: f32, frame: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let edge = |value: f32, limit: u32| (value * scale).round().clamp(0.0, limit as f32) as u32;
    let (left, right) = (edge(rect.x, frame.0), edge(rect.x + rect.width, frame.0));
    let (top, bottom) = (edge(rect.y, frame.1), edge(rect.y + rect.height, frame.1));
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

/// Size readout of a selection, in physical pixels: `1320 × 700`.
pub fn size_label(rect: Rect, scale: f32) -> String {
    format!("{} × {}", (rect.width * scale).round(), (rect.height * scale).round())
}

/// ii's file name: `screenshot-2026-11-23_14.05.09.png`.
pub fn file_name(now: NaiveDateTime) -> String {
    now.format("screenshot-%Y-%m-%d_%H.%M.%S.png").to_string()
}

/// One monitor frozen for the selector: its frame and what a click can pick on it.
#[derive(Clone)]
pub struct Shot {
    pub monitor: Monitor,
    pub frame: Arc<RgbaImage>,
    pub handle: Handle,
    pub regions: Vec<Region>,
}

impl std::fmt::Debug for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shot").field("monitor", &self.monitor).field("regions", &self.regions.len()).finish()
    }
}

/// Freezes every monitor: one `grim` per output, all at once, before any surface opens.
pub async fn capture() -> Vec<Shot> {
    let Ok(Some((monitors, clients, layers))) = tokio::task::spawn_blocking(hyprland::layout).await else {
        eprintln!("zehll: cannot read the Hyprland layout for the snip");
        return Vec::new();
    };
    let grabs = parse_monitors(&monitors).into_iter().map(|monitor| {
        let regions = regions(&monitor, &clients, &layers);
        async move {
            let frame = grab(&monitor.name).await?;
            let handle = Handle::from_rgba(frame.width(), frame.height(), frame.as_raw().clone());
            Some(Shot { monitor, frame: Arc::new(frame), handle, regions })
        }
    });
    iced::futures::future::join_all(grabs).await.into_iter().flatten().collect()
}

/// The output's current frame; PPM skips compressing a picture that is decoded right away.
async fn grab(output: &str) -> Option<RgbaImage> {
    let shot = tokio::process::Command::new("grim").args(["-o", output, "-t", "ppm", "-"]).output().await;
    let bytes = match shot {
        Ok(shot) if shot.status.success() => shot.stdout,
        Ok(shot) => {
            eprintln!("zehll: grim failed on {output}: {}", String::from_utf8_lossy(&shot.stderr).trim());
            return None;
        }
        Err(err) => {
            eprintln!("zehll: cannot run grim: {err}");
            return None;
        }
    };
    let decode = move || ::image::load_from_memory_with_format(&bytes, ::image::ImageFormat::Pnm).map(|frame| frame.to_rgba8());
    match tokio::task::spawn_blocking(decode).await {
        Ok(Ok(frame)) => Some(frame),
        Ok(Err(err)) => {
            eprintln!("zehll: cannot decode the frame of {output}: {err}");
            None
        }
        Err(_) => None,
    }
}

/// What happens to the selection: copy it (and save it), or open it in swappy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy { save: bool },
    Edit,
}

/// Crops `frame` to `area` (physical pixels) and copies, saves or edits it.
pub async fn deliver(frame: Arc<RgbaImage>, area: (u32, u32, u32, u32), action: Action) {
    let encode = move || {
        let (x, y, width, height) = area;
        let crop = ::image::imageops::crop_imm(frame.as_ref(), x, y, width, height).to_image();
        let mut png = Vec::new();
        crop.write_to(&mut std::io::Cursor::new(&mut png), ::image::ImageFormat::Png).map(|()| png)
    };
    let png = match tokio::task::spawn_blocking(encode).await {
        Ok(Ok(png)) => png,
        Ok(Err(err)) => return eprintln!("zehll: cannot encode the snip: {err}"),
        Err(_) => return,
    };
    match action {
        Action::Edit => {
            // swappy keeps running on its own; the runtime reaps it when it exits.
            pipe("swappy", &["-f", "-"], &png, false).await;
        }
        Action::Copy { save } => {
            if save {
                let dir = screenshots_dir().await;
                let path = dir.join(file_name(chrono::Local::now().naive_local()));
                let written = tokio::fs::create_dir_all(&dir).await.and(tokio::fs::write(&path, &png).await);
                if let Err(err) = written {
                    eprintln!("zehll: cannot save {}: {err}", path.display());
                }
            }
            pipe("wl-copy", &["--type", "image/png"], &png, true).await;
        }
    }
}

/// Starts `program` with `input` on its stdin, waiting for it when `wait`.
async fn pipe(program: &str, args: &[&str], input: &[u8], wait: bool) {
    let child = tokio::process::Command::new(program).args(args).stdin(Stdio::piped()).spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => return eprintln!("zehll: cannot run {program}: {err}"),
    };
    if let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(input).await
    {
        eprintln!("zehll: cannot write to {program}: {err}");
    }
    if wait {
        let _ = child.wait().await;
    }
}

/// `$(xdg-user-dir PICTURES)/Screenshots`, like ii and the Print binds.
async fn screenshots_dir() -> PathBuf {
    let pictures = tokio::process::Command::new("xdg-user-dir").arg("PICTURES").output().await.ok();
    let pictures = pictures
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Pictures"));
    pictures.join("Screenshots")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn monitor() -> Monitor {
        Monitor { name: "DP-1".into(), x: 1920.0, y: 0.0, width: 2560, height: 1440, scale: 1.0, workspace: 1, focused: true }
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn monitors_keep_their_layout_scale_and_workspace() {
        let json = json!([
            { "name": "DP-1", "x": 1920, "y": 0, "width": 2560, "height": 1440, "scale": 1.0, "focused": true, "activeWorkspace": { "id": 1 } },
            { "name": "eDP-1", "x": 0, "y": 0, "width": 2880, "height": 1800, "scale": 1.5, "focused": false, "activeWorkspace": { "id": 5 } },
            { "name": "broken" },
        ]);
        assert_eq!(
            parse_monitors(&json),
            [
                monitor(),
                Monitor { name: "eDP-1".into(), x: 0.0, y: 0.0, width: 2880, height: 1800, scale: 1.5, workspace: 5, focused: false },
            ]
        );
    }

    #[test]
    fn layers_come_first_then_floating_then_tiled_windows_of_the_active_workspace() {
        let clients = json!([
            { "class": "kitty", "at": [1930, 50], "size": [800, 600], "workspace": { "id": 1 }, "floating": false, "hidden": false, "mapped": true, "focusHistoryID": 0 },
            { "class": "pavucontrol", "at": [2500, 300], "size": [500, 400], "workspace": { "id": 1 }, "floating": true, "hidden": false, "mapped": true, "focusHistoryID": 3 },
            { "class": "mpv", "at": [2600, 350], "size": [400, 300], "workspace": { "id": 1 }, "floating": true, "hidden": false, "mapped": true, "focusHistoryID": 1 },
            { "class": "firefox", "at": [0, 0], "size": [1920, 1080], "workspace": { "id": 5 }, "floating": false, "hidden": false, "mapped": true, "focusHistoryID": 2 },
            { "class": "hidden", "at": [1930, 50], "size": [800, 600], "workspace": { "id": 1 }, "floating": false, "hidden": true, "mapped": true, "focusHistoryID": 4 },
        ]);
        let layers = json!({
            "DP-1": { "levels": {
                "2": [
                    { "namespace": "quickshell:bar", "x": 1920, "y": 0, "w": 2560, "h": 63 },
                    { "namespace": "zehll-system", "x": 2820, "y": 58, "w": 760, "h": 456 },
                    { "namespace": "rofi", "x": 2920, "y": 400, "w": 600, "h": 500 },
                    { "namespace": "quickshell", "x": 4479, "y": 1439, "w": 1, "h": 1 },
                ],
                "3": [{ "namespace": "notifications", "x": 4000, "y": 10, "w": 400, "h": 100 }],
            } },
            "HDMI-A-2": { "levels": { "2": [{ "namespace": "launcher", "x": 0, "y": 0, "w": 500, "h": 500 }] } },
        });
        let regions = regions(&monitor(), &clients, &layers);
        let labels: Vec<&str> = regions.iter().map(|region| region.label.as_str()).collect();
        assert_eq!(labels, ["rofi", "mpv", "pavucontrol", "kitty"]);
        assert_eq!(regions[0].rect, rect(1000.0, 400.0, 600.0, 500.0));
        assert_eq!(regions[3].rect, rect(10.0, 50.0, 800.0, 600.0));
    }

    #[test]
    fn regions_are_clipped_to_the_monitor_and_dropped_when_outside() {
        let clients = json!([
            { "class": "wide", "at": [1800, 1300], "size": [400, 400], "workspace": { "id": 1 }, "floating": true, "hidden": false, "mapped": true, "focusHistoryID": 0 },
            { "class": "away", "at": [0, 0], "size": [400, 400], "workspace": { "id": 1 }, "floating": true, "hidden": false, "mapped": true, "focusHistoryID": 1 },
        ]);
        let regions = regions(&monitor(), &clients, &json!({}));
        assert_eq!(regions, [Region { rect: rect(0.0, 1300.0, 280.0, 140.0), label: "wide".into() }]);
    }

    #[test]
    fn a_drag_selects_the_rectangle_between_its_ends_in_any_direction() {
        assert_eq!(selection(Point::new(500.0, 300.0), Point::new(100.0, 400.0)), rect(100.0, 300.0, 400.0, 100.0));
        assert_eq!(selection(Point::new(10.0, 20.0), Point::new(30.0, 60.0)), rect(10.0, 20.0, 20.0, 40.0));
    }

    #[test]
    fn a_short_move_counts_as_a_click() {
        assert!(is_click(Point::new(100.0, 100.0), Point::new(103.0, 97.0)));
        assert!(!is_click(Point::new(100.0, 100.0), Point::new(104.0, 100.0)));
        assert!(!is_click(Point::new(100.0, 100.0), Point::new(100.0, 90.0)));
    }

    #[test]
    fn a_click_picks_the_first_region_under_the_pointer() {
        let regions = [
            Region { rect: rect(100.0, 100.0, 100.0, 100.0), label: "popup".into() },
            Region { rect: rect(0.0, 0.0, 500.0, 500.0), label: "window".into() },
        ];
        assert_eq!(target(&regions, Point::new(150.0, 150.0)).map(|region| region.label.as_str()), Some("popup"));
        assert_eq!(target(&regions, Point::new(50.0, 450.0)).map(|region| region.label.as_str()), Some("window"));
        assert_eq!(target(&regions, Point::new(600.0, 50.0)), None);
    }

    #[test]
    fn crops_scale_to_physical_pixels_and_stay_inside_the_frame() {
        assert_eq!(crop(rect(10.0, 20.0, 300.0, 200.0), 1.0, (2560, 1440)), Some((10, 20, 300, 200)));
        assert_eq!(crop(rect(10.0, 20.0, 300.0, 200.0), 1.5, (2880, 1800)), Some((15, 30, 450, 300)));
        assert_eq!(crop(rect(-50.0, 1400.0, 300.0, 100.0), 1.0, (2560, 1440)), Some((0, 1400, 250, 40)));
        assert_eq!(crop(rect(2600.0, 10.0, 100.0, 100.0), 1.0, (2560, 1440)), None);
        assert_eq!(crop(rect(10.0, 10.0, 0.2, 100.0), 1.0, (2560, 1440)), None);
    }

    #[test]
    fn the_size_readout_is_in_physical_pixels() {
        assert_eq!(size_label(rect(0.0, 0.0, 1320.0, 700.0), 1.0), "1320 × 700");
        assert_eq!(size_label(rect(0.0, 0.0, 880.0, 466.7), 1.5), "1320 × 700");
    }

    #[test]
    fn saved_files_are_named_like_ii() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap().and_hms_opt(14, 5, 9).unwrap();
        assert_eq!(file_name(now), "screenshot-2026-11-23_14.05.09.png");
    }
}
