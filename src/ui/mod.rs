//! The window: custom title bar, the timer card and the settings drawer.

mod fonts;
mod pencil;
mod settings;
mod stats;
mod themes;
mod timer;
mod widgets;

use crate::config::{self, Config};
use crate::core::{lock, Core, Shared};
use crate::theme::{mix, Theme};
use crate::tray::{NativeWindow, Tray};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Ui,
    ViewportCommand,
};
use widgets::{icon_button, Icon};

pub const WINDOW_SIZE: [f32; 2] = [360.0, 480.0];
const HEADER_H: f32 = 56.0;
const TABS_H: f32 = 46.0;
/// macOS rounds the window itself; elsewhere the card is drawn on a transparent window.
const CORNER: u8 = if cfg!(target_os = "macos") { 0 } else { 10 };

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Timer,
    Options,
    Themes,
    Stats,
    Sync,
    About,
}

const TABS: [(Tab, Icon, &str); 6] = [
    (Tab::Timer, Icon::Clock, "Timer"),
    (Tab::Options, Icon::Sliders, "Options"),
    (Tab::Themes, Icon::Palette, "Themes"),
    (Tab::Stats, Icon::Calendar, "Activity"),
    (Tab::Sync, Icon::Cloud, "Cloud Sync"),
    (Tab::About, Icon::Info, "About"),
];

pub struct App {
    shared: Shared,
    window: Option<NativeWindow>,
    tray: Option<Tray>,

    drawer_open: bool,
    tab: Tab,
    themes: themes::State,
    stats: stats::State,
    settings: settings::State,

