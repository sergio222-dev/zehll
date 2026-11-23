//! System indicator left of the media button, its hover tooltip and the sub-panel it opens.

use std::f32::consts::PI;

use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke, path::Arc};
use iced::widget::{Column, Space, Stack, button, column, container, mouse_area, row, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Point, Radians, Rectangle, Renderer, Theme, mouse};

use crate::usage::{self, Disk, HOT};
use crate::{
    BODY_FONT, Bar, CONDENSED, CONDENSED_BOLD, CONDENSED_HEAVY, MONO, MONO_VALUE, Message, PILL_BACKGROUND, Tooltip, glow_style,
    icon_sized, mono_width, panel_style, slanted, system,
};

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 456;
pub const TOOLTIP_SIZE: (u32, u32) = (460, 122);
const HOT_COLOR: Color = Color::from_rgb8(0xff, 0x4d, 0x5e);
const VALUE: Color = Color::from_rgb8(0xc9, 0xd2, 0xe8);
const RING: f32 = 132.0;
const GRAPH_HEIGHT: f32 = 130.0;
const DISKS: usize = 3;

/// Memory and CPU percentages; hovering shows the tooltip, clicking opens the sub-panel.
pub fn bar_button(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let figure = |glyph, percent: f32| {
        let color = if percent >= HOT { HOT_COLOR } else { theme.text };
        row![
            container(icon_sized(glyph, 16.0)).style(move |_| container::Style {
                text_color: Some(theme.text_muted),
                ..Default::default()
            }),
            // Right-aligned in room for "100%", so a new reading never resizes the button.
            text(format!("{:.0}%", percent)).font(MONO_VALUE).size(14).color(color).width(mono_width(4, 14.0)).align_x(Alignment::End),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    let divider = container(Space::new()).width(1).height(14).style(move |_| container::background(theme.border));
    let content = row![figure("memory", bar.usage.memory.percent()), divider, figure("monitor_heart", bar.usage.cpu)]
        .spacing(12)
        .align_y(Alignment::Center);
    let open = bar.system_popover.is_some();
    let button = button(container(content).padding([0, 12]).center_y(30))
        .padding(0)
        .on_press(Message::ToggleSystem)
        .style(move |_, status| {
            let status = if open { button::Status::Hovered } else { status };
            let mut style = glow_style(theme, status, 10.0);
            if style.background.is_none() {
                style.background = Some(PILL_BACKGROUND.into());
                style.border = Border { color: theme.border, width: 1.0, radius: 0.0.into() };
            }
            style
        });
    mouse_area(button).on_enter(Message::ShowTooltip(Tooltip::System)).on_exit(Message::HideTooltip).into()
}

/// RAM, swap and CPU in three columns.
pub fn tooltip(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let usage = &bar.usage;
    let memory = usage.memory;
    let gib = usage::format_gib;
    let column_of = |glyph: &'static str, title: &'static str, rows: [(&'static str, String); 3]| {
        let header = row![icon_sized(glyph, 15.0), text(title).font(CONDENSED_BOLD).size(14)]
            .spacing(6)
            .align_y(Alignment::Center);
        let header = container(header).style(move |_| container::Style { text_color: Some(theme.accent), ..Default::default() });
        let rows = rows.into_iter().map(|(label, value)| {
            row![
                text(label).size(13).color(theme.text_muted).width(44),
                text(value).font(MONO).size(13).color(theme.text),
            ]
            .align_y(Alignment::Center)
            .into()
        });
        Column::new().push(header).extend(rows).spacing(6).width(Length::Fill)
    };
    let rule = || container(Space::new()).width(1).height(Length::Fill).style(move |_| container::background(theme.border));
    let celsius = |value: Option<f32>| value.map_or_else(|| String::from("—"), |value| format!("{value:.0} °C"));
    let content = row![
        column_of("memory", "RAM", [
            ("Used", gib(memory.used())),
            ("Free", gib(memory.available)),
            ("Total", gib(memory.total)),
        ]),
        rule(),
        column_of("swap_horiz", "SWAP", [
            ("Used", gib(memory.swap_used())),
            ("Free", gib(memory.swap_free)),
            ("Total", gib(memory.swap_total)),
        ]),
        rule(),
        column_of("monitor_heart", "CPU", [
            ("Load", format!("{:.0} %", usage.cpu)),
            ("Temp", celsius(usage.cpu_temperature)),
            ("Clock", usage.cpu_ghz.map_or_else(|| String::from("—"), |ghz| format!("{ghz:.2} GHz"))),
        ]),
    ]
    .spacing(18);
    container(content)
        .padding([12, 16])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| panel_style(theme))
        .into()
}

/// CPU, GPU, memory and swap rings over the disks and the network graph.
pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let usage = &bar.usage;
    let memory = usage.memory;
    let gib = usage::format_gib;
    let celsius = |value: Option<f32>| value.map_or_else(|| String::from("—"), |value| format!("{value:.0} °C"));
    let uptime = format!("UPTIME {}", system::format_uptime(bar.uptime).replace(',', "").to_uppercase());

    let gpu = usage.gpu;
    let processors = block(theme, row![
        gauge(bar, "CPU", usage.cpu / 100.0, [celsius(usage.cpu_temperature), format!("{:.0} %", usage.cpu), usage.cpu_ghz.map_or_else(|| String::from("—"), |ghz| format!("{ghz:.2} GHz"))], true),
        rule(theme),
        gauge(bar, "GPU", gpu.map_or(0.0, |gpu| gpu.percent / 100.0), [
            celsius(gpu.map(|gpu| gpu.temperature)),
            gpu.map_or_else(|| String::from("—"), |gpu| format!("{:.0} %", gpu.percent)),
            gpu.map_or_else(String::new, |gpu| format!("{:.2} / {:.0} GiB", gpu.memory_used as f64 / (1u64 << 30) as f64, gpu.memory_total as f64 / (1u64 << 30) as f64)),
        ], true),
    ]);
    let memories = block(theme, row![
        gauge(bar, "MEMORY", usage::fraction(memory.used(), memory.total), [String::from("USED"), gib(memory.used()), format!("of {}", gib(memory.total))], false),
        rule(theme),
        gauge(bar, "SWAP", usage::fraction(memory.swap_used(), memory.swap_total), [String::from("USED"), gib(memory.swap_used()), format!("of {}", gib(memory.swap_total))], false),
    ]);

    let disks = Column::with_children(usage.disks.iter().take(DISKS).map(|disk| disk_row(bar, disk))).spacing(14).padding([0, 14]);
    let disks = block(theme, column![strip(bar, "DISKS", String::new()), disks].spacing(12).padding(Padding::ZERO.bottom(14.0)));

    let download = usage.download.iter().copied().collect::<Vec<_>>();
    let upload = usage.upload.iter().copied().collect::<Vec<_>>();
    let top = scale(download.iter().chain(&upload).copied().fold(0.0, f32::max));
    let axis = Column::with_children((0..5).map(|step| {
        let label = usage::format_rate(top * (4 - step) as f32 / 4.0).replace("/s", "");
        text(label).font(MONO).size(10).color(theme.bracket).wrapping(text::Wrapping::None).into()
    }))
    // Room for the longest label, "124.0 KB".
    .width(mono_width(8, 10.0))
    .height(GRAPH_HEIGHT)
    .align_x(Alignment::End)
    .spacing((GRAPH_HEIGHT - 5.0 * 14.0) / 4.0);
    let (last_download, last_upload) = (download.last().copied().unwrap_or(0.0), upload.last().copied().unwrap_or(0.0));
    let graph = canvas::Canvas::new(Graph { download, upload, top, theme: bar.theme })
        .width(Length::Fill)
        .height(GRAPH_HEIGHT);
    let legend = |color: Color, label: String| {
        row![container(Space::new()).width(10).height(2).style(move |_| container::background(color)), text(label).font(MONO).size(12).color(theme.text_muted)]
            .spacing(6)
            .align_y(Alignment::Center)
    };
    let legend = row![
        legend(theme.accent, format!("↓ {}", usage::format_rate(last_download))),
        legend(theme.glow, format!("↑ {}", usage::format_rate(last_upload))),
    ]
    .spacing(22);
    let interface = usage.interface.clone().unwrap_or_default().to_uppercase();
    let network = block(theme, column![
        strip(bar, "NETWORK", interface),
        row![axis, graph].spacing(8).padding([0, 14]),
        container(legend).center_x(Length::Fill),
    ]
    .spacing(10)
    .padding(Padding::ZERO.bottom(10.0)));

    let body = column![
        strip(bar, "SYSTEM", uptime),
        row![processors, memories].spacing(12).padding([0, 16]),
        row![disks, network].spacing(12).padding([0, 16]).height(Length::Fill),
    ]
    .spacing(12)
    .padding(Padding::ZERO.bottom(16.0));
    container(body).width(Length::Fill).height(Length::Fill).style(move |_| panel_style(theme)).into()
}

/// Section title strip, like the panel's.
fn strip<'a>(bar: &Bar, title: &'a str, right: String) -> Element<'a, Message> {
    let theme = bar.theme;
    let strip = row![
        text(title).font(CONDENSED).size(14).color(theme.accent),
        Space::new().width(Length::Fill),
        text(right).font(CONDENSED).size(13).color(theme.text_muted),
    ]
    .align_y(Alignment::Center)
    .padding([0, 14])
    .height(26);
    container(slanted(strip, 10.0, theme.section_background)).padding(Padding::ZERO.right(16.0)).into()
}

