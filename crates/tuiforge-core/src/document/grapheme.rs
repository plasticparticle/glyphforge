use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Errors produced when a string cannot be used as the content of one cell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphemeError {
    #[error("empty string is not a grapheme")]
    Empty,
    #[error("string contains more than one grapheme cluster: {0:?}")]
    MultipleClusters(String),
    #[error("control characters cannot be placed in a cell: {0:?}")]
    Control(String),
    #[error("grapheme has zero display width and cannot occupy a cell: {0:?}")]
    ZeroWidth(String),
    #[error("grapheme is wider than two columns: {0:?}")]
    TooWide(String),
}

/// Exactly one extended grapheme cluster with a display width of 1 or 2
/// terminal columns.
///
/// Width follows `unicode-width`'s non-CJK interpretation: East Asian
/// Ambiguous characters (which include the box-drawing block) are one
/// column wide.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Grapheme {
    text: Box<str>,
    width: u8,
}

impl Grapheme {
    /// Validates `text` as a single, displayable grapheme cluster.
    pub fn new(text: &str) -> Result<Self, GraphemeError> {
        if text.is_empty() {
            return Err(GraphemeError::Empty);
        }
        let mut clusters = text.graphemes(true);
        let first = clusters.next().ok_or(GraphemeError::Empty)?;
        if clusters.next().is_some() {
            return Err(GraphemeError::MultipleClusters(text.to_owned()));
        }
        if first.chars().any(char::is_control) {
            return Err(GraphemeError::Control(text.to_owned()));
        }
        match first.width() {
            0 => Err(GraphemeError::ZeroWidth(text.to_owned())),
            w @ (1 | 2) => Ok(Self {
                text: first.into(),
                width: w as u8,
            }),
            _ => Err(GraphemeError::TooWide(text.to_owned())),
        }
    }

    /// Builds a grapheme from a single `char`. Fails for control characters
    /// and zero-width characters such as lone combining marks.
    pub fn from_char(c: char) -> Result<Self, GraphemeError> {
        let mut buf = [0u8; 4];
        Self::new(c.encode_utf8(&mut buf))
    }

    /// A plain ASCII space.
    pub fn space() -> Self {
        Self {
            text: " ".into(),
            width: 1,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Display width in terminal columns: 1 or 2.
    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn is_wide(&self) -> bool {
        self.width == 2
    }

    /// Splits arbitrary text into graphemes, skipping anything that cannot
    /// occupy a cell (control characters, zero-width clusters).
    pub fn split_text(text: &str) -> Vec<Self> {
        text.graphemes(true)
            .filter_map(|g| Self::new(g).ok())
            .collect()
    }
}

impl fmt::Debug for Grapheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Grapheme({:?}, w={})", &*self.text, self.width)
    }
}

impl fmt::Display for Grapheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for Grapheme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Grapheme {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::new(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_width_one() {
        let g = Grapheme::new("a").unwrap();
        assert_eq!(g.width(), 1);
        assert!(!g.is_wide());
    }

    #[test]
    fn box_drawing_is_width_one() {
        for s in ["─", "│", "┼", "╔", "█", "▀", "░"] {
            assert_eq!(Grapheme::new(s).unwrap().width(), 1, "{s}");
        }
    }

    #[test]
    fn cjk_is_width_two() {
        assert_eq!(Grapheme::new("漢").unwrap().width(), 2);
        assert_eq!(Grapheme::new("ア").unwrap().width(), 2);
    }

    #[test]
    fn emoji_is_width_two() {
        assert_eq!(Grapheme::new("🦀").unwrap().width(), 2);
    }

    #[test]
    fn combining_sequence_is_one_cluster_of_width_one() {
        // e + COMBINING ACUTE ACCENT
        let g = Grapheme::new("e\u{301}").unwrap();
        assert_eq!(g.width(), 1);
        assert_eq!(g.as_str(), "e\u{301}");
    }

    #[test]
    fn lone_combining_mark_is_rejected() {
        assert_eq!(
            Grapheme::new("\u{301}"),
            Err(GraphemeError::ZeroWidth("\u{301}".into()))
        );
    }

    #[test]
    fn zero_width_space_is_rejected() {
        assert!(matches!(
            Grapheme::new("\u{200B}"),
            Err(GraphemeError::ZeroWidth(_))
        ));
    }

    #[test]
    fn multiple_clusters_are_rejected() {
        assert!(matches!(
            Grapheme::new("ab"),
            Err(GraphemeError::MultipleClusters(_))
        ));
    }

    #[test]
    fn control_characters_are_rejected() {
        assert!(matches!(
            Grapheme::new("\n"),
            Err(GraphemeError::Control(_))
        ));
        assert!(matches!(
            Grapheme::new("\t"),
            Err(GraphemeError::Control(_))
        ));
        assert!(matches!(
            Grapheme::from_char('\u{7}'),
            Err(GraphemeError::Control(_))
        ));
    }

    #[test]
    fn empty_is_rejected() {
        assert_eq!(Grapheme::new(""), Err(GraphemeError::Empty));
    }

    #[test]
    fn braille_is_width_one() {
        assert_eq!(Grapheme::new("⣿").unwrap().width(), 1);
    }

    #[test]
    fn split_text_drops_undisplayable_parts() {
        let parts = Grapheme::split_text("a\tb漢");
        let strs: Vec<_> = parts.iter().map(Grapheme::as_str).collect();
        assert_eq!(strs, vec!["a", "b", "漢"]);
    }

    #[test]
    fn serde_round_trip_as_string() {
        let g = Grapheme::new("┼").unwrap();
        let json = serde_json::to_string(&g).unwrap();
        assert_eq!(json, "\"┼\"");
        let back: Grapheme = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g);
        assert!(serde_json::from_str::<Grapheme>("\"ab\"").is_err());
    }
}
