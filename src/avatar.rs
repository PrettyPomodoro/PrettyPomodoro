//! Profile pictures: turning a file the user picked into something small enough
//! to upload, and drawing what comes back — or their initials, when there is
//! nothing to draw.

use crate::theme::{luminance, Theme};
use eframe::egui::{self, pos2, vec2, Align2, Color32, ColorImage, FontId, Painter, Rect, TextureHandle};
use std::path::Path;

/// Pictures are stored and drawn as a square of this many pixels. Big enough
/// for a crisp avatar on a retina screen, small enough to be a few kilobytes.
pub const SIZE: u32 = 128;
/// Images the app will even try to open, for the file dialog's filter.
pub const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "webp", "PNG", "JPG"];

/// Read an image file, square it off, shrink it and encode the PNG the server
/// takes. Done on a background thread: a camera photo is slow to decode.
pub fn prepare(path: &Path) -> Result<Vec<u8>, String> {
    let image = image::open(path)
        .map_err(|e| format!("That file could not be read as a picture ({e})."))?;
    // Centre-crop to a square first, so nobody comes out stretched.
    let (width, height) = (image.width(), image.height());
    let side = width.min(height);
    if side == 0 {
        return Err("That picture is empty.".into());
    }
    let square = image.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let small = square.resize_exact(SIZE, SIZE, image::imageops::FilterType::Lanczos3);

    let mut png = Vec::new();
    small
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("That picture could not be saved as a PNG ({e})."))?;
    Ok(png)
}

/// Decode a stored picture with its corners taken off, so it draws as a circle
/// on any background without needing a mask at paint time.
pub fn round(bytes: &[u8]) -> Result<ColorImage, String> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|e| format!("The picture could not be read ({e})."))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    let (centre_x, centre_y) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = centre_x.min(centre_y);

    let pixels = decoded
        .enumerate_pixels()
        .map(|(x, y, pixel)| {
            let [r, g, b, a] = pixel.0;
            let (dx, dy) = (x as f32 + 0.5 - centre_x, y as f32 + 0.5 - centre_y);
            // One pixel of feathering, so the edge is not a staircase.
            let edge = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * edge).round() as u8)
        })
        .collect();
    Ok(ColorImage { size: [width as usize, height as usize], pixels })
}

/// Draw a picture, or initials on a coloured disc when there is none.
pub fn draw(
    painter: &Painter,
    rect: Rect,
    picture: Option<&TextureHandle>,
    name: &str,
    theme: &Theme,
) {
    // Keep it square and centred, whatever it was handed.
    let side = rect.width().min(rect.height());
    let rect = Rect::from_center_size(rect.center(), vec2(side, side));
    match picture {
        Some(texture) => {
            let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture.id(), rect, whole, Color32::WHITE);
        }
        None => {
            let fill = tint(name, theme);
            painter.circle_filled(rect.center(), side / 2.0, fill);
            let on_fill = if luminance(fill) > 0.55 {
                Color32::from_rgb(0x1a, 0x1a, 0x18)
            } else {
                Color32::from_rgb(0xf5, 0xf3, 0xef)
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                initials(name),
                FontId::proportional(side * 0.42),
                on_fill,
            );
        }
    }
}

/// Up to two letters standing in for a name.
pub fn initials(name: &str) -> String {
    let letters: Vec<char> = name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect();
    if letters.is_empty() {
        return "?".to_owned();
    }
    letters.into_iter().flat_map(char::to_uppercase).collect()
}

/// A colour for someone's disc, picked from the theme so every face belongs to
/// whatever palette is on. The same name always gets the same one.
fn tint(name: &str, theme: &Theme) -> Color32 {
    let choices = [theme.focus, theme.short_break, theme.long_break, theme.accent];
    choices[scramble(name) as usize % choices.len()]
}

/// A well-spread number from a name. FNV on its own barely moves the low bits,
/// which is all that survives the modulus, so it gets a finishing mix.
fn scramble(name: &str) -> u32 {
    let mut hash = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7feb_352d);
    hash ^= hash >> 15;
    hash
}

/// A circular button showing someone's picture. Brightens on hover like the
/// icon buttons beside it.
pub fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    picture: Option<&TextureHandle>,
    name: &str,
    theme: &Theme,
) -> egui::Response {
    let resp = ui
        .interact(rect, ui.id().with(("avatar", rect.min.x as i32)), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let side = rect.width().min(rect.height());
    let face = Rect::from_center_size(rect.center(), vec2(side - 8.0, side - 8.0));
    draw(ui.painter(), face, picture, name, theme);
    let ring = if resp.hovered() { theme.text } else { theme.text_dim };
    ui.painter().circle_stroke(
        face.center(),
        face.width() / 2.0,
        egui::Stroke::new(1.3_f32, ring),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_come_from_the_first_two_words() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ada"), "A");
        assert_eq!(initials("  ada   byron   king  "), "AB");
        assert_eq!(initials("Ada Lovelace King"), "AL", "two is enough");
        assert_eq!(initials("周 樹人"), "周樹");
        assert_eq!(initials("🙂"), "?", "a name with no letters still draws");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn a_name_always_gets_the_same_colour() {
        let theme = crate::theme::builtin_themes().remove(0);
        assert_eq!(tint("Ada", &theme), tint("Ada", &theme));
        let all = [theme.focus, theme.short_break, theme.long_break, theme.accent];
        assert!(all.contains(&tint("Someone Else", &theme)), "always a theme colour");
    }

    #[test]
    fn names_are_spread_across_the_choices() {
        // Short names all landing on one colour is exactly what a plain FNV
        // hash does here, and it makes every face on the board look alike.
        let names = ["Priya", "Marcus", "Ada", "Yuki", "Tomas", "Noor", "Bo", "Cy"];
        let buckets: std::collections::HashSet<u32> =
            names.iter().map(|n| scramble(n) % 4).collect();
        assert!(buckets.len() >= 3, "eight names should not share one colour: {buckets:?}");
    }

    #[test]
    fn rounding_keeps_the_middle_and_clears_the_corners() {
        // A 64x64 opaque red square, encoded as a PNG.
        let square = image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(square)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let rounded = round(&png).expect("a readable PNG");
        assert_eq!(rounded.size, [64, 64]);
        let at = |x: usize, y: usize| rounded.pixels[y * 64 + x];
        assert_eq!(at(32, 32).a(), 255, "the middle is kept");
        assert_eq!(at(0, 0).a(), 0, "the corners are taken off");
        assert_eq!(at(63, 63).a(), 0);
        assert_eq!(at(32, 32).r(), 255, "and the colour survives");
    }

    #[test]
    fn an_unreadable_file_is_reported_not_panicked_on() {
        assert!(round(b"not a picture").is_err());
        assert!(prepare(std::path::Path::new("/no/such/file.png")).is_err());
    }
}
