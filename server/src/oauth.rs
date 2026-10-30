//! The Google and Facebook authorization-code flows, with PKCE.
//!
//! The app never sees a client secret: it asks this server to start a sign-in,
//! opens the returned page in the user's browser, and polls until the browser
//! has come back through the callback below.

use crate::error::ApiError;
use crate::model::{Profile, Provider};
use crate::state::AppState;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::Value;
use url::Url;

const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const FACEBOOK_VERSION: &str = "v21.0";

fn facebook_url(path: &str) -> String {
    match path {
        "dialog/oauth" => format!("https://www.facebook.com/{FACEBOOK_VERSION}/dialog/oauth"),
        other => format!("https://graph.facebook.com/{FACEBOOK_VERSION}/{other}"),
    }
}

/// The page to open in the browser to begin a sign-in.
pub fn authorize_url(
    state: &AppState,
    provider: Provider,
    csrf: &str,
    challenge: &str,
) -> Result<String, ApiError> {
    let app = state.config.oauth(provider)?;
    let redirect = state.config.redirect_uri(provider);
    let (base, scope) = match provider {
        Provider::Google => (GOOGLE_AUTH.to_owned(), "openid email profile"),
        Provider::Facebook => (facebook_url("dialog/oauth"), "email,public_profile"),
        Provider::Email => return Err(ApiError::BadRequest("Email is not an OAuth provider.".into())),
    };
    let mut url = Url::parse(&base).map_err(|e| ApiError::Internal(format!("bad auth URL: {e}")))?;
    url.query_pairs_mut()
        .append_pair("client_id", &app.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", scope)
        .append_pair("state", csrf)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    if provider == Provider::Google {
        // Always show the chooser, so a shared computer does not sign in silently.
        url.query_pairs_mut().append_pair("prompt", "select_account");
    }
    Ok(url.into())
}

/// Swap the code the browser brought back for the user's identity.
pub async fn exchange(
    state: &AppState,
    provider: Provider,
    code: &str,
    verifier: &str,
) -> Result<Profile, ApiError> {
    match provider {
        Provider::Google => google(state, code, verifier).await,
        Provider::Facebook => facebook(state, code, verifier).await,
        Provider::Email => Err(ApiError::BadRequest("Email is not an OAuth provider.".into())),
    }
}

async fn google(state: &AppState, code: &str, verifier: &str) -> Result<Profile, ApiError> {
    let app = state.config.oauth(Provider::Google)?;
    let form = [
        ("code", code),
        ("client_id", app.client_id.as_str()),
        ("client_secret", app.client_secret.as_str()),
        ("redirect_uri", &state.config.redirect_uri(Provider::Google)),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier),
    ];
    let reply = state.http.post(GOOGLE_TOKEN).form(&form).send().await;
    let body = read_json("Google", reply).await?;
    let id_token = body["id_token"]
        .as_str()
        .ok_or_else(|| ApiError::Internal("Google returned no id_token.".into()))?;
    profile_from_id_token(id_token)
}

async fn facebook(state: &AppState, code: &str, verifier: &str) -> Result<Profile, ApiError> {
    let app = state.config.oauth(Provider::Facebook)?;
    let token_request = state
        .http
        .get(facebook_url("oauth/access_token"))
        .query(&[
            ("code", code),
            ("client_id", app.client_id.as_str()),
            ("client_secret", app.client_secret.as_str()),
            ("redirect_uri", &state.config.redirect_uri(Provider::Facebook)),
            ("code_verifier", verifier),
        ])
        .send()
        .await;
    let body = read_json("Facebook", token_request).await?;
    let token = body["access_token"]
        .as_str()
        .ok_or_else(|| ApiError::Internal("Facebook returned no access token.".into()))?;

    let me = state
        .http
        .get(facebook_url("me"))
        .query(&[("fields", "id,name,email"), ("access_token", token)])
        .send()
        .await;
    let me = read_json("Facebook", me).await?;
    let subject = me["id"]
        .as_str()
        .ok_or_else(|| ApiError::Internal("Facebook returned no user id.".into()))?;
    Ok(Profile {
        subject: subject.to_owned(),
        email: me["email"].as_str().map(str::to_owned),
        name: fallback_name(me["name"].as_str(), me["email"].as_str()),
    })
}

/// Read a provider's JSON reply, turning a refusal into a message the app can show.
async fn read_json(
    who: &str,
    reply: Result<reqwest::Response, reqwest::Error>,
) -> Result<Value, ApiError> {
    let reply = reply.map_err(|e| ApiError::Internal(format!("reaching {who}: {e}")))?;
    let status = reply.status();
    let text = reply
        .text()
        .await
        .map_err(|e| ApiError::Internal(format!("reading {who}'s reply: {e}")))?;
    if !status.is_success() {
        // The body carries the real reason; log it and keep the app's message short.
        tracing::warn!("{who} returned {status}: {text}");
        return Err(ApiError::BadRequest(format!("{who} refused the sign-in.")));
    }
    serde_json::from_str(&text).map_err(|e| ApiError::Internal(format!("{who} sent no JSON: {e}")))
}

