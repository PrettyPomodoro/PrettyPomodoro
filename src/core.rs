//! Timer state machine and the shared application state. The state is ticked
//! from a background thread so rounds keep completing (with sound and
//! notifications) while the window is hidden in the tray.

use crate::audio::{Audio, Sound};
use crate::config::{self, Config, GoalLog, History};
use crate::theme::Theme;
use eframe::egui;
use std::collections::{HashMap, VecDeque};
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

/// An identity provider the backend can sign a user in through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    Facebook,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Facebook => "facebook",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::Google => "Google",
            Provider::Facebook => "Facebook",
        }
    }
}

/// Background work against the backend. Queued by the UI, started by the ticker.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// Merge this computer's history with the server's.
    Sync,
    /// Sign in through the browser with `Provider`.
    SignIn(Provider),
    SignUp { email: String, password: String, display_name: String },
    LogIn { email: String, password: String },
    SignOut,
    DeleteAccount,
    /// Push the display name and leaderboard choice.
    SaveProfile,
    /// Refresh the leaderboard.
    Leaderboard,
    /// Ask which sign-in buttons this server offers.
    Providers,
    /// Fetch one profile picture by its path on the server.
    Avatar(String),
    /// Upload a picture the user picked, as their new profile picture.
    UploadAvatar(std::path::PathBuf),
    RemoveAvatar,
}

/// Progress of a sign-in.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SignIn {
    #[default]
    Idle,
    /// Talking to the server: starting a browser sign-in, or sending a password.
    Working,
    /// The browser is open at `url`; waiting for the user to finish there.
    InBrowser { url: String },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncState {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

/// A profile picture on its way from the server, or already here.
pub enum Avatar {
    Loading,
    Ready(egui::TextureHandle),
    /// There was no picture there, or it could not be read. Not retried.
    Missing,
}

/// How often the leaderboard refreshes while its page is open.
pub const BOARD_REFRESH: Duration = Duration::from_secs(60);

/// The leaderboard as the app last saw it.
#[derive(Default)]
pub struct Board {
    pub data: Option<crate::api::Leaderboard>,
    pub error: Option<String>,
    /// A refresh is in flight.
    pub loading: bool,
    pub fetched_at: Option<Instant>,
    /// The leaderboard page is on screen, so it is worth keeping fresh.
    pub open: bool,
}

impl Board {
    /// Time for another refresh: never fetched, or a minute has gone by.
    pub fn due(&self, now: Instant) -> bool {
        match self.fetched_at {
            None => true,
            Some(at) => now.saturating_duration_since(at) >= BOARD_REFRESH,
        }
    }

    /// How long ago the board was fetched, for the "updated …" line.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.fetched_at.map(|at| now.saturating_duration_since(at))
    }

    /// Mark what is on screen as out of date, so the next tick refreshes it.
    pub fn stale(&mut self) {
        self.fetched_at = None;
    }

    /// A failed attempt is still an attempt: stamping it keeps the ticker from
    /// retrying ten times a second while something is wrong.
    pub fn failed(&mut self, message: String) {
        self.error = Some(message);
        self.fetched_at = Some(Instant::now());
    }
}

pub struct Core {
    pub cfg: Config,
    pub history: History,

    pub phase: Phase,
    /// 1-based focus round within the current set.
    pub round: u32,
    pub running: bool,
    /// What the current focus round is for, typed on the timer page. It stays
    /// through the break after a finished round, to be ticked off, and is
    /// cleared for the next round.
    pub goal: String,
    /// The goal has been ticked off; set through `set_goal_achieved`.
    pub goal_achieved: bool,
    /// Every goal ticked off so far.
    pub goals: GoalLog,
    /// Time left when paused; while running, `deadline` is authoritative.
    remaining: Duration,
    deadline: Option<Instant>,
    /// The current phase has not been started yet, so duration edits apply to it.
    fresh: bool,
    last_tick_sec: u64,
    /// Focus time spent in the current round, measured tick by tick so that
    /// editing the round length part-way through cannot invent minutes.
    focus_elapsed: Duration,
    /// Whole minutes of `focus_elapsed` already added to the history.
    focus_credited: u32,
    /// When the last tick measured; `None` while paused.
    measured_at: Option<Instant>,

