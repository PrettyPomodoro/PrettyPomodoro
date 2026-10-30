//! Everything that talks to the PrettyPomodoro backend: signing in, syncing the
//! focus history and reading the daily leaderboard.
//!
//! Each job runs on a thread of its own and writes its result back into the
//! shared state, so the UI thread never waits on the network.

use crate::config::{AccountConfig, GoalLog, History};
use crate::core::{lock, Avatar, Board, Job, Provider, Shared, SignIn, SyncState};
use eframe::egui;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

/// Where the app looks for the backend unless the user points it somewhere else
/// on the Account page (or sets `POMO_SERVER` while developing).
pub const DEFAULT_SERVER: &str = "https://api.prettypomodoro.app";

const TIMEOUT: Duration = Duration::from_secs(20);
/// Never read more than this from the server when fetching a picture.
const MAX_PICTURE_BYTES: usize = 512 * 1024;
/// How often the app asks the server whether a browser sign-in has finished.
const POLL_EVERY: Duration = Duration::from_secs(2);

/// One row of the leaderboard.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Entry {
    /// Absent for a user who has opted out of being listed, or has no minutes yet.
    pub rank: Option<u32>,
    pub name: String,
    pub minutes: u32,
    pub sessions: u32,
    /// Set on the signed-in user's own row.
    #[serde(default)]
    pub you: bool,
    /// Path to this player's picture on the server, if they have set one.
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Leaderboard {
    pub day: chrono::NaiveDate,
    pub entries: Vec<Entry>,
    /// The user's own row, even when they are below the listed top.
    pub you: Option<Entry>,
    /// How many accounts have put minutes on the board today.
    pub players: u32,
}

/// The sign-in buttons this server can actually offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Providers {
    pub email: bool,
    pub google: bool,
    pub facebook: bool,
}

impl Default for Providers {
    /// Before the server has answered, assume it offers everything, so the
    /// buttons do not appear one by one on a slow connection.
    fn default() -> Self {
        Self { email: true, google: true, facebook: true }
    }
}

/// Run a job in the background.
///
/// Locks the shared state, so it must not be called while the caller holds that
/// lock; the UI queues jobs through `Core::request` and the ticker starts them.
pub fn spawn(shared: Shared, ctx: egui::Context, job: Job) {
    match job {
        Job::Sync => sync(shared, ctx),
        Job::SignIn(provider) => sign_in(shared, ctx, provider),
        Job::SignUp { email, password, display_name } => {
            with_password(shared, ctx, "/v1/auth/email/signup", json!({
                "email": email, "password": password, "display_name": display_name
            }))
        }
        Job::LogIn { email, password } => {
            with_password(shared, ctx, "/v1/auth/email/login", json!({
                "email": email, "password": password
            }))
        }
        Job::SignOut => sign_out(shared, ctx, false),
        Job::DeleteAccount => sign_out(shared, ctx, true),
        Job::SaveProfile => save_profile(shared, ctx),
        Job::Leaderboard => leaderboard(shared, ctx),
        Job::Providers => providers(shared, ctx),
        Job::Avatar(path) => fetch_avatar(shared, ctx, path),
        Job::UploadAvatar(file) => upload_avatar(shared, ctx, file),
        Job::RemoveAvatar => remove_avatar(shared, ctx),
    }
}

// ---------------------------------------------------------------- history sync

