use serde::{Deserialize, Serialize};

use super::{Canvas, CanvasError, Size};
use crate::component::Component;
use crate::id::ObjectId;

/// What a layer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    /// Raw cells: logos, banners, ornaments (Subcell Mode).
    Artwork,
    /// Semantic components rendered on demand (Interface Mode).
    Interface,
}

impl LayerKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Artwork => "artwork",
            Self::Interface => "interface",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LayerContent {
    Artwork {
        cells: Canvas,
    },
    Interface {
        #[serde(default)]
        components: Vec<Component>,
    },
}

impl LayerContent {
    pub const fn kind(&self) -> LayerKind {
        match self {
            Self::Artwork { .. } => LayerKind::Artwork,
            Self::Interface { .. } => LayerKind::Interface,
        }
    }
}

/// One layer of a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub id: ObjectId,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(flatten)]
    pub content: LayerContent,
}

fn default_true() -> bool {
    true
}

impl Layer {
    pub fn artwork(id: ObjectId, name: impl Into<String>, size: Size) -> Result<Self, CanvasError> {
        Ok(Self {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            content: LayerContent::Artwork {
                cells: Canvas::new(size)?,
            },
        })
    }

    pub fn interface(id: ObjectId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            content: LayerContent::Interface {
                components: Vec::new(),
            },
        }
    }

    pub const fn kind(&self) -> LayerKind {
        self.content.kind()
    }

    pub const fn cells(&self) -> Option<&Canvas> {
        match &self.content {
            LayerContent::Artwork { cells } => Some(cells),
            LayerContent::Interface { .. } => None,
        }
    }

    pub const fn cells_mut(&mut self) -> Option<&mut Canvas> {
        match &mut self.content {
            LayerContent::Artwork { cells } => Some(cells),
            LayerContent::Interface { .. } => None,
        }
    }

    pub fn components(&self) -> Option<&[Component]> {
        match &self.content {
            LayerContent::Interface { components } => Some(components),
            LayerContent::Artwork { .. } => None,
        }
    }

    pub const fn components_mut(&mut self) -> Option<&mut Vec<Component>> {
        match &mut self.content {
            LayerContent::Interface { components } => Some(components),
            LayerContent::Artwork { .. } => None,
        }
    }

    /// A copy with a new id and name.
    #[must_use]
    pub fn duplicate(&self, id: ObjectId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            ..self.clone()
        }
    }

    /// Resizes artwork to `size`; interface layers have no size of their own.
    pub fn resize(&mut self, size: Size) -> Result<(), CanvasError> {
        if let LayerContent::Artwork { cells } = &mut self.content {
            *cells = cells.resized(size)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;

    #[test]
    fn serde_uses_kind_tag() {
        let l = Layer::interface(id!("ui"), "UI");
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"kind\":\"interface\""));
        let a = Layer::artwork(id!("art"), "Art", Size::new(2, 1)).unwrap();
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"kind\":\"artwork\""));
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn duplicate_gets_new_id() {
        let a = Layer::artwork(id!("a"), "A", Size::new(2, 2)).unwrap();
        let b = a.duplicate(id!("b"), "B");
        assert_ne!(a.id, b.id);
        assert_eq!(a.cells(), b.cells());
    }
}
