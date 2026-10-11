//! Cloud sync of the focus history through a private GitHub Gist (free with any
//! GitHub account). The user supplies a personal access token with the `gist`
//! scope; the gist is found or created automatically.

use crate::config::{self, History};
use crate::core::{lock, Shared, SignIn, SyncJob, SyncState};
use std::time::{Duration, Instant};
use eframe::egui;
use serde_json::{json, Value};

const API: &str = "https://api.github.com";
const FILE: &str = "pomodoro-history.json";
const DESCRIPTION: &str = "Pomodoro focus history (synced by PrettyPomodoro)";

/// Client ID of the GitHub OAuth app users sign in through. It is public by
/// design (the device flow uses no client secret). The app must have "Enable
/// Device Flow" ticked in its GitHub settings.
pub const GITHUB_CLIENT_ID: &str = "Ov23liYZ52NESUYMTBoh";

/// Run a job in the background.
///
/// Locks the shared state, so it must not be called while the caller holds
/// that lock; the UI queues jobs through `Core::sync_request` instead.
pub fn spawn(shared: Shared, ctx: egui::Context, job: SyncJob) {
    match job {
        SyncJob::Sync => spawn_sync(shared, ctx),
        SyncJob::SignIn => spawn_sign_in(shared, ctx),
    }
}

fn spawn_sync(shared: Shared, ctx: egui::Context) {
    let (token, gist_id, local, known_login) = {
        let mut core = lock(&shared);
        if core.sync_state == SyncState::Running {
            return;
        }
        if core.cfg.sync.token.trim().is_empty() {
            core.sync_state = SyncState::Failed("Sign in with GitHub first.".into());
            return;
        }
        core.sync_state = SyncState::Running;
        (
            core.cfg.sync.token.trim().to_owned(),
            core.cfg.sync.gist_id.trim().to_owned(),
            core.history.clone(),
            core.cfg.sync.login.is_some(),
        )
    };

    std::thread::spawn(move || {
        let result = run(&token, &gist_id, &local);
        // Tokens saved by older versions came without a username.
        let login = if known_login || result.is_err() { None } else { fetch_login(&token).ok() };
        let mut core = lock(&shared);
        core.sync_state = match result {
            Ok((gist_id, merged)) => {
                // Sessions may have completed while we were talking to GitHub.
                core.history.merge(&merged);
                config::save_history(&core.history);
                core.cfg.sync.gist_id = gist_id;
                if login.is_some() {
                    core.cfg.sync.login = login;
                }
                let now = chrono::Local::now();
                core.cfg.sync.last_sync = Some(now.format("%Y-%m-%d %H:%M").to_string());
                config::save_config(&core.cfg);
                let total = core.history.total().sessions;
                SyncState::Done(format!("Synced · {total} sessions in total"))
            }
            Err(e) => SyncState::Failed(e),
        };
        drop(core);
        ctx.request_repaint();
    });
}

/// "Sign in with GitHub" through the OAuth device flow: GitHub hands out a
/// short code, the user enters it in their browser, and the app polls until
/// GitHub releases an access token limited to gists.
fn spawn_sign_in(shared: Shared, ctx: egui::Context) {
    let attempt = {
        let mut core = lock(&shared);
        if matches!(core.sign_in, SignIn::Starting | SignIn::Waiting { .. }) {
            return;
        }
        core.sign_in = SignIn::Starting;
        core.sign_in_attempt += 1;
        core.sign_in_attempt
    };

    std::thread::spawn(move || {
        let outcome = sign_in(&shared, &ctx, attempt);
        let mut core = lock(&shared);
        if core.sign_in_attempt != attempt {
            return; // cancelled; a newer state is already showing
        }
        match outcome {
            Ok((token, login)) => {
                core.cfg.sync.token = token;
                core.cfg.sync.login = login;
                core.cfg.sync.gist_id.clear();
                config::save_config(&core.cfg);
                core.sign_in = SignIn::Idle;
                core.sync_state = SyncState::Idle;
                core.sync_request = Some(SyncJob::Sync);
            }
            Err(e) => core.sign_in = SignIn::Failed(e),
        }
        drop(core);
        ctx.request_repaint();
    });
}

