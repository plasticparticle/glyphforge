//! Conversion from the core cell model to Ratatui's buffer.

use glyphforge_core::{Canvas, CellContent, CellStyle, Color, Position, Rect};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as RatRect;
use ratatui::style::{Color as RColor, Modifier, Style};

use crate::terminal::ColorDepth;

/// Maps a core colour to a Ratatui colour, degrading to the given depth.
pub fn to_ratatui_color(color: Color, depth: ColorDepth) -> RColor {
    match color {
        Color::Default => RColor::Reset,
        Color::Indexed(i) => indexed(i, depth),
        Color::Rgb(r, g, b) => match depth {
            ColorDepth::TrueColor => RColor::Rgb(r, g, b),
            ColorDepth::Ansi256 | ColorDepth::Ansi16 => match color.quantize_256() {
                Color::Indexed(i) => indexed(i, depth),
                _ => RColor::Reset,
            },
        },
    }
}

fn indexed(i: u8, depth: ColorDepth) -> RColor {
    match i {
        0 => RColor::Black,
        1 => RColor::Red,
        2 => RColor::Green,
        3 => RColor::Yellow,
        4 => RColor::Blue,
        5 => RColor::Magenta,
        6 => RColor::Cyan,
        7 => RColor::Gray,
        8 => RColor::DarkGray,
        9 => RColor::LightRed,
        10 => RColor::LightGreen,
        11 => RColor::LightYellow,
        12 => RColor::LightBlue,
        13 => RColor::LightMagenta,
        14 => RColor::LightCyan,
        15 => RColor::White,
        _ if depth == ColorDepth::Ansi16 => indexed(ansi256_to_16(i), depth),
        _ => RColor::Indexed(i),
    }
}

/// Coarse mapping of the 256-colour extension onto the ANSI 16 set.
fn ansi256_to_16(i: u8) -> u8 {
    if i < 16 {
        return i;
    }
    if i >= 232 {
        return if i < 244 { 0 } else { 7 };
    }
    let idx = i - 16;
    let (r, g, b) = (idx / 36, (idx / 6) % 6, idx % 6);
    let bright = r.max(g).max(b) >= 4;
    let bit = |v: u8| u8::from(v >= 2);
    let base = bit(r) | (bit(g) << 1) | (bit(b) << 2);
    if bright { base + 8 } else { base }
}

/// Converts a core cell style to a Ratatui style.
pub fn to_ratatui_style(style: CellStyle, depth: ColorDepth) -> Style {
    let mut s = Style::default()
        .fg(to_ratatui_color(style.fg, depth))
        .bg(to_ratatui_color(style.bg, depth));
    let a = style.attrs;
    let mut m = Modifier::empty();
    m.set(Modifier::BOLD, a.bold);
    m.set(Modifier::DIM, a.dim);
    m.set(Modifier::ITALIC, a.italic);
    m.set(Modifier::UNDERLINED, a.underline);
    m.set(Modifier::SLOW_BLINK, a.blink);
    m.set(Modifier::REVERSED, a.reverse);
    m.set(Modifier::CROSSED_OUT, a.strikethrough);
    if !m.is_empty() {
        s = s.add_modifier(m);
    }
    s
}

/// Draws the part of `cells` visible through `viewport` (document
/// coordinates) into `area` (screen coordinates), top-left aligned.
///
/// Cells outside the document are left untouched. Transparent cells are
/// drawn as spaces with `empty_style` so the canvas has a visible "paper".
/// Wide glyphs write their symbol into the head cell and reset the tail,
/// which is Ratatui's own convention for multi-column symbols.
pub fn draw_cells(
    cells: &Canvas,
    viewport: Rect,
    area: RatRect,
    buf: &mut Buffer,
    depth: ColorDepth,
    empty_style: Style,
) {
    let visible = viewport.intersection(cells.bounds());
    for pos in visible.positions() {
        let sx = u32::from(area.x) + u32::from(pos.x - viewport.x);
        let sy = u32::from(area.y) + u32::from(pos.y - viewport.y);
        if sx >= u32::from(area.right()) || sy >= u32::from(area.bottom()) {
            continue;
        }
        let (sx, sy) = (sx as u16, sy as u16);
        let Some(cell) = cells.get(pos) else { continue };
        let Some(target) = buf.cell_mut((sx, sy)) else {
            continue;
        };
        match &cell.content {
            CellContent::Empty => {
                target.set_symbol(" ").set_style(empty_style);
            }
            CellContent::Glyph(g) => {
                let style = to_ratatui_style(cell.style, depth);
                if g.is_wide() && sx + 1 >= area.right() {
                    target.set_symbol(" ").set_style(style);
                } else {
                    target.set_symbol(g.as_str()).set_style(style);
                }
            }
            CellContent::WideTail => {
                if pos.x > viewport.x {
                    target.reset();
                    target.set_style(to_ratatui_style(cell.style, depth));
                } else {
                    target
                        .set_symbol(" ")
                        .set_style(to_ratatui_style(cell.style, depth));
                }
            }
        }
    }
}

