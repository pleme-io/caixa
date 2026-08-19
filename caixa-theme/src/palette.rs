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

    /// ANSI 24-bit SGR foreground sequence — `\x1b[38;2;R;G;Bm`.
    #[must_use]
    pub fn fg_ansi(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// ANSI 24-bit SGR background sequence — `\x1b[48;2;R;G;Bm`.
    #[must_use]
    pub fn bg_ansi(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// Reset SGR — ends a color.
pub const ANSI_RESET: &str = "\x1b[0m";

/// Nord palette — 16 entries, named Nord0 … Nord15 per the canonical spec.
#[allow(non_snake_case)]
pub struct Nord;

impl Nord {
    // Polar Night — backgrounds, UI chrome.
    pub const NORD0: Rgb = Rgb::new(0x2E, 0x34, 0x40);
    pub const NORD1: Rgb = Rgb::new(0x3B, 0x42, 0x52);
    pub const NORD2: Rgb = Rgb::new(0x43, 0x4C, 0x5E);
    pub const NORD3: Rgb = Rgb::new(0x4C, 0x56, 0x6A);
    // Snow Storm — foregrounds.
    pub const NORD4: Rgb = Rgb::new(0xD8, 0xDE, 0xE9);
    pub const NORD5: Rgb = Rgb::new(0xE5, 0xE9, 0xF0);
    pub const NORD6: Rgb = Rgb::new(0xEC, 0xEF, 0xF4);
    // Frost — classes, types, primary accent.
    pub const NORD7: Rgb = Rgb::new(0x8F, 0xBC, 0xBB);
    pub const NORD8: Rgb = Rgb::new(0x88, 0xC0, 0xD0);
    pub const NORD9: Rgb = Rgb::new(0x81, 0xA1, 0xC1);
    pub const NORD10: Rgb = Rgb::new(0x5E, 0x81, 0xAC);
    // Aurora — diagnostics, literals.
    pub const NORD11: Rgb = Rgb::new(0xBF, 0x61, 0x6A); // red   — errors
    pub const NORD12: Rgb = Rgb::new(0xD0, 0x87, 0x70); // orange — warnings
    pub const NORD13: Rgb = Rgb::new(0xEB, 0xCB, 0x8B); // yellow — hints
    pub const NORD14: Rgb = Rgb::new(0xA3, 0xBE, 0x8C); // green  — added / info
    pub const NORD15: Rgb = Rgb::new(0xB4, 0x8E, 0xAD); // purple — strings / symbols
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
    fn palette_table_matches_rgb_new_byte_slots() {
        // Fail-before-pass-after pin that the Rgb::new(r, g, b) table
        // form the palette entries route through preserves the canonical
        // Nord byte triples the pre-lift Rgb::from_hex(0xRRGGBB) form
        // encoded — one spot-check per Polar Night / Snow Storm / Frost
        // / Aurora quadrant to catch any accidental byte transposition
        // (r↔g, g↔b) on a future edit. The Rgb::new(r, g, b) surface
        // aligns the source triple with the runtime field triple, which
        // is why the pre-lift Rgb::from_hex(0xRRGGBB) hex-nibble split
        // (once the sole use of the u32→(u8, u8, u8) shift math at
        // compile time inside the Nord table) is no longer needed here
        // — Rgb::from_hex stays a public API for consumers with a hex
        // literal in hand, but no compile-time constant in this crate
        // still walks it.
        assert_eq!(Nord::NORD0, Rgb::new(0x2E, 0x34, 0x40));
        assert_eq!(Nord::NORD6, Rgb::new(0xEC, 0xEF, 0xF4));
        assert_eq!(Nord::NORD10, Rgb::new(0x5E, 0x81, 0xAC));
        assert_eq!(Nord::NORD11, Rgb::new(0xBF, 0x61, 0x6A));
    }

    #[test]
    fn ansi_fg_shape() {
        let c = Nord::NORD11;
        let s = c.fg_ansi();
        assert!(s.starts_with("\x1b[38;2;"));
    }
}
