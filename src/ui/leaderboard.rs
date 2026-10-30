//! Leaderboard page: who has focused the most today, refreshed every minute
//! for as long as the page is open.

use super::stats::duration;
use super::widgets::{heading, ink_outline, text_link};
use crate::api::{Entry, Leaderboard};
use crate::core::{Board, Core, BOARD_REFRESH};
use crate::theme::{mix, Theme};
use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align2, Color32, FontId, Painter, Sense, Stroke,
    TextFormat, Ui,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One row per player.
const ROW: f32 = 38.0;
const ROW_GAP: f32 = 4.0;
const RADIUS: f32 = 7.0;
/// Width set aside for the rank, left of the face.
const RANK_W: f32 = 26.0;
/// The face between the rank and the name.
const FACE: f32 = 26.0;
/// Colour of the flame beside the leader's minutes.
const FLAME: Color32 = Color32::from_rgb(0xf5, 0x7c, 0x1f);

/// What the page is asking the app to do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Fetch the board again now.
    Refresh,
    /// Take the user to the Account page to sign in.
    OpenAccount,
}

pub fn show(ui: &mut Ui, core: &mut Core, theme: &Theme) -> Option<Action> {
    heading(ui, "Leaderboard", theme);
    // Keep the "updated …" line and the minute refresh moving along.
    ui.ctx().request_repaint_after(Duration::from_secs(1));

    let mut action = None;
    let signed_in = core.cfg.account.signed_in();

    // Pictures first: looking one up may start fetching it, which needs the
    // whole of `core`; everything below only reads the board.
    let paths: Vec<Option<String>> = core
        .board
        .data
        .iter()
        .flat_map(|board| board.entries.iter())
        .map(|entry| entry.avatar.clone())
        .collect();
    let faces: Vec<Option<egui::TextureHandle>> =
        paths.iter().map(|path| core.avatar(path.as_deref())).collect();

    match (&core.board.data, &core.board.error) {
        // Nothing yet: either the first fetch or a failure with no board to show.
        (None, None) => {
            waiting(ui, &core.board, theme);
        }
        (None, Some(error)) => {
            if let Some(got) = trouble(ui, error, theme) {
                action = Some(got);
            }
        }
        (Some(board), error) => {
            let players = board.players;
            let mine = board.you.clone();
            table(ui, board, &faces, theme);
            ui.add_space(8.0);
            standing(ui, mine.as_ref(), players, signed_in, theme);
            if let Some(error) = error {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(short(error)).size(11.5).color(theme.text_dim),
                    );
                });
            }
        }
    }

    // Signing in is what makes the board yours, so say so until it is done.
    if !signed_in {
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Sign in to put your minutes on the board.")
                    .size(13.0)
                    .color(theme.text),
            );
            ui.add_space(4.0);
            let button =
                egui::Button::new(egui::RichText::new("Sign In").size(14.0).color(theme.on_accent()))
                    .fill(theme.accent)
                    .min_size(vec2(160.0, 30.0));
            if ui.add(button).clicked() {
                action = Some(Action::OpenAccount);
            }
        });
    }

    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(freshness(&core.board)).size(11.5).color(theme.text_dim));
        if !core.board.loading && text_link(ui, "Refresh Now", 12.5, theme).clicked() {
            action = Some(Action::Refresh);
        }
    });
    action
}

fn waiting(ui: &mut Ui, board: &Board, theme: &Theme) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        let message = if board.loading { "Loading the board…" } else { "Nothing yet." };
        ui.label(egui::RichText::new(message).size(13.5).color(theme.text_dim));
    });
    ui.add_space(20.0);
}

/// A failure with nothing to fall back on: say what happened, offer another go.
fn trouble(ui: &mut Ui, error: &str, theme: &Theme) -> Option<Action> {
    let mut action = None;
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("The board could not be loaded").size(14.0));
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(egui::RichText::new(error).size(12.5).color(theme.focus)).wrap(),
        );
        ui.add_space(8.0);
        if text_link(ui, "Try Again", 13.5, theme).clicked() {
            action = Some(Action::Refresh);
        }
    });
    ui.add_space(12.0);
    action
}

