//! Cloud sync of the focus history through a private GitHub Gist (free with any
//! GitHub account). The user supplies a personal access token with the `gist`
//! scope; the gist is found or created automatically.

use crate::config::{self, History};
use crate::core::{lock, Shared, SyncJob, SyncState};
use eframe::egui;
use serde_json::{json, Value};

const API: &str = "https://api.github.com";
const FILE: &str = "pomodoro-history.json";
const DESCRIPTION: &str = "Pomodoro focus history (synced by the Pomodoro app)";

/// Run a job in the background unless one is already in flight.
///
/// Locks the shared state, so it must not be called while the caller holds
/// that lock; the UI queues jobs through `Core::sync_request` instead.
pub fn spawn(shared: Shared, ctx: egui::Context, job: SyncJob) {
    let (token, gist_id, local) = {
        let mut core = lock(&shared);
        if core.sync_state == SyncState::Running {
            return;
        }
        if core.cfg.sync.token.trim().is_empty() {
            core.sync_state = SyncState::Failed("Add a GitHub token first.".into());
            return;
        }
        core.sync_state = SyncState::Running;
        (
            core.cfg.sync.token.trim().to_owned(),
            core.cfg.sync.gist_id.trim().to_owned(),
            core.history.clone(),
        )
    };

    std::thread::spawn(move || {
        let state = match job {
            SyncJob::Sync => match run(&token, &gist_id, &local) {
                Ok((gist_id, merged)) => {
                    let mut core = lock(&shared);
                    // Sessions may have completed while we were talking to GitHub.
                    core.history.merge(&merged);
                    config::save_history(&core.history);
                    core.cfg.sync.gist_id = gist_id;
                    let now = chrono::Local::now();
                    core.cfg.sync.last_sync = Some(now.format("%Y-%m-%d %H:%M").to_string());
                    config::save_config(&core.cfg);
                    let total = core.history.total().sessions;
                    SyncState::Done(format!("Synced · {total} sessions in total"))
                }
                Err(e) => SyncState::Failed(e),
            },
            SyncJob::FindGist => match find_gist(&token) {
                Ok(Some(id)) => {
                    let mut core = lock(&shared);
                    core.cfg.sync.gist_id = id;
                    config::save_config(&core.cfg);
                    SyncState::Done("Found your sync gist.".into())
                }
                Ok(None) => SyncState::Done("No sync gist yet — Sync Now will create one.".into()),
                Err(e) => SyncState::Failed(e),
            },
        };
        lock(&shared).sync_state = state;
        ctx.request_repaint();
    });
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
        Err(ureq::Error::Status(401, _)) => Err("GitHub rejected the token (401).".into()),
        Err(ureq::Error::Status(403, _)) => {
            Err("Access denied (403). The token needs the “gist” scope.".into())
        }
        Err(ureq::Error::Status(404, _)) => {
            Err("Gist not found (404). Check the gist ID or clear it.".into())
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
        for job in [SyncJob::Sync, SyncJob::FindGist] {
            spawn(shared.clone(), egui::Context::default(), job);
            assert!(matches!(lock(&shared).sync_state, SyncState::Failed(_)));
        }
    }

    /// Talks to api.github.com; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn bad_token_is_reported() {
        let err = run("not-a-real-token", "", &History::default()).unwrap_err();
        assert!(err.contains("401"), "{err}");
    }
}