/// Converts a screen position inside `area` to a document position using
/// `viewport`, or `None` when outside the area or the document.
pub fn screen_to_document(
    screen: (u16, u16),
    area: RatRect,
    viewport: Rect,
    doc_bounds: Rect,
) -> Option<Position> {
    let (sx, sy) = screen;
    if !area.contains(ratatui::layout::Position::new(sx, sy)) {
        return None;
    }
    let x = u32::from(viewport.x) + u32::from(sx - area.x);
    let y = u32::from(viewport.y) + u32::from(sy - area.y);
    if x >= doc_bounds.right() || y >= doc_bounds.bottom() {
        return None;
    }
    Some(Position::new(x as u16, y as u16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphforge_core::{Cell, Grapheme, Size};

    #[test]
    fn colour_degradation() {
        assert_eq!(
            to_ratatui_color(Color::Default, ColorDepth::Ansi16),
            RColor::Reset
        );
        assert_eq!(
            to_ratatui_color(Color::Rgb(255, 0, 0), ColorDepth::TrueColor),
            RColor::Rgb(255, 0, 0)
        );
        assert_eq!(
            to_ratatui_color(Color::Rgb(255, 0, 0), ColorDepth::Ansi256),
            RColor::Indexed(196)
        );
        assert_eq!(
            to_ratatui_color(Color::Rgb(255, 0, 0), ColorDepth::Ansi16),
            RColor::LightRed
        );
        assert_eq!(
            to_ratatui_color(Color::Indexed(196), ColorDepth::Ansi16),
            RColor::LightRed
        );
        assert_eq!(
            to_ratatui_color(Color::Indexed(4), ColorDepth::Ansi16),
            RColor::Blue
        );
    }

    #[test]
    fn style_modifiers_are_mapped() {
        let mut style = CellStyle::DEFAULT;
        style.attrs.bold = true;
        style.attrs.underline = true;
        let s = to_ratatui_style(style, ColorDepth::TrueColor);
        assert!(
            s.add_modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
        assert!(!s.add_modifier.contains(Modifier::ITALIC));
    }

    fn canvas_with(cells: &[(u16, u16, &str)]) -> Canvas {
        let mut c = Canvas::new(Size::new(6, 3)).unwrap();
        for (x, y, s) in cells {
            c.put(
                Position::new(*x, *y),
                Cell::glyph(Grapheme::new(s).unwrap(), CellStyle::DEFAULT),
            )
            .unwrap();
        }
        c
    }

    #[test]
    fn draws_viewport_with_offset_and_clipping() {
        let cells = canvas_with(&[(0, 0, "a"), (5, 2, "z"), (2, 1, "漢")]);
        let area = RatRect::new(1, 1, 3, 2);
        let mut buf = Buffer::empty(RatRect::new(0, 0, 6, 4));
        draw_cells(
            &cells,
            Rect::new(1, 1, 3, 2),
            area,
            &mut buf,
            ColorDepth::TrueColor,
            Style::default(),
        );
        assert_eq!(buf[(1, 1)].symbol(), " ");
        assert_eq!(buf[(2, 1)].symbol(), "漢");
        assert_eq!(buf[(3, 1)].symbol(), " ");
        assert_eq!(buf[(0, 0)], ratatui::buffer::Cell::EMPTY);
    }

    #[test]
    fn wide_glyph_at_right_edge_of_area_is_blanked() {
        let cells = canvas_with(&[(2, 0, "漢")]);
        let area = RatRect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        draw_cells(
            &cells,
            Rect::new(0, 0, 3, 1),
            area,
            &mut buf,
            ColorDepth::TrueColor,
            Style::default(),
        );
        assert_eq!(buf[(2, 0)].symbol(), " ");
    }

    #[test]
    fn wide_tail_with_scrolled_out_head_is_blank() {
        let cells = canvas_with(&[(0, 0, "漢")]);
        let area = RatRect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        draw_cells(
            &cells,
            Rect::new(1, 0, 3, 1),
            area,
            &mut buf,
            ColorDepth::TrueColor,
            Style::default(),
        );
        assert_eq!(buf[(0, 0)].symbol(), " ");
    }

    #[test]
    fn screen_to_document_mapping() {
        let area = RatRect::new(10, 5, 20, 10);
        let vp = Rect::new(3, 2, 20, 10);
        let bounds = Rect::new(0, 0, 80, 24);
        assert_eq!(
            screen_to_document((10, 5), area, vp, bounds),
            Some(Position::new(3, 2))
        );
        assert_eq!(
            screen_to_document((15, 7), area, vp, bounds),
            Some(Position::new(8, 4))
        );
        assert_eq!(screen_to_document((9, 5), area, vp, bounds), None);
        assert_eq!(
            screen_to_document((29, 14), area, Rect::new(70, 20, 20, 10), bounds),
            None
        );
    }
}
