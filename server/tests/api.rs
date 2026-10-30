//! End-to-end tests over real HTTP: a server on an ephemeral port, a temporary
//! database and a `reqwest` client, exercising the endpoints the app calls.
//!
//! The OAuth flows cannot complete without Google or Facebook, so what is
//! checked here is everything around them: starting, polling, the callback's
//! refusals, and that an unconfigured provider says so.

use chrono::NaiveDate;
use prettypomodoro_server::model::*;
use prettypomodoro_server::state::Config;
use reqwest::StatusCode;
use serde_json::{json, Value};

struct Server {
    base: String,
    client: reqwest::Client,
    /// Holds the temporary database directory open for the test's lifetime.
    _dir: tempfile::TempDir,
}

async fn start() -> Server {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = dir.path().join("test.db").to_string_lossy().into_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let config = Config {
        bind: addr.to_string(),
        database,
        public_url: format!("http://{addr}"),
        leaderboard_limit: 50,
        google: None,
        facebook: None,
    };
    let (_state, router) = prettypomodoro_server::build(config).expect("build");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    Server { base: format!("http://{addr}"), client: reqwest::Client::new(), _dir: dir }
}

impl Server {
    fn request(&self, method: reqwest::Method, path: &str, token: Option<&str>) -> reqwest::RequestBuilder {
        let req = self.client.request(method, format!("{}{path}", self.base));
        match token {
            Some(token) => req.bearer_auth(token),
            None => req,
        }
    }

    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> reqwest::Response {
        self.request(reqwest::Method::POST, path, token)
            .json(&body)
            .send()
            .await
            .expect("post")
    }

    async fn get(&self, path: &str, token: Option<&str>) -> reqwest::Response {
        self.request(reqwest::Method::GET, path, token).send().await.expect("get")
    }

    /// Sign up and return the session, failing the test if the server refuses.
    async fn sign_up(&self, name: &str, email: &str) -> Session {
        let reply = self
            .post(
                "/v1/auth/email/signup",
                None,
                json!({ "email": email, "password": "a-good-password", "display_name": name }),
            )
            .await;
        assert_eq!(reply.status(), StatusCode::OK, "{}", body_text(reply).await);
        reply.json().await.expect("session")
    }

    /// Report `minutes` of focus on `day` from one device.
    async fn record(&self, token: &str, day: NaiveDate, device: &str, sessions: u32, minutes: u32) -> SyncResponse {
        let reply = self
            .post(
                "/v1/sync",
                Some(token),
                json!({ "history": { "days": { day.to_string(): { device: { "sessions": sessions, "minutes": minutes } } } } }),
            )
            .await;
        assert_eq!(reply.status(), StatusCode::OK, "{}", body_text(reply).await);
        reply.json().await.expect("sync reply")
    }

    async fn board(&self, token: Option<&str>, day: NaiveDate) -> Leaderboard {
        let reply = self.get(&format!("/v1/leaderboard?day={day}"), token).await;
        assert_eq!(reply.status(), StatusCode::OK);
        reply.json().await.expect("leaderboard")
    }
}

async fn body_text(reply: reqwest::Response) -> String {
    reply.text().await.unwrap_or_else(|e| format!("<unreadable body: {e}>"))
}

/// The message the app would show the user.
async fn error_of(reply: reqwest::Response) -> String {
    let body: Value = reply.json().await.expect("error body");
    body["error"].as_str().unwrap_or_default().to_owned()
}

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, n).expect("date")
}

#[tokio::test]
async fn the_server_reports_which_sign_ins_it_offers() {
    let server = start().await;
    assert_eq!(body_text(server.get("/healthz", None).await).await, "ok");

    let providers: Providers = server.get("/v1/auth/providers", None).await.json().await.unwrap();
    assert!(providers.email, "email always works");
    assert!(!providers.google && !providers.facebook, "no credentials in the test config");
}

