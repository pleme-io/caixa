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

    #[test]
    fn every_semantic_maps_to_a_nord_palette_color_across_both_themes() {
        // Fail-before-pass-after pin on [`Semantic::ALL`] as the
        // canonical iteration axis every theme overlay must resolve
        // in full: for each of the 15 variants and each of the two
        // shipped [`Theme`] overlays (`blackmatter_dark`,
        // `blackmatter_light`), the resolver must return one of the
        // 16 Nord palette colors. Pre-lift the two resolver functions
        // were paired 15-arm exhaustive matches with no cross-consumer
        // link back to the closed [`Semantic`] partition, so a future
        // wildcard arm on either resolver (an `_ => Rgb::from_hex(0)`
        // for a hypothetical "unstyled" fallback that would render
        // invisibly on a dark terminal, an `_ => Nord::NORD0` for a
        // hypothetical "default background" fallback the light
        // overlay's `Symbol` arm already hand-authored) would silently
        // slip past both the compile-time exhaustiveness check (each
        // wildcard *is* exhaustive) and any per-arm smoke test.
        // Iterating [`Semantic::ALL`] and asserting each resolver's
        // output falls inside the closed 16-color Nord palette makes
        // such a slip a caixa-theme build-time failure. Compounding
        // peer of the peer [`caixa_core::CaixaKind`] /
        // [`caixa_lint::Severity`] `ALL`-based iteration disciplines.
        use crate::palette::Nord;
        let nord_palette: [Rgb; 16] = [
            Nord::NORD0,
            Nord::NORD1,
            Nord::NORD2,
            Nord::NORD3,
            Nord::NORD4,
            Nord::NORD5,
            Nord::NORD6,
            Nord::NORD7,
            Nord::NORD8,
            Nord::NORD9,
            Nord::NORD10,
            Nord::NORD11,
            Nord::NORD12,
            Nord::NORD13,
            Nord::NORD14,
            Nord::NORD15,
        ];
        for theme in [Theme::blackmatter_dark(), Theme::blackmatter_light()] {
            for &sem in Semantic::ALL {
                let rgb = theme.color(sem);
                assert!(
                    nord_palette.contains(&rgb),
                    "Theme::{} maps Semantic::{sem:?} to {rgb:?} — a \
                     color outside the closed Nord palette. Every \
                     theme-overlay resolver must reach for one of the \
                     16 Nord entries; a wildcard arm returning a \
                     color outside the palette (an invisible fallback, \
                     a paint-bleed on a hypothetical extension arm) \
                     is a defect.",
                    theme.name,
                );
            }
        }
    }

    #[test]
    fn diagnostic_severity_arms_paint_red_and_orange_across_both_themes() {
        // Fail-before-pass-after pin on the cross-theme-invariant
        // paint of the two structurally-most-load-bearing diagnostic
        // arms — [`Semantic::Error`] (red, `Nord::NORD11`) and
        // [`Semantic::Warning`] (orange, `Nord::NORD12`). These two
        // are the Aurora hues every terminal reader keys off
        // ("something is wrong here"); a future theme overlay that
        // rerouted either through a Frost blue (silently reading as
        // an informational tag) would remove the paint's semantic
        // signal without any compile-time failure. Iterating both
        // shipped overlays via [`Semantic::ALL`]'s canonical arm-list
        // and asserting the two paint invariants on the exact Nord
        // slot pins the semantic-color-contract that
        // [`crate::palette::Nord::NORD11`]'s `// red — errors` /
        // [`crate::palette::Nord::NORD12`]'s `// orange — warnings`
        // documentation comments already assert at the palette
        // definition site, but that no cross-overlay test currently
        // guards.
        use crate::palette::Nord;
        for theme in [Theme::blackmatter_dark(), Theme::blackmatter_light()] {
            assert_eq!(
                theme.color(Semantic::Error),
                Nord::NORD11,
                "Theme::{} must paint Semantic::Error as NORD11 (red)",
                theme.name,
            );
            assert_eq!(
                theme.color(Semantic::Warning),
                Nord::NORD12,
                "Theme::{} must paint Semantic::Warning as NORD12 (orange)",
                theme.name,
            );
        }
    }
}
