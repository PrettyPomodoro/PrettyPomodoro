# PrettyPomodoro server

The backend behind the app's Account page and leaderboard: accounts, focus
history and the daily board. One Rust binary and a SQLite file beside it — no
database server, no runtime to install.

```bash
cargo run -p prettypomodoro-server
```

It listens on `127.0.0.1:8787` and writes `prettypomodoro.db` in the working
directory. Point the app at it from **Account → Server**, or run the app with
`POMO_SERVER=http://127.0.0.1:8787`.

## Settings

Everything is read from the environment. Copy `.env.example` and adapt it, or
export the variables directly.

| Variable | Default | What it does |
| --- | --- | --- |
| `PP_BIND` | `127.0.0.1:8787` | Address to listen on. Use `0.0.0.0:8787` behind a proxy. |
| `PP_DATABASE` | `prettypomodoro.db` | Path to the SQLite file. It is created if missing. |
| `PP_PUBLIC_URL` | `http://$PP_BIND` | Where a browser reaches this server. The OAuth redirect URIs are built from it, so it must match what you register with Google and Facebook. |
| `PP_LEADERBOARD_LIMIT` | `50` | How many rows the board returns at most. |
| `PP_GOOGLE_CLIENT_ID` / `PP_GOOGLE_CLIENT_SECRET` | – | Enables "Continue with Google". |
| `PP_FACEBOOK_APP_ID` / `PP_FACEBOOK_APP_SECRET` | – | Enables "Continue with Facebook". |
| `PP_LOG` | `info` | Log filter, e.g. `debug` for a line per request. |

Email sign-in always works. A provider whose credentials are missing is simply
not offered: the app asks `/v1/auth/providers` and only draws the buttons the
server can honour.

## Setting up Google and Facebook

Both need an app registered with the provider, and both need the redirect URI to
match `PP_PUBLIC_URL` exactly.

**Google** — in the [Cloud console](https://console.cloud.google.com/apis/credentials),
create an *OAuth client ID* of type *Web application* and add this redirect URI:

```
https://your-server.example.com/v1/auth/oauth/callback/google
```

**Facebook** — in [Meta for Developers](https://developers.facebook.com/apps),
create an app, add *Facebook Login*, and add:

```
https://your-server.example.com/v1/auth/oauth/callback/facebook
```

Facebook only returns an email address for apps that have been reviewed for the
`email` permission; without it, accounts are created with a name and no address,
which the leaderboard is perfectly happy with.

## How a browser sign-in works

The app is a desktop window with no URL of its own, so it cannot receive a
redirect. Instead:

1. The app asks `POST /v1/auth/oauth/start` for a provider. The server stores a
   *ticket* with a CSRF `state` and a PKCE verifier, and returns the ticket plus
   the page to open.
2. The app opens that page in the user's browser and starts polling
   `POST /v1/auth/oauth/poll` every two seconds.
3. The provider sends the browser to `/v1/auth/oauth/callback/{provider}`. The
   server exchanges the code, finds or creates the account, mints a session
   token and parks it on the ticket.
4. The next poll collects the token and deletes the ticket, so a token is handed
   over exactly once. Unfinished tickets expire after ten minutes and are swept
   hourly.

No client secret ever reaches the app.

## API

All bodies are JSON. Authenticated calls carry `Authorization: Bearer <token>`.

| Endpoint | What it does |
| --- | --- |
| `GET /healthz` | Liveness: returns `ok`. |
| `GET /v1/auth/providers` | Which sign-in buttons to draw. |
| `POST /v1/auth/email/signup` | `{email, password, display_name}` → session. |
| `POST /v1/auth/email/login` | `{email, password}` → session. |
| `POST /v1/auth/oauth/start` | `{provider}` → `{ticket, url, expires_in}`. |
| `POST /v1/auth/oauth/poll` | `{ticket}` → `pending`, `ready` with a session, or `failed`. |
| `GET /v1/auth/oauth/callback/{provider}` | Where the browser lands. Returns a page for a person to read. |
| `POST /v1/auth/signout` | Spends the current token. |
| `GET /v1/me` | The account. |
| `PATCH /v1/me` | `{display_name?, show_on_leaderboard?}`. |
| `DELETE /v1/me` | Erases the account, its history and its sessions. |
| `POST /v1/sync` | `{history, goals?}` → the merged history (and goals, if sent). |
| `GET /v1/leaderboard?day=YYYY-MM-DD&limit=N` | The board for a day. Readable without signing in; `you` is then null. |

Errors come back as `{"error": "…", "code": "…"}` with a message meant to be
shown to a person.

### Syncing is a merge, not an overwrite

History is stored per account, per day, per device, and a sync takes the higher
of the two counts for each. A device only ever raises its own number, so
replaying a request, or syncing from several computers, cannot inflate a total.
Goals are added, never removed.

### "Today" is local

Every row is filed under the date the app had when the round finished, so asking
for `2026-10-30` compares each person's own 30 October. The app sends its local
date with every request to the board.

## Security notes

- Passwords are hashed with Argon2id and a per-password salt.
- Session tokens are 32 random bytes; only their SHA-256 is stored, so a leaked
  database cannot be used to sign in as anyone.
- Signing in with Google and signing up with the same address by password make
  **two separate accounts**, deliberately: linking them automatically would let
  anyone who can get a token from a provider take over a password account.
  Someone who tries the password form for an address that belongs to a provider
  account is told which button to press.
- A Google email address is only kept when Google marks it verified.
- Sign-ins carry PKCE and a CSRF `state`, both checked on the callback.

### What the board trusts

The server rejects a day with more than 24 hours or 288 sessions on one device,
which catches broken clients. Below that line it takes the app's word for it: a
modified client could report minutes it never focused. Ranking people by a
number their own computer reports is the design; it is a leaderboard among
people who want one, not an audited record.

Tokens travel in an `Authorization` header, so run this behind a reverse proxy
that terminates TLS before exposing it to anything but localhost, and put a rate
limit in front of `/v1/auth/*` if it is public. There is no rate limiting built
in.

## Tests

```bash
cargo test -p prettypomodoro-server
```

Unit tests cover the merge rules, ranking, ticket handling, password hashing and
the OAuth URL and token parsing. `tests/api.rs` starts a real server on an
ephemeral port with a temporary database and drives it over HTTP. The parts that
need Google or Facebook themselves are not covered; everything around them is.

To check the desktop app against a running server:

```bash
cargo run -p prettypomodoro-server &
POMO_SERVER=http://127.0.0.1:8787 cargo test -- --ignored
```
