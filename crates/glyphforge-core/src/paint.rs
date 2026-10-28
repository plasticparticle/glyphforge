//! Cell-level drawing geometry for Subcell Mode tools.
//!
//! Pure functions that turn tool input (two points, a seed cell, a path)
//! into cell writes: plain glyph stamping, auto-joining box-drawing lines
//! and pattern brushes. The editor records the result as a `set_cells` operation, so a
//! stroke, a rectangle or a flood fill is undoable like any other patch, and
//! a headless caller can produce exactly the same cells.

use std::collections::{HashSet, VecDeque};

use crate::boxdraw::{Arms, BorderFamily, Side, Weight};
use crate::document::{Canvas, Cell, CellStyle, Color, Grapheme, Position, Rect, Size};
use crate::patch::CellWrite;

/// Every position on the straight line from `a` to `b`, both ends included
/// (Bresenham).
pub fn line(a: Position, b: Position) -> Vec<Position> {
    let (mut x, mut y) = (i32::from(a.x), i32::from(a.y));
    let (x1, y1) = (i32::from(b.x), i32::from(b.y));
    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let sx = if x < x1 { 1 } else { -1 };
    let sy = if y < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::with_capacity((dx.max(-dy) + 1) as usize);
    loop {
        out.push(Position::new(x as u16, y as u16));
        if x == x1 && y == y1 {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// The rectangle spanned by two corner positions, in any order.
pub fn rect_between(a: Position, b: Position) -> Rect {
    Rect::new(
        a.x.min(b.x),
        a.y.min(b.y),
        a.x.abs_diff(b.x) + 1,
        a.y.abs_diff(b.y) + 1,
    )
}

/// The border positions of `rect`, each exactly once, row-major.
pub fn rect_outline(rect: Rect) -> Vec<Position> {
    rect.positions()
        .filter(|p| {
            p.x == rect.x
                || p.y == rect.y
                || u32::from(p.x) + 1 == rect.right()
                || u32::from(p.y) + 1 == rect.bottom()
        })
        .collect()
}

/// A box-drawing frame around `rect` in `family`. Degenerate rectangles
/// become a horizontal or vertical rule.
pub fn box_outline(rect: Rect, family: BorderFamily, style: CellStyle) -> Vec<CellWrite> {
    let g = family.glyphs();
    let last_x = u32::from(rect.x) + u32::from(rect.width.max(1)) - 1;
    let last_y = u32::from(rect.y) + u32::from(rect.height.max(1)) - 1;
    rect_outline(rect)
        .into_iter()
        .map(|p| {
            let (left, right) = (p.x == rect.x, u32::from(p.x) == last_x);
            let (top, bottom) = (p.y == rect.y, u32::from(p.y) == last_y);
            let symbol = if rect.height == 1 {
                g.horizontal
            } else if rect.width == 1 {
                g.vertical
            } else {
                match (top, bottom, left, right) {
                    (true, _, true, _) => g.top_left,
                    (true, _, _, true) => g.top_right,
                    (_, true, true, _) => g.bottom_left,
                    (_, true, _, true) => g.bottom_right,
                    (true, ..) | (_, true, ..) => g.horizontal,
                    _ => g.vertical,
                }
            };
            let grapheme = Grapheme::new(symbol).unwrap_or_else(|_| Grapheme::space());
            CellWrite {
                x: p.x,
                y: p.y,
                cell: Cell::glyph(grapheme, style),
            }
        })
        .collect()
}

/// The 4-connected region of cells identical to the cell at `seed` (same
/// content and style), including `seed`. Empty when `seed` is outside.
pub fn flood_region(canvas: &Canvas, seed: Position) -> Vec<Position> {
    let Some(target) = canvas.get(seed) else {
        return Vec::new();
    };
    let size = canvas.size();
    let mut seen = HashSet::from([seed]);
    let mut queue = VecDeque::from([seed]);
    let mut out = Vec::new();
    while let Some(p) = queue.pop_front() {
        out.push(p);
        let neighbours = [
            p.x.checked_sub(1).map(|x| Position::new(x, p.y)),
            p.y.checked_sub(1).map(|y| Position::new(p.x, y)),
            p.x.checked_add(1).map(|x| Position::new(x, p.y)),
            p.y.checked_add(1).map(|y| Position::new(p.x, y)),
        ];
        for n in neighbours.into_iter().flatten() {
            if size.contains(n) && canvas.get(n) == Some(target) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    out.sort_by_key(|p| (p.y, p.x));
    out
}

/// Writes `cell` at every position that fits into `size`. Duplicates are
/// dropped; a double-width glyph skips the last column and any position
/// whose left neighbour already received it, so the glyphs do not destroy
/// each other.
pub fn stamp(positions: &[Position], cell: &Cell, size: Size) -> Vec<CellWrite> {
    stamp_each(positions.iter().map(|p| (*p, cell.clone())), size)
}

/// Like [`stamp`], with a cell per position.
pub fn stamp_each(cells: impl IntoIterator<Item = (Position, Cell)>, size: Size) -> Vec<CellWrite> {
    let mut taken = HashSet::new();
    let mut wide_at = HashSet::new();
    let mut out = Vec::new();
    for (p, cell) in cells {
        let wide = cell.is_wide_head();
        if !size.contains(p) || (wide && p.x + 1 >= size.width) {
            continue;
        }
        if p.x > 0 && wide_at.contains(&Position::new(p.x - 1, p.y)) {
            continue;
        }
        if taken.insert(p) {
            if wide {
                wide_at.insert(p);
            }
            out.push(CellWrite {
                x: p.x,
                y: p.y,
                cell,
            });
        }
    }
    out
}

/// A 4-connected path from `a` to `b` (no diagonal steps), both ends
/// included. Box-drawing lines can only join through cell sides.
pub fn line4(a: Position, b: Position) -> Vec<Position> {
    let mut out: Vec<Position> = Vec::new();
    for p in line(a, b) {
        if let Some(&q) = out.last() {
            if p.x != q.x && p.y != q.y {
                out.push(Position::new(p.x, q.y));
            }
        }
        out.push(p);
    }
    out
}

/// A path from `a` to `b` that runs horizontally first, then vertically.
pub fn elbow(a: Position, b: Position) -> Vec<Position> {
    let corner = Position::new(b.x, a.y);
    let mut out = line(a, corner);
    out.extend(line(corner, b).into_iter().skip(1));
    out
}

/// Pairs of consecutive positions of a path.
pub fn path_edges(path: &[Position]) -> Vec<(Position, Position)> {
    path.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Edges around the border of `rect`, closing the loop.
pub fn rect_edges(rect: Rect) -> Vec<(Position, Position)> {
    if rect.width <= 1 || rect.height <= 1 {
        let a = Position::new(rect.x, rect.y);
        let b = Position::new(
            rect.x + rect.width.max(1) - 1,
            rect.y + rect.height.max(1) - 1,
        );
        return path_edges(&line(a, b));
    }
    let (l, t) = (rect.x, rect.y);
    let (r, bm) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
    let mut ring = line(Position::new(l, t), Position::new(r, t));
    ring.extend(
        line(Position::new(r, t), Position::new(r, bm))
            .into_iter()
            .skip(1),
    );
    ring.extend(
        line(Position::new(r, bm), Position::new(l, bm))
            .into_iter()
            .skip(1),
    );
    ring.extend(
        line(Position::new(l, bm), Position::new(l, t))
            .into_iter()
            .skip(1),
    );
    path_edges(&ring)
}

/// How an auto-joining line brush draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineStyle {
    pub weight: Weight,
    /// Light corners as arcs (`╭`).
    pub rounded: bool,
}

/// The side of `from` that faces the 4-neighbour `to`.
fn side_towards(from: Position, to: Position) -> Option<Side> {
    match (
        i32::from(to.x) - i32::from(from.x),
        i32::from(to.y) - i32::from(from.y),
    ) {
        (0, -1) => Some(Side::Up),
        (1, 0) => Some(Side::Right),
        (0, 1) => Some(Side::Down),
        (-1, 0) => Some(Side::Left),
        _ => None,
    }
}

fn neighbour(p: Position, side: Side) -> Option<Position> {
    Some(match side {
        Side::Up => Position::new(p.x, p.y.checked_sub(1)?),
        Side::Right => Position::new(p.x.checked_add(1)?, p.y),
        Side::Down => Position::new(p.x, p.y.checked_add(1)?),
        Side::Left => Position::new(p.x.checked_sub(1)?, p.y),
    })
}

fn arms_at(canvas: &Canvas, pos: Position) -> Option<Arms> {
    canvas
        .get(pos)
        .and_then(Cell::as_glyph)
        .and_then(|g| Arms::of(g.as_str()))
}

/// The arms of the line glyph at `pos` that really connect somewhere: to a
/// neighbour with an arm pointing back, or along a side `new` draws. The
/// straight run-through of a line's loose end is dropped, so a stroke that
/// continues from an end turns a corner instead of making a tee.
fn connected_arms(canvas: &Canvas, pos: Position, new: Arms) -> Arms {
    let Some(arms) = arms_at(canvas, pos) else {
        return Arms::NONE;
    };
    Side::ALL.into_iter().fold(Arms::NONE, |acc, side| {
        let weight = arms.get(side);
        let connected = weight != Weight::None
            && (new.get(side) != Weight::None
                || neighbour(pos, side)
                    .and_then(|n| arms_at(canvas, n))
                    .is_some_and(|n| n.get(side.opposite()) != Weight::None));
        if connected {
            acc.with(side, weight)
        } else {
            acc
        }
    })
}

/// Box-drawing cells for lines along `edges` (pairs of 4-neighbours), merged
/// with the box-drawing glyphs already on `canvas`: crossing a line makes
/// `┼`, ending on one makes `├`. `cells` lists positions to draw even
/// without an edge (a single click); a lone cell becomes a horizontal line
/// unless it already holds a line, which is then only restyled.
pub fn join_edges(
    canvas: &Canvas,
    cells: &[Position],
    edges: &[(Position, Position)],
    line: LineStyle,
    style: CellStyle,
) -> Vec<CellWrite> {
    let mut arms: std::collections::BTreeMap<(u16, u16), Arms> =
        cells.iter().map(|p| ((p.y, p.x), Arms::NONE)).collect();
    for &(a, b) in edges {
        let Some(side) = side_towards(a, b) else {
            continue;
        };
        let ea = arms.entry((a.y, a.x)).or_default();
        *ea = ea.with(side, line.weight);
        let eb = arms.entry((b.y, b.x)).or_default();
        *eb = eb.with(side.opposite(), line.weight);
    }
    let size = canvas.size();
    arms.into_iter()
        .filter_map(|((y, x), new)| {
            let pos = Position::new(x, y);
            if !size.contains(pos) {
                return None;
            }
            let existing = connected_arms(canvas, pos, new);
            let mut merged = existing.merge(new);
            if merged.is_empty() {
                merged = Arms::NONE
                    .with(Side::Left, line.weight)
                    .with(Side::Right, line.weight);
            }
            let glyph = merged.glyph(line.rounded)?;
            let grapheme = Grapheme::new(glyph).ok()?;
            Some(CellWrite {
                x,
                y,
                cell: Cell::glyph(grapheme, style),
            })
        })
        .collect()
}

/// Rewrites the box-drawing neighbours of `pos` without their arm into
/// `pos`, for erasing a cell out of a joined line: `┬` above becomes `─`.
pub fn detach(canvas: &Canvas, pos: Position) -> Vec<CellWrite> {
    Side::ALL
        .into_iter()
        .filter_map(|side| {
            let n = neighbour(pos, side)?;
            let cell = canvas.get(n)?;
            let arms = Arms::of(cell.as_glyph()?.as_str())?;
            let towards = side.opposite();
            if arms.get(towards) == Weight::None {
                return None;
            }
            let rest = arms.with(towards, Weight::None);
            let cell = match rest.glyph(false).and_then(|g| Grapheme::new(g).ok()) {
                Some(g) => Cell::glyph(g, cell.style),
                None => Cell::EMPTY,
            };
            Some(CellWrite {
                x: n.x,
                y: n.y,
                cell,
            })
        })
        .collect()
}

/// Pattern brushes: glyph choice by position or by what is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// Each pass darkens the cell one step: `░ ▒ ▓ █`.
    Shade,
    /// The brush glyph on every other cell, like a chessboard.
    Checker,
    /// The brush glyph on every other row.
    Rows,
    /// The brush glyph on every other column.
    Columns,
    /// The brush glyph on every other cell of every other row.
    Dots,
    /// The brush glyph with the foreground running from the brush
    /// foreground to the brush background colour, left to right.
    Gradient,
}

/// The shade ramp used by [`Pattern::Shade`], lightest first.
pub const SHADE_RAMP: [&str; 4] = ["░", "▒", "▓", "█"];

/// Cells a pattern brush writes at `positions`. Masks use canvas
/// coordinates, so separate strokes line up. The gradient spans `span`, or
/// the bounding box of `positions` when `None`.
pub fn pattern_writes(
    canvas: &Canvas,
    positions: &[Position],
    pattern: Pattern,
    glyph: &Grapheme,
    style: CellStyle,
    span: Option<Rect>,
) -> Vec<CellWrite> {
    let span = span.unwrap_or_else(|| bounding_box(positions));
    let cells = positions.iter().filter_map(|&p| {
        let glyph_cell = || Cell::glyph(glyph.clone(), style);
        let cell = match pattern {
            Pattern::Shade => {
                let current = canvas
                    .get(p)
                    .and_then(Cell::as_glyph)
                    .and_then(|g| SHADE_RAMP.iter().position(|s| *s == g.as_str()));
                let next = current.map_or(0, |i| (i + 1).min(SHADE_RAMP.len() - 1));
                Cell::glyph(Grapheme::new(SHADE_RAMP[next]).ok()?, style)
            }
            Pattern::Checker => ((p.x + p.y) % 2 == 0).then(glyph_cell)?,
            Pattern::Rows => (p.y % 2 == 0).then(glyph_cell)?,
            Pattern::Columns => (p.x % 2 == 0).then(glyph_cell)?,
            Pattern::Dots => (p.x % 2 == 0 && p.y % 2 == 0).then(glyph_cell)?,
            Pattern::Gradient => {
                let t = if span.width > 1 {
                    f32::from(p.x.saturating_sub(span.x)) / f32::from(span.width - 1)
                } else {
                    0.0
                };
                let fg = gradient_color(style.fg, style.bg, t);
                Cell::glyph(
                    glyph.clone(),
                    CellStyle {
                        fg,
                        bg: Color::Default,
                        ..style
                    },
                )
            }
        };
        Some((p, cell))
    });
    stamp_each(cells.collect::<Vec<_>>(), canvas.size())
}

fn gradient_color(from: Color, to: Color, t: f32) -> Color {
    match (from.to_rgb(), to.to_rgb()) {
        (Some((r, g, b)), Some((r2, g2, b2))) => Color::Rgb(r, g, b).mix(Color::Rgb(r2, g2, b2), t),
        (Some(_), None) => from,
        (None, _) => to,
    }
}

fn bounding_box(positions: &[Position]) -> Rect {
    let min_x = positions.iter().map(|p| p.x).min().unwrap_or(0);
    let max_x = positions.iter().map(|p| p.x).max().unwrap_or(0);
    let min_y = positions.iter().map(|p| p.y).min().unwrap_or(0);
    let max_y = positions.iter().map(|p| p.y).max().unwrap_or(0);
    Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: u16, y: u16) -> Position {
        Position::new(x, y)
    }

    fn glyph(s: &str) -> Cell {
        Cell::glyph(Grapheme::new(s).unwrap(), CellStyle::DEFAULT)
    }

    #[test]
    fn line_covers_both_ends_in_any_direction() {
        assert_eq!(
            line(p(0, 0), p(3, 0)),
            vec![p(0, 0), p(1, 0), p(2, 0), p(3, 0)]
        );
        assert_eq!(line(p(2, 2), p(2, 2)), vec![p(2, 2)]);
        assert_eq!(
            line(p(3, 3), p(0, 0)),
            vec![p(3, 3), p(2, 2), p(1, 1), p(0, 0)]
        );
        let steep = line(p(0, 0), p(1, 4));
        assert_eq!(steep.len(), 5, "one cell per row on a steep line");
        assert_eq!(steep.first(), Some(&p(0, 0)));
        assert_eq!(steep.last(), Some(&p(1, 4)));
    }

    #[test]
    fn rect_between_normalises_corners() {
        assert_eq!(rect_between(p(5, 1), p(2, 4)), Rect::new(2, 1, 4, 4));
        assert_eq!(rect_between(p(3, 3), p(3, 3)), Rect::new(3, 3, 1, 1));
    }

    #[test]
    fn outline_has_each_border_cell_once() {
        let o = rect_outline(Rect::new(0, 0, 4, 3));
        assert_eq!(o.len(), 10);
        assert!(!o.contains(&p(1, 1)));
        assert_eq!(rect_outline(Rect::new(0, 0, 1, 1)), vec![p(0, 0)]);
    }

    #[test]
    fn box_uses_corners_and_rules() {
        let cells = box_outline(
            Rect::new(1, 1, 4, 3),
            BorderFamily::Single,
            CellStyle::DEFAULT,
        );
        let at = |x, y| {
            cells
                .iter()
                .find(|w| w.x == x && w.y == y)
                .and_then(|w| w.cell.as_glyph())
                .map(|g| g.as_str().to_owned())
        };
        assert_eq!(at(1, 1).as_deref(), Some("┌"));
        assert_eq!(at(2, 1).as_deref(), Some("─"));
        assert_eq!(at(4, 1).as_deref(), Some("┐"));
        assert_eq!(at(1, 2).as_deref(), Some("│"));
        assert_eq!(at(4, 3).as_deref(), Some("┘"));
        assert_eq!(at(2, 2), None, "interior untouched");
        let rule = box_outline(
            Rect::new(0, 0, 3, 1),
            BorderFamily::Double,
            CellStyle::DEFAULT,
        );
        assert!(
            rule.iter()
                .all(|w| w.cell.as_glyph().unwrap().as_str() == "═")
        );
    }

    #[test]
    fn flood_fills_connected_identical_cells_only() {
        let mut c = Canvas::new(Size::new(5, 3)).unwrap();
        // A vertical wall at x = 2 splits the canvas.
        for y in 0..3 {
            c.put(p(2, y), glyph("#")).unwrap();
        }
        let left = flood_region(&c, p(0, 0));
        assert_eq!(left.len(), 6);
        assert!(left.iter().all(|q| q.x < 2));
        assert_eq!(flood_region(&c, p(2, 1)).len(), 3);
        assert!(flood_region(&c, p(9, 9)).is_empty());
    }

    #[test]
    fn stamp_clips_dedupes_and_spaces_wide_glyphs() {
        let size = Size::new(4, 2);
        let pts = [p(0, 0), p(0, 0), p(9, 0), p(3, 1)];
        assert_eq!(stamp(&pts, &glyph("x"), size).len(), 2);
        let wide = glyph("漢");
        let row = [p(0, 0), p(1, 0), p(2, 0), p(3, 0)];
        let writes = stamp(&row, &wide, size);
        let xs: Vec<u16> = writes.iter().map(|w| w.x).collect();
        assert_eq!(xs, vec![0, 2], "neighbours and the last column are skipped");
    }

    fn text_rows(canvas: &Canvas) -> Vec<String> {
        (0..canvas.size().height)
            .map(|y| {
                (0..canvas.size().width)
                    .map(|x| {
                        canvas
                            .get(p(x, y))
                            .and_then(Cell::as_glyph)
                            .map_or(".".to_owned(), |g| g.as_str().to_owned())
                    })
                    .collect()
            })
            .collect()
    }

    fn apply(canvas: &mut Canvas, writes: Vec<CellWrite>) {
        for w in writes {
            canvas.put(p(w.x, w.y), w.cell).unwrap();
        }
    }

    fn join(
        canvas: &mut Canvas,
        cells: &[Position],
        edges: &[(Position, Position)],
        line: LineStyle,
    ) {
        let writes = join_edges(canvas, cells, edges, line, CellStyle::DEFAULT);
        apply(canvas, writes);
    }

    const LIGHT: LineStyle = LineStyle {
        weight: Weight::Light,
        rounded: false,
    };

    #[test]
    fn paths_are_four_connected() {
        let path = line4(p(0, 0), p(3, 2));
        for w in path.windows(2) {
            assert_eq!(
                w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y),
                1,
                "{path:?}"
            );
        }
        assert_eq!(
            elbow(p(0, 0), p(2, 2)),
            vec![p(0, 0), p(1, 0), p(2, 0), p(2, 1), p(2, 2)]
        );
        assert_eq!(rect_edges(Rect::new(0, 0, 3, 3)).len(), 8, "closed ring");
    }

    #[test]
    fn joined_lines_form_corners_tees_and_crossings() {
        let mut c = Canvas::new(Size::new(5, 5)).unwrap();
        join(&mut c, &[], &rect_edges(Rect::new(0, 0, 5, 3)), LIGHT);
        join(&mut c, &[], &path_edges(&line4(p(2, 0), p(2, 4))), LIGHT);
        assert_eq!(
            text_rows(&c),
            vec!["┌─┬─┐", "│.│.│", "└─┼─┘", "..│..", "..│.."]
        );

        let double = LineStyle {
            weight: Weight::Double,
            rounded: false,
        };
        join(&mut c, &[], &path_edges(&line4(p(0, 1), p(4, 1))), double);
        assert_eq!(text_rows(&c)[1], "╞═╪═╡");

        let rounded = LineStyle {
            weight: Weight::Light,
            rounded: true,
        };
        let mut r = Canvas::new(Size::new(3, 2)).unwrap();
        join(&mut r, &[], &rect_edges(Rect::new(0, 0, 3, 2)), rounded);
        assert_eq!(text_rows(&r), vec!["╭─╮", "╰─╯"]);

        let mut single = Canvas::new(Size::new(3, 1)).unwrap();
        join(&mut single, &[p(1, 0)], &[], LIGHT);
        assert_eq!(text_rows(&single), vec![".─."], "a click draws a dash");

        // Separate strokes that meet at loose ends turn corners.
        let mut box4 = Canvas::new(Size::new(3, 3)).unwrap();
        join(&mut box4, &[], &path_edges(&line4(p(0, 0), p(2, 0))), LIGHT);
        join(&mut box4, &[], &path_edges(&line4(p(2, 0), p(2, 2))), LIGHT);
        join(&mut box4, &[], &path_edges(&line4(p(2, 2), p(0, 2))), LIGHT);
        join(&mut box4, &[], &path_edges(&line4(p(0, 2), p(0, 0))), LIGHT);
        assert_eq!(text_rows(&box4), vec!["┌─┐", "│.│", "└─┘"]);
    }

    #[test]
    fn detaching_removes_arms_into_the_erased_cell() {
        let mut c = Canvas::new(Size::new(3, 3)).unwrap();
        let plus = [
            (p(1, 0), p(1, 1)),
            (p(1, 1), p(1, 2)),
            (p(0, 1), p(1, 1)),
            (p(1, 1), p(2, 1)),
        ];
        join(&mut c, &[], &plus, LIGHT);
        assert_eq!(text_rows(&c), vec![".│.", "─┼─", ".│."]);
        let mut writes = detach(&c, p(1, 1));
        writes.push(CellWrite {
            x: 1,
            y: 1,
            cell: Cell::EMPTY,
        });
        apply(&mut c, writes);
        assert_eq!(text_rows(&c), vec![".│.", "─.─", ".│."]);

        let mut t = Canvas::new(Size::new(3, 2)).unwrap();
        join(
            &mut t,
            &[],
            &[(p(0, 0), p(1, 0)), (p(1, 0), p(2, 0)), (p(1, 0), p(1, 1))],
            LIGHT,
        );
        assert_eq!(text_rows(&t)[0], "─┬─");
        let writes = detach(&t, p(1, 1));
        apply(&mut t, writes);
        assert_eq!(text_rows(&t)[0], "───");
    }

    #[test]
    fn patterns_mask_shade_and_blend() {
        let mut c = Canvas::new(Size::new(4, 2)).unwrap();
        let all: Vec<Position> = Rect::new(0, 0, 4, 2).positions().collect();
        let hash = Grapheme::new("#").unwrap();
        let checker = pattern_writes(&c, &all, Pattern::Checker, &hash, CellStyle::DEFAULT, None);
        apply(&mut c, checker);
        assert_eq!(text_rows(&c), vec!["#.#.", ".#.#"]);

        let cell = [p(1, 0)];
        let mut seen = Vec::new();
        for _ in 0..5 {
            let w = pattern_writes(&c, &cell, Pattern::Shade, &hash, CellStyle::DEFAULT, None);
            apply(&mut c, w);
            seen.push(text_rows(&c)[0].chars().nth(1).unwrap());
        }
        assert_eq!(seen.into_iter().collect::<String>(), "░▒▓██");

        let style = CellStyle::new(Color::Rgb(0, 0, 0), Color::Rgb(100, 100, 100));
        let g = pattern_writes(
            &c,
            &[p(0, 1), p(3, 1)],
            Pattern::Gradient,
            &hash,
            style,
            None,
        );
        assert_eq!(g[0].cell.style.fg, Color::Rgb(0, 0, 0));
        assert_eq!(g[1].cell.style.fg, Color::Rgb(100, 100, 100));
        assert_eq!(g[1].cell.style.bg, Color::Default);
    }
}
