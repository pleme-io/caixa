//! Blackmatter overlays — the chosen Nord→Semantic mapping.
//!
//! The default overlay is `blackmatter_dark`, which matches blackmatter-nvim
//! and blackmatter-shell. Light and high-contrast overlays are provided so a
//! caller can pick at runtime.

use crate::palette::{Nord, Rgb};
use crate::style::Semantic;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    resolver: fn(Semantic) -> Rgb,
}

impl Theme {
    #[must_use]
    pub fn blackmatter_dark() -> Self {
        Self {
            name: "blackmatter-dark",
            resolver: blackmatter_dark_color,
        }
    }

    #[must_use]
    pub fn blackmatter_light() -> Self {
        Self {
            name: "blackmatter-light",
            resolver: blackmatter_light_color,
        }
    }

    #[must_use]
    pub fn color(&self, s: Semantic) -> Rgb {
        (self.resolver)(s)
    }

    #[must_use]
    pub fn ansi(&self, s: Semantic) -> String {
        self.color(s).fg_ansi()
    }

    #[must_use]
    pub fn paint(&self, s: Semantic, text: &str) -> String {
        format!("{}{}{}", self.ansi(s), text, crate::palette::ANSI_RESET)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::blackmatter_dark()
    }
}

fn blackmatter_dark_color(s: Semantic) -> Rgb {
    match s {
        Semantic::Keyword => Nord::NORD9,
        Semantic::Symbol => Nord::NORD4,
        Semantic::KeywordArg => Nord::NORD8,
        Semantic::String => Nord::NORD14,
        Semantic::Number => Nord::NORD15,
        Semantic::Literal => Nord::NORD13,
        Semantic::Comment => Nord::NORD3,
        Semantic::Accent => Nord::NORD8,
        Semantic::Muted => Nord::NORD3,
        Semantic::Error => Nord::NORD11,
        Semantic::Warning => Nord::NORD12,
        Semantic::Info => Nord::NORD8,
        Semantic::Hint => Nord::NORD13,
        Semantic::Added => Nord::NORD14,
        Semantic::Removed => Nord::NORD11,
        Semantic::Unchanged => Nord::NORD4,
    }
}

fn blackmatter_light_color(s: Semantic) -> Rgb {
    // Invert background-assuming choices for readability on light terminals.
    match s {
        Semantic::Keyword => Nord::NORD10,
        Semantic::Symbol => Nord::NORD0,
        Semantic::KeywordArg => Nord::NORD10,
        Semantic::String => Nord::NORD14,
        Semantic::Number => Nord::NORD15,
        Semantic::Literal => Nord::NORD12,
        Semantic::Comment => Nord::NORD2,
        Semantic::Accent => Nord::NORD10,
        Semantic::Muted => Nord::NORD2,
        Semantic::Error => Nord::NORD11,
        Semantic::Warning => Nord::NORD12,
        Semantic::Info => Nord::NORD10,
        Semantic::Hint => Nord::NORD13,
        Semantic::Added => Nord::NORD14,
        Semantic::Removed => Nord::NORD11,
        Semantic::Unchanged => Nord::NORD0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dark() {
        let t = Theme::default();
        assert_eq!(t.name, "blackmatter-dark");
    }

    #[test]
    fn error_maps_to_red() {
        let t = Theme::blackmatter_dark();
        assert_eq!(t.color(Semantic::Error), Nord::NORD11);
    }

    #[test]
    fn paint_wraps_with_reset() {
        let t = Theme::blackmatter_dark();
        let out = t.paint(Semantic::Error, "boom");
        assert!(out.starts_with("\x1b["));
        assert!(out.ends_with(crate::palette::ANSI_RESET));
        assert!(out.contains("boom"));
    }
}
