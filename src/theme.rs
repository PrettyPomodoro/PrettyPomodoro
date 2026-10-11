//! Colour themes: flat light and dark palettes, the pencil art family and two
//! families that follow a design system each (Sketch and Editorial). Users can
//! add their own on the Themes page.

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

/// How a theme is drawn, independent of its colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    /// Flat shapes.
    #[default]
    Standard,
    /// Hand-drawn illustration look: ink outlines, hatching, wobbly lines.
    PencilArt,
    /// Greyscale interface around a timer drawn like a hand-sketched chart:
    /// dashed grid rings, a wobbly series line, handwritten figures.
    Sketch,
    /// Warm editorial look: serif figures and clean ink outlines on paper.
    Editorial,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Standard, Style::PencilArt, Style::Sketch, Style::Editorial];

    pub fn label(self) -> &'static str {
        match self {
            Style::Standard => "Light / Dark",
            Style::PencilArt => "Pencil Art",
            Style::Sketch => "Sketch",
            Style::Editorial => "Editorial",
        }
    }
}

/// The group a theme is listed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Light,
    Dark,
    PencilArt,
    Sketch,
    Editorial,
}

impl Family {
    pub const ALL: [Family; 5] =
        [Family::Light, Family::Dark, Family::PencilArt, Family::Sketch, Family::Editorial];

    pub fn label(self) -> &'static str {
        match self {
            Family::Light => "Light",
            Family::Dark => "Dark",
            Family::PencilArt => "Pencil Art",
            Family::Sketch => "Sketch",
            Family::Editorial => "Editorial",
        }
    }
}

/// Outline colour of the pencil art style.
pub const INK: Color32 = Color32::from_rgb(16, 14, 20);
/// Outline and text colour of the editorial style.
pub const EDITORIAL_INK: Color32 = Color32::from_rgb(0x14, 0x14, 0x13);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub style: Style,
    #[serde(with = "hex")]
    pub focus: Color32,
    #[serde(with = "hex")]
    pub short_break: Color32,
    #[serde(with = "hex")]
    pub long_break: Color32,
    /// Window chrome and the timer page.
    #[serde(with = "hex")]
    pub background: Color32,
    /// Settings pages and raised surfaces.
    #[serde(with = "hex")]
    pub surface: Color32,
    /// Outlines, slider tracks, inactive icons.
    #[serde(with = "hex")]
    pub muted: Color32,
    #[serde(with = "hex")]
    pub text: Color32,
    #[serde(with = "hex")]
    pub text_dim: Color32,
    #[serde(with = "hex")]
    pub accent: Color32,
}

impl Theme {
    pub fn is_dark(&self) -> bool {
        luminance(self.background) < 0.5
    }

    pub fn family(&self) -> Family {
        match self.style {
            Style::PencilArt => Family::PencilArt,
            Style::Sketch => Family::Sketch,
            Style::Editorial => Family::Editorial,
            Style::Standard if self.is_dark() => Family::Dark,
            Style::Standard => Family::Light,
        }
    }

    /// The outline colour, for themes drawn in a style that outlines its shapes.
    pub fn ink(&self) -> Option<Color32> {
        match self.style {
            Style::PencilArt => Some(INK),
            Style::Editorial => Some(EDITORIAL_INK),
            Style::Standard | Style::Sketch => None,
        }
    }

    /// Text colour on an accent fill. Editorial sets ink on its accent, never white.
    pub fn on_accent(&self) -> Color32 {
        match self.style {
            Style::Editorial => EDITORIAL_INK,
            _ => self.surface,
        }
    }
}

pub fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

mod hex {
    use super::*;
    use serde::{de::Error, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(c: &Color32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(*c))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex(&s).ok_or_else(|| D::Error::custom(format!("invalid colour: {s}")))
    }
}

pub const DEFAULT_THEME: &str = "Graphite";

/// Built-in themes that used to go by another name: (old, new).
const RENAMED: &[(&str, &str)] = &[
    ("Catppuccin Mocha", "Velvet Dusk"),
    ("Catppuccin Latte", "Milk Foam"),
    ("Tokyo Night", "Neon Harbor"),
    ("Dracula", "Midnight Orchid"),
    ("Nord", "Arctic Slate"),
    ("Gruvbox Light", "Old Parchment"),
    ("Gruvbox", "Ember Bark"),
    ("Rosé Pine Dawn", "Blush Dawn"),
    ("Rosé Pine", "Plum Twilight"),
    ("One Dark", "Quiet Carbon"),
    ("Solarized Dark", "Deep Lagoon"),
    ("Solarized Light", "Sunlit Sand"),
    ("GitHub Dark", "Ink Well"),
    ("GitHub Light", "Clean Sheet"),
    ("Everforest", "Mossy Hollow"),
    ("Kanagawa", "Indigo Tide"),
    ("Monokai Pro", "Candy Charcoal"),
    ("Night Owl", "Abyss Blue"),
    ("Ayu Mirage", "Desert Night"),
    ("Synthwave", "Retro Arcade"),
];

