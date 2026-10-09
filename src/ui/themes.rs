//! Theme picker and the custom theme editor.

use super::widgets::{heading, icon_button, Icon};
use crate::config::Config;
use crate::core::Core;
use crate::theme::{builtin_themes, pencil_art_theme, to_hex, Family, Style, Theme};
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, RichText, Sense, Stroke, Ui};

#[derive(Default)]
pub struct State {
    pub editor: Option<Editor>,
}

pub struct Editor {
    pub theme: Theme,
    /// Name of the custom theme being edited; `None` for a new one.
    original: Option<String>,
    /// Starting palettes for a new theme, one per style. Switching style swaps
    /// the palette as long as the colours haven't been touched.
    seeds: Option<(Theme, Theme)>,
    error: Option<String>,
}

impl State {
    pub fn start_new(&mut self, cfg: &Config) {
        let active = cfg.active_theme();
        let mut standard = match active.style {
            Style::Standard => active,
            Style::PencilArt => builtin_themes().swap_remove(0),
        };
        standard.name = unique_name(cfg, "My Theme");
        let mut pencil = pencil_art_theme();
        pencil.name = standard.name.clone();
        self.editor = Some(Editor {
            theme: standard.clone(),
            original: None,
            seeds: Some((standard, pencil)),
            error: None,
        });
    }
}

impl Editor {
    fn set_style(&mut self, style: Style) {
        if self.theme.style == style {
            return;
        }
        if let Some((standard, pencil)) = &self.seeds {
            let (from, to) = match style {
                Style::PencilArt => (standard, pencil),
                Style::Standard => (pencil, standard),
            };
            let name = std::mem::take(&mut self.theme.name);
            let untouched = Theme { name: from.name.clone(), ..self.theme.clone() } == *from;
            if untouched {
                self.theme = to.clone();
            }
            self.theme.name = name;
        }
        self.theme.style = style;
    }
}