fn sign_in(shared: &Shared, ctx: &egui::Context, attempt: u64) -> Result<(String, Option<String>), String> {
    if GITHUB_CLIENT_ID.is_empty() {
        return Err("Sign-in is not set up in this build yet.".into());
    }
    let start = github_form(
        "https://github.com/login/device/code",
        &[("client_id", GITHUB_CLIENT_ID), ("scope", "gist")],
    )?;
    let field = |name: &str| start[name].as_str().map(str::to_owned);
    let (Some(device_code), Some(code), Some(url)) =
        (field("device_code"), field("user_code"), field("verification_uri"))
    else {
        return Err(oauth_error(&start));
    };
    let mut interval = start["interval"].as_u64().unwrap_or(5);
    let deadline = Instant::now() + Duration::from_secs(start["expires_in"].as_u64().unwrap_or(900));

    {
        let mut core = lock(shared);
        if core.sign_in_attempt != attempt {
            return Err("Cancelled.".into());
        }
        core.sign_in = SignIn::Waiting { code: code.clone(), url: url.clone() };
    }
    // Put the code on the clipboard and open GitHub, so the user only pastes.
    ctx.copy_text(code);
    ctx.open_url(egui::OpenUrl::new_tab(url));
    ctx.request_repaint();

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(interval));
        if lock(shared).sign_in_attempt != attempt {
            return Err("Cancelled.".into());
        }
        let reply = github_form(
            "https://github.com/login/oauth/access_token",
            &[
                ("client_id", GITHUB_CLIENT_ID),
                ("device_code", &device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        if let Some(token) = reply["access_token"].as_str() {
            return Ok((token.to_owned(), fetch_login(token).ok()));
        }
        match reply["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            _ => return Err(oauth_error(&reply)),
        }
    }
    Err("The code expired. Try signing in again.".into())
}

fn oauth_error(reply: &Value) -> String {
    match reply["error"].as_str() {
        Some("access_denied") => "Sign-in was cancelled on GitHub.".into(),
        Some("expired_token") => "The code expired. Try signing in again.".into(),
        Some("device_flow_disabled") => "Device flow is not enabled for this GitHub app.".into(),
        Some("incorrect_client_credentials") => "GitHub does not recognise this app's client ID.".into(),
        Some(other) => format!("GitHub refused the sign-in ({other})."),
        None => "Unexpected reply from GitHub.".into(),
    }
}

/// POST a form to github.com and read the JSON reply.
fn github_form(url: &str, form: &[(&str, &str)]) -> Result<Value, String> {
    let reply = ureq::post(url)
        .timeout(Duration::from_secs(20))
        .set("Accept", "application/json")
        .set("User-Agent", "pomodoro-rs")
        .send_form(form);
    match reply {
        Ok(resp) => resp.into_json().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(404, _)) => Err("GitHub does not recognise this app's client ID.".into()),
        Err(ureq::Error::Status(code, _)) => Err(format!("GitHub returned HTTP {code}.")),
        Err(e) => Err(format!("Network error: {e}")),
    }
}

fn fetch_login(token: &str) -> Result<String, String> {
    let user = send("GET", &format!("{API}/user"), token, None)?;
    user["login"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "GitHub did not return a username.".to_owned())
}

fn run(token: &str, gist_id: &str, local: &History) -> Result<(String, History), String> {
    let gist_id = match gist_id {
        "" => find_gist(token)?,
        id => Some(id.to_owned()),
    };

    let mut merged = local.clone();
    let Some(id) = gist_id else {
        let id = create_gist(token, &merged)?;
        return Ok((id, merged));
    };

    let remote = fetch(token, &id)?;
    if let Some(remote) = &remote {
        merged.merge(remote);
    }
    if remote.as_ref() != Some(&merged) {
        let body = json!({ "files": { FILE: { "content": to_content(&merged)? } } });
        send("PATCH", &format!("{API}/gists/{id}"), token, Some(body))?;
    }
    Ok((id, merged))
}

fn to_content(history: &History) -> Result<String, String> {
    serde_json::to_string_pretty(history).map_err(|e| e.to_string())
}

/// Look for a gist created by another device with the same token.
fn find_gist(token: &str) -> Result<Option<String>, String> {
    let list = send("GET", &format!("{API}/gists?per_page=100"), token, None)?;
    let found = list.as_array().into_iter().flatten().find_map(|gist| {
        gist["files"].get(FILE)?;
        Some(gist["id"].as_str()?.to_owned())
    });
    Ok(found)
}

fn create_gist(token: &str, history: &History) -> Result<String, String> {
    let body = json!({
        "description": DESCRIPTION,
        "public": false,
        "files": { FILE: { "content": to_content(history)? } },
    });
    let created = send("POST", &format!("{API}/gists"), token, Some(body))?;
    created["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "GitHub did not return a gist id.".to_owned())
}

fn fetch(token: &str, id: &str) -> Result<Option<History>, String> {
    let gist = send("GET", &format!("{API}/gists/{id}"), token, None)?;
    let Some(file) = gist["files"].get(FILE) else {
        return Ok(None);
    };
    let content = file["content"].as_str().unwrap_or_default();
    serde_json::from_str(content)
        .map(Some)
        .map_err(|e| format!("The synced file is not valid history data: {e}"))
}

fn send(method: &str, url: &str, token: &str, body: Option<Value>) -> Result<Value, String> {
    let req = ureq::request(method, url)
        .timeout(std::time::Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "pomodoro-rs");
    let resp = match body {
        Some(body) => req.send_json(body),
        None => req.call(),
    };
    match resp {
        Ok(resp) => resp.into_json().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(401, _)) => {
            Err("GitHub no longer accepts this sign-in (401). Sign out and in again.".into())
        }
        Err(ureq::Error::Status(403, _)) => Err("GitHub denied access to gists (403).".into()),
        Err(ureq::Error::Status(404, _)) => {
            Err("The sync gist was not found (404). Sign out and in again.".into())
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("GitHub returned HTTP {code}.")),
        Err(e) => Err(format!("Network error: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_token_fails_without_blocking() {
        let core = crate::core::Core::new(Default::default(), History::default(), None);
        let shared: Shared = std::sync::Arc::new(std::sync::Mutex::new(core));
        spawn(shared.clone(), egui::Context::default(), SyncJob::Sync);
        assert!(matches!(lock(&shared).sync_state, SyncState::Failed(_)));
    }

    /// Talks to api.github.com; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn unknown_client_id_is_reported() {
        let reply = github_form(
            "https://github.com/login/device/code",
            &[("client_id", "not-a-real-client-id"), ("scope", "gist")],
        );
        let message = match reply {
            Ok(json) => oauth_error(&json),
            Err(e) => e,
        };
        assert!(message.contains("client ID"), "{message}");
    }

    /// Talks to api.github.com; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn bad_token_is_reported() {
        let err = run("not-a-real-token", "", &History::default()).unwrap_err();
        assert!(err.contains("401"), "{err}");
    }
}
