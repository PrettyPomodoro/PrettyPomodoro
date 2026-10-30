//! Small custom-painted widgets and icons shared by the pages.

use crate::theme::{mix, Theme};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape,
    Stroke, Ui,
};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Back,
    Minimize,
    Close,
    Play,
    Pause,
    Skip,
    Volume,
    Muted,
    Clock,
    Sliders,
    Palette,
    Calendar,
    Cloud,
    Info,
    Edit,
    Trash,
    Previous,
    Trophy,
    Person,
}

/// A clickable icon. Dim by default, full text colour when hovered or `active`.
pub fn icon_button(ui: &mut Ui, rect: Rect, icon: Icon, theme: &Theme, active: bool) -> Response {
    let resp = ui
        .interact(rect, ui.id().with(("icon", icon, rect.min.x as i32)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if resp.hovered() || active {
        theme.text
    } else {
        theme.text_dim
    };
    paint_icon(ui.painter(), rect.center(), icon, color);
    resp
}

pub fn paint_icon(p: &Painter, c: Pos2, icon: Icon, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([c + vec2(a.0, a.1), c + vec2(b.0, b.1)], s);
    };
    let poly = |pts: &[(f32, f32)]| {
        let pts = pts.iter().map(|&(x, y)| c + vec2(x, y)).collect();
        p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    };
    let arc = |center: Pos2, r: f32, from: f32, to: f32| {
        let n = 16;
        let pts = (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                center + r * vec2(a.cos(), a.sin())
            })
            .collect();
        p.add(Shape::line(pts, s));
    };
    match icon {
        Icon::Back => {
            line((4.0, -8.0), (-4.0, 0.0));
            line((-4.0, 0.0), (4.0, 8.0));
        }
        Icon::Minimize => line((-7.0, 0.0), (7.0, 0.0)),
        Icon::Close => {
            line((-7.0, -7.0), (7.0, 7.0));
            line((-7.0, 7.0), (7.0, -7.0));
        }
        Icon::Play => poly(&[(-4.0, -7.0), (7.0, 0.0), (-4.0, 7.0)]),
        Icon::Pause => {
            p.rect_filled(Rect::from_center_size(c + vec2(-4.0, 0.0), vec2(3.5, 14.0)), 1.0, color);
            p.rect_filled(Rect::from_center_size(c + vec2(4.0, 0.0), vec2(3.5, 14.0)), 1.0, color);
        }
        Icon::Skip => {
            poly(&[(-6.0, -7.0), (4.0, 0.0), (-6.0, 7.0)]);
            p.rect_filled(Rect::from_center_size(c + vec2(6.0, 0.0), vec2(2.0, 14.0)), 0.0, color);
        }
        Icon::Volume | Icon::Muted => {
            p.rect_filled(Rect::from_center_size(c + vec2(-6.5, 0.0), vec2(4.0, 6.0)), 0.0, color);
            poly(&[(-5.0, -3.0), (0.0, -7.0), (0.0, 7.0), (-5.0, 3.0)]);
            if icon == Icon::Volume {
                arc(c, 4.5, -0.9, 0.9);
                arc(c, 8.5, -0.9, 0.9);
            } else {
                line((3.5, -3.5), (9.5, 3.5));
                line((3.5, 3.5), (9.5, -3.5));
            }
        }
        Icon::Clock => {
            p.circle_stroke(c, 8.5, s);
            line((0.0, -5.0), (0.0, 0.0));
            line((0.0, 0.0), (3.5, 2.5));
        }
        Icon::Sliders => {
            for (y, knob) in [(-6.0, 3.0), (0.0, -4.0), (6.0, 1.0)] {
                line((-9.0, y), (9.0, y));
                p.circle_filled(c + vec2(knob, y), 2.8, color);
            }
        }
        Icon::Palette => {
            p.circle_stroke(c, 8.5, s);
            for a in [3.6f32, 4.7, 5.8] {
                p.circle_filled(c + 4.8 * vec2(a.cos(), a.sin()), 1.5, color);
            }
            p.circle_filled(c + vec2(3.5, 3.5), 2.4, color);
        }
        Icon::Calendar => {
            let r = Rect::from_center_size(c + vec2(0.0, 1.0), vec2(17.0, 15.0));
            p.rect_stroke(r, 2.0, s, egui::StrokeKind::Middle);
            line((-8.5, -2.5), (8.5, -2.5));
            line((-4.0, -9.0), (-4.0, -5.0));
            line((4.0, -9.0), (4.0, -5.0));
            for (x, y) in [(-4.0, 1.5), (0.0, 1.5), (4.0, 1.5), (-4.0, 5.0), (0.0, 5.0)] {
                p.circle_filled(c + vec2(x, y), 1.0, color);
            }
        }
        Icon::Cloud => {
            arc(c + vec2(-4.5, 2.0), 4.5, TAU * 0.25, TAU * 0.75);
            arc(c + vec2(-1.0, -1.5), 5.5, TAU * 0.52, TAU * 0.95);
            arc(c + vec2(5.0, 2.0), 4.5, -TAU * 0.25, TAU * 0.25);
            line((-4.5, 6.5), (5.0, 6.5));
        }
        Icon::Info => {
            p.circle_stroke(c, 8.5, s);
            p.circle_filled(c + vec2(0.0, -4.2), 1.2, color);
            line((0.0, -1.0), (0.0, 4.8));
        }
        Icon::Edit => {
            line((-5.0, 5.0), (3.0, -3.0));
            line((3.0, -3.0), (5.5, -0.5));
            line((5.5, -0.5), (-2.5, 7.5));
            line((-2.5, 7.5), (-5.5, 7.5));
            line((-5.5, 7.5), (-5.0, 5.0));
        }
        Icon::Trash => {
            line((-6.0, -4.0), (6.0, -4.0));
            line((-2.0, -6.5), (2.0, -6.5));
            line((-4.5, -4.0), (-3.5, 7.0));
            line((4.5, -4.0), (3.5, 7.0));
            line((-3.5, 7.0), (3.5, 7.0));
        }
        Icon::Previous => {
            poly(&[(6.0, -7.0), (-4.0, 0.0), (6.0, 7.0)]);
            p.rect_filled(Rect::from_center_size(c + vec2(-6.0, 0.0), vec2(2.0, 14.0)), 0.0, color);
        }
        Icon::Person => {
            // A head over shoulders, for an account with no picture yet.
            p.circle_stroke(c + vec2(0.0, -4.0), 4.2, s);
            arc(c + vec2(0.0, 9.5), 7.0, TAU * 0.55, TAU * 0.95);
        }
        Icon::Trophy => {
            // A cup tapering to a stem and a base, with a handle each side.
            // Handles first, so the cup is drawn over where they meet it.
            arc(c + vec2(-5.0, -5.5), 3.4, TAU * 0.2, TAU * 0.8);
            arc(c + vec2(5.0, -5.5), 3.4, -TAU * 0.3, TAU * 0.3);
            poly(&[(-6.0, -8.5), (6.0, -8.5), (4.0, -1.0), (-4.0, -1.0)]);
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 1.5), vec2(2.6, 5.0)), 0.0, color);
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 5.5), vec2(11.0, 2.6)), 0.8, color);
        }
    }
}