#[tokio::test]
async fn signing_up_syncing_and_reading_the_board() {
    let server = start().await;
    let session = server.sign_up("Ada", "ada@example.com").await;
    assert_eq!(session.user.display_name, "Ada");
    assert_eq!(session.user.provider, Provider::Email);
    assert!(session.user.show_on_leaderboard);

    let me: User = server.get("/v1/me", Some(&session.token)).await.json().await.unwrap();
    assert_eq!(me, session.user);

    // Two focus rounds on one device, then a third from another computer.
    let synced = server.record(&session.token, day(30), "laptop", 2, 50).await;
    assert_eq!(synced.history.day(day(30)), DayStat { sessions: 2, minutes: 50 });
    let synced = server.record(&session.token, day(30), "desktop", 1, 25).await;
    assert_eq!(synced.history.day(day(30)), DayStat { sessions: 3, minutes: 75 });

    // Re-sending what the server already has cannot inflate the total.
    let synced = server.record(&session.token, day(30), "laptop", 2, 50).await;
    assert_eq!(synced.history.day(day(30)), DayStat { sessions: 3, minutes: 75 });

    let board = server.board(Some(&session.token), day(30)).await;
    assert_eq!(board.day, day(30));
    assert_eq!(board.players, 1);
    assert_eq!(board.entries.len(), 1);
    assert_eq!(board.entries[0].name, "Ada");
    assert_eq!(board.entries[0].minutes, 75);
    assert_eq!(board.entries[0].rank, Some(1));
    assert!(board.entries[0].you);
    assert_eq!(board.you.expect("own row").minutes, 75);
}

#[tokio::test]
async fn the_board_ranks_everyone_and_marks_the_caller() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    let bo = server.sign_up("Bo", "bo@example.com").await;
    let cy = server.sign_up("Cy", "cy@example.com").await;
    server.record(&ada.token, day(30), "d", 2, 50).await;
    server.record(&bo.token, day(30), "d", 4, 100).await;
    server.record(&cy.token, day(30), "d", 1, 25).await;

    let board = server.board(Some(&ada.token), day(30)).await;
    assert_eq!(
        board.entries.iter().map(|e| (e.name.as_str(), e.minutes, e.rank, e.you)).collect::<Vec<_>>(),
        [("Bo", 100, Some(1), false), ("Ada", 50, Some(2), true), ("Cy", 25, Some(3), false)]
    );
    assert_eq!(board.players, 3);

    // Anyone may read the board; without a token there is simply no "you".
    let anonymous = server.board(None, day(30)).await;
    assert_eq!(anonymous.entries.len(), 3);
    assert!(anonymous.you.is_none());
    assert!(anonymous.entries.iter().all(|e| !e.you));

    // The board is per day: yesterday is empty but still answers.
    let quiet = server.board(Some(&ada.token), day(29)).await;
    assert!(quiet.entries.is_empty() && quiet.players == 0);
    assert_eq!(quiet.you.expect("own row").minutes, 0);
}

#[tokio::test]
async fn a_renamed_user_shows_the_new_name_and_can_leave_the_board() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    let bo = server.sign_up("Bo", "bo@example.com").await;
    server.record(&ada.token, day(30), "d", 4, 100).await;
    server.record(&bo.token, day(30), "d", 1, 25).await;

    let renamed = server
        .request(reqwest::Method::PATCH, "/v1/me", Some(&ada.token))
        .json(&json!({ "display_name": "  Ada  Lovelace " }))
        .send()
        .await
        .expect("patch");
    let renamed: User = renamed.json().await.unwrap();
    assert_eq!(renamed.display_name, "Ada Lovelace", "whitespace is tidied up");
    assert_eq!(server.board(None, day(30)).await.entries[0].name, "Ada Lovelace");

    // Opting out hides the row from everyone but still reports her own minutes.
    let hidden = server
        .request(reqwest::Method::PATCH, "/v1/me", Some(&ada.token))
        .json(&json!({ "show_on_leaderboard": false }))
        .send()
        .await
        .expect("patch");
    assert!(!hidden.json::<User>().await.unwrap().show_on_leaderboard);

    let board = server.board(Some(&ada.token), day(30)).await;
    assert_eq!(board.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Bo"]);
    assert_eq!(board.players, 1);
    let you = board.you.expect("own row");
    assert_eq!((you.minutes, you.rank), (100, None), "no rank while unlisted");
}

#[tokio::test]
async fn goals_only_travel_when_they_are_sent() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;

    // No `goals` key: the server leaves goals alone and reports nothing back.
    let reply = server.record(&ada.token, day(30), "d", 1, 25).await;
    assert!(reply.goals.is_none());

    let reply = server
        .post(
            "/v1/sync",
            Some(&ada.token),
            json!({ "history": { "days": {} }, "goals": { "days": { "2026-10-30": ["Write the report"] } } }),
        )
        .await;
    let reply: SyncResponse = reply.json().await.unwrap();
    assert_eq!(reply.goals.expect("goals").days[&day(30)], ["Write the report"]);

    // A second computer's goals are added to, not replaced.
    let reply = server
        .post(
            "/v1/sync",
            Some(&ada.token),
            json!({ "history": { "days": {} }, "goals": { "days": { "2026-10-30": ["Read the paper"] } } }),
        )
        .await;
    let reply: SyncResponse = reply.json().await.unwrap();
    assert_eq!(
        reply.goals.expect("goals").days[&day(30)],
        ["Read the paper", "Write the report"]
    );
}

