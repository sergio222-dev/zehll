use std::fmt;

use iced::widget::{Column, Row, Space, Stack, button, column, container, mouse_area, pick_list, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, mouse};

use crate::{BODY_BOLD, Bar, CONDENSED_BOLD, CONDENSED_HEAVY, Message, glow_style, line, mixer, shape, slanted, tray};

const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 620.0;
const VOLUME_CELLS: u32 = 20;

/// Floating dialogs opened by right-clicking a quick settings tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subpanel {
    Audio,
}

impl Subpanel {
    /// Program "Details" opens, the same ones ii uses.
    pub fn details(self) -> &'static [&'static str] {
        match self {
            Subpanel::Audio => &["pavucontrol"],
        }
    }
}

pub fn view(bar: &Bar, subpanel: Subpanel) -> Element<'_, Message> {
    match subpanel {
        Subpanel::Audio => audio(bar),
    }
}

/// Shared dialog chrome: title strip, body, DETAILS and DONE.
fn frame<'a>(bar: &'a Bar, title: &'a str, count: String, body: Element<'a, Message>, subpanel: Subpanel) -> Element<'a, Message> {
    let theme = bar.theme;
    let strip = row![
        text(title).font(CONDENSED_HEAVY).size(20),
        Space::new().width(Length::Fill),
        text(count).font(CONDENSED_BOLD).size(14).color(theme.text_muted),
    ]
    .align_y(Alignment::Center)
    .padding([0, 16])
    .height(40);
    let strip = container(slanted(strip, 14.0, theme.section_background)).padding(Padding { top: 16.0, right: 40.0, bottom: 0.0, left: 0.0 });

    let details = Stack::new()
        .push(container(text("DETAILS").font(CONDENSED_BOLD).size(15)).padding([0, 16]).center_y(36))
        .push_under(crate::shape(shape::Kind::BRACKETS, theme.bracket));
    let details = button(details).on_press(Message::SubpanelDetails(subpanel)).padding(0).style(move |_, status| glow_style(theme, status, 8.0));
    let done = container(text("DONE").font(CONDENSED_HEAVY).size(16).color(theme.on_accent))
        .padding(Padding { top: 0.0, right: 28.0, bottom: 0.0, left: 18.0 })
        .center_y(36);
    let done = button(slanted(done, 12.0, theme.accent)).on_press(Message::CloseSubpanel).padding(0).style(|_, _| button::Style::default());
    let footer = row![details, Space::new().width(Length::Fill), done].align_y(Alignment::Center).padding([14, 16]);

    let dialog = container(column![strip, container(body).padding([14, 16]).height(Length::Fill), line(theme.border), footer])
        .width(WIDTH)
        .height(HEIGHT)
        .style(move |_| container::Style {
            // Opaque, so the panel behind does not show through.
            background: Some(Color { a: 1.0, ..theme.popup_background }.into()),
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            shadow: iced::Shadow { color: Color { a: 0.55, ..Color::BLACK }, offset: iced::Vector::new(0.0, 18.0), blur_radius: 48.0 },
            ..Default::default()
        });
    Stack::new()
        .push(dialog)
        .push(crate::shape(shape::Kind::Brackets { arm: 18.0, width: 2.0 }, theme.accent))
        .into()
}

/// An output device in the picker; shown by its description.
#[derive(Debug, Clone, PartialEq)]
struct SinkOption(mixer::Sink);

impl fmt::Display for SinkOption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.description.to_uppercase())
    }
}

fn audio(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let mixer = bar.mixer.as_ref();
    let streams = mixer.map(|mixer| mixer.streams.as_slice()).unwrap_or_default();

    let list = Column::with_children(streams.iter().map(|stream| stream_row(bar, stream))).spacing(14);
    let sinks: Vec<SinkOption> = mixer.map(|mixer| mixer.sinks.iter().cloned().map(SinkOption).collect()).unwrap_or_default();
    let selected = mixer.and_then(|mixer| sinks.iter().find(|sink| sink.0.name == mixer.default_sink).cloned());
    let device = pick_list(sinks, selected, |sink: SinkOption| Message::SetDefaultSink(sink.0.name))
        .font(CONDENSED_BOLD)
        .text_size(16)
        .padding([10, 14])
        .width(Length::Fill)
        .style(move |_, status| pick_list::Style {
            text_color: theme.text,
            placeholder_color: theme.text_muted,
            handle_color: theme.text,
            background: theme.section_background.into(),
            border: Border {
                color: if matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. }) { theme.glow } else { theme.border },
                width: 1.0,
                radius: 0.0.into(),
            },
        })
        .menu_style(move |_| iced::overlay::menu::Style {
            background: theme.popup_background.into(),
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            text_color: theme.text,
            selected_text_color: theme.on_accent,
            selected_background: theme.accent.into(),
            shadow: iced::Shadow::default(),
        });
    let device = column![text("OUTPUT DEVICE").font(CONDENSED_BOLD).size(13).color(theme.text_muted), device].spacing(6);

    let body = column![scrollable(list).height(Length::Fill), device].spacing(14).into();
    frame(bar, "AUDIO OUTPUT", format!("{} STREAMS", streams.len()), body, Subpanel::Audio)
}

fn stream_row<'a>(bar: &'a Bar, stream: &'a mixer::Stream) -> Element<'a, Message> {
    let theme = bar.theme;
    let icon: Element<'a, Message> = match stream.binary.as_ref().and_then(|binary| bar.app_icons.get(binary)).and_then(Option::as_ref) {
        Some(tray::Icon::Raster(handle)) => iced::widget::image(handle.clone()).width(32).height(32).into(),
        Some(tray::Icon::Svg(handle)) => iced::widget::svg(handle.clone()).width(32).height(32).into(),
        None => {
            let initial = stream.app.chars().next().map(String::from).unwrap_or_default();
            container(text(initial).font(CONDENSED_BOLD).size(18).color(theme.text_muted))
                .center(36)
                .style(move |_| container::Style {
                    background: Some(theme.section_background.into()),
                    border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
                    ..Default::default()
                })
                .into()
        }
    };
    let label = row![
        text(&stream.app).font(BODY_BOLD).size(14),
        text(&stream.title).size(13).color(theme.text_muted).wrapping(text::Wrapping::None).width(Length::Fill),
        text(format!("{}%", stream.volume)).font(CONDENSED_BOLD).size(15).color(theme.accent),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    // A segmented bar like the HUD's HP gauges: each cell is 5%.
    let (index, volume) = (stream.index, stream.volume);
    let filled = volume.div_ceil(100 / VOLUME_CELLS);
    let cells = Row::with_children((1..=VOLUME_CELLS).map(|cell| {
        let color = if cell <= filled { theme.accent } else { theme.section_background };
        mouse_area(container(Space::new()).width(Length::Fill).height(8).style(move |_| container::background(color)))
            .on_press(Message::SetStreamVolume(index, cell * 100 / VOLUME_CELLS))
            .into()
    }))
    .spacing(2);
    let cells = mouse_area(cells).on_scroll(move |delta| {
        let up = match delta {
            mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y > 0.0,
        };
        let volume = if up { (volume + 5).min(100) } else { volume.saturating_sub(5) };
        Message::SetStreamVolume(index, volume)
    });

    row![icon, column![label, cells].spacing(6).width(Length::Fill)].spacing(12).align_y(Alignment::Center).into()
}
