//! The wire format. `History` and `GoalLog` deliberately mirror the structs the
//! desktop app keeps in `history.json` and `goals.json`, so the app can send and
//! receive its own types without a translation layer.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How a user signed in. Also the set of accepted OAuth providers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Email,
    Google,
    Facebook,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Google => "google",
            Self::Facebook => "facebook",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "email" => Some(Self::Email),
            "google" => Some(Self::Google),
            "facebook" => Some(Self::Facebook),
            _ => None,
        }
    }
}

/// Who an identity provider says signed in.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// The provider's own stable id for this person.
    pub subject: String,
    /// Only kept when the provider says it is verified.
    pub email: Option<String>,
    pub name: String,
}

/// The account, as the app shows it on the Account page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub display_name: String,
    pub email: Option<String>,
    pub provider: Provider,
    pub show_on_leaderboard: bool,
    /// Path to the profile picture, e.g. `/v1/avatars/ab12…`; `None` when the
    /// user has not set one and the app should draw their initials instead.
    pub avatar: Option<String>,
}

/// A fresh sign-in: the bearer token plus who it belongs to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user: User,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayStat {
    pub sessions: u32,
    pub minutes: u32,
}

/// Focus history: day -> device -> totals. A device only ever increases its own
/// entry, so merging two copies is a per-device max and never double counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub days: BTreeMap<NaiveDate, BTreeMap<String, DayStat>>,
}

impl History {
    pub fn day(&self, day: NaiveDate) -> DayStat {
        let mut total = DayStat::default();
        for s in self.days.get(&day).into_iter().flat_map(|d| d.values()) {
            total.sessions += s.sessions;
            total.minutes += s.minutes;
        }
        total
    }
}

/// Goals the user ticked off, by the day they were achieved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoalLog {
    pub days: BTreeMap<NaiveDate, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRequest {
    #[serde(default)]
    pub history: History,
    /// Only sent when the user opted into syncing goal texts; `None` leaves the
    /// stored goals untouched.
    #[serde(default)]
    pub goals: Option<GoalLog>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub history: History,
    pub goals: Option<GoalLog>,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

/// One row of the board. `rank` is absent for a user who is not listed because
/// they switched off `show_on_leaderboard`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub rank: Option<u32>,
    pub name: String,
    pub minutes: u32,
    pub sessions: u32,
    /// True on the row belonging to the caller.
    #[serde(default)]
    pub you: bool,
    /// Path to this player's profile picture, if they have one.
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Leaderboard {
    /// The local date the board was asked for, echoed back.
    pub day: NaiveDate,
    pub entries: Vec<Entry>,
    /// The caller's own row, always present when signed in — even when they are
    /// below the listed top, or not listed at all.
    pub you: Option<Entry>,
    /// How many accounts put minutes on the board today.
    pub players: u32,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct EmailSignUp {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct EmailLogIn {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ProfileUpdate {
    pub display_name: Option<String>,
    pub show_on_leaderboard: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartOauth {
    pub provider: Provider,
}

/// What the app needs to start an OAuth sign-in: a ticket to poll with and the
/// page to open in the browser.
#[derive(Debug, Serialize, Deserialize)]
pub struct OauthStart {
    pub ticket: String,
    pub url: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct PollOauth {
    pub ticket: String,
}

/// The reply to a poll: still waiting, or the finished session.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PollReply {
    Pending,
    Ready(Session),
    /// The user refused on the provider's page, or the ticket ran out.
    Failed { error: String },
}

/// Which sign-in buttons this deployment can actually offer.
#[derive(Debug, Serialize, Deserialize)]
pub struct Providers {
    pub email: bool,
    pub google: bool,
    pub facebook: bool,
}
