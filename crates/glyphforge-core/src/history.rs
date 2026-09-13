//! Undo/redo built on patches.
//!
//! Every change to a document is a [`Transaction`]: a label, an origin and
//! a forward/inverse patch pair. Continuous edits (a stroke, a typed word)
//! accumulate into one open transaction until it is committed.

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::patch::{Operation, Patch, PatchError};

/// Who made a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    User,
    Agent { name: String, description: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub label: String,
    pub origin: Origin,
    pub forward: Patch,
    pub inverse: Patch,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Patch(#[from] PatchError),
    #[error("no transaction is open")]
    NoOpenTransaction,
    #[error("a transaction ({0:?}) is already open")]
    TransactionOpen(String),
}

#[derive(Debug)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    open: Option<Transaction>,
    limit: usize,
    /// `undo.len()` when the document was last saved, if reachable.
    saved_depth: Option<usize>,
}

impl History {
    pub const DEFAULT_LIMIT: usize = 1000;

    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
            limit: limit.max(1),
            saved_depth: Some(0),
        }
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.open.as_ref().is_some_and(|t| !t.forward.is_empty())
            || self.saved_depth != Some(self.undo.len())
    }

    pub fn mark_saved(&mut self) {
        self.saved_depth = Some(self.undo.len());
    }

    pub fn has_open_transaction(&self) -> bool {
        self.open.is_some()
    }

    pub fn open_label(&self) -> Option<&str> {
        self.open.as_ref().map(|t| t.label.as_str())
    }

    /// Applies a whole patch as one transaction.
    pub fn apply(
        &mut self,
        doc: &mut Document,
        patch: Patch,
        label: impl Into<String>,
        origin: Origin,
    ) -> Result<(), HistoryError> {
        if let Some(open) = &self.open {
            return Err(HistoryError::TransactionOpen(open.label.clone()));
        }
        let inverse = patch.apply(doc)?;
        self.commit(Transaction {
            label: label.into(),
            origin,
            forward: patch,
            inverse,
        });
        Ok(())
    }

    /// Opens a transaction that collects operations until [`Self::end`].
    pub fn begin(&mut self, label: impl Into<String>, origin: Origin) -> Result<(), HistoryError> {
        if let Some(open) = &self.open {
            return Err(HistoryError::TransactionOpen(open.label.clone()));
        }
        self.open = Some(Transaction {
            label: label.into(),
            origin,
            forward: Patch::default(),
            inverse: Patch::default(),
        });
        Ok(())
    }

    /// Applies `op` immediately and records it in the open transaction.
    pub fn record(&mut self, doc: &mut Document, op: Operation) -> Result<(), HistoryError> {
        let open = self.open.as_mut().ok_or(HistoryError::NoOpenTransaction)?;
        let inverse = Patch::single(op.clone()).apply(doc)?;
        open.forward.operations.push(op);
        // Inverses are replayed in reverse order, so prepend.
        let mut inv = inverse.operations;
        inv.append(&mut open.inverse.operations);
        open.inverse.operations = inv;
        Ok(())
    }

    /// Commits the open transaction; an empty one is dropped.
    pub fn end(&mut self) -> Option<&Transaction> {
        let tx = self.open.take()?;
        if tx.forward.is_empty() {
            return None;
        }
        self.commit(tx);
        self.undo.last()
    }

    fn commit(&mut self, tx: Transaction) {
        self.undo.push(tx);
        self.redo.clear();
        if self.undo.len() > self.limit {
            let drop = self.undo.len() - self.limit;
            self.undo.drain(..drop);
            self.saved_depth = self.saved_depth.and_then(|d| d.checked_sub(drop));
        }
    }

    /// Reverts the latest transaction and returns its label.
    pub fn undo(&mut self, doc: &mut Document) -> Result<Option<String>, HistoryError> {
        if self.open.is_some() {
            self.end();
        }
        let Some(tx) = self.undo.pop() else {
            return Ok(None);
        };
        tx.inverse.apply(doc)?;
        let label = tx.label.clone();
        self.redo.push(tx);
        Ok(Some(label))
    }

    pub fn redo(&mut self, doc: &mut Document) -> Result<Option<String>, HistoryError> {
        let Some(tx) = self.redo.pop() else {
            return Ok(None);
        };
        tx.forward.apply(doc)?;
        let label = tx.label.clone();
        self.undo.push(tx);
        Ok(Some(label))
    }

    pub fn undo_stack(&self) -> &[Transaction] {
        &self.undo
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(Self::DEFAULT_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cell, CellStyle, Grapheme, Position, Size};
    use crate::id;
    use crate::patch::CellWrite;

    fn write(x: u16, s: &str) -> Operation {
        Operation::SetCells {
            layer: id!("main-artwork"),
            cells: vec![CellWrite {
                x,
                y: 0,
                cell: Cell::glyph(Grapheme::new(s).unwrap(), CellStyle::DEFAULT),
            }],
        }
    }

    fn glyphs(doc: &Document) -> String {
        doc.layer(&id!("main-artwork"))
            .unwrap()
            .cells()
            .unwrap()
            .row(0)
            .unwrap()
            .iter()
            .map(|c| c.as_glyph().map_or(".", Grapheme::as_str).to_owned())
            .collect()
    }

    #[test]
    fn stroke_is_one_transaction() {
        let mut doc = Document::new(Size::new(4, 1)).unwrap();
        let mut h = History::default();
        h.begin("Type", Origin::User).unwrap();
        h.record(&mut doc, write(0, "a")).unwrap();
        h.record(&mut doc, write(1, "b")).unwrap();
        assert!(h.is_dirty());
        h.end();
        assert_eq!(h.undo_len(), 1);
        assert_eq!(glyphs(&doc), "ab..");
        assert_eq!(h.undo(&mut doc).unwrap().as_deref(), Some("Type"));
        assert_eq!(glyphs(&doc), "....");
        assert!(!h.is_dirty());
        h.redo(&mut doc).unwrap();
        assert_eq!(glyphs(&doc), "ab..");
    }

    #[test]
    fn overlapping_writes_undo_in_the_right_order() {
        let mut doc = Document::new(Size::new(4, 1)).unwrap();
        let mut h = History::default();
        h.begin("Type", Origin::User).unwrap();
        h.record(&mut doc, write(0, "漢")).unwrap();
        h.record(&mut doc, write(1, "x")).unwrap();
        h.end();
        h.undo(&mut doc).unwrap();
        assert_eq!(glyphs(&doc), "....");
        assert!(
            doc.layer(&id!("main-artwork"))
                .unwrap()
                .cells()
                .unwrap()
                .invariant_holds()
        );
    }

    #[test]
    fn new_commit_clears_redo_and_limit_drops_oldest() {
        let mut doc = Document::new(Size::new(8, 1)).unwrap();
        let mut h = History::new(2);
        for (i, s) in ["a", "b", "c"].iter().enumerate() {
            h.apply(
                &mut doc,
                Patch::single(write(i as u16, s)),
                format!("w{i}"),
                Origin::User,
            )
            .unwrap();
        }
        assert_eq!(h.undo_len(), 2, "limit trims the oldest");
        assert!(h.is_dirty());
        h.undo(&mut doc).unwrap();
        assert_eq!(h.redo_len(), 1);
        h.apply(&mut doc, Patch::single(write(5, "z")), "new", Origin::User)
            .unwrap();
        assert_eq!(h.redo_len(), 0);
        assert!(h.undo(&mut doc).unwrap().is_some());
        assert!(h.undo(&mut doc).unwrap().is_some());
        assert!(
            h.undo(&mut doc).unwrap().is_none(),
            "trimmed transactions cannot be undone"
        );
        assert!(h.is_dirty(), "saved state is unreachable after trimming");
    }

    #[test]
    fn saved_marker_tracks_dirty_state() {
        let mut doc = Document::new(Size::new(4, 1)).unwrap();
        let mut h = History::default();
        h.apply(&mut doc, Patch::single(write(0, "a")), "a", Origin::User)
            .unwrap();
        h.mark_saved();
        assert!(!h.is_dirty());
        h.undo(&mut doc).unwrap();
        assert!(h.is_dirty());
        h.redo(&mut doc).unwrap();
        assert!(!h.is_dirty());
    }

    #[test]
    fn failed_record_leaves_transaction_consistent() {
        let mut doc = Document::new(Size::new(2, 1)).unwrap();
        let mut h = History::default();
        h.begin("Type", Origin::User).unwrap();
        h.record(&mut doc, write(0, "a")).unwrap();
        assert!(h.record(&mut doc, write(5, "b")).is_err());
        h.end();
        h.undo(&mut doc).unwrap();
        assert_eq!(glyphs(&doc), "..");
        assert_eq!(
            doc.layer(&id!("main-artwork"))
                .unwrap()
                .cells()
                .unwrap()
                .get(Position::ORIGIN)
                .unwrap(),
            &Cell::EMPTY
        );
    }

    #[test]
    fn nested_begin_is_an_error() {
        let mut h = History::default();
        h.begin("a", Origin::User).unwrap();
        assert!(matches!(
            h.begin("b", Origin::User),
            Err(HistoryError::TransactionOpen(_))
        ));
    }
}
