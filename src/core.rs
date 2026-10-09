//! Timer state machine and the shared application state. The state is ticked
//! from a background thread so rounds keep completing (with sound and
//! notifications) while the window is hidden in the tray.

use crate::audio::{Audio, Sound};
use crate::config::{self, Config, History};
use crate::theme::Theme;
use eframe::egui;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Focus => "FOCUS",
            Phase::ShortBreak => "SHORT BREAK",
            Phase::LongBreak => "LONG BREAK",
        }
    }

    pub fn color(self, theme: &Theme) -> egui::Color32 {
        match self {
            Phase::Focus => theme.focus,
            Phase::ShortBreak => theme.short_break,
            Phase::LongBreak => theme.long_break,
        }
    }

}

/// Background work on the cloud copy of the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncJob {
    Sync,
    /// Only look up the gist ID for the current token.
    FindGist,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncState {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

pub struct Core {
    pub cfg: Config,
    pub history: History,

    pub phase: Phase,
    /// 1-based focus round within the current set.
    pub round: u32,
    pub running: bool,
    /// Time left when paused; while running, `deadline` is authoritative.
    remaining: Duration,
    deadline: Option<Instant>,
    /// The current phase has not been started yet, so duration edits apply to it.
    fresh: bool,
    last_tick_sec: u64,

    pub sync_state: SyncState,
    /// Picked up by the ticker thread, which owns starting sync jobs.
    pub sync_request: Option<SyncJob>,

    audio: Option<Audio>,
    notifications: bool,
    persist: bool,
}

pub type Shared = Arc<Mutex<Core>>;

pub fn lock(shared: &Shared) -> MutexGuard<'_, Core> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    pub fn new(cfg: Config, history: History, audio: Option<Audio>) -> Self {
        let remaining = Duration::from_secs(cfg.focus_min as u64 * 60);
        Self {
            cfg,
            history,
            phase: Phase::Focus,
            round: 1,
            running: false,
            remaining,
            deadline: None,
            fresh: true,
            last_tick_sec: 0,
            sync_state: SyncState::Idle,
            sync_request: None,
            notifications: audio.is_some(),
            persist: audio.is_some(),
            audio,
        }
    }

    pub fn phase_duration(&self, phase: Phase) -> Duration {
        let min = match phase {
            Phase::Focus => self.cfg.focus_min,
            Phase::ShortBreak => self.cfg.short_break_min,
            Phase::LongBreak => self.cfg.long_break_min,
        };
        Duration::from_secs(min.max(1) as u64 * 60)
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        match self.deadline {
            Some(deadline) if self.running => deadline.saturating_duration_since(now),
            _ => self.remaining,
        }
    }

    /// Fraction of the current phase still to go, 1.0 at the start.
    pub fn fraction_left(&self, now: Instant) -> f32 {
        let total = self.phase_duration(self.phase).as_secs_f32();
        (self.remaining(now).as_secs_f32() / total).clamp(0.0, 1.0)
    }

    pub fn toggle(&mut self, now: Instant) {
        if self.running {
            self.remaining = self.remaining(now);
            self.running = false;
            self.deadline = None;
        } else {
            self.start(now);
        }
    }

    fn start(&mut self, now: Instant) {
        self.deadline = Some(now + self.remaining);
        self.running = true;
        self.fresh = false;
        self.last_tick_sec = self.remaining.as_secs();
    }

    fn enter(&mut self, phase: Phase, autostart: bool, now: Instant) {
        self.phase = phase;
        self.remaining = self.phase_duration(phase);
        self.running = false;
        self.deadline = None;
        self.fresh = true;
        if autostart {
            self.start(now);
        }
    }

    /// Restart the current phase from its full duration.
    pub fn restart_phase(&mut self) {
        let phase = self.phase;
        self.enter(phase, false, Instant::now());
    }

    /// Back to the first focus round.
    pub fn reset(&mut self) {
        self.round = 1;
        self.enter(Phase::Focus, false, Instant::now());
    }

    /// Move on without counting the current phase as completed.
    pub fn skip(&mut self, now: Instant) {
        let was_running = self.running;
        self.advance(was_running, now);
    }

    fn advance(&mut self, autostart: bool, now: Instant) {
        match self.phase {
            Phase::Focus => {
                let next = if self.round >= self.cfg.rounds {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                };
                self.enter(next, autostart, now);
            }
            Phase::ShortBreak => {
                self.round += 1;
                self.enter(Phase::Focus, autostart, now);
            }
            Phase::LongBreak => {
                self.round = 1;
                self.enter(Phase::Focus, autostart, now);
            }
        }
    }

    /// Call after timer durations were edited in settings.
    pub fn durations_changed(&mut self) {
        self.round = self.round.min(self.cfg.rounds.max(1));
        if self.fresh {
            self.remaining = self.phase_duration(self.phase);
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if !self.running {
            return;
        }
        let left = self.remaining(now);
        if left.is_zero() {
            self.complete(now);
            return;
        }
        let sec = left.as_secs();
        if sec != self.last_tick_sec {
            self.last_tick_sec = sec;
            match self.phase {
                Phase::Focus if self.cfg.tick_sound_focus => self.play(Sound::TickFocus),
                Phase::ShortBreak | Phase::LongBreak if self.cfg.tick_sound_break => {
                    self.play(Sound::TickBreak)
                }
                _ => {}
            }
        }
    }

    fn complete(&mut self, now: Instant) {
        let finished = self.phase;
        if finished == Phase::Focus {
            let today = chrono::Local::now().date_naive();
            let device = self.cfg.device_id.clone();
            self.history.record(today, &device, self.cfg.focus_min);
            if self.persist {
                config::save_history(&self.history);
            }
            if !self.cfg.sync.token.is_empty() {
                self.sync_request = Some(SyncJob::Sync);
            }
            self.play(Sound::FocusEnd);
            self.advance(self.cfg.auto_start_break, now);
            if self.cfg.notify_focus_end {
                let (kind, min) = match self.phase {
                    Phase::LongBreak => ("long", self.cfg.long_break_min),
                    _ => ("short", self.cfg.short_break_min),
                };
                self.notify(
                    "Focus round complete",
                    &format!("Time for a {kind} break ({min} min)."),
                );
            }
        } else {
            self.play(Sound::BreakEnd);
            self.advance(self.cfg.auto_start_focus, now);
            if self.cfg.notify_break_end {
                self.notify(
                    "Break is over",
                    &format!("Ready for focus round {} of {}.", self.round, self.cfg.rounds),
                );
            }
        }
    }

    fn play(&self, sound: Sound) {
        if let Some(audio) = &self.audio {
            if !self.cfg.muted && self.cfg.volume > 0.0 {
                audio.play(sound, self.cfg.volume);
            }
        }
    }

    /// Stop saving and notifying; used when rendering with demo data.
    pub fn detach_from_disk(&mut self) {
        self.persist = false;
        self.notifications = false;
        self.audio = None;
    }

    /// Play the end-of-focus chime, e.g. to preview the volume.
    pub fn preview_sound(&self) {
        self.play(Sound::FocusEnd);
    }

    fn notify(&self, title: &str, body: &str) {
        if !self.notifications {
            return;
        }
        let (title, body) = (title.to_owned(), body.to_owned());
        // Showing a notification can block on the OS; keep it off the timer thread.
        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .appname("PrettyPomodoro")
                .summary(&title)
                .body(&body)
                .show();
        });
    }
}

