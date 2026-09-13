//! Low-level drawing helpers over a [`Canvas`]: text with clipping and wide
//! glyphs, fills and borders.

use crate::boxdraw::BorderFamily;
use crate::document::{Canvas, Cell, CellStyle, Grapheme, Position, Rect};

/// Writes `text` starting at (`x`, `y`), clipped to `clip`. Returns the
/// number of columns written. A wide glyph that would cross the clip edge
/// is not drawn.
pub fn draw_text(
    canvas: &mut Canvas,
    x: u16,
    y: u16,
    text: &str,
    style: CellStyle,
    clip: Rect,
) -> u16 {
    let clip = clip.intersection(canvas.bounds());
    if !clip.contains(Position::new(x, y)) {
        return 0;
    }
    let mut cx = x;
    for g in Grapheme::split_text(text) {
        let w = u16::from(g.width());
        if u32::from(cx) + u32::from(w) > clip.right() {
            break;
        }
        let _ = canvas.put(Position::new(cx, y), Cell::glyph(g, style));
        cx += w;
    }
    cx - x
}

/// Display width of `text` in columns.
pub fn text_width(text: &str) -> u16 {
    Grapheme::split_text(text)
        .iter()
        .map(|g| u16::from(g.width()))
        .sum()
}

/// Truncates `text` to `max` columns, appending `…` when it does not fit
/// (and `max` allows it).
pub fn truncate(text: &str, max: u16) -> String {
    if text_width(text) <= max {
        return text.to_owned();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0u16;
    for g in Grapheme::split_text(text) {
        let w = u16::from(g.width());
        if width + w > max.saturating_sub(1) {
            break;
        }
        out.push_str(g.as_str());
        width += w;
    }
    out.push('…');
    out
}

/// Fills `rect` (clipped to the canvas) with `cell`.
pub fn fill(canvas: &mut Canvas, rect: Rect, cell: &Cell) {
    for pos in rect.intersection(canvas.bounds()).positions() {
        let _ = canvas.put(pos, cell.clone());
    }
}

/// Draws a border of `family` along the edges of `rect`.
pub fn draw_border(canvas: &mut Canvas, rect: Rect, family: BorderFamily, style: CellStyle) {
    if rect.width < 2 || rect.height < 2 || !family.is_visible() {
        return;
    }
    let g = family.glyphs();
    let glyph = |s: &str| {
        Cell::glyph(
            Grapheme::new(s).unwrap_or_else(|_| Grapheme::space()),
            style,
        )
    };
    let right = (rect.right() - 1) as u16;
    let bottom = (rect.bottom() - 1) as u16;
    let clip = canvas.bounds();
    for x in rect.x + 1..right {
        if clip.contains(Position::new(x, rect.y)) {
            let _ = canvas.put(Position::new(x, rect.y), glyph(g.horizontal));
        }
        if clip.contains(Position::new(x, bottom)) {
            let _ = canvas.put(Position::new(x, bottom), glyph(g.horizontal));
        }
    }
    if family != BorderFamily::Minimal {
        for y in rect.y + 1..bottom {
            if clip.contains(Position::new(rect.x, y)) {
                let _ = canvas.put(Position::new(rect.x, y), glyph(g.vertical));
            }
            if clip.contains(Position::new(right, y)) {
                let _ = canvas.put(Position::new(right, y), glyph(g.vertical));
            }
        }
    }
    for (pos, s) in [
        (Position::new(rect.x, rect.y), g.top_left),
        (Position::new(right, rect.y), g.top_right),
        (Position::new(rect.x, bottom), g.bottom_left),
        (Position::new(right, bottom), g.bottom_right),
    ] {
        if clip.contains(pos) {
            let _ = canvas.put(pos, glyph(s));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Size;
    use crate::render::to_text_lines;

    #[test]
    fn text_is_clipped_and_wide_glyphs_do_not_straddle() {
        let mut c = Canvas::new(Size::new(6, 1)).unwrap();
        let n = draw_text(
            &mut c,
            1,
            0,
            "ab漢字",
            CellStyle::DEFAULT,
            Rect::new(0, 0, 5, 1),
        );
        assert_eq!(n, 4, "a, b and 漢 fit; 字 would cross the clip");
        assert_eq!(to_text_lines(&c)[0], " ab漢");
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("Request latency", 8), "Request…");
        assert_eq!(truncate("short", 8), "short");
        assert_eq!(truncate("漢字漢字", 3), "漢…");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn border_families_render() {
        let mut c = Canvas::new(Size::new(5, 3)).unwrap();
        draw_border(
            &mut c,
            Rect::new(0, 0, 5, 3),
            BorderFamily::Double,
            CellStyle::DEFAULT,
        );
        assert_eq!(to_text_lines(&c), vec!["╔═══╗", "║   ║", "╚═══╝"]);
        let mut c = Canvas::new(Size::new(5, 3)).unwrap();
        draw_border(
            &mut c,
            Rect::new(0, 0, 5, 3),
            BorderFamily::Minimal,
            CellStyle::DEFAULT,
        );
        assert_eq!(to_text_lines(&c), vec!["─────", "", "─────"]);
        let mut c = Canvas::new(Size::new(5, 3)).unwrap();
        draw_border(
            &mut c,
            Rect::new(0, 0, 5, 3),
            BorderFamily::None,
            CellStyle::DEFAULT,
        );
        assert_eq!(to_text_lines(&c), vec!["", "", ""]);
    }

    #[test]
    fn border_outside_canvas_is_clipped_not_panicking() {
        let mut c = Canvas::new(Size::new(3, 2)).unwrap();
        draw_border(
            &mut c,
            Rect::new(1, 1, 10, 10),
            BorderFamily::Single,
            CellStyle::DEFAULT,
        );
        assert_eq!(to_text_lines(&c), vec!["", " ┌─"]);
    }
}
