//! Activity page: today's minutes of focus, a heat map of one week's
//! completed focus sessions shaded from the theme's colours, summary tiles and
//! the goals achieved so far.

use super::widgets::{ink_outline, heading, icon_button, Icon};
use crate::core::Core;
use crate::theme::{mix, Theme};
use chrono::{Datelike, Days, NaiveDate, Weekday};
use std::sync::OnceLock;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

/// Side of a day's tile in the week row, and the space between tiles.
const TILE: f32 = 38.0;
const TILE_GAP: f32 = 7.0;
const RADIUS: f32 = 6.0;
/// Legend swatches.
const CELL: f32 = 13.0;
const STEP: f32 = 16.0;
/// Sessions needed to reach each shade above "empty".
const LEVELS: [u32; 4] = [1, 3, 5, 8];
/// Colour of the flame beside today's minutes.
const FLAME: Color32 = Color32::from_rgb(0xf5, 0x7c, 0x1f);

#[derive(Default)]
pub struct State {
    /// How many weeks back from the current one the map is showing.
    page: u64,
    /// Scroll the goal history into view; used when capturing that section.
    pub show_goals: bool,
}

fn shade(theme: &Theme, sessions: u32) -> Color32 {
    let empty = mix(theme.surface, theme.muted, 0.3);
    let level = LEVELS.iter().filter(|&&min| sessions >= min).count();
    match level {
        0 => empty,
        n => mix(empty, theme.focus, [0.3, 0.52, 0.76, 1.0][n - 1]),
    }
}

