//! Nord — Arctic-themed palette. Reference: <https://www.nordtheme.com/>.
//!
//! Polar Night → Snow Storm → Frost → Aurora. All members are const — use
//! them directly, e.g. `Nord::NORD11` for red.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    #[must_use]
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
        }
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    #[must_use]
    pub const fn from_irodori(c: irodori::Color) -> Self {
        Self::new(c.r, c.g, c.b)
    }

    /// ANSI 24-bit SGR foreground sequence — `ESC[38;2;R;G;Bm`.
    #[must_use]
    pub fn fg_ansi(self) -> String {
        anstyle::RgbColor(self.r, self.g, self.b)
            .render_fg()
            .to_string()
    }

    /// ANSI 24-bit SGR background sequence — `ESC[48;2;R;G;Bm`.
    #[must_use]
    pub fn bg_ansi(self) -> String {
        anstyle::RgbColor(self.r, self.g, self.b)
            .render_bg()
            .to_string()
    }
}

impl From<Rgb> for kazari::Rgb {
    fn from(c: Rgb) -> Self {
        kazari::Rgb::new(c.r, c.g, c.b)
    }
}

impl From<kazari::Rgb> for Rgb {
    fn from(c: kazari::Rgb) -> Self {
        Rgb::new(c.r, c.g, c.b)
    }
}

impl From<irodori::Color> for Rgb {
    fn from(c: irodori::Color) -> Self {
        Rgb::from_irodori(c)
    }
}

/// Reset SGR — ends a color.
pub const ANSI_RESET: anstyle::Reset = anstyle::Reset;

/// Nord palette — 16 entries, named Nord0 … Nord15 per the canonical spec.
#[allow(non_snake_case)]
pub struct Nord;

impl Nord {
    // Polar Night — backgrounds, UI chrome.
    pub const NORD0: Rgb = Rgb::from_irodori(irodori::NORD.polar_night[0]);
    pub const NORD1: Rgb = Rgb::from_irodori(irodori::NORD.polar_night[1]);
    pub const NORD2: Rgb = Rgb::from_irodori(irodori::NORD.polar_night[2]);
    pub const NORD3: Rgb = Rgb::from_irodori(irodori::NORD.polar_night[3]);
    // Snow Storm — foregrounds.
    pub const NORD4: Rgb = Rgb::from_irodori(irodori::NORD.snow_storm[0]);
    pub const NORD5: Rgb = Rgb::from_irodori(irodori::NORD.snow_storm[1]);
    pub const NORD6: Rgb = Rgb::from_irodori(irodori::NORD.snow_storm[2]);
    // Frost — classes, types, primary accent.
    pub const NORD7: Rgb = Rgb::from_irodori(irodori::NORD.frost[0]);
    pub const NORD8: Rgb = Rgb::from_irodori(irodori::NORD.frost[1]);
    pub const NORD9: Rgb = Rgb::from_irodori(irodori::NORD.frost[2]);
    pub const NORD10: Rgb = Rgb::from_irodori(irodori::NORD.frost[3]);
    // Aurora — diagnostics, literals.
    pub const NORD11: Rgb = Rgb::from_irodori(irodori::NORD.aurora[0]); // red   — errors
    pub const NORD12: Rgb = Rgb::from_irodori(irodori::NORD.aurora[1]); // orange — warnings
    pub const NORD13: Rgb = Rgb::from_irodori(irodori::NORD.aurora[2]); // yellow — hints
    pub const NORD14: Rgb = Rgb::from_irodori(irodori::NORD.aurora[3]); // green  — added / info
    pub const NORD15: Rgb = Rgb::from_irodori(irodori::NORD.aurora[4]); // purple — strings / symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgb::from_hex(0x5E_81_AC);
        assert_eq!(c.to_hex(), "#5E81AC");
    }

    #[test]
    fn palette_is_irodori_nord_in_canonical_order() {
        let table = [
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
        for (ours, theirs) in table.iter().zip(irodori::NORD.all_colors()) {
            assert_eq!(*ours, Rgb::from(theirs));
        }
    }

    #[test]
    fn ansi_fg_matches_kazari_truecolor_emission() {
        let c = Nord::NORD11;
        let caps = kazari::Capability::fixed(kazari::ColorLevel::Truecolor, 80, true);
        assert_eq!(
            format!("{}x{ANSI_RESET}", c.fg_ansi()),
            kazari::paint_rgb_at(c.into(), "x", false, false, &caps),
        );
        assert_ne!(c.fg_ansi(), c.bg_ansi());
    }
}