fn sync(shared: Shared, ctx: egui::Context) {
    let (base, token, local, goals) = {
        let mut core = lock(&shared);
        if core.sync_state == SyncState::Running {
            return;
        }
        if !core.cfg.account.signed_in() {
            core.sync_state = SyncState::Failed("Sign in to sync your history.".into());
            return;
        }
        let base = match base_url(&core.cfg.account) {
            Ok(base) => base,
            Err(e) => {
                core.sync_state = SyncState::Failed(e);
                return;
            }
        };
        core.sync_state = SyncState::Running;
        (
            base,
            core.cfg.account.token.clone(),
            core.history.clone(),
            core.cfg.account.sync_goals.then(|| core.goals.clone()),
        )
    };

    std::thread::spawn(move || {
        let result = push(&base, &token, &local, goals.as_ref());
        let mut core = lock(&shared);
        // Whether it worked or not, this counts as having tried just now.
        core.synced_at = Some(std::time::Instant::now());
        match result {
            Ok((merged, goals)) => {
                // Rounds may have completed while we were talking to the server.
                core.history.merge(&merged);
                core.save_history();
                if let Some(goals) = goals {
                    core.goals.merge(&goals);
                    core.save_goals();
                }
                let now = chrono::Local::now();
                core.cfg.account.last_sync = Some(now.format("%Y-%m-%d %H:%M").to_string());
                core.save_config();
                let total = core.history.total().sessions;
                core.sync_state = SyncState::Done(format!("Synced · {total} sessions in total"));
                // Today's minutes just changed, so the board is out of date.
                core.board.stale();
            }
            Err(fail) => {
                core.sync_state = SyncState::Failed(fail.message.clone());
                if fail.signed_out {
                    forget_account(&mut core);
                }
            }
        }
        drop(core);
        ctx.request_repaint();
    });
}

fn push(
    base: &str,
    token: &str,
    local: &History,
    goals: Option<&GoalLog>,
) -> Result<(History, Option<GoalLog>), Fail> {
    let body = json!({ "history": local, "goals": goals });
    let reply = send(base, "POST", "/v1/sync", Some(token), Some(body))?;
    let history = read(&reply, "history")?;
    let goals = match reply.get("goals") {
        Some(Value::Null) | None => None,
        Some(_) => Some(read(&reply, "goals")?),
    };
    Ok((history, goals))
}

// -------------------------------------------------------------------- sign-in

/// "Sign in with Google / Facebook": the server starts the flow and hands back
/// a page to open and a ticket to poll. The browser does the rest.
fn sign_in(shared: Shared, ctx: egui::Context, provider: Provider) {
    let Some((base, attempt)) = begin_sign_in(&shared) else {
        return;
    };

    std::thread::spawn(move || {
        let outcome = browser_sign_in(&shared, &ctx, &base, provider, attempt);
        finish_sign_in(&shared, &ctx, attempt, outcome);
    });
}

/// Email and password, which need no browser.
fn with_password(shared: Shared, ctx: egui::Context, path: &'static str, body: Value) {
    let Some((base, attempt)) = begin_sign_in(&shared) else {
        return;
    };

    std::thread::spawn(move || {
        let outcome = send(&base, "POST", path, None, Some(body))
            .and_then(|reply| session(&reply))
            .map_err(|fail| fail.message);
        finish_sign_in(&shared, &ctx, attempt, outcome);
    });
}

/// Mark a sign-in as started and return the server to use, unless one is already
/// under way.
fn begin_sign_in(shared: &Shared) -> Option<(String, u64)> {
    let mut core = lock(shared);
    if matches!(core.sign_in, SignIn::Working | SignIn::InBrowser { .. }) {
        return None;
    }
    let base = match base_url(&core.cfg.account) {
        Ok(base) => base,
        Err(e) => {
            core.sign_in = SignIn::Failed(e);
            return None;
        }
    };
    core.sign_in = SignIn::Working;
    core.sign_in_attempt += 1;
    Some((base, core.sign_in_attempt))
}

/// Store a finished sign-in, or show why it failed. Ignores an attempt the user
/// has since cancelled.
fn finish_sign_in(
    shared: &Shared,
    ctx: &egui::Context,
    attempt: u64,
    outcome: Result<(String, Account), String>,
) {
    let mut core = lock(shared);
    if core.sign_in_attempt != attempt {
        return; // cancelled; a newer state is already showing
    }
    match outcome {
        Ok((token, account)) => {
            let server = core.cfg.account.server.clone();
            let sync_goals = core.cfg.account.sync_goals;
            core.cfg.account = AccountConfig {
                server,
                token,
                display_name: account.display_name,
                avatar: account.avatar,
                email: account.email,
                provider: Some(account.provider),
                show_on_leaderboard: account.show_on_leaderboard,
                last_sync: None,
                sync_goals,
            };
            core.save_config();
            core.sign_in = SignIn::Idle;
            core.sync_state = SyncState::Idle;
            // A new account means a new board; the page reopens it on the next frame.
            core.board = Board::default();
            core.request(Job::Sync);
        }
        Err(e) => core.sign_in = SignIn::Failed(e),
    }
    drop(core);
    ctx.request_repaint();
}

