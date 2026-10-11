//! The main card: progress ring, countdown and transport controls.

use super::widgets::{icon_button, ink_outline, paint_icon, spaced_text, Icon};
use super::{fonts, pencil, sketch};
use crate::core::{Core, Phase};
use crate::theme::{luminance, mix, Style, Theme, EDITORIAL_INK, INK};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Rect, RichText, Sense, Shape, Stroke, Ui};
use std::f32::consts::TAU;
use std::time::Instant;

pub fn show(ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
    let now = Instant::now();
    let cx = rect.center().x;
    let color = core.phase.color(theme);

    if core.phase == Phase::Focus || !core.goal.is_empty() {
        goal_box(ui, rect, core, theme);
    }

    // Progress ring: a faint full track with the remaining share drawn over it,
    // emptying clockwise from the top.
    let center = pos2(cx, rect.top() + 218.0);
    let radius = 108.0;
    let p = ui.painter().clone();
    let ring = |share: f32, width: f32, color| {
        let n = (share * 180.0).ceil().max(1.0) as usize;
        let mut points: Vec<_> = (0..=n)
            .map(|i| {
                // The arc always ends at 12 o'clock; its start sweeps clockwise as time passes.
                let a = -TAU / 4.0 + TAU * (1.0 - share) + TAU * share * i as f32 / n as f32;
                center + radius * vec2(a.cos(), a.sin())
            })
            .collect();
        let stroke = Stroke::new(width, color);
        if share >= 1.0 {
            points.pop();
            p.add(Shape::closed_line(points, stroke));
        } else {
            p.add(Shape::line(points, stroke));
        }
    };
    let left = core.fraction_left(now);
    let top = -TAU / 4.0;
    let spent_end = top + TAU * (1.0 - left);
    match theme.style {
        Style::Standard => {
            ring(1.0, 9.0, mix(theme.background, theme.muted, 0.3));
            if left > 0.0 {
                ring(left, 9.0, color);
            }
        }
        Style::PencilArt => {
            // The ring is an inked tomato. The spent part of the band is
            // cross-hatched, the rest is shaded with a few darker strokes.
            let (ink, band) = (INK, (radius - 4.5, radius + 4.5));
            pencil::shadow(&p, pos2(cx, center.y + radius + 8.5), 170.0, ink, 11);
            ring(1.0, 9.0, mix(theme.background, theme.muted, 0.55));
            let hatch = Stroke::new(1.0_f32, ink.gamma_multiply(0.75));
            pencil::hatch_band(&p, center, band, (top, spent_end), 0.055, 0.07, hatch, 3);
            pencil::hatch_band(&p, center, band, (top, spent_end), 0.11, -0.07, hatch, 4);
            if left > 0.0 {
                ring(left, 9.0, color);
                let shade = Stroke::new(1.0_f32, mix(color, ink, 0.45));
                pencil::hatch_band(&p, center, band, (spent_end, top + TAU), 0.09, 0.05, shade, 5);
            }
            pencil::ink_circle(&p, center, radius + 5.0, 1.8, ink, 1);
            pencil::ink_circle(&p, center, radius - 5.0, 1.5, ink, 2);
            // The leaves ride on the tip of the remaining arc as it moves clockwise.
            let tip = center + (radius + 3.0) * vec2(spent_end.cos(), spent_end.sin());
            pencil::calyx(&p, tip, spent_end - top, theme.short_break, ink);
        }
        Style::Sketch => {
            // A hand-sketched chart: a dashed frame and grid rings, with the
            // time left as the one series over a wash. Breaks are drawn dashed,
            // like a comparison series, so they differ from focus by more than hue.
            let grid = Stroke::new(1.0_f32, mix(theme.background, theme.muted, 0.5));
            let full = top + TAU;
            sketch::arc(&p, center, radius + 12.0, top, full, Stroke::new(1.5_f32, theme.muted), Some((8.0, 5.0)), 1);
            sketch::arc(&p, center, radius, top, full, grid, Some((6.0, 4.0)), 2);
            sketch::arc(&p, center, radius - 16.0, top, full, grid, Some((6.0, 4.0)), 3);
            if left > 0.0 {
                let wash = Stroke::new(16.0_f32, color.gamma_multiply(0.16));
                sketch::arc(&p, center, radius - 8.0, full, spent_end, wash, None, 2);
                // Dashes are laid from the fixed end, so they don't crawl as time passes.
                let dash = (core.phase != Phase::Focus).then_some((8.0, 5.0));
                sketch::arc(&p, center, radius, full, spent_end, Stroke::new(3.5_f32, color), dash, 2);
                for a in [full, spent_end] {
                    let point = center + (radius + sketch::wobble(a, 2)) * vec2(a.cos(), a.sin());
                    p.circle_filled(point, 6.5, theme.background);
                    p.circle_filled(point, 4.5, color);
                }
            }
        }
        Style::Editorial => {
            // A flat band on paper with clean ink outlines; no shading.
            let outline = Stroke::new(2.0_f32, EDITORIAL_INK);
            ring(1.0, 12.0, theme.surface);
            if left > 0.0 {
                ring(left, 12.0, color);
            }
            p.circle_stroke(center, radius + 6.0, outline);
            p.circle_stroke(center, radius - 6.0, outline);
            if left > 0.0 && left < 1.0 {
                for a in [top, spent_end] {
                    let out = vec2(a.cos(), a.sin());
                    p.line_segment([center + (radius - 6.0) * out, center + (radius + 6.0) * out], outline);
                }
            }
        }
    }

    // Countdown, one glyph per fixed-width cell so the digits don't jitter.
    // Rounded to the nearest second, so a round visibly counts down to 00:00
    // before it ends rather than ending on 00:01.
    let secs = core.remaining(now).as_secs_f32().round() as u64;
    let text = format!("{:02}:{:02}", secs / 60, secs % 60);
    let face = match theme.style {
        Style::Standard | Style::PencilArt => fonts::LIGHT,
        Style::Sketch => fonts::HAND,
        Style::Editorial => fonts::SERIF,
    };
    let font = FontId::new(54.0, egui::FontFamily::Name(face.into()));
    let cell = |ch: char| if ch == ':' { 20.0 } else { 31.0 };
    let width: f32 = text.chars().map(cell).sum();
    let mut x = cx - width / 2.0;
    for ch in text.chars() {
        let w = cell(ch);
        p.text(
            pos2(x + w / 2.0, center.y - 10.0),
            Align2::CENTER_CENTER,
            ch,
            font.clone(),
            theme.text,
        );
        x += w;
    }
    let label = pos2(cx, center.y + 44.0);
    match theme.style {
        Style::Standard | Style::PencilArt => {
            spaced_text(&p, label, core.phase.label(), FontId::proportional(15.0), theme.text, 2.2);
        }
        Style::Sketch => {
            let font = FontId::new(16.0, egui::FontFamily::Name(fonts::HAND.into()));
            spaced_text(&p, label, core.phase.label(), font, theme.text_dim, 1.3);
        }
        Style::Editorial => {
            // Sentence case, as everywhere in this style.
            let name = core.phase.label();
            let name = format!("{}{}", &name[..1], name[1..].to_lowercase());
            p.text(label, Align2::CENTER_CENTER, name, FontId::proportional(16.0), theme.text_dim);
        }
    }

    // Play / pause.
    let play = Rect::from_center_size(pos2(cx, rect.top() + 378.0), vec2(56.0, 56.0));
    let resp = ui
        .interact(play, ui.id().with("play"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = |from, to, idle: f32, hovered: f32| mix(from, to, if resp.hovered() { hovered } else { idle });
    match theme.style {
        Style::Standard => {
            let outline = if resp.hovered() { theme.text } else { theme.muted };
            p.circle_stroke(play.center(), 27.0, Stroke::new(1.5_f32, outline));
        }
        Style::PencilArt => {
            p.circle_filled(play.center(), 27.0, hover(theme.background, theme.text, 0.1, 0.22));
            pencil::ink_circle(&p, play.center(), 27.0, 1.8, INK, 7);
        }
        Style::Sketch => {
            // A quiet grey button with a hairline; controls are never coloured.
            p.circle_filled(play.center(), 27.0, hover(theme.background, theme.muted, 0.1, 0.22));
            p.circle_stroke(play.center(), 27.0, Stroke::new(1.0_f32, mix(theme.background, theme.muted, 0.45)));
        }
        Style::Editorial => {
            p.circle_filled(play.center(), 27.0, hover(theme.surface, theme.accent, 0.0, 0.3));
            p.circle_stroke(play.center(), 27.0, Stroke::new(2.0_f32, EDITORIAL_INK));
        }
    }
    let icon = if core.running { Icon::Pause } else { Icon::Play };
    paint_icon(&p, play.center(), icon, theme.text);
    let space = ui.input(|i| i.key_pressed(egui::Key::Space)) && !ui.ctx().wants_keyboard_input();
    if resp.clicked() || space {
        core.toggle(now);
    }

    // Restart and skip sit either side of play; the round counter, reset and
    // volume share the same three columns in a row below.
    let column = |i: f32, y: f32| pos2(cx + 60.0 * i, rect.top() + y);
    let slot = |i: f32, y: f32| Rect::from_center_size(column(i, y), vec2(32.0, 32.0));
    if icon_button(ui, slot(-1.0, 378.0), Icon::Previous, theme, false)
        .on_hover_text("Restart this round")
        .clicked()
    {
        core.restart_phase();
    }
    if icon_button(ui, slot(1.0, 378.0), Icon::Skip, theme, false)
        .on_hover_text("Skip to the next round")
        .clicked()
    {
        core.skip(now);
    }

    // Round counter: "current/total (focus sessions completed today)".
    let row = 438.0;
    let today = core.history.day(chrono::Local::now().date_naive()).sessions;
    let fonts = (FontId::proportional(15.0), FontId::proportional(11.0));
    let rounds = format!("{}/{}", core.round, core.cfg.rounds);
    let sessions = format!("({today})");
    let widths = (
        p.layout_no_wrap(rounds.clone(), fonts.0.clone(), theme.text).size().x,
        p.layout_no_wrap(sessions.clone(), fonts.1.clone(), theme.text_dim).size().x,
    );
    let base = column(-1.0, row + 8.0) - vec2((widths.0 + 4.0 + widths.1) / 2.0, 0.0);
    let r = p.text(base, Align2::LEFT_BOTTOM, rounds, fonts.0, theme.text);
    let r2 = p.text(r.right_bottom() + vec2(4.0, -1.0), Align2::LEFT_BOTTOM, sessions, fonts.1, theme.text_dim);
    ui.interact(r.union(r2), ui.id().with("rounds"), Sense::hover())
        .on_hover_text(format!(
            "Round {} of {} · {today} focus sessions today",
            core.round, core.cfg.rounds
        ));

    let reset = Rect::from_center_size(column(0.0, row), vec2(52.0, 26.0));
    let resp = ui
        .interact(reset, ui.id().with("reset"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Back to round 1");
    let c = if resp.hovered() { theme.text } else { theme.text_dim };
    p.text(reset.center(), Align2::CENTER_CENTER, "Reset", FontId::proportional(14.0), c);
    if resp.clicked() {
        core.reset();
    }

    let vol = if core.cfg.muted { Icon::Muted } else { Icon::Volume };
    if icon_button(ui, slot(1.0, row), vol, theme, false)
        .on_hover_text(if core.cfg.muted { "Unmute" } else { "Mute" })
        .clicked()
    {
        core.cfg.muted = !core.cfg.muted;
    }
}

/// The goal of the current focus round: a text box above the ring. Once the
/// round has finished the goal is fixed and gets a box on its left to tick it
/// off in.
fn goal_box(ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
    let row = Rect::from_center_size(pos2(rect.center().x, rect.top() + 72.0), vec2(236.0, 28.0));
    let tick = Rect::from_min_max(row.min, pos2(row.left() + 28.0, row.bottom()));
    let finished = core.phase != Phase::Focus;
    let frame = if finished { row.with_min_x(tick.right() + 6.0) } else { row };
    let id = ui.id().with("goal");
    let p = ui.painter().clone();
    let edge = |on: bool, color| {
        let idle = mix(theme.background, theme.muted, 0.45);
        Stroke::new(1.0_f32, if on { color } else { idle })
    };
    let boxed = |rect: Rect, stroke: Stroke| {
        p.rect_filled(rect, 8.0, theme.surface);
        if theme.ink().is_some() {
            ink_outline(&p, rect, 8.0, theme);
        } else {
            p.rect_stroke(rect, 8.0, stroke, egui::StrokeKind::Inside);
        }
    };
    let font = FontId::proportional(14.0);
    // A row of text has more room above its letters than below; nudged up, the
    // letters sit in the middle of the box.
    let text_center = frame.center() - vec2(0.0, 1.0);

    if !finished {
        boxed(frame, edge(ui.memory(|m| m.has_focus(id)), theme.accent));
        // The field is exactly one row tall and centred in its box by hand;
        // left to itself the text sits against the top.
        let row_height = ui.fonts(|f| f.row_height(&font));
        let field = Rect::from_center_size(text_center, vec2(frame.width() - 20.0, row_height));
        let edit = egui::TextEdit::singleline(&mut core.goal)
            .id(id)
            .frame(false)
            .hint_text(RichText::new("Goal for this focus round").color(theme.text_dim))
            .font(font)
            .text_color(theme.text)
            .horizontal_align(Align::Center)
            .margin(vec2(0.0, 0.0))
            .desired_width(field.width())
            .char_limit(80);
        ui.put(field, edit);
        return;
    }

    // The goal on one line, cut short with an ellipsis if it doesn't fit.
    boxed(frame, edge(false, theme.accent));
    let mut job = egui::text::LayoutJob::simple_singleline(core.goal.trim().to_owned(), font, theme.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width(frame.width() - 20.0);
    let galley = p.layout_job(job);
    p.galley(text_center - galley.size() / 2.0, galley, theme.text);

    // Only a goal that has been typed can be ticked off.
    let has_goal = !core.goal.trim().is_empty();
    let sense = if has_goal { Sense::click() } else { Sense::hover() };
    let mut resp = ui.interact(tick, ui.id().with("goal-tick"), sense);
    if has_goal {
        resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        core.set_goal_achieved(!core.goal_achieved);
    }
    let achieved = core.goal_achieved;
    // A green that reads on the box's fill, light or dark.
    let green = match luminance(theme.surface) < 0.5 {
        true => Color32::from_rgb(0x22, 0xc5, 0x5e),
        false => Color32::from_rgb(0x16, 0xa3, 0x4a),
    };
    boxed(tick, edge(achieved || (has_goal && resp.hovered()), green));
    // The tick is drawn in, stroke by stroke, when the box is ticked.
    let drawn = ui.ctx().animate_bool_with_time(resp.id, achieved, 0.25);
    if drawn > 0.0 {
        let c = tick.center();
        let (start, bend, end) = (c + vec2(-6.5, 0.5), c + vec2(-2.0, 5.0), c + vec2(7.0, -5.5));
        let stroke = Stroke::new(2.6_f32, green);
        // The short stroke takes the first third of the time.
        let (down, up) = ((drawn * 3.0).min(1.0), ((drawn * 3.0 - 1.0) / 2.0).clamp(0.0, 1.0));
        let mut points = vec![start, start + (bend - start) * down];
        if up > 0.0 {
            points.push(bend + (end - bend) * up);
        }
        p.add(Shape::line(points, stroke));
    }
    resp.on_hover_text(match (has_goal, achieved) {
        (false, _) => "Type a goal to tick it off",
        (true, false) => "Tick off your goal",
        (true, true) => "Goal achieved",
    });
}