pub fn show(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    heading(ui, "Focus", theme);
    let today = chrono::Local::now().date_naive();
    let history = &core.history;

    let week_start = week_start_of(today, first_weekday());
    let week: u32 = (0..7)
        .map(|i| history.day(week_start + Days::new(i)).sessions)
        .sum();
    let total = history.total();
    let tiles = [
        (history.day(today).sessions.to_string(), "Today"),
        (week.to_string(), "This Week"),
        (history.streak(today).to_string(), "Day Streak"),
        (total.sessions.to_string(), "All Time"),
    ];
    // Today's focus, up front: a flame and the minutes.
    let (hero, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
    let p = ui.painter();
    let figure = FontId::proportional(28.0);
    let minutes = history.day(today).minutes.to_string();
    // The flame glyph carries blank space on its right, so the gap is small.
    let (flame_w, gap) = (p.layout_no_wrap("🔥".into(), figure.clone(), FLAME).size().x, 2.0);
    let minutes_w = p.layout_no_wrap(minutes.clone(), figure.clone(), theme.text).size().x;
    // Nudged left by the same blank space, so the drawn shapes are centred
    // rather than their boxes.
    let left = pos2(hero.center().x - (flame_w + gap + minutes_w) / 2.0 - 2.5, hero.top() + 15.0);
    p.text(left, Align2::LEFT_CENTER, "🔥", figure.clone(), FLAME);
    p.text(left + vec2(flame_w + gap, 0.0), Align2::LEFT_CENTER, minutes, figure, theme.text);
    p.text(
        pos2(hero.center().x, hero.top() + 38.0),
        Align2::CENTER_CENTER,
        "minutes today",
        FontId::proportional(11.5),
        theme.text_dim,
    );
    ui.add_space(2.0);

    // One row of seven tiles for the week `page` weeks back, starting on the
    // first day of the week from the system settings.
    let first = week_start - Days::new(7 * state.page);
    let last = first + Days::new(6);

    let (nav, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let slot = |x: f32| Rect::from_center_size(pos2(x, nav.center().y), vec2(26.0, 26.0));
    let range = if first.year() == last.year() {
        format!("{} – {}", first.format("%b %-d"), last.format("%b %-d, %Y"))
    } else {
        format!("{} – {}", first.format("%b %-d, %Y"), last.format("%b %-d, %Y"))
    };
    ui.painter().text(nav.center(), Align2::CENTER_CENTER, range, FontId::proportional(12.5), theme.text_dim);
    let has_older = history.days.keys().next().is_some_and(|d| *d < first);
    if has_older && icon_button(ui, slot(nav.left() + 10.0), Icon::Back, theme, false)
        .on_hover_text("Earlier")
        .clicked()
    {
        state.page += 1;
    }
    if state.page > 0 {
        // The forward arrow is the back chevron mirrored.
        let r = slot(nav.right() - 10.0);
        let resp = ui
            .interact(r, ui.id().with("later"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Later");
        let c = if resp.hovered() { theme.text } else { theme.text_dim };
        let s = Stroke::new(1.6_f32, c);
        let m = r.center();
        ui.painter().line_segment([m + vec2(-4.0, -8.0), m + vec2(4.0, 0.0)], s);
        ui.painter().line_segment([m + vec2(4.0, 0.0), m + vec2(-4.0, 8.0)], s);
        if resp.clicked() {
            state.page -= 1;
        }
    }

    let grid_w = TILE * 7.0 + TILE_GAP * 6.0;
    let small = FontId::proportional(10.0);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0 + TILE), Sense::hover());
    let origin = pos2(area.center().x - grid_w / 2.0, area.top() + 18.0);
    for dow in 0..7u64 {
        let day = first + Days::new(dow);
        let x = origin.x + dow as f32 * (TILE + TILE_GAP);
        let label_color = if day == today { theme.text } else { theme.text_dim };
        ui.painter().text(
            pos2(x + TILE / 2.0, origin.y - 5.0),
            Align2::CENTER_BOTTOM,
            day.format("%a").to_string(),
            small.clone(),
            label_color,
        );
        let rect = Rect::from_min_size(pos2(x, origin.y), vec2(TILE, TILE));
        if day > today {
            // Days still to come: a faint empty tile.
            ui.painter().rect_filled(rect, RADIUS, shade(theme, 0).gamma_multiply(0.45));
            ink_outline(ui.painter(), rect.shrink(0.5), RADIUS, theme);
            continue;
        }
        let stat = history.day(day);
        ui.painter().rect_filled(rect, RADIUS, shade(theme, stat.sessions));
        ink_outline(ui.painter(), rect.shrink(0.5), RADIUS, theme);
        if day == today {
            ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.5_f32, theme.text), egui::StrokeKind::Outside);
        }
        ui.interact(rect, ui.id().with(("day", dow)), Sense::hover())
            .on_hover_text(tooltip(day, stat.sessions, stat.minutes));
    }

    // Legend.
    ui.add_space(8.0);
    let (legend, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    let mut x = legend.center().x + grid_w / 2.0;
    let y = legend.center().y;
    let p = ui.painter();
    let more = p.text(pos2(x, y), Align2::RIGHT_CENTER, "More", small.clone(), theme.text_dim);
    x = more.left() - 5.0;
    for sessions in [8, 5, 3, 1, 0] {
        x -= CELL;
        let cell = Rect::from_min_size(pos2(x, y - CELL / 2.0), vec2(CELL, CELL));
        p.rect_filled(cell, 3.0, shade(theme, sessions));
        ink_outline(p, cell.shrink(0.5), 3.0, theme);
        x -= STEP - CELL;
    }
    p.text(pos2(x - 2.0, y), Align2::RIGHT_CENTER, "Less", small, theme.text_dim);

    // Summary tiles.
    ui.add_space(8.0);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    let gap = 6.0;
    let w = (row.width() - gap * 3.0) / 4.0;
    for (i, (value, label)) in tiles.iter().enumerate() {
        let tile = Rect::from_min_size(row.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, row.height()));
        let p = ui.painter();
        p.rect_filled(tile, 7.0, mix(theme.surface, theme.muted, 0.16));
        ink_outline(p, tile, 7.0, theme);
        p.text(tile.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, value, FontId::proportional(21.0), theme.text);
        p.text(tile.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), theme.text_dim);
    }

    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} of focus across {} sessions",
                duration(total.minutes),
                total.sessions
            ))
            .size(12.5)
            .color(theme.text_dim),
        );
    });

    goal_history(ui, state, core, theme);
}