/// The current name of a built-in theme saved under its old one.
pub fn current_name(name: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(old, _)| *old == name).map(|(_, new)| *new)
}

/// name, focus, short, long, background, surface, muted, text, text_dim, accent
#[rustfmt::skip]
const BUILTIN: &[[&str; 10]] = &[
    ["Graphite",        "505154", "505154", "505154", "ebebea", "fcfcfc", "adafb1", "27292d", "656a75", "08568c"],
    ["Velvet Dusk",     "f38ba8", "a6e3a1", "89b4fa", "1e1e2e", "313244", "585b70", "cdd6f4", "bac2de", "cba6f7"],
    ["Milk Foam",       "d20f39", "40a02b", "209fb5", "eff1f5", "e6e9ef", "bcc0cc", "4c4f69", "6c6f85", "8839ef"],
    ["Neon Harbor",     "f7768e", "73daca", "7aa2f7", "24283b", "1b1e2e", "565f89", "c0caf5", "9aa5ce", "9d7cd8"],
    ["Midnight Orchid", "ff5555", "50fa7b", "8be9fd", "282a36", "44475a", "6272a4", "f8f8f2", "bfc3d9", "bd93f9"],
    ["Arctic Slate",    "bf616a", "a3be8c", "81a1c1", "2e3440", "3b4252", "616e88", "d8dee9", "a9b4c7", "88c0d0"],
    ["Ember Bark",      "fb4934", "b8bb26", "83a598", "282828", "3c3836", "7c6f64", "ebdbb2", "bdae93", "fabd2f"],
    ["Old Parchment",   "9d0006", "79740e", "076678", "fbf1c7", "ebdbb2", "bdae93", "3c3836", "665c54", "b57614"],
    ["Plum Twilight",   "eb6f92", "9ccfd8", "c4a7e7", "191724", "1f1d2e", "524f67", "e0def4", "908caa", "c4a7e7"],
    ["Blush Dawn",      "b4637a", "56949f", "907aa9", "faf4ed", "fffaf3", "dfdad9", "575279", "797593", "ea9d34"],
    ["Quiet Carbon",    "e06c75", "98c379", "61afef", "282c34", "3b4048", "5c6370", "abb2bf", "8b92a0", "c678dd"],
    ["Deep Lagoon",     "dc322f", "859900", "2aa198", "002b36", "073642", "586e75", "93a1a1", "839496", "268bd2"],
    ["Sunlit Sand",     "cb4b16", "859900", "2aa198", "fdf6e3", "eee8d5", "93a1a1", "586e75", "657b83", "268bd2"],
    ["Ink Well",        "f85149", "3fb950", "58a6ff", "0d1117", "161b22", "30363d", "e6edf3", "8b949e", "58a6ff"],
    ["Clean Sheet",     "cf222e", "1a7f37", "8250df", "ffffff", "f6f8fa", "d0d7de", "24292f", "57606a", "0969da"],
    ["Mossy Hollow",    "e67e80", "a7c080", "7fbbb3", "2d353b", "3d484d", "596a6d", "d3c6aa", "9da9a0", "a7c080"],
    ["Indigo Tide",     "c34043", "76946a", "7e9cd8", "1f1f28", "2a2a37", "54546d", "dcd7ba", "c8c093", "957fb8"],
    ["Candy Charcoal",  "ff6188", "a9dc76", "78dce8", "2d2a2e", "403e41", "5b595c", "fcfcfa", "939293", "ffd866"],
    ["Abyss Blue",      "ef5350", "addb67", "82aaff", "011627", "0d2231", "1d3b53", "d6deeb", "7fa3b3", "c792ea"],
    ["Desert Night",    "f28779", "bae67e", "5ccfe6", "1f2430", "2a3546", "707a8c", "cbccc6", "a0a6ae", "ffcc66"],
    ["Retro Arcade",    "ff7edb", "72f1b8", "36f9f6", "262335", "372d4b", "495495", "e0e3e6", "b893ce", "fede5d"],
];

/// The Pencil Art family. Tomato Sketch is the app icon's blue, tomato red and
/// leaf green; the others keep its tomato on a different sheet of paper.
#[rustfmt::skip]
const PENCIL_ART: &[[&str; 10]] = &[
    ["Tomato Sketch", "e5392b", "7cbf4b", "f6c544", "2a62e6", "2456d3", "153a9c", "fff7e8", "d3dfff", "8fd155"],
    ["Paper Sketch",  "e5392b", "7cbf4b", "f6c544", "fff7e8", "f6ecd6", "b9a98a", "100e14", "5b5246", "e5392b"],
    ["Garden Sketch", "e5392b", "a8e063", "f6c544", "2f8f5b", "27804f", "185c37", "fff7e8", "d3f0dc", "f6c544"],
    ["Sunny Sketch",  "e5392b", "4f9d2d", "2a62e6", "f6c544", "efb92e", "b98a12", "100e14", "5a4508", "e5392b"],
    ["Sky Sketch",    "e5392b", "4f9d2d", "f6c544", "8fd1ff", "7cc4f5", "3f8fc4", "100e14", "1f4d6e", "e5392b"],
];

