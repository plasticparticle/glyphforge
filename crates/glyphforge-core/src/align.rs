//! Alignment geometry: align, distribute, equal size and snapping.
//!
//! Pure functions over screen rectangles. They return new rectangles (or
//! offsets); the editor turns them into `move`/`resize` operations, so the
//! same geometry is available headlessly.

use crate::document::Rect;

/// An edge or centre line to align on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    Left,
    /// Horizontal centre (centres share an x).
    Center,
    Right,
    Top,
    /// Vertical centre (middles share a y).
    Middle,
    Bottom,
}

impl Edge {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// The smallest rect containing all of `rects`.
pub fn bounds(rects: &[Rect]) -> Option<Rect> {
    let first = rects.first()?;
    let (mut x0, mut y0) = (u32::from(first.x), u32::from(first.y));
    let (mut x1, mut y1) = (first.right(), first.bottom());
    for r in &rects[1..] {
        x0 = x0.min(u32::from(r.x));
        y0 = y0.min(u32::from(r.y));
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    }
    Some(Rect::new(
        x0 as u16,
        y0 as u16,
        (x1 - x0) as u16,
        (y1 - y0) as u16,
    ))
}

/// Aligns every rect on `edge` of the selection's bounds. Sizes never
/// change; a centre that falls between cells rounds towards the start.
pub fn align(rects: &[Rect], edge: Edge) -> Vec<Rect> {
    let Some(b) = bounds(rects) else {
        return Vec::new();
    };
    rects
        .iter()
        .map(|r| {
            let mut out = *r;
            match edge {
                Edge::Left => out.x = b.x,
                Edge::Right => out.x = b.right().saturating_sub(u32::from(r.width)) as u16,
                Edge::Center => out.x = b.x + (b.width - r.width.min(b.width)) / 2,
                Edge::Top => out.y = b.y,
                Edge::Bottom => out.y = b.bottom().saturating_sub(u32::from(r.height)) as u16,
                Edge::Middle => out.y = b.y + (b.height - r.height.min(b.height)) / 2,
            }
            out
        })
        .collect()
}

/// Spaces rects evenly along `axis`: the first and last (by position) stay
/// put and the gaps between neighbours become equal; leftover cells go to
/// the first gaps. Needs three rects to change anything.
pub fn distribute(rects: &[Rect], axis: Axis) -> Vec<Rect> {
    let mut out = rects.to_vec();
    if rects.len() < 3 {
        return out;
    }
    let start = |r: &Rect| match axis {
        Axis::Horizontal => i64::from(r.x),
        Axis::Vertical => i64::from(r.y),
    };
    let size = |r: &Rect| match axis {
        Axis::Horizontal => i64::from(r.width),
        Axis::Vertical => i64::from(r.height),
    };
    let mut order: Vec<usize> = (0..rects.len()).collect();
    order.sort_by_key(|&i| (start(&rects[i]), i));
    let first = &rects[order[0]];
    let last = &rects[order[order.len() - 1]];
    let span = start(last) + size(last) - start(first);
    let total: i64 = order.iter().map(|&i| size(&rects[i])).sum();
    let gaps = i64::try_from(order.len() - 1).unwrap_or(i64::MAX);
    let free = span - total;
    let (gap, mut extra) = (free.div_euclid(gaps), free.rem_euclid(gaps));
    let mut cursor = start(first);
    for &i in &order {
        let pos = cursor.max(0) as u16;
        match axis {
            Axis::Horizontal => out[i].x = pos,
            Axis::Vertical => out[i].y = pos,
        }
        cursor += size(&rects[i]) + gap + i64::from(extra > 0);
        extra = (extra - 1).max(0);
    }
    out
}

/// Gives every rect the width and/or height of `reference`.
pub fn equal_size(rects: &[Rect], reference: Rect, width: bool, height: bool) -> Vec<Rect> {
    rects
        .iter()
        .map(|r| {
            let mut out = *r;
            if width {
                out.width = reference.width;
            }
            if height {
                out.height = reference.height;
            }
            out
        })
        .collect()
}

/// A guide line shown while dragging: a column or a row, and the extent
/// it spans between the rects that line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
    /// Vertical line at column `x` from row `from` to row `to` (inclusive).
    Column { x: u16, from: u16, to: u16 },
    /// Horizontal line at row `y` from column `from` to column `to`.
    Row { y: u16, from: u16, to: u16 },
}

/// Snapping settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snap {
    /// Snap to the edges and centres of other components.
    pub neighbours: bool,
    /// Grid spacing in cells; 0 or 1 disables the grid.
    pub grid: u16,
    /// How close (in cells) an edge must be to snap to a neighbour.
    pub threshold: u16,
}

impl Default for Snap {
    fn default() -> Self {
        Self {
            neighbours: true,
            grid: 0,
            threshold: 1,
        }
    }
}

/// The three lines of a rect along one axis: start, centre, end (the end
/// is the last cell, so equal ends line up exactly).
fn lines(start: u16, size: u16) -> [i32; 3] {
    let s = i32::from(start);
    let len = i32::from(size.max(1));
    [s, s + (len - 1) / 2, s + len - 1]
}

