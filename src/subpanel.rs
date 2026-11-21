use std::fmt;

use iced::widget::{Column, Row, Space, Stack, button, column, container, mouse_area, pick_list, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, mouse};

use crate::{BODY_BOLD, Bar, CONDENSED_BOLD, CONDENSED_HEAVY, ICON_FONT, Message, bluetooth, glow_style, line, mixer, shape, slanted, tray};

const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 620.0;
const VOLUME_CELLS: u32 = 20;

/// Floating dialogs opened by right-clicking a quick settings tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subpanel {
    Audio,
    Bluetooth,
}

impl Subpanel {
    /// Program "Details" opens, the same ones ii uses.
    pub fn details(self) -> &'static [&'static str] {
        match self {
            Subpanel::Audio => &["pavucontrol"],
            Subpanel::Bluetooth => &["kcmshell6", "kcm_bluetooth"],
        }
    }
}

pub fn view(bar: &Bar, subpanel: Subpanel) -> Element<'_, Message> {
    match subpanel {
        Subpanel::Audio => audio(bar),
        Subpanel::Bluetooth => bluetooth_devices(bar),
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

fn bluetooth_devices(bar: &Bar) -> Element<'_, Message> {
    let Some(bluetooth) = &bar.bluetooth else {
        return frame(bar, "BLUETOOTH DEVICES", String::new(), empty("NO ADAPTER", bar), Subpanel::Bluetooth);
    };
    let linked = bluetooth.devices.iter().filter(|device| device.connected).count();
    let body = if bluetooth.devices.is_empty() {
        empty("NO PAIRED DEVICES", bar)
    } else {
        let cards = Column::with_children(bluetooth.devices.iter().map(|device| device_card(bar, &bluetooth.adapter, device))).spacing(10);
        scrollable(cards).height(Length::Fill).into()
    };
    frame(bar, "BLUETOOTH DEVICES", format!("{linked} / {} LINKED", bluetooth.devices.len()), body, Subpanel::Bluetooth)
}

fn device_card<'a>(bar: &'a Bar, adapter: &zbus::zvariant::OwnedObjectPath, device: &'a bluetooth::Device) -> Element<'a, Message> {
    use bluetooth::DeviceAction;
    let theme = bar.theme;
    let (block, icon_color) = if device.connected { (theme.accent, theme.on_accent) } else { (theme.section_background, theme.text) };
    let icon = container(text(device_icon(&device.kind)).font(ICON_FONT).size(20).color(icon_color).shaping(text::Shaping::Advanced))
        .padding(Padding::ZERO.right(8.0))
        .center(44);
    let status = match (device.connected, device.paired) {
        (true, _) => ("CONNECTED", theme.accent),
        (false, true) => ("PAIRED", theme.text_muted),
        (false, false) => ("NOT PAIRED", theme.text_dim),
    };
    let mut header = row![
        slanted(icon, 10.0, block),
        column![text(&device.alias).font(BODY_BOLD).size(15), text(status.0).font(CONDENSED_BOLD).size(13).color(status.1)].spacing(4).width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if let Some(battery) = device.battery {
        // Battery as a segmented gauge, like the HUD's HP bar.
        let cells = Row::with_children((1..=10u8).map(|cell| {
            let color = if cell * 10 <= battery { theme.accent } else { theme.section_background };
            container(Space::new()).width(5).height(8).style(move |_| container::background(color)).into()
        }))
        .spacing(2);
        header = header.push(column![text(format!("{battery}%")).font(CONDENSED_HEAVY).size(20).color(theme.accent), cells].spacing(4).align_x(Alignment::End));
    }

    let path = device.path.clone();
    let actions: Element<'a, Message> = if device.connected {
        row![
            outline_button(bar, "FORGET", Message::BluetoothDevice(DeviceAction::Forget { adapter: adapter.clone(), device: path.clone() })),
            accent_button(bar, "DISCONNECT", Message::BluetoothDevice(DeviceAction::Disconnect(path))),
        ]
        .spacing(8)
        .into()
    } else {
        let check = if device.trusted { "check_box" } else { "check_box_outline_blank" };
        let always = row![text(check).font(ICON_FONT).size(16).shaping(text::Shaping::Advanced), text("ALWAYS CONNECT").font(CONDENSED_BOLD).size(14)]
            .spacing(8)
            .align_y(Alignment::Center);
        let always = button(container(always).padding([0, 12]).center_y(32))
            .on_press(Message::BluetoothDevice(DeviceAction::Trust(path.clone(), !device.trusted)))
            .padding(0)
            .style(move |_, status| glow_style(theme, status, 6.0));
        let connect = match device.paired {
            true => accent_button(bar, "CONNECT", Message::BluetoothDevice(DeviceAction::Connect(path))),
            false => accent_button(bar, "PAIR", Message::BluetoothDevice(DeviceAction::Pair(path))),
        };
        row![always, connect].spacing(8).align_y(Alignment::Center).into()
    };

    let connected = device.connected;
    container(column![header, container(actions).align_right(Length::Fill)].spacing(12))
        .padding(12)
        .width(Length::Fill)
        .style(move |_| match connected {
            true => container::Style {
                background: Some(Color { a: 0.08, ..theme.glow }.into()),
                border: Border { color: theme.glow, width: 1.0, radius: 0.0.into() },
                shadow: iced::Shadow { color: Color { a: 0.3, ..theme.glow }, blur_radius: 12.0, ..Default::default() },
                ..Default::default()
            },
            false => container::Style { border: Border { color: theme.border, width: 1.0, radius: 0.0.into() }, ..Default::default() },
        })
        .into()
}

/// Material Symbols name for BlueZ's device icon.
fn device_icon(kind: &str) -> &'static str {
    match kind {
        "audio-headset" | "audio-headphones" => "headphones",
        "audio-card" => "speaker",
        "input-mouse" => "mouse",
        "input-keyboard" => "keyboard",
        "input-gaming" => "sports_esports",
        "phone" => "smartphone",
        "computer" => "computer",
        _ => "bluetooth",
    }
}

fn empty<'a>(label: &'a str, bar: &'a Bar) -> Element<'a, Message> {
    container(text(label).font(CONDENSED_BOLD).size(16).color(bar.theme.text_muted)).center(Length::Fill).into()
}

fn outline_button<'a>(bar: &'a Bar, label: &'a str, message: Message) -> Element<'a, Message> {
    let theme = bar.theme;
    button(container(text(label).font(CONDENSED_BOLD).size(14)).padding([0, 14]).center_y(32))
        .on_press(message)
        .padding(0)
        .style(move |_, status| {
            let mut style = glow_style(theme, status, 6.0);
            if style.border.width == 0.0 {
                style.border = Border { color: theme.border, width: 1.0, radius: 0.0.into() };
            }
            style
        })
        .into()
}

fn accent_button<'a>(bar: &'a Bar, label: &'a str, message: Message) -> Element<'a, Message> {
    let theme = bar.theme;
    let label = container(text(label).font(CONDENSED_BOLD).size(14).color(theme.on_accent))
        .padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 14.0 })
        .center_y(32);
    button(slanted(label, 10.0, theme.accent)).on_press(message).padding(0).style(|_, _| button::Style::default()).into()
}