fn table(ui: &mut Ui, board: &Leaderboard, faces: &[Option<egui::TextureHandle>], theme: &Theme) {
    let date = board.day.format("%a, %-d %b").to_string();
    let players = match board.players {
        1 => "1 player".to_owned(),
        n => format!("{n} players"),
    };
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(format!("{date} · {players}")).size(12.0).color(theme.text_dim));
    });
    ui.add_space(6.0);

    if board.entries.is_empty() {
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Nobody has finished a focus round today.")
                    .size(13.5)
                    .color(theme.text_dim),
            );
            ui.label(egui::RichText::new("Be the first.").size(13.5).color(theme.text_dim));
        });
        ui.add_space(14.0);
        return;
    }

    for (i, entry) in board.entries.iter().enumerate() {
        row(ui, entry, faces.get(i).and_then(Option::as_ref), theme);
        ui.add_space(ROW_GAP);
    }
}

/// One player: rank, face, name, and the minutes they have put in today.
fn row(ui: &mut Ui, entry: &Entry, face: Option<&egui::TextureHandle>, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let p = ui.painter();
    // The user's own row is tinted, so it is findable in a long list.
    let fill = if entry.you {
        mix(theme.surface, theme.accent, 0.22)
    } else {
        mix(theme.surface, theme.muted, 0.14)
    };
    p.rect_filled(rect, RADIUS, fill);
    ink_outline(p, rect, RADIUS, theme);
    if entry.you {
        p.rect_stroke(rect, RADIUS, Stroke::new(1.3_f32, theme.accent), egui::StrokeKind::Inside);
    }

    // Rank. The leader gets the flame the Activity page uses for today.
    let rank_centre = pos2(rect.left() + RANK_W / 2.0 + 3.0, rect.center().y);
    match entry.rank {
        Some(1) => {
            p.text(rank_centre, Align2::CENTER_CENTER, "🔥", FontId::proportional(16.0), FLAME);
        }
        Some(rank) => {
            p.text(
                rank_centre,
                Align2::CENTER_CENTER,
                rank.to_string(),
                FontId::proportional(14.0),
                theme.text_dim,
            );
        }
        None => {}
    }

    // Minutes on the right, name in whatever room is left.
    let minutes = duration(entry.minutes);
    let figure = FontId::proportional(14.0);
    let minutes_galley = p.layout_no_wrap(minutes, figure, theme.text);
    let minutes_w = minutes_galley.size().x;
    p.galley(
        pos2(rect.right() - 10.0 - minutes_w, rect.center().y - minutes_galley.size().y / 2.0),
        minutes_galley,
        theme.text,
    );

    let face_rect = egui::Rect::from_center_size(
        pos2(rect.left() + RANK_W + FACE / 2.0, rect.center().y),
        vec2(FACE, FACE),
    );
    crate::avatar::draw(p, face_rect, face, &entry.name, theme);

    let name_left = face_rect.right() + 8.0;
    let room = (rect.right() - 16.0 - minutes_w - name_left).max(20.0);
    let name = if entry.you {
        format!("{} (you)", entry.name)
    } else {
        entry.name.clone()
    };
    let name_galley = one_line(p, &name, FontId::proportional(14.5), theme.text, room);
    p.galley(
        pos2(name_left, rect.center().y - name_galley.size().y / 2.0),
        name_galley,
        theme.text,
    );

    ui.interact(rect, ui.id().with(("row", &entry.name, entry.rank)), Sense::hover())
        .on_hover_text(tooltip(entry));
}

/// Where the user stands, spelled out under the table.
fn standing(ui: &mut Ui, you: Option<&Entry>, players: u32, signed_in: bool, theme: &Theme) {
    let Some(you) = you.filter(|_| signed_in) else {
        return;
    };
    let line = match (you.rank, you.minutes) {
        (_, 0) => "You have not finished a focus round today.".to_owned(),
        (None, minutes) => format!(
            "{} today, kept to yourself.",
            duration(minutes)
        ),
        (Some(rank), minutes) => {
            format!("You are {} of {players} with {}.", ordinal(rank), duration(minutes))
        }
    };
    ui.vertical_centered(|ui| {
        ui.add(egui::Label::new(egui::RichText::new(line).size(12.5).color(theme.text_dim)).wrap());
    });
}