/// Snaps a moving rect: to the closest edge or centre of `others` within
/// the threshold, otherwise to the grid. Returns the snapped rect and the
/// guides that explain it.
pub fn snap(moving: Rect, others: &[Rect], snap: Snap) -> (Rect, Vec<Guide>) {
    let mut out = moving;
    let mut guides = Vec::new();
    let mut matched = Vec::new();
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let (start, size) = match axis {
            Axis::Horizontal => (moving.x, moving.width),
            Axis::Vertical => (moving.y, moving.height),
        };
        let mine = lines(start, size);
        // (distance, delta, line, other index)
        let mut best: Option<(i32, i32, i32, usize)> = None;
        if snap.neighbours {
            for (k, o) in others.iter().enumerate() {
                let theirs = match axis {
                    Axis::Horizontal => lines(o.x, o.width),
                    Axis::Vertical => lines(o.y, o.height),
                };
                for m in mine {
                    for t in theirs {
                        let d = t - m;
                        if d.abs() <= i32::from(snap.threshold)
                            && best.is_none_or(|b| d.abs() < b.0)
                        {
                            best = Some((d.abs(), d, t, k));
                        }
                    }
                }
            }
        }
        let delta = match best {
            Some((_, d, _, _)) => d,
            None if snap.grid > 1 => {
                let g = i32::from(snap.grid);
                let s = i32::from(start);
                let down = s - s.rem_euclid(g);
                (if s - down <= g / 2 { down } else { down + g }) - s
            }
            None => 0,
        };
        let pos = (i32::from(start) + delta).max(0) as u16;
        match axis {
            Axis::Horizontal => out.x = pos,
            Axis::Vertical => out.y = pos,
        }
        if let Some((_, _, line, k)) = best {
            matched.push((axis, line.max(0) as u16, k));
        }
    }
    for (axis, line, k) in matched {
        let o = others[k];
        guides.push(match axis {
            Axis::Horizontal => Guide::Column {
                x: line,
                from: o.y.min(out.y),
                to: ((o.bottom().max(out.bottom())).saturating_sub(1)) as u16,
            },
            Axis::Vertical => Guide::Row {
                y: line,
                from: o.x.min(out.x),
                to: ((o.right().max(out.right())).saturating_sub(1)) as u16,
            },
        });
    }
    (out, guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn align_edges_and_centres_on_bounds() {
        let rects = [r(2, 1, 4, 2), r(10, 5, 8, 3)];
        assert_eq!(
            align(&rects, Edge::Left),
            vec![r(2, 1, 4, 2), r(2, 5, 8, 3)]
        );
        assert_eq!(
            align(&rects, Edge::Right),
            vec![r(14, 1, 4, 2), r(10, 5, 8, 3)]
        );
        // Bounds 2..18 (16 wide): a 4-wide rect centres at 8, an 8-wide at 6.
        assert_eq!(
            align(&rects, Edge::Center),
            vec![r(8, 1, 4, 2), r(6, 5, 8, 3)]
        );
        assert_eq!(
            align(&rects, Edge::Bottom),
            vec![r(2, 6, 4, 2), r(10, 5, 8, 3)]
        );
        assert_eq!(align(&rects, Edge::Middle)[0].y, 3);
    }

    #[test]
    fn distribute_equalises_gaps_and_keeps_the_ends() {
        let rects = [r(0, 0, 4, 1), r(20, 0, 4, 1), r(5, 0, 2, 1)];
        // Span 0..24, sizes 10, free 14 in two gaps of 7.
        let out = distribute(&rects, Axis::Horizontal);
        assert_eq!(out[0].x, 0);
        assert_eq!(out[2].x, 11);
        assert_eq!(out[1].x, 20);
        assert_eq!(
            distribute(&rects[..2], Axis::Horizontal),
            rects[..2].to_vec()
        );
    }

    #[test]
    fn equal_size_copies_the_reference() {
        let out = equal_size(&[r(0, 0, 3, 3), r(5, 5, 9, 1)], r(0, 0, 6, 2), true, false);
        assert_eq!(out, vec![r(0, 0, 6, 3), r(5, 5, 6, 1)]);
    }

    #[test]
    fn snap_prefers_neighbour_edges_then_grid() {
        let other = r(10, 10, 10, 4);
        let s = Snap {
            neighbours: true,
            grid: 4,
            threshold: 1,
        };
        // Left edge 11 is one cell from the neighbour's 10: snaps to it.
        let (out, guides) = snap(r(11, 3, 5, 2), &[other], s);
        assert_eq!(out.x, 10);
        // y 3 has no neighbour line within 1: the grid of 4 rounds it to 4.
        assert_eq!(out.y, 4);
        assert_eq!(
            guides,
            vec![Guide::Column {
                x: 10,
                from: 4,
                to: 13
            }]
        );
        // Already on a line (its last row is the neighbour's top row): stays.
        let (out, guides) = snap(r(30, 9, 3, 2), &[other], s);
        assert_eq!(out.y, 9);
        assert!(matches!(guides[0], Guide::Row { y: 10, .. }));
        // Snapping off: unchanged.
        let off = Snap {
            neighbours: false,
            grid: 0,
            threshold: 1,
        };
        assert_eq!(snap(r(11, 2, 5, 2), &[other], off).0, r(11, 2, 5, 2));
    }

    #[test]
    fn bounds_of_nothing_is_none() {
        assert_eq!(bounds(&[]), None);
        assert_eq!(bounds(&[r(1, 2, 3, 4), r(0, 0, 1, 1)]), Some(r(0, 0, 4, 6)));
    }
}