#[tokio::test]
async fn logging_in_again_needs_the_right_password() {
    let server = start().await;
    server.sign_up("Ada", "ada@example.com").await;

    let ok = server
        .post(
            "/v1/auth/email/login",
            None,
            json!({ "email": "ADA@example.com", "password": "a-good-password" }),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::OK, "the address is not case sensitive");
    let session: Session = ok.json().await.unwrap();
    assert_eq!(session.user.display_name, "Ada");

    let wrong = server
        .post(
            "/v1/auth/email/login",
            None,
            json!({ "email": "ada@example.com", "password": "not-the-password" }),
        )
        .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_of(wrong).await, "No account for that email and password.");

    let unknown = server
        .post(
            "/v1/auth/email/login",
            None,
            json!({ "email": "nobody@example.com", "password": "a-good-password" }),
        )
        .await;
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_address_cannot_sign_up_twice() {
    let server = start().await;
    server.sign_up("Ada", "ada@example.com").await;
    let again = server
        .post(
            "/v1/auth/email/signup",
            None,
            json!({ "email": "ada@example.com", "password": "another-password", "display_name": "Ada" }),
        )
        .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    assert_eq!(error_of(again).await, "That email already has an account.");
}

#[tokio::test]
async fn signing_up_refuses_a_bad_address_weak_password_or_empty_name() {
    let server = start().await;
    let cases = [
        (json!({ "email": "not-an-address", "password": "a-good-password", "display_name": "Ada" }), "address"),
        (json!({ "email": "ada@example.com", "password": "short", "display_name": "Ada" }), "password"),
        (json!({ "email": "ada@example.com", "password": "a-good-password", "display_name": "   " }), "name"),
    ];
    for (body, what) in cases {
        let reply = server.post("/v1/auth/email/signup", None, body).await;
        assert_eq!(reply.status(), StatusCode::BAD_REQUEST, "{what} should be refused");
        assert!(!error_of(reply).await.is_empty(), "{what} needs a message");
    }
}

#[tokio::test]
async fn an_impossible_day_is_refused() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    let reply = server
        .post(
            "/v1/sync",
            Some(&ada.token),
            json!({ "history": { "days": { "2026-10-30": { "d": { "sessions": 1, "minutes": 99999 } } } } }),
        )
        .await;
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    assert!(error_of(reply).await.contains("more focus than a day holds"));
    // Nothing was stored, so the board stays empty.
    assert!(server.board(Some(&ada.token), day(30)).await.entries.is_empty());
}

