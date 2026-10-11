//! Developer aid: turn the shapes of one rendered frame into an SVG, so the
//! screenshots in the README can be vectors. Text becomes `<text>` in the
//! viewer's own fonts, fitted to the width it has in the app.

use eframe::egui::epaint::{ClippedShape, ColorMode, CornerRadius, PathShape, RectShape, TextShape};
use eframe::egui::{Color32, FontFamily, Rect, Shape, Stroke, StrokeKind, Vec2};
use std::fmt::Write;

/// Font stacks standing in for the app's faces, by egui family.
fn font_stack(family: &FontFamily) -> (&'static str, u32) {
    const SANS: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif";
    match family {
        FontFamily::Monospace => ("ui-monospace, Menlo, Consolas, monospace", 400),
        FontFamily::Name(name) if &**name == crate::ui::fonts::LIGHT => (SANS, 300),
        FontFamily::Name(name) if &**name == crate::ui::fonts::HAND => {
            ("Chalkboard, 'Comic Sans MS', 'Segoe Print', cursive", 400)
        }
        FontFamily::Name(name) if &**name == crate::ui::fonts::SERIF => {
            // Faces with lining figures, so the countdown's digits stand level.
            ("ui-serif, 'New York', 'Palatino Linotype', Palatino, 'Times New Roman', serif", 400)
        }
        _ => (SANS, 400),
    }
}

/// A number with at most two decimals and no trailing zeros.
fn n(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `name="#rrggbb"` plus an opacity attribute when the colour is translucent.
fn paint(name: &str, color: Color32) -> String {
    if color.a() == 0 {
        return format!(" {name}=\"none\"");
    }
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let mut out = format!(" {name}=\"#{r:02x}{g:02x}{b:02x}\"");
    if a < 255 {
        let _ = write!(out, " {name}-opacity=\"{}\"", n(a as f32 / 255.0));
    }
    out
}

fn stroke_attrs(stroke: Stroke) -> String {
    if stroke.width <= 0.0 || stroke.color.a() == 0 {
        return String::new();
    }
    format!("{} stroke-width=\"{}\"", paint("stroke", stroke.color), n(stroke.width))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn render(shapes: &[ClippedShape], size: Vec2) -> String {
    let screen = Rect::from_min_size(Default::default(), size);
    let mut clips: Vec<Rect> = Vec::new();
    let mut body = String::new();
    for clipped in shapes {
        let mut inner = String::new();
        shape(&mut inner, &clipped.shape);
        if inner.is_empty() {
            continue;
        }
        if clipped.clip_rect.contains_rect(screen) {
            body.push_str(&inner);
            continue;
        }
        let clip = clipped.clip_rect.intersect(screen);
        let id = clips.iter().position(|c| *c == clip).unwrap_or_else(|| {
            clips.push(clip);
            clips.len() - 1
        });
        let _ = writeln!(body, "<g clip-path=\"url(#c{id})\">\n{inner}</g>");
    }

    let (w, h) = (n(size.x), n(size.y));
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n"
    );
    if !clips.is_empty() {
        out.push_str("<defs>\n");
        for (id, c) in clips.iter().enumerate() {
            let _ = writeln!(
                out,
                "<clipPath id=\"c{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
                n(c.left()),
                n(c.top()),
                n(c.width()),
                n(c.height())
            );
        }
        out.push_str("</defs>\n");
    }
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

fn shape(out: &mut String, shape: &Shape) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| self::shape(out, s)),
        Shape::Circle(c) => {
            let _ = writeln!(
                out,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}{}/>",
                n(c.center.x),
                n(c.center.y),
                n(c.radius),
                paint("fill", c.fill),
                stroke_attrs(c.stroke)
            );
        }
        Shape::LineSegment { points, stroke } => {
            let _ = writeln!(
                out,
                "<path d=\"M{} {}L{} {}\" fill=\"none\"{}/>",
                n(points[0].x),
                n(points[0].y),
                n(points[1].x),
                n(points[1].y),
                stroke_attrs(*stroke)
            );
        }
        Shape::Path(path) => self::path(out, path),
        Shape::Rect(rect) => self::rect(out, rect),
        Shape::Text(text) => self::text(out, text),
        // Nothing in the app draws these.
        _ => {}
    }
}

