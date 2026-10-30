//! Every endpoint the desktop app talks to.

use crate::auth::{self, Auth, MaybeAuth};
use crate::db;
use crate::error::ApiError;
use crate::model::*;
use crate::oauth;
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;

/// How long a browser sign-in may stay unfinished.
const TICKET_MINUTES: i64 = 10;
/// A day holds no more focus than this, so anything above it is a broken or
/// doctored client rather than a very good day.
const MAX_MINUTES_PER_DAY: u32 = 24 * 60;
const MAX_SESSIONS_PER_DAY: u32 = 288;
/// Enough for decades of history; keeps one request's work bounded.
const MAX_HISTORY_ENTRIES: usize = 20_000;
/// A profile picture is a small square; the app sends 128x128 PNG, which is a
/// few kilobytes. The ceiling leaves room without inviting uploads of photos.
const MAX_AVATAR_BYTES: usize = 256 * 1024;
const MAX_AVATAR_SIDE: u32 = 512;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/auth/providers", get(providers))
        .route("/v1/auth/email/signup", post(email_signup))
        .route("/v1/auth/email/login", post(email_login))
        .route("/v1/auth/oauth/start", post(oauth_start))
        .route("/v1/auth/oauth/poll", post(oauth_poll))
        .route("/v1/auth/oauth/callback/{provider}", get(oauth_callback))
        .route("/v1/auth/signout", post(sign_out))
        .route("/v1/me", get(me).patch(update_me).delete(delete_me))
        .route(
            "/v1/me/avatar",
            // A picture arrives as raw PNG bytes rather than JSON, so it is not
            // base64-inflated on the way.
            put(set_avatar).delete(remove_avatar).layer(DefaultBodyLimit::max(MAX_AVATAR_BYTES)),
        )
        .route("/v1/avatars/{avatar_id}", get(avatar))
        .route(
            "/v1/sync",
            // Years of history in one body is still only a few hundred kilobytes,
            // but the default two megabytes is uncomfortably close.
            post(sync).layer(DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route("/v1/leaderboard", get(leaderboard))
        .layer(axum::middleware::from_fn(log_request))
        .with_state(state)
}

/// One line per request, at debug level: `PP_LOG=debug` turns it on.
async fn log_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let (method, path) = (request.method().clone(), request.uri().path().to_owned());
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    tracing::debug!(
        "{method} {path} -> {} in {}ms",
        response.status().as_u16(),
        started.elapsed().as_millis()
    );
    response
}

async fn providers(State(state): State<AppState>) -> Json<Providers> {
    Json(state.config.providers())
}

async fn email_signup(
    State(state): State<AppState>,
    Json(body): Json<EmailSignUp>,
) -> Result<Json<Session>, ApiError> {
    let email = auth::clean_email(&body.email)?;
    auth::check_password(&body.password)?;
    let display_name = auth::clean_display_name(&body.display_name)?;

    let password = body.password.clone();
    let hash = state.blocking(move || auth::hash_password(&password)).await?;
    let (email_for_db, name_for_db) = (email.clone(), display_name.clone());
    let user_id = state
        .db(move |conn| db::create_email_user(conn, &email_for_db, &hash, &name_for_db))
        .await?;

    let token = auth::start_session(&state, user_id).await?;
    Ok(Json(Session {
        token,
        user: User {
            display_name,
            email: Some(email),
            provider: Provider::Email,
            show_on_leaderboard: true,
            avatar: None,
        },
    }))
}

async fn email_login(
    State(state): State<AppState>,
    Json(body): Json<EmailLogIn>,
) -> Result<Json<Session>, ApiError> {
    let email = auth::clean_email(&body.email)?;
    let lookup = email.clone();
    let found = state.db(move |conn| db::email_account(conn, &lookup)).await?;

    let Some((user_id, stored)) = found else {
        // Point someone who signed up through a provider at the right button,
        // rather than letting them retype a password they never set.
        let lookup = email.clone();
        let through = state.db(move |conn| db::oauth_providers_for_email(conn, &lookup)).await?;
        return Err(ApiError::Unauthorized(match through.first().map(String::as_str) {
            Some("google") => "That email signs in with Google. Use the Google button.".into(),
            Some("facebook") => "That email signs in with Facebook. Use the Facebook button.".into(),
            _ => "No account for that email and password.".to_owned(),
        }));
    };

    let password = body.password.clone();
    let ok = state.blocking(move || Ok(auth::verify_password(&password, &stored))).await?;
    if !ok {
        return Err(ApiError::Unauthorized("No account for that email and password.".into()));
    }
    let user = state
        .db(move |conn| db::user(conn, user_id))
        .await?
        .ok_or_else(|| ApiError::Internal("the account vanished mid-login".into()))?;
    let token = auth::start_session(&state, user_id).await?;
    Ok(Json(Session { token, user }))
}