/// Text with extra space between letters, centred on `center`.
pub fn spaced_text(p: &Painter, center: Pos2, text: &str, font: FontId, color: Color32, gap: f32) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: gap,
            ..Default::default()
        },
    );
    let galley = p.layout_job(job);
    // The trailing letter carries a gap too; shift by half of it to stay centred.
    let pos = center - galley.size() / 2.0 + vec2(gap / 2.0, 0.0);
    p.galley(pos, galley, color);
}

/// Thin-track slider with a round knob. Returns true once the knob is let go.
pub fn slider(ui: &mut Ui, value: &mut u32, min: u32, max: u32, color: Color32, theme: &Theme) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
    let knob_r = 9.0;
    let (x0, x1) = (rect.left() + knob_r, rect.right() - knob_r);
    if let Some(pos) = resp.interact_pointer_pos() {
        let t = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        *value = min + (t * (max - min) as f32).round() as u32;
    }
    let released = resp.drag_stopped() || resp.clicked();
    let t = (*value - min) as f32 / (max - min).max(1) as f32;
    let x = x0 + t * (x1 - x0);
    let y = rect.center().y;
    let p = ui.painter();
    let track = mix(theme.surface, theme.muted, 0.35);
    p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(3.0_f32, track));
    p.line_segment([pos2(rect.left(), y), pos2(x, y)], Stroke::new(3.0_f32, color));
    let r = if resp.hovered() || resp.dragged() { knob_r + 1.0 } else { knob_r };
    p.circle_filled(pos2(x, y), r, color);
    if let Some(ink) = theme.ink() {
        p.circle_stroke(pos2(x, y), r, Stroke::new(1.5_f32, ink));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    released
}

