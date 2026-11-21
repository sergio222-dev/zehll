//! Calendar popover opened from the date in the bar: the month with week numbers and
//! meeting dots, and the agenda of the selected day.

use chrono::Datelike;
use iced::widget::{Column, Row, Space, Stack, button, column, container, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::meetings::{self, AgendaState, Meeting};
use crate::{
    BODY_BOLD, BODY_FONT, Bar, CONDENSED, CONDENSED_BOLD, CONDENSED_HEAVY, Message, calendar, icon_sized, line,
    panel, panel_style, shape, slanted,
};

pub const WIDTH: u32 = 840;
pub const HEIGHT: u32 = 390;
const MONTH_WIDTH: f32 = 400.0;
const WEEK_WIDTH: f32 = 32.0;
const CELL_HEIGHT: f32 = 44.0;
const WEEKDAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
const NEXT_BADGE: Color = Color::from_rgb8(0xff, 0xd2, 0x7a);

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let divider = container(Space::new()).width(1).height(Length::Fill).style(move |_| container::background(theme.border));
    let body = container(row![month_view(bar), divider, agenda_view(bar)])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| panel_style(theme));
    Stack::new()
        .push(body)
        .push(shape(shape::Kind::Brackets { arm: 18.0, width: 2.0 }, theme.accent))
        .into()
}

fn month_view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let month = bar.calendar_month;
    let title = container(text(month.format("%B %Y").to_string().to_uppercase()).font(CONDENSED_HEAVY).size(20))
        .padding(Padding { top: 0.0, right: 32.0, bottom: 0.0, left: 16.0 })
        .center_y(40);
    let header = row![
        slanted(title, 14.0, theme.section_background),
        Space::new().width(Length::Fill),
        panel::nav_button(bar, "chevron_left", Message::CalendarMonth(-1)),
        panel::nav_button(bar, "chevron_right", Message::CalendarMonth(1)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let weekdays = Row::with_children(WEEKDAYS.iter().enumerate().map(|(index, name)| {
        let color = if index >= 5 { theme.bracket } else { theme.text_muted };
        container(text(*name).font(CONDENSED_BOLD).size(13).color(color)).center_x(Length::Fill).center_y(28).into()
    }));
    let weekdays = row![Space::new().width(WEEK_WIDTH), weekdays];

    let days = calendar::month_grid(month);
    let weeks = Column::with_children(days.chunks(7).zip(calendar::week_numbers(month)).map(|(week, number)| {
        let number = container(text(number.to_string()).font(CONDENSED).size(13).color(theme.bracket))
            .center_x(WEEK_WIDTH - 1.0)
            .center_y(CELL_HEIGHT);
        let rule = container(Space::new()).width(1).height(CELL_HEIGHT).style(move |_| container::background(theme.border));
        row![number, rule, Row::with_children(week.iter().map(|day| day_cell(bar, day)))].into()
    }))
    .spacing(2);

    container(column![header, weekdays, weeks].spacing(10))
        .width(MONTH_WIDTH)
        .padding(Padding { top: 16.0, right: 16.0, bottom: 18.0, left: 16.0 })
        .into()
}

fn day_cell<'a>(bar: &'a Bar, day: &calendar::Day) -> Element<'a, Message> {
    let theme = bar.theme;
    let selected = day.date == bar.calendar_day;
    let today = day.date == bar.now.date_naive();
    let color = if selected {
        theme.on_accent
    } else if !day.in_month {
        theme.text_dim
    } else if today {
        theme.accent
    } else if day.date.weekday().number_from_monday() > 5 {
        theme.text_muted
    } else {
        theme.text
    };
    let number = container(text(day.day.to_string()).font(CONDENSED_BOLD).size(17).color(color)).center_x(32).center_y(26);
    let number = match selected {
        true => slanted(number, 7.0, theme.accent),
        false => number.into(),
    };
    let dot_color = match meetings::has_meetings(&bar.meetings, day.date) {
        true if day.in_month => theme.accent,
        true => theme.bracket,
        false => Color::TRANSPARENT,
    };
    let dot = container(Space::new()).width(4).height(4).style(move |_| container::background(dot_color));
    let cell = column![container(number).width(32).height(26), dot].spacing(3).align_x(Alignment::Center);
    button(container(cell).center_x(Length::Fill).center_y(CELL_HEIGHT))
        .on_press(Message::SelectDay(day.date))
        .padding(0)
        .style(|_, _| button::Style::default())
        .into()
}

