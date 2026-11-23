//! Volume indicator shown when the default output's volume or mute changes (keys, a headset
//! button, an app), hidden a moment after the last change.

use std::time::{Duration, Instant};

use iced::widget::{Row, Space, Stack, column, container, mouse_area, rich_text, row, span, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::audio::Device;
use crate::{BODY_FONT, Bar, CONDENSED_HEAVY, Message, shape, slanted};

pub const SIZE: (u32, u32) = (320, 84);
pub const NAMESPACE: &str = "zehll-osd";
/// The indicator hides this long after the last change.
pub const HOLD: Duration = Duration::from_millis(1500);
const CELLS: usize = 20;
const BOOST: Color = Color::from_rgb8(0xff, 0xd2, 0x3f);
const MUTED: Color = Color::from_rgb8(0xff, 0x4d, 0x5e);
const MUTED_CELL: Color = Color::from_rgb8(0x3a, 0x45, 0x66);

#[derive(Debug, Default)]
pub struct Osd {
    /// Last reading of the default output, to tell changes from the first read.
    pub last: Option<Device>,
    /// The reading shown and when it hides.
    pub shown: Option<(Device, Instant)>,
    pub surface: Option<iced::window::Id>,
}

/// Whether going from `previous` to `now` is a change to show: the same device with another
/// volume or mute. The first read and a switch of the default device are not.
pub fn changed(previous: Option<&Device>, now: &Device) -> bool {
    previous.is_some_and(|previous| {
        previous.name == now.name && (percent(previous.volume) != percent(now.volume) || previous.muted != now.muted)
    })
}

/// Lit cells of the 20-cell gauge (5 % each), full above 100 %.
pub fn lit(volume: f32) -> usize {
    (percent(volume.min(1.0)) as usize / 5).min(CELLS)
}

/// Volume as a whole percentage.
pub fn percent(volume: f32) -> u32 {
    (volume * 100.0).round().max(0.0) as u32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Volume,
    Boost,
    Muted,
}

pub fn mode(device: &Device) -> Mode {
    match (device.muted, percent(device.volume) > 100) {
        (true, _) => Mode::Muted,
        (false, true) => Mode::Boost,
        (false, false) => Mode::Volume,
    }
}

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let Some((device, _)) = &bar.osd.shown else {
        return Space::new().into();
    };
    let theme = bar.theme;
    let mode = mode(device);
    let (label, chip_color, value_color, cell_color, corners) = match mode {
        Mode::Volume => ("VOLUME", theme.accent, theme.text, theme.accent, theme.accent),
        Mode::Boost => ("BOOST", theme.accent, BOOST, BOOST, theme.accent),
        Mode::Muted => ("MUTED", MUTED, theme.text_dim, MUTED_CELL, theme.bracket),
    };
    let chip = container(text(label).font(CONDENSED_HEAVY).size(14).color(theme.on_accent))
        .padding(Padding { top: 0.0, right: 18.0, bottom: 0.0, left: 9.0 })
        .center_y(24);
    let header = row![
        slanted(chip, 8.0, chip_color),
        text(device.name.to_uppercase()).font(BODY_FONT).size(12).color(theme.text_muted).wrapping(text::Wrapping::None).width(Length::Fill),
        // Struck through while muted; the level stays readable.
        rich_text::<(), _, _, _>([span(format!("{}%", percent(device.volume))).font(CONDENSED_HEAVY).size(28).color(value_color).strikethrough(mode == Mode::Muted)]),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let lit = lit(device.volume);
    let gauge = Row::with_children((0..CELLS).map(|cell| {
        let color = if cell < lit { cell_color } else { theme.section_background };
        container(Space::new()).width(Length::Fill).height(8).style(move |_| container::background(color)).into()
    }))
    .spacing(3);
    let glow = mode != Mode::Muted;
    let card = container(column![header, gauge].spacing(10))
        .padding(Padding { top: 12.0, right: 14.0, bottom: 14.0, left: 14.0 })
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(theme.popup_background.into()),
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            shadow: match glow {
                true => Shadow { color: Color { a: 0.18, ..theme.glow }, blur_radius: 16.0, ..Default::default() },
                false => Shadow::default(),
            },
            ..Default::default()
        });
    let osd = Stack::new().push(card).push(shape(shape::Kind::Brackets { arm: 14.0, width: 2.0 }, corners));
    // Touching it hides it at once, like ii.
    mouse_area(osd).on_enter(Message::HideOsd).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(volume: f32, muted: bool) -> Device {
        Device { name: String::from("Soundcore Life Q20"), volume, muted }
    }

    #[test]
    fn shows_volume_and_mute_changes_only() {
        assert!(!changed(None, &device(0.5, false)), "first read");
        assert!(!changed(Some(&device(0.5, false)), &device(0.5, false)));
        assert!(changed(Some(&device(0.5, false)), &device(0.52, false)));
        assert!(changed(Some(&device(0.5, false)), &device(0.5, true)));
        let other = Device { name: String::from("Easy Effects Sink"), ..device(0.3, false) };
        assert!(!changed(Some(&device(0.5, false)), &other), "default device switched");
    }

    #[test]
    fn gauge_fills_by_five_percent_and_caps_at_full() {
        assert_eq!(lit(0.0), 0);
        assert_eq!(lit(0.4), 8);
        assert_eq!(lit(0.62), 12);
        assert_eq!(lit(1.0), 20);
        assert_eq!(lit(1.25), 20);
    }

    #[test]
    fn percent_rounds() {
        assert_eq!(percent(0.69), 69);
        assert_eq!(percent(1.25), 125);
        assert_eq!(percent(0.996), 100);
    }

    #[test]
    fn mode_marks_boost_and_mute() {
        assert_eq!(mode(&device(0.4, false)), Mode::Volume);
        assert_eq!(mode(&device(1.0, false)), Mode::Volume);
        assert_eq!(mode(&device(1.25, false)), Mode::Boost);
        assert_eq!(mode(&device(1.25, true)), Mode::Muted);
    }
}
