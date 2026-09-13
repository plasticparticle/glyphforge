//! Compositing of visible layers into a single cell buffer.

use crate::document::{Cell, CellContent, Document, Position, Rect, Size};

/// A flat, row-major buffer of composited cells. It satisfies the same
/// wide-glyph invariant as a [`Canvas`](crate::Canvas): every wide head is
/// followed by a tail and every tail follows a head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellBuffer {
    size: Size,
    cells: Vec<Cell>,
}

impl CellBuffer {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            cells: vec![Cell::EMPTY; size.area()],
        }
    }

    pub const fn size(&self) -> Size {
        self.size
    }

    pub const fn bounds(&self) -> Rect {
        Rect::from_size(self.size)
    }

    pub fn get(&self, pos: Position) -> Option<&Cell> {
        self.size
            .contains(pos)
            .then(|| &self.cells[self.index(pos)])
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    pub fn row(&self, y: u16) -> Option<&[Cell]> {
        if y >= self.size.height {
            return None;
        }
        let w = self.size.width as usize;
        let start = y as usize * w;
        Some(&self.cells[start..start + w])
    }

    fn index(&self, pos: Position) -> usize {
        pos.y as usize * self.size.width as usize + pos.x as usize
    }

    fn set(&mut self, pos: Position, cell: Cell) {
        let i = self.index(pos);
        self.cells[i] = cell;
    }

    /// Repairs wide glyphs whose other half was covered by a higher layer:
    /// an orphaned head or tail becomes a blank cell with its own style.
    fn repair_wide_glyphs(&mut self) {
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
}

/// Composites all visible layers of `doc`, top-most opaque cell wins.
///
/// Compositing is per cell: the first non-[`CellContent::Empty`] cell from
/// the top wins, including tails. A wide glyph half-covered by a higher
/// layer is then repaired into blanks, so the output is always renderable.
pub fn composite(doc: &Document) -> CellBuffer {
    let mut out = CellBuffer::new(doc.size());
    let visible: Vec<_> = doc.layers().iter().rev().filter(|l| l.visible).collect();
    for pos in out.bounds().positions() {
        let winner = visible
            .iter()
            .find_map(|layer| layer.canvas().get(pos).filter(|c| !c.is_empty()));
        if let Some(cell) = winner {
            out.set(pos, cell.clone());
        }
    }
    out.repair_wide_glyphs();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CellStyle, Color, Grapheme};

    fn cell(s: &str) -> Cell {
        Cell::glyph(Grapheme::new(s).unwrap(), CellStyle::DEFAULT)
    }

    fn styled(s: &str, bg: Color) -> Cell {
        Cell::glyph(
            Grapheme::new(s).unwrap(),
            CellStyle::new(Color::Default, bg),
        )
    }

    fn row_text(buf: &CellBuffer, y: u16) -> String {
        buf.row(y)
            .unwrap()
            .iter()
            .map(|c| match &c.content {
                CellContent::Empty => ".".to_owned(),
                CellContent::Glyph(g) => g.as_str().to_owned(),
                CellContent::WideTail => "<".to_owned(),
            })
            .collect()
    }

    fn two_layer_doc(size: Size) -> Document {
        let mut doc = Document::new(size).unwrap();
        doc.add_layer("Top").unwrap();
        doc
    }

    #[test]
    fn empty_document_composites_to_empty_cells() {
        let doc = Document::new(Size::new(3, 2)).unwrap();
        let buf = composite(&doc);
        assert!(buf.cells().iter().all(Cell::is_empty));
        assert_eq!(buf.size(), Size::new(3, 2));
    }

    #[test]
    fn upper_layer_wins_and_transparent_cells_show_lower() {
        let mut doc = two_layer_doc(Size::new(4, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(0, 0), cell("a"))
            .unwrap();
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), cell("b"))
            .unwrap();
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), styled("B", Color::RED))
            .unwrap();
        let buf = composite(&doc);
        assert_eq!(row_text(&buf, 0), "aB..");
        assert_eq!(buf.get(Position::new(1, 0)).unwrap().style.bg, Color::RED);
    }

    #[test]
    fn hidden_layer_is_skipped() {
        let mut doc = two_layer_doc(Size::new(2, 1));
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(0, 0), cell("x"))
            .unwrap();
        doc.layer_at_mut(1).unwrap().visible = false;
        assert_eq!(row_text(&composite(&doc), 0), "..");
    }

    #[test]
    fn opaque_blank_covers_lower_glyph() {
        let mut doc = two_layer_doc(Size::new(2, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(0, 0), cell("x"))
            .unwrap();
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(0, 0), Cell::blank(CellStyle::DEFAULT))
            .unwrap();
        assert_eq!(row_text(&composite(&doc), 0), " .");
    }

    #[test]
    fn wide_glyph_survives_when_uncovered() {
        let mut doc = two_layer_doc(Size::new(4, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), cell("漢"))
            .unwrap();
        assert_eq!(row_text(&composite(&doc), 0), ".漢<.");
    }

    #[test]
    fn covered_head_leaves_repaired_tail() {
        let mut doc = two_layer_doc(Size::new(4, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), cell("漢"))
            .unwrap();
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), cell("x"))
            .unwrap();
        let buf = composite(&doc);
        assert_eq!(row_text(&buf, 0), ".x .");
    }

    #[test]
    fn covered_tail_leaves_repaired_head() {
        let mut doc = two_layer_doc(Size::new(4, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), styled("漢", Color::BLUE))
            .unwrap();
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(2, 0), cell("x"))
            .unwrap();
        let buf = composite(&doc);
        assert_eq!(row_text(&buf, 0), ". x.");
        // The repaired blank keeps the style of the glyph it replaced.
        assert_eq!(buf.get(Position::new(1, 0)).unwrap().style.bg, Color::BLUE);
    }

    #[test]
    fn upper_wide_glyph_over_lower_wide_glyph_offset() {
        let mut doc = two_layer_doc(Size::new(5, 1));
        doc.layer_at_mut(0)
            .unwrap()
            .canvas_mut()
            .put(Position::new(0, 0), cell("漢"))
            .unwrap();
        doc.layer_at_mut(1)
            .unwrap()
            .canvas_mut()
            .put(Position::new(1, 0), cell("字"))
            .unwrap();
        assert_eq!(row_text(&composite(&doc), 0), " 字<..");
    }
}