fn browser_sign_in(
    shared: &Shared,
    ctx: &egui::Context,
    base: &str,
    provider: Provider,
    attempt: u64,
) -> Result<(String, Account), String> {
    let start = send(base, "POST", "/v1/auth/oauth/start", None, Some(json!({
        "provider": provider.as_str()
    })))
    .map_err(|fail| fail.message)?;
    let ticket = start["ticket"]
        .as_str()
        .ok_or("The server did not start the sign-in.")?
        .to_owned();
    let url = start["url"].as_str().ok_or("The server sent no sign-in page.")?.to_owned();
    let expires_in = start["expires_in"].as_u64().unwrap_or(600);

    {
        let mut core = lock(shared);
        if core.sign_in_attempt != attempt {
            return Err("Cancelled.".into());
        }
        core.sign_in = SignIn::InBrowser { url: url.clone() };
    }
    ctx.open_url(egui::OpenUrl::new_tab(url));
    ctx.request_repaint();

    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(POLL_EVERY);
        if lock(shared).sign_in_attempt != attempt {
            return Err("Cancelled.".into());
        }
        let reply = send(base, "POST", "/v1/auth/oauth/poll", None, Some(json!({
            "ticket": ticket
        })))
        .map_err(|fail| fail.message)?;
        match reply["status"].as_str() {
            Some("pending") => {}
            Some("ready") => return session(&reply).map_err(|fail| fail.message),
            Some("failed") => {
                let why = reply["error"].as_str().unwrap_or("The sign-in did not finish.");
                return Err(why.to_owned());
            }
            _ => return Err("Unexpected reply from the server.".into()),
        }
    }
    Err("The sign-in timed out. Try again.".into())
}

// ------------------------------------------------------------------- account

fn sign_out(shared: Shared, ctx: egui::Context, delete: bool) {
    // Forget the account here and now, whatever the server has to say about it.
    let (base, token) = {
        let mut core = lock(&shared);
        let server = base_url(&core.cfg.account).ok();
        let token = core.cfg.account.token.clone();
        forget_account(&mut core);
        (server, token)
    };
    ctx.request_repaint();

    let Some(base) = base else {
        return; // nowhere to tell; the account is gone from this computer anyway
    };
    std::thread::spawn(move || {
        let _ = if delete {
            send(&base, "DELETE", "/v1/me", Some(&token), None)
        } else {
            send(&base, "POST", "/v1/auth/signout", Some(&token), None)
        };
    });
}

/// Clear the stored account and everything on screen that belonged to it.
fn forget_account(core: &mut crate::core::Core) {
    core.cfg.account.sign_out();
    core.save_config();
    core.sync_state = SyncState::Idle;
    core.sign_in = SignIn::Idle;
    core.sign_in_attempt += 1;
    core.board = Board::default();
}

/// Push the display name and leaderboard choice the user just edited.
fn save_profile(shared: Shared, ctx: egui::Context) {
    let (base, token, name, show) = {
        let core = lock(&shared);
        if !core.cfg.account.signed_in() {
            return;
        }
        match base_url(&core.cfg.account) {
            Ok(base) => (
                base,
                core.cfg.account.token.clone(),
                core.cfg.account.display_name.clone(),
                core.cfg.account.show_on_leaderboard,
            ),
            Err(_) => return,
        }
    };

    std::thread::spawn(move || {
        let body = json!({ "display_name": name, "show_on_leaderboard": show });
        let result = send(&base, "PATCH", "/v1/me", Some(&token), Some(body));
        let mut core = lock(&shared);
        match result {
            Ok(reply) => {
                // The server tidies the name, so take back what it stored.
                if let Ok(account) = serde_json::from_value::<Account>(reply) {
                    core.cfg.account.display_name = account.display_name;
                    core.cfg.account.show_on_leaderboard = account.show_on_leaderboard;
                    core.cfg.account.avatar = account.avatar;
                    core.save_config();
                }
                core.board.stale();
            }
            Err(fail) => {
                core.sync_state = SyncState::Failed(fail.message.clone());
                if fail.signed_out {
                    forget_account(&mut core);
                }
            }
        }
        drop(core);
        ctx.request_repaint();
    });
}

