//! The SQLite store: schema, accounts, sessions, sign-in tickets and the focus
//! history that the leaderboard is computed from.
//!
//! Every function here is blocking and takes a connection, so handlers call
//! them through `AppState::db`, which runs them on the blocking pool.

use crate::error::ApiError;
use crate::model::{DayStat, Entry, GoalLog, History, Provider, User};
use chrono::{DateTime, NaiveDate, Utc};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{named_params, params, Connection, OptionalExtension};

pub type Pool = r2d2::Pool<SqliteConnectionManager>;

/// Open (creating if needed) the database at `path` and bring the schema up to
/// date. `:memory:` gives a throwaway database for tests; it is held open by a
/// single connection, because a second one would see an empty database of its own.
pub fn open(path: &str) -> Result<Pool, ApiError> {
    let in_memory = path == ":memory:";
    let manager = if in_memory {
        SqliteConnectionManager::memory()
    } else {
        SqliteConnectionManager::file(path)
    };
    let manager = manager.with_init(move |conn| {
        conn.execute_batch(if in_memory {
            "PRAGMA foreign_keys = ON;"
        } else {
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;"
        })
    });
    let pool = r2d2::Pool::builder()
        .max_size(if in_memory { 1 } else { 8 })
        .build(manager)
        .map_err(|e| ApiError::Internal(format!("opening {path}: {e}")))?;
    let mut conn = pool.get()?;
    migrate(&mut conn)?;
    Ok(pool)
}

/// Schema version 1. Later versions add a branch here rather than editing the
/// statements above them, so existing databases upgrade in place.
fn migrate(conn: &mut Connection) -> Result<(), ApiError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(
            r#"
            CREATE TABLE users (
                id                  INTEGER PRIMARY KEY,
                provider            TEXT    NOT NULL,
                email               TEXT,
                password_hash       TEXT,
                display_name        TEXT    NOT NULL,
                show_on_leaderboard INTEGER NOT NULL DEFAULT 1,
                created_at          TEXT    NOT NULL
            );
            -- Only password accounts claim an address; an OAuth account may carry
            -- the same email without colliding, because the two are never linked
            -- automatically (see `email_account`).
            CREATE UNIQUE INDEX users_email
                ON users(email) WHERE email IS NOT NULL AND provider = 'email';

            CREATE TABLE identities (
                provider TEXT    NOT NULL,
                subject  TEXT    NOT NULL,
                user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                PRIMARY KEY (provider, subject)
            );

            CREATE TABLE sessions (
                token_hash TEXT    PRIMARY KEY,
                user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                created_at TEXT    NOT NULL
            );
            CREATE INDEX sessions_user ON sessions(user_id);

            -- One in-flight browser sign-in. `session_token` is the finished
            -- token, held in the clear until the app polls for it once.
            CREATE TABLE tickets (
                ticket_hash   TEXT PRIMARY KEY,
                provider      TEXT NOT NULL,
                state         TEXT NOT NULL UNIQUE,
                pkce_verifier TEXT NOT NULL,
                expires_at    TEXT NOT NULL,
                session_token TEXT,
                error         TEXT
            );

            CREATE TABLE day_stats (
                user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                day      TEXT    NOT NULL,
                device   TEXT    NOT NULL,
                sessions INTEGER NOT NULL,
                minutes  INTEGER NOT NULL,
                PRIMARY KEY (user_id, day, device)
            );
            CREATE INDEX day_stats_day ON day_stats(day);

            CREATE TABLE goals (
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                day     TEXT    NOT NULL,
                text    TEXT    NOT NULL,
                PRIMARY KEY (user_id, day, text)
            );

            PRAGMA user_version = 1;
            "#,
        )?;
    }
    if version < 2 {
        // Profile pictures. `avatar_id` is random and changes on every upload,
        // so the URL doubles as a cache buster and reveals no user id.
        conn.execute_batch(
            r#"
            ALTER TABLE users ADD COLUMN avatar BLOB;
            ALTER TABLE users ADD COLUMN avatar_id TEXT;
            CREATE UNIQUE INDEX users_avatar_id
                ON users(avatar_id) WHERE avatar_id IS NOT NULL;
            PRAGMA user_version = 2;
            "#,
        )?;
    }
    Ok(())
}