/// Drive the timer from a background thread and wake the UI while it runs.
pub fn spawn_ticker(shared: Shared, ctx: egui::Context) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let (running, job) = {
            let mut core = lock(&shared);
            core.tick(Instant::now());
            (core.running, core.sync_request.take())
        };
        if let Some(job) = job {
            crate::sync::spawn(shared.clone(), ctx.clone(), job);
        }
        if running || job.is_some() {
            ctx.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> Core {
        let mut cfg = Config::default();
        cfg.rounds = 2;
        cfg.auto_start_break = true;
        cfg.auto_start_focus = false;
        cfg.device_id = "test".into();
        Core::new(cfg, History::default(), None)
    }

    fn finish(c: &mut Core, now: &mut Instant) {
        if !c.running {
            c.toggle(*now);
        }
        *now += c.remaining(*now);
        c.tick(*now);
    }

    #[test]
    fn full_cycle() {
        let mut c = core();
        let mut now = Instant::now();

        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::ShortBreak);
        assert!(c.running, "breaks auto-start");
        assert_eq!(c.history.total().sessions, 1);

        finish(&mut c, &mut now);
        assert_eq!((c.phase, c.round), (Phase::Focus, 2));
        assert!(!c.running, "focus waits for the user");

        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::LongBreak);
        assert_eq!(c.history.total().minutes, 50);

        finish(&mut c, &mut now);
        assert_eq!((c.phase, c.round), (Phase::Focus, 1));
    }

    #[test]
    fn skip_does_not_count() {
        let mut c = core();
        let now = Instant::now();
        c.toggle(now);
        c.skip(now);
        assert_eq!(c.phase, Phase::ShortBreak);
        assert!(c.running);
        assert_eq!(c.history.total().sessions, 0);
    }

    #[test]
    fn pause_keeps_remaining_and_edits_apply_only_when_fresh() {
        let mut c = core();
        let now = Instant::now();
        c.cfg.focus_min = 10;
        c.durations_changed();
        assert_eq!(c.remaining(now), Duration::from_secs(600));

        c.toggle(now);
        let later = now + Duration::from_secs(60);
        c.toggle(later);
        assert_eq!(c.remaining(later + Duration::from_secs(5)), Duration::from_secs(540));

        c.cfg.focus_min = 20;
        c.durations_changed();
        assert_eq!(c.remaining(later), Duration::from_secs(540));
    }
}
