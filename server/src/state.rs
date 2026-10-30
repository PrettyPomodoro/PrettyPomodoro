//! Settings read from the environment, and the state every handler shares.

use crate::db::Pool;
use crate::error::ApiError;
use crate::model::{Provider, Providers};
use std::sync::Arc;
use std::time::Duration;

/// Credentials for one OAuth app, as registered with Google or Facebook.
#[derive(Clone, Debug)]
pub struct OauthApp {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub database: String,
    /// Where this server is reachable from a browser; the OAuth redirect URIs
    /// are built from it, so it must match what is registered with the providers.
    pub public_url: String,
    pub leaderboard_limit: u32,
    pub google: Option<OauthApp>,
    pub facebook: Option<OauthApp>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Both halves of a provider's credentials, or none of it.
fn oauth_app(id: &str, secret: &str) -> Option<OauthApp> {
    Some(OauthApp { client_id: env(id)?, client_secret: env(secret)? })
}

impl Config {
    pub fn from_env() -> Self {
        let bind = env("PP_BIND").unwrap_or_else(|| "127.0.0.1:8787".to_owned());
        let public_url = env("PP_PUBLIC_URL").unwrap_or_else(|| format!("http://{bind}"));
        Self {
            bind,
            database: env("PP_DATABASE").unwrap_or_else(|| "prettypomodoro.db".to_owned()),
            public_url: public_url.trim_end_matches('/').to_owned(),
            leaderboard_limit: env("PP_LEADERBOARD_LIMIT")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            google: oauth_app("PP_GOOGLE_CLIENT_ID", "PP_GOOGLE_CLIENT_SECRET"),
            facebook: oauth_app("PP_FACEBOOK_APP_ID", "PP_FACEBOOK_APP_SECRET"),
        }
    }

    /// Used only by tests, which bind an ephemeral port and a temporary file.
    #[cfg(test)]
    pub fn for_test(database: &str) -> Self {
        Self {
            bind: "127.0.0.1:0".to_owned(),
            database: database.to_owned(),
            public_url: "http://127.0.0.1:0".to_owned(),
            leaderboard_limit: 50,
            google: None,
            facebook: None,
        }
    }

    pub fn oauth(&self, provider: Provider) -> Result<&OauthApp, ApiError> {
        let app = match provider {
            Provider::Google => self.google.as_ref(),
            Provider::Facebook => self.facebook.as_ref(),
            Provider::Email => None,
        };
        app.ok_or_else(|| {
            ApiError::NotConfigured(format!(
                "Signing in with {} is not set up on this server.",
                match provider {
                    Provider::Google => "Google",
                    Provider::Facebook => "Facebook",
                    Provider::Email => "email",
                }
            ))
        })
    }

    pub fn redirect_uri(&self, provider: Provider) -> String {
        format!("{}/v1/auth/oauth/callback/{}", self.public_url, provider.as_str())
    }

    pub fn providers(&self) -> Providers {
        Providers {
            email: true,
            google: self.google.is_some(),
            facebook: self.facebook.is_some(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pool: Pool,
    pub config: Arc<Config>,
    /// Shared client for talking to the identity providers.
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(pool: Pool, config: Config) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("prettypomodoro-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ApiError::Internal(format!("building an HTTP client: {e}")))?;
        Ok(Self { pool, config: Arc::new(config), http })
    }

    /// Run a blocking database closure on the blocking pool.
    pub async fn db<T, F>(&self, work: F) -> Result<T, ApiError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get()?;
            work(&mut conn)
        })
        .await
        .map_err(|e| ApiError::Internal(format!("database task: {e}")))?
    }

    /// Run slow CPU work (password hashing) off the async runtime.
    pub async fn blocking<T, F>(&self, work: F) -> Result<T, ApiError>
    where
        F: FnOnce() -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|e| ApiError::Internal(format!("blocking task: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_half_of_a_credential_pair_disables_the_provider() {
        let config = Config {
            google: Some(OauthApp { client_id: "id".into(), client_secret: "secret".into() }),
            ..Config::for_test("x.db")
        };
        assert!(config.oauth(Provider::Google).is_ok());
        assert!(matches!(config.oauth(Provider::Facebook), Err(ApiError::NotConfigured(_))));
        assert!(config.providers().google && !config.providers().facebook);
    }

    #[test]
    fn redirect_uris_hang_off_the_public_url() {
        let config = Config { public_url: "https://api.example.com".into(), ..Config::for_test("x.db") };
        assert_eq!(
            config.redirect_uri(Provider::Google),
            "https://api.example.com/v1/auth/oauth/callback/google"
        );
    }
}
