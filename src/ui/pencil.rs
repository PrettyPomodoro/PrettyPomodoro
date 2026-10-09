//! Drawing helpers for the pencil art theme style: ink lines that drift like
//! a hand-held pen, hatching and scribbled shadows. Everything is seeded so
//! the drawing holds still from frame to frame.

use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Shape, Stroke, Vec2};
use std::f32::consts::TAU;

/// Deterministic noise in -1..1.
fn noise(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B1) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    x as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn dir(angle: f32) -> Vec2 {
    vec2(angle.cos(), angle.sin())
}

/// An arc traced twice with a slightly wandering radius.
pub fn ink_arc(p: &Painter, center: Pos2, radius: f32, from: f32, to: f32, width: f32, color: Color32, seed: u32) {
    let n = ((to - from).abs() / TAU * 120.0).ceil().max(2.0) as usize;
    for pass in 0..2 {
        let phase = noise(seed, pass) * TAU;
        let points = (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                // Whole-number frequencies, so a full circle closes on itself.
                let drift = 0.55 * (a * 3.0 + phase).sin() + 0.35 * (a * 7.0 + phase * 1.7).sin();
                center + (radius + drift) * dir(a)
            })
            .collect();
        let width = if pass == 0 { width } else { width * 0.55 };
        p.add(Shape::line(points, Stroke::new(width, color)));
    }
}

pub fn ink_circle(p: &Painter, center: Pos2, radius: f32, width: f32, color: Color32, seed: u32) {
    ink_arc(p, center, radius, 0.0, TAU, width, color, seed);
}

/// Slanted hatch strokes across a ring band between two angles.
#[allow(clippy::too_many_arguments)]
pub fn hatch_band(
    p: &Painter,
    center: Pos2,
    radii: (f32, f32),
    angles: (f32, f32),
    step: f32,
    slant: f32,
    stroke: Stroke,
    seed: u32,
) {
    let count = ((angles.1 - angles.0) / step).floor().max(0.0) as u32;
    for i in 0..count {
        let a = angles.0 + (i as f32 + 0.5 + 0.3 * noise(seed, i)) * step;
        // Strokes vary in length, as they do when shading by hand.
        let reach = 0.75 + 0.25 * noise(seed ^ 0xA5, i).abs();
        let outer = radii.0 + (radii.1 - radii.0) * reach;
        p.line_segment([center + radii.0 * dir(a), center + outer * dir(a + slant)], stroke);
    }
}

/// A scribbled ground shadow: a stack of uneven horizontal strokes.
pub fn shadow(p: &Painter, center: Pos2, width: f32, color: Color32, seed: u32) {
    for row in 0..5u32 {
        let y = center.y + row as f32 * 2.6;
        let taper = 1.0 - 0.13 * (row as f32 - 1.5).abs();
        let half = width / 2.0 * taper + 9.0 * noise(seed, row);
        let shift = 6.0 * noise(seed ^ 0x51, row);
        let n = 24;
        let points = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let x = center.x + shift - half + 2.0 * half * t;
                pos2(x, y + 0.7 * noise(seed.wrapping_add(row * 97), i))
            })
            .collect();
        p.add(Shape::line(points, Stroke::new(1.1_f32, color)));
    }
}

/// The leafy top of a tomato, sitting on `top`.
pub fn calyx(p: &Painter, top: Pos2, green: Color32, ink: Color32) {
    let outline = Stroke::new(1.3_f32, ink);
    // (direction in degrees, length); screen y points down.
    let leaves = [(197.0_f32, 31.0_f32), (158.0, 23.0), (-17.0, 31.0), (22.0, 23.0), (95.0, 15.0)];
    for (deg, len) in leaves {
        let along = dir(deg.to_radians());
        let across = vec2(-along.y, along.x) * 5.0;
        let points = vec![top - across, top + along * len * 0.45 - across * 0.75, top + along * len, top + along * len * 0.4 + across * 0.9, top + across];
        p.add(Shape::convex_polygon(points, green, outline));
    }
    let up = dir((-72.0_f32).to_radians());
    let across = vec2(-up.y, up.x) * 2.6;
    let stem = vec![top - across, top + up * 17.0 - across * 0.8, top + up * 17.0 + across * 0.8, top + across];
    p.add(Shape::convex_polygon(stem, green, outline));
    p.circle_filled(top, 4.8, green);
}
