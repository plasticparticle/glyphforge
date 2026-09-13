//! Diff-friendly serialisation of a [`Canvas`].
//!
//! Instead of one object per cell, a canvas is stored as text runs (one
//! per contiguous stretch of opaque cells) and style runs (one per stretch
//! of identical non-default style). Editing one row touches one line of
//! the file. Wide-glyph tails are not stored; they are rebuilt on load.

use serde::{Deserialize, Serialize};

use super::{
    Attributes, Canvas, CanvasError, Cell, CellContent, CellStyle, Color, Grapheme, Position, Size,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasRepr {
    pub width: u16,
    pub height: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<TextRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<StyleRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRun {
    pub y: u16,
    pub x: u16,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRun {
    pub y: u16,
    pub x: u16,
    pub len: u16,
    #[serde(default, skip_serializing_if = "is_default_color")]
    pub fg: Color,
    #[serde(default, skip_serializing_if = "is_default_color")]
    pub bg: Color,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if takes a reference"
)]
fn is_default_color(c: &Color) -> bool {
    *c == Color::Default
}

/// Errors while rebuilding a canvas from its representation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanvasReprError {
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error("text run at ({x}, {y}) does not fit into the {width}x{height} canvas")]
    RunOutOfBounds {
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
    #[error("unknown attribute {0:?}")]
    UnknownAttribute(String),
}

impl From<&Canvas> for CanvasRepr {
    fn from(canvas: &Canvas) -> Self {
        let mut text = Vec::new();
        let mut styles = Vec::new();
        for y in 0..canvas.height() {
            let Some(row) = canvas.row(y) else { continue };
            let mut run: Option<TextRun> = None;
            let mut style_run: Option<(StyleRun, CellStyle)> = None;
            for (x, cell) in row.iter().enumerate() {
                let x = x as u16;
                // Text runs.
                match &cell.content {
                    CellContent::Glyph(g) => match run.as_mut() {
                        Some(r) => r.text.push_str(g.as_str()),
                        None => {
                            run = Some(TextRun {
                                y,
                                x,
                                text: g.as_str().to_owned(),
                            });
                        }
                    },
                    CellContent::WideTail => {}
                    CellContent::Empty => {
                        if let Some(r) = run.take() {
                            text.push(r);
                        }
                    }
                }
                // Style runs over opaque cells.
                let opaque = !cell.is_empty();
                let same = style_run.as_ref().is_some_and(|(_, s)| *s == cell.style);
                if opaque && same {
                    if let Some((r, _)) = style_run.as_mut() {
                        r.len += 1;
                    }
                } else {
                    if let Some((r, _)) = style_run.take() {
                        styles.push(r);
                    }
                    if opaque && cell.style != CellStyle::DEFAULT {
                        style_run = Some((
                            StyleRun {
                                y,
                                x,
                                len: 1,
                                fg: cell.style.fg,
                                bg: cell.style.bg,
                                attrs: cell
                                    .style
                                    .attrs
                                    .names()
                                    .iter()
                                    .map(|s| (*s).to_owned())
                                    .collect(),
                            },
                            cell.style,
                        ));
                    }
                }
            }
            if let Some(r) = run.take() {
                text.push(r);
            }
            if let Some((r, _)) = style_run.take() {
                styles.push(r);
            }
        }
        Self {
            width: canvas.width(),
            height: canvas.height(),
            text,
            styles,
        }
    }
}

impl TryFrom<CanvasRepr> for Canvas {
    type Error = CanvasReprError;

    fn try_from(repr: CanvasRepr) -> Result<Self, Self::Error> {
        let size = Size::new(repr.width, repr.height);
        let mut canvas = Canvas::new(size)?;
        for run in &repr.text {
            let mut x = run.x;
            for g in Grapheme::split_text(&run.text) {
                let w = u16::from(g.width());
                if run.y >= size.height || x.saturating_add(w) > size.width {
                    return Err(CanvasReprError::RunOutOfBounds {
                        x: run.x,
                        y: run.y,
                        width: size.width,
                        height: size.height,
                    });
                }
                canvas.put(Position::new(x, run.y), Cell::glyph(g, CellStyle::DEFAULT))?;
                x += w;
            }
        }
        for run in &repr.styles {
            let attrs = Attributes::from_names(run.attrs.iter().map(String::as_str))
                .map_err(CanvasReprError::UnknownAttribute)?;
            let style = CellStyle {
                fg: run.fg,
                bg: run.bg,
                attrs,
            };
            for x in run.x..run.x.saturating_add(run.len) {
                canvas.set_style(Position::new(x, run.y), style);
            }
        }
        Ok(canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_wide_glyphs_and_styles() {
        let mut c = Canvas::new(Size::new(8, 2)).unwrap();
        let red = CellStyle::new(Color::RED, Color::Default);
        c.put(
            Position::new(1, 0),
            Cell::glyph(Grapheme::new("a").unwrap(), red),
        )
        .unwrap();
        c.put(
            Position::new(2, 0),
            Cell::glyph(Grapheme::new("漢").unwrap(), red),
        )
        .unwrap();
        c.put(
            Position::new(5, 0),
            Cell::glyph(Grapheme::new("b").unwrap(), CellStyle::DEFAULT),
        )
        .unwrap();
        c.put(
            Position::new(0, 1),
            Cell::blank(CellStyle::new(Color::Default, Color::BLUE)),
        )
        .unwrap();
        let repr = CanvasRepr::from(&c);
        assert_eq!(repr.text.len(), 3);
        assert_eq!(
            repr.text[0],
            TextRun {
                y: 0,
                x: 1,
                text: "a漢".into()
            }
        );
        assert_eq!(
            repr.text[1],
            TextRun {
                y: 0,
                x: 5,
                text: "b".into()
            }
        );
        assert_eq!(repr.styles.len(), 2);
        assert_eq!(
            repr.styles[0].len, 3,
            "wide tail is covered by the style run"
        );
        let back = Canvas::try_from(repr.clone()).unwrap();
        assert_eq!(back, c);
        assert!(back.invariant_holds());
        let json = serde_json::to_string(&repr).unwrap();
        assert!(!json.contains("wide_tail"));
        let repr2: CanvasRepr = serde_json::from_str(&json).unwrap();
        assert_eq!(repr2, repr);
    }

    #[test]
    fn out_of_bounds_runs_are_rejected() {
        let repr = CanvasRepr {
            width: 3,
            height: 1,
            text: vec![TextRun {
                y: 0,
                x: 2,
                text: "漢".into(),
            }],
            styles: vec![],
        };
        assert!(matches!(
            Canvas::try_from(repr),
            Err(CanvasReprError::RunOutOfBounds { .. })
        ));
    }
}
