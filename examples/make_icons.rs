//! Regenerates the icon files in `assets/` from `assets/icon-source.webp`.
//!
//! `cargo run --example make_icons`

use image::{imageops::FilterType, Rgba, RgbaImage};

/// The artwork scaled into a `size` canvas with `pad` transparent pixels on
/// every side and anti-aliased rounded corners.
fn rounded(source: &RgbaImage, size: u32, pad: u32, corner: f32) -> RgbaImage {
    let inner = size - 2 * pad;
    let art = image::imageops::resize(source, inner, inner, FilterType::Lanczos3);
    let radius = corner * inner as f32;
    let half = inner as f32 / 2.0;
    let mut out = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    for (x, y, px) in art.enumerate_pixels() {
        // Signed distance from the pixel centre to the rounded square's edge.
        let (dx, dy) = ((x as f32 + 0.5 - half).abs(), (y as f32 + 0.5 - half).abs());
        let (qx, qy) = ((dx - half + radius).max(0.0), (dy - half + radius).max(0.0));
        let dist = (qx * qx + qy * qy).sqrt() - radius;
        let alpha = (0.5 - dist).clamp(0.0, 1.0);
        let mut px = *px;
        px[3] = (px[3] as f32 * alpha) as u8;
        out.put_pixel(x + pad, y + pad, px);
    }
    out
}

fn png_bytes(img: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode png");
    bytes
}

/// A single-image .ico holding a PNG, which Windows Vista and later accept.
fn ico_bytes(png: &[u8]) -> Vec<u8> {
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // reserved, type = icon, count = 1
    ico.extend_from_slice(&[0, 0, 0, 0]); // 256 x 256, no palette
    ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // data offset
    ico.extend_from_slice(png);
    ico
}

fn main() {
    let source = image::open("assets/icon-source.webp")
        .expect("assets/icon-source.webp")
        .to_rgba8();

    // Window icon on Windows and Linux, and the About page.
    let plain = png_bytes(&rounded(&source, 256, 0, 0.2));
    std::fs::write("assets/icon.png", &plain).unwrap();
    std::fs::write("assets/icon.ico", ico_bytes(&plain)).unwrap();

    // macOS icons sit inside a margin, like the system's own.
    let mac = rounded(&source, 512, 50, 0.225);
    std::fs::write("assets/icon-macos.png", png_bytes(&mac)).unwrap();
}
