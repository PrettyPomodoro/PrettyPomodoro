//! Developer aid: render one page with demo data to a PNG and exit.
//!
//! `POMO_CAPTURE=out.png [POMO_PAGE=stats] [POMO_THEME="Nord"] prettypomodoro`
//!
//! Pages: `running`, `timer`, `options`, `themes`, `family`, `editor`, `stats`, `sync`, `about`.
//!
//! Nothing is read from or written to the user's saved data in this mode.

use crate::config::History;
use crate::core::Core;
use chrono::Days;
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Capture {
    path: PathBuf,
    pub page: Option<String>,
    theme: Option<String>,
    frame: u32,
}

impl Capture {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            path: std::env::var_os("POMO_CAPTURE")?.into(),
            page: std::env::var("POMO_PAGE").ok(),
            theme: std::env::var("POMO_THEME").ok(),
            frame: 0,
        })
    }

    pub fn prepare(&self, core: &mut Core) {
        core.detach_from_disk();
        if let Some(theme) = &self.theme {
            core.cfg.theme = theme.clone();
        }
        // Deterministic, plausible-looking history.
        let mut history = History::default();
        let today = chrono::Local::now().date_naive();
        let mut seed = 0x2545_f491_u32;
        for back in 0..150 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let sessions = match seed % 10 {
                0..=2 => 0,
                3..=5 => 1 + seed % 3,
                6..=8 => 3 + seed % 4,
                _ => 8 + seed % 3,
            };
            for _ in 0..sessions {
                history.record(today - Days::new(back), "demo", 25);
            }
        }
        core.history = history;

        // "running" shows the timer a third of the way through a focus round.
        if self.page.as_deref() == Some("running") {
            let started = Instant::now().checked_sub(Duration::from_secs(8 * 60 + 20));
            core.toggle(started.unwrap_or_else(Instant::now));
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        // Give layout and animations a few frames to settle first.
        if self.frame == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(shot) = shot {
            let [w, h] = shot.size;
            let result = image::save_buffer(
                &self.path,
                shot.as_raw(),
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            );
            if let Err(e) = &result {
                eprintln!("capture failed: {e}");
            }
            std::process::exit(result.is_err() as i32);
        }
        ctx.request_repaint();
    }
}
