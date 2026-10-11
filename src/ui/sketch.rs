//! Drawing helpers for the sketch theme style, after the chart grammar of its
//! design system: every line wobbles as if drawn by hand, grid lines are
//! dashed. The wobble depends only on the angle and a seed, so the drawing
//! holds still from frame to frame.

use eframe::egui::{vec2, Painter, Pos2, Shape, Stroke};
use std::f32::consts::TAU;

/// How far a hand-drawn circle of this seed strays from its radius at `angle`.
pub fn wobble(angle: f32, seed: u32) -> f32 {
    let phase = seed as f32 * 2.4;
    // Whole-number frequencies, so a full circle closes on itself.
    1.1 * (angle * 3.0 + phase).sin() + 0.7 * (angle * 7.0 + phase * 1.7).sin()
}

/// A wobbly arc, drawn from angle `from` to `to`. With `dash` (dash and gap
/// lengths) the pattern starts at `from`.
pub fn arc(p: &Painter, center: Pos2, radius: f32, from: f32, to: f32, stroke: Stroke, dash: Option<(f32, f32)>, seed: u32) {
    let n = ((to - from).abs() / TAU * 160.0).ceil().max(2.0) as usize;
    let points: Vec<_> = (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            center + (radius + wobble(a, seed)) * vec2(a.cos(), a.sin())
        })
        .collect();
    match dash {
        Some((dash, gap)) => p.extend(Shape::dashed_line(&points, stroke, dash, gap)),
        None => {
            p.add(Shape::line(points, stroke));
        }
    }
}
