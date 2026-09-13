use serde::{Deserialize, Serialize};

use super::{CanvasError, Layer, Size};
use crate::id::ObjectId;

/// One screen of a design: a size and a stack of layers, bottom first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    pub id: ObjectId,
    pub name: String,
    size: Size,
    layers: Vec<Layer>,
}

impl Screen {
    /// A screen with an `interface` layer on top of an `artwork` layer,
    /// so both editing models are available immediately.
    pub fn new(id: ObjectId, name: impl Into<String>, size: Size) -> Result<Self, CanvasError> {
        let prefix = id.as_str();
        let art_id = ObjectId::slugify(&format!("{prefix}-artwork"));
        let ui_id = ObjectId::slugify(&format!("{prefix}-ui"));
        Ok(Self {
            id,
            name: name.into(),
            size,
            layers: vec![
                Layer::artwork(art_id, "Artwork", size)?,
                Layer::interface(ui_id, "UI"),
            ],
        })
    }

    pub const fn size(&self) -> Size {
        self.size
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layers_mut(&mut self) -> &mut Vec<Layer> {
        &mut self.layers
    }

    pub fn layer(&self, id: &ObjectId) -> Option<&Layer> {
        self.layers.iter().find(|l| &l.id == id)
    }

    pub fn layer_mut(&mut self, id: &ObjectId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| &l.id == id)
    }

    pub fn layer_index(&self, id: &ObjectId) -> Option<usize> {
        self.layers.iter().position(|l| &l.id == id)
    }

    /// Resizes the screen and all artwork layers.
    pub fn resize(&mut self, size: Size) -> Result<(), CanvasError> {
        for l in &mut self.layers {
            l.resize(size)?;
        }
        self.size = size;
        Ok(())
    }
}
