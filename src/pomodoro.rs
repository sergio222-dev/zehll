use std::time::Duration;

// ii's defaults.
const FOCUS: Duration = Duration::from_secs(25 * 60);
const BREAK: Duration = Duration::from_secs(5 * 60);
const LONG_BREAK: Duration = Duration::from_secs(15 * 60);
pub const CYCLES_BEFORE_LONG_BREAK: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Focus,
    Break,
    LongBreak,
}

impl Phase {
    pub fn duration(self) -> Duration {
        match self {
            Phase::Focus => FOCUS,
            Phase::Break => BREAK,
            Phase::LongBreak => LONG_BREAK,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pomodoro {
    pub phase: Phase,
    pub remaining: Duration,
    pub running: bool,
    /// Focus phases finished since the last long break.
    pub cycle: u32,
}

impl Default for Pomodoro {
    fn default() -> Self {
        Self { phase: Phase::Focus, remaining: FOCUS, running: false, cycle: 0 }
    }
}

impl Pomodoro {
    /// Advances a running timer by `elapsed`; returns the phase it just finished, if any.
    pub fn tick(&mut self, elapsed: Duration) -> Option<Phase> {
        if !self.running {
            return None;
        }
        if elapsed < self.remaining {
            self.remaining -= elapsed;
            return None;
        }
        Some(self.skip())
    }

    /// Moves to the next phase, keeping whether the timer runs.
    pub fn skip(&mut self) -> Phase {
        let finished = self.phase;
        self.phase = match finished {
            Phase::Focus => {
                self.cycle += 1;
                if self.cycle >= CYCLES_BEFORE_LONG_BREAK { Phase::LongBreak } else { Phase::Break }
            }
            Phase::Break => Phase::Focus,
            Phase::LongBreak => {
                self.cycle = 0;
                Phase::Focus
            }
        };
        self.remaining = self.phase.duration();
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Pomodoro {
        Pomodoro { running: true, ..Pomodoro::default() }
    }

    #[test]
    fn counts_down_only_while_running() {
        let mut timer = Pomodoro::default();
        assert_eq!(timer.tick(Duration::from_secs(1)), None);
        assert_eq!(timer.remaining, FOCUS);
        let mut timer = running();
        timer.tick(Duration::from_secs(1));
        assert_eq!(timer.remaining, FOCUS - Duration::from_secs(1));
    }

    #[test]
    fn focus_ends_in_a_break() {
        let mut timer = running();
        assert_eq!(timer.tick(FOCUS), Some(Phase::Focus));
        assert_eq!((timer.phase, timer.remaining, timer.cycle), (Phase::Break, BREAK, 1));
        assert!(timer.running);
    }

    #[test]
    fn every_fourth_focus_ends_in_a_long_break() {
        let mut timer = running();
        for _ in 0..3 {
            timer.tick(FOCUS);
            timer.tick(BREAK);
        }
        timer.tick(FOCUS);
        assert_eq!(timer.phase, Phase::LongBreak);
        timer.tick(LONG_BREAK);
        assert_eq!((timer.phase, timer.cycle), (Phase::Focus, 0));
    }

    #[test]
    fn skip_keeps_running_state() {
        let mut timer = Pomodoro::default();
        assert_eq!(timer.skip(), Phase::Focus);
        assert_eq!((timer.phase, timer.running), (Phase::Break, false));
    }
}