    pub sync_state: SyncState,
    pub sign_in: SignIn,
    /// Bumped to abandon a sign-in that is still waiting.
    pub sign_in_attempt: u64,
    /// Queued background work, drained by the ticker thread, which owns
    /// starting jobs.
    pub jobs: VecDeque<Job>,
    pub board: Board,
    /// The Activity page is on screen, so the history is worth refreshing: a
    /// sync brings in what the user's other computers have focused.
    pub activity_open: bool,
    /// When the last sync finished, successfully or not.
    pub synced_at: Option<Instant>,
    /// Which sign-in buttons the server offers, once it has said.
    pub providers: Option<crate::api::Providers>,
    /// Profile pictures by their path on the server.
    pub avatars: HashMap<String, Avatar>,
    /// What went wrong with the last profile change, for the Account page.
    pub profile_error: Option<String>,

    /// A newer release, once the update check has found one.
    pub update: crate::update::UpdateState,
    /// Set by the Update button; picked up by the ticker thread.
    pub update_request: bool,

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
            goal: String::new(),
            goal_achieved: false,
            goals: GoalLog::default(),
            remaining,
            deadline: None,
            fresh: true,
            last_tick_sec: 0,
            focus_elapsed: Duration::ZERO,
            focus_credited: 0,
            measured_at: None,
            sync_state: SyncState::Idle,
            sign_in: SignIn::Idle,
            sign_in_attempt: 0,
            jobs: VecDeque::new(),
            board: Board::default(),
            activity_open: false,
            synced_at: None,
            providers: None,
            avatars: HashMap::new(),
            profile_error: None,
            update: Default::default(),
            update_request: false,
            notifications: audio.is_some(),
            persist: audio.is_some(),
            audio,
        }
    }

    /// Queue background work for the ticker thread to start. Asking for the
    /// same job twice before it runs is the same as asking once.
    pub fn request(&mut self, job: Job) {
        if !self.jobs.contains(&job) {
            self.jobs.push_back(job);
        }
    }

    /// Queue a sync, if there is an account to sync with and this instance is
    /// allowed to reach the network at all.
    fn request_sync(&mut self) {
        if !self.detached() && self.cfg.account.signed_in() {
            self.request(Job::Sync);
        }
    }

    /// The picture at `path`, asking the server for it the first time it is
    /// wanted. `None` means draw initials for now.
    pub fn avatar(&mut self, path: Option<&str>) -> Option<egui::TextureHandle> {
        let path = path?;
        if let Some(have) = self.avatars.get(path) {
            return match have {
                Avatar::Ready(texture) => Some(texture.clone()),
                Avatar::Loading | Avatar::Missing => None,
            };
        }
        self.avatars.insert(path.to_owned(), Avatar::Loading);
        if !self.detached() {
            self.request(Job::Avatar(path.to_owned()));
        }
        None
    }

    /// Demo captures and tests run detached: no disk, no sound, no network.
    pub fn detached(&self) -> bool {
        !self.persist
    }

    pub fn save_config(&self) {
        if self.persist {
            config::save_config(&self.cfg);
        }
    }

    pub fn save_history(&self) {
        if self.persist {
            config::save_history(&self.history);
        }
    }

    pub fn save_goals(&self) {
        if self.persist {
            config::save_goals(&self.goals);
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
            self.measure_focus(now);
            self.remaining = self.remaining(now);
            self.running = false;
            self.deadline = None;
            // Stop the focus clock; a part-minute is kept and carries on later.
            self.measured_at = None;
        } else {
            self.start(now);
        }
    }

    fn start(&mut self, now: Instant) {
        self.deadline = Some(now + self.remaining);
        self.running = true;
        self.fresh = false;
        self.last_tick_sec = self.remaining.as_secs();
        self.measured_at = Some(now);
    }

    fn enter(&mut self, phase: Phase, autostart: bool, now: Instant) {
        self.phase = phase;
        self.remaining = self.phase_duration(phase);
        self.running = false;
        self.deadline = None;
        self.fresh = true;
        // Minutes already credited stay in the history; the count starts over.
        self.focus_elapsed = Duration::ZERO;
        self.focus_credited = 0;
        self.measured_at = None;
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
        self.clear_goal();
        self.enter(Phase::Focus, false, Instant::now());
    }

    /// Move on without counting the current phase as completed.
    pub fn skip(&mut self, now: Instant) {
        if self.phase == Phase::Focus {
            self.clear_goal();
        }
        let was_running = self.running;
        self.advance(was_running, now);
    }

    /// Tick the current goal off, or undo that. A ticked goal is entered in
    /// the goal log under today; unticking takes the entry back.
    pub fn set_goal_achieved(&mut self, achieved: bool) {
        let goal = self.goal.trim();
        if achieved == self.goal_achieved || goal.is_empty() {
            return;
        }
        if achieved {
            self.goals.add(chrono::Local::now().date_naive(), goal);
        } else {
            self.goals.remove(goal);
        }
        self.goal_achieved = achieved;
        self.save_goals();
        if self.cfg.account.sync_goals {
            self.request_sync();
        }
    }

    fn clear_goal(&mut self) {
        self.goal.clear();
        self.goal_achieved = false;
    }

    /// A ticked goal is done, so the next focus round starts fresh; an
    /// unticked one carries over.
    fn clear_achieved_goal(&mut self) {
        if self.goal_achieved {
            self.clear_goal();
        }
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
                self.clear_achieved_goal();
                self.round += 1;
                self.enter(Phase::Focus, autostart, now);
            }
            Phase::LongBreak => {
                self.clear_achieved_goal();
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
        self.measure_focus(now);
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

    /// Add the time since the last tick to this round's focus, and hand whole
    /// minutes to the history as they are reached.
    ///
    /// Capped at the length of the round, so a laptop that slept through one
    /// credits the round and no more.
    fn measure_focus(&mut self, now: Instant) {
        if self.phase != Phase::Focus {
            return;
        }
        if let Some(previous) = self.measured_at {
            let step = now.saturating_duration_since(previous);
            self.focus_elapsed =
                (self.focus_elapsed + step).min(self.phase_duration(Phase::Focus));
        }
        self.measured_at = Some(now);

        let whole = (self.focus_elapsed.as_secs() / 60) as u32;
        if whole <= self.focus_credited {
            return;
        }
        // Filed under the date right now, so a round over midnight splits.
        let day = chrono::Local::now().date_naive();
        let device = self.cfg.device_id.clone();
        self.history.add_minutes(day, &device, whole - self.focus_credited);
        self.focus_credited = whole;
        self.save_history();
        self.request_sync();
        // The board is a minute out of date the moment this lands.
        self.board.stale();
    }

    fn complete(&mut self, now: Instant) {
        let finished = self.phase;
        if finished == Phase::Focus {
            // The minutes went in as they passed; only the round is left to count.
            let today = chrono::Local::now().date_naive();
            let device = self.cfg.device_id.clone();
            self.history.add_session(today, &device);
            self.save_history();
            self.request_sync();
            self.board.stale();
            self.play(Sound::FocusEnd);
            if self.goal.trim().is_empty() {
                self.clear_goal();
            }
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

    pub fn notify(&self, title: &str, body: &str) {
        if !self.notifications {
            return;
        }
        let (title, body) = (title.to_owned(), body.to_owned());
        // Showing a notification can block on the OS; keep it off the timer thread.
        std::thread::spawn(move || {
            // Without this, the notification library asks AppleScript for an app
            // named "use_default", which makes macOS show a "Choose Application"
            // window. Only the first call takes effect, so later errors are fine.
            #[cfg(target_os = "macos")]
            let _ = notify_rust::set_application("io.github.ahmed-abd-elwahed.prettypomodoro");
            let _ = notify_rust::Notification::new()
                .appname("PrettyPomodoro")
                .summary(&title)
                .body(&body)
                .show();
        });
    }
}

/// Drive the timer from a background thread, start the queued jobs and wake the
/// UI while either is working.
pub fn spawn_ticker(shared: Shared, ctx: egui::Context) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let (running, jobs, update) = {
            let now = Instant::now();
            let mut core = lock(&shared);
            core.tick(now);
            // Keep the board, and the history behind the Activity page, fresh
            // for as long as the page showing them is open.
            if !core.detached() && core.board.open && !core.board.loading && core.board.due(now) {
                core.request(Job::Leaderboard);
            }
            if !core.detached() && core.activity_open && core.sync_state != SyncState::Running {
                let due = core
                    .synced_at
                    .is_none_or(|at| now.saturating_duration_since(at) >= BOARD_REFRESH);
                if due {
                    core.request_sync();
                }
            }
            let jobs: Vec<Job> = core.jobs.drain(..).collect();
            (core.running, jobs, std::mem::take(&mut core.update_request))
        };
        if update {
            crate::update::spawn_install(shared.clone(), ctx.clone());
        }
        // Started with the lock released: each job takes it for itself.
        for job in &jobs {
            crate::api::spawn(shared.clone(), ctx.clone(), job.clone());
        }
        if running || !jobs.is_empty() {
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
    fn minutes_are_credited_as_they_pass_not_at_the_end() {
        let mut c = core();
        let mut now = Instant::now();
        c.toggle(now);

        now += Duration::from_secs(60);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 1, "the first minute counts straight away");
        assert_eq!(c.history.total().sessions, 0, "but the round is not done");

        now += Duration::from_secs(90);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 2, "part minutes wait their turn");

        now += Duration::from_secs(30);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 3);

        // Giving up keeps what was really focused, and counts no round.
        c.skip(now);
        assert_eq!(c.history.total().minutes, 3);
        assert_eq!(c.history.total().sessions, 0);
    }

    #[test]
    fn a_pause_stops_the_clock_and_keeps_the_part_minute() {
        let mut c = core();
        let mut now = Instant::now();
        c.toggle(now);
        now += Duration::from_secs(90);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 1);

        c.toggle(now); // paused at a minute and a half
        now += Duration::from_secs(30 * 60);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 1, "time away from the timer is not focus");

        c.toggle(now); // back to it; the half minute carries on
        now += Duration::from_secs(30);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 2);
    }

    #[test]
    fn a_longer_round_set_part_way_through_does_not_backfill() {
        let mut c = core();
        let mut now = Instant::now();
        c.toggle(now);
        now += Duration::from_secs(120);
        c.tick(now);
        assert_eq!(c.history.total().minutes, 2);

        c.cfg.focus_min = 90;
        c.durations_changed();
        c.tick(now);
        assert_eq!(c.history.total().minutes, 2, "two minutes focused is two minutes");
    }

    #[test]
    fn a_sleeping_laptop_credits_the_round_and_no_more() {
        let mut c = core();
        let now = Instant::now();
        c.toggle(now);
        c.tick(now + Duration::from_secs(3 * 3600));
        let total = c.history.total();
        assert_eq!(total.minutes, 25, "a round is worth its own length at most");
        assert_eq!(total.sessions, 1, "and it did finish");
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
    fn goal_and_its_tick_last_through_the_break() {
        let mut c = core();
        let mut now = Instant::now();
        c.goal = "Write the report".into();
        c.toggle(now);
        c.toggle(now);
        c.restart_phase();
        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::ShortBreak);
        assert_eq!((c.goal.as_str(), c.goal_achieved), ("Write the report", false));

        // Ticked during the break, the goal enters the log; unticking takes it back.
        c.set_goal_achieved(true);
        assert_eq!(c.goals.days.values().next().unwrap(), &["Write the report"]);
        c.set_goal_achieved(false);
        assert!(c.goals.days.is_empty());
        c.set_goal_achieved(true);

        // The next focus round starts with an empty, unticked goal.
        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::Focus);
        assert_eq!((c.goal.as_str(), c.goal_achieved), ("", false));

        // The log keeps the achieved goal after it has left the timer.
        assert_eq!(c.goals.days.values().next().unwrap(), &["Write the report"]);

        // An unticked goal carries over into the next focus round.
        c.goal = "Read the paper".into();
        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::LongBreak);
        finish(&mut c, &mut now);
        assert_eq!(c.phase, Phase::Focus);
        assert_eq!((c.goal.as_str(), c.goal_achieved), ("Read the paper", false));

        // A skipped round takes its goal with it.
        c.goal = "Skipped".into();
        c.skip(now);
        assert_eq!((c.goal.as_str(), c.goal_achieved), ("", false));
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