/// Begin a browser sign-in: store a ticket and hand back the page to open.
async fn oauth_start(
    State(state): State<AppState>,
    Json(body): Json<StartOauth>,
) -> Result<Json<OauthStart>, ApiError> {
    if body.provider == Provider::Email {
        return Err(ApiError::BadRequest("Email sign-in does not use the browser.".into()));
    }
    // Fail before writing anything if this server has no credentials.
    state.config.oauth(body.provider)?;

    let (ticket, csrf, verifier) = (auth::secret()?, auth::secret()?, auth::secret()?);
    let challenge = auth::pkce_challenge(&verifier);
    let url = oauth::authorize_url(&state, body.provider, &csrf, &challenge)?;

    let hash = auth::sha256(&ticket);
    let expires_at = Utc::now() + chrono::Duration::minutes(TICKET_MINUTES);
    let provider = body.provider;
    state
        .db(move |conn| db::insert_ticket(conn, &hash, provider, &csrf, &verifier, expires_at))
        .await?;

    Ok(Json(OauthStart { ticket, url, expires_in: TICKET_MINUTES as u64 * 60 }))
}

#[derive(Debug, Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Where the provider sends the browser back. Finishes the sign-in and leaves
/// the session token on the ticket for the app to collect.
async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<Callback>,
) -> Html<String> {
    match finish_oauth(&state, &provider, query).await {
        Ok(()) => page("You're signed in", "Go back to PrettyPomodoro — it is already waiting."),
        Err(e) => {
            if let ApiError::Internal(detail) = &e {
                tracing::error!("sign-in callback: {detail}");
            }
            page("That didn't work", &e.to_string())
        }
    }
}

async fn finish_oauth(state: &AppState, provider: &str, mut query: Callback) -> Result<(), ApiError> {
    let provider = Provider::parse(provider)
        .filter(|p| *p != Provider::Email)
        .ok_or_else(|| ApiError::NotFound("Unknown sign-in provider.".into()))?;
    let csrf = query
        .state
        .take()
        .ok_or_else(|| ApiError::BadRequest("That sign-in link is incomplete.".into()))?;

    let lookup = csrf.clone();
    let ticket = state
        .db(move |conn| db::ticket_by_state(conn, &lookup))
        .await?
        .filter(|t| t.provider == provider)
        .ok_or_else(|| {
            ApiError::NotFound("That sign-in has expired. Start again in the app.".into())
        })?;

    // From here on, failures are recorded on the ticket so the app stops waiting.
    let outcome = sign_in_through(state, provider, &ticket, query).await;
    let stored: Result<String, String> = match &outcome {
        Ok(token) => Ok(token.clone()),
        Err(e) => Err(e.to_string()),
    };
    let hash = ticket.ticket_hash.clone();
    state
        .db(move |conn| {
            db::resolve_ticket(conn, &hash, stored.as_deref().map_err(String::as_str))
        })
        .await?;
    outcome.map(|_| ())
}

async fn sign_in_through(
    state: &AppState,
    provider: Provider,
    ticket: &db::Ticket,
    query: Callback,
) -> Result<String, ApiError> {
    if let Some(error) = query.error {
        let detail = query.error_description.unwrap_or(error);
        return Err(ApiError::BadRequest(match detail.as_str() {
            "access_denied" => "The sign-in was cancelled.".to_owned(),
            _ => format!("The provider refused the sign-in ({detail})."),
        }));
    }
    let code = query
        .code
        .ok_or_else(|| ApiError::BadRequest("The provider sent no authorization code.".into()))?;

    let profile = oauth::exchange(state, provider, &code, &ticket.pkce_verifier).await?;
    let name = auth::clean_display_name(&profile.name)
        .unwrap_or_else(|_| "Pomodoro user".to_owned());
    let email = profile.email.as_deref().and_then(|e| auth::clean_email(e).ok());
    let subject = profile.subject.clone();
    let user_id = state
        .db(move |conn| db::upsert_oauth_user(conn, provider, &subject, email.as_deref(), &name))
        .await?;
    auth::start_session(state, user_id).await
}

