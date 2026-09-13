//! Terminal-independent document model for Glyphforge.
//!
//! This crate knows nothing about Ratatui or Crossterm. It models a design
//! semantically (screens, layers, components with stable ids, layout and
//! style tokens), keeps raw artwork as cells, renders both to a cell canvas,
//! and provides the patch/history mechanism that humans and agents share.

pub mod api;
pub mod boxdraw;
pub mod component;
pub mod document;
pub mod history;
pub mod id;
pub mod layout;
pub mod patch;
pub mod project;
pub mod render;
pub mod theme;
pub mod validate;
pub mod value;

pub use component::Component;
pub use document::{
    Attributes, Canvas, CanvasError, Cell, CellChange, CellContent, CellStyle, Color, Document,
    DocumentError, Grapheme, GraphemeError, Layer, LayerContent, LayerKind, Position, Rect, Screen,
    Size,
};
pub use id::ObjectId;
pub use theme::Theme;
pub use value::Value;
