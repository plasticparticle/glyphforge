//! The document model: cells, canvases, layers, screens and documents.

mod canvas;
mod canvas_repr;
mod cell;
mod color;
mod doc;
mod grapheme;
mod layer;
mod position;
mod screen;

pub use canvas::{Canvas, CanvasError, CellChange};
pub use canvas_repr::{CanvasRepr, CanvasReprError, StyleRun, TextRun};
pub use cell::{Attributes, Cell, CellContent, CellStyle};
pub use color::Color;
pub use doc::{Document, DocumentError, DocumentMetadata};
pub use grapheme::{Grapheme, GraphemeError};
pub use layer::{Layer, LayerContent, LayerKind};
pub use position::{Position, Rect, Size};
pub use screen::Screen;
