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
    pub const NORD0: Rgb = Rgb::from_hex(0x2E3440);
    pub const NORD1: Rgb = Rgb::from_hex(0x3B4252);
    pub const NORD2: Rgb = Rgb::from_hex(0x434C5E);
    pub const NORD3: Rgb = Rgb::from_hex(0x4C566A);
    // Snow Storm — foregrounds.
    pub const NORD4: Rgb = Rgb::from_hex(0xD8DEE9);
    pub const NORD5: Rgb = Rgb::from_hex(0xE5E9F0);
    pub const NORD6: Rgb = Rgb::from_hex(0xECEFF4);
    // Frost — classes, types, primary accent.
    pub const NORD7: Rgb = Rgb::from_hex(0x8FBCBB);
    pub const NORD8: Rgb = Rgb::from_hex(0x88C0D0);
    pub const NORD9: Rgb = Rgb::from_hex(0x81A1C1);
    pub const NORD10: Rgb = Rgb::from_hex(0x5E81AC);
    // Aurora — diagnostics, literals.
    pub const NORD11: Rgb = Rgb::from_hex(0xBF616A); // red   — errors
    pub const NORD12: Rgb = Rgb::from_hex(0xD08770); // orange — warnings
    pub const NORD13: Rgb = Rgb::from_hex(0xEBCB8B); // yellow — hints
    pub const NORD14: Rgb = Rgb::from_hex(0xA3BE8C); // green  — added / info
    pub const NORD15: Rgb = Rgb::from_hex(0xB48EAD); // purple — strings / symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgb::from_hex(0x5E81AC);
        assert_eq!(c.to_hex(), "#5E81AC");
    }

    #[test]
    fn ansi_fg_shape() {
        let c = Nord::NORD11;
        let s = c.fg_ansi();
        assert!(s.starts_with("\x1b[38;2;"));
    }
}
