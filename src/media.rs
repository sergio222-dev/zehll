//! Media button beside the clock and the player popover it opens.

use std::f32::consts::TAU;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{Space, Stack, button, column, container, image, mouse_area, row, slider, text};
use iced::{Alignment, Border, Color, ContentFit, Element, Length, Padding, Point, Rectangle, Renderer, Shadow, Theme, mouse};

use crate::cava;
use crate::flame::Flare;
use crate::mpris::{self, Command, Loop, Player};
use crate::{BODY_FONT, Bar, CONDENSED, CONDENSED_BOLD, Message, Tooltip, glow_style, icon_sized, panel_style, shape, slanted};

pub const WIDTH: u32 = 580;
pub const HEIGHT: u32 = 212;
pub const TOOLTIP_SIZE: (u32, u32) = (300, 56);
const BUTTON: f32 = 30.0;
// Room for the flame burst around the button: the full bar height.
const BUTTON_SLOT: f32 = 44.0;
const PADDING: f32 = 18.0;
const ART: f32 = 176.0;
const SOURCE: Color = Color::from_rgb8(0x5f, 0xe3, 0x9a);
const SPECTRUM_HEIGHT: f32 = 140.0;
// ii's WaveVisualizer fill; without its blur the area reads stronger, so it stays faint.
const SPECTRUM_ALPHA: f32 = 0.18;
// Neighbours averaged on each side of a cava bar, as in ii.
const SPECTRUM_SMOOTHING: usize = 2;
const WAVE_HEIGHT: f32 = 14.0;
const WAVE_AMPLITUDE: f32 = 4.0;
const WAVE_LENGTH: f32 = 24.0;
// Wavelengths the live wave travels per second.
const WAVE_SPEED: f32 = 1.0;

/// Pause while playing, play otherwise; hidden without a player.
pub fn bar_button(bar: &Bar) -> Option<Element<'_, Message>> {
    let theme = bar.theme;
    let player = bar.media.as_ref()?;
    let playing = player.playing;
    let glyph = if playing { "pause" } else { "play_arrow" };
    let button = button(container(icon_sized(glyph, 18.0)).center(BUTTON))
        .on_press(Message::ToggleMedia)
        .padding(0)
        .style(move |_, status| {
            let mut style = glow_style(theme, status, 10.0);
            if playing && style.background.is_none() {
                style.background = Some(Color { a: 0.12, ..theme.glow }.into());
                style.border = Border { color: theme.glow, width: 1.0, radius: 0.0.into() };
                style.shadow = Shadow { color: Color { a: 0.45, ..theme.glow }, blur_radius: 10.0, ..Default::default() };
            } else if style.border.width == 0.0 {
                style.border = Border { color: theme.border, width: 1.0, radius: 0.0.into() };
            }
            style
        });
    let button = mouse_area(button).on_enter(Message::ShowTooltip(Tooltip::Media)).on_exit(Message::HideTooltip);
    // The slot keeps its size with or without flames, so the bar does not shift during a burst.
    let flames: Element<'_, Message> = match bar.media_flare {
        Some((started, playing)) => {
            let flare = Flare { elapsed: started.elapsed(), playing, half_extent: BUTTON / 2.0 };
            iced::widget::Shader::new(flare).width(Length::Fill).height(Length::Fill).into()
        }
        None => Space::new().width(Length::Fill).height(Length::Fill).into(),
    };
    let slot = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(flames)
        .push(container(button).center(Length::Fill));
    Some(container(slot).width(BUTTON_SLOT).height(BUTTON_SLOT).into())
}

/// Title, then `ARTIST · PLAYER`.
pub fn tooltip(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let content: Element<'_, Message> = match &bar.media {
        Some(player) => {
            let source = match player.artist.is_empty() {
                true => player.identity.to_uppercase(),
                false => format!("{} · {}", player.artist.to_uppercase(), player.identity.to_uppercase()),
            };
            column![
                text(title(player)).font(crate::BODY_BOLD).size(13).wrapping(text::Wrapping::None),
                text(source).font(CONDENSED).size(13).color(theme.accent).wrapping(text::Wrapping::None),
            ]
            .spacing(2)
            .into()
        }
        None => Space::new().into(),
    };
    container(content)
        .padding([8, 12])
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| panel_style(theme))
        .into()
}

