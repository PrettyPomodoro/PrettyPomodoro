//! Settings drawer pages: timer durations, options, cloud sync and about.

use super::widgets::{ink_outline, heading, slider, text_link, toggle_row};
use crate::core::{Core, SignIn, SyncJob, SyncState};
use crate::theme::{mix, Theme};
use eframe::egui::{self, vec2, Align2, Color32, FontId, RichText, Sense, Ui};

#[derive(Default)]
pub struct State {
    icon: Option<egui::TextureHandle>,
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
            ink_outline(ui.painter(), rect, 5.0, theme);
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

/// Returns the background job the user asked for, if any.
pub fn sync(ui: &mut Ui, core: &mut Core, theme: &Theme) -> Option<SyncJob> {
    heading(ui, "Cloud Sync", theme);
    let dim = |s: &str| RichText::new(s).size(12.5).color(theme.text_dim);
    let primary = |label: &str, enabled: bool| {
        egui::Button::new(RichText::new(label).size(14.0).color(theme.on_accent()))
            .fill(if enabled { theme.accent } else { theme.muted })
            .min_size(vec2(190.0, 34.0))
    };
    let mut job = None;

    ui.vertical_centered(|ui| {
        ui.label(dim("Sync your history privately for free."));
        ui.add_space(14.0);

        if !core.cfg.sync.token.is_empty() {
            // Signed in.
            let who = match &core.cfg.sync.login {
                Some(login) => format!("Signed in as {login}"),
                None => "Signed in with GitHub".to_owned(),
            };
            ui.label(RichText::new(who).size(15.0));
            ui.add_space(10.0);

            let running = core.sync_state == SyncState::Running;
            let label = if running { "Syncing…" } else { "Sync Now" };
            if ui.add_enabled(!running, primary(label, !running)).clicked() {
                job = Some(SyncJob::Sync);
            }
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
            ui.label(dim("Syncs on launch and after every focus round."));
            ui.add_space(12.0);
            if text_link(ui, "Sign Out", 13.5, theme).clicked() {
                // Signing out forgets the account, not the choice below.
                let sync_goals = core.cfg.sync.sync_goals;
                core.cfg.sync = crate::config::SyncConfig { sync_goals, ..Default::default() };
                core.sync_state = SyncState::Idle;
            }
            return;
        }

        match core.sign_in.clone() {
            SignIn::Waiting { code, url } => {
                ui.label(RichText::new("Enter this code on GitHub").size(14.0));
                ui.add_space(6.0);
                let galley = ui.painter().layout_no_wrap(code.clone(), FontId::monospace(26.0), theme.text);
                let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(28.0, 18.0), Sense::hover());
                ui.painter().rect_filled(rect, 8.0, theme.background);
                ink_outline(ui.painter(), rect, 8.0, theme);
                ui.painter().galley(rect.center() - galley.size() / 2.0, galley, theme.text);
                ui.add_space(4.0);
                ui.label(dim("It is already copied. GitHub opened in your browser."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 8.0) / 2.0;
                    if ui.add_sized([w, 30.0], egui::Button::new("Copy Code")).clicked() {
                        ui.ctx().copy_text(code.clone());
                    }
                    if ui.add_sized([w, 30.0], egui::Button::new("Open GitHub")).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&url));
                    }
                });
                ui.add_space(8.0);
                ui.label(dim("Waiting for you to approve…"));
                if text_link(ui, "Cancel", 13.5, theme).clicked() {
                    core.sign_in = SignIn::Idle;
                    core.sign_in_attempt += 1;
                }
            }
            state => {
                let starting = state == SignIn::Starting;
                let label = if starting { "Contacting GitHub…" } else { "Sign in with GitHub" };
                if ui.add_enabled(!starting, primary(label, !starting)).clicked() {
                    job = Some(SyncJob::SignIn);
                }
                ui.add_space(6.0);
                if let SignIn::Failed(msg) = &state {
                    ui.label(RichText::new(msg).size(12.5).color(theme.focus));
                }
                ui.label(dim(
                    "You get a short code to enter on github.com. The app can only \
                     read and write your gists, where it keeps your data in one private gist.",
                ));
            }
        }
    });

    // Goal texts are personal, so syncing them is something to opt into, and
    // the choice is only offered once there is an account to sync to.
    if !core.cfg.sync.token.is_empty() {
        ui.add_space(14.0);
        if toggle_row(ui, "Sync Goal Texts", &mut core.cfg.sync.sync_goals, theme) && core.cfg.sync.sync_goals {
            job = Some(SyncJob::Sync);
        }
        ui.label(dim(
            "Also keeps the goals you tick off in your private gist, so your goal \
             history follows you to your other computers.",
        ));
    }
    job
}

pub fn about(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    heading(ui, "About", theme);
    ui.vertical_centered(|ui| {
        let icon = state.icon.get_or_insert_with(|| {
            // Mipmaps keep the detailed artwork smooth when drawn small.
            let options = egui::TextureOptions {
                mipmap_mode: Some(egui::TextureFilter::Linear),
                ..egui::TextureOptions::LINEAR
            };
            ui.ctx().load_texture("app-icon", crate::icon::about_image(), options)
        });
        ui.add(egui::Image::new(&*icon).fit_to_exact_size(vec2(72.0, 72.0)));
        ui.add_space(4.0);
        ui.label(RichText::new("PrettyPomodoro").size(20.0));
        ui.label(
            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(12.5)
                .color(theme.text_dim),
        );
        if let Some((text, action)) = super::widgets::update_notice(&core.update) {
            ui.add_space(4.0);
            ui.label(RichText::new(text).size(13.5));
            if let crate::update::UpdateState::Failed(_, error) = &core.update {
                ui.label(RichText::new(error).size(12.5).color(theme.focus));
            }
            if let Some(action) = action {
                let button = egui::Button::new(RichText::new(action).size(14.0).color(theme.on_accent()))
                    .fill(theme.accent)
                    .min_size(vec2(120.0, 30.0));
                if ui.add(button).clicked() {
                    super::widgets::start_update(ui, core);
                }
            }
        }
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
        ink_outline(p, pill, 5.0, theme);
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
