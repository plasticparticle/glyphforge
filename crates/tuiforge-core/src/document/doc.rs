use serde::{Deserialize, Serialize};

use super::{Canvas, CanvasError, Cell, CellChange, Layer, LayerId, Position, Size};

/// Errors from document-level operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error("layer {0:?} does not exist")]
    NoSuchLayer(LayerId),
    #[error("layer {name:?} is locked")]
    LayerLocked { name: String },
    #[error("layer {name:?} is hidden")]
    LayerHidden { name: String },
}

/// Free-form information about the document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentMetadata {
    pub title: String,
    pub author: String,
    pub description: String,
}

/// A document: a stack of equally sized layers plus metadata.
///
/// Layers are stored bottom to top; index 0 is drawn first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    size: Size,
    layers: Vec<Layer>,
    active_layer: usize,
    pub metadata: DocumentMetadata,
}

impl Document {
    pub const DEFAULT_SIZE: Size = Size::new(80, 24);

    /// A document with one empty layer named "Background".
    pub fn new(size: Size) -> Result<Self, DocumentError> {
        let layer = Layer::new("Background", size)?;
        Ok(Self {
            size,
            layers: vec![layer],
            active_layer: 0,
            metadata: DocumentMetadata::default(),
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

    /// Layers bottom to top.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub const fn active_layer_index(&self) -> usize {
        self.active_layer
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active_layer]
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id() == id)
    }

    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id() == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id() == id)
    }

    /// Mutable access to the layer at z-index `index` (0 = bottom).
    pub fn layer_at_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    /// Adds a new empty layer on top and returns its id. The new layer does
    /// not become active; callers decide that.
    pub fn add_layer(&mut self, name: impl Into<String>) -> Result<LayerId, DocumentError> {
        let layer = Layer::new(name, self.size)?;
        let id = layer.id();
        self.layers.push(layer);
        Ok(id)
    }

    /// Makes the layer with `id` active.
    pub fn set_active_layer(&mut self, id: LayerId) -> Result<(), DocumentError> {
        self.active_layer = self.layer_index(id).ok_or(DocumentError::NoSuchLayer(id))?;
        Ok(())
    }

    /// Writes `cell` on the active layer, honouring the layer lock.
    pub fn put_cell(
        &mut self,
        pos: Position,
        cell: Cell,
    ) -> Result<Vec<CellChange>, DocumentError> {
        let layer = &mut self.layers[self.active_layer];
        if layer.locked {
            return Err(DocumentError::LayerLocked {
                name: layer.name.clone(),
            });
        }
        Ok(layer.canvas_mut().put(pos, cell)?)
    }

    /// The canvas of the active layer.
    pub fn active_canvas(&self) -> &Canvas {
        self.layers[self.active_layer].canvas()
    }

    /// Cell at `pos` on the active layer.
    pub fn cell_at(&self, pos: Position) -> Option<&Cell> {
        self.active_canvas().get(pos)
    }
}

impl Default for Document {
    fn default() -> Self {
        // A non-zero constant size cannot fail.
        Self::new(Self::DEFAULT_SIZE).unwrap_or_else(|_| unreachable!("DEFAULT_SIZE is non-zero"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CellStyle, Grapheme};

    #[test]
    fn new_document_has_one_background_layer() {
        let doc = Document::new(Size::new(10, 5)).unwrap();
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.active_layer().name, "Background");
        assert_eq!(doc.size(), Size::new(10, 5));
    }

    #[test]
    fn locked_layer_rejects_edits() {
        let mut doc = Document::new(Size::new(10, 5)).unwrap();
        doc.layer_at_mut(0).unwrap().locked = true;
        let cell = Cell::glyph(Grapheme::new("x").unwrap(), CellStyle::DEFAULT);
        assert!(matches!(
            doc.put_cell(Position::ORIGIN, cell),
            Err(DocumentError::LayerLocked { .. })
        ));
    }

    #[test]
    fn put_cell_writes_to_active_layer() {
        let mut doc = Document::new(Size::new(10, 5)).unwrap();
        let cell = Cell::glyph(Grapheme::new("x").unwrap(), CellStyle::DEFAULT);
        doc.put_cell(Position::new(2, 3), cell.clone()).unwrap();
        assert_eq!(doc.cell_at(Position::new(2, 3)), Some(&cell));
    }
}