#[tokio::test]
async fn the_private_endpoints_need_a_real_token() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;

    for token in [None, Some("not-a-token")] {
        assert_eq!(server.get("/v1/me", token).await.status(), StatusCode::UNAUTHORIZED);
        let sync = server.post("/v1/sync", token, json!({ "history": { "days": {} } })).await;
        assert_eq!(sync.status(), StatusCode::UNAUTHORIZED);
    }

    // Signing out spends the token.
    let out = server.post("/v1/auth/signout", Some(&ada.token), json!({})).await;
    assert_eq!(out.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        server.get("/v1/me", Some(&ada.token)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn deleting_an_account_clears_it_from_the_board() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    server.record(&ada.token, day(30), "d", 4, 100).await;
    assert_eq!(server.board(None, day(30)).await.players, 1);

    let gone = server
        .request(reqwest::Method::DELETE, "/v1/me", Some(&ada.token))
        .send()
        .await
        .expect("delete");
    assert_eq!(gone.status(), StatusCode::NO_CONTENT);
    assert_eq!(server.board(None, day(30)).await.players, 0);
    assert_eq!(server.get("/v1/me", Some(&ada.token)).await.status(), StatusCode::UNAUTHORIZED);

    // The address is free again.
    server.sign_up("Ada again", "ada@example.com").await;
}

/// A 1x1 PNG, the smallest real one there is.
fn tiny_png() -> Vec<u8> {
    const BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(BASE64).expect("a valid PNG")
}

#[tokio::test]
async fn a_profile_picture_is_stored_served_and_shown_on_the_board() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    assert!(ada.user.avatar.is_none(), "a new account has no picture");

    let uploaded = server
        .request(reqwest::Method::PUT, "/v1/me/avatar", Some(&ada.token))
        .header("Content-Type", "image/png")
        .body(tiny_png())
        .send()
        .await
        .expect("upload");
    assert_eq!(uploaded.status(), StatusCode::OK);
    let user: User = uploaded.json().await.unwrap();
    let path = user.avatar.clone().expect("a picture path");
    assert!(path.starts_with("/v1/avatars/"), "{path}");

    // It comes back as a PNG that caches forever.
    let fetched = server.get(&path, None).await;
    assert_eq!(fetched.status(), StatusCode::OK);
    assert_eq!(fetched.headers()["content-type"], "image/png");
    assert_eq!(fetched.headers()["x-content-type-options"], "nosniff");
    assert!(fetched.headers()["cache-control"].to_str().unwrap().contains("immutable"));
    assert_eq!(fetched.bytes().await.unwrap().to_vec(), tiny_png());

    // The board carries it, so other people see the face.
    server.record(&ada.token, day(30), "d", 1, 25).await;
    let board = server.board(None, day(30)).await;
    assert_eq!(board.entries[0].avatar.as_deref(), Some(path.as_str()));

    // Uploading again moves the picture to a new address.
    let again = server
        .request(reqwest::Method::PUT, "/v1/me/avatar", Some(&ada.token))
        .body(tiny_png())
        .send()
        .await
        .expect("upload");
    let moved: User = again.json().await.unwrap();
    assert_ne!(moved.avatar, user.avatar, "a new address, so no stale cache");

    // Removing it takes it off the board too.
    let removed = server
        .request(reqwest::Method::DELETE, "/v1/me/avatar", Some(&ada.token))
        .send()
        .await
        .expect("delete");
    assert!(removed.json::<User>().await.unwrap().avatar.is_none());
    assert!(server.board(None, day(30)).await.entries[0].avatar.is_none());
    assert_eq!(server.get(&path, None).await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_picture_that_is_not_a_png_is_refused() {
    let server = start().await;
    let ada = server.sign_up("Ada", "ada@example.com").await;
    let reply = server
        .request(reqwest::Method::PUT, "/v1/me/avatar", Some(&ada.token))
        .body(b"<svg onload=alert(1)></svg>".to_vec())
        .send()
        .await
        .expect("upload");
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    assert!(error_of(reply).await.contains("PNG"));

    // And a picture needs an account behind it.
    let anonymous = server
        .request(reqwest::Method::PUT, "/v1/me/avatar", None)
        .body(tiny_png())
        .send()
        .await
        .expect("upload");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_unconfigured_provider_says_so_instead_of_failing_obscurely() {
    let server = start().await;
    for provider in ["google", "facebook"] {
        let reply = server.post("/v1/auth/oauth/start", None, json!({ "provider": provider })).await;
        assert_eq!(reply.status(), StatusCode::SERVICE_UNAVAILABLE, "{provider}");
        assert!(error_of(reply).await.contains("not set up"));
    }
    // "email" is a sign-in but not an OAuth one.
    let reply = server.post("/v1/auth/oauth/start", None, json!({ "provider": "email" })).await;
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn polling_an_unknown_ticket_stops_the_app_waiting() {
    let server = start().await;
    let reply = server.post("/v1/auth/oauth/poll", None, json!({ "ticket": "never-issued" })).await;
    assert_eq!(reply.status(), StatusCode::OK);
    match reply.json::<PollReply>().await.expect("poll reply") {
        PollReply::Failed { error } => assert!(error.contains("expired")),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[tokio::test]
async fn the_callback_answers_a_browser_even_when_it_cannot_help() {
    let server = start().await;

    // A state nobody issued: a page, not a crash, since a person is reading it.
    let reply = server.get("/v1/auth/oauth/callback/google?code=x&state=unknown", None).await;
    assert_eq!(reply.status(), StatusCode::OK);
    let html = body_text(reply).await;
    assert!(html.contains("expired"), "{html}");

    let reply = server.get("/v1/auth/oauth/callback/myspace?code=x&state=y", None).await;
    assert!(body_text(reply).await.contains("Unknown sign-in provider"));

    let reply = server.get("/v1/auth/oauth/callback/google", None).await;
    assert!(body_text(reply).await.contains("incomplete"));
}