fn providers(shared: Shared, ctx: egui::Context) {
    let Ok(base) = base_url(&lock(&shared).cfg.account) else {
        return;
    };
    std::thread::spawn(move || {
        if let Ok(reply) = send(&base, "GET", "/v1/auth/providers", None, None) {
            if let Ok(providers) = serde_json::from_value::<Providers>(reply) {
                lock(&shared).providers = Some(providers);
                ctx.request_repaint();
            }
        }
    });
}

// ------------------------------------------------------------ profile pictures

/// Fetch one picture and turn it into a texture the pages can draw.
fn fetch_avatar(shared: Shared, ctx: egui::Context, path: String) {
    let Ok(base) = base_url(&lock(&shared).cfg.account) else {
        lock(&shared).avatars.insert(path, Avatar::Missing);
        return;
    };

    std::thread::spawn(move || {
        let loaded = bytes(&base, &path).and_then(|png| {
            crate::avatar::round(&png).map_err(Fail::from)
        });
        let state = match loaded {
            // Loading the texture is safe from here: the context locks its own.
            Ok(image) => Avatar::Ready(ctx.load_texture(
                format!("avatar{path}"),
                image,
                egui::TextureOptions::LINEAR,
            )),
            Err(_) => Avatar::Missing,
        };
        lock(&shared).avatars.insert(path, state);
        ctx.request_repaint();
    });
}

/// Shrink the file the user picked and make it their profile picture.
fn upload_avatar(shared: Shared, ctx: egui::Context, file: std::path::PathBuf) {
    let Some((base, token)) = signed_in_at(&shared) else {
        return;
    };
    lock(&shared).profile_error = None;

    std::thread::spawn(move || {
        let result = crate::avatar::prepare(&file)
            .map_err(Fail::from)
            .and_then(|png| send_png(&base, &token, png));
        finish_profile_change(&shared, &ctx, result);
    });
}

fn remove_avatar(shared: Shared, ctx: egui::Context) {
    let Some((base, token)) = signed_in_at(&shared) else {
        return;
    };
    std::thread::spawn(move || {
        let result = send(&base, "DELETE", "/v1/me/avatar", Some(&token), None);
        finish_profile_change(&shared, &ctx, result);
    });
}

/// Store the account the server sends back after a picture changed.
fn finish_profile_change(shared: &Shared, ctx: &egui::Context, result: Result<Value, Fail>) {
    let mut core = lock(shared);
    match result.and_then(|reply| read::<Account>(&reply, "")) {
        Ok(account) => {
            // The old picture is at an address nobody will ask for again.
            if let Some(old) = core.cfg.account.avatar.take() {
                core.avatars.remove(&old);
            }
            core.cfg.account.avatar = account.avatar;
            core.cfg.account.display_name = account.display_name;
            core.save_config();
            core.profile_error = None;
            core.board.stale();
        }
        Err(fail) => {
            core.profile_error = Some(fail.message.clone());
            if fail.signed_out {
                forget_account(&mut core);
            }
        }
    }
    drop(core);
    ctx.request_repaint();
}

/// The server and token, if this computer is signed in to one.
fn signed_in_at(shared: &Shared) -> Option<(String, String)> {
    let core = lock(shared);
    if !core.cfg.account.signed_in() {
        return None;
    }
    let base = base_url(&core.cfg.account).ok()?;
    Some((base, core.cfg.account.token.clone()))
}

// ---------------------------------------------------------------- leaderboard