fn unique_name(cfg: &Config, base: &str) -> String {
    let taken = cfg.all_themes();
    let mut name = base.to_owned();
    let mut n = 2;
    while taken.iter().any(|t| t.name == name) {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

pub fn show(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    if state.editor.is_some() {
        edit(ui, state, core, theme);
    } else {
        list(ui, state, core, theme);
    }
}

fn list(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    heading(ui, "Themes", theme);

    ui.vertical_centered(|ui| {
        let button = egui::Button::new(RichText::new("+  New Theme").size(14.0))
            .min_size(vec2(ui.available_width(), 34.0));
        if ui.add(button).clicked() {
            state.start_new(&core.cfg);
        }
    });

    let all = core.cfg.all_themes();
    let builtin_count = all.len() - core.cfg.custom_themes.len();
    let mut delete = None;
    for family in Family::ALL {
        let members: Vec<_> = all.iter().enumerate().filter(|(_, t)| t.family() == family).collect();
        if members.is_empty() {
            continue;
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("{}  ·  {}", family.label().to_uppercase(), members.len()))
                .size(12.0)
                .color(theme.text_dim),
        );
        for (i, t) in members {
            let custom = i >= builtin_count;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            let selected = core.cfg.theme == t.name;
            let p = ui.painter().clone();
            p.rect_filled(rect, 7.0, t.background);
            let border = match (selected, t.ink()) {
                (true, _) => Stroke::new(2.0_f32, theme.accent),
                (false, Some(ink)) => Stroke::new(1.5_f32, ink),
                (false, None) => Stroke::new(1.0_f32, crate::theme::mix(theme.surface, theme.muted, 0.5)),
            };
            p.rect_stroke(rect, 7.0, border, egui::StrokeKind::Inside);
            p.text(
                rect.left_center() + vec2(14.0, 0.0),
                Align2::LEFT_CENTER,
                &t.name,
                FontId::proportional(14.5),
                t.text,
            );
            for (j, c) in [t.long_break, t.short_break, t.focus].into_iter().enumerate() {
                let dot = pos2(rect.right() - 18.0 - 16.0 * j as f32, rect.center().y);
                p.circle_filled(dot, 5.0, c);
                if let Some(ink) = t.ink() {
                    p.circle_stroke(dot, 5.0, Stroke::new(1.2_f32, ink));
                }
            }

            if custom {
                let slot = |k: f32| {
                    Rect::from_center_size(
                        pos2(rect.right() - 82.0 - 28.0 * k, rect.center().y),
                        vec2(26.0, 26.0),
                    )
                };
                if icon_button(ui, slot(0.0), Icon::Trash, t, false)
                    .on_hover_text("Delete theme")
                    .clicked()
                {
                    delete = Some(t.name.clone());
                }
                if icon_button(ui, slot(1.0), Icon::Edit, t, false)
                    .on_hover_text("Edit theme")
                    .clicked()
                {
                    state.editor = Some(Editor {
                        original: Some(t.name.clone()),
                        theme: t.clone(),
                        seeds: None,
                        error: None,
                    });
                }
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                core.cfg.theme = t.name.clone();
            }
        }
    }

    if let Some(name) = delete {
        core.cfg.custom_themes.retain(|t| t.name != name);
        if core.cfg.theme == name {
            core.cfg.theme = crate::theme::DEFAULT_THEME.to_owned();
        }
    }
}

fn edit(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    let Some(editor) = &mut state.editor else {
        return;
    };
    let title = if editor.original.is_some() {
        "Edit Theme"
    } else {
        "New Theme"
    };
    heading(ui, title, theme);

    ui.label(RichText::new("Name").size(13.0).color(theme.text_dim));
    ui.add(
        egui::TextEdit::singleline(&mut editor.theme.name)
            .margin(vec2(8.0, 6.0))
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);

    // A theme is either a flat light/dark one or drawn as pencil art.
    ui.label(RichText::new("Style").size(13.0).color(theme.text_dim));
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        for (style, label) in [(Style::Standard, "Light / Dark"), (Style::PencilArt, "Pencil Art")] {
            let on = editor.theme.style == style;
            let text = RichText::new(label).color(if on { theme.surface } else { theme.text });
            let mut button = egui::Button::new(text);
            if on {
                button = button.fill(theme.accent);
            }
            if ui.add_sized([w, 30.0], button).clicked() {
                editor.set_style(style);
            }
        }
    });
    let hint = match editor.theme.family() {
        Family::PencilArt => "Hand-drawn look: ink outlines, hatching and a sketched timer.".to_owned(),
        family => format!("Flat colours. Listed under {} because of its background.", family.label()),
    };
    ui.label(RichText::new(hint).size(12.0).color(theme.text_dim));
    ui.add_space(6.0);

    let t = &mut editor.theme;
    let rows = [
        ("Focus", &mut t.focus),
        ("Short Break", &mut t.short_break),
        ("Long Break", &mut t.long_break),
        ("Background", &mut t.background),
        ("Surface", &mut t.surface),
        ("Muted / Outlines", &mut t.muted),
        ("Text", &mut t.text),
        ("Secondary Text", &mut t.text_dim),
        ("Accent", &mut t.accent),
    ];
    for (label, color) in rows {
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            ui.label(RichText::new(label).size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.color_edit_button_srgba(color);
                ui.label(
                    RichText::new(to_hex(*color))
                        .monospace()
                        .size(12.0)
                        .color(theme.text_dim),
                );
            });
        });
    }

    if let Some(error) = &editor.error {
        ui.add_space(4.0);
        ui.label(RichText::new(error).size(12.5).color(theme.focus));
    }
    ui.add_space(8.0);

    let mut close = false;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        if ui.add_sized([w, 32.0], egui::Button::new("Cancel")).clicked() {
            close = true;
        }
        let save = egui::Button::new(RichText::new("Save Theme").color(theme.surface)).fill(theme.accent);
        if ui.add_sized([w, 32.0], save).clicked() {
            match validate(editor, &core.cfg) {
                Ok(()) => {
                    let mut saved = editor.theme.clone();
                    saved.name = saved.name.trim().to_owned();
                    let customs = &mut core.cfg.custom_themes;
                    match editor.original.as_ref().and_then(|o| customs.iter().position(|t| &t.name == o)) {
                        Some(i) => customs[i] = saved.clone(),
                        None => customs.push(saved.clone()),
                    }
                    core.cfg.theme = saved.name;
                    close = true;
                }
                Err(e) => editor.error = Some(e),
            }
        }
    });
    if close {
        state.editor = None;
    }
}

fn validate(editor: &Editor, cfg: &Config) -> Result<(), String> {
    let name = editor.theme.name.trim();
    if name.is_empty() {
        return Err("Give the theme a name.".into());
    }
    let renamed_from = editor.original.as_deref();
    let clash = cfg
        .all_themes()
        .iter()
        .any(|t| t.name == name && Some(t.name.as_str()) != renamed_from);
    if clash {
        return Err(format!("A theme named “{name}” already exists."));
    }
    Ok(())
}
