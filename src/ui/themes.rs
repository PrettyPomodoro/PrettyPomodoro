//! Theme picker and the custom theme editor.

use super::widgets::{heading, icon_button, Icon};
use crate::config::Config;
use crate::core::Core;
use crate::theme::{first_of_style, parse_hex, to_hex, Family, Style, Theme};
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
    seeds: Option<Vec<Theme>>,
    /// Index of the colour row whose choices are open.
    picking: Option<usize>,
    picker: Picker,
    error: Option<String>,
}

impl State {
    /// Open the choices of the first colour row in the editor.
    pub fn pick_first_color(&mut self) {
        let row = 0;
        if let Some(editor) = &mut self.editor {
            editor.picking = Some(row);
            editor.picker.load(editor.theme.focus);
        }
    }

    /// Expand the family holding the active theme.
    pub fn expand_current(&mut self, cfg: &Config) {
        self.open = Some(cfg.active_theme().family());
    }

    pub fn start_new(&mut self, cfg: &Config) {
        let active = cfg.active_theme();
        let name = unique_name(cfg, "My Theme");
        let seeds: Vec<_> = Style::ALL
            .into_iter()
            .map(|style| {
                let seed = if active.style == style { active.clone() } else { first_of_style(style) };
                Theme { name: name.clone(), ..seed }
            })
            .collect();
        self.editor = Some(Editor {
            theme: seeds[0].clone(),
            original: None,
            seeds: Some(seeds),
            picking: None,
            picker: Picker::default(),
            error: None,
        });
    }
}

impl Editor {
    fn set_style(&mut self, style: Style) {
        if self.theme.style == style {
            return;
        }
        if let Some(seeds) = &self.seeds {
            let seed = |style| seeds.iter().find(|t| t.style == style);
            if let (Some(from), Some(to)) = (seed(self.theme.style), seed(style)) {
                let name = std::mem::take(&mut self.theme.name);
                let untouched = Theme { name: from.name.clone(), ..self.theme.clone() } == *from;
                if untouched {
                    self.theme = to.clone();
                }
                self.theme.name = name;
            }
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
                        picker: Picker::default(),
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

    // A theme is either a flat light/dark one or drawn in one of the styles.
    ui.label(RichText::new("Style").size(13.0).color(theme.text_dim));
    for row in Style::ALL.chunks(2) {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 8.0) / 2.0;
            for &style in row {
                let on = editor.theme.style == style;
                let color = if on { theme.on_accent() } else { theme.text };
                let mut button = egui::Button::new(RichText::new(style.label()).color(color));
                if on {
                    button = button.fill(theme.accent);
                }
                if ui.add_sized([w, 30.0], button).clicked() {
                    editor.set_style(style);
                }
            }
        });
    }
    let hint = match editor.theme.family() {
        Family::PencilArt => "Hand-drawn look: ink outlines, hatching and a sketched timer.".to_owned(),
        Family::Sketch => "Greyscale, with the timer drawn as a hand-sketched chart.".to_owned(),
        Family::Editorial => "Editorial look: serif figures and clean ink outlines.".to_owned(),
        family => format!("Flat colours. Listed under {} because of its background.", family.label()),
    };
    ui.label(RichText::new(hint).size(12.0).color(theme.text_dim));
    ui.add_space(6.0);

    let Editor { theme: t, picking, picker, .. } = editor;
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
            picker.load(*color);
        }
        if *picking == Some(i) {
            color_choices(ui, color, picker, theme);
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
        let save = egui::Button::new(RichText::new("Save Theme").color(theme.on_accent())).fill(theme.accent);
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

/// Editing state of the open colour row. Hue, saturation and brightness are
/// kept here rather than re-derived from the colour, so the hue survives a trip
/// through grey or black.
#[derive(Default)]
struct Picker {
    hex: String,
    hsv: [f32; 3],
}

impl Picker {
    fn load(&mut self, color: Color32) {
        let hsva = egui::ecolor::Hsva::from(color);
        self.hsv = [hsva.h, hsva.s, hsva.v];
        self.hex = to_hex(color);
    }

    fn color(&self) -> Color32 {
        hsv(self.hsv[0], self.hsv[1], self.hsv[2])
    }
}

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    egui::ecolor::Hsva::new(h, s, v, 1.0).into()
}

/// Inline colour choices for one row: hue / saturation / brightness sliders
/// and a hex field.
fn color_choices(ui: &mut Ui, color: &mut Color32, picker: &mut Picker, theme: &Theme) {
    let [h, s, v] = picker.hsv;
    let sliders: [(&str, Box<dyn Fn(f32) -> Color32>); 3] = [
        ("Hue", Box::new(move |x| hsv(x, 1.0, 1.0))),
        ("Saturation", Box::new(move |x| hsv(h, x, v.max(0.35)))),
        ("Brightness", Box::new(move |x| hsv(h, s, x))),
    ];
    for (i, (label, color_at)) in sliders.into_iter().enumerate() {
        if gradient_slider(ui, label, &mut picker.hsv[i], color_at, theme) {
            *color = picker.color();
            picker.hex = to_hex(*color);
        }
    }
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Hex").size(13.0).color(theme.text_dim));
        let field = egui::TextEdit::singleline(&mut picker.hex)
            .font(FontId::monospace(13.0))
            .char_limit(7)
            .margin(vec2(8.0, 5.0))
            .desired_width(90.0);
        if ui.add(field).changed() {
            // Applied as soon as the text is a complete colour.
            if let Some(parsed) = parse_hex(&picker.hex) {
                *color = parsed;
                let typed = std::mem::take(&mut picker.hex);
                picker.load(parsed);
                picker.hex = typed;
            }
        }
    });
    ui.add_space(6.0);
}

/// A 0..1 slider whose track shows the colour each position gives.
fn gradient_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    color_at: impl Fn(f32) -> Color32,
    theme: &Theme,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
    let p = ui.painter();
    p.text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(12.5), theme.text_dim);

    let knob_r = 8.0;
    let track = Rect::from_min_max(
        pos2(rect.left() + 78.0, rect.center().y - 5.0),
        pos2(rect.right() - knob_r, rect.center().y + 5.0),
    );
    let before = *value;
    if let Some(pos) = resp.interact_pointer_pos() {
        *value = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
    }

    let steps = 48;
    let w = track.width() / steps as f32;
    for i in 0..steps {
        let x = track.left() + i as f32 * w;
        // Overlap by a hair so no seams show between the strips.
        let strip = Rect::from_min_max(pos2(x, track.top()), pos2(x + w + 0.5, track.bottom()));
        p.rect_filled(strip, 0.0, color_at((i as f32 + 0.5) / steps as f32));
    }
    let edge = Stroke::new(1.0_f32, crate::theme::mix(theme.surface, theme.text, 0.35));
    p.rect_stroke(track, 2.0, edge, egui::StrokeKind::Outside);

    let knob = pos2(track.left() + *value * track.width(), rect.center().y);
    p.circle_filled(knob, knob_r, Color32::WHITE);
    p.circle_filled(knob, knob_r - 2.5, color_at(*value));
    p.circle_stroke(knob, knob_r, Stroke::new(1.0_f32, Color32::from_black_alpha(140)));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    *value != before
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
