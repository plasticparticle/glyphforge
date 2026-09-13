use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::canvas_repr::CanvasRepr;
use super::{Cell, CellContent, CellStyle, Grapheme, Position, Rect, Size};

/// Errors from canvas mutation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanvasError {
    #[error("position ({}, {}) is outside the {}x{} canvas", pos.x, pos.y, size.width, size.height)]
    OutOfBounds { pos: Position, size: Size },
    #[error("a double-width glyph cannot be placed in the last column (x = {x})")]
    WideGlyphAtEdge { x: u16 },
    #[error("canvas dimensions must be at least 1x1")]
    ZeroSize,
}

/// One cell changed by a canvas operation: where, what was there before and
/// what is there now. This is the delta the history system stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellChange {
    pub pos: Position,
    pub before: Cell,
    pub after: Cell,
}

/// A dense, fixed-size grid of cells.
///
/// The canvas maintains the double-width glyph invariant: a width-2 glyph at
/// `(x, y)` always has a [`CellContent::WideTail`] at `(x + 1, y)`, and a
/// tail always has a wide head to its left. All mutation goes through
/// [`Canvas::put`], which repairs the neighbours of every write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    size: Size,
    cells: Vec<Cell>,
}

impl Canvas {
    /// Creates a canvas filled with transparent cells.
    pub fn new(size: Size) -> Result<Self, CanvasError> {
        if size.width == 0 || size.height == 0 {
            return Err(CanvasError::ZeroSize);
        }
        Ok(Self {
            size,
            cells: vec![Cell::EMPTY; size.area()],
        })
    }

    pub const fn size(&self) -> Size {
        self.size
    }

    pub const fn width(&self) -> u16 {
        self.size.width
    }

    pub const fn height(&self) -> u16 {
        self.size.height
    }

    pub const fn bounds(&self) -> Rect {
        Rect::from_size(self.size)
    }

    fn index(&self, pos: Position) -> Option<usize> {
        self.size
            .contains(pos)
            .then(|| pos.y as usize * self.size.width as usize + pos.x as usize)
    }

    /// Returns the cell at `pos`, or `None` outside the canvas.
    pub fn get(&self, pos: Position) -> Option<&Cell> {
        self.index(pos).map(|i| &self.cells[i])
    }

    /// Row-major access to all cells.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Cells of one row, or `None` if `y` is outside the canvas.
    pub fn row(&self, y: u16) -> Option<&[Cell]> {
        if y >= self.size.height {
            return None;
        }
        let w = self.size.width as usize;
        let start = y as usize * w;
        Some(&self.cells[start..start + w])
    }

    /// Writes `cell` at `pos`, maintaining the wide-glyph invariant, and
    /// returns every cell that changed (including neighbours).
    ///
    /// Writing a [`CellContent::WideTail`] directly is treated as writing a
    /// blank glyph of the same style: tails are owned by the canvas.
    pub fn put(&mut self, pos: Position, cell: Cell) -> Result<Vec<CellChange>, CanvasError> {
        let idx = self.index(pos).ok_or(CanvasError::OutOfBounds {
            pos,
            size: self.size,
        })?;
        let cell = if cell.is_wide_tail() {
            Cell::blank(cell.style)
        } else {
            cell
        };
        if cell.is_wide_head() && pos.x + 1 >= self.size.width {
            return Err(CanvasError::WideGlyphAtEdge { x: pos.x });
        }

        let mut changes = Vec::with_capacity(3);

        // Detach the halves of any wide glyph we are about to overwrite.
        self.detach_wide(pos, idx, &mut changes);
        if cell.is_wide_head() {
            let tail_pos = Position::new(pos.x + 1, pos.y);
            let tail_idx = idx + 1;
            self.detach_wide(tail_pos, tail_idx, &mut changes);
            self.replace(
                tail_idx,
                tail_pos,
                Cell::wide_tail(cell.style),
                &mut changes,
            );
        }
        self.replace(idx, pos, cell, &mut changes);
        Ok(changes)
    }

