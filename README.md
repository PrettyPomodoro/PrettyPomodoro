# PrettyPomodoro

A small, themeable Pomodoro timer for the desktop, written in pure Rust with
[egui](https://github.com/emilk/egui). The layout follows
[Pomotroid](https://github.com/Splode/pomotroid).

| Sky Sketch | Editorial Lavender | Sketch Light |
| --- | --- | --- |
| ![Sky Sketch](docs/sky-sketch.svg) | ![Editorial Lavender](docs/editorial-lavender.svg) | ![Sketch Light](docs/sketch-light.svg) |

| Leaderboard | Activity | Activity (dark) | Account |
| --- | --- | --- | --- |
| ![Leaderboard](docs/leaderboard.svg) | ![Activity](docs/activity.svg) | ![Activity dark](docs/activity-dark.svg) | ![Account](docs/account.svg) |

## Features

- **Pretty themes.** 27 built-in themes in five families (Editorial, Sketch,
  Light, Dark and Pencil Art), plus an editor for making your own.
- **Progress calendar.** A heat map of every focus round you finish,
  with today, this week, streak and all-time totals.
- **A leaderboard for today.** See how your focus minutes compare with everyone
  else's, refreshed every minute while you watch. Opt out at any time and keep
  your minutes to yourself.
- **Your history on your account.** Sign in with Google, Facebook or an email
  address and your focus history follows you between computers.
- **Set goals for each run.** Type a goal above the timer and tick it off in
  green when the round ends. Achieved goals are kept in a history, grouped by day.
- **Lightweight and efficient (built with Rust).** One small native executable
  with no bundled browser engine: a download under 10 MB that uses no CPU
  while idle.

## Download

[![Download for macOS](https://img.shields.io/badge/Download-macOS%20(Apple%20Silicon%20%2B%20Intel)-000000?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/PrettyPomodoro/PrettyPomodoro/releases/latest/download/PrettyPomodoro-macos-universal.zip)
[![Download for Windows](https://img.shields.io/badge/Download-Windows%20x64-0078D4?style=for-the-badge)](https://github.com/PrettyPomodoro/PrettyPomodoro/releases/latest/download/PrettyPomodoro-windows-x64.zip)
[![Download for Linux](https://img.shields.io/badge/Download-Linux%20x64-E95420?style=for-the-badge&logo=linux&logoColor=white)](https://github.com/PrettyPomodoro/PrettyPomodoro/releases/latest/download/PrettyPomodoro-linux-x64.tar.gz)

The buttons always download the latest published release.

- **macOS**: unzip and move `PrettyPomodoro.app` to Applications. The app is not
  notarized, so macOS blocks the first launch: press **Done**, open **System
  Settings → Privacy & Security**, scroll to Security and press **Open Anyway**.
  This is needed once.
- **Windows**: unzip and run `PrettyPomodoro.exe`. SmartScreen may ask for
  confirmation because the executable is unsigned.
- **Linux**: extract the archive and run `./install.sh`, or run `./prettypomodoro`
  directly. Sound needs ALSA (`libasound2`).

## Updates

The app checks GitHub for a newer release when it starts and twice a day while
it runs. When there is one it shows a desktop notification, and an **Update**
button appears under the title bar and on the About page.

- **macOS**: the button downloads the release, replaces the app in place and
  relaunches it. A round in progress is not carried over.
- **Windows** and **Linux**: the button opens the download for your platform
  in the browser; install it as before.

The check is one request to GitHub's public API and sends nothing about you.

## Small and fast

PrettyPomodoro is a single native Rust executable. It does not bundle a browser
engine or a language runtime.

| | Measured |
| --- | --- |
| Download (macOS universal zip, both architectures) | 9.8 MB |
| Executable (Apple Silicon) | 8.6 MB |
| Memory while open | 67 MB |
| CPU while idle | 0.0 % |
| Launch, render the window and exit | 0.45 s |
| Threads | 8 |
| Runtime dependencies to install | 0 |

Measured on an Apple M1 Max running macOS 26.5 with the release build:
memory, CPU and threads from `top` on the idle app with the timer paused;
launch time is the wall-clock time of a warm start that draws ten frames and
quits. The window only redraws when something changes, which is why an idle
timer uses no CPU.

## Build and run

```bash
cargo run --release
```

On macOS, build `PrettyPomodoro.app`, install it into `/Applications` and open it with:

```bash
packaging/macos/install.sh
```

Requires a recent stable Rust toolchain. On Linux, install the ALSA and
D-Bus development packages first (`libasound2-dev`, `libdbus-1-dev` on Debian/Ubuntu).

`Space` starts and pauses the timer.

## Account, sync and the leaderboard

Signing in gives you two things: your focus history on every computer you use,
and a place on the leaderboard.

1. Open **Settings → Account** (the cloud tab).
2. Press **Continue with Google**, **Continue with Facebook**, or make an
   account with an email address and a password.
3. Google and Facebook open your browser; finish there and come back. The app
   is waiting and signs itself in.

From then on the app syncs on launch and after every completed focus round.
Syncing merges rather than overwrites: each computer only ever raises its own
count for a day, so syncing repeatedly or from several machines never double
counts.

### The leaderboard

The trophy button on the home screen opens the leaderboard: everyone signed in,
ranked by the focus minutes they have put in today, with your own row marked.
It refreshes every minute while the page is open, and again whenever you finish
a round.

"Today" is each person's own local date, so a day in Melbourne is compared with
a day in Berlin rather than with a slice of two.

Your name and today's minutes are the only things that appear. Turn off
**Show Me on the Leaderboard** on the Account page to disappear from it; your
history still syncs, and you still see your own total. Goal texts never appear
on the board.

### Privacy and what is stored

The server keeps your display name, your account's email address if you signed
up with one, and a count of sessions and minutes per day per device. The text of
the goals you tick off stays on your computer unless you switch on **Sync Goal
Texts**.

**Delete Account** on the Account page erases the account and everything stored
with it.

The session token is kept in the config file, readable only by your user account
on macOS and Linux. **Sign Out** removes it.

### Running the backend

The server is a second crate in this repository, in `server/`. It is a single
Rust binary with a SQLite file beside it:

```bash
cargo run -p prettypomodoro-server
```

See [server/README.md](server/README.md) for the settings, the OAuth setup and
the API. Point the app at your own copy from **Account → Server**, or set
`POMO_SERVER` while developing. The address the app uses by default is the
`DEFAULT_SERVER` constant in `src/api.rs`.

## Data

Settings and history live in the platform config directory under
`pomodoro-rs/` (`~/Library/Application Support/pomodoro-rs` on macOS):
`config.json` for settings, custom themes and the signed-in account,
`history.json` for completed focus sessions and `goals.json` for the goals you
have ticked off.

Your computer stays the source of truth: the app works signed out, and the
server holds a copy, not the original.

## Releases

`.github/workflows/release.yml` builds and tests the app on macOS, Windows and
Linux on every push to `main` and keeps the three packages as workflow
artifacts. Pushing a version tag also creates a **draft** GitHub release with
the packages attached; it becomes public only when published by hand.

```bash
git tag v0.1.0
```

```bash
git push origin v0.1.0
```

The icon files in `assets/` are generated from `assets/icon-source.webp` with
`cargo run --example make_icons`.

## Platform notes

- **macOS**: fully supported and the platform this was developed and tested on.
- **Windows**: builds from the same code, including the tray; not yet tested on
  a real machine.
- **Linux**: no tray icon yet, so the two tray options fall back to a normal
  minimize / close.
- Text uses the system UI font (San Francisco, Segoe UI, or Inter / Ubuntu /
  Noto / Liberation / DejaVu on Linux) and falls back to a bundled font, so
  every label renders on all three.

## Acknowledgements

Thank you to [Pomotroid](https://github.com/Splode/pomotroid) for inspiring this project.

## License

MIT
