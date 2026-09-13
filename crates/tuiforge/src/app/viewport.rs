//! Viewport math: which part of the document is visible.

use tuiforge_core::{Position, Rect, Size};

/// The visible window onto the document, in document coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Viewport {
    pub offset: Position,
    /// Size of the on-screen canvas area, updated by the UI on every frame.
    pub size: Size,
}

impl Viewport {
    pub const fn rect(self) -> Rect {
        Rect::new(
            self.offset.x,
            self.offset.y,
            self.size.width,
            self.size.height,
        )
    }

    /// Keeps the offset inside the document: never scroll past the end,
    /// never scroll at all when the document fits.
    pub fn clamp(&mut self, doc: Size) {
        let max_x = doc.width.saturating_sub(self.size.width);
        let max_y = doc.height.saturating_sub(self.size.height);
        self.offset.x = self.offset.x.min(max_x);
        self.offset.y = self.offset.y.min(max_y);
    }

    /// Scrolls the least amount needed so `pos` is visible.
    pub fn ensure_visible(&mut self, pos: Position, doc: Size) {
        if self.size.width == 0 || self.size.height == 0 {
            return;
        }
        if pos.x < self.offset.x {
            self.offset.x = pos.x;
        } else if pos.x >= self.offset.x + self.size.width {
            self.offset.x = pos.x - self.size.width + 1;
        }
        if pos.y < self.offset.y {
            self.offset.y = pos.y;
        } else if pos.y >= self.offset.y + self.size.height {
            self.offset.y = pos.y - self.size.height + 1;
        }
        self.clamp(doc);
    }

    pub fn scroll_by(&mut self, dx: i32, dy: i32, doc: Size) {
        let x = i32::from(self.offset.x) + dx;
        let y = i32::from(self.offset.y) + dy;
        self.offset.x = x.clamp(0, i32::from(u16::MAX)) as u16;
        self.offset.y = y.clamp(0, i32::from(u16::MAX)) as u16;
        self.clamp(doc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(x: u16, y: u16, w: u16, h: u16) -> Viewport {
        Viewport {
            offset: Position::new(x, y),
            size: Size::new(w, h),
        }
    }

    #[test]
    fn no_scrolling_when_document_fits() {
        let mut v = vp(5, 5, 100, 50);
        v.clamp(Size::new(80, 24));
        assert_eq!(v.offset, Position::ORIGIN);
    }

    #[test]
    fn ensure_visible_scrolls_minimally() {
        let doc = Size::new(200, 100);
        let mut v = vp(0, 0, 10, 5);
        v.ensure_visible(Position::new(12, 7), doc);
        assert_eq!(v.offset, Position::new(3, 3));
        v.ensure_visible(Position::new(1, 1), doc);
        assert_eq!(v.offset, Position::new(1, 1));
        v.ensure_visible(Position::new(199, 99), doc);
        assert_eq!(v.offset, Position::new(190, 95));
    }

    #[test]
    fn scroll_by_clamps_to_document() {
        let doc = Size::new(30, 30);
        let mut v = vp(0, 0, 10, 10);
        v.scroll_by(-5, 100, doc);
        assert_eq!(v.offset, Position::new(0, 20));
        v.scroll_by(25, 0, doc);
        assert_eq!(v.offset, Position::new(20, 20));
    }

    #[test]
    fn zero_sized_viewport_is_inert() {
        let mut v = vp(0, 0, 0, 0);
        v.ensure_visible(Position::new(50, 50), Size::new(100, 100));
        assert_eq!(v.offset, Position::ORIGIN);
    }
}
