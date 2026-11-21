use std::path::PathBuf;
use std::time::SystemTime;

use iced::Color;

/// UI roles mapped from a Material 3 palette in matugen's `colors.json` format.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bar_background: Color,
    pub popup_background: Color,
    pub text: Color,
    pub text_muted: Color,
    pub border: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bar_background: Color::from_rgb8(0x1d, 0x15, 0x15),
            popup_background: Color::from_rgb8(0x2a, 0x20, 0x20),
            text: Color::from_rgb8(0xf1, 0xdf, 0xdb),
            text_muted: Color::from_rgb8(0xb0, 0xa0, 0xa0),
            border: Color::from_rgb8(0x4a, 0x3c, 0x3c),
        }
    }
}

impl Theme {
    /// Loads the theme file, falling back to the default role by role.
    pub fn load() -> Self {
        let palette: serde_json::Value = std::fs::read_to_string(path())
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        let color = |key: &str, fallback: Color| palette[key].as_str().and_then(|hex| hex.parse().ok()).unwrap_or(fallback);

        let default = Self::default();
        Self {
            bar_background: color("background", default.bar_background),
            popup_background: color("surface_container_low", default.popup_background),
            text: color("on_surface", default.text),
            text_muted: color("on_surface_variant", default.text_muted),
            border: color("outline_variant", default.border),
        }
    }
}

/// Modification time of the theme file, used to reload it when matugen rewrites it.
pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|metadata| metadata.modified()).ok()
}

fn path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    config.join("zehll/theme.json")
}
