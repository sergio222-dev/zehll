use iced::widget::{Column, container, mouse_area};
use iced::{Element, Length};

use crate::{Bar, Message, notifications, panel};

pub const WIDTH: u32 = 380;
pub const MAX_VISIBLE: usize = 3;
const SPACING: u32 = 8;
// Rough layout of a card; the surface needs its size before the text is laid out.
const CARD_CHROME: u32 = 16 + 20 + 4 + 20;
const BODY_LINE_HEIGHT: u32 = 18;
const BODY_CHAR_WIDTH: f32 = 6.5;
const MAX_BODY_LINES: u32 = 4;
const ACTIONS_HEIGHT: u32 = 26 + 4;

/// Height of the popup surface holding `notifications`.
pub fn height<'a>(notifications: impl Iterator<Item = &'a notifications::Notification>) -> u32 {
    let chars_per_line = ((WIDTH - 20) as f32 / BODY_CHAR_WIDTH) as u32;
    let cards: Vec<u32> = notifications
        .map(|notification| {
            let chars = notification.body.chars().count() as u32;
            let lines = if chars == 0 { 0 } else { chars.div_ceil(chars_per_line).min(MAX_BODY_LINES) };
            let actions = if notification.buttons().next().is_some() { ACTIONS_HEIGHT } else { 0 };
            CARD_CHROME + lines * (BODY_LINE_HEIGHT + 1) + if lines > 0 { 4 } else { 0 } + actions
        })
        .collect();
    cards.iter().sum::<u32>() + SPACING * (cards.len() as u32).saturating_sub(1)
}

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let cards = bar.popups.iter().filter_map(|popup| {
        let notification = bar.notifications.iter().find(|notification| notification.id == popup.id)?;
        // Clicking a popup without a default action hides it; the notification stays in the panel.
        let card = panel::notification_card(bar, notification);
        Some(match notification.has_default_action() {
            true => card,
            false => mouse_area(card).on_press(Message::HidePopup(popup.id)).into(),
        })
    });
    container(Column::with_children(cards).spacing(SPACING as f32)).width(Length::Fill).height(Length::Fill).clip(true).into()
}