/// The goals ticked off so far, newest day first.
fn goal_history(ui: &mut Ui, state: &State, core: &Core, theme: &Theme) {
    ui.add_space(14.0);
    if state.show_goals {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    heading(ui, "Goal history", theme);
    if core.goals.days.is_empty() {
        ui.vertical_centered(|ui| {
            let hint = egui::RichText::new("Goals you tick off will be listed here.");
            ui.label(hint.size(12.5).color(theme.text_dim));
        });
        return;
    }
    for (day, goals) in core.goals.days.iter().rev() {
        let date = day.format("%-d %B %y").to_string();
        ui.label(egui::RichText::new(date).size(12.0).color(theme.text_dim));
        for goal in goals {
            ui.horizontal_top(|ui| {
                // A bullet, then the goal wrapped beside it.
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.5, theme.focus);
                ui.add(egui::Label::new(egui::RichText::new(goal).size(14.0).color(theme.text)).wrap());
            });
        }
        ui.add_space(8.0);
    }
}

/// The day `today`'s week began on, for weeks starting on `first`.
fn week_start_of(today: NaiveDate, first: Weekday) -> NaiveDate {
    let back = (7 + today.weekday().num_days_from_monday() - first.num_days_from_monday()) % 7;
    today - Days::new(back as u64)
}

#[allow(dead_code)]
fn days_after(day: Weekday, n: u64) -> Weekday {
    (0..n % 7).fold(day, |d, _| d.succ())
}

/// The first day of the week in the system settings, looked up once.
/// Sunday when the system doesn't say.
fn first_weekday() -> Weekday {
    static FIRST: OnceLock<Weekday> = OnceLock::new();
    *FIRST.get_or_init(|| system_first_weekday().unwrap_or(Weekday::Sun))
}

#[cfg(target_os = "macos")]
fn system_first_weekday() -> Option<Weekday> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // NSCalendar counts 1 = Sunday … 7 = Saturday.
    let day: usize = unsafe {
        let calendar: *mut AnyObject = msg_send![class!(NSCalendar), currentCalendar];
        if calendar.is_null() {
            return None;
        }
        msg_send![calendar, firstWeekday]
    };
    (1..=7).contains(&day).then(|| days_after(Weekday::Sun, day as u64 - 1))
}

#[cfg(windows)]
fn system_first_weekday() -> Option<Weekday> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK};
    // Windows counts "0" = Monday … "6" = Sunday.
    let mut buf = [0u16; 4];
    let len = unsafe {
        GetLocaleInfoEx(std::ptr::null(), LOCALE_IFIRSTDAYOFWEEK, buf.as_mut_ptr(), buf.len() as i32)
    };
    let text = String::from_utf16_lossy(buf.get(..(len as usize).checked_sub(1)?)?);
    let day: u64 = text.trim().parse().ok().filter(|d| *d < 7)?;
    Some(days_after(Weekday::Mon, day))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_first_weekday() -> Option<Weekday> {
    // glibc: `first_weekday` counts from `week-1stday` (1 = that day).
    let out = std::process::Command::new("locale").args(["week-1stday", "first_weekday"]).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines();
    let base = NaiveDate::parse_from_str(lines.next()?.trim(), "%Y%m%d").ok()?;
    let n: u64 = lines.next()?.trim().parse().ok().filter(|n| (1..=7).contains(n))?;
    Some(days_after(base.weekday(), n - 1))
}

#[cfg(not(any(unix, windows)))]
fn system_first_weekday() -> Option<Weekday> {
    None
}

/// "25 min", "2 h", "2 h 5 min". Shared with the leaderboard.
pub fn duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn tooltip(day: NaiveDate, sessions: u32, minutes: u32) -> String {
    let date = day.format("%a, %b %-d, %Y");
    match sessions {
        0 => format!("No focus sessions on {date}"),
        1 => format!("1 focus session ({}) on {date}", duration(minutes)),
        n => format!("{n} focus sessions ({}) on {date}", duration(minutes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_starts_on_the_chosen_day() {
        let thu = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        assert_eq!(week_start_of(thu, Weekday::Sun), d(25));
        assert_eq!(week_start_of(thu, Weekday::Mon), d(26));
        assert_eq!(week_start_of(thu, Weekday::Sat), d(24));
        assert_eq!(week_start_of(thu, Weekday::Thu), d(29));
    }
}
