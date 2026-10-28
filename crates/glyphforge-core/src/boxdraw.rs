//! Box-drawing glyph families and their topology: which arms each glyph
//! has, so lines can be merged and joined automatically.

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
    /// Every family name, in the order the inspector cycles through them.
    pub const NAMES: [&'static str; 7] = [
        "rounded", "single", "double", "heavy", "ascii", "minimal", "none",
    ];

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

// --- topology: auto-joining lines -------------------------------------------

/// Line weight of one arm of a box-drawing glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum Weight {
    #[default]
    None,
    Light,
    Heavy,
    Double,
}

/// A side of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Up,
    Right,
    Down,
    Left,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::Up, Self::Right, Self::Down, Self::Left];

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Right => Self::Left,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
        }
    }
}

/// The four arms of a box-drawing glyph, in the order up, right, down,
/// left. `─` is `[None, Light, None, Light]`, `╬` is all `Double`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Arms(pub [Weight; 4]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlyphKind {
    Solid,
    Dashed,
    Arc,
}

use GlyphKind::{Arc, Dashed, Solid};
use Weight::{Double as D, Heavy as H, Light as L, None as N};

/// Every box-drawing glyph of U+2500..U+257F except the diagonals, with its
/// arms (derived from the Unicode character names).
const TOPOLOGY: &[(&str, [Weight; 4], GlyphKind)] = &[
    ("─", [N, L, N, L], Solid),
    ("━", [N, H, N, H], Solid),
    ("│", [L, N, L, N], Solid),
    ("┃", [H, N, H, N], Solid),
    ("┄", [N, L, N, L], Dashed),
    ("┅", [N, H, N, H], Dashed),
    ("┆", [L, N, L, N], Dashed),
    ("┇", [H, N, H, N], Dashed),
    ("┈", [N, L, N, L], Dashed),
    ("┉", [N, H, N, H], Dashed),
    ("┊", [L, N, L, N], Dashed),
    ("┋", [H, N, H, N], Dashed),
    ("┌", [N, L, L, N], Solid),
    ("┍", [N, H, L, N], Solid),
    ("┎", [N, L, H, N], Solid),
    ("┏", [N, H, H, N], Solid),
    ("┐", [N, N, L, L], Solid),
    ("┑", [N, N, L, H], Solid),
    ("┒", [N, N, H, L], Solid),
    ("┓", [N, N, H, H], Solid),
    ("└", [L, L, N, N], Solid),
    ("┕", [L, H, N, N], Solid),
    ("┖", [H, L, N, N], Solid),
    ("┗", [H, H, N, N], Solid),
    ("┘", [L, N, N, L], Solid),
    ("┙", [L, N, N, H], Solid),
    ("┚", [H, N, N, L], Solid),
    ("┛", [H, N, N, H], Solid),
    ("├", [L, L, L, N], Solid),
    ("┝", [L, H, L, N], Solid),
    ("┞", [H, L, L, N], Solid),
    ("┟", [L, L, H, N], Solid),
    ("┠", [H, L, H, N], Solid),
    ("┡", [H, H, L, N], Solid),
    ("┢", [L, H, H, N], Solid),
    ("┣", [H, H, H, N], Solid),
    ("┤", [L, N, L, L], Solid),
    ("┥", [L, N, L, H], Solid),
    ("┦", [H, N, L, L], Solid),
    ("┧", [L, N, H, L], Solid),
    ("┨", [H, N, H, L], Solid),
    ("┩", [H, N, L, H], Solid),
    ("┪", [L, N, H, H], Solid),
    ("┫", [H, N, H, H], Solid),
    ("┬", [N, L, L, L], Solid),
    ("┭", [N, L, L, H], Solid),
    ("┮", [N, H, L, L], Solid),
    ("┯", [N, H, L, H], Solid),
    ("┰", [N, L, H, L], Solid),
    ("┱", [N, L, H, H], Solid),
    ("┲", [N, H, H, L], Solid),
    ("┳", [N, H, H, H], Solid),
    ("┴", [L, L, N, L], Solid),
    ("┵", [L, L, N, H], Solid),
    ("┶", [L, H, N, L], Solid),
    ("┷", [L, H, N, H], Solid),
    ("┸", [H, L, N, L], Solid),
    ("┹", [H, L, N, H], Solid),
    ("┺", [H, H, N, L], Solid),
    ("┻", [H, H, N, H], Solid),
    ("┼", [L, L, L, L], Solid),
    ("┽", [L, L, L, H], Solid),
    ("┾", [L, H, L, L], Solid),
    ("┿", [L, H, L, H], Solid),
    ("╀", [H, L, L, L], Solid),
    ("╁", [L, L, H, L], Solid),
    ("╂", [H, L, H, L], Solid),
    ("╃", [H, L, L, H], Solid),
    ("╄", [H, H, L, L], Solid),
    ("╅", [L, L, H, H], Solid),
    ("╆", [L, H, H, L], Solid),
    ("╇", [H, H, L, H], Solid),
    ("╈", [L, H, H, H], Solid),
    ("╉", [H, L, H, H], Solid),
    ("╊", [H, H, H, L], Solid),
    ("╋", [H, H, H, H], Solid),
    ("╌", [N, L, N, L], Dashed),
    ("╍", [N, H, N, H], Dashed),
    ("╎", [L, N, L, N], Dashed),
    ("╏", [H, N, H, N], Dashed),
    ("═", [N, D, N, D], Solid),
    ("║", [D, N, D, N], Solid),
    ("╒", [N, D, L, N], Solid),
    ("╓", [N, L, D, N], Solid),
    ("╔", [N, D, D, N], Solid),
    ("╕", [N, N, L, D], Solid),
    ("╖", [N, N, D, L], Solid),
    ("╗", [N, N, D, D], Solid),
    ("╘", [L, D, N, N], Solid),
    ("╙", [D, L, N, N], Solid),
    ("╚", [D, D, N, N], Solid),
    ("╛", [L, N, N, D], Solid),
    ("╜", [D, N, N, L], Solid),
    ("╝", [D, N, N, D], Solid),
    ("╞", [L, D, L, N], Solid),
    ("╟", [D, L, D, N], Solid),
    ("╠", [D, D, D, N], Solid),
    ("╡", [L, N, L, D], Solid),
    ("╢", [D, N, D, L], Solid),
    ("╣", [D, N, D, D], Solid),
    ("╤", [N, D, L, D], Solid),
    ("╥", [N, L, D, L], Solid),
    ("╦", [N, D, D, D], Solid),
    ("╧", [L, D, N, D], Solid),
    ("╨", [D, L, N, L], Solid),
    ("╩", [D, D, N, D], Solid),
    ("╪", [L, D, L, D], Solid),
    ("╫", [D, L, D, L], Solid),
    ("╬", [D, D, D, D], Solid),
    ("╭", [N, L, L, N], Arc),
    ("╮", [N, N, L, L], Arc),
    ("╯", [L, N, N, L], Arc),
    ("╰", [L, L, N, N], Arc),
    ("╴", [N, N, N, L], Solid),
    ("╵", [L, N, N, N], Solid),
    ("╶", [N, L, N, N], Solid),
    ("╷", [N, N, L, N], Solid),
    ("╸", [N, N, N, H], Solid),
    ("╹", [H, N, N, N], Solid),
    ("╺", [N, H, N, N], Solid),
    ("╻", [N, N, H, N], Solid),
    ("╼", [N, H, N, L], Solid),
    ("╽", [L, N, H, N], Solid),
    ("╾", [N, L, N, H], Solid),
    ("╿", [H, N, L, N], Solid),
];

