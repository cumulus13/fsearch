// File: src\colors.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use colored::{Color, ColoredString, Colorize};

/// Parse a CSS-style hex colour string (`"#RRGGBB"` or `"RRGGBB"`) into
/// a `colored::Color::TrueColor`.  Falls back to `Color::White` on any
/// parse error so callers never need to handle a `Result`.
pub fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::TrueColor { r, g, b };
        }
    }
    Color::White
}

/// Apply a hex colour to a string slice, returning a `ColoredString`.
pub fn hex_color<S: AsRef<str>>(text: S, hex: &str) -> ColoredString {
    text.as_ref().color(hex_to_color(hex))
}

/// Apply **bold** + hex colour.
pub fn bold_hex<S: AsRef<str>>(text: S, hex: &str) -> ColoredString {
    text.as_ref().color(hex_to_color(hex)).bold()
}

/// Apply underline + hex colour (accent / emphasis variant).
#[allow(dead_code)]
pub fn bright_hex<S: AsRef<str>>(text: S, hex: &str) -> ColoredString {
    text.as_ref().color(hex_to_color(hex)).underline()
}