    /// Convenience: writes a glyph with a style.
    pub fn put_glyph(
        &mut self,
        pos: Position,
        grapheme: Grapheme,
        style: CellStyle,
    ) -> Result<Vec<CellChange>, CanvasError> {
        self.put(pos, Cell::glyph(grapheme, style))
    }

    /// Makes the cell transparent, detaching a wide partner if needed.
    pub fn clear(&mut self, pos: Position) -> Result<Vec<CellChange>, CanvasError> {
        self.put(pos, Cell::EMPTY)
    }

    /// Restores a previous cell value without invariant repair. Used by
    /// history to revert deltas that were recorded with the invariant
    /// already satisfied. Out-of-range positions are ignored.
    pub fn restore(&mut self, pos: Position, cell: Cell) {
        if let Some(i) = self.index(pos) {
            self.cells[i] = cell;
        }
    }

    /// Fills the whole canvas with transparent cells.
    pub fn clear_all(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = Cell::EMPTY);
    }

    /// Changes only the style of an opaque cell; transparent cells and
    /// out-of-range positions are ignored.
    pub fn set_style(&mut self, pos: Position, style: CellStyle) {
        if let Some(i) = self.index(pos) {
            if !self.cells[i].is_empty() {
                self.cells[i].style = style;
            }
        }
    }

    /// Composites `top` over this canvas: opaque cells of `top` win. Both
    /// canvases must have the same size; a size mismatch composites the
    /// overlapping region only. Afterwards half-covered wide glyphs are
    /// repaired so the invariant holds.
    pub fn composite_over(&mut self, top: &Canvas) {
        for pos in self.bounds().intersection(top.bounds()).positions() {
            let Some(cell) = top.get(pos) else { continue };
            if !cell.is_empty() {
                if let Some(i) = self.index(pos) {
                    self.cells[i] = cell.clone();
                }
            }
        }
        self.repair_wide_glyphs();
    }

    /// Repairs wide glyphs whose other half is missing: an orphaned head
    /// or tail becomes a blank cell with its own style.
    pub fn repair_wide_glyphs(&mut self) {
        let w = self.size.width as usize;
        for y in 0..self.size.height as usize {
            let row = &mut self.cells[y * w..(y + 1) * w];
            for x in 0..w {
                let orphan = match &row[x].content {
                    CellContent::Glyph(g) if g.is_wide() => {
                        !row.get(x + 1).is_some_and(Cell::is_wide_tail)
                    }
                    CellContent::WideTail => x == 0 || !row[x - 1].is_wide_head(),
                    _ => false,
                };
                if orphan {
                    row[x] = Cell::blank(row[x].style);
                }
            }
        }
    }

    /// Returns a new canvas of `size` with this canvas's content copied to
    /// the top-left corner. A wide glyph that would straddle the new right
    /// edge is dropped.
    pub fn resized(&self, size: Size) -> Result<Self, CanvasError> {
        let mut out = Self::new(size)?;
        for pos in self.bounds().intersection(out.bounds()).positions() {
            let cell = &self.cells[self.index(pos).unwrap_or_default()];
            if cell.is_wide_tail() {
                continue; // written together with its head
            }
            if cell.is_wide_head() && pos.x + 1 >= size.width {
                continue;
            }
            // Cannot fail: position is inside and the edge case was excluded.
            let _ = out.put(pos, cell.clone());
        }
        Ok(out)
    }

    /// If the cell at `idx` is half of a wide glyph, blanks the *other*
    /// half so no orphaned head or tail remains.
    fn detach_wide(&mut self, pos: Position, idx: usize, changes: &mut Vec<CellChange>) {
        let current = &self.cells[idx];
        if current.is_wide_head() {
            let tail_idx = idx + 1;
            let tail_pos = Position::new(pos.x + 1, pos.y);
            if pos.x + 1 < self.size.width && self.cells[tail_idx].is_wide_tail() {
                let style = self.cells[tail_idx].style;
                self.replace(tail_idx, tail_pos, Cell::blank(style), changes);
            }
        } else if current.is_wide_tail() && pos.x > 0 {
            let head_idx = idx - 1;
            let head_pos = Position::new(pos.x - 1, pos.y);
            if self.cells[head_idx].is_wide_head() {
                let style = self.cells[head_idx].style;
                self.replace(head_idx, head_pos, Cell::blank(style), changes);
            }
        }
    }

    fn replace(&mut self, idx: usize, pos: Position, cell: Cell, changes: &mut Vec<CellChange>) {
        if self.cells[idx] == cell {
            return;
        }
        let before = std::mem::replace(&mut self.cells[idx], cell.clone());
        changes.push(CellChange {
            pos,
            before,
            after: cell,
        });
    }

    /// Checks the wide-glyph invariant over the whole canvas. Intended for
    /// tests and debug assertions.
    pub fn invariant_holds(&self) -> bool {
        for y in 0..self.size.height {
            let Some(row) = self.row(y) else { return false };
            for (x, cell) in row.iter().enumerate() {
                match &cell.content {
                    CellContent::Glyph(g) if g.is_wide() => {
                        if !row.get(x + 1).is_some_and(Cell::is_wide_tail) {
                            return false;
                        }
                    }
                    CellContent::WideTail => {
                        if x == 0 || !row[x - 1].is_wide_head() {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }
        true
    }
}

impl Serialize for Canvas {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CanvasRepr::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Canvas {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = CanvasRepr::deserialize(deserializer)?;
        Canvas::try_from(repr).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(s: &str) -> Grapheme {
        Grapheme::new(s).unwrap()
    }

    fn cell(s: &str) -> Cell {
        Cell::glyph(g(s), CellStyle::DEFAULT)
    }

    fn content_row(c: &Canvas, y: u16) -> String {
        c.row(y)
            .unwrap()
            .iter()
            .map(|cell| match &cell.content {
                CellContent::Empty => ".".to_owned(),
                CellContent::Glyph(g) => g.as_str().to_owned(),
                CellContent::WideTail => "<".to_owned(),
            })
            .collect()
    }

    #[test]
    fn zero_size_is_rejected() {
        assert_eq!(
            Canvas::new(Size::new(0, 5)).unwrap_err(),
            CanvasError::ZeroSize
        );
    }

    #[test]
    fn put_and_get_narrow() {
        let mut c = Canvas::new(Size::new(4, 2)).unwrap();
        let changes = c.put(Position::new(1, 1), cell("a")).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].before, Cell::EMPTY);
        assert_eq!(c.get(Position::new(1, 1)), Some(&cell("a")));
        assert_eq!(content_row(&c, 1), ".a..");
        assert!(c.invariant_holds());
    }

    #[test]
    fn put_out_of_bounds_fails() {
        let mut c = Canvas::new(Size::new(4, 2)).unwrap();
        assert!(matches!(
            c.put(Position::new(4, 0), cell("a")),
            Err(CanvasError::OutOfBounds { .. })
        ));
        assert_eq!(c.get(Position::new(0, 2)), None);
    }

    #[test]
    fn wide_glyph_creates_tail() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        let changes = c.put(Position::new(1, 0), cell("漢")).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(content_row(&c, 0), ".漢<.");
        assert!(c.get(Position::new(2, 0)).unwrap().is_wide_tail());
        assert!(c.invariant_holds());
    }

    #[test]
    fn wide_glyph_at_last_column_is_rejected() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        assert_eq!(
            c.put(Position::new(3, 0), cell("漢")),
            Err(CanvasError::WideGlyphAtEdge { x: 3 })
        );
        assert_eq!(content_row(&c, 0), "....");
    }

    #[test]
    fn overwriting_head_blanks_tail() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(0, 0), cell("漢")).unwrap();
        let changes = c.put(Position::new(0, 0), cell("a")).unwrap();
        assert_eq!(content_row(&c, 0), "a ..");
        assert_eq!(changes.len(), 2);
        assert!(c.invariant_holds());
    }

    #[test]
    fn overwriting_tail_blanks_head() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(0, 0), cell("漢")).unwrap();
        c.put(Position::new(1, 0), cell("b")).unwrap();
        assert_eq!(content_row(&c, 0), " b..");
        assert!(c.invariant_holds());
    }

    #[test]
    fn clearing_tail_blanks_head_and_clears_tail() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(1, 0), cell("漢")).unwrap();
        c.clear(Position::new(2, 0)).unwrap();
        assert_eq!(content_row(&c, 0), ". ..");
        assert!(c.invariant_holds());
    }

    #[test]
    fn wide_over_wide_offset_by_one() {
        // 漢 at 0 (tail at 1), then 字 at 1 (tail at 2): 漢 loses its tail and
        // becomes blank; 字 owns 1..2.
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(0, 0), cell("漢")).unwrap();
        c.put(Position::new(1, 0), cell("字")).unwrap();
        assert_eq!(content_row(&c, 0), " 字<.");
        assert!(c.invariant_holds());
    }

    #[test]
    fn wide_whose_tail_hits_another_wide_head() {
        // 字 at 2 (tail 3); put 漢 at 1: its tail at 2 overwrites 字's head,
        // so 字's tail at 3 must become blank.
        let mut c = Canvas::new(Size::new(5, 1)).unwrap();
        c.put(Position::new(2, 0), cell("字")).unwrap();
        c.put(Position::new(1, 0), cell("漢")).unwrap();
        assert_eq!(content_row(&c, 0), ".漢< .");
        assert!(c.invariant_holds());
    }

    #[test]
    fn writing_a_tail_directly_becomes_blank() {
        let mut c = Canvas::new(Size::new(3, 1)).unwrap();
        c.put(Position::new(0, 0), Cell::wide_tail(CellStyle::DEFAULT))
            .unwrap();
        assert_eq!(content_row(&c, 0), " ..");
        assert!(c.invariant_holds());
    }

    #[test]
    fn putting_identical_cell_yields_no_change() {
        let mut c = Canvas::new(Size::new(3, 1)).unwrap();
        c.put(Position::new(0, 0), cell("x")).unwrap();
        assert!(c.put(Position::new(0, 0), cell("x")).unwrap().is_empty());
    }

    #[test]
    fn changes_can_be_reverted_with_restore() {
        let mut c = Canvas::new(Size::new(4, 1)).unwrap();
        c.put(Position::new(0, 0), cell("漢")).unwrap();
        let snapshot = c.clone();
        let changes = c.put(Position::new(1, 0), cell("b")).unwrap();
        for ch in changes.iter().rev() {
            c.restore(ch.pos, ch.before.clone());
        }
        assert_eq!(c, snapshot);
    }

    #[test]
    fn resize_drops_straddling_wide_glyph() {
        let mut c = Canvas::new(Size::new(4, 2)).unwrap();
        c.put(Position::new(0, 0), cell("a")).unwrap();
        c.put(Position::new(2, 0), cell("漢")).unwrap();
        c.put(Position::new(0, 1), cell("b")).unwrap();
        let r = c.resized(Size::new(3, 1)).unwrap();
        assert_eq!(content_row(&r, 0), "a..");
        assert!(r.invariant_holds());
        let bigger = c.resized(Size::new(6, 3)).unwrap();
        assert_eq!(content_row(&bigger, 0), "a.漢<..");
        assert_eq!(content_row(&bigger, 1), "b.....");
        assert_eq!(content_row(&bigger, 2), "......");
    }

    #[test]
    fn serde_round_trip() {
        let mut c = Canvas::new(Size::new(3, 1)).unwrap();
        c.put(Position::new(0, 0), cell("漢")).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        let back: Canvas = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }
}
