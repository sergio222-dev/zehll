//! Workspace indicator of each bar: the workspaces of its own monitor (hyprsplit gives each
//! monitor a range of `PER_MONITOR` ids) as dots, the current one as an accent disc that slides
//! to the new slot leaving a trail drawn by `trail.wgsl`.

use std::time::{Duration, Instant};

use iced::widget::{Row, Shader, Space, Stack, container, mouse_area};
use iced::{Border, Color, Element, Length, Padding, Shadow, mouse};

use crate::theme::Theme;
use crate::trail::Trail;

const DOT: f32 = 6.0;
const DISC: f32 = 18.0;
// Room on each side of the slots, apart from the bar row's spacing; even, so the slots stay
// centered on the bar.
const MARGIN: f32 = 6.0;

/// hyprsplit's `num_workspaces`.
pub const PER_MONITOR: i64 = 4;
/// Width of each workspace's slot; fixed so the disc moves between known centers.
pub const SLOT: f32 = 22.0;
/// How long a move lasts, until the trail has faded.
pub const DURATION: Duration = Duration::from_millis(450);
// The head eases out into its slot; the tail follows more slowly, so the streak stretches.
const HEAD: Duration = Duration::from_millis(160);
const TAIL: Duration = Duration::from_millis(320);

/// Slot of a workspace id in its monitor's range; `None` for special workspaces.
pub fn slot(workspace: i64) -> Option<usize> {
    (workspace >= 1).then(|| ((workspace - 1) % PER_MONITOR) as usize)
}

/// Id of the workspace in `slot` of the range that holds `active`, the monitor's workspace.
pub fn target(active: i64, slot: usize) -> Option<i64> {
    let current = self::slot(active)?;
    Some(active - current as i64 + slot as i64)
}

/// Which slots of a monitor have windows, from the ids of its workspaces with windows.
pub fn occupied_slots(occupied: &[i64]) -> [bool; PER_MONITOR as usize] {
    let mut slots = [false; PER_MONITOR as usize];
    for slot in occupied.iter().filter_map(|id| slot(*id)) {
        slots[slot] = true;
    }
    slots
}

/// Head and tail progress (0 to 1) and the trail's opacity, `elapsed` into a move.
fn timing(elapsed: Duration) -> (f32, f32, f32) {
    let fraction = |of: Duration| (elapsed.as_secs_f32() / of.as_secs_f32()).clamp(0.0, 1.0);
    // Ease-out cubic: fast start, gentle landing.
    let head = 1.0 - (1.0 - fraction(HEAD)).powi(3);
    // Slow start, so the streak stretches out behind the head before it shrinks.
    let tail = fraction(TAIL).powf(1.6);
    let t = fraction(DURATION);
    let fade = 1.0 - smoothstep(0.55, 1.0, t);
    (head, tail.min(head), fade)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where the disc and the trail's tail are, in slots, and the trail's opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub head: f32,
    pub tail: f32,
    pub fade: f32,
}

/// The current workspace of one monitor and its move in progress.
#[derive(Debug, Clone, Copy)]
pub struct Indicator {
    slot: usize,
    // Position the last move started from, in slots.
    from: f32,
    changed: Option<Instant>,
}

impl Indicator {
    pub fn new(slot: usize) -> Self {
        Self { slot, from: slot as f32, changed: None }
    }

    /// Starts a move to `slot` from where the disc is drawn now.
    pub fn move_to(&mut self, slot: usize, now: Instant) {
        if slot == self.slot {
            return;
        }
        self.from = self.frame(now).head;
        self.slot = slot;
        self.changed = Some(now);
    }

    pub fn frame(&self, now: Instant) -> Frame {
        let to = self.slot as f32;
        let Some(changed) = self.changed else {
            return Frame { head: to, tail: to, fade: 0.0 };
        };
        let (head, tail, fade) = timing(now.saturating_duration_since(changed));
        let at = |progress: f32| self.from + (to - self.from) * progress;
        Frame { head: at(head), tail: at(tail), fade }
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.changed.is_some_and(|changed| now.saturating_duration_since(changed) < DURATION)
    }

    /// Ends a move whose trail has faded.
    pub fn settle(&mut self, now: Instant) {
        if !self.animating(now) {
            self.changed = None;
        }
    }
}