/// Where a stored picture is served from. Relative, so the app puts its own
/// server in front of it.
pub fn avatar_path(avatar_id: &str) -> String {
    format!("/v1/avatars/{avatar_id}")
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    let provider: String = row.get("provider")?;
    let avatar_id: Option<String> = row.get("avatar_id")?;
    Ok(User {
        display_name: row.get("display_name")?,
        email: row.get("email")?,
        provider: Provider::parse(&provider).unwrap_or(Provider::Email),
        show_on_leaderboard: row.get::<_, i64>("show_on_leaderboard")? != 0,
        avatar: avatar_id.as_deref().map(avatar_path),
    })
}

const USER_COLUMNS: &str =
    "id, provider, email, display_name, show_on_leaderboard, avatar_id";

pub fn user(conn: &Connection, id: i64) -> Result<Option<User>, ApiError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?1");
    Ok(conn.query_row(&sql, params![id], row_to_user).optional()?)
}

/// Create a password account. The caller has already hashed the password.
pub fn create_email_user(
    conn: &Connection,
    email: &str,
    password_hash: &str,
    display_name: &str,
) -> Result<i64, ApiError> {
    let done = conn.execute(
        "INSERT INTO users (provider, email, password_hash, display_name, created_at)
         VALUES ('email', ?1, ?2, ?3, ?4)",
        params![email, password_hash, display_name, Utc::now()],
    );
    match done {
        Ok(_) => Ok(conn.last_insert_rowid()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(ApiError::Conflict("That email already has an account.".into()))
        }
        Err(e) => Err(e.into()),
    }
}

/// The password account for `email`, if there is one: its id and stored hash.
pub fn email_account(conn: &Connection, email: &str) -> Result<Option<(i64, String)>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT id, password_hash FROM users WHERE provider = 'email' AND email = ?1",
            params![email],
            |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())),
        )
        .optional()?)
}