fn leaderboard(shared: Shared, ctx: egui::Context) {
    let (base, token, day) = {
        let mut core = lock(&shared);
        if core.board.loading {
            return;
        }
        let base = match base_url(&core.cfg.account) {
            Ok(base) => base,
            Err(e) => {
                core.board.failed(e);
                return;
            }
        };
        core.board.loading = true;
        let token = core.cfg.account.signed_in().then(|| core.cfg.account.token.clone());
        (base, token, chrono::Local::now().date_naive())
    };

    std::thread::spawn(move || {
        // The day is this computer's own local date: every account's minutes are
        // filed under its local day, so the board compares like with like.
        let path = format!("/v1/leaderboard?day={day}");
        let result = send(&base, "GET", &path, token.as_deref(), None);
        let mut core = lock(&shared);
        core.board.loading = false;
        core.board.fetched_at = Some(std::time::Instant::now());
        match result.and_then(|reply| read::<Leaderboard>(&reply, "")) {
            Ok(board) => {
                core.board.data = Some(board);
                core.board.error = None;
            }
            Err(fail) => {
                core.board.failed(fail.message.clone());
                if fail.signed_out {
                    forget_account(&mut core);
                }
            }
        }
        drop(core);
        ctx.request_repaint();
    });
}

// ----------------------------------------------------------------- HTTP plumbing

/// The account as the server describes it.
#[derive(Debug, Deserialize)]
struct Account {
    display_name: String,
    email: Option<String>,
    provider: String,
    show_on_leaderboard: bool,
    #[serde(default)]
    avatar: Option<String>,
}

/// A failed call. `signed_out` means the token is no longer any good.
#[derive(Debug)]
struct Fail {
    message: String,
    signed_out: bool,
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { message, signed_out: false }
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Self {
        Self { message: message.to_owned(), signed_out: false }
    }
}

/// The token and account out of a sign-in reply.
fn session(reply: &Value) -> Result<(String, Account), Fail> {
    let token = reply["token"].as_str().ok_or("The server sent no sign-in token.")?.to_owned();
    Ok((token, read(reply, "user")?))
}

/// Parse `field` of a reply, or the whole reply when `field` is empty.
fn read<T: serde::de::DeserializeOwned>(reply: &Value, field: &str) -> Result<T, Fail> {
    let value = if field.is_empty() { reply } else { &reply[field] };
    serde_json::from_value(value.clone())
        .map_err(|e| Fail::from(format!("The server sent data the app could not read: {e}")))
}

/// Where to send requests: the configured server, or `POMO_SERVER` while developing.
fn base_url(account: &AccountConfig) -> Result<String, String> {
    let configured = std::env::var("POMO_SERVER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| account.server.clone());
    let base = configured.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Set the server address on the Account page.".into());
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("The server address must start with http:// or https://".into());
    }
    Ok(base.to_owned())
}

