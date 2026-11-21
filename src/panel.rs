use chrono::{Datelike, NaiveDate};
use iced::widget::{Column, Row, Space, Stack, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use crate::{
    BODY_BOLD, Bar, CONDENSED, CONDENSED_BOLD, CONDENSED_HEAVY, ICON_FONT, Message, audio, calendar, glow_style, line,
    network, nightlight, notifications, panel_style, shape, slanted, system,
};

pub const WIDTH: u32 = 460;
const PADDING: u16 = 16;
const DAY_HEIGHT: f32 = 34.0;
const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let content = column![header(bar)]
        .push(quick_settings(bar))
        .push(notifications_view(bar))
        .push(calendar_view(bar))
        .spacing(14)
        .padding(PADDING);
    let content = container(content).width(Length::Fill).height(Length::Fill).style(move |_| {
        iced::widget::container::background(theme.bar_background)
    });
    row![line(theme.bar_border).width(1).height(Length::Fill), content].into()
}

fn header(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let uptime = row![
        text("UPTIME").font(CONDENSED).size(13).color(theme.on_accent),
        text(system::format_uptime(bar.uptime).replace(',', "").to_uppercase()).font(CONDENSED_HEAVY).size(20).color(theme.on_accent),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let uptime = container(uptime)
        .padding(Padding { top: 0.0, right: 30.0, bottom: 0.0, left: 14.0 })
        .center_y(40);

    let actions = row![
        action_button(bar, "refresh", Message::ReloadShell),
        action_button(bar, "power_settings_new", Message::PowerMenu),
    ]
    .spacing(8);

    row![slanted(uptime, 16.0, theme.accent), Space::new().width(Length::Fill), actions]
        .height(40)
        .align_y(Alignment::Center)
        .into()
}

fn action_button<'a>(bar: &'a Bar, icon: &'a str, message: Message) -> Element<'a, Message> {
    let theme = bar.theme;
    let content = Stack::new()
        .push(container(crate::icon_sized(icon, 18.0)).center(34))
        .push_under(crate::shape(shape::Kind::Brackets, theme.bracket));
    button(content)
        .on_press(message)
        .padding(0)
        .style(move |_, status| glow_style(theme, status, 10.0))
        .into()
}

struct Toggle {
    icon: &'static str,
    name: &'static str,
    state: String,
    on: bool,
    message: Message,
}

fn quick_settings(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let mut toggles = Vec::new();
    if let Some(network) = &bar.network {
        toggles.push(Toggle {
            icon: match (network.connected, network.kind) {
                (false, _) => "wifi_off",
                (true, network::Kind::Wired) => "lan",
                (true, _) => "wifi",
            },
            name: "Internet",
            state: network.label(),
            on: network.connected,
            message: Message::SetWifi(!network.wifi_enabled),
        });
    }
    if let Some(bluetooth) = &bar.bluetooth {
        toggles.push(Toggle {
            icon: match (bluetooth.powered, bluetooth.device.is_some()) {
                (false, _) => "bluetooth_disabled",
                (true, true) => "bluetooth_connected",
                (true, false) => "bluetooth",
            },
            name: "Bluetooth",
            state: bluetooth.label(),
            on: bluetooth.powered,
            message: Message::SetBluetooth(bluetooth.adapter.clone(), !bluetooth.powered),
        });
    }
    if let Some(audio) = &bar.audio {
        let output = &audio.output;
        toggles.push(Toggle {
            icon: if output.muted { "volume_off" } else { "volume_up" },
            name: "Audio output",
            state: if output.muted { String::from("MUTED") } else { format!("UNMUTED · {:.0}%", output.volume * 100.0) },
            on: !output.muted,
            message: Message::ToggleMute(audio::OUTPUT),
        });
        toggles.push(Toggle {
            icon: if audio.input.muted { "mic_off" } else { "mic" },
            name: "Microphone",
            state: String::from(if audio.input.muted { "MUTED" } else { "LIVE" }),
            on: !audio.input.muted,
            message: Message::ToggleMute(audio::INPUT),
        });
    }
    if let Some(temperature) = bar.night_light {
        let on = nightlight::is_on(temperature);
        toggles.push(Toggle {
            icon: if on { "bedtime" } else { "bedtime_off" },
            name: "Night light",
            state: {
                let state = if on { format!("ON · {temperature}K") } else { String::from("OFF") };
                // "AUTO" while the light follows the schedule, as ii shows it.
                if bar.night_schedule == Some(on) { format!("AUTO · {state}") } else { state }
            },
            on,
            message: Message::SetNightLight(!on),
        });
    }
    let keep_awake = bar.keep_awake.is_some();
    toggles.push(Toggle {
        icon: "coffee",
        name: "Keep awake",
        state: String::from(if keep_awake { "ACTIVE" } else { "OFF" }),
        on: keep_awake,
        message: Message::ToggleKeepAwake,
    });

    let on = toggles.iter().filter(|toggle| toggle.on).count();
    let strip = row![
        text("QUICK SETTINGS").font(CONDENSED).size(14).color(theme.accent),
        Space::new().width(Length::Fill),
        text(format!("{on} / {} ON", toggles.len())).font(CONDENSED_BOLD).size(14).color(theme.text_muted),
    ]
    .align_y(Alignment::Center)
    .padding([0, 14])
    .height(26);
    let strip = container(slanted(strip, 10.0, theme.section_background)).padding(Padding::ZERO.right(16.0));

    let mut tiles = toggles.into_iter().map(|toggle| tile(bar, toggle)).collect::<Vec<_>>().into_iter();
    let mut grid = Column::new().spacing(8);
    while let Some(first) = tiles.next() {
        let second = tiles.next().unwrap_or_else(|| Space::new().width(Length::Fill).into());
        grid = grid.push(row![first, second].spacing(8));
    }
    column![strip, grid].spacing(8).into()
}

fn tile(bar: &Bar, toggle: Toggle) -> Element<'_, Message> {
    let theme = bar.theme;
    let (block_color, icon_color, state_color) = match toggle.on {
        true => (theme.accent, theme.on_accent, theme.accent),
        false => (theme.section_background, theme.text, theme.text_muted),
    };
    let icon = container(text(toggle.icon).font(ICON_FONT).size(20).color(icon_color).shaping(text::Shaping::Advanced))
        .padding(Padding::ZERO.right(8.0))
        .center_x(46)
        .center_y(56);
    let label = column![
        text(toggle.name).font(BODY_BOLD).size(14),
        text(toggle.state).font(CONDENSED_BOLD).size(14).color(state_color),
    ]
    .spacing(1);
    let content = row![slanted(icon, 10.0, block_color), label].spacing(10).align_y(Alignment::Center);
    button(content)
        .on_press(toggle.message)
        .padding(0)
        .width(Length::Fill)
        .height(56)
        .style(move |_, status| {
            let mut style = glow_style(theme, status, 10.0);
            if style.background.is_none() {
                style.background = Some(theme.popup_background.into());
                style.border = iced::Border { color: theme.border, width: 1.0, radius: 0.0.into() };
            }
            style
        })
        .into()
}

