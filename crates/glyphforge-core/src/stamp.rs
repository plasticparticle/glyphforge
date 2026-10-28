//! UI element stamps for artwork layers: buttons, checkboxes, progress bars
//! and other terminal-UI shapes, drawn as plain cells.
//!
//! Stamps are artwork, not components: once placed they are ordinary cells.
//! A stamp has a natural size derived from its label and can be stretched to
//! a given size where that makes sense (buttons, bars, rules, boxes).

use crate::boxdraw::BorderFamily;
use crate::document::{Attributes, Cell, CellStyle, Color, Grapheme, Size};

/// Visual style of a button stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonStyle {
    /// `[ OK ]`
    Bracket,
    /// `< OK >`
    Angle,
    /// `« OK »`
    Guillemet,
    /// `▕ OK ▏`
    Keycap,
    /// `▐ OK ▌` with a filled face.
    Pill,
    /// ` OK ` on a filled face.
    Solid,
    /// Filled face with a drop shadow below and to the right.
    Shadow,
    /// Underlined, bold label.
    Link,
    /// Three-row button framed in single lines.
    Single,
    /// Three-row button framed in rounded lines.
    Rounded,
    /// Three-row button framed in double lines.
    Double,
    /// Three-row button framed in heavy lines.
    Heavy,
}

/// A stamp kind. The label is passed separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StampKind {
    Button(ButtonStyle),
    /// `☑ label` / `☐ label`
    Checkbox(bool),
    /// `◉ label` / `○ label`
    Radio(bool),
    /// `━━● label` / `●━━ label`
    Toggle(bool),
    /// Horizontal bar; a numeric label (`75`, `75%`) sets the fill.
    Progress,
    /// Vertical scrollbar; a numeric label sets the thumb position.
    Scrollbar,
    /// ` q  quit`: the first word as a reversed key, the rest as text.
    KeyHint,
    /// Two-row tab with a rounded top.
    Tab,
    /// `──┤ label ├──`
    Rule,
    /// `⠋ label`
    Spinner,
}

/// One stamp cell, relative to the stamp's top-left corner.
pub type StampCell = (u16, u16, Cell);

fn text_width(text: &str) -> u16 {
    Grapheme::split_text(text)
        .iter()
        .map(|g| u16::from(g.width()))
        .sum()
}

/// Truncates `text` to at most `width` columns.
fn fit(text: &str, width: u16) -> String {
    let mut used = 0;
    let mut out = String::new();
    for g in Grapheme::split_text(text) {
        if used + u16::from(g.width()) > width {
            break;
        }
        used += u16::from(g.width());
        out.push_str(g.as_str());
    }
    out
}

/// Percentage from a numeric label, clamped to 0..=100.
fn percent(label: &str) -> Option<u16> {
    label
        .trim()
        .trim_end_matches('%')
        .parse::<u16>()
        .ok()
        .map(|p| p.min(100))
}

/// Face and label colours of filled buttons: the brush background is the
/// face when set, otherwise the foreground, otherwise blue.
fn face_colors(style: CellStyle) -> (Color, Color) {
    if style.bg != Color::Default {
        (style.bg, style.fg)
    } else if style.fg != Color::Default {
        (style.fg, style.fg.contrast())
    } else {
        (Color::BLUE, Color::BRIGHT_WHITE)
    }
}

struct Sheet {
    cells: Vec<StampCell>,
}

impl Sheet {
    const fn new() -> Self {
        Self { cells: Vec::new() }
    }

    fn put(&mut self, x: u16, y: u16, glyph: &str, style: CellStyle) {
        if let Ok(g) = Grapheme::new(glyph) {
            self.cells.push((x, y, Cell::glyph(g, style)));
        }
    }

    /// Writes `text` from `x`; returns the column after it.
    fn text(&mut self, x: u16, y: u16, text: &str, style: CellStyle) -> u16 {
        let mut cx = x;
        for g in Grapheme::split_text(text) {
            let w = u16::from(g.width());
            self.cells.push((cx, y, Cell::glyph(g, style)));
            cx += w;
        }
        cx
    }

    fn repeat(&mut self, x: u16, y: u16, count: u16, glyph: &str, style: CellStyle) {
        for i in 0..count {
            self.put(x + i, y, glyph, style);
        }
    }

