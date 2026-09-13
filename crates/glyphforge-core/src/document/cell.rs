use serde::{Deserialize, Serialize};

use super::{Color, Grapheme};

/// Name, getter and setter of one attribute flag.
type AttributeEntry = (&'static str, fn(&Attributes) -> bool, fn(&mut Attributes));

/// Text attributes of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attributes {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub strikethrough: bool,
}

impl Attributes {
    pub const NONE: Self = Self {
        bold: false,
        dim: false,
        italic: false,
        underline: false,
        blink: false,
        reverse: false,
        strikethrough: false,
    };

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    const NAMES: [AttributeEntry; 7] = [
        ("bold", |a| a.bold, |a| a.bold = true),
        ("dim", |a| a.dim, |a| a.dim = true),
        ("italic", |a| a.italic, |a| a.italic = true),
        ("underline", |a| a.underline, |a| a.underline = true),
        ("blink", |a| a.blink, |a| a.blink = true),
        ("reverse", |a| a.reverse, |a| a.reverse = true),
        (
            "strikethrough",
            |a| a.strikethrough,
            |a| a.strikethrough = true,
        ),
    ];

    /// The names of the set attributes, in a stable order.
    pub fn names(self) -> Vec<&'static str> {
        Self::NAMES
            .iter()
            .filter(|(_, get, _)| get(&self))
            .map(|(n, _, _)| *n)
            .collect()
    }

    /// Builds attributes from names; unknown names are returned as errors.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut out = Self::NONE;
        for name in names {
            match Self::NAMES.iter().find(|(n, _, _)| *n == name) {
                Some((_, _, set)) => set(&mut out),
                None => return Err(name.to_owned()),
            }
        }
        Ok(out)
    }
}

/// Colours and attributes of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CellStyle {
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attributes,
}

impl CellStyle {
    pub const DEFAULT: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        attrs: Attributes::NONE,
    };

    pub const fn new(fg: Color, bg: Color) -> Self {
        Self {
            fg,
            bg,
            attrs: Attributes::NONE,
        }
    }

    #[must_use]
    pub const fn with_attrs(mut self, attrs: Attributes) -> Self {
        self.attrs = attrs;
        self
    }
}

/// What a cell contains.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CellContent {
    /// Transparent: the compositor looks through to lower layers.
    #[default]
    Empty,
    /// A displayable grapheme cluster of width 1 or 2.
    Glyph(Grapheme),
    /// The right half of a double-width glyph located one cell to the left.
    /// Maintained by [`Canvas`](super::Canvas); never written directly.
    WideTail,
}

/// One terminal cell: content plus style.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cell {
    pub content: CellContent,
    pub style: CellStyle,
}

impl Cell {
    /// A transparent cell with default style.
    pub const EMPTY: Self = Self {
        content: CellContent::Empty,
        style: CellStyle::DEFAULT,
    };

    pub const fn new(content: CellContent, style: CellStyle) -> Self {
        Self { content, style }
    }

    pub const fn glyph(grapheme: Grapheme, style: CellStyle) -> Self {
        Self {
            content: CellContent::Glyph(grapheme),
            style,
        }
    }

    /// An opaque blank cell: a space with the given style.
    pub fn blank(style: CellStyle) -> Self {
        Self::glyph(Grapheme::space(), style)
    }

    pub const fn wide_tail(style: CellStyle) -> Self {
        Self {
            content: CellContent::WideTail,
            style,
        }
    }

    pub const fn is_empty(&self) -> bool {
        matches!(self.content, CellContent::Empty)
    }

    pub const fn is_wide_tail(&self) -> bool {
        matches!(self.content, CellContent::WideTail)
    }

    pub const fn is_wide_head(&self) -> bool {
        matches!(&self.content, CellContent::Glyph(g) if g.is_wide())
    }

    pub const fn as_glyph(&self) -> Option<&Grapheme> {
        match &self.content {
            CellContent::Glyph(g) => Some(g),
            _ => None,
        }
    }

    /// Display width of the content: 0 for empty and tail cells.
    pub const fn width(&self) -> u8 {
        match &self.content {
            CellContent::Glyph(g) => g.width(),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_is_default() {
        assert_eq!(Cell::default(), Cell::EMPTY);
        assert!(Cell::EMPTY.is_empty());
        assert_eq!(Cell::EMPTY.width(), 0);
    }

    #[test]
    fn wide_head_detection() {
        let wide = Cell::glyph(Grapheme::new("漢").unwrap(), CellStyle::DEFAULT);
        let narrow = Cell::glyph(Grapheme::new("a").unwrap(), CellStyle::DEFAULT);
        assert!(wide.is_wide_head());
        assert!(!narrow.is_wide_head());
        assert_eq!(wide.width(), 2);
    }

    #[test]
    fn attribute_names_round_trip() {
        let a = Attributes {
            bold: true,
            strikethrough: true,
            ..Attributes::NONE
        };
        assert_eq!(a.names(), vec!["bold", "strikethrough"]);
        assert_eq!(Attributes::from_names(a.names()).unwrap(), a);
        assert_eq!(Attributes::from_names(["shiny"]), Err("shiny".to_owned()));
    }

    #[test]
    fn serde_round_trip() {
        let cell = Cell::glyph(
            Grapheme::new("é").unwrap(),
            CellStyle::new(Color::Rgb(1, 2, 3), Color::Indexed(4)).with_attrs(Attributes {
                bold: true,
                ..Attributes::NONE
            }),
        );
        let json = serde_json::to_string(&cell).unwrap();
        let back: Cell = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cell);
        let tail: Cell = serde_json::from_str(r#"{"content":{"kind":"wide_tail"}}"#).unwrap();
        assert!(tail.is_wide_tail());
    }
}
