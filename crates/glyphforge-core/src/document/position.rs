use serde::{Deserialize, Serialize};

/// A cell coordinate. `x` is the column, `y` the row, both zero based.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

impl Position {
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// A size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    pub const fn area(self) -> usize {
        self.width as usize * self.height as usize
    }

    pub const fn contains(self, pos: Position) -> bool {
        pos.x < self.width && pos.y < self.height
    }
}

/// An axis-aligned rectangle of cells. `width` or `height` may be zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn from_size(size: Size) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Exclusive right edge.
    pub const fn right(self) -> u32 {
        self.x as u32 + self.width as u32
    }

    /// Exclusive bottom edge.
    pub const fn bottom(self) -> u32 {
        self.y as u32 + self.height as u32
    }

    pub const fn contains(self, pos: Position) -> bool {
        pos.x >= self.x
            && pos.y >= self.y
            && (pos.x as u32) < self.right()
            && (pos.y as u32) < self.bottom()
    }

    /// The overlap of two rectangles, or an empty rect at the origin when
    /// they do not overlap.
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());
        if u32::from(x1) >= x2 || u32::from(y1) >= y2 {
            return Self::new(0, 0, 0, 0);
        }
        Self::new(
            x1,
            y1,
            (x2 - u32::from(x1)) as u16,
            (y2 - u32::from(y1)) as u16,
        )
    }

    /// Iterates all positions inside the rectangle in row-major order.
    pub fn positions(self) -> impl Iterator<Item = Position> {
        (self.y..self.y.saturating_add(self.height)).flat_map(move |y| {
            (self.x..self.x.saturating_add(self.width)).map(move |x| Position::new(x, y))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_of_overlapping_rects() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(b), Rect::new(5, 5, 5, 5));
    }

    #[test]
    fn intersection_of_disjoint_rects_is_empty() {
        let a = Rect::new(0, 0, 3, 3);
        let b = Rect::new(3, 0, 3, 3);
        assert!(a.intersection(b).is_empty());
    }

    #[test]
    fn positions_are_row_major() {
        let r = Rect::new(1, 1, 2, 2);
        let v: Vec<_> = r.positions().collect();
        assert_eq!(
            v,
            vec![
                Position::new(1, 1),
                Position::new(2, 1),
                Position::new(1, 2),
                Position::new(2, 2)
            ]
        );
    }

    #[test]
    fn contains_is_exclusive_at_edges() {
        let r = Rect::new(2, 2, 2, 2);
        assert!(r.contains(Position::new(3, 3)));
        assert!(!r.contains(Position::new(4, 3)));
        assert!(!r.contains(Position::new(1, 2)));
    }
}