/// A monitor's workspaces, a dot each or a faint circle when it has windows, with the disc on
/// the current one; no disc when the monitor is unknown. Clicking a slot sends `select(slot)`.
pub fn view<'a, Message: Clone + 'a>(
    indicator: Option<&Indicator>,
    occupied: [bool; PER_MONITOR as usize],
    theme: Theme,
    select: impl Fn(usize) -> Option<Message>,
) -> Element<'a, Message> {
    let slot = |occupied: bool| {
        let mark = match occupied {
            true => round_with(round(DOT, mix(theme.accent, Color::WHITE, 0.7), None), DISC, Color { a: 0.22, ..theme.accent }, None),
            false => round(DOT, theme.bracket, None),
        };
        container(mark).center_x(Length::Fixed(SLOT)).center_y(Length::Fill)
    };
    let dots = Row::with_children(occupied.map(|occupied| slot(occupied).into()));
    let mut layers = Stack::new().width(Length::Fixed(SLOT * PER_MONITOR as f32)).height(Length::Fill).push(dots);

    if let Some(indicator) = indicator {
        let frame = indicator.frame(Instant::now());
        let center = |position: f32| position * SLOT + SLOT / 2.0;
        if frame.fade > 0.0 && (frame.head - frame.tail).abs() > 0.01 {
            let trail = Trail { tail: center(frame.tail), head: center(frame.head), fade: frame.fade, color: theme.accent };
            layers = layers.push(Shader::new(trail).width(Length::Fill).height(Length::Fill));
        }
        let core = round(DOT, theme.bar_background, None);
        let glow = Shadow { color: Color { a: 0.45, ..theme.accent }, blur_radius: 8.0, ..Default::default() };
        let disc = container(round_with(core, DISC, theme.accent, Some(glow))).center_y(Length::Fill);
        let left = center(frame.head) - DISC / 2.0;
        layers = layers.push(container(disc).padding(Padding::ZERO.left(left)));
    }
    // On top, so the disc and the trail do not swallow the clicks.
    let targets = (0..PER_MONITOR as usize).map(|slot| {
        let area = mouse_area(Space::new().width(Length::Fixed(SLOT)).height(Length::Fill));
        match select(slot) {
            Some(message) => area.on_press(message).interaction(mouse::Interaction::Pointer).into(),
            None => area.into(),
        }
    });
    layers = layers.push(Row::with_children(targets));
    container(layers).padding([0.0, MARGIN]).height(Length::Fill).into()
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    let channel = |a: f32, b: f32| a + (b - a) * amount;
    Color::from_rgb(channel(from.r, to.r), channel(from.g, to.g), channel(from.b, to.b))
}

fn round<'a, Message: 'a>(size: f32, color: Color, shadow: Option<Shadow>) -> Element<'a, Message> {
    round_with(Space::new(), size, color, shadow)
}

fn round_with<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    size: f32,
    color: Color,
    shadow: Option<Shadow>,
) -> Element<'a, Message> {
    container(content)
        .center(Length::Fixed(size))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: Border { radius: (size / 2.0).into(), ..Default::default() },
            shadow: shadow.unwrap_or_default(),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn workspaces_map_to_their_slot_in_the_monitor_range() {
        assert_eq!([1, 2, 3, 4, 5, 8, 11].map(slot), [0, 1, 2, 3, 0, 3, 2].map(Some));
        assert_eq!(slot(0), None);
        assert_eq!(slot(-98), None);
    }

    #[test]
    fn a_click_picks_the_workspace_in_the_same_range() {
        assert_eq!(target(6, 0), Some(5));
        assert_eq!(target(6, 3), Some(8));
        assert_eq!(target(1, 2), Some(3));
        assert_eq!(target(-98, 1), None);
    }

    #[test]
    fn workspaces_with_windows_mark_their_slot() {
        assert_eq!(occupied_slots(&[5, 8]), [true, false, false, true]);
        assert_eq!(occupied_slots(&[]), [false; 4]);
        assert_eq!(occupied_slots(&[-98, 0]), [false; 4]);
    }

    #[test]
    fn the_head_leads_and_the_trail_fades_by_the_end() {
        assert_eq!(timing(Duration::ZERO), (0.0, 0.0, 1.0));
        assert_eq!(timing(HEAD).0, 1.0);
        assert_eq!(timing(DURATION), (1.0, 1.0, 0.0));
        let mut elapsed = Duration::ZERO;
        while elapsed <= DURATION {
            let (head, tail, fade) = timing(elapsed);
            assert!(tail <= head, "{elapsed:?}");
            assert!((0.0..=1.0).contains(&fade), "{elapsed:?}");
            elapsed += 10 * MS;
        }
    }

    #[test]
    fn a_move_runs_from_the_old_slot_to_the_new_one() {
        let start = Instant::now();
        let mut indicator = Indicator::new(0);
        indicator.move_to(2, start);
        assert_eq!(indicator.frame(start), Frame { head: 0.0, tail: 0.0, fade: 1.0 });
        let middle = indicator.frame(start + 100 * MS);
        assert!(middle.tail < middle.head && middle.head < 2.0);
        assert!(indicator.animating(start + 100 * MS));
        assert_eq!(indicator.frame(start + DURATION), Frame { head: 2.0, tail: 2.0, fade: 0.0 });
        assert!(!indicator.animating(start + DURATION));
        indicator.settle(start + DURATION);
        assert_eq!(indicator.frame(start), Frame { head: 2.0, tail: 2.0, fade: 0.0 });
    }

    #[test]
    fn a_new_move_starts_where_the_disc_is_drawn() {
        let start = Instant::now();
        let mut indicator = Indicator::new(0);
        indicator.move_to(3, start);
        let now = start + 80 * MS;
        let head = indicator.frame(now).head;
        indicator.move_to(1, now);
        assert_eq!(indicator.frame(now), Frame { head, tail: head, fade: 1.0 });
        assert_eq!(indicator.frame(now + DURATION).head, 1.0);
    }

    #[test]
    fn staying_on_the_same_slot_does_not_move() {
        let start = Instant::now();
        let mut indicator = Indicator::new(1);
        indicator.move_to(1, start);
        assert!(!indicator.animating(start));
    }
}
