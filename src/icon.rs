//! The app icon, embedded at build time. Regenerate the files with
//! `cargo run --example make_icons`.

use eframe::egui;

/// Rounded artwork without a margin: window icon on Windows and Linux, About page.
const PLAIN: &[u8] = include_bytes!("../assets/icon.png");
/// With the margin macOS expects around Dock icons.
#[cfg(target_os = "macos")]
const WINDOW: &[u8] = include_bytes!("../assets/icon-macos.png");
#[cfg(not(target_os = "macos"))]
const WINDOW: &[u8] = PLAIN;

fn decode(png: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .expect("embedded icon is a valid PNG")
        .to_rgba8()
}

pub fn window_icon() -> egui::IconData {
    let img = decode(WINDOW);
    egui::IconData {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    }
}

pub fn about_image() -> egui::ColorImage {
    let img = decode(PLAIN);
    let size = [img.width() as usize, img.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}
