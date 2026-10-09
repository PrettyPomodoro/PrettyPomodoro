//! Fonts. UI text uses the operating system's regular-weight UI font, which
//! stays crisp on standard-density (1x) displays where the bundled light face
//! turns soft. The light face is kept for the large countdown digits.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::Arc;

/// Family name of the bundled light face.
pub const LIGHT: &str = "light";

/// Regular-weight UI fonts to try, best first. If none is present or
/// readable, the bundled face is used, so text renders on every system; all
/// UI strings stick to Latin letters and punctuation that each of these and
/// the bundled face contain.
fn system_fonts() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Supplemental/Arial.ttf"]
            .map(PathBuf::from)
            .to_vec()
    } else if cfg!(target_os = "windows") {
        let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        ["segoeui.ttf", "arial.ttf", "tahoma.ttf"]
            .map(|file| PathBuf::from(&windows).join("Fonts").join(file))
            .to_vec()
    } else {
        // Debian/Ubuntu, Fedora and Arch locations of common UI fonts.
        [
            "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]
        .map(PathBuf::from)
        .to_vec()
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let bundled = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.insert(FontFamily::Name(LIGHT.into()), bundled);

    // egui panics on a font it cannot parse, so check before handing one over.
    let system = system_fonts()
        .iter()
        .filter_map(|path| std::fs::read(path).ok())
        .find(|bytes| ab_glyph::FontRef::try_from_slice(bytes).is_ok());
    if let Some(bytes) = system {
        fonts
            .font_data
            .insert("system-ui".into(), Arc::new(FontData::from_owned(bytes)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            // Ahead of the bundled faces for text; they remain as fallbacks
            // (monospace keeps its own face first so digits stay aligned).
            let list = fonts.families.entry(family.clone()).or_default();
            let at = if family == FontFamily::Monospace { 1.min(list.len()) } else { 0 };
            list.insert(at, "system-ui".into());
        }
    }
    ctx.set_fonts(fonts);
}
