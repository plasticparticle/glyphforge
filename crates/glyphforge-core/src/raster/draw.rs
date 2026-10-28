//! Pixel drawing geometry for raster tools: brush footprints, strokes,
//! rectangles and flood fills, producing pixel writes.

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::{Pixel, Raster};
use crate::document::{Position, Rect};
use crate::paint;

/// One pixel write inside a `set_pixels` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelWrite {
    pub x: u16,
    pub y: u16,
    pub pixel: Pixel,
}

/// The pixels a square brush of `size` covers when centred on `p`.
pub fn footprint(p: Position, size: u8) -> Vec<Position> {
    let size = u16::from(size.max(1));
    let (x0, y0) = (
        p.x.saturating_sub((size - 1) / 2),
        p.y.saturating_sub((size - 1) / 2),
    );
    Rect::new(x0, y0, size, size).positions().collect()
}

/// A stroke from `a` to `b` with a brush of `size`.
pub fn stroke(a: Position, b: Position, size: u8) -> Vec<Position> {
    let mut seen = HashSet::new();
    paint::line(a, b)
        .into_iter()
        .flat_map(|p| footprint(p, size))
        .filter(|p| seen.insert(*p))
        .collect()
}

/// The outline of the rectangle between two corners, `size` pixels thick.
pub fn rect_outline(a: Position, b: Position, size: u8) -> Vec<Position> {
    let r = paint::rect_between(a, b);
    let t = u16::from(size.max(1));
    r.positions()
        .filter(|p| {
            p.x < r.x.saturating_add(t)
                || p.y < r.y.saturating_add(t)
                || u32::from(p.x) + u32::from(t) >= r.right()
                || u32::from(p.y) + u32::from(t) >= r.bottom()
        })
        .collect()
}

/// The 4-connected region of pixels equal to the one at `seed`.
pub fn flood(raster: &Raster, seed: Position) -> Vec<Position> {
    let Some(target) = raster.get(seed.x, seed.y) else {
        return Vec::new();
    };
    let mut seen = HashSet::from([seed]);
    let mut queue = VecDeque::from([seed]);
    let mut out = Vec::new();
    while let Some(p) = queue.pop_front() {
        out.push(p);
        let next = [
            p.x.checked_sub(1).map(|x| Position::new(x, p.y)),
            p.y.checked_sub(1).map(|y| Position::new(p.x, y)),
            Some(Position::new(p.x.saturating_add(1), p.y)),
            Some(Position::new(p.x, p.y.saturating_add(1))),
        ];
        for n in next.into_iter().flatten() {
            if raster.get(n.x, n.y) == Some(target) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    out.sort_by_key(|p| (p.y, p.x));
    out
}

/// Writes of `pixel` at every position inside `raster`.
pub fn writes(raster: &Raster, positions: &[Position], pixel: Pixel) -> Vec<PixelWrite> {
    positions
        .iter()
        .filter(|p| p.x < raster.width() && p.y < raster.height())
        .map(|p| PixelWrite {
            x: p.x,
            y: p.y,
            pixel,
        })
        .collect()
}

/// Writes of individual pixels, clipped to `raster`.
pub fn writes_each(raster: &Raster, pixels: &[(Position, Pixel)]) -> Vec<PixelWrite> {
    pixels
        .iter()
        .filter(|(p, _)| p.x < raster.width() && p.y < raster.height())
        .map(|(p, pixel)| PixelWrite {
            x: p.x,
            y: p.y,
            pixel: *pixel,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;

    #[test]
    fn brushes_and_strokes() {
        assert_eq!(footprint(Position::new(5, 5), 1), vec![Position::new(5, 5)]);
        assert_eq!(footprint(Position::new(5, 5), 3).len(), 9);
        assert_eq!(
            footprint(Position::new(0, 0), 3).len(),
            9,
            "clamped at the edge"
        );
        let s = stroke(Position::new(0, 0), Position::new(4, 0), 2);
        assert_eq!(
            s.len(),
            12,
            "5 columns plus the brush width, 2 rows, no duplicates"
        );
        assert_eq!(
            rect_outline(Position::new(0, 0), Position::new(3, 3), 1).len(),
            12
        );
        assert_eq!(
            rect_outline(Position::new(0, 0), Position::new(5, 5), 2).len(),
            32
        );
    }

    #[test]
    fn flood_and_clipping() {
        let mut r = Raster::new(4, 2);
        r.set(2, 0, Pixel::Solid(Color::Indexed(1)));
        r.set(2, 1, Pixel::Solid(Color::Indexed(1)));
        assert_eq!(flood(&r, Position::new(0, 0)).len(), 4);
        assert_eq!(flood(&r, Position::new(2, 0)).len(), 2);
        let w = writes(
            &r,
            &[Position::new(3, 1), Position::new(9, 9)],
            Pixel::Clear,
        );
        assert_eq!(w.len(), 1);
    }
}
