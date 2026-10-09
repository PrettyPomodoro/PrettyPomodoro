//! Settings drawer pages: timer durations, options, cloud sync and about.

use super::widgets::{heading, slider, text_link, toggle_row};
use crate::core::{Core, SyncState};
use crate::theme::{mix, Theme};
use eframe::egui::{self, vec2, Align2, Color32, FontId, RichText, Sense, Ui};

#[derive(Default)]
pub struct State {
    show_token: bool,
}

fn section(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(12.0).color(theme.text_dim));
}

pub fn timer(ui: &mut Ui, core: &mut Core, theme: &Theme) {
    heading(ui, "Timer", theme);

    let row = |ui: &mut Ui, label: &str, value: &mut u32, max: u32, minutes: bool, color: Color32| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(label).size(15.0).color(theme.text_dim));
            let text = if minutes {
                format!("{value}:00")
            } else {
                value.to_string()
            };
            let galley =
                ui.painter()
                    .layout_no_wrap(text, FontId::monospace(13.0), theme.text);
            let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 6.0), Sense::hover());
            ui.painter().rect_filled(rect, 5.0, theme.background);
            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, theme.text);
        });
        slider(ui, value, 1, max, color, theme);
        ui.add_space(4.0);
    };

    let cfg = &mut core.cfg;
    row(ui, "Focus", &mut cfg.focus_min, 90, true, theme.focus);
    row(ui, "Short Break", &mut cfg.short_break_min, 30, true, theme.short_break);
    row(ui, "Long Break", &mut cfg.long_break_min, 60, true, theme.long_break);
    row(ui, "Rounds", &mut cfg.rounds, 12, false, theme.muted);

    ui.vertical_centered(|ui| {
        if text_link(ui, "Reset Defaults", 15.0, theme).clicked() {
            core.cfg.reset_timer_defaults();
        }
    });
}

pub fn options(ui: &mut Ui, core: &mut Core, theme: &Theme) {
    heading(ui, "Options", theme);
    let cfg = &mut core.cfg;

    toggle_row(ui, "Always on Top", &mut cfg.always_on_top, theme);
    toggle_row(ui, "Auto-start Focus Timer", &mut cfg.auto_start_focus, theme);
    toggle_row(ui, "Auto-start Break Timer", &mut cfg.auto_start_break, theme);

    section(ui, "SOUND", theme);
    toggle_row(ui, "Tick Sounds – Focus", &mut cfg.tick_sound_focus, theme);
    toggle_row(ui, "Tick Sounds – Break", &mut cfg.tick_sound_break, theme);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Volume").size(14.5));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{:>3}%", (cfg.volume * 100.0).round()))
                    .monospace()
                    .size(12.0)
                    .color(theme.text_dim),
            );
        });
    });
    let mut volume = (cfg.volume * 100.0).round() as u32;
    let released = slider(ui, &mut volume, 0, 100, theme.accent, theme);
    cfg.volume = volume as f32 / 100.0;
    if released {
        cfg.muted = false;
        core.preview_sound();
    }
    let cfg = &mut core.cfg;

    section(ui, "DESKTOP NOTIFICATIONS", theme);
    toggle_row(ui, "Focus Round End", &mut cfg.notify_focus_end, theme);
    toggle_row(ui, "Break Round End", &mut cfg.notify_break_end, theme);

    section(ui, "TRAY", theme);
    toggle_row(ui, "Minimize to Tray", &mut cfg.minimize_to_tray, theme);
    toggle_row(ui, "Minimize to Tray on Close", &mut cfg.close_to_tray, theme);
    if cfg!(not(any(target_os = "macos", target_os = "windows"))) {
        ui.label(
            RichText::new("The tray is not available on this platform; the window minimizes instead.")
                .size(11.5)
                .color(theme.text_dim),
        );
    }
}

