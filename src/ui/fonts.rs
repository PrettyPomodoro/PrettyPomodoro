//! Fonts. UI text uses the operating system's regular-weight UI font, which
//! stays crisp on standard-density (1x) displays where the bundled light face
//! turns soft. The light face is kept for the large countdown digits.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// Family name of the bundled light face.
pub const LIGHT: &str = "light";

#[cfg(target_os = "macos")]
const SYSTEM_FONTS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];
#[cfg(target_os = "windows")]
const SYSTEM_FONTS: &[&str] = &[
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SYSTEM_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let bundled = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.insert(FontFamily::Name(LIGHT.into()), bundled);

    // egui panics on a font it cannot parse, so check before handing one over.
    let system = SYSTEM_FONTS
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
