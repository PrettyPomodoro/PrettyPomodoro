//! The main card: progress ring, countdown and transport controls.

use super::widgets::{icon_button, paint_icon, spaced_text, Icon};
use crate::core::Core;
use crate::theme::{mix, Theme};
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Sense, Shape, Stroke, Ui};
use std::f32::consts::TAU;
use std::time::Instant;

pub fn show(ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
    let now = Instant::now();
    let cx = rect.center().x;
    let color = core.phase.color(theme);

    // Progress ring: a faint full track with the remaining share drawn over it,
    // emptying clockwise from the top.
    let center = pos2(cx, rect.top() + 212.0);
    let radius = 108.0;
    let p = ui.painter().clone();
    let ring = |share: f32, color| {
        let n = (share * 180.0).ceil().max(1.0) as usize;
        let mut points: Vec<_> = (0..=n)
            .map(|i| {
                // The arc always ends at 12 o'clock; its start sweeps clockwise as time passes.
                let a = -TAU / 4.0 + TAU * (1.0 - share) + TAU * share * i as f32 / n as f32;
                center + radius * vec2(a.cos(), a.sin())
            })
            .collect();
        let stroke = Stroke::new(9.0_f32, color);
        if share >= 1.0 {
            points.pop();
            p.add(Shape::closed_line(points, stroke));
        } else {
            p.add(Shape::line(points, stroke));
        }
    };
    ring(1.0, mix(theme.background, theme.muted, 0.3));
    let left = core.fraction_left(now);
    if left > 0.0 {
        ring(left, color);
    }

    // Countdown, one glyph per fixed-width cell so the digits don't jitter.
    let secs = core.remaining(now).as_secs_f32().ceil() as u64;
    let text = format!("{:02}:{:02}", secs / 60, secs % 60);
    let font = FontId::new(54.0, egui::FontFamily::Name(super::fonts::LIGHT.into()));
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
    spaced_text(&p, pos2(cx, center.y + 44.0), core.phase.label(), 15.0, theme.text, 2.2);

    // Play / pause.
    let play = Rect::from_center_size(pos2(cx, rect.top() + 370.0), vec2(56.0, 56.0));
    let resp = ui
        .interact(play, ui.id().with("play"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let outline = if resp.hovered() { theme.text } else { theme.muted };
    p.circle_stroke(play.center(), 27.0, Stroke::new(1.5_f32, outline));
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
