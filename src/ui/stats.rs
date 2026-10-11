//! Activity page: today's minutes of focus, a contribution-style heat map of
//! completed focus sessions shaded from the theme's colours, and summary tiles.

use super::widgets::{ink_outline, heading, icon_button, Icon};
use crate::core::Core;
use crate::theme::{mix, Theme};
use chrono::{Datelike, Days, NaiveDate};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

const WEEKS: u64 = 18;
const CELL: f32 = 13.0;
const STEP: f32 = 16.0;
const LABEL_W: f32 = 28.0;
/// Sessions needed to reach each shade above "empty".
const LEVELS: [u32; 4] = [1, 3, 5, 8];
/// Colour of the flame beside today's minutes.
const FLAME: Color32 = Color32::from_rgb(0xf5, 0x7c, 0x1f);

#[derive(Default)]
pub struct State {
    /// How many pages of `WEEKS` back from the current week the map is showing.
    page: u64,
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

    let week_start = today - Days::new(today.weekday().num_days_from_sunday() as u64);
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
    let (flame_w, gap) = (p.layout_no_wrap("🔥".into(), figure.clone(), FLAME).size().x, 6.0);
    let minutes_w = p.layout_no_wrap(minutes.clone(), figure.clone(), theme.text).size().x;
    let left = pos2(hero.center().x - (flame_w + gap + minutes_w) / 2.0, hero.top() + 15.0);
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

    // The grid covers WEEKS columns ending with the week `page` pages back.
    let last_week = week_start - Days::new(7 * WEEKS * state.page);
    let first = last_week - Days::new(7 * (WEEKS - 1));
    let last = (last_week + Days::new(6)).min(today);

    let (nav, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let slot = |x: f32| Rect::from_center_size(pos2(x, nav.center().y), vec2(26.0, 26.0));
    ui.painter().text(
        nav.center(),
        Align2::CENTER_CENTER,
        format!("{} – {}", first.format("%b %-d, %Y"), last.format("%b %-d, %Y")),
        FontId::proportional(12.5),
        theme.text_dim,
    );
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

    let grid_w = LABEL_W + STEP * WEEKS as f32 - (STEP - CELL);
    let (area, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), 16.0 + STEP * 7.0),
        Sense::hover(),
    );
    let origin = pos2(area.center().x - grid_w / 2.0 + LABEL_W, area.top() + 16.0);
    let small = FontId::proportional(10.0);
    for (row, name) in [(1, "Mon"), (3, "Wed"), (5, "Fri")] {
        ui.painter().text(
            pos2(origin.x - 6.0, origin.y + row as f32 * STEP + CELL / 2.0),
            Align2::RIGHT_CENTER,
            name,
            small.clone(),
            theme.text_dim,
        );
    }

    let mut prev_month = 0;
    for week in 0..WEEKS {
        let week_first = first + Days::new(7 * week);
        if week_first.month() != prev_month {
            // Skip the label on the first column unless the month starts there,
            // so it doesn't collide with the next one.
            if week > 0 || week_first.day() <= 7 {
                ui.painter().text(
                    pos2(origin.x + week as f32 * STEP, origin.y - 4.0),
                    Align2::LEFT_BOTTOM,
                    week_first.format("%b").to_string(),
                    small.clone(),
                    theme.text_dim,
                );
            }
            prev_month = week_first.month();
        }
        for dow in 0..7 {
            let day = week_first + Days::new(dow);
            if day > today {
                continue;
            }
            let rect = Rect::from_min_size(
                origin + vec2(week as f32 * STEP, dow as f32 * STEP),
                vec2(CELL, CELL),
            );
            let stat = history.day(day);
            ui.painter().rect_filled(rect, 3.0, shade(theme, stat.sessions));
            ink_outline(ui.painter(), rect.shrink(0.5), 3.0, theme);
            if day == today {
                ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme.text), egui::StrokeKind::Outside);
            }
            ui.interact(rect.expand(1.5), ui.id().with(("day", week, dow)), Sense::hover())
                .on_hover_text(tooltip(day, stat.sessions, stat.minutes));
        }
    }

    // Legend.
    ui.add_space(6.0);
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
}

fn duration(minutes: u32) -> String {
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
