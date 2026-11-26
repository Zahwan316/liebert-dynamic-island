use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PomodoroPhase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::ShortBreak => "Short Break",
            Self::LongBreak => "Long Break",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::ShortBreak => "Break",
            Self::LongBreak => "Long Break",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PomodoroTimer {
    pub phase: PomodoroPhase,
    pub running: bool,
    pub remaining_secs: u64,
    pub total_secs: u64,
    pub completed_focus: u32,
    pub focus_mins: u64,
    pub short_break_mins: u64,
    pub long_break_mins: u64,
    last_tick: Option<Instant>,
}

impl PomodoroTimer {
    pub fn new(focus_mins: u64, short_break_mins: u64, long_break_mins: u64) -> Self {
        let focus = focus_mins.clamp(1, 120);
        let total = focus * 60;
        Self {
            phase: PomodoroPhase::Focus,
            running: false,
            remaining_secs: total,
            total_secs: total,
            completed_focus: 0,
            focus_mins: focus,
            short_break_mins: short_break_mins.clamp(1, 60),
            long_break_mins: long_break_mins.clamp(1, 90),
            last_tick: None,
        }
    }

    pub fn sync_durations(&mut self, focus_mins: u64, short_break_mins: u64, long_break_mins: u64) {
        let f = focus_mins.clamp(1, 120);
        let sb = short_break_mins.clamp(1, 60);
        let lb = long_break_mins.clamp(1, 90);
        if self.focus_mins == f && self.short_break_mins == sb && self.long_break_mins == lb {
            return;
        }
        self.focus_mins = f;
        self.short_break_mins = sb;
        self.long_break_mins = lb;
        if !self.running && self.remaining_secs == self.total_secs {
            let secs = self.phase_duration_secs(self.phase);
            self.total_secs = secs;
            self.remaining_secs = secs;
        }
    }

    pub fn toggle(&mut self, now: Instant) {
        self.running = !self.running;
        self.last_tick = if self.running { Some(now) } else { None };
    }

    pub fn reset(&mut self) {
        self.running = false;
        self.last_tick = None;
        self.phase = PomodoroPhase::Focus;
        self.completed_focus = 0;
        let secs = self.phase_duration_secs(self.phase);
        self.total_secs = secs;
        self.remaining_secs = secs;
    }

    pub fn skip(&mut self) -> PomodoroPhase {
        self.advance_phase(false)
    }

    /// Advances the timer using `now`; returns `Some(finished_phase)` when a phase completes.
    pub fn tick(&mut self, now: Instant) -> Option<PomodoroPhase> {
        if !self.running {
            return None;
        }
        let prev = self.last_tick.get_or_insert(now);
        let elapsed = now.saturating_duration_since(*prev);
        let whole_secs = elapsed.as_secs();
        if whole_secs == 0 {
            return None;
        }
        *prev += Duration::from_secs(whole_secs);
        if whole_secs >= self.remaining_secs {
            Some(self.advance_phase(true))
        } else {
            self.remaining_secs -= whole_secs;
            None
        }
    }

    pub fn progress(&self) -> f32 {
        if self.total_secs == 0 {
            return 0.0;
        }
        let done = self.total_secs.saturating_sub(self.remaining_secs) as f32;
        (done / self.total_secs as f32).clamp(0.0, 1.0)
    }

    pub fn format_mm_ss(&self) -> String {
        format!("{:02}:{:02}", self.remaining_secs / 60, self.remaining_secs % 60)
    }

    fn phase_duration_secs(&self, phase: PomodoroPhase) -> u64 {
        match phase {
            PomodoroPhase::Focus => self.focus_mins * 60,
            PomodoroPhase::ShortBreak => self.short_break_mins * 60,
            PomodoroPhase::LongBreak => self.long_break_mins * 60,
        }
    }

    fn advance_phase(&mut self, count_completed: bool) -> PomodoroPhase {
        let finished = self.phase;
        self.running = false;
        self.last_tick = None;
        self.phase = match finished {
            PomodoroPhase::Focus => {
                if count_completed {
                    self.completed_focus = self.completed_focus.saturating_add(1);
                }
                if count_completed && self.completed_focus.is_multiple_of(4) {
                    PomodoroPhase::LongBreak
                } else {
                    PomodoroPhase::ShortBreak
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => PomodoroPhase::Focus,
        };
        let secs = self.phase_duration_secs(self.phase);
        self.total_secs = secs;
        self.remaining_secs = secs;
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pomodoro_counts_down_and_transitions_to_break() {
        let t0 = Instant::now();
        let mut p = PomodoroTimer::new(1, 5, 15);
        assert_eq!(p.format_mm_ss(), "01:00");
        p.toggle(t0);
        assert_eq!(p.tick(t0 + Duration::from_secs(20)), None);
        assert_eq!(p.format_mm_ss(), "00:40");
        assert_eq!(
            p.tick(t0 + Duration::from_secs(60)),
            Some(PomodoroPhase::Focus)
        );
        assert_eq!(p.phase, PomodoroPhase::ShortBreak);
        assert_eq!(p.completed_focus, 1);
        assert!(!p.running);
        assert_eq!(p.format_mm_ss(), "05:00");
    }

    #[test]
    fn pomodoro_triggers_long_break_after_four_focus_sessions() {
        let t0 = Instant::now();
        let mut p = PomodoroTimer::new(1, 5, 15);
        for i in 0..4 {
            p.phase = PomodoroPhase::Focus;
            p.remaining_secs = 1;
            p.total_secs = 60;
            p.toggle(t0);
            let done = p.tick(t0 + Duration::from_secs(1));
            assert_eq!(done, Some(PomodoroPhase::Focus));
            if i < 3 {
                assert_eq!(p.phase, PomodoroPhase::ShortBreak);
            } else {
                assert_eq!(p.phase, PomodoroPhase::LongBreak);
            }
        }
        assert_eq!(p.completed_focus, 4);

        p.reset();
        assert_eq!(p.phase, PomodoroPhase::Focus);
        assert_eq!(p.completed_focus, 0);
        assert!(!p.running);
        assert_eq!(p.format_mm_ss(), "01:00");
    }
}
