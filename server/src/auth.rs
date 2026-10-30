//! Session tokens, password hashing and the `Auth` extractor.
//!
//! A token is 32 random bytes, handed to the app once and stored here only as
//! its SHA-256, so a stolen database cannot be used to sign in as anyone.

use crate::db;
use crate::error::ApiError;
use crate::model::User;
use crate::state::AppState;
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Longest password we will hash, so a huge body cannot tie up a thread.
const MAX_PASSWORD: usize = 200;
const MIN_PASSWORD: usize = 8;
pub const MAX_DISPLAY_NAME: usize = 40;

/// 32 bytes of OS randomness, URL-safe. Used for tokens, tickets, PKCE and state.
pub fn secret() -> Result<String, ApiError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| ApiError::Internal(format!("randomness: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The S256 PKCE challenge for a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Hash a password for storage. Deliberately slow: call it off the async runtime.
pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(format!("hashing a password: {e}")))
}

/// Check a password against a stored hash. Also slow; same rule applies.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// Normalise an address for storage and lookup, and refuse what is clearly not one.
pub fn clean_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let shaped = {
        let mut parts = email.split('@');
        let (local, domain, extra) = (parts.next(), parts.next(), parts.next());
        match (local, domain, extra) {
            (Some(l), Some(d), None) => !l.is_empty() && d.contains('.') && !d.starts_with('.'),
            _ => false,
        }
    };
    if !shaped || email.len() > 254 || email.contains(char::is_whitespace) {
        return Err(ApiError::BadRequest("That does not look like an email address.".into()));
    }
    Ok(email)
}

pub fn check_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(ApiError::BadRequest(format!(
            "Use a password of at least {MIN_PASSWORD} characters."
        )));
    }
    if password.len() > MAX_PASSWORD {
        return Err(ApiError::BadRequest("That password is too long.".into()));
    }
    Ok(())
}

/// Squeeze a display name into one tidy line, since it goes on a public board.
pub fn clean_display_name(name: &str) -> Result<String, ApiError> {
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DISPLAY_NAME)
        .collect();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Choose a name to show on the leaderboard.".into()));
    }
    Ok(name)
}

/// Mint a session for `user_id` and return the token to hand to the app.
pub async fn start_session(state: &AppState, user_id: i64) -> Result<String, ApiError> {
    let token = secret()?;
    let hash = sha256(&token);
    state.db(move |conn| db::insert_session(conn, user_id, &hash)).await?;
    Ok(token)
}

/// A signed-in caller, taken from the `Authorization: Bearer …` header.
#[derive(Clone, Debug)]
pub struct Auth {
    pub user_id: i64,
    pub user: User,
    pub token_hash: String,
}

fn bearer(parts: &Parts) -> Option<String> {
    let value = parts.headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim().to_owned())
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let signed_out = || ApiError::Unauthorized("Sign in again to continue.".to_owned());
        let token = bearer(parts).ok_or_else(signed_out)?;
        let hash = sha256(&token);
        let lookup = hash.clone();
        let found = state.db(move |conn| db::session_user(conn, &lookup)).await?;
        let (user_id, user) = found.ok_or_else(signed_out)?;
        Ok(Self { user_id, user, token_hash: hash })
    }
}

/// Like `Auth`, but a request without a usable token is allowed through. Used by
/// the leaderboard, which anyone may read; only `you` needs an account.
#[derive(Clone, Debug)]
pub struct MaybeAuth(pub Option<Auth>);

impl FromRequestParts<AppState> for MaybeAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match Auth::from_request_parts(parts, state).await {
            Ok(auth) => Ok(Self(Some(auth))),
            Err(ApiError::Unauthorized(_)) => Ok(Self(None)),
            Err(other) => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_only_verifies_against_its_own_hash() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("Correct horse battery", &hash));
        assert!(!verify_password("", &hash));
        assert!(!verify_password("correct horse battery", "not-a-hash"));
        // The salt makes two hashes of one password differ.
        assert_ne!(hash, hash_password("correct horse battery").unwrap());
    }

    #[test]
    fn passwords_have_a_floor_and_a_ceiling() {
        assert!(check_password("12345678").is_ok());
        assert!(check_password("1234567").is_err());
        assert!(check_password(&"a".repeat(MAX_PASSWORD + 1)).is_err());
    }

    #[test]
    fn emails_are_normalised_and_obvious_junk_refused() {
        assert_eq!(clean_email("  Ada@Example.COM ").unwrap(), "ada@example.com");
        for bad in ["ada", "ada@", "@example.com", "ada@example", "a b@example.com", "a@@b.com"] {
            assert!(clean_email(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn display_names_become_one_tidy_line() {
        assert_eq!(clean_display_name("  Ada \n Lovelace\t ").unwrap(), "Ada Lovelace");
        assert_eq!(clean_display_name(&"x".repeat(100)).unwrap().chars().count(), MAX_DISPLAY_NAME);
        assert!(clean_display_name("   ").is_err());
    }

    #[test]
    fn secrets_are_unique_and_hashing_is_stable() {
        let (a, b) = (secret().unwrap(), secret().unwrap());
        assert_ne!(a, b);
        assert_eq!(sha256(&a), sha256(&a));
        assert_ne!(sha256(&a), sha256(&b));
        assert_eq!(sha256("abc").len(), 64);
    }

    #[test]
    fn the_pkce_challenge_matches_the_rfc_example() {
        // RFC 7636, appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(pkce_challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
}
