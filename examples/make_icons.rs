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

/// Grow (`grow` true) or shrink a mask by `radius` pixels.
fn morph(mask: &[bool], n: usize, radius: i32, grow: bool) -> Vec<bool> {
    let mut out = vec![!grow; n * n];
    for y in 0..n as i32 {
        for x in 0..n as i32 {
            'search: for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    let inside = sx >= 0 && sy >= 0 && sx < n as i32 && sy < n as i32;
                    let set = inside && mask[sy as usize * n + sx as usize];
                    if set == grow {
                        out[y as usize * n + x as usize] = grow;
                        break 'search;
                    }
                }
            }
        }
    }
    out
}

/// Fill the gaps in a mask that are narrower than about twice `radius`.
fn close(mask: &[bool], n: usize, radius: i32) -> Vec<bool> {
    morph(&morph(mask, n, radius, true), n, radius, false)
}

/// The tomato alone, in white on a transparent background, for the menu bar
/// and system tray: the fruit as one solid shape with the leaves set off from
/// it by a thin gap. The artwork's blue background and ground shadow are left out.
fn silhouette(source: &RgbaImage, size: u32) -> RgbaImage {
    const N: usize = 384;
    let small = image::imageops::resize(source, N as u32, N as u32, FilterType::Lanczos3);
    let (mut red, mut leaf) = (vec![false; N * N], vec![false; N * N]);
    for (x, y, px) in small.enumerate_pixels() {
        let [r, g, b, _] = px.0.map(|c| c as f32);
        let i = y as usize * N + x as usize;
        red[i] = r > g * 1.3 && r > b * 1.3 && r > 90.0;
        leaf[i] = g > r * 1.05 && g > b * 1.05 && g > 60.0;
    }
    // Closing fills in the ink lines drawn across the fruit and the leaves.
    let leaf = close(&leaf, N, 4);
    let fruit = close(&red.iter().zip(&leaf).map(|(r, l)| *r || *l).collect::<Vec<_>>(), N, 8);
    let fruit = morph(&fruit, N, 2, true);
    let around_leaf = morph(&leaf, N, 12, true);

    let mut mask = RgbaImage::from_pixel(N as u32, N as u32, Rgba([255, 255, 255, 0]));
    let (mut min, mut max) = ((N as u32, N as u32), (0, 0));
    for i in 0..N * N {
        let gap = around_leaf[i] && !leaf[i];
        if (fruit[i] || leaf[i]) && !gap {
            let (x, y) = ((i % N) as u32, (i / N) as u32);
            mask.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
        }
    }
    // Crop to the tomato, centre it in a square and scale that down.
    let (w, h) = (max.0 - min.0 + 1, max.1 - min.1 + 1);
    let side = w.max(h);
    let mut square = RgbaImage::from_pixel(side, side, Rgba([255, 255, 255, 0]));
    let cropped = image::imageops::crop_imm(&mask, min.0, min.1, w, h).to_image();
    image::imageops::replace(&mut square, &cropped, ((side - w) / 2) as i64, ((side - h) / 2) as i64);
    let mut out = image::imageops::resize(&square, size, size, FilterType::Lanczos3);
    for px in out.pixels_mut() {
        px.0 = [255, 255, 255, px.0[3]];
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

    // Menu bar and system tray: the tomato on its own, in white.
    std::fs::write("assets/icon-tray.png", png_bytes(&silhouette(&source, 88))).unwrap();
}