/// Collect the finished sign-in. A ready ticket is spent by this call.
async fn oauth_poll(
    State(state): State<AppState>,
    Json(body): Json<PollOauth>,
) -> Result<Json<PollReply>, ApiError> {
    let hash = auth::sha256(&body.ticket);
    let claim = state.db(move |conn| db::claim_ticket(conn, &hash)).await?;
    Ok(Json(match claim {
        db::Claim::Pending => PollReply::Pending,
        db::Claim::Failed(error) => PollReply::Failed { error },
        db::Claim::Expired => {
            PollReply::Failed { error: "That sign-in expired. Try again.".to_owned() }
        }
        db::Claim::Ready(token) => {
            let hash = auth::sha256(&token);
            let found = state.db(move |conn| db::session_user(conn, &hash)).await?;
            match found {
                Some((_, user)) => PollReply::Ready(Session { token, user }),
                None => PollReply::Failed { error: "That sign-in expired. Try again.".to_owned() },
            }
        }
    }))
}

async fn sign_out(State(state): State<AppState>, auth: Auth) -> Result<StatusCode, ApiError> {
    state.db(move |conn| db::delete_session(conn, &auth.token_hash)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(auth: Auth) -> Json<User> {
    Json(auth.user)
}

async fn update_me(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<ProfileUpdate>,
) -> Result<Json<User>, ApiError> {
    let name = match &body.display_name {
        Some(name) => Some(auth::clean_display_name(name)?),
        None => None,
    };
    let show = body.show_on_leaderboard;
    let id = auth.user_id;
    let user = state
        .db(move |conn| {
            db::update_profile(conn, id, name.as_deref(), show)?;
            db::user(conn, id)
        })
        .await?
        .ok_or_else(|| ApiError::Internal("the account vanished mid-update".into()))?;
    Ok(Json(user))
}

async fn delete_me(State(state): State<AppState>, auth: Auth) -> Result<StatusCode, ApiError> {
    state.db(move |conn| db::delete_user(conn, auth.user_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Store a new profile picture. The body is the PNG itself.
async fn set_avatar(
    State(state): State<AppState>,
    auth: Auth,
    body: Bytes,
) -> Result<Json<User>, ApiError> {
    let (width, height) = png_size(&body).ok_or_else(|| {
        ApiError::BadRequest("A profile picture has to be a PNG image.".into())
    })?;
    if width > MAX_AVATAR_SIDE || height > MAX_AVATAR_SIDE {
        return Err(ApiError::BadRequest(format!(
            "That picture is {width}x{height}; {MAX_AVATAR_SIDE} a side is the most."
        )));
    }
    // A fresh id on every upload, so caches never serve the old face.
    let avatar_id = auth::secret()?;
    let id = auth.user_id;
    let user = state
        .db(move |conn| {
            db::set_avatar(conn, id, Some((&body, &avatar_id)))?;
            db::user(conn, id)
        })
        .await?
        .ok_or_else(|| ApiError::Internal("the account vanished mid-upload".into()))?;
    Ok(Json(user))
}

async fn remove_avatar(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<User>, ApiError> {
    let id = auth.user_id;
    let user = state
        .db(move |conn| {
            db::set_avatar(conn, id, None)?;
            db::user(conn, id)
        })
        .await?
        .ok_or_else(|| ApiError::Internal("the account vanished mid-update".into()))?;
    Ok(Json(user))
}

/// Serve a picture. The id changes whenever the picture does, so this can be
/// cached forever.
async fn avatar(
    State(state): State<AppState>,
    Path(avatar_id): Path<String>,
) -> Result<Response, ApiError> {
    let found = state.db(move |conn| db::avatar(conn, &avatar_id)).await?;
    let bytes = found.ok_or_else(|| ApiError::NotFound("No such picture.".into()))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            // The bytes came from a client, so never let a browser guess at them.
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

/// Width and height from a PNG header, or `None` if this is not a PNG.
///
/// Deliberately not a full decode: the server never renders these, it only
/// stores and serves them, and refusing anything that is not a plausible PNG
/// keeps an image library out of the request path.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    // Signature, then the IHDR chunk: length 13, type, width, height.
    if bytes.len() < 24 || !bytes.starts_with(SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let number =
        |at: usize| -> Option<u32> { Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
    let (width, height) = (number(16)?, number(20)?);
    (width > 0 && height > 0).then_some((width, height))
}

/// Merge what the app has into what is stored, and send back the union. Running
/// it twice changes nothing, so the app may sync as often as it likes.
async fn sync(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<SyncRequest>,
) -> Result<Json<SyncResponse>, ApiError> {
    check_history(&body.history)?;
    let id = auth.user_id;
    let (history, goals) = state
        .db(move |conn| {
            db::merge_history(conn, id, &body.history)?;
            let goals = match &body.goals {
                Some(goals) => {
                    db::merge_goals(conn, id, goals)?;
                    Some(db::goals(conn, id)?)
                }
                None => None,
            };
            Ok((db::history(conn, id)?, goals))
        })
        .await?;
    Ok(Json(SyncResponse { history, goals, synced_at: Utc::now() }))
}

/// Refuse a day that could not have happened. The board still trusts the app
/// for everything below this line — see the README.
fn check_history(history: &History) -> Result<(), ApiError> {
    let mut entries = 0;
    for (day, devices) in &history.days {
        for stat in devices.values() {
            entries += 1;
            if stat.minutes > MAX_MINUTES_PER_DAY || stat.sessions > MAX_SESSIONS_PER_DAY {
                return Err(ApiError::BadRequest(format!(
                    "{day} reports more focus than a day holds."
                )));
            }
        }
        if entries > MAX_HISTORY_ENTRIES {
            return Err(ApiError::BadRequest("That is too much history for one request.".into()));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct BoardQuery {
    /// The caller's own local date. Everyone's rows are keyed by their local
    /// day, so one date compares like with like across time zones.
    day: Option<NaiveDate>,
    limit: Option<u32>,
}

async fn leaderboard(
    State(state): State<AppState>,
    MaybeAuth(auth): MaybeAuth,
    Query(query): Query<BoardQuery>,
) -> Result<Json<Leaderboard>, ApiError> {
    let day = query.day.unwrap_or_else(|| Utc::now().date_naive());
    let limit = query
        .limit
        .unwrap_or(state.config.leaderboard_limit)
        .clamp(1, state.config.leaderboard_limit);
    let me = auth.as_ref().map(|a| a.user_id);
    let (entries, you, players) =
        state.db(move |conn| db::leaderboard(conn, day, limit, me)).await?;
    Ok(Json(Leaderboard { day, entries, you, players, generated_at: Utc::now() }))
}

/// The one page a browser ever sees from this server.
fn page(title: &str, message: &str) -> Html<String> {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · PrettyPomodoro</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ margin: 0; min-height: 100vh; display: grid; place-items: center;
         font: 16px/1.5 -apple-system, "Segoe UI", Inter, system-ui, sans-serif;
         background: #faf7f2; color: #2b2a28; }}
  @media (prefers-color-scheme: dark) {{ body {{ background: #1c1b19; color: #ece8e1; }} }}
  main {{ max-width: 24rem; padding: 2rem; text-align: center; }}
  .tomato {{ font-size: 3rem; }}
  h1 {{ font-size: 1.35rem; margin: 0.6rem 0 0.4rem; font-weight: 600; }}
  p {{ margin: 0; opacity: 0.75; }}
</style></head>
<body><main>
  <div class="tomato">🍅</div>
  <h1>{title}</h1>
  <p>{message}</p>
</main></body></html>"#,
        title = escape(title),
        message = escape(message),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_with(minutes: u32, sessions: u32) -> History {
        let mut history = History::default();
        let day = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        history
            .days
            .entry(day)
            .or_default()
            .insert("device".into(), DayStat { sessions, minutes });
        history
    }

    #[test]
    fn a_plausible_day_passes_and_an_impossible_one_does_not() {
        assert!(check_history(&history_with(1440, 288)).is_ok());
        assert!(check_history(&history_with(1441, 1)).is_err());
        assert!(check_history(&history_with(25, 289)).is_err());
        assert!(check_history(&History::default()).is_ok());
    }

    /// The smallest valid PNG header this server will accept, with the given size.
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn a_png_header_gives_up_its_size_and_anything_else_is_refused() {
        assert_eq!(png_size(&png_header(128, 128)), Some((128, 128)));
        assert_eq!(png_size(&png_header(512, 256)), Some((512, 256)));
        assert_eq!(png_size(&png_header(0, 128)), None, "an empty image is not one");

        assert_eq!(png_size(b""), None);
        assert_eq!(png_size(b"GIF89a and then some padding bytes here"), None);
        // A real PNG signature but no IHDR where one belongs.
        let mut wrong = b"\x89PNG\r\n\x1a\n".to_vec();
        wrong.extend_from_slice(&[0; 16]);
        assert_eq!(png_size(&wrong), None);
        // Truncated right after the signature.
        assert_eq!(png_size(&png_header(128, 128)[..20]), None);
    }

    #[test]
    fn the_callback_page_escapes_what_it_is_given() {
        let Html(html) = page("ok", "<script>alert(1)</script>");
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