impl Arms {
    pub const NONE: Self = Self([Weight::None; 4]);

    const fn index(side: Side) -> usize {
        match side {
            Side::Up => 0,
            Side::Right => 1,
            Side::Down => 2,
            Side::Left => 3,
        }
    }

    pub const fn get(self, side: Side) -> Weight {
        self.0[Self::index(side)]
    }

    #[must_use]
    pub const fn with(mut self, side: Side, weight: Weight) -> Self {
        self.0[Self::index(side)] = weight;
        self
    }

    pub fn count(self) -> usize {
        self.0.iter().filter(|w| **w != Weight::None).count()
    }

    pub fn is_empty(self) -> bool {
        self.count() == 0
    }

    /// The arms of a box-drawing glyph (solid, dashed or rounded).
    pub fn of(glyph: &str) -> Option<Self> {
        TOPOLOGY
            .iter()
            .find(|(g, _, _)| *g == glyph)
            .map(|(_, arms, _)| Self(*arms))
    }

    /// `other`'s arms added to these; where both have an arm, `other` wins.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        let mut out = self;
        for side in Side::ALL {
            if other.get(side) != Weight::None {
                out = out.with(side, other.get(side));
            }
        }
        out
    }

    /// The glyph that draws these arms. Unicode lacks many mixtures (double
    /// with heavy, double half-lines), so the nearest drawable glyph is
    /// used: a lone arm is extended straight through the cell, heavy gives
    /// way to light next to double, then each axis is made uniform, then
    /// double and heavy become light. With `rounded`, light corners use
    /// arcs. `None` for a cell without arms.
    pub fn glyph(self, rounded: bool) -> Option<&'static str> {
        if self.is_empty() {
            return None;
        }
        let mut arms = self;
        if arms.count() == 1 {
            let side = Side::ALL
                .into_iter()
                .find(|s| arms.get(*s) != Weight::None)
                .unwrap_or(Side::Right);
            arms = arms.with(side.opposite(), arms.get(side));
        }
        let lookup = |a: Self, kind: GlyphKind| {
            TOPOLOGY
                .iter()
                .find(|(_, w, k)| *k == kind && *w == a.0)
                .map(|(g, _, _)| *g)
        };
        if rounded {
            if let Some(g) = lookup(arms, Arc) {
                return Some(g);
            }
        }
        let map = |a: Self, f: &dyn Fn(Weight) -> Weight| Self(a.0.map(f));
        let no_heavy = if arms.0.contains(&Weight::Double) {
            map(arms, &|w| {
                if w == Weight::Heavy { Weight::Light } else { w }
            })
        } else {
            arms
        };
        let uniform = {
            let v = no_heavy.get(Side::Up).max(no_heavy.get(Side::Down));
            let h = no_heavy.get(Side::Left).max(no_heavy.get(Side::Right));
            let keep = |w: Weight, axis: Weight| if w == Weight::None { w } else { axis };
            Self([
                keep(no_heavy.0[0], v),
                keep(no_heavy.0[1], h),
                keep(no_heavy.0[2], v),
                keep(no_heavy.0[3], h),
            ])
        };
        let light = map(arms, &|w| if w == Weight::None { w } else { Weight::Light });
        [arms, no_heavy, uniform, light]
            .into_iter()
            .find_map(|a| lookup(a, Solid))
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

    #[test]
    fn solid_glyphs_round_trip_through_arms() {
        for (g, _, kind) in TOPOLOGY {
            let arms = Arms::of(g).unwrap();
            match kind {
                GlyphKind::Solid if arms.count() == 1 => {
                    assert_eq!(
                        arms.glyph(false).map(|x| Arms::of(x).unwrap().count()),
                        Some(2)
                    );
                }
                GlyphKind::Solid => assert_eq!(arms.glyph(false), Some(*g), "{g}"),
                GlyphKind::Arc => assert_eq!(arms.glyph(true), Some(*g), "{g}"),
                GlyphKind::Dashed => assert!(arms.glyph(false).is_some()),
            }
        }
        assert_eq!(TOPOLOGY.len(), 125);
    }

    #[test]
    fn merging_builds_junctions() {
        let h = Arms::of("─").unwrap();
        let v = Arms::of("│").unwrap();
        assert_eq!(h.merge(v).glyph(false), Some("┼"));
        assert_eq!(Arms::of("═").unwrap().merge(v).glyph(false), Some("╪"));
        assert_eq!(
            Arms::of("┌")
                .unwrap()
                .merge(Arms::of("┐").unwrap())
                .glyph(false),
            Some("┬")
        );
        assert_eq!(Arms::of("╭").unwrap().glyph(true), Some("╭"));
        assert_eq!(Arms::of("╭").unwrap().glyph(false), Some("┌"));
        assert_eq!(
            Arms::of("┼").unwrap().glyph(true),
            Some("┼"),
            "only corners round"
        );
    }

    #[test]
    fn missing_mixtures_fall_back_to_drawable_glyphs() {
        let lone = Arms::NONE.with(Side::Down, Weight::Double);
        assert_eq!(
            lone.glyph(false),
            Some("║"),
            "a lone arm runs through the cell"
        );
        let double_heavy = Arms::of("═").unwrap().with(Side::Up, Weight::Heavy);
        assert_eq!(double_heavy.glyph(false), Some("╧"));
        let skewed = Arms::NONE
            .with(Side::Up, Weight::Double)
            .with(Side::Down, Weight::Light)
            .with(Side::Right, Weight::Light);
        assert_eq!(skewed.glyph(false), Some("╟"));
        assert_eq!(Arms::NONE.glyph(false), None);
        assert_eq!(Arms::of("x"), None);
    }
}