/// A labelled on/off row: label on the left, switch on the right.
pub fn toggle_row(ui: &mut Ui, label: &str, on: &mut bool, theme: &Theme) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    p.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.5),
        theme.text,
    );
    let how_on = ui.ctx().animate_bool(resp.id, *on);
    let sw = Rect::from_center_size(rect.right_center() - vec2(18.0, 0.0), vec2(36.0, 18.0));
    let off = mix(theme.surface, theme.muted, 0.6);
    p.rect_filled(sw, 9.0, mix(off, theme.accent, how_on));
    let knob_x = egui::lerp((sw.left() + 9.0)..=(sw.right() - 9.0), how_on);
    p.circle_filled(pos2(knob_x, sw.center().y), 6.5, theme.surface);
    if let Some(ink) = theme.ink() {
        p.rect_stroke(sw, 9.0, Stroke::new(1.4_f32, ink), egui::StrokeKind::Outside);
        p.circle_stroke(pos2(knob_x, sw.center().y), 6.5, Stroke::new(1.2_f32, ink));
    }
    let clicked = resp.clicked();
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// Centred, clickable plain-text action such as "Reset Defaults".
pub fn text_link(ui: &mut Ui, text: &str, size: f32, theme: &Theme) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme.text);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(8.0, 6.0), Sense::click());
    let color = if resp.hovered() { theme.accent } else { theme.text };
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(size), color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Like `text_link`, but in the focus colour: for a destructive action.
pub fn danger_link(ui: &mut Ui, text: &str, size: f32, theme: &Theme) -> Response {
    let font = FontId::proportional(size);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), theme.focus);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(8.0, 6.0), Sense::click());
    let color = if resp.hovered() {
        theme.focus
    } else {
        theme.focus.gamma_multiply(0.85)
    };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// What to say about a newer release and what its button is called; `None`
/// when the app is up to date. The button is absent while installing.
pub fn update_notice(update: &crate::update::UpdateState) -> Option<(String, Option<&'static str>)> {
    use crate::update::UpdateState;
    match update {
        UpdateState::UpToDate => None,
        UpdateState::Available(r) => Some((format!("Version {} is available", r.version), Some("Update"))),
        UpdateState::Installing(r) => Some((format!("Updating to {}…", r.version), None)),
        UpdateState::Failed(..) => Some(("The update did not install".to_owned(), Some("Retry"))),
    }
}

/// Act on a press of the update button: install in place where the app can,
/// otherwise open the download in the browser.
pub fn start_update(ui: &Ui, core: &mut crate::core::Core) {
    let Some(release) = core.update.release() else {
        return;
    };
    if crate::update::INSTALLS_IN_PLACE {
        core.update_request = true;
    } else {
        ui.ctx().open_url(egui::OpenUrl::new_tab(release.download_url()));
    }
}

pub fn heading(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(text).size(16.0).color(theme.text));
    });
    ui.add_space(6.0);
}

/// Ink outline around a filled shape, for themes in a style that has one.
pub fn ink_outline(p: &Painter, rect: Rect, radius: f32, theme: &Theme) {
    if let Some(ink) = theme.ink() {
        p.rect_stroke(rect, radius, Stroke::new(1.3_f32, ink), egui::StrokeKind::Outside);
    }
}