fn block<'a>(theme: crate::theme::Theme, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .style(move |_| container::Style {
            border: Border { color: theme.bar_border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into()
}

fn rule<'a>(theme: crate::theme::Theme) -> Element<'a, Message> {
    container(Space::new()).width(1).height(Length::Fill).style(move |_| container::background(theme.bar_border)).into()
}

/// A ring with three lines inside; `big` makes the middle one a large percentage.
fn gauge<'a>(bar: &Bar, title: &'a str, fraction: f32, [top, middle, bottom]: [String; 3], big: bool) -> Element<'a, Message> {
    let theme = bar.theme;
    let inside = column![
        // CPU and GPU show a temperature on top; the memory rings show the label USED.
        match big {
            true => text(top).font(MONO),
            false => text(top),
        }
        .size(12)
        .color(theme.text_muted),
        // The big percentage is the ring's headline and keeps the bar's display face.
        match big {
            true => text(middle).font(CONDENSED_HEAVY).size(26),
            false => text(middle).font(MONO_VALUE).size(16),
        }
        .color(theme.text),
        text(bottom).font(MONO).size(12).color(theme.text_muted),
    ]
    .spacing(1)
    .align_x(Alignment::Center);
    let ring = Stack::new()
        .width(RING)
        .height(RING)
        .push(canvas::Canvas::new(Ring { fraction, theme: bar.theme }).width(Length::Fill).height(Length::Fill))
        .push(container(inside).center(Length::Fill));
    column![text(title).font(CONDENSED_BOLD).size(14).color(theme.text_muted), ring]
        .spacing(6)
        .padding([10, 0])
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

