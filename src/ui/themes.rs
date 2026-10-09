//! Theme picker and the custom theme editor.

use super::widgets::{heading, icon_button, Icon};
use crate::config::Config;
use crate::core::Core;
use crate::theme::{builtin_themes, parse_hex, pencil_art_theme, to_hex, Family, Style, Theme};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui};

#[derive(Default)]
pub struct State {
    pub editor: Option<Editor>,
    /// The family whose themes are expanded, if any.
    open: Option<Family>,
}

pub struct Editor {
    pub theme: Theme,
    /// Name of the custom theme being edited; `None` for a new one.
    original: Option<String>,
    /// Starting palettes for a new theme, one per style. Switching style swaps
    /// the palette as long as the colours haven't been touched.
    seeds: Option<(Theme, Theme)>,
    /// Index of the colour row whose choices are open, and its hex text.
    picking: Option<usize>,
    hex: String,
    error: Option<String>,
}

impl State {
    /// Open the choices of the first colour row in the editor.
    pub fn pick_first_color(&mut self) {
        let row = 0;
        if let Some(editor) = &mut self.editor {
            editor.picking = Some(row);
            editor.hex = to_hex(editor.theme.focus);
        }
    }

    /// Expand the family holding the active theme.
    pub fn expand_current(&mut self, cfg: &Config) {
        self.open = Some(cfg.active_theme().family());
    }

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
            picking: None,
            hex: String::new(),
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
        ui.add_space(4.0);
        let open = state.open == Some(family);
        if family_tile(ui, family, &members, open, &core.cfg.theme, theme) {
            state.open = (!open).then_some(family);
        }
        if !open {
            continue;
        }
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
                        picking: None,
                        hex: String::new(),
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

/// The header tile of a theme family, previewed in one of its themes: the
/// active theme if it belongs here, otherwise the family's first. Returns true
/// when clicked.
fn family_tile(
    ui: &mut Ui,
    family: Family,
    members: &[(usize, &Theme)],
    open: bool,
    active: &str,
    theme: &Theme,
) -> bool {
    let current = members.iter().find(|(_, t)| t.name == active);
    let face = current.unwrap_or(&members[0]).1;

    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::click());
    let p = ui.painter().clone();
    p.rect_filled(rect, 9.0, face.background);
    let border = match (current.is_some(), face.ink()) {
        (true, _) => Stroke::new(2.0_f32, theme.accent),
        (false, Some(ink)) => Stroke::new(1.5_f32, ink),
        (false, None) => Stroke::new(1.0_f32, crate::theme::mix(theme.surface, theme.muted, 0.5)),
    };
    p.rect_stroke(rect, 9.0, border, egui::StrokeKind::Inside);

    p.text(
        rect.left_center() + vec2(16.0, -10.0),
        Align2::LEFT_CENTER,
        family.label(),
        FontId::proportional(17.0),
        face.text,
    );
    let count = match members.len() {
        1 => "1 theme".to_owned(),
        n => format!("{n} themes"),
    };
    let detail = match current {
        Some((_, t)) => format!("{count}  ·  {} selected", t.name),
        None => count,
    };
    p.text(
        rect.left_center() + vec2(16.0, 12.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(12.0),
        face.text_dim,
    );

    for (j, c) in [face.long_break, face.short_break, face.focus].into_iter().enumerate() {
        let dot = pos2(rect.right() - 46.0 - 16.0 * j as f32, rect.center().y);
        p.circle_filled(dot, 5.0, c);
        if let Some(ink) = face.ink() {
            p.circle_stroke(dot, 5.0, Stroke::new(1.2_f32, ink));
        }
    }
    // Chevron: down when collapsed, up when expanded.
    let c = pos2(rect.right() - 22.0, rect.center().y);
    let tip = if open { -3.0 } else { 3.0 };
    let stroke = Stroke::new(1.6_f32, face.text);
    p.line_segment([c + vec2(-6.0, -tip), c + vec2(0.0, tip)], stroke);
    p.line_segment([c + vec2(0.0, tip), c + vec2(6.0, -tip)], stroke);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
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

    let Editor { theme: t, picking, hex, .. } = editor;
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
    let outline = Stroke::new(1.0_f32, crate::theme::mix(theme.surface, theme.text, 0.35));
    for (i, (label, color)) in rows.into_iter().enumerate() {
        // The whole row toggles its colour choices open and closed.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let p = ui.painter();
        p.text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(14.0), theme.text);
        let swatch = Rect::from_center_size(rect.right_center() - vec2(20.0, 0.0), vec2(40.0, 20.0));
        p.rect_filled(swatch, 5.0, *color);
        p.rect_stroke(swatch, 5.0, outline, egui::StrokeKind::Inside);
        p.text(
            swatch.left_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            to_hex(*color),
            FontId::monospace(12.0),
            theme.text_dim,
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *picking = (*picking != Some(i)).then_some(i);
            *hex = to_hex(*color);
        }
        if *picking == Some(i) {
            color_choices(ui, color, hex, theme);
        }
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
        // Land on the list with the current theme's family expanded.
        state.expand_current(&core.cfg);
    }
}

/// Inline colour choices for one row: a grid of swatches and a hex field.
fn color_choices(ui: &mut Ui, color: &mut Color32, hex: &mut String, theme: &Theme) {
    const HUES: [f32; 9] = [0.0, 0.07, 0.13, 0.28, 0.45, 0.55, 0.62, 0.75, 0.9];
    // (saturation, value) per row, pale to deep; a row of greys follows.
    const TONES: [(f32, f32); 5] = [(0.25, 1.0), (0.5, 1.0), (0.75, 0.95), (0.85, 0.75), (0.9, 0.5)];
    let columns = HUES.len();
    let swatch_at = |row: usize, col: usize| -> Color32 {
        match TONES.get(row) {
            Some(&(s, v)) => egui::ecolor::Hsva::new(HUES[col], s, v, 1.0).into(),
            None => Color32::from_gray((255.0 - 245.0 * col as f32 / (columns - 1) as f32) as u8),
        }
    };

    let gap = 4.0;
    let (rows, cell_h) = (TONES.len() + 1, 22.0);
    let (area, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), rows as f32 * (cell_h + gap) - gap),
        Sense::hover(),
    );
    let cell_w = (area.width() - gap * (columns - 1) as f32) / columns as f32;
    for row in 0..rows {
        for col in 0..columns {
            let rect = Rect::from_min_size(
                area.min + vec2(col as f32 * (cell_w + gap), row as f32 * (cell_h + gap)),
                vec2(cell_w, cell_h),
            );
            let choice = swatch_at(row, col);
            let resp = ui
                .interact(rect, ui.id().with(("swatch", row, col)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().rect_filled(rect, 4.0, choice);
            if choice == *color || resp.hovered() {
                ui.painter().rect_stroke(rect, 4.0, Stroke::new(2.0_f32, theme.text), egui::StrokeKind::Inside);
            }
            if resp.clicked() {
                *color = choice;
                *hex = to_hex(choice);
            }
        }
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new("Hex").size(13.0).color(theme.text_dim));
        let field = egui::TextEdit::singleline(hex)
            .font(FontId::monospace(13.0))
            .char_limit(7)
            .margin(vec2(8.0, 5.0))
            .desired_width(90.0);
        if ui.add(field).changed() {
            // Applied as soon as the text is a complete colour.
            if let Some(parsed) = parse_hex(hex) {
                *color = parsed;
            }
        }
    });
    ui.add_space(6.0);
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
