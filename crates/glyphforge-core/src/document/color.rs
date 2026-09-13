use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A terminal colour.
///
/// `Default` is the terminal's own default colour and is distinct from any
/// concrete colour: it is preserved through export as `SGR 39`/`49`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    #[default]
    Default,
    /// ANSI colour index. `0..=15` is the classic 16-colour set, `16..=255`
    /// the xterm 256-colour extension.
    Indexed(u8),
    /// 24-bit "TrueColor".
    Rgb(u8, u8, u8),
}

/// Errors from parsing the textual colour form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorParseError {
    #[error("invalid colour {0:?}; expected \"default\", \"ansi:N\" or \"#rrggbb\"")]
    Invalid(String),
}

impl Color {
    pub const BLACK: Self = Self::Indexed(0);
    pub const RED: Self = Self::Indexed(1);
    pub const GREEN: Self = Self::Indexed(2);
    pub const YELLOW: Self = Self::Indexed(3);
    pub const BLUE: Self = Self::Indexed(4);
    pub const MAGENTA: Self = Self::Indexed(5);
    pub const CYAN: Self = Self::Indexed(6);
    pub const WHITE: Self = Self::Indexed(7);
    pub const BRIGHT_BLACK: Self = Self::Indexed(8);
    pub const BRIGHT_RED: Self = Self::Indexed(9);
    pub const BRIGHT_GREEN: Self = Self::Indexed(10);
    pub const BRIGHT_YELLOW: Self = Self::Indexed(11);
    pub const BRIGHT_BLUE: Self = Self::Indexed(12);
    pub const BRIGHT_MAGENTA: Self = Self::Indexed(13);
    pub const BRIGHT_CYAN: Self = Self::Indexed(14);
    pub const BRIGHT_WHITE: Self = Self::Indexed(15);

    /// Parses `#rrggbb` (case-insensitive, leading `#` optional).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self::Rgb(r, g, b))
    }

    /// Maps the colour into the 256-colour space. `Default` and indexed
    /// colours are returned unchanged.
    #[must_use]
    pub fn quantize_256(self) -> Self {
        match self {
            Self::Rgb(r, g, b) => Self::Indexed(rgb_to_ansi256(r, g, b)),
            other => other,
        }
    }

    /// Linear mix of two RGB colours; `t = 0` yields `self`, `t = 1` yields
    /// `other`. Non-RGB inputs are returned as `self`.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        match (self, other) {
            (Self::Rgb(r1, g1, b1), Self::Rgb(r2, g2, b2)) => {
                let t = t.clamp(0.0, 1.0);
                let lerp =
                    |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
                Self::Rgb(lerp(r1, r2), lerp(g1, g2), lerp(b1, b2))
            }
            _ => self,
        }
    }
}

/// Nearest xterm-256 index for an RGB triple, considering both the 6x6x6
/// colour cube and the 24-step grey ramp.
fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest_step = |v: u8| -> (u8, u8) {
        let mut best = 0u8;
        let mut best_dist = u16::MAX;
        for (i, &s) in STEPS.iter().enumerate() {
            let d = v.abs_diff(s);
            if u16::from(d) < best_dist {
                best_dist = u16::from(d);
                best = i as u8;
            }
        }
        (best, STEPS[best as usize])
    };
    let (ri, rv) = nearest_step(r);
    let (gi, gv) = nearest_step(g);
    let (bi, bv) = nearest_step(b);
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let cube_dist = dist2(r, g, b, rv, gv, bv);

    let grey_avg = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let grey_level = ((grey_avg.saturating_sub(8)) / 10).min(23) as u8;
    let grey_value = 8 + grey_level * 10;
    let grey_dist = dist2(r, g, b, grey_value, grey_value, grey_value);

    if grey_dist < cube_dist {
        232 + grey_level
    } else {
        cube_index
    }
}

fn dist2(r: u8, g: u8, b: u8, r2: u8, g2: u8, b2: u8) -> u32 {
    let d = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
    d(r, r2) + d(g, g2) + d(b, b2)
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Indexed(i) => write!(f, "ansi:{i}"),
            Self::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("default") {
            return Ok(Self::Default);
        }
        if let Some(idx) = s.strip_prefix("ansi:") {
            return idx
                .parse::<u8>()
                .map(Self::Indexed)
                .map_err(|_| ColorParseError::Invalid(s.into()));
        }
        Self::from_hex(s).ok_or_else(|| ColorParseError::Invalid(s.into()))
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_forms() {
        assert_eq!("default".parse::<Color>().unwrap(), Color::Default);
        assert_eq!("ansi:12".parse::<Color>().unwrap(), Color::Indexed(12));
        assert_eq!(
            "#7aa2f7".parse::<Color>().unwrap(),
            Color::Rgb(0x7a, 0xa2, 0xf7)
        );
        assert_eq!(
            "7AA2F7".parse::<Color>().unwrap(),
            Color::Rgb(0x7a, 0xa2, 0xf7)
        );
        assert!("ansi:256".parse::<Color>().is_err());
        assert!("#12345".parse::<Color>().is_err());
        assert!("blue".parse::<Color>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for c in [Color::Default, Color::Indexed(200), Color::Rgb(1, 2, 3)] {
            assert_eq!(c.to_string().parse::<Color>().unwrap(), c);
        }
    }

    #[test]
    fn serde_uses_string_form() {
        let json = serde_json::to_string(&Color::Rgb(255, 0, 16)).unwrap();
        assert_eq!(json, "\"#ff0010\"");
        let back: Color = serde_json::from_str("\"ansi:3\"").unwrap();
        assert_eq!(back, Color::Indexed(3));
    }

    #[test]
    fn quantize_hits_cube_corners_and_greys() {
        assert_eq!(Color::Rgb(0, 0, 0).quantize_256(), Color::Indexed(16));
        assert_eq!(
            Color::Rgb(255, 255, 255).quantize_256(),
            Color::Indexed(231)
        );
        assert_eq!(Color::Rgb(255, 0, 0).quantize_256(), Color::Indexed(196));
        assert_eq!(
            Color::Rgb(128, 128, 128).quantize_256(),
            Color::Indexed(244)
        );
        assert_eq!(Color::Indexed(4).quantize_256(), Color::Indexed(4));
        assert_eq!(Color::Default.quantize_256(), Color::Default);
    }

    #[test]
    fn mix_interpolates() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 50);
        assert_eq!(a.mix(b, 0.5), Color::Rgb(50, 100, 25));
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
        assert_eq!(Color::Default.mix(b, 0.5), Color::Default);
    }
}