/// Returns true when a sync should be started.
pub fn sync(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) -> bool {
    heading(ui, "Cloud Sync", theme);
    let dim = |s: &str| RichText::new(s).size(12.5).color(theme.text_dim);

    ui.label(dim(
        "Keep your focus history in a private GitHub Gist — free with any GitHub \
         account — and share it between your devices.",
    ));
    ui.add_space(6.0);

    ui.label(RichText::new("GitHub token").size(14.0));
    let cfg = &mut core.cfg.sync;
    ui.horizontal(|ui| {
        let w = ui.available_width() - 62.0;
        ui.add(
            egui::TextEdit::singleline(&mut cfg.token)
                .password(!state.show_token)
                .hint_text("ghp_…")
                .desired_width(w),
        );
        let label = if state.show_token { "Hide" } else { "Show" };
        if ui.add_sized([54.0, 22.0], egui::Button::new(label)).clicked() {
            state.show_token = !state.show_token;
        }
    });
    ui.hyperlink_to(
        RichText::new("Create a token with the “gist” scope ↗").size(12.5),
        "https://github.com/settings/tokens/new?scopes=gist&description=Pomodoro%20Sync",
    );
    ui.add_space(6.0);

    ui.label(RichText::new("Gist ID").size(14.0));
    ui.add(
        egui::TextEdit::singleline(&mut cfg.gist_id)
            .hint_text("Found or created on first sync")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(2.0);
    toggle_row(ui, "Sync Automatically", &mut cfg.auto, theme);
    ui.label(dim("On launch and after every completed focus round."));
    ui.add_space(8.0);

    let running = core.sync_state == SyncState::Running;
    let mut start = false;
    ui.vertical_centered(|ui| {
        let label = if running { "Syncing…" } else { "Sync Now" };
        let button = egui::Button::new(RichText::new(label).size(14.0).color(theme.surface))
            .fill(if running { theme.muted } else { theme.accent })
            .min_size(vec2(140.0, 32.0));
        start = ui.add_enabled(!running, button).clicked();

        ui.add_space(4.0);
        match &core.sync_state {
            SyncState::Idle | SyncState::Running => {}
            SyncState::Done(msg) => {
                ui.label(RichText::new(msg).size(12.5).color(theme.short_break));
            }
            SyncState::Failed(msg) => {
                ui.label(RichText::new(msg).size(12.5).color(theme.focus));
            }
        }
        if let Some(when) = &core.cfg.sync.last_sync {
            ui.label(dim(&format!("Last synced {when}")));
        }
    });
    start
}

pub fn about(ui: &mut Ui, theme: &Theme) {
    heading(ui, "About", theme);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
        let p = ui.painter();
        p.circle_stroke(rect.center(), 22.0, egui::Stroke::new(5.0_f32, theme.focus));
        p.circle_filled(rect.center(), 7.0, theme.accent);
        ui.add_space(4.0);
        ui.label(RichText::new("Pomodoro").size(20.0));
        ui.label(
            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(12.5)
                .color(theme.text_dim),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new("A small, themeable focus timer written in pure Rust with egui.")
                .size(13.5),
        );
        ui.add_space(10.0);
    });

    let keys = [("Space", "Start or pause the timer"), ("Tray menu", "Show, start, skip, quit")];
    for (key, what) in keys {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        let p = ui.painter();
        let pill = egui::Rect::from_min_size(rect.min + vec2(0.0, 3.0), vec2(84.0, 20.0));
        p.rect_filled(pill, 5.0, mix(theme.surface, theme.muted, 0.25));
        p.text(pill.center(), Align2::CENTER_CENTER, key, FontId::proportional(12.0), theme.text);
        p.text(
            rect.left_center() + vec2(94.0, 0.0),
            Align2::LEFT_CENTER,
            what,
            FontId::proportional(13.0),
            theme.text_dim,
        );
    }

    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        if text_link(ui, "Open Data Folder", 13.5, theme).clicked() {
            let dir = crate::config::data_dir();
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", dir.display())));
        }
    });
}
