#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod capture;
mod config;
mod core;
mod sync;
mod theme;
mod tray;
mod ui;

use eframe::egui;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result {
    let core = core::Core::new(
        config::load_config(),
        config::load_history(),
        Some(audio::Audio::spawn()),
    );
    let shared = Arc::new(Mutex::new(core));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Pomodoro")
        .with_inner_size(ui::WINDOW_SIZE)
        .with_resizable(false)
        .with_maximize_button(false)
        .with_icon(app_icon());
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
        "Pomodoro",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shared)))),
    )
}

/// App icon drawn in code: a green timer ring on a white rounded square.
fn app_icon() -> egui::IconData {
    const N: u32 = 256;
    const GREEN: [f32; 3] = [38.0, 191.0, 115.0];
    let c = (N as f32 - 1.0) / 2.0;
    let cover = |edge: f32| (0.5 - edge).clamp(0.0, 1.0);
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = ((x as f32 - c).abs(), (y as f32 - c).abs());
            // Signed distance to a rounded square.
            let (half, radius) = (112.0, 50.0);
            let (qx, qy) = ((dx - half + radius).max(0.0), (dy - half + radius).max(0.0));
            let square = cover((qx * qx + qy * qy).sqrt() - radius);
            let ring = cover(((dx * dx + dy * dy).sqrt() - 66.0).abs() - 10.0);
            let px = GREEN.map(|g| (255.0 + (g - 255.0) * ring) as u8);
            rgba.extend_from_slice(&[px[0], px[1], px[2], (square * 255.0) as u8]);
        }
    }
    egui::IconData {
        rgba,
        width: N,
        height: N,
    }
}
