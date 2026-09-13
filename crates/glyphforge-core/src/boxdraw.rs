//! Box-drawing glyph families. Topology-aware junction merging follows in
//! a later milestone; this module owns the glyph tables it will use.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderFamily {
    Ascii,
    Single,
    #[default]
    Rounded,
    Double,
    Heavy,
    /// Light horizontal rules only: top and bottom lines, no verticals.
    Minimal,
    None,
}

impl BorderFamily {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ascii" => Some(Self::Ascii),
            "single" => Some(Self::Single),
            "rounded" => Some(Self::Rounded),
            "double" => Some(Self::Double),
            "heavy" => Some(Self::Heavy),
            "minimal" => Some(Self::Minimal),
            "none" => Some(Self::None),
            _ => Option::None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ascii => "ascii",
            Self::Single => "single",
            Self::Rounded => "rounded",
            Self::Double => "double",
            Self::Heavy => "heavy",
            Self::Minimal => "minimal",
            Self::None => "none",
        }
    }

    /// Whether this family draws a border (and therefore consumes cells).
    pub const fn is_visible(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn glyphs(self) -> BorderGlyphs {
        match self {
            Self::Ascii => BorderGlyphs::new("+", "+", "+", "+", "-", "|"),
            Self::Single => BorderGlyphs::new("┌", "┐", "└", "┘", "─", "│"),
            Self::Rounded => BorderGlyphs::new("╭", "╮", "╰", "╯", "─", "│"),
            Self::Double => BorderGlyphs::new("╔", "╗", "╚", "╝", "═", "║"),
            Self::Heavy => BorderGlyphs::new("┏", "┓", "┗", "┛", "━", "┃"),
            Self::Minimal => BorderGlyphs::new("─", "─", "─", "─", "─", " "),
            Self::None => BorderGlyphs::new(" ", " ", " ", " ", " ", " "),
        }
    }
}

/// The six glyphs of a rectangular border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderGlyphs {
    pub top_left: &'static str,
    pub top_right: &'static str,
    pub bottom_left: &'static str,
    pub bottom_right: &'static str,
    pub horizontal: &'static str,
    pub vertical: &'static str,
}

impl BorderGlyphs {
    const fn new(
        top_left: &'static str,
        top_right: &'static str,
        bottom_left: &'static str,
        bottom_right: &'static str,
        horizontal: &'static str,
        vertical: &'static str,
    ) -> Self {
        Self {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_has_single_width_glyphs() {
        use crate::document::Grapheme;
        for f in [
            BorderFamily::Ascii,
            BorderFamily::Single,
            BorderFamily::Rounded,
            BorderFamily::Double,
            BorderFamily::Heavy,
            BorderFamily::Minimal,
            BorderFamily::None,
        ] {
            let g = f.glyphs();
            for s in [
                g.top_left,
                g.top_right,
                g.bottom_left,
                g.bottom_right,
                g.horizontal,
                g.vertical,
            ] {
                assert_eq!(Grapheme::new(s).unwrap().width(), 1, "{f:?} {s}");
            }
            assert_eq!(BorderFamily::parse(f.name()), Some(f));
        }
    }
}