    /// `text` centred in `width` columns from `x`, padded with spaces.
    fn centred(&mut self, x: u16, y: u16, width: u16, text: &str, style: CellStyle) {
        let text = fit(text, width);
        let left = (width - text_width(&text)) / 2;
        self.repeat(x, y, left, " ", style);
        let end = self.text(x + left, y, &text, style);
        self.repeat(end, y, x + width - end, " ", style);
    }
}

/// The size a stamp takes without stretching.
pub fn natural_size(kind: StampKind, label: &str) -> Size {
    let w = text_width(label).max(1);
    let (width, height) = match kind {
        StampKind::Button(style) => match style {
            ButtonStyle::Bracket
            | ButtonStyle::Angle
            | ButtonStyle::Guillemet
            | ButtonStyle::Keycap
            | ButtonStyle::Pill => (w + 4, 1),
            ButtonStyle::Solid | ButtonStyle::Link => (w + 2, 1),
            ButtonStyle::Shadow => (w + 3, 2),
            ButtonStyle::Single
            | ButtonStyle::Rounded
            | ButtonStyle::Double
            | ButtonStyle::Heavy => (w + 4, 3),
        },
        StampKind::Checkbox(_) | StampKind::Radio(_) | StampKind::Spinner => (w + 2, 1),
        StampKind::Toggle(_) => (w + 4, 1),
        StampKind::Progress => (12, 1),
        StampKind::Scrollbar => (1, 6),
        StampKind::KeyHint => {
            let (key, rest) = split_key(label);
            let rest_w = text_width(rest);
            (
                text_width(key) + 2 + if rest_w > 0 { rest_w + 1 } else { 0 },
                1,
            )
        }
        StampKind::Tab => (w + 4, 2),
        StampKind::Rule => (w + 8, 1),
    };
    Size::new(width, height)
}

fn split_key(label: &str) -> (&str, &str) {
    let label = label.trim();
    match label.split_once(char::is_whitespace) {
        Some((key, rest)) => (key, rest.trim()),
        None => (label, ""),
    }
}

/// Renders a stamp. With `size`, stretchable stamps fill it (never smaller
/// than their chrome needs); others keep their natural size.
pub fn render(
    kind: StampKind,
    label: &str,
    style: CellStyle,
    size: Option<Size>,
) -> Vec<StampCell> {
    let natural = natural_size(kind, label);
    let size = size.unwrap_or(natural);
    let mut s = Sheet::new();
    match kind {
        StampKind::Button(button) => render_button(&mut s, button, label, style, size),
        StampKind::Checkbox(on) => {
            let end = s.text(0, 0, if on { "☑" } else { "☐" }, style);
            s.text(end, 0, &format!(" {label}"), style);
        }
        StampKind::Radio(on) => {
            let end = s.text(0, 0, if on { "◉" } else { "○" }, style);
            s.text(end, 0, &format!(" {label}"), style);
        }
        StampKind::Toggle(on) => {
            let (switch, attrs) = if on {
                (
                    "━━●",
                    Attributes {
                        bold: true,
                        ..Attributes::NONE
                    },
                )
            } else {
                (
                    "●━━",
                    Attributes {
                        dim: true,
                        ..Attributes::NONE
                    },
                )
            };
            let end = s.text(0, 0, switch, style.with_attrs(attrs));
            s.text(end, 0, &format!(" {label}"), style);
        }
        StampKind::Progress => {
            let width = size.width.max(2);
            let filled = (u32::from(width) * u32::from(percent(label).unwrap_or(50)) + 50) / 100;
            let filled = filled as u16;
            s.repeat(0, 0, filled, "█", style);
            s.repeat(filled, 0, width - filled, "░", style);
        }
        StampKind::Scrollbar => {
            let height = size.height.max(3);
            let track = height - 2;
            let thumb = (track / 3).max(1);
            let offset = u32::from(track - thumb) * u32::from(percent(label).unwrap_or(0)) / 100;
            s.put(0, 0, "▲", style);
            for i in 0..track {
                let in_thumb = u32::from(i) >= offset && u32::from(i) < offset + u32::from(thumb);
                s.put(0, 1 + i, if in_thumb { "█" } else { "░" }, style);
            }
            s.put(0, height - 1, "▼", style);
        }
        StampKind::KeyHint => {
            let (key, rest) = split_key(label);
            let reversed = style.with_attrs(Attributes {
                reverse: true,
                bold: true,
                ..style.attrs
            });
            let end = s.text(0, 0, &format!(" {key} "), reversed);
            if !rest.is_empty() {
                s.text(end, 0, &format!(" {rest}"), style);
            }
        }
        StampKind::Tab => {
            let width = size.width.max(natural.width.min(5));
            let g = BorderFamily::Rounded.glyphs();
            s.put(0, 0, g.top_left, style);
            s.repeat(1, 0, width - 2, g.horizontal, style);
            s.put(width - 1, 0, g.top_right, style);
            s.put(0, 1, g.vertical, style);
            s.centred(1, 1, width - 2, label, style);
            s.put(width - 1, 1, g.vertical, style);
        }
        StampKind::Rule => {
            let width = size.width.max(6);
            let text = fit(label, width - 6);
            let dashes = width - 4 - text_width(&text);
            let left = dashes / 2;
            s.repeat(0, 0, left, "─", style);
            let mut x = s.text(left, 0, "┤ ", style);
            x = s.text(x, 0, &text, style);
            x = s.text(x, 0, " ├", style);
            s.repeat(x, 0, width - x, "─", style);
        }
        StampKind::Spinner => {
            let end = s.text(0, 0, "⠋", style);
            s.text(end, 0, &format!(" {label}"), style);
        }
    }
    s.cells
}

