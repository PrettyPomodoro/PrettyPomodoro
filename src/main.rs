#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod capture;
mod config;
mod core;
mod icon;
mod sync;
mod theme;
mod tray;
mod ui;

use eframe::egui;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result {
    let mut core = core::Core::new(
        config::load_config(),
        config::load_history(),
        Some(audio::Audio::spawn()),
    );
    core.goals = config::load_goals();
    let shared = Arc::new(Mutex::new(core));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PrettyPomodoro")
        .with_inner_size(ui::WINDOW_SIZE)
        .with_resizable(false)
        .with_maximize_button(false)
        .with_icon(icon::window_icon());
    if cfg!(target_os = "macos") {
        // Keep the native window (rounded corners, shadow) but draw our own title bar.
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(false);
    } else {
        viewport = viewport.with_decorations(false).with_transparent(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "PrettyPomodoro",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shared)))),
    )
}