/// Read `sub`, `email` and `name` out of Google's ID token.
///
/// The signature is not checked, and does not need to be: the token arrived in
/// the body of our own HTTPS call to Google's token endpoint, authenticated with
/// this app's client secret, rather than by way of the browser.
fn profile_from_id_token(id_token: &str) -> Result<Profile, ApiError> {
    let claims = id_token
        .split('.')
        .nth(1)
        .and_then(|payload| URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| ApiError::Internal("Google's id_token could not be read.".into()))?;

    let subject = claims["sub"]
        .as_str()
        .ok_or_else(|| ApiError::Internal("Google's id_token has no subject.".into()))?;
    // Only keep an address Google says it has verified.
    let verified = claims["email_verified"].as_bool().unwrap_or(false)
        || claims["email_verified"].as_str() == Some("true");
    let email = claims["email"].as_str().filter(|_| verified);
    Ok(Profile {
        subject: subject.to_owned(),
        email: email.map(str::to_owned),
        name: fallback_name(claims["name"].as_str(), claims["email"].as_str()),
    })
}

/// A name for the leaderboard: what the provider gave, else the local part of
/// the address, else something neutral.
fn fallback_name(name: Option<&str>, email: Option<&str>) -> String {
    let from_name = name.map(str::trim).filter(|n| !n.is_empty());
    let from_email = email.and_then(|e| e.split('@').next()).filter(|n| !n.is_empty());
    from_name.or(from_email).unwrap_or("Pomodoro user").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Config, OauthApp};

    fn state() -> AppState {
        let config = Config {
            public_url: "https://api.example.com".into(),
            google: Some(OauthApp { client_id: "google-id".into(), client_secret: "s".into() }),
            facebook: Some(OauthApp { client_id: "fb-id".into(), client_secret: "s".into() }),
            ..Config::for_test(":memory:")
        };
        let pool = crate::db::open(":memory:").unwrap();
        AppState::new(pool, config).unwrap()
    }

    fn id_token(claims: Value) -> String {
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        format!("header.{payload}.signature")
    }

    #[test]
    fn the_authorize_url_carries_state_and_the_pkce_challenge() {
        let state = state();
        let url = authorize_url(&state, Provider::Google, "csrf-1", "challenge-1").unwrap();
        let parsed = Url::parse(&url).unwrap();
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(parsed.host_str(), Some("accounts.google.com"));
        assert_eq!(q["client_id"], "google-id");
        assert_eq!(q["state"], "csrf-1");
        assert_eq!(q["code_challenge"], "challenge-1");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["response_type"], "code");
        assert_eq!(
            q["redirect_uri"],
            "https://api.example.com/v1/auth/oauth/callback/google"
        );

        let url = authorize_url(&state, Provider::Facebook, "csrf-2", "challenge-2").unwrap();
        let parsed = Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("www.facebook.com"));
        assert!(parsed.query().unwrap().contains("state=csrf-2"));
    }

    #[test]
    fn an_unconfigured_provider_is_reported_not_guessed() {
        let pool = crate::db::open(":memory:").unwrap();
        let state = AppState::new(pool, Config::for_test(":memory:")).unwrap();
        let err = authorize_url(&state, Provider::Google, "c", "c").unwrap_err();
        assert!(matches!(err, ApiError::NotConfigured(_)));
    }

    #[test]
    fn an_id_token_yields_the_subject_name_and_verified_email() {
        let token = id_token(serde_json::json!({
            "sub": "123", "email": "ada@example.com", "email_verified": true, "name": "Ada L"
        }));
        let profile = profile_from_id_token(&token).unwrap();
        assert_eq!(profile.subject, "123");
        assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
        assert_eq!(profile.name, "Ada L");
    }

    #[test]
    fn an_unverified_email_is_dropped_but_still_names_the_user() {
        let token = id_token(serde_json::json!({
            "sub": "123", "email": "ada@example.com", "email_verified": false
        }));
        let profile = profile_from_id_token(&token).unwrap();
        assert_eq!(profile.email, None);
        assert_eq!(profile.name, "ada", "the local part stands in for a missing name");
    }

    #[test]
    fn a_token_without_a_subject_is_refused() {
        assert!(profile_from_id_token(&id_token(serde_json::json!({ "email": "a@b.com" }))).is_err());
        assert!(profile_from_id_token("not-a-jwt").is_err());
        assert!(profile_from_id_token("a.!!!!.c").is_err());
    }

    #[test]
    fn a_profile_with_nothing_in_it_still_gets_a_name() {
        let profile = profile_from_id_token(&id_token(serde_json::json!({ "sub": "1" }))).unwrap();
        assert_eq!(profile.name, "Pomodoro user");
    }
}
