//! Alert cards for meetings that are starting: persistent until joined or dismissed.

use iced::widget::{Column, Space, Stack, button, column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::meetings::{self, Meeting};
use crate::{BODY_BOLD, BODY_FONT, Bar, CONDENSED_BOLD, CONDENSED_HEAVY, Message, agenda, icon_sized, shape, slanted};

use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub const WIDTH: u32 = 400;
/// The alert rings again this often while any card is open.
pub const RING_INTERVAL: Duration = Duration::from_secs(5);
const RING_SOUND: &str = "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga";
const CARD_HEIGHT: u32 = 152;
const SPACING: u32 = 10;
const RING: Color = Color::from_rgb8(0xff, 0xd2, 0x3f);
const RING_STATUS: Color = Color::from_rgb8(0xff, 0xd2, 0x7a);

/// Height of the surface stacking `cards` alert cards; 0 closes it.
pub fn height(cards: usize) -> u32 {
    let cards = cards as u32;
    cards * CARD_HEIGHT + SPACING * cards.saturating_sub(1)
}

/// Plays the alert sound once, stopping the previous ring first: the sound lasts about 6 s,
/// longer than `RING_INTERVAL`, so rings would otherwise overlap.
pub fn ring(previous: Option<Child>) -> Option<Child> {
    stop(previous);
    match Command::new("pw-play").arg(RING_SOUND).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => Some(child),
        Err(err) => {
            eprintln!("zehll: cannot play the meeting alert sound: {err}");
            None
        }
    }
}

/// Stops a ring still playing and reaps its process.
pub fn stop(ring: Option<Child>) {
    if let Some(mut child) = ring {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The first card is the one ringing; the rest started at the same time.
pub fn view(bar: &Bar) -> Element<'_, Message> {
    let cards = bar.meeting_alerts.iter().enumerate().map(|(index, meeting)| card(bar, meeting, index == 0));
    container(Column::with_children(cards).spacing(SPACING as f32))
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into()
}

fn card<'a>(bar: &'a Bar, meeting: &'a Meeting, ringing: bool) -> Element<'a, Message> {
    let theme = bar.theme;
    let now = bar.now.naive_local();
    let (tag_background, tag_text) = match ringing {
        true => (RING, theme.on_accent),
        false => (theme.section_background, theme.text_muted),
    };
    let tag = row![
        container(icon_sized("notifications", 14.0)).style(move |_| container::Style {
            text_color: Some(tag_text),
            ..Default::default()
        }),
        text(if ringing { "MEETING NOW" } else { "ALSO NOW" }).font(CONDENSED_HEAVY).size(14).color(tag_text),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let tag = container(tag).padding(Padding { top: 0.0, right: 20.0, bottom: 0.0, left: 10.0 }).center_y(26);
    let status = text(meetings::started_label(meeting, now))
        .font(CONDENSED_BOLD)
        .size(13)
        .color(if ringing { RING_STATUS } else { theme.text_muted });
    let header = row![slanted(tag, 9.0, tag_background), Space::new().width(Length::Fill), status].align_y(Alignment::Center);

    let details = row![
        text(meeting.start.format("%H:%M").to_string())
            .font(CONDENSED_HEAVY)
            .size(30)
            .color(if ringing { theme.accent } else { theme.text }),
        column![
            text(&meeting.title).font(BODY_BOLD).size(17).wrapping(text::Wrapping::None),
            text(meetings::alert_meta(meeting)).font(BODY_FONT).size(12).color(theme.text_muted).wrapping(text::Wrapping::None),
        ]
        .spacing(3)
        .width(Length::Fill)
        .clip(true),
    ]
    .spacing(12);

    let key = (meeting.id.clone(), meeting.start);
    let dismiss = Stack::new()
        .push(container(text("DISMISS").font(CONDENSED_BOLD).size(14).color(theme.text_muted)).padding([0, 16]).center_y(32))
        .push(shape(shape::Kind::BRACKETS, theme.bracket));
    let dismiss = button(dismiss)
        .on_press(Message::DismissMeetingAlert(key.0.clone(), key.1))
        .padding(0)
        .style(|_, _| button::Style::default());
    let join = agenda::join_button(bar, meeting, Message::JoinMeetingAlert(key.0, key.1));
    let actions = row![Space::new().width(Length::Fill), dismiss, join].spacing(10).align_y(Alignment::Center);

    let body = container(column![header, details, actions].spacing(12))
        .padding(Padding { top: 14.0, right: 16.0, bottom: 16.0, left: 16.0 })
        .width(Length::Fill)
        .height(CARD_HEIGHT as f32)
        .style(move |_| match ringing {
            true => container::Style {
                background: Some(theme.popup_background.into()),
                border: Border { color: theme.glow, width: 1.0, radius: 0.0.into() },
                shadow: Shadow { color: Color { a: 0.35, ..theme.glow }, blur_radius: 18.0, ..Default::default() },
                ..Default::default()
            },
            false => container::Style {
                background: Some(theme.popup_background.into()),
                border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
                ..Default::default()
            },
        });
    let corners = if ringing { theme.accent } else { theme.bracket };
    Stack::new().push(body).push(shape(shape::Kind::Brackets { arm: 16.0, width: 2.0 }, corners)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_height_stacks_cards() {
        assert_eq!(height(0), 0);
        assert_eq!(height(1), 152);
        assert_eq!(height(2), 314);
    }
}