/// The Sketch family. The timer page is the system's `surface` and the settings
/// pages its `canvas`; the one hue belongs to the focus series alone, breaks
/// are the grey comparison series, and controls stay greyscale. The hue is the
/// system's green, its link blue or its seal gold.
#[rustfmt::skip]
const SKETCH: &[[&str; 10]] = &[
    ["Sketch Light",     "16a34a", "737373", "404040", "ffffff", "fafafa", "999999", "171717", "737373", "404040"],
    ["Sketch Dark",      "22c55e", "a3a3a3", "d4d4d4", "171717", "0a0a0a", "7a7a7a", "f5f5f5", "a3a3a3", "d4d4d4"],
    ["Sketch Blue",      "1d4ed8", "737373", "404040", "ffffff", "fafafa", "999999", "171717", "737373", "404040"],
    ["Sketch Blue Dark", "60a5fa", "a3a3a3", "d4d4d4", "171717", "0a0a0a", "7a7a7a", "f5f5f5", "a3a3a3", "d4d4d4"],
    ["Sketch Gold",      "d8a83c", "a3a3a3", "d4d4d4", "171717", "0a0a0a", "7a7a7a", "f5f5f5", "a3a3a3", "d4d4d4"],
];

/// The Editorial family: terracotta focus, teal and amber breaks, ink text, on
/// each of the system's canvases and illustration tints. Muted sits between
/// the system's `rule` and `ink-muted` so tracks stay visible; on the tinted
/// canvases the secondary text is darkened to stay legible.
#[rustfmt::skip]
const EDITORIAL: &[[&str; 10]] = &[
    ["Editorial",          "d97757", "0e7c8c", "f29f05", "f3f2ee", "ffffff", "b9b8b2", "141413", "5e5d59", "d97757"],
    ["Editorial Warm",     "d97757", "0e7c8c", "f29f05", "e5cabb", "f3f2ee", "a8978c", "141413", "4d4c48", "d97757"],
    ["Editorial White",    "d97757", "0e7c8c", "f29f05", "ffffff", "f3f2ee", "b9b8b2", "141413", "5e5d59", "d97757"],
    ["Editorial Sage",     "d97757", "0e7c8c", "f29f05", "b8d0c6", "f3f2ee", "88a096", "141413", "45504b", "d97757"],
    ["Editorial Lavender", "d97757", "0e7c8c", "f29f05", "c9c8e0", "f3f2ee", "9897b4", "141413", "4a4960", "d97757"],
];

/// The first built-in theme drawn in `style`: the starting palette for a new
/// theme of that style.
pub fn first_of_style(style: Style) -> Theme {
    let found = builtin_themes().into_iter().find(|t| t.style == style);
    found.expect("a built-in theme per style")
}

pub fn builtin_themes() -> Vec<Theme> {
    let mut themes: Vec<_> = BUILTIN
        .iter()
        .map(|t| from_row(t, Style::Standard))
        .collect();
    themes.extend(PENCIL_ART.iter().map(|t| from_row(t, Style::PencilArt)));
    themes.extend(SKETCH.iter().map(|t| from_row(t, Style::Sketch)));
    themes.extend(EDITORIAL.iter().map(|t| from_row(t, Style::Editorial)));
    themes
}

fn from_row(t: &[&str; 10], style: Style) -> Theme {
    let c = |s: &str| parse_hex(s).expect("built-in colour");
    Theme {
        name: t[0].to_owned(),
        style,
        focus: c(t[1]),
        short_break: c(t[2]),
        long_break: c(t[3]),
        background: c(t[4]),
        surface: c(t[5]),
        muted: c(t[6]),
        text: c(t[7]),
        text_dim: c(t[8]),
        accent: c(t[9]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse_and_are_unique() {
        let themes = builtin_themes();
        assert!(themes.iter().any(|t| t.name == DEFAULT_THEME));
        let mut names: Vec<_> = themes.iter().map(|t| &t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), themes.len());
    }

    #[test]
    fn renamed_themes_resolve() {
        let themes = builtin_themes();
        for (old, new) in RENAMED {
            assert!(themes.iter().all(|t| t.name != *old), "{old}");
            assert!(themes.iter().any(|t| t.name == *new), "{new}");
            assert_eq!(current_name(old), Some(*new));
        }
    }

    #[test]
    fn hex_round_trip() {
        let t = &builtin_themes()[0];
        let json = serde_json::to_string(t).unwrap();
        assert_eq!(&serde_json::from_str::<Theme>(&json).unwrap(), t);
    }

    #[test]
    fn families() {
        let themes = builtin_themes();
        for family in Family::ALL {
            assert!(themes.iter().filter(|t| t.family() == family).count() >= 5, "{family:?}");
        }
        for style in Style::ALL {
            assert_eq!(first_of_style(style).style, style);
        }
        // Themes saved before styles existed load as standard ones.
        let mut json = serde_json::to_value(first_of_style(Style::PencilArt)).unwrap();
        json.as_object_mut().unwrap().remove("style");
        let old: Theme = serde_json::from_value(json).unwrap();
        assert_eq!(old.family(), Family::Dark);
    }
}