fn disk_row<'a>(bar: &Bar, disk: &'a Disk) -> Element<'a, Message> {
    let theme = bar.theme;
    let fraction = usage::fraction(disk.used, disk.total);
    let size = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
    let label = row![
        text(&disk.mount).font(BODY_FONT).size(13).color(VALUE),
        Space::new().width(Length::Fill),
        text(format!("{:.1} / {:.1} GiB", size(disk.used), size(disk.total))).font(MONO).size(13).color(theme.text),
    ];
    let fill = container(slanted(Space::new().width(Length::Fill).height(8), 6.0, theme.accent))
        .width(Length::FillPortion((fraction * 1000.0).round().max(1.0) as u16));
    let rest = Space::new().width(Length::FillPortion((1000.0 - fraction * 1000.0).round().max(1.0) as u16));
    let track = container(row![fill, rest]).height(8).style(move |_| container::background(theme.section_background));
    column![label, track].spacing(6).into()
}

/// Rounds the graph's top up to 1, 2 or 5 times a power of ten KB/s, at least 1 KB/s.
fn scale(peak: f32) -> f32 {
    let kib = (peak / 1024.0).max(1.0);
    let magnitude = 10f32.powf(kib.log10().floor());
    let top = [1.0, 2.0, 5.0, 10.0].into_iter().map(|step| step * magnitude).find(|top| *top >= kib).unwrap_or(kib);
    top * 1024.0
}

