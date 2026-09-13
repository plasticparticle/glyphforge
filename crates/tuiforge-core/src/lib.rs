//! Terminal-independent document model for TUIForge.
//!
//! This crate knows nothing about Ratatui or Crossterm. It models a document
//! as layers of terminal cells, composites them, and (in later milestones)
//! provides history commands, selections, box-drawing topology and
//! import/export codecs.

pub mod compose;
pub mod document;

pub use compose::{CellBuffer, composite};
pub use document::{
    Attributes, Canvas, CanvasError, Cell, CellChange, CellContent, CellStyle, Color, Document,
    DocumentError, Grapheme, GraphemeError, Layer, LayerId, Position, Rect, Size,
};
