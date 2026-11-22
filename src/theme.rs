use std::path::PathBuf;
use std::time::SystemTime;

use iced::Color;

/// HUD palette: a fixed navy base with accent colors taken from a matugen Material 3 palette.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bar_background: Color,
    pub bar_border: Color,
    pub popup_background: Color,
    pub border: Color,
    pub section_background: Color,
    pub bracket: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub glow: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bar_background: Color::from_rgb8(0x07, 0x0c, 0x1d),
            bar_border: Color::from_rgb8(0x1c, 0x28, 0x46),
            popup_background: Color::from_rgb8(0x09, 0x0e, 0x20),
            border: Color::from_rgb8(0x22, 0x30, 0x4f),
            section_background: Color::from_rgb8(0x15, 0x20, 0x39),
            bracket: Color::from_rgb8(0x5d, 0x6b, 0x8f),
            text: Color::from_rgb8(0xee, 0xf2, 0xfb),
            text_muted: Color::from_rgb8(0x9a, 0xa6, 0xc4),
            text_dim: Color::from_rgb8(0x4a, 0x56, 0x73),
            accent: Color::from_rgb8(0xf5, 0xa6, 0x23),
            on_accent: Color::from_rgb8(0x12, 0x0c, 0x02),
            glow: Color::from_rgb8(0x2d, 0xe2, 0xe6),
        }
    }
}

impl Theme {
    /// Loads the accent colors from the theme file, keeping the defaults for anything missing.
    pub fn load() -> Self {
        let palette: serde_json::Value = std::fs::read_to_string(path())
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        let color = |key: &str, fallback: Color| palette[key].as_str().and_then(|hex| hex.parse().ok()).unwrap_or(fallback);

        let default = Self::default();
        Self {
            accent: color("primary", default.accent),
            on_accent: color("on_primary", default.on_accent),
            glow: color("tertiary", default.glow),
            ..default
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