fn send(
    base: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Result<Value, Fail> {
    let mut req = ureq::request(method, &format!("{base}{path}"))
        .timeout(TIMEOUT)
        .set("Accept", "application/json")
        .set("User-Agent", concat!("PrettyPomodoro/", env!("CARGO_PKG_VERSION")));
    if let Some(token) = token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    let reply = match body {
        Some(body) => req.send_json(body),
        None => req.call(),
    };
    match reply {
        Ok(resp) => parse(resp),
        Err(ureq::Error::Status(code, resp)) => Err(status_error(code, resp)),
        Err(e) => Err(Fail::from(format!("Could not reach the server: {e}"))),
    }
}

/// Turn a refusal into something worth showing the user. The server explains
/// itself in the body; the status is the fallback.
fn status_error(code: u16, resp: ureq::Response) -> Fail {
    let message = parse(resp)
        .ok()
        .and_then(|body| body["error"].as_str().map(str::to_owned))
        .unwrap_or_else(|| match code {
            401 => "You were signed out. Sign in again.".to_owned(),
            404 => "This server does not look like a PrettyPomodoro backend.".to_owned(),
            413 => "That picture is too big.".to_owned(),
            429 => "The server is busy. Try again in a moment.".to_owned(),
            500..=599 => format!("The server had a problem (HTTP {code})."),
            _ => format!("The server returned HTTP {code}."),
        });
    Fail { message, signed_out: code == 401 }
}

/// Fetch raw bytes, for a picture rather than JSON.
fn bytes(base: &str, path: &str) -> Result<Vec<u8>, Fail> {
    let reply = ureq::get(&format!("{base}{path}"))
        .timeout(TIMEOUT)
        .set("User-Agent", concat!("PrettyPomodoro/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| Fail::from(format!("Could not fetch the picture: {e}")))?;
    let mut buffer = Vec::new();
    reply
        .into_reader()
        // However odd the reply, never read an unbounded amount into memory.
        .take(MAX_PICTURE_BYTES as u64)
        .read_to_end(&mut buffer)
        .map_err(|e| Fail::from(format!("Could not read the picture: {e}")))?;
    Ok(buffer)
}

/// PUT a PNG as the signed-in user's profile picture.
fn send_png(base: &str, token: &str, png: Vec<u8>) -> Result<Value, Fail> {
    let reply = ureq::put(&format!("{base}/v1/me/avatar"))
        .timeout(TIMEOUT)
        .set("Content-Type", "image/png")
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", concat!("PrettyPomodoro/", env!("CARGO_PKG_VERSION")))
        .send_bytes(&png);
    match reply {
        Ok(resp) => parse(resp),
        Err(ureq::Error::Status(code, resp)) => Err(status_error(code, resp)),
        Err(e) => Err(Fail::from(format!("Could not reach the server: {e}"))),
    }
}

/// Read a reply as JSON, treating an empty body (such as 204) as null.
fn parse(resp: ureq::Response) -> Result<Value, Fail> {
    let text = resp
        .into_string()
        .map_err(|e| Fail::from(format!("Could not read the server's reply: {e}")))?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text)
        .map_err(|_| Fail::from("The server's reply was not JSON.".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DayStat;

    fn account(server: &str) -> AccountConfig {
        AccountConfig { server: server.to_owned(), ..Default::default() }
    }

    #[test]
    fn server_addresses_are_tidied_and_checked() {
        assert_eq!(base_url(&account("https://api.example.com/")).unwrap(), "https://api.example.com");
        assert_eq!(base_url(&account(" http://127.0.0.1:8787 ")).unwrap(), "http://127.0.0.1:8787");
        assert!(base_url(&account("")).is_err());
        assert!(base_url(&account("api.example.com")).is_err(), "the scheme is required");
    }

    #[test]
    fn a_sync_reply_is_read_back_into_the_local_types() {
        let reply = json!({
            "history": { "days": { "2026-10-30": { "laptop": { "sessions": 2, "minutes": 50 } } } },
            "goals": { "days": { "2026-10-30": ["Write the report"] } },
            "synced_at": "2026-10-30T01:02:03Z"
        });
        let history: History = read(&reply, "history").unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        assert_eq!(history.day(day), DayStat { sessions: 2, minutes: 50 });
        let goals: GoalLog = read(&reply, "goals").unwrap();
        assert_eq!(goals.days[&day], ["Write the report"]);
    }

    #[test]
    fn a_board_is_read_whole_and_tolerates_extra_fields() {
        let reply = json!({
            "day": "2026-10-30",
            "entries": [
                { "rank": 1, "name": "Bo", "minutes": 100, "sessions": 4 },
                { "rank": 2, "name": "Ada", "minutes": 50, "sessions": 2, "you": true }
            ],
            "you": { "rank": 2, "name": "Ada", "minutes": 50, "sessions": 2, "you": true },
            "players": 2,
            "generated_at": "2026-10-30T01:02:03Z"
        });
        let board: Leaderboard = read(&reply, "").unwrap();
        assert_eq!(board.players, 2);
        assert_eq!(board.entries[0].name, "Bo");
        assert!(board.entries[1].you && !board.entries[0].you);
        assert_eq!(board.you.unwrap().rank, Some(2));
    }

    #[test]
    fn an_unlisted_user_has_no_rank() {
        let reply = json!({
            "day": "2026-10-30", "entries": [], "players": 0,
            "you": { "rank": null, "name": "Ada", "minutes": 75, "sessions": 3, "you": true }
        });
        let board: Leaderboard = read(&reply, "").unwrap();
        let you = board.you.unwrap();
        assert_eq!((you.rank, you.minutes), (None, 75));
    }

    #[test]
    fn the_sign_in_reply_yields_a_token_and_the_account() {
        let reply = json!({
            "token": "a-token",
            "user": {
                "display_name": "Ada", "email": "ada@example.com",
                "provider": "email", "show_on_leaderboard": true
            }
        });
        let (token, account) = session(&reply).unwrap();
        assert_eq!(token, "a-token");
        assert_eq!(account.display_name, "Ada");
        assert_eq!(account.provider, "email");
        assert!(session(&json!({ "user": {} })).is_err());
    }

    #[test]
    fn syncing_without_an_account_fails_without_blocking() {
        let core = crate::core::Core::new(Default::default(), History::default(), None);
        let shared: Shared = std::sync::Arc::new(std::sync::Mutex::new(core));
        spawn(shared.clone(), egui::Context::default(), Job::Sync);
        assert!(matches!(lock(&shared).sync_state, SyncState::Failed(_)));
    }

    /// Talks to a real backend, so it is off by default. Start one with
    /// `cargo run -p prettypomodoro-server` and run this with
    /// `cargo test -- --ignored`; `POMO_SERVER` picks a different one.
    #[test]
    #[ignore]
    fn a_round_trip_against_a_running_server() {
        let base = base_url(&account(DEFAULT_SERVER)).unwrap();
        let providers: Providers =
            read(&send(&base, "GET", "/v1/auth/providers", None, None).unwrap(), "").unwrap();
        assert!(providers.email, "the server offers email sign-in");

        // A fresh account, so repeated runs do not collide.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let reply = send(&base, "POST", "/v1/auth/email/signup", None, Some(json!({
            "email": format!("round-trip-{stamp}@example.com"),
            "password": "a-good-password",
            "display_name": "Round Trip",
        })))
        .unwrap();
        let (token, account) = session(&reply).unwrap();
        assert_eq!(account.display_name, "Round Trip");

        // Two focus rounds, reported twice: the totals must not double.
        let today = chrono::Local::now().date_naive();
        let mut history = History::default();
        history.record(today, "round-trip", 25);
        history.record(today, "round-trip", 25);
        let (merged, goals) = push(&base, &token, &history, None).unwrap();
        assert_eq!(merged.day(today), DayStat { sessions: 2, minutes: 50 });
        assert!(goals.is_none(), "goals stay put unless they are sent");
        let (merged, _) = push(&base, &token, &history, None).unwrap();
        assert_eq!(merged.day(today), DayStat { sessions: 2, minutes: 50 });

        // The board knows about it, and marks the row as ours.
        let path = format!("/v1/leaderboard?day={today}");
        let board: Leaderboard =
            read(&send(&base, "GET", &path, Some(&token), None).unwrap(), "").unwrap();
        assert_eq!(board.day, today);
        let you = board.you.expect("own row");
        assert_eq!(you.minutes, 50);
        assert!(you.rank.is_some(), "a new account is listed by default");
        assert!(board.entries.iter().any(|e| e.you && e.name == "Round Trip"));

        // Tidy up after ourselves.
        send(&base, "DELETE", "/v1/me", Some(&token), None).unwrap();
        let after = send(&base, "GET", "/v1/me", Some(&token), None);
        assert!(after.is_err_and(|f| f.signed_out), "the token dies with the account");
    }

    #[test]
    fn signing_out_forgets_the_account_but_keeps_this_computers_choices() {
        let mut cfg = crate::config::Config::default();
        cfg.account.token = "a-token".into();
        cfg.account.display_name = "Ada".into();
        cfg.account.server = "http://127.0.0.1:8787".into();
        cfg.account.sync_goals = true;
        let mut core = crate::core::Core::new(cfg, History::default(), None);
        core.board.open = true;

        forget_account(&mut core);
        assert!(!core.cfg.account.signed_in());
        assert!(core.cfg.account.display_name.is_empty());
        assert_eq!(core.cfg.account.server, "http://127.0.0.1:8787");
        assert!(core.cfg.account.sync_goals, "the goal-sync choice is this computer's");
        assert!(core.board.data.is_none(), "the old account's board goes with it");
    }
}
