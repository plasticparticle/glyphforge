//! The document model: cells, canvases, layers and documents.

mod canvas;
mod cell;
mod color;
mod doc;
mod grapheme;
mod layer;
mod position;

pub use canvas::{Canvas, CanvasError, CellChange};
pub use cell::{Attributes, Cell, CellContent, CellStyle};
pub use color::Color;
pub use doc::{Document, DocumentError, DocumentMetadata};
pub use grapheme::{Grapheme, GraphemeError};
pub use layer::{Layer, LayerId};
pub use position::{Position, Rect, Size};
