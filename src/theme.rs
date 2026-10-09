//! Colour themes. Built-in palettes follow the well-known editor colour
//! schemes of the same name; users can add their own on the Themes page.

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
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

/// name, focus, short, long, background, surface, muted, text, text_dim, accent
#[rustfmt::skip]
const BUILTIN: &[[&str; 10]] = &[
    ["Graphite",         "505154", "505154", "505154", "ebebea", "fcfcfc", "adafb1", "27292d", "656a75", "08568c"],
    ["Pomotroid",        "ff4e4d", "05ec8c", "0bbddb", "2f384b", "3d4457", "9ca5b5", "f6f2eb", "c0c9da", "05ec8c"],
    ["Pomotroid Light",  "c84840", "3a7d58", "6aaec4", "f5f0e8", "ede6db", "c4bdb5", "2f384b", "55647a", "3a7d58"],
    ["Catppuccin Mocha", "f38ba8", "a6e3a1", "89b4fa", "1e1e2e", "313244", "585b70", "cdd6f4", "bac2de", "cba6f7"],
    ["Catppuccin Latte", "d20f39", "40a02b", "209fb5", "eff1f5", "e6e9ef", "bcc0cc", "4c4f69", "6c6f85", "8839ef"],
    ["Tokyo Night",      "f7768e", "73daca", "7aa2f7", "24283b", "1b1e2e", "565f89", "c0caf5", "9aa5ce", "9d7cd8"],
    ["Dracula",          "ff5555", "50fa7b", "8be9fd", "282a36", "44475a", "6272a4", "f8f8f2", "bfc3d9", "bd93f9"],
    ["Nord",             "bf616a", "a3be8c", "81a1c1", "2e3440", "3b4252", "616e88", "d8dee9", "a9b4c7", "88c0d0"],
    ["Gruvbox",          "fb4934", "b8bb26", "83a598", "282828", "3c3836", "7c6f64", "ebdbb2", "bdae93", "fabd2f"],
    ["Gruvbox Light",    "9d0006", "79740e", "076678", "fbf1c7", "ebdbb2", "bdae93", "3c3836", "665c54", "b57614"],
    ["Rosé Pine",        "eb6f92", "9ccfd8", "c4a7e7", "191724", "1f1d2e", "524f67", "e0def4", "908caa", "c4a7e7"],
    ["Rosé Pine Dawn",   "b4637a", "56949f", "907aa9", "faf4ed", "fffaf3", "dfdad9", "575279", "797593", "ea9d34"],
    ["One Dark",         "e06c75", "98c379", "61afef", "282c34", "3b4048", "5c6370", "abb2bf", "8b92a0", "c678dd"],
    ["Solarized Dark",   "dc322f", "859900", "2aa198", "002b36", "073642", "586e75", "93a1a1", "839496", "268bd2"],
    ["Solarized Light",  "cb4b16", "859900", "2aa198", "fdf6e3", "eee8d5", "93a1a1", "586e75", "657b83", "268bd2"],
    ["GitHub Dark",      "f85149", "3fb950", "58a6ff", "0d1117", "161b22", "30363d", "e6edf3", "8b949e", "58a6ff"],
    ["GitHub Light",     "cf222e", "1a7f37", "8250df", "ffffff", "f6f8fa", "d0d7de", "24292f", "57606a", "0969da"],
    ["Everforest",       "e67e80", "a7c080", "7fbbb3", "2d353b", "3d484d", "596a6d", "d3c6aa", "9da9a0", "a7c080"],
    ["Kanagawa",         "c34043", "76946a", "7e9cd8", "1f1f28", "2a2a37", "54546d", "dcd7ba", "c8c093", "957fb8"],
    ["Monokai Pro",      "ff6188", "a9dc76", "78dce8", "2d2a2e", "403e41", "5b595c", "fcfcfa", "939293", "ffd866"],
    ["Night Owl",        "ef5350", "addb67", "82aaff", "011627", "0d2231", "1d3b53", "d6deeb", "7fa3b3", "c792ea"],
    ["Ayu Mirage",       "f28779", "bae67e", "5ccfe6", "1f2430", "2a3546", "707a8c", "cbccc6", "a0a6ae", "ffcc66"],
    ["Synthwave",        "ff7edb", "72f1b8", "36f9f6", "262335", "372d4b", "495495", "e0e3e6", "b893ce", "fede5d"],
];

pub fn builtin_themes() -> Vec<Theme> {
    let c = |s: &str| parse_hex(s).expect("built-in colour");
    BUILTIN
        .iter()
        .map(|t| Theme {
            name: t[0].to_owned(),
            focus: c(t[1]),
            short_break: c(t[2]),
            long_break: c(t[3]),
            background: c(t[4]),
            surface: c(t[5]),
            muted: c(t[6]),
            text: c(t[7]),
            text_dim: c(t[8]),
            accent: c(t[9]),
        })
        .collect()
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
    fn hex_round_trip() {
        let t = &builtin_themes()[0];
        let json = serde_json::to_string(t).unwrap();
        assert_eq!(&serde_json::from_str::<Theme>(&json).unwrap(), t);
    }
}