fn render_button(s: &mut Sheet, button: ButtonStyle, label: &str, style: CellStyle, size: Size) {
    let (face, ink) = face_colors(style);
    let face_style = CellStyle::new(ink, face).with_attrs(Attributes {
        bold: true,
        ..style.attrs
    });
    match button {
        ButtonStyle::Bracket
        | ButtonStyle::Angle
        | ButtonStyle::Guillemet
        | ButtonStyle::Keycap => {
            let (open, close) = match button {
                ButtonStyle::Bracket => ("[", "]"),
                ButtonStyle::Angle => ("<", ">"),
                ButtonStyle::Guillemet => ("«", "»"),
                _ => ("▕", "▏"),
            };
            let width = size.width.max(3);
            s.put(0, 0, open, style);
            s.centred(1, 0, width - 2, label, style);
            s.put(width - 1, 0, close, style);
        }
        ButtonStyle::Pill => {
            let width = size.width.max(3);
            let cap = CellStyle::new(face, Color::Default);
            s.put(0, 0, "▐", cap);
            s.centred(1, 0, width - 2, label, face_style);
            s.put(width - 1, 0, "▌", cap);
        }
        ButtonStyle::Solid => s.centred(0, 0, size.width.max(1), label, face_style),
        ButtonStyle::Link => {
            let width = size.width.max(1);
            let link = style.with_attrs(Attributes {
                underline: true,
                bold: true,
                ..style.attrs
            });
            let text = fit(label, width);
            let left = (width - text_width(&text)) / 2;
            s.text(left, 0, &text, link);
        }
        ButtonStyle::Shadow => {
            let width = size.width.max(3);
            let shadow = CellStyle::new(Color::BLACK, Color::Default);
            s.centred(0, 0, width - 1, label, face_style);
            s.put(width - 1, 0, "▄", shadow);
            s.repeat(1, 1, width - 1, "▀", shadow);
        }
        ButtonStyle::Single | ButtonStyle::Rounded | ButtonStyle::Double | ButtonStyle::Heavy => {
            let family = match button {
                ButtonStyle::Single => BorderFamily::Single,
                ButtonStyle::Rounded => BorderFamily::Rounded,
                ButtonStyle::Double => BorderFamily::Double,
                _ => BorderFamily::Heavy,
            };
            let g = family.glyphs();
            let width = size.width.max(3);
            let height = size.height.max(3);
            let middle = height / 2;
            s.put(0, 0, g.top_left, style);
            s.repeat(1, 0, width - 2, g.horizontal, style);
            s.put(width - 1, 0, g.top_right, style);
            for y in 1..height - 1 {
                s.put(0, y, g.vertical, style);
                if y == middle {
                    s.centred(1, y, width - 2, label, style);
                } else {
                    s.repeat(1, y, width - 2, " ", style);
                }
                s.put(width - 1, y, g.vertical, style);
            }
            s.put(0, height - 1, g.bottom_left, style);
            s.repeat(1, height - 1, width - 2, g.horizontal, style);
            s.put(width - 1, height - 1, g.bottom_right, style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(kind: StampKind, label: &str, size: Option<Size>) -> Vec<String> {
        let cells = render(kind, label, CellStyle::DEFAULT, size);
        let natural = natural_size(kind, label);
        let size = size.unwrap_or(natural);
        let h = cells
            .iter()
            .map(|c| c.1 + 1)
            .max()
            .unwrap_or(0)
            .max(size.height.min(1));
        let w = cells
            .iter()
            .map(|c| c.0 + u16::from(c.2.width()))
            .max()
            .unwrap_or(0);
        (0..h)
            .map(|y| {
                let mut line = String::new();
                let mut x = 0;
                while x < w {
                    if let Some((_, _, cell)) = cells.iter().find(|c| c.0 == x && c.1 == y) {
                        line.push_str(cell.as_glyph().unwrap().as_str());
                        x += u16::from(cell.width());
                    } else {
                        line.push('.');
                        x += 1;
                    }
                }
                line
            })
            .collect()
    }

    #[test]
    fn buttons_have_their_shapes() {
        use ButtonStyle::*;
        let b = |style| rows(StampKind::Button(style), "OK", None);
        assert_eq!(b(Bracket), vec!["[ OK ]"]);
        assert_eq!(b(Angle), vec!["< OK >"]);
        assert_eq!(b(Guillemet), vec!["« OK »"]);
        assert_eq!(b(Keycap), vec!["▕ OK ▏"]);
        assert_eq!(b(Pill), vec!["▐ OK ▌"]);
        assert_eq!(b(Solid), vec![" OK "]);
        assert_eq!(b(Link), vec![".OK"]);
        assert_eq!(b(Shadow), vec![" OK ▄", ".▀▀▀▀"]);
        assert_eq!(b(Rounded), vec!["╭────╮", "│ OK │", "╰────╯"]);
        assert_eq!(b(Double), vec!["╔════╗", "║ OK ║", "╚════╝"]);
        assert_eq!(
            rows(StampKind::Button(Bracket), "OK", Some(Size::new(10, 1))),
            vec!["[   OK   ]"],
            "stretched and centred"
        );
        assert_eq!(
            rows(StampKind::Button(Single), "Cancel", Some(Size::new(6, 5)))[2],
            "│Canc│",
            "label truncated, middle row"
        );
    }

    #[test]
    fn filled_buttons_use_brush_colours() {
        let style = CellStyle::new(Color::BRIGHT_WHITE, Color::GREEN);
        let cells = render(StampKind::Button(ButtonStyle::Solid), "Go", style, None);
        assert!(cells.iter().all(|c| c.2.style.bg == Color::GREEN));
        let fg_only = CellStyle::new(Color::Indexed(15), Color::Default);
        let cells = render(StampKind::Button(ButtonStyle::Solid), "Go", fg_only, None);
        assert_eq!(cells[0].2.style.bg, Color::Indexed(15));
        assert_eq!(cells[0].2.style.fg, Color::BLACK, "contrasting label");
    }

    #[test]
    fn controls_and_indicators() {
        assert_eq!(
            rows(StampKind::Checkbox(true), "Dark", None),
            vec!["☑ Dark"]
        );
        assert_eq!(rows(StampKind::Radio(false), "A", None), vec!["○ A"]);
        assert_eq!(
            rows(StampKind::Toggle(true), "Wi-Fi", None),
            vec!["━━● Wi-Fi"]
        );
        assert_eq!(
            rows(StampKind::Progress, "25%", Some(Size::new(8, 1))),
            vec!["██░░░░░░"]
        );
        assert_eq!(rows(StampKind::Progress, "x", None), vec!["██████░░░░░░"]);
        assert_eq!(
            rows(StampKind::Scrollbar, "100", Some(Size::new(1, 5))),
            vec!["▲", "░", "░", "█", "▼"]
        );
        assert_eq!(rows(StampKind::KeyHint, "q quit", None), vec![" q  quit"]);
        assert_eq!(
            rows(StampKind::Tab, "Files", None),
            vec!["╭───────╮", "│ Files │"]
        );
        assert_eq!(rows(StampKind::Rule, "Logs", None), vec!["──┤ Logs ├──"]);
        assert_eq!(natural_size(StampKind::Rule, "Logs"), Size::new(12, 1));
        assert_eq!(rows(StampKind::Spinner, "Loading", None), vec!["⠋ Loading"]);
    }
}