pub fn view(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    let body: Element<'_, Message> = match &bar.media {
        Some(player) => row![cover(bar), details(bar, player)].spacing(20).into(),
        None => Space::new().into(),
    };
    let background = container(Space::new()).width(Length::Fill).height(Length::Fill).style(move |_| panel_style(theme));
    // The spectrum only moves while playing; cava runs only then (see `subscription`).
    let spectrum: Element<'_, Message> = match bar.media.as_ref().is_some_and(|player| player.playing) {
        true => {
            let spectrum = canvas::Canvas::new(Spectrum { points: &bar.media_spectrum, color: theme.accent }).width(Length::Fill).height(SPECTRUM_HEIGHT);
            // Inset by the border so the area stays inside it.
            container(spectrum).padding(1).width(Length::Fill).align_bottom(Length::Fill).into()
        }
        false => Space::new().into(),
    };
    Stack::new()
        .push(background)
        .push(spectrum)
        .push(container(body).padding(PADDING).width(Length::Fill).height(Length::Fill))
        .push(shape(crate::shape::Kind::Brackets { arm: 18.0, width: 2.0 }, theme.accent))
        .into()
}

fn title(player: &Player) -> &str {
    if player.title.is_empty() { "Unknown title" } else { &player.title }
}

fn cover(bar: &Bar) -> Element<'_, Message> {
    let theme = bar.theme;
    match &bar.media_art {
        Some(handle) => image(handle.clone()).width(ART).height(ART).content_fit(ContentFit::Cover).into(),
        None => container(container(icon_sized("music_note", 44.0)).style(move |_| container::Style {
            text_color: Some(theme.bracket),
            ..Default::default()
        }))
        .center(ART)
        .style(move |_| container::Style {
            background: Some(theme.section_background.into()),
            border: Border { color: theme.border, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into(),
    }
}

fn details<'a>(bar: &'a Bar, player: &'a Player) -> Element<'a, Message> {
    let theme = bar.theme;
    let names = column![
        text(title(player)).font(CONDENSED_BOLD).size(28).wrapping(text::Wrapping::None),
        text(&player.artist).font(BODY_FONT).size(14).color(theme.text_muted).wrapping(text::Wrapping::None),
    ]
    .spacing(4)
    .width(Length::Fill)
    .clip(true);
    let source = row![
        container(shape(crate::shape::Kind::Diamond, SOURCE)).width(10).height(10),
        text(player.identity.to_uppercase()).font(CONDENSED_BOLD).size(12).color(SOURCE),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let source = slanted(
        container(source).padding(Padding { top: 0.0, right: 14.0, bottom: 0.0, left: 8.0 }).center_y(22),
        8.0,
        theme.section_background,
    );
    let header = row![names, source].spacing(12);

    column![header, Space::new().height(Length::Fill), progress(bar, player), controls(bar, player)]
        .spacing(10)
        .height(Length::Fill)
        .into()
}

/// Seek slider with the elapsed and total times, or the live wave for streams without a length.
fn progress<'a>(bar: &'a Bar, player: &'a Player) -> Element<'a, Message> {
    let theme = bar.theme;
    let Some(length) = player.length else {
        return live(bar, player);
    };
    let position = bar.media_seek.unwrap_or_else(|| player.position(Instant::now()).as_secs_f32());
    let total = length.as_secs_f32().max(1.0);
    let seek = slider(0.0..=total, position.min(total), Message::DragMediaSeek)
        .on_release(Message::ReleaseMediaSeek)
        .height(14)
        .style(move |_, status| slider::Style {
            rail: slider::Rail {
                backgrounds: (theme.accent.into(), theme.bar_border.into()),
                width: 4.0,
                border: Border::default(),
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Rectangle { width: 6, border_radius: 0.0.into() },
                background: match status {
                    slider::Status::Active => theme.accent.into(),
                    slider::Status::Hovered | slider::Status::Dragged => theme.glow.into(),
                },
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        });
    let times = row![
        text(mpris::format_time(std::time::Duration::from_secs_f32(position))).font(CONDENSED).size(13).color(theme.text_muted),
        Space::new().width(Length::Fill),
        text(mpris::format_time(length)).font(CONDENSED).size(13).color(theme.text_muted),
    ];
    column![seek, times].spacing(6).into()
}

/// A wave that travels while playing and lies flat when paused, and `LIVE`. Streams cannot be
/// sought and their elapsed time is not shown: players such as Firefox report it as 0.
fn live<'a>(bar: &'a Bar, player: &'a Player) -> Element<'a, Message> {
    let theme = bar.theme;
    let wave = Wave {
        phase: wave_phase(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()),
        amplitude: if player.playing { WAVE_AMPLITUDE } else { 0.0 },
        color: theme.accent,
    };
    let end = container(Space::new()).width(4).height(WAVE_HEIGHT).style(move |_| container::background(theme.accent));
    let wave = row![canvas::Canvas::new(wave).width(Length::Fill).height(WAVE_HEIGHT), end].align_y(Alignment::Center);
    let badge = row![
        container(shape(crate::shape::Kind::Diamond, theme.accent)).width(10).height(10),
        text("LIVE").font(CONDENSED_BOLD).size(13).color(theme.accent),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    column![wave, container(badge).align_right(Length::Fill)].spacing(6).into()
}

/// Phase of the travelling wave at `now` since the epoch, in radians within one period.
/// Reduced in f64 first: an f32 of the epoch seconds only changes every 128 s.
fn wave_phase(now: std::time::Duration) -> f32 {
    let period = 1.0 / f64::from(WAVE_SPEED);
    (now.as_secs_f64() % period / period) as f32 * TAU
}

fn controls<'a>(bar: &'a Bar, player: &'a Player) -> Element<'a, Message> {
    let theme = bar.theme;
    let shuffle = player.shuffle;
    let repeat = player.loop_status;
    let play = container(icon_sized(if player.playing { "pause" } else { "play_arrow" }, 22.0))
        .style(move |_| container::Style { text_color: Some(theme.on_accent), ..Default::default() })
        .padding(Padding::ZERO.right(6.0))
        .center_x(60)
        .center_y(44);
    let play = button(slanted(play, 10.0, theme.accent))
        .on_press_maybe(player.can_play_pause.then_some(Message::Media(Command::PlayPause)))
        .padding(0)
        .style(|_, _| button::Style::default());
    let row = row![
        control(bar, "skip_previous", false, player.can_go_previous.then_some(Message::Media(Command::Previous))),
        control(bar, "shuffle", shuffle == Some(true), shuffle.map(|on| Message::Media(Command::Shuffle(!on)))),
        play,
        control(
            bar,
            if repeat == Some(Loop::Track) { "repeat_one" } else { "repeat" },
            repeat.is_some_and(|status| status != Loop::None),
            repeat.map(|status| Message::Media(Command::Loop(status.next()))),
        ),
        control(bar, "skip_next", false, player.can_go_next.then_some(Message::Media(Command::Next))),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    container(row).center_x(Length::Fill).into()
}

/// Bracketed icon button; `on` marks an enabled toggle. Disabled when the player does not support it.
fn control<'a>(bar: &'a Bar, glyph: &'a str, on: bool, message: Option<Message>) -> Element<'a, Message> {
    let theme = bar.theme;
    let color = match (on, message.is_some()) {
        (_, false) => theme.text_dim,
        (true, true) => theme.glow,
        (false, true) => theme.text,
    };
    let content = Stack::new()
        .push(container(text(glyph).font(crate::ICON_FONT).size(20).color(color).shaping(text::Shaping::Advanced)).center(40))
        .push_under(shape(crate::shape::Kind::BRACKETS, theme.bracket));
    button(content)
        .on_press_maybe(message)
        .padding(0)
        .style(move |_, status| {
            let mut style = glow_style(theme, status, 8.0);
            if on && style.background.is_none() {
                style.background = Some(Color { a: 0.12, ..theme.glow }.into());
            }
            style
        })
        .into()
}

/// cava's spectrum as a faint filled area along the bottom edge.
struct Spectrum<'a> {
    points: &'a [f32],
    color: Color,
}

impl<Message> canvas::Program<Message> for Spectrum<'_> {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let points = cava::smooth(self.points, SPECTRUM_SMOOTHING);
        if points.len() >= 2 {
            let (width, height) = (bounds.width, bounds.height);
            let step = width / (points.len() - 1) as f32;
            let area = Path::new(|builder| {
                builder.move_to(Point::new(0.0, height));
                for (index, value) in points.iter().enumerate() {
                    builder.line_to(Point::new(index as f32 * step, height - value * height));
                }
                builder.line_to(Point::new(width, height));
                builder.close();
            });
            frame.fill(&area, Color { a: SPECTRUM_ALPHA, ..self.color });
        }
        vec![frame.into_geometry()]
    }
}

/// Sine line across the bounds, shifted by `phase` radians.
struct Wave {
    phase: f32,
    amplitude: f32,
    color: Color,
}

impl<Message> canvas::Program<Message> for Wave {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let middle = bounds.height / 2.0;
        let line = Path::new(|builder| {
            builder.move_to(Point::new(0.0, middle - self.amplitude * (-self.phase).sin()));
            let mut x = 2.0;
            while x <= bounds.width {
                let angle = x / WAVE_LENGTH * TAU - self.phase;
                builder.line_to(Point::new(x, middle - self.amplitude * angle.sin()));
                x += 2.0;
            }
        });
        frame.stroke(&line, Stroke::default().with_color(self.color).with_width(3.0));
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn wave_phase_advances_with_time_at_present_day_epochs() {
        let now = Duration::from_secs(1_795_300_000);
        let quarter = Duration::from_secs_f64(0.25 / f64::from(WAVE_SPEED));
        let step = wave_phase(now + quarter) - wave_phase(now);
        assert!((step - TAU / 4.0).abs() < 1e-3, "step was {step}");
        assert!((0.0..TAU).contains(&wave_phase(now + quarter)));
    }
}
