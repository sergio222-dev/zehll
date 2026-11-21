use chrono::{Datelike, NaiveDate};
use iced::widget::{Column, Row, Space, Stack, button, column, container, row, text};
use iced::{Alignment, Element, Length, Padding};

use crate::{
    Bar, CONDENSED, CONDENSED_BOLD, CONDENSED_HEAVY, Message, calendar, glow_style, line, panel_style, shape, slanted,
    system,
};

pub const WIDTH: u32 = 460;
const PADDING: u16 = 16;
const DAY_HEIGHT: f32 = 34.0;
const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let content = column![header(bar), Space::new().height(Length::Fill), calendar_view(bar)]
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
