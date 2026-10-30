//! Start the server. Everything is configured through the environment; see
//! `server/README.md` and `.env.example`.

use prettypomodoro_server::state::Config;
use prettypomodoro_server::{build, spawn_ticket_cleanup};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("PP_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    let (bind, database) = (config.bind.clone(), config.database.clone());
    let providers = config.providers();
    let public_url = config.public_url.clone();

    let (state, router) = build(config)?;
    spawn_ticket_cleanup(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("listening on http://{bind} (public URL {public_url})");
    tracing::info!("database {database}");
    tracing::info!(
        "sign-in: email {}, Google {}, Facebook {}",
        on_off(providers.email),
        on_off(providers.google),
        on_off(providers.facebook),
    );
    if !providers.google && !providers.facebook {
        tracing::warn!(
            "no OAuth credentials set, so only email sign-in will work; \
             set PP_GOOGLE_CLIENT_ID / PP_GOOGLE_CLIENT_SECRET or \
             PP_FACEBOOK_APP_ID / PP_FACEBOOK_APP_SECRET to enable the others"
        );
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;
    Ok(())
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}