fn notifications_view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let count = bar.notifications.len();
    let strip = row![
        text("NOTIFICATIONS").font(CONDENSED).size(14).color(theme.accent),
        Space::new().width(Length::Fill),
        text(if bar.do_not_disturb { String::from("DO NOT DISTURB") } else { format!("{count} NEW") })
            .font(CONDENSED_BOLD)
            .size(14)
            .color(theme.text_muted),
    ]
    .align_y(Alignment::Center)
    .padding([0, 14])
    .height(26);
    let strip = container(slanted(strip, 10.0, theme.section_background)).padding(Padding::ZERO.right(16.0));

    let list: Element<'_, Message> = if bar.notifications.is_empty() {
        let bell = Stack::new()
            .push(container(text("notifications").font(ICON_FONT).size(34).color(theme.text_muted).shaping(text::Shaping::Advanced)).center(72))
            .push_under(crate::shape(shape::Kind::Brackets, theme.bracket));
        let empty = column![bell, text("ALL CLEAR").font(CONDENSED_BOLD).size(16).color(theme.text_muted)]
            .spacing(12)
            .align_x(Alignment::Center);
        container(empty).center(Length::Fill).into()
    } else {
        let cards = Column::with_children(bar.notifications.iter().map(|notification| notification_card(bar, notification)))
            .spacing(8)
            .padding(Padding::ZERO.right(8.0));
        scrollable(cards).height(Length::Fill).into()
    };

    let footer = row![
        footer_button(bar, if bar.do_not_disturb { "notifications_off" } else { "notifications" }, Message::ToggleDoNotDisturb),
        container(text(format!("{count} NOTIFICATIONS")).font(CONDENSED_BOLD).size(15).color(theme.text_muted))
            .center_x(Length::Fill)
            .center_y(36)
            .style(move |_| iced::widget::container::Style {
                border: iced::Border { color: theme.border, width: 1.0, radius: 0.0.into() },
                ..Default::default()
            }),
        footer_button(bar, "delete_sweep", Message::ClearNotifications),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([0, 10]);

    container(column![strip, container(list).padding([0, 10]).height(Length::Fill), footer].spacing(10))
        .padding(Padding::ZERO.bottom(10.0).left(0.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| panel_style(theme))
        .into()
}

pub fn notification_card<'a>(bar: &'a Bar, notification: &'a notifications::Notification) -> Element<'a, Message> {
    let theme = bar.theme;
    let header = row![
        text(notification.app_name.to_uppercase()).font(CONDENSED_BOLD).size(13).color(theme.accent),
        Space::new().width(Length::Fill),
        text(notification.time.format("%H:%M").to_string()).font(CONDENSED_BOLD).size(13).color(theme.text_muted),
        button(text("close").font(ICON_FONT).size(16).shaping(text::Shaping::Advanced))
            .on_press(Message::DismissNotification(notification.id))
            .padding(2)
            .style(move |_, status| glow_style(theme, status, 6.0)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut content = column![header, text(&notification.summary).font(BODY_BOLD).size(14)].spacing(4);
    if !notification.body.is_empty() {
        content = content.push(text(&notification.body).size(13).color(theme.text_muted));
    }
    let buttons = Row::with_children(notification.buttons().map(|action| {
        button(container(text(action.label.to_uppercase()).font(CONDENSED_BOLD).size(13)).center_y(26).padding([0, 10]))
            .on_press(Message::InvokeAction(notification.id, action.key.clone()))
            .padding(0)
            .style(move |_, status| {
                let mut style = glow_style(theme, status, 6.0);
                if style.border.width == 0.0 {
                    style.border = iced::Border { color: theme.border, width: 1.0, radius: 0.0.into() };
                }
                style
            })
            .into()
    }))
    .spacing(6);
    if notification.buttons().next().is_some() {
        content = content.push(buttons);
    }
    let content: Element<'a, Message> = match &notification.icon {
        Some(icon) => {
            let icon: Element<'a, Message> = match icon {
                crate::tray::Icon::Raster(handle) => iced::widget::image(handle.clone()).width(32).height(32).into(),
                crate::tray::Icon::Svg(handle) => iced::widget::svg(handle.clone()).width(32).height(32).into(),
            };
            row![icon, content].spacing(10).into()
        }
        None => content.into(),
    };
    let border = if notification.critical { theme.accent } else { theme.border };
    let card = container(content)
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            background: Some(theme.section_background.into()),
            border: iced::Border { color: border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });
    // Clicking the notification triggers its default action, as most daemons do.
    match notification.has_default_action() {
        true => iced::widget::mouse_area(card)
            .on_press(Message::InvokeAction(notification.id, String::from(notifications::DEFAULT_ACTION)))
            .into(),
        false => card.into(),
    }
}