struct Ring {
    fraction: f32,
    theme: crate::theme::Theme,
}

impl<Message> canvas::Program<Message> for Ring {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = bounds.width.min(bounds.height) / 2.0 - 5.0;
        // A 270° arc open at the bottom, clockwise from the lower left.
        let (start, sweep) = (0.75 * PI, 1.5 * PI);
        let arc = |to: f32| {
            Path::new(|builder| {
                builder.arc(Arc { center, radius, start_angle: Radians(start), end_angle: Radians(start + sweep * to) })
            })
        };
        frame.stroke(&arc(1.0), Stroke::default().with_color(self.theme.section_background).with_width(7.0));
        let fraction = self.fraction.clamp(0.0, 1.0);
        if fraction > 0.005 {
            let color = if fraction * 100.0 >= HOT { HOT_COLOR } else { self.theme.accent };
            frame.stroke(&arc(fraction), Stroke::default().with_color(Color { a: 0.35, ..color }).with_width(11.0).with_line_cap(LineCap::Butt));
            frame.stroke(&arc(fraction), Stroke::default().with_color(color).with_width(7.0).with_line_cap(LineCap::Butt));
        }
        let tick = Color::from_rgb8(0x2a, 0x3a, 0x5f);
        for step in 0..=10 {
            let angle = start + sweep * step as f32 / 10.0;
            let at = |distance: f32| Point::new(center.x + distance * angle.cos(), center.y + distance * angle.sin());
            frame.stroke(&Path::line(at(radius * 0.81), at(radius * 0.87)), Stroke::default().with_color(tick).with_width(1.5));
        }
        vec![frame.into_geometry()]
    }
}

struct Graph {
    download: Vec<f32>,
    upload: Vec<f32>,
    top: f32,
    theme: crate::theme::Theme,
}

impl<Message> canvas::Program<Message> for Graph {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (width, height) = (bounds.width, bounds.height);
        for step in 0..5 {
            let y = (height - 1.0) * step as f32 / 4.0 + 0.5;
            frame.stroke(&Path::line(Point::new(0.0, y), Point::new(width, y)), Stroke::default().with_color(self.theme.bar_border).with_width(1.0));
        }
        // The newest sample sits on the right edge.
        let point = |values: &[f32], index: usize| {
            let x = width - (values.len() - 1 - index) as f32 * width / (usage::HISTORY - 1) as f32;
            Point::new(x, height - (values[index] / self.top).clamp(0.0, 1.0) * height)
        };
        let line = |values: &[f32]| {
            Path::new(|builder| {
                for index in 0..values.len() {
                    match index {
                        0 => builder.move_to(point(values, index)),
                        _ => builder.line_to(point(values, index)),
                    }
                }
            })
        };
        if self.download.len() > 1 {
            let area = Path::new(|builder| {
                builder.move_to(Point::new(point(&self.download, 0).x, height));
                for index in 0..self.download.len() {
                    builder.line_to(point(&self.download, index));
                }
                builder.line_to(Point::new(width, height));
                builder.close();
            });
            frame.fill(&area, Color { a: 0.18, ..self.theme.accent });
            frame.stroke(&line(&self.download), Stroke::default().with_color(self.theme.accent).with_width(1.6));
        }
        if self.upload.len() > 1 {
            frame.stroke(&line(&self.upload), Stroke::default().with_color(Color { a: 0.9, ..self.theme.glow }).with_width(1.4));
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graph_top_rounds_up_to_a_readable_step() {
        assert_eq!(scale(0.0), 1024.0);
        assert_eq!(scale(1024.0), 1024.0);
        assert_eq!(scale(1500.0), 2048.0);
        assert_eq!(scale(47_800.0), 51_200.0);
        assert_eq!(scale(120_000.0), 204_800.0);
    }
}
