//! Persistent settings and focus history.

use crate::theme::{builtin_themes, Theme, DEFAULT_THEME};
use chrono::NaiveDate;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub focus_min: u32,
    pub short_break_min: u32,
    pub long_break_min: u32,
    pub rounds: u32,

    pub always_on_top: bool,
    pub auto_start_focus: bool,
    pub auto_start_break: bool,
    pub tick_sound_focus: bool,
    pub tick_sound_break: bool,
    pub notify_focus_end: bool,
    pub notify_break_end: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,

    pub volume: f32,
    pub muted: bool,

    pub theme: String,
    pub custom_themes: Vec<Theme>,

    pub sync: SyncConfig,
    /// Identifies this install in the synced history.
    pub device_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// GitHub access token from "Sign in with GitHub".
    pub token: String,
    /// GitHub username, once known.
    pub login: Option<String>,
    pub gist_id: String,
    pub last_sync: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            focus_min: 25,
            short_break_min: 5,
            long_break_min: 15,
            rounds: 4,
            always_on_top: false,
            auto_start_focus: false,
            auto_start_break: true,
            tick_sound_focus: false,
            tick_sound_break: false,
            notify_focus_end: true,
            notify_break_end: true,
            minimize_to_tray: false,
            close_to_tray: false,
            volume: 0.7,
            muted: false,
            theme: DEFAULT_THEME.to_owned(),
            custom_themes: Vec::new(),
            sync: SyncConfig::default(),
            device_id: String::new(),
        }
    }
}

impl Config {
    pub fn reset_timer_defaults(&mut self) {
        let d = Config::default();
        self.focus_min = d.focus_min;
        self.short_break_min = d.short_break_min;
        self.long_break_min = d.long_break_min;
        self.rounds = d.rounds;
    }

    pub fn all_themes(&self) -> Vec<Theme> {
        let mut themes = builtin_themes();
        themes.extend(self.custom_themes.iter().cloned());
        themes
    }

    pub fn active_theme(&self) -> Theme {
        let themes = self.all_themes();
        let named = |name: &str| themes.iter().find(|t| t.name == name);
        named(&self.theme).or_else(|| named(DEFAULT_THEME)).unwrap_or(&themes[0]).clone()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayStat {
    pub sessions: u32,
    pub minutes: u32,
}

/// Focus history: day -> device -> totals. Each device only ever increases its
/// own entry, so merging two copies is a per-device max and never double counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub days: BTreeMap<NaiveDate, BTreeMap<String, DayStat>>,
}

impl History {
    pub fn record(&mut self, day: NaiveDate, device: &str, minutes: u32) {
        let stat = self
            .days
            .entry(day)
            .or_default()
            .entry(device.to_owned())
            .or_default();
        stat.sessions += 1;
        stat.minutes += minutes;
    }

    pub fn day(&self, day: NaiveDate) -> DayStat {
        let mut total = DayStat::default();
        if let Some(devices) = self.days.get(&day) {
            for s in devices.values() {
                total.sessions += s.sessions;
                total.minutes += s.minutes;
            }
        }
        total
    }

    pub fn total(&self) -> DayStat {
        let mut total = DayStat::default();
        for day in self.days.keys() {
            let s = self.day(*day);
            total.sessions += s.sessions;
            total.minutes += s.minutes;
        }
        total
    }

    /// Consecutive days with at least one session, ending today (or yesterday
    /// if today has none yet).
    pub fn streak(&self, today: NaiveDate) -> u32 {
        let mut day = today;
        if self.day(day).sessions == 0 {
            match day.pred_opt() {
                Some(d) => day = d,
                None => return 0,
            }
        }
        let mut n = 0;
        while self.day(day).sessions > 0 {
            n += 1;
            match day.pred_opt() {
                Some(d) => day = d,
                None => break,
            }
        }
        n
    }

    pub fn merge(&mut self, other: &History) {
        for (day, devices) in &other.days {
            let mine = self.days.entry(*day).or_default();
            for (device, stat) in devices {
                let s = mine.entry(device.clone()).or_default();
                s.sessions = s.sessions.max(stat.sessions);
                s.minutes = s.minutes.max(stat.minutes);
            }
        }
    }
}

pub fn data_dir() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pomodoro-rs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load<T: DeserializeOwned + Default>(file: &str) -> T {
    let path = data_dir().join(file);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    serde_json::from_str(&text).unwrap_or_else(|_| {
        // Keep the unreadable file around instead of overwriting it on the next save.
        let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        T::default()
    })
}

fn save<T: Serialize>(file: &str, value: &T, private: bool) {
    let path = data_dir().join(file);
    let tmp = path.with_extension("tmp");
    let Ok(json) = serde_json::to_string_pretty(value) else {
        return;
    };
    if std::fs::write(&tmp, json).is_err() {
        return;
    }
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    let _ = private;
    let _ = std::fs::rename(&tmp, &path);
}

pub fn load_config() -> Config {
    let mut cfg: Config = load("config.json");
    // Follow a built-in theme that has been renamed, unless a custom theme
    // has since taken the old name.
    if let Some(name) = crate::theme::current_name(&cfg.theme) {
        if cfg.custom_themes.iter().all(|t| t.name != cfg.theme) {
            cfg.theme = name.to_owned();
            save_config(&cfg);
        }
    }
    if cfg.device_id.is_empty() {
        cfg.device_id = new_device_id();
        save_config(&cfg);
    }
    cfg
}

/// The config holds the sync token, so it is written owner-readable only.
pub fn save_config(cfg: &Config) {
    save("config.json", cfg, true);
}

pub fn load_history() -> History {
    load("history.json")
}

pub fn save_history(history: &History) {
    save("history.json", history, false);
}

fn new_device_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn merge_is_idempotent_and_sums_devices() {
        let mut a = History::default();
        a.record(d(1), "a", 25);
        a.record(d(1), "a", 25);
        let mut b = History::default();
        b.record(d(1), "b", 25);
        b.record(d(2), "b", 25);

        a.merge(&b);
        a.merge(&b);
        assert_eq!(a.day(d(1)), DayStat { sessions: 3, minutes: 75 });
        assert_eq!(a.day(d(2)).sessions, 1);

        b.merge(&a);
        assert_eq!(a, b);
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let mut h = History::default();
        assert_eq!(h.streak(d(5)), 0);
        h.record(d(3), "a", 25);
        h.record(d(4), "a", 25);
        assert_eq!(h.streak(d(5)), 2);
        h.record(d(5), "a", 25);
        assert_eq!(h.streak(d(5)), 3);
        assert_eq!(h.streak(d(7)), 0);
    }
}