fn footer_button<'a>(bar: &'a Bar, icon: &'a str, message: Message) -> Element<'a, Message> {
    let theme = bar.theme;
    let content = Stack::new()
        .push(container(crate::icon_sized(icon, 18.0)).center_x(40).center_y(36))
        .push_under(crate::shape(shape::Kind::Brackets, theme.bracket));
    button(content).on_press(message).padding(0).style(move |_, status| glow_style(theme, status, 10.0)).into()
}

fn calendar_view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let month = bar.calendar_month;
    let today = bar.now.date_naive();

    let title = row![
        text(month.format("%B").to_string().to_uppercase()).font(CONDENSED_BOLD).size(18),
        text(month.year().to_string()).font(CONDENSED_BOLD).size(18).color(theme.accent),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let title = container(title).padding([0, 14]).center_y(30).width(Length::Fill);
    let header = row![
        slanted(title, 10.0, theme.section_background),
        nav_button(bar, "chevron_left", Message::CalendarMonth(-1)),
        nav_button(bar, "chevron_right", Message::CalendarMonth(1)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let weekdays = Row::with_children(WEEKDAYS.iter().enumerate().map(|(index, name)| {
        let color = if index >= 5 { theme.accent } else { theme.text_muted };
        container(text(*name).font(CONDENSED_BOLD).size(13).color(color)).center_x(Length::Fill).center_y(26).into()
    }));

    let days = calendar::month_grid(month);
    let weeks = Column::with_children(days.chunks(7).map(|week| {
        Row::with_children(week.iter().map(|day| {
            let is_today = day.in_month && month.year() == today.year() && month.month() == today.month() && day.day == today.day();
            day_cell(bar, day, is_today)
        }))
        .into()
    }))
    .spacing(4);

    container(column![header, weekdays, weeks].spacing(10))
        .padding(12)
        .width(Length::Fill)
        .style(move |_| panel_style(theme))
        .into()
}

fn day_cell<'a>(bar: &'a Bar, day: &calendar::Day, is_today: bool) -> Element<'a, Message> {
    let theme = bar.theme;
    let label = day.day.to_string();
    if is_today {
        let number = container(text(label).font(CONDENSED_HEAVY).size(20).color(theme.on_accent))
            .center_x(Length::Fill)
            .center_y(DAY_HEIGHT);
        return container(slanted(number, 8.0, theme.accent)).width(Length::Fill).into();
    }
    let color = if day.in_month { theme.text } else { theme.text_dim };
    container(text(label).font(CONDENSED_BOLD).size(16).color(color))
        .center_x(Length::Fill)
        .center_y(DAY_HEIGHT)
        .into()
}

fn nav_button<'a>(bar: &'a Bar, icon: &'a str, message: Message) -> Element<'a, Message> {
    let theme = bar.theme;
    button(container(crate::icon_sized(icon, 18.0)).center(30))
        .on_press(message)
        .padding(0)
        .style(move |_, status| {
            let mut style = glow_style(theme, status, 8.0);
            if style.border.width == 0.0 {
                style.border = iced::Border { color: theme.border, width: 1.0, radius: 0.0.into() };
            }
            style
        })
        .into()
}

/// First day of the month `delta` months away from `month`.
pub fn shift_month(month: NaiveDate, delta: i32) -> NaiveDate {
    let index = month.year() * 12 + month.month0() as i32 + delta;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1).unwrap_or(month)
}

pub fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_month_crosses_years() {
        let november = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert_eq!(shift_month(november, 2), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(shift_month(november, -11), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
    }
}
