//! The main card: progress ring, countdown and transport controls.

use super::widgets::{icon_button, ink_outline, paint_icon, spaced_text, Icon};
use super::{fonts, pencil, sketch};
use crate::core::{Core, GoalStatus, Phase};
use crate::theme::{luminance, mix, Style, Theme, EDITORIAL_INK, INK};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Rect, RichText, Sense, Shape, Stroke, Ui};
use std::f32::consts::TAU;
use std::time::Instant;

pub fn show(ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
    let now = Instant::now();
    let cx = rect.center().x;
    let color = core.phase.color(theme);

    if core.phase == Phase::Focus || core.goal_status != GoalStatus::Open {
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
    let play = Rect::from_center_size(pos2(cx, rect.top() + 374.0), vec2(56.0, 56.0));
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

    // Round counter: "current/total (focus sessions completed today)".
    let base = pos2(rect.left() + 18.0, rect.bottom() - 62.0);
    let today = core.history.day(chrono::Local::now().date_naive()).sessions;
    let r = p.text(
        base,
        Align2::LEFT_BOTTOM,
        format!("{}/{}", core.round, core.cfg.rounds),
        FontId::proportional(18.0),
        theme.text,
    );
    let r2 = p.text(
        r.right_bottom() + vec2(4.0, -1.0),
        Align2::LEFT_BOTTOM,
        format!("({today})"),
        FontId::proportional(11.0),
        theme.text_dim,
    );
    ui.interact(r.union(r2), ui.id().with("rounds"), Sense::hover())
        .on_hover_text(format!(
            "Round {} of {} · {today} focus sessions today",
            core.round, core.cfg.rounds
        ));

    let reset = Rect::from_min_size(pos2(rect.left() + 14.0, rect.bottom() - 46.0), vec2(48.0, 26.0));
    let resp = ui
        .interact(reset, ui.id().with("reset"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Back to round 1");
    let c = if resp.hovered() { theme.text } else { theme.text_dim };
    p.text(reset.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, "Reset", FontId::proportional(14.0), c);
    if resp.clicked() {
        core.reset();
    }

    // Transport controls, bottom right.
    let slot = |i: f32| {
        Rect::from_center_size(
            pos2(rect.right() - 30.0 - 38.0 * i, rect.bottom() - 46.0),
            vec2(32.0, 32.0),
        )
    };
    let vol = if core.cfg.muted { Icon::Muted } else { Icon::Volume };
    if icon_button(ui, slot(0.0), vol, theme, false)
        .on_hover_text(if core.cfg.muted { "Unmute" } else { "Mute" })
        .clicked()
    {
        core.cfg.muted = !core.cfg.muted;
    }
    if icon_button(ui, slot(1.0), Icon::Skip, theme, false)
        .on_hover_text("Skip to the next round")
        .clicked()
    {
        core.skip(now);
    }
    if icon_button(ui, slot(2.0), Icon::Restart, theme, false)
        .on_hover_text("Restart this round")
        .clicked()
    {
        core.restart_phase();
    }
}

/// The goal of the current focus round: a text box above the ring. Once the
/// round has finished the goal is fixed and gets a box to tick it off in.
fn goal_box(ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
    let row = Rect::from_center_size(pos2(rect.center().x, rect.top() + 72.0), vec2(236.0, 28.0));
    let finished = core.goal_status != GoalStatus::Open;
    let frame = match finished {
        true => row.with_max_x(row.right() - 34.0),
        false => row,
    };
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

    if !finished {
        boxed(frame, edge(ui.memory(|m| m.has_focus(id)), theme.accent));
        let edit = egui::TextEdit::singleline(&mut core.goal)
            .id(id)
            .frame(false)
            .hint_text(RichText::new("Goal for this focus round").color(theme.text_dim))
            .font(font)
            .text_color(theme.text)
            .horizontal_align(Align::Center)
            .vertical_align(Align::Center)
            .margin(vec2(10.0, 0.0))
            .char_limit(80);
        ui.put(frame, edit);
        return;
    }

    // The goal on one line, cut short with an ellipsis if it doesn't fit.
    boxed(frame, edge(false, theme.accent));
    let mut job = egui::text::LayoutJob::simple_singleline(core.goal.trim().to_owned(), font, theme.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width(frame.width() - 20.0);
    let galley = p.layout_job(job);
    p.galley(frame.center() - galley.size() / 2.0, galley, theme.text);

    let tick = Rect::from_min_max(pos2(row.right() - 28.0, row.top()), row.max);
    let resp = ui
        .interact(tick, ui.id().with("goal-tick"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        core.goal_status = match core.goal_status {
            GoalStatus::Achieved => GoalStatus::Finished,
            _ => GoalStatus::Achieved,
        };
    }
    let achieved = core.goal_status == GoalStatus::Achieved;
    // A green that reads on the box's fill, light or dark.
    let green = match luminance(theme.surface) < 0.5 {
        true => Color32::from_rgb(0x22, 0xc5, 0x5e),
        false => Color32::from_rgb(0x16, 0xa3, 0x4a),
    };
    boxed(tick, edge(achieved || resp.hovered(), green));
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
    resp.on_hover_text(if achieved { "Goal achieved" } else { "Tick off your goal" });
}
