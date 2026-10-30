//! The PrettyPomodoro backend: accounts, focus-history sync and the daily
//! leaderboard.
//!
//! The desktop app is the only client. It signs in (Google, Facebook or
//! email and password), pushes its local history after every focus round, and
//! reads the board for its own local date.

pub mod auth;
pub mod db;
pub mod error;
pub mod model;
pub mod oauth;
pub mod routes;
pub mod state;

use error::ApiError;
use state::{AppState, Config};
use std::time::Duration;

/// Open the database and assemble the router. Returns the state too, so a
/// caller (or a test) can reach the database directly.
pub fn build(config: Config) -> Result<(AppState, axum::Router), ApiError> {
    let pool = db::open(&config.database)?;
    let state = AppState::new(pool, config)?;
    Ok((state.clone(), routes::router(state)))
}

/// Clear out sign-ins nobody finished, once an hour.
pub fn spawn_ticket_cleanup(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            match state.db(|conn| db::purge_expired_tickets(conn)).await {
                Ok(0) => {}
                Ok(n) => tracing::debug!("cleared {n} unfinished sign-in(s)"),
                Err(e) => tracing::warn!("clearing unfinished sign-ins: {e}"),
            }
        }
    });
}