fn path(out: &mut String, path: &PathShape) {
    let Some((first, rest)) = path.points.split_first() else {
        return;
    };
    let mut d = format!("M{} {}", n(first.x), n(first.y));
    for p in rest {
        let _ = write!(d, "L{} {}", n(p.x), n(p.y));
    }
    if path.closed {
        d.push('Z');
    }
    let color = match &path.stroke.color {
        ColorMode::Solid(color) => *color,
        ColorMode::UV(_) => Color32::TRANSPARENT,
    };
    let stroke = stroke_attrs(Stroke::new(path.stroke.width, color));
    // Thick lines meet in rounded corners in the app.
    let join = if stroke.is_empty() { "" } else { " stroke-linejoin=\"round\"" };
    let _ = writeln!(out, "<path d=\"{d}\"{}{stroke}{join}/>", paint("fill", path.fill));
}

fn rect(out: &mut String, shape: &RectShape) {
    // SVG strokes straddle the outline; move it to where the app draws.
    let grow = match shape.stroke_kind {
        _ if shape.stroke.width <= 0.0 => 0.0,
        StrokeKind::Inside => -shape.stroke.width / 2.0,
        StrokeKind::Middle => 0.0,
        StrokeKind::Outside => shape.stroke.width / 2.0,
    };
    let r = shape.rect.expand(grow);
    let CornerRadius { nw, .. } = shape.corner_radius;
    let radius = (nw as f32 + grow).clamp(0.0, r.width().min(r.height()) / 2.0);
    let _ = writeln!(
        out,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}{}/>",
        n(r.left()),
        n(r.top()),
        n(r.width()),
        n(r.height()),
        n(radius),
        paint("fill", shape.fill),
        stroke_attrs(shape.stroke)
    );
}

fn text(out: &mut String, shape: &TextShape) {
    for row in &shape.galley.rows {
        // One <text> per run of glyphs sharing a format.
        let mut start = 0;
        while start < row.glyphs.len() {
            let section = row.glyphs[start].section_index;
            let len = row.glyphs[start..].iter().take_while(|g| g.section_index == section).count();
            let run = &row.glyphs[start..start + len];
            start += len;

            let string: String = run.iter().map(|g| g.chr).collect();
            if string.trim().is_empty() {
                continue;
            }
            let format = &shape.galley.job.sections[section as usize].format;
            let color = match shape.override_text_color {
                Some(color) => color,
                None if format.color == Color32::PLACEHOLDER => shape.fallback_color,
                None => format.color,
            };
            let (family, weight) = font_stack(&format.font_id.family);
            // A colour emoji fills more of its box than the app's outline
            // glyph does, so it is set a little smaller.
            let emoji = string.chars().any(|c| c as u32 >= 0x1F000);
            let size = format.font_id.size * if emoji { 0.8 } else { 1.0 };
            let x = shape.pos.x + run[0].pos.x;
            let baseline = shape.pos.y + row.rect.top() + run[0].font_ascent;
            let width: f32 = run.iter().map(|g| g.advance_width).sum::<f32>() - format.extra_letter_spacing;
            // Fitted to its width in the app, so the layout holds in any font.
            let fit = match run.len() {
                1 => String::new(),
                _ => format!(" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\"", n(width)),
            };
            let _ = writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" font-family=\"{family}\" font-size=\"{}\" font-weight=\"{weight}\" style=\"font-variant-numeric:lining-nums\"{}{fit} xml:space=\"preserve\">{}</text>",
                n(x),
                n(baseline),
                n(size),
                paint("fill", color.gamma_multiply(shape.opacity_factor)),
                escape(&string)
            );
        }
    }
}
