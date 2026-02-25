use ratatui::style::Color;
use std::sync::LazyLock;

pub(super) struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub error: Color,
    pub text: Color,
    pub muted: Color,
    pub bg: Color,
    pub panel: Color,
}

pub(super) fn parse_hex(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return Color::Rgb(0, 0, 0);
    }

    let r = hex
        .get(0..2)
        .and_then(|s| u8::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    let g = hex
        .get(2..4)
        .and_then(|s| u8::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    let b = hex
        .get(4..6)
        .and_then(|s| u8::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    Color::Rgb(r, g, b)
}

pub(super) static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let json: serde_json::Value =
        serde_json::from_str(super::OPENCODE_THEME_JSON).unwrap_or_default();
    let defs = &json["defs"];
    let hex =
        |key: &str, fallback: &str| -> Color { parse_hex(defs[key].as_str().unwrap_or(fallback)) };
    Theme {
        primary: hex("primary", "#a277ff"),
        secondary: hex("secondary", "#61ffca"),
        accent: hex("accent", "#ffca85"),
        error: hex("error", "#ff6767"),
        text: hex("text", "#edecee"),
        muted: hex("muted", "#6d6d6d"),
        bg: hex("bg", "#15141b"),
        panel: hex("element", "#1f1d28"),
    }
});

pub(super) fn purple() -> Color {
    THEME.primary
}
pub(super) fn green() -> Color {
    THEME.secondary
}
pub(super) fn yellow() -> Color {
    THEME.accent
}
pub(super) fn red() -> Color {
    THEME.error
}
pub(super) fn text_fg() -> Color {
    THEME.text
}
pub(super) fn muted() -> Color {
    THEME.muted
}
pub(super) fn bg() -> Color {
    THEME.bg
}
pub(super) fn panel() -> Color {
    THEME.panel
}