fn agenda_view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let day = bar.calendar_day;
    let meetings = meetings::meetings_on(&bar.meetings, day);
    let header = row![
        column![
            text(day.format("%A").to_string()).font(CONDENSED_BOLD).size(26),
            text(day.format("%B %-d, %Y").to_string()).font(BODY_FONT).size(13).color(theme.text_muted),
        ]
        .spacing(2),
        Space::new().width(Length::Fill),
        text(meetings::meeting_count(meetings.len())).font(CONDENSED_BOLD).size(14).color(theme.text_muted),
    ]
    .align_y(Alignment::End);
    let header = column![container(header).padding(Padding::ZERO.bottom(10.0)), line(theme.accent).height(2)];

    let list = Column::with_children(meetings.into_iter().map(|meeting| meeting_row(bar, meeting))).spacing(8);
    container(column![header, scrollable(list).height(Length::Fill)].spacing(12))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn meeting_row<'a>(bar: &'a Bar, meeting: &'a Meeting) -> Element<'a, Message> {
    let theme = bar.theme;
    let now = bar.now.naive_local();
    let state = meetings::agenda_state(meeting, &bar.meetings, now);
    let highlighted = state == AgendaState::Next;
    // Past meetings are dimmed; iced text has no opacity, so their colours change instead.
    let done = state == AgendaState::Done;
    let (strong, muted) = match done {
        true => (theme.text_dim, theme.text_dim),
        false => (theme.text, theme.text_muted),
    };

    let times = column![
        text(meeting.start.format("%H:%M").to_string())
            .font(CONDENSED_HEAVY)
            .size(22)
            .color(if highlighted { theme.accent } else { strong }),
        text(meeting.end.format("%H:%M").to_string()).font(CONDENSED_BOLD).size(13).color(theme.bracket),
    ]
    .spacing(2)
    .width(58);
    let source = match meetings::has_meet_link(meeting) {
        true => format!("{} · Google Meet", meeting.calendar),
        false => meeting.calendar.clone(),
    };
    let details = column![
        text(&meeting.title).font(BODY_BOLD).size(15).color(strong).wrapping(text::Wrapping::None),
        text(source).font(BODY_FONT).size(12).color(muted).wrapping(text::Wrapping::None),
    ]
    .spacing(3)
    .width(Length::Fill)
    .clip(true);

    let badge = text(meetings::agenda_label(meeting, &bar.meetings, now))
        .font(CONDENSED_BOLD)
        .size(13)
        .color(if highlighted { NEXT_BADGE } else { muted });
    let status: Element<'_, Message> = match state {
        AgendaState::Next | AgendaState::Now => {
            column![badge, join_button(bar, meeting)].spacing(6).align_x(Alignment::End).into()
        }
        AgendaState::Done | AgendaState::Later => badge.into(),
    };

    container(row![times, details, status].spacing(12).align_y(Alignment::Center))
        .padding([10, 12])
        .width(Length::Fill)
        .style(move |_| match highlighted {
            true => container::Style {
                background: Some(Color { a: 0.08, ..theme.glow }.into()),
                border: Border { color: theme.glow, width: 1.0, radius: 0.0.into() },
                shadow: Shadow { color: Color { a: 0.3, ..theme.glow }, blur_radius: 12.0, ..Default::default() },
                ..Default::default()
            },
            false => container::Style {
                border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
                ..Default::default()
            },
        })
        .into()
}

/// `JOIN` for a Google Meet link, `OPEN` for the event page.
pub fn join_button<'a>(bar: &'a Bar, meeting: &'a Meeting) -> Element<'a, Message> {
    let theme = bar.theme;
    let label = if meetings::has_meet_link(meeting) { "JOIN" } else { "OPEN" };
    let content = row![
        container(icon_sized("videocam", 14.0)).style(move |_| container::Style {
            text_color: Some(theme.on_accent),
            ..Default::default()
        }),
        text(label).font(CONDENSED_BOLD).size(14).color(theme.on_accent),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let content = container(content).padding(Padding { top: 0.0, right: 22.0, bottom: 0.0, left: 12.0 }).center_y(30);
    button(slanted(content, 10.0, theme.accent))
        .on_press(Message::OpenMeetingLink(meeting.link.clone()))
        .padding(0)
        .style(|_, _| button::Style::default())
        .into()
}
