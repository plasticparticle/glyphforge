use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Canvas, CanvasError, Size};

/// Stable identity of a layer, independent of its z-order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerId(Uuid);

impl LayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::new()
    }
}

/// One layer of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    id: LayerId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    /// Reserved for future formats; compositing ignores it for now.
    pub opacity: f32,
    canvas: Canvas,
}

impl Layer {
    pub fn new(name: impl Into<String>, size: Size) -> Result<Self, CanvasError> {
        Ok(Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            canvas: Canvas::new(size)?,
        })
    }

    pub const fn id(&self) -> LayerId {
        self.id
    }

    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub const fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    pub const fn size(&self) -> Size {
        self.canvas.size()
    }

    /// A copy with a fresh id and the given name.
    #[must_use]
    pub fn duplicate(&self, name: impl Into<String>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            ..self.clone()
        }
    }

    pub fn resize(&mut self, size: Size) -> Result<(), CanvasError> {
        self.canvas = self.canvas.resized(size)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_gets_new_id() {
        let a = Layer::new("A", Size::new(2, 2)).unwrap();
        let b = a.duplicate("B");
        assert_ne!(a.id(), b.id());
        assert_eq!(b.name, "B");
        assert_eq!(a.canvas(), b.canvas());
    }
}