/// Which OAuth providers, if any, already have an account for `email`. Used to
/// point someone at the right button instead of a bare "wrong password".
pub fn oauth_providers_for_email(conn: &Connection, email: &str) -> Result<Vec<String>, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT provider FROM users WHERE email = ?1 AND provider <> 'email'",
    )?;
    let rows = stmt.query_map(params![email], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Find the account behind an OAuth identity, creating it on first sign-in.
/// An existing account keeps the display name it has, so a name edited in the
/// app is not overwritten by the provider's on every sign-in.
pub fn upsert_oauth_user(
    conn: &mut Connection,
    provider: Provider,
    subject: &str,
    email: Option<&str>,
    display_name: &str,
) -> Result<i64, ApiError> {
    let tx = conn.transaction()?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT user_id FROM identities WHERE provider = ?1 AND subject = ?2",
            params![provider.as_str(), subject],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        tx.commit()?;
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO users (provider, email, display_name, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![provider.as_str(), email, display_name, Utc::now()],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO identities (provider, subject, user_id) VALUES (?1, ?2, ?3)",
        params![provider.as_str(), subject, id],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn update_profile(
    conn: &Connection,
    id: i64,
    display_name: Option<&str>,
    show: Option<bool>,
) -> Result<(), ApiError> {
    conn.execute(
        "UPDATE users
            SET display_name        = COALESCE(:name, display_name),
                show_on_leaderboard = COALESCE(:show, show_on_leaderboard)
          WHERE id = :id",
        named_params! { ":name": display_name, ":show": show, ":id": id },
    )?;
    Ok(())
}

/// Delete the account and, through `ON DELETE CASCADE`, its history, goals and
/// sessions.
pub fn delete_user(conn: &Connection, id: i64) -> Result<(), ApiError> {
    conn.execute("DELETE FROM users WHERE id = ?1", params![id])?;
    Ok(())
}

/// Replace the user's picture, or clear it when `image` is `None`. The caller
/// has already checked that the bytes are a PNG of a sensible size.
pub fn set_avatar(
    conn: &Connection,
    user_id: i64,
    image: Option<(&[u8], &str)>,
) -> Result<(), ApiError> {
    match image {
        Some((bytes, avatar_id)) => conn.execute(
            "UPDATE users SET avatar = ?1, avatar_id = ?2 WHERE id = ?3",
            params![bytes, avatar_id, user_id],
        )?,
        None => conn.execute(
            "UPDATE users SET avatar = NULL, avatar_id = NULL WHERE id = ?1",
            params![user_id],
        )?,
    };
    Ok(())
}

pub fn avatar(conn: &Connection, avatar_id: &str) -> Result<Option<Vec<u8>>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT avatar FROM users WHERE avatar_id = ?1",
            params![avatar_id],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?
        .flatten())
}

pub fn insert_session(conn: &Connection, user_id: i64, token_hash: &str) -> Result<(), ApiError> {
    conn.execute(
        "INSERT INTO sessions (token_hash, user_id, created_at) VALUES (?1, ?2, ?3)",
        params![token_hash, user_id, Utc::now()],
    )?;
    Ok(())
}

pub fn session_user(conn: &Connection, token_hash: &str) -> Result<Option<(i64, User)>, ApiError> {
    let sql = format!(
        "SELECT {USER_COLUMNS} FROM users
           JOIN sessions ON sessions.user_id = users.id
          WHERE sessions.token_hash = ?1"
    );
    Ok(conn
        .query_row(&sql, params![token_hash], |r| Ok((r.get("id")?, row_to_user(r)?)))
        .optional()?)
}

pub fn delete_session(conn: &Connection, token_hash: &str) -> Result<(), ApiError> {
    conn.execute("DELETE FROM sessions WHERE token_hash = ?1", params![token_hash])?;
    Ok(())
}

/// A browser sign-in waiting to be finished, found by the `state` the provider
/// hands back on the callback.
pub struct Ticket {
    pub ticket_hash: String,
    pub provider: Provider,
    pub pkce_verifier: String,
}

pub fn insert_ticket(
    conn: &Connection,
    ticket_hash: &str,
    provider: Provider,
    state: &str,
    pkce_verifier: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    conn.execute(
        "INSERT INTO tickets (ticket_hash, provider, state, pkce_verifier, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![ticket_hash, provider.as_str(), state, pkce_verifier, expires_at],
    )?;
    Ok(())
}

pub fn ticket_by_state(conn: &Connection, state: &str) -> Result<Option<Ticket>, ApiError> {
    Ok(conn
        .query_row(
            "SELECT ticket_hash, provider, pkce_verifier FROM tickets
              WHERE state = ?1 AND expires_at > ?2",
            params![state, Utc::now()],
            |r| {
                let provider: String = r.get(1)?;
                Ok(Ticket {
                    ticket_hash: r.get(0)?,
                    provider: Provider::parse(&provider).unwrap_or(Provider::Google),
                    pkce_verifier: r.get(2)?,
                })
            },
        )
        .optional()?)
}

/// Park the finished session token (or the reason it failed) for the app to collect.
pub fn resolve_ticket(
    conn: &Connection,
    ticket_hash: &str,
    outcome: Result<&str, &str>,
) -> Result<(), ApiError> {
    let (token, error) = match outcome {
        Ok(token) => (Some(token), None),
        Err(message) => (None, Some(message)),
    };
    conn.execute(
        "UPDATE tickets SET session_token = ?1, error = ?2 WHERE ticket_hash = ?3",
        params![token, error, ticket_hash],
    )?;
    Ok(())
}

/// What a poll found. A finished ticket is removed as it is handed over, so a
/// token is only ever collected once.
pub enum Claim {
    Pending,
    Ready(String),
    Failed(String),
    Expired,
}

pub fn claim_ticket(conn: &mut Connection, ticket_hash: &str) -> Result<Claim, ApiError> {
    let tx = conn.transaction()?;
    let found: Option<(Option<String>, Option<String>, DateTime<Utc>)> = tx
        .query_row(
            "SELECT session_token, error, expires_at FROM tickets WHERE ticket_hash = ?1",
            params![ticket_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let claim = match found {
        None => Claim::Expired,
        Some((Some(token), _, _)) => Claim::Ready(token),
        Some((_, Some(error), _)) => Claim::Failed(error),
        Some((_, _, expires_at)) if expires_at <= Utc::now() => Claim::Expired,
        Some(_) => Claim::Pending,
    };
    if !matches!(claim, Claim::Pending) {
        tx.execute("DELETE FROM tickets WHERE ticket_hash = ?1", params![ticket_hash])?;
    }
    tx.commit()?;
    Ok(claim)
}

/// Drop tickets nobody finished. Called on a timer.
pub fn purge_expired_tickets(conn: &Connection) -> Result<usize, ApiError> {
    Ok(conn.execute("DELETE FROM tickets WHERE expires_at <= ?1", params![Utc::now()])?)
}

/// Take the higher count for every (day, device) the app reports. Replaying the
/// same request, or syncing from several computers, cannot inflate a total.
pub fn merge_history(conn: &mut Connection, user_id: i64, history: &History) -> Result<(), ApiError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO day_stats (user_id, day, device, sessions, minutes)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (user_id, day, device) DO UPDATE SET
                 sessions = MAX(sessions, excluded.sessions),
                 minutes  = MAX(minutes,  excluded.minutes)",
        )?;
        for (day, devices) in &history.days {
            for (device, stat) in devices {
                stmt.execute(params![user_id, day, device, stat.sessions, stat.minutes])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn history(conn: &Connection, user_id: i64) -> Result<History, ApiError> {
    let mut stmt = conn.prepare(
        "SELECT day, device, sessions, minutes FROM day_stats WHERE user_id = ?1 ORDER BY day",
    )?;
    let rows = stmt.query_map(params![user_id], |r| {
        Ok((
            r.get::<_, NaiveDate>(0)?,
            r.get::<_, String>(1)?,
            DayStat { sessions: r.get(2)?, minutes: r.get(3)? },
        ))
    })?;
    let mut history = History::default();
    for row in rows {
        let (day, device, stat) = row?;
        history.days.entry(day).or_default().insert(device, stat);
    }
    Ok(history)
}

/// Add the goal texts that are not stored yet, day by day.
pub fn merge_goals(conn: &mut Connection, user_id: i64, goals: &GoalLog) -> Result<(), ApiError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO goals (user_id, day, text) VALUES (?1, ?2, ?3)",
        )?;
        for (day, texts) in &goals.days {
            for text in texts {
                stmt.execute(params![user_id, day, text])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn goals(conn: &Connection, user_id: i64) -> Result<GoalLog, ApiError> {
    let mut stmt =
        conn.prepare("SELECT day, text FROM goals WHERE user_id = ?1 ORDER BY day, text")?;
    let rows = stmt.query_map(params![user_id], |r| {
        Ok((r.get::<_, NaiveDate>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut goals = GoalLog::default();
    for row in rows {
        let (day, text) = row?;
        goals.days.entry(day).or_default().push(text);
    }
    Ok(goals)
}

/// Everyone's minutes for `day`, best first, and the caller's own row.
///
/// `day` is each user's own local date, which is how the app records it, so the
/// board compares like with like across time zones.
pub fn leaderboard(
    conn: &Connection,
    day: NaiveDate,
    limit: u32,
    me: Option<i64>,
) -> Result<(Vec<Entry>, Option<Entry>, u32), ApiError> {
    // Totals per listed account for the day, ordered as the board shows them.
    let mut stmt = conn.prepare(
        "SELECT u.id, u.display_name, u.avatar_id,
                SUM(d.sessions) AS sessions, SUM(d.minutes) AS minutes
           FROM day_stats d
           JOIN users u ON u.id = d.user_id
          WHERE d.day = ?1 AND u.show_on_leaderboard = 1
          GROUP BY u.id
         HAVING minutes > 0
          ORDER BY minutes DESC, sessions DESC, u.created_at ASC",
    )?;
    let listed: Vec<(i64, String, Option<String>, u32, u32)> = stmt
        .query_map(params![day], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let players = listed.len() as u32;

    // Standard competition ranking: equal minutes and sessions share a rank.
    let rank_of = |minutes: u32, sessions: u32| -> u32 {
        let ahead = listed
            .iter()
            .filter(|(_, _, _, s, m)| *m > minutes || (*m == minutes && *s > sessions))
            .count();
        ahead as u32 + 1
    };

    let entries: Vec<Entry> = listed
        .iter()
        .take(limit as usize)
        .map(|(id, name, avatar_id, sessions, minutes)| Entry {
            rank: Some(rank_of(*minutes, *sessions)),
            name: name.clone(),
            minutes: *minutes,
            sessions: *sessions,
            you: Some(*id) == me,
            avatar: avatar_id.as_deref().map(avatar_path),
        })
        .collect();

    let you = match me {
        None => None,
        Some(id) => {
            let (name, show, avatar_id): (String, bool, Option<String>) = conn.query_row(
                "SELECT display_name, show_on_leaderboard, avatar_id FROM users WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0, r.get(2)?)),
            )?;
            let (sessions, minutes): (u32, u32) = conn.query_row(
                "SELECT COALESCE(SUM(sessions), 0), COALESCE(SUM(minutes), 0)
                   FROM day_stats WHERE user_id = ?1 AND day = ?2",
                params![id, day],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            // No rank for someone who is not listed, or who has not focused yet.
            let rank = (show && minutes > 0).then(|| rank_of(minutes, sessions));
            let avatar = avatar_id.as_deref().map(avatar_path);
            Some(Entry { rank, name, minutes, sessions, you: true, avatar })
        }
    };

    Ok((entries, you, players))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GoalLog;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        migrate(&mut conn).unwrap();
        conn
    }

    fn day(n: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, n).unwrap()
    }

    fn user_with(conn: &mut Connection, name: &str, email: &str) -> i64 {
        create_email_user(conn, email, "hash", name).unwrap()
    }

    fn record(conn: &mut Connection, id: i64, day: NaiveDate, device: &str, stat: (u32, u32)) {
        let mut history = History::default();
        history
            .days
            .entry(day)
            .or_default()
            .insert(device.to_owned(), DayStat { sessions: stat.0, minutes: stat.1 });
        merge_history(conn, id, &history).unwrap();
    }

    #[test]
    fn merging_the_same_history_twice_changes_nothing() {
        let mut conn = db();
        let id = user_with(&mut conn, "A", "a@example.com");
        record(&mut conn, id, day(1), "laptop", (2, 50));
        record(&mut conn, id, day(1), "laptop", (2, 50));
        assert_eq!(history(&conn, id).unwrap().day(day(1)), DayStat { sessions: 2, minutes: 50 });

        // A second device adds to the day; a stale report from the first does not.
        record(&mut conn, id, day(1), "desktop", (1, 25));
        record(&mut conn, id, day(1), "laptop", (1, 25));
        assert_eq!(history(&conn, id).unwrap().day(day(1)), DayStat { sessions: 3, minutes: 75 });
    }

    #[test]
    fn leaderboard_ranks_by_minutes_and_respects_opting_out() {
        let mut conn = db();
        let (a, b, c) = (
            user_with(&mut conn, "Ada", "a@example.com"),
            user_with(&mut conn, "Bo", "b@example.com"),
            user_with(&mut conn, "Cy", "c@example.com"),
        );
        record(&mut conn, a, day(1), "d", (2, 50));
        record(&mut conn, b, day(1), "d", (4, 100));
        record(&mut conn, c, day(1), "d", (1, 25));
        record(&mut conn, c, day(2), "d", (9, 225));

        let (entries, you, players) = leaderboard(&conn, day(1), 10, Some(a)).unwrap();
        assert_eq!(players, 3);
        assert_eq!(
            entries.iter().map(|e| (e.name.as_str(), e.minutes, e.rank)).collect::<Vec<_>>(),
            [("Bo", 100, Some(1)), ("Ada", 50, Some(2)), ("Cy", 25, Some(3))]
        );
        assert!(entries[1].you, "the caller's row is marked");
        assert_eq!(you.unwrap().rank, Some(2));

        // Opting out removes the row but keeps the user's own totals visible.
        update_profile(&conn, b, None, Some(false)).unwrap();
        let (entries, you, players) = leaderboard(&conn, day(1), 10, Some(b)).unwrap();
        assert_eq!(players, 2);
        assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Ada", "Cy"]);
        let you = you.unwrap();
        assert_eq!((you.minutes, you.rank), (100, None));
    }

    #[test]
    fn a_tie_shares_a_rank() {
        let mut conn = db();
        let (a, b, c) = (
            user_with(&mut conn, "Ada", "a@example.com"),
            user_with(&mut conn, "Bo", "b@example.com"),
            user_with(&mut conn, "Cy", "c@example.com"),
        );
        record(&mut conn, a, day(1), "d", (2, 50));
        record(&mut conn, b, day(1), "d", (2, 50));
        record(&mut conn, c, day(1), "d", (1, 25));
        let (entries, ..) = leaderboard(&conn, day(1), 10, None).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.rank).collect::<Vec<_>>(),
            [Some(1), Some(1), Some(3)]
        );
    }

    #[test]
    fn the_board_is_per_day() {
        let mut conn = db();
        let a = user_with(&mut conn, "Ada", "a@example.com");
        record(&mut conn, a, day(1), "d", (2, 50));
        let (entries, you, players) = leaderboard(&conn, day(2), 10, Some(a)).unwrap();
        assert!(entries.is_empty() && players == 0);
        assert_eq!(you.unwrap().minutes, 0, "a day with no focus still reports the user");
    }

    #[test]
    fn goals_are_merged_without_duplicates() {
        let mut conn = db();
        let a = user_with(&mut conn, "Ada", "a@example.com");
        let mut log = GoalLog::default();
        log.days.insert(day(1), vec!["Report".into(), "Slides".into()]);
        merge_goals(&mut conn, a, &log).unwrap();
        log.days.get_mut(&day(1)).unwrap().push("Review".into());
        merge_goals(&mut conn, a, &log).unwrap();
        assert_eq!(goals(&conn, a).unwrap().days[&day(1)], ["Report", "Review", "Slides"]);
    }

    #[test]
    fn an_email_is_claimed_once_but_oauth_may_reuse_it() {
        let mut conn = db();
        create_email_user(&conn, "a@example.com", "h", "Ada").unwrap();
        let again = create_email_user(&conn, "a@example.com", "h", "Ada");
        assert!(matches!(again, Err(ApiError::Conflict(_))));

        // The same address arriving through Google is a separate account.
        upsert_oauth_user(&mut conn, Provider::Google, "sub-1", Some("a@example.com"), "Ada G")
            .unwrap();
        assert_eq!(oauth_providers_for_email(&conn, "a@example.com").unwrap(), ["google"]);
    }

    #[test]
    fn signing_in_again_reuses_the_account_and_keeps_the_chosen_name() {
        let mut conn = db();
        let first =
            upsert_oauth_user(&mut conn, Provider::Google, "sub-1", None, "From Google").unwrap();
        update_profile(&conn, first, Some("My Name"), None).unwrap();
        let again =
            upsert_oauth_user(&mut conn, Provider::Google, "sub-1", None, "From Google").unwrap();
        assert_eq!(first, again);
        assert_eq!(user(&conn, first).unwrap().unwrap().display_name, "My Name");
    }

    #[test]
    fn deleting_an_account_takes_its_history_with_it() {
        let mut conn = db();
        let a = user_with(&mut conn, "Ada", "a@example.com");
        record(&mut conn, a, day(1), "d", (2, 50));
        insert_session(&conn, a, "token-hash").unwrap();
        delete_user(&conn, a).unwrap();
        assert!(user(&conn, a).unwrap().is_none());
        assert!(session_user(&conn, "token-hash").unwrap().is_none());
        let (entries, ..) = leaderboard(&conn, day(1), 10, None).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn a_ticket_hands_its_token_over_once() {
        let mut conn = db();
        let soon = Utc::now() + chrono::Duration::minutes(10);
        insert_ticket(&conn, "t-hash", Provider::Google, "state-1", "verifier", soon).unwrap();
        assert!(matches!(claim_ticket(&mut conn, "t-hash").unwrap(), Claim::Pending));

        let found = ticket_by_state(&conn, "state-1").unwrap().unwrap();
        assert_eq!(found.pkce_verifier, "verifier");
        resolve_ticket(&conn, &found.ticket_hash, Ok("session-token")).unwrap();

        let claim = claim_ticket(&mut conn, "t-hash").unwrap();
        assert!(matches!(claim, Claim::Ready(t) if t == "session-token"));
        assert!(matches!(claim_ticket(&mut conn, "t-hash").unwrap(), Claim::Expired));
    }

    #[test]
    fn an_expired_ticket_is_neither_found_nor_claimable() {
        let mut conn = db();
        let past = Utc::now() - chrono::Duration::minutes(1);
        insert_ticket(&conn, "t-hash", Provider::Google, "state-1", "verifier", past).unwrap();
        assert!(ticket_by_state(&conn, "state-1").unwrap().is_none());
        assert!(matches!(claim_ticket(&mut conn, "t-hash").unwrap(), Claim::Expired));
        insert_ticket(&conn, "t2", Provider::Google, "state-2", "v", past).unwrap();
        assert_eq!(purge_expired_tickets(&conn).unwrap(), 1);
    }
}