/// "Updated 12s ago", and what happens next.
fn freshness(board: &Board) -> String {
    if board.loading {
        return "Refreshing…".to_owned();
    }
    let Some(age) = board.age(Instant::now()) else {
        return "Refreshes every minute".to_owned();
    };
    let seconds = age.as_secs();
    let when = match seconds {
        0..=2 => "just now".to_owned(),
        s if s < 60 => format!("{s}s ago"),
        s if s < BOARD_REFRESH.as_secs() * 2 => "a minute ago".to_owned(),
        s => format!("{}m ago", s / 60),
    };
    format!("Updated {when} · refreshes every minute")
}

/// Lay out one line of text, cut short with an ellipsis if it will not fit.
fn one_line(
    p: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat { font_id: font, color, ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    p.layout_job(job)
}

/// Trim a message down to something that fits one small line under the board:
/// its first sentence, cut short with an ellipsis if even that is too long.
fn short(message: &str) -> String {
    let first = match message.split_once(". ") {
        Some((head, _)) => format!("{}.", head.trim()),
        None => message.trim().to_owned(),
    };
    match first.char_indices().nth(60) {
        None => first,
        Some((cut, _)) => format!("{}…", first[..cut].trim_end()),
    }
}

fn tooltip(entry: &Entry) -> String {
    let who = if entry.you { "You" } else { entry.name.as_str() };
    let rounds = match entry.sessions {
        1 => "1 focus round".to_owned(),
        n => format!("{n} focus rounds"),
    };
    match entry.rank {
        Some(rank) => format!("{who} · {} · {rounds} · {}", duration(entry.minutes), ordinal(rank)),
        None => format!("{who} · {} · {rounds} · not listed", duration(entry.minutes)),
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: Option<u32>, minutes: u32, you: bool) -> Entry {
        Entry { rank, name: "Ada".into(), minutes, sessions: minutes / 25, you, avatar: None }
    }

    #[test]
    fn ordinals_read_correctly() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111]
            .iter()
            .map(|n| ordinal(*n))
            .collect();
        assert_eq!(
            got,
            ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "101st", "111th"]
        );
    }

    #[test]
    fn the_tooltip_names_the_rank_and_the_rounds() {
        assert_eq!(tooltip(&entry(Some(2), 50, false)), "Ada · 50 min · 2 focus rounds · 2nd");
        assert_eq!(tooltip(&entry(Some(1), 25, true)), "You · 25 min · 1 focus round · 1st");
        assert!(tooltip(&entry(None, 75, true)).ends_with("not listed"));
    }

    #[test]
    fn a_failed_attempt_backs_off_instead_of_trying_again_at_once() {
        let mut board = Board::default();
        assert!(board.due(Instant::now()));
        board.failed("Could not reach the server.".into());
        let tried_at = board.fetched_at.expect("a failed attempt is still stamped");
        assert!(!board.due(tried_at), "the next tick waits rather than hammering the server");
        assert!(board.due(tried_at + BOARD_REFRESH), "but it does try again");
    }

    #[test]
    fn a_stale_boards_error_is_cut_down_to_one_line() {
        assert_eq!(short("Could not reach the server."), "Could not reach the server.");
        assert_eq!(short("Offline. Retrying in the background."), "Offline.");
        let long = short(&"x".repeat(100));
        assert!(long.chars().count() <= 61 && long.ends_with('…'));
    }

    #[test]
    fn freshness_counts_up_from_the_last_fetch() {
        let mut board = Board::default();
        assert_eq!(freshness(&board), "Refreshes every minute");
        board.loading = true;
        assert_eq!(freshness(&board), "Refreshing…");

        board.loading = false;
        board.fetched_at = Some(Instant::now() - Duration::from_secs(12));
        assert_eq!(freshness(&board), "Updated 12s ago · refreshes every minute");
        board.fetched_at = Some(Instant::now() - Duration::from_secs(300));
        assert_eq!(freshness(&board), "Updated 5m ago · refreshes every minute");
    }

    #[test]
    fn a_board_is_due_for_a_refresh_after_a_minute() {
        let mut board = Board::default();
        let now = Instant::now();
        assert!(board.due(now), "a board that was never fetched is due");
        board.fetched_at = Some(now);
        assert!(!board.due(now));
        assert!(!board.due(now + Duration::from_secs(59)));
        assert!(board.due(now + BOARD_REFRESH));
        board.stale();
        assert!(board.due(now), "a finished round makes it due again");
    }
}