    saved_cfg: Config,
    applied_theme: Option<Theme>,
    applied_on_top: Option<bool>,
    capture: Option<crate::capture::Capture>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shared: Shared) -> Self {
        fonts::install(&cc.egui_ctx);
        let window = NativeWindow::new(cc);
        let tray = window
            .clone()
            .and_then(|w| crate::tray::create(shared.clone(), w, cc.egui_ctx.clone()));
        crate::core::spawn_ticker(shared.clone(), cc.egui_ctx.clone());

        let capture = crate::capture::Capture::from_env();
        let saved_cfg = {
            let mut core = lock(&shared);
            if let Some(capture) = &capture {
                capture.prepare(&mut core);
            } else if !core.cfg.sync.token.is_empty() {
                core.sync_request = Some(crate::core::SyncJob::Sync);
            }
            core.cfg.clone()
        };

        let mut app = Self {
            shared,
            window,
            tray,
            drawer_open: false,
            tab: Tab::Timer,
            themes: Default::default(),
            stats: Default::default(),
            settings: Default::default(),
            saved_cfg,
            applied_theme: None,
            applied_on_top: None,
            capture,
        };
        if let Some(page) = app.capture.as_ref().and_then(|c| c.page.clone()) {
            app.open_page(&page);
        }
        app
    }

    fn open_page(&mut self, page: &str) {
        let tab = match page {
            "timer" => Tab::Timer,
            "options" => Tab::Options,
            "themes" | "family" | "editor" => Tab::Themes,
            "stats" => Tab::Stats,
            "sync" => Tab::Sync,
            "about" => Tab::About,
            _ => return,
        };
        self.drawer_open = true;
        self.tab = tab;
        let core = lock(&self.shared);
        match page {
            "editor" => self.themes.start_new(&core.cfg),
            "family" => self.themes.expand_current(&core.cfg),
            _ => {}
        }
    }

    fn hide_or(&self, ctx: &egui::Context, to_tray: bool, otherwise: ViewportCommand) {
        match (&self.window, &self.tray) {
            (Some(window), Some(_)) if to_tray => window.hide(),
            _ => ctx.send_viewport_cmd(otherwise),
        }
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect, core: &Core, theme: &Theme) {
        let ctx = ui.ctx().clone();
        let drag = ui.interact(rect, ui.id().with("titlebar"), Sense::click_and_drag());
        if drag.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "PrettyPomodoro",
            FontId::proportional(16.0),
            theme.text_dim,
        );

        let slot = |x: f32| Rect::from_center_size(pos2(x, rect.center().y), vec2(34.0, 34.0));
        let left = slot(rect.left() + 28.0);
        if self.drawer_open {
            if icon_button(ui, left, Icon::Back, theme, false).clicked() {
                self.drawer_open = false;
                self.themes.editor = None;
            }
        } else if icon_button(ui, left, Icon::Menu, theme, false)
            .on_hover_text("Settings")
            .clicked()
        {
            self.drawer_open = true;
        }

        if icon_button(ui, slot(rect.right() - 28.0), Icon::Close, theme, false).clicked() {
            self.hide_or(&ctx, core.cfg.close_to_tray, ViewportCommand::Close);
        }
        if icon_button(ui, slot(rect.right() - 64.0), Icon::Minimize, theme, false).clicked() {
            self.hide_or(&ctx, core.cfg.minimize_to_tray, ViewportCommand::Minimized(true));
        }
    }

    fn drawer(&mut self, ui: &mut Ui, rect: Rect, core: &mut Core, theme: &Theme) {
        let content = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - TABS_H));
        ui.painter().rect_filled(content, 0.0, theme.surface);

        let inner = content.shrink2(vec2(22.0, 0.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.set_clip_rect(content);
        egui::ScrollArea::vertical()
            .id_salt(self.tab as u8)
            .auto_shrink(false)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut child, |ui| {
                ui.add_space(12.0);
                match self.tab {
                    Tab::Timer => settings::timer(ui, core, theme),
                    Tab::Options => settings::options(ui, core, theme),
                    Tab::Themes => themes::show(ui, &mut self.themes, core, theme),
                    Tab::Stats => stats::show(ui, &mut self.stats, core, theme),
                    Tab::Sync => {
                        if let Some(job) = settings::sync(ui, &mut self.settings, core, theme) {
                            core.sync_request = Some(job);
                        }
                    }
                    Tab::About => settings::about(ui, &mut self.settings, theme),
                }
                ui.add_space(14.0);
            });

        let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - TABS_H), rect.max);
        let w = bar.width() / TABS.len() as f32;
        for (i, (tab, icon, name)) in TABS.into_iter().enumerate() {
            let cell = Rect::from_min_size(bar.min + vec2(i as f32 * w, 0.0), vec2(w, TABS_H));
            let active = self.tab == tab;
            if icon_button(ui, cell.shrink(4.0), icon, theme, active)
                .on_hover_text(name)
                .clicked()
            {
                self.tab = tab;
                self.themes.editor = None;
            }
            if active {
                let y = bar.bottom() - 1.5;
                ui.painter().line_segment(
                    [pos2(cell.center().x - 15.0, y), pos2(cell.center().x + 15.0, y)],
                    Stroke::new(3.0_f32, theme.accent),
                );
            }
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context, theme: &Theme) {
        if self.applied_theme.as_ref() == Some(theme) {
            return;
        }
        self.applied_theme = Some(theme.clone());
        let mut v = if theme.is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let raised = mix(theme.surface, theme.muted, 0.22);
        v.override_text_color = Some(theme.text);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = theme.surface;
        v.window_stroke = Stroke::new(1.0_f32, theme.muted);
        v.extreme_bg_color = theme.background;
        v.hyperlink_color = theme.accent;
        v.selection.bg_fill = theme.accent.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0_f32, theme.accent);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, mix(theme.surface, theme.muted, 0.5));
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme.text);
        for (w, fill) in [
            (&mut v.widgets.inactive, raised),
            (&mut v.widgets.hovered, mix(theme.surface, theme.muted, 0.4)),
            (&mut v.widgets.active, mix(theme.surface, theme.muted, 0.55)),
            (&mut v.widgets.open, raised),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = match theme.ink() {
                Some(ink) => Stroke::new(1.3_f32, ink),
                None => Stroke::NONE,
            };
            w.fg_stroke = Stroke::new(1.2_f32, theme.text);
            w.corner_radius = CornerRadius::same(6);
            w.expansion = 0.0;
        }
        ctx.set_visuals(v);
        ctx.style_mut(|s| {
            s.spacing.button_padding = vec2(12.0, 6.0);
            s.spacing.item_spacing = vec2(8.0, 6.0);
            s.spacing.scroll = egui::style::ScrollStyle::floating();
            s.interaction.selectable_labels = false;
        });
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let shared = self.shared.clone();
        let mut core = lock(&shared);

        // While a theme is being edited the whole app previews it.
        let theme = match &self.themes.editor {
            Some(editor) => editor.theme.clone(),
            None => core.cfg.active_theme(),
        };
        self.apply_theme(ctx, &theme);

        if self.applied_on_top != Some(core.cfg.always_on_top) {
            self.applied_on_top = Some(core.cfg.always_on_top);
            let level = if core.cfg.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, CORNER, theme.background);
                let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
                let body = Rect::from_min_max(header.left_bottom(), rect.max);
                self.header(ui, header, &core, &theme);
                if self.drawer_open {
                    self.drawer(ui, body, &mut core, &theme);
                } else {
                    timer::show(ui, rect, &mut core, &theme);
                }
            });

        if core.cfg != self.saved_cfg {
            core.durations_changed();
            self.saved_cfg = core.cfg.clone();
            if self.capture.is_none() {
                config::save_config(&core.cfg);
            }
        }
        drop(core);

        if let Some(capture) = &mut self.capture {
            capture.update(ctx);
        }
    }
}
