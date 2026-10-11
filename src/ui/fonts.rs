//! Fonts. UI text uses the operating system's regular-weight UI font, which
//! stays crisp on standard-density (1x) displays where the bundled light face
//! turns soft. The light face is kept for the large countdown digits.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::Arc;

/// Family name of the bundled light face.
pub const LIGHT: &str = "light";

/// Family name of the handwritten face used by the sketch theme style.
pub const HAND: &str = "hand";
/// Family name of the serif face used by the editorial theme style.
pub const SERIF: &str = "serif";

/// Handwritten fonts to try, best first: stand-ins for the sketch design
/// system's Patrick Hand, from its own fallback list where the OS has them.
fn hand_fonts() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        ["Chalkboard.ttc", "Comic Sans MS.ttf", "Bradley Hand Bold.ttf"]
            .map(|file| PathBuf::from("/System/Library/Fonts/Supplemental").join(file))
            .to_vec()
    } else if cfg!(target_os = "windows") {
        windows_fonts(&["segoepr.ttf", "comic.ttf"])
    } else {
        [
            "/usr/share/fonts/truetype/comic-neue/ComicNeue-Regular.ttf",
            "/usr/share/fonts/comic-neue/ComicNeue-Regular.ttf",
            "/usr/share/fonts/TTF/ComicNeue-Regular.ttf",
        ]
        .map(PathBuf::from)
        .to_vec()
    }
}

/// Serif fonts to try, best first: stand-ins for the editorial design system's
/// Newsreader.
fn serif_fonts() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/NewYork.ttf", "/System/Library/Fonts/Supplemental/Georgia.ttf"]
            .map(PathBuf::from)
            .to_vec()
    } else if cfg!(target_os = "windows") {
        windows_fonts(&["georgia.ttf", "times.ttf"])
    } else {
        [
            "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSerif-Regular.ttf",
            "/usr/share/fonts/noto/NotoSerif-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
            "/usr/share/fonts/liberation-serif/LiberationSerif-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            "/usr/share/fonts/dejavu-serif-fonts/DejaVuSerif.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif.ttf",
        ]
        .map(PathBuf::from)
        .to_vec()
    }
}

fn windows_fonts(files: &[&str]) -> Vec<PathBuf> {
    let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    files.iter().map(|file| PathBuf::from(&windows).join("Fonts").join(file)).collect()
}

/// The first font in `paths` that is present and parses. egui panics on a font
/// it cannot parse, so check before handing one over.
fn first_usable(paths: &[PathBuf]) -> Option<Vec<u8>> {
    paths
        .iter()
        .filter_map(|path| std::fs::read(path).ok())
        .find(|bytes| ab_glyph::FontRef::try_from_slice(bytes).is_ok())
}

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
        windows_fonts(&["segoeui.ttf", "arial.ttf", "tahoma.ttf"])
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

    if let Some(bytes) = first_usable(&system_fonts()) {
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

    // The style faces fall back to the UI text fonts where the OS has none.
    for (name, paths) in [(HAND, hand_fonts()), (SERIF, serif_fonts())] {
        let mut list = fonts.families[&FontFamily::Proportional].clone();
        if let Some(bytes) = first_usable(&paths) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            list.insert(0, name.into());
        }
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}
