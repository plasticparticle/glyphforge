use serde::{Deserialize, Serialize};

use super::{CanvasError, Layer, Screen, Size};
use crate::component::{Component, tree};
use crate::id::ObjectId;
use crate::theme::Theme;

/// Errors from document-level operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error("no object with id {0}")]
    NoSuchObject(ObjectId),
    #[error("id {0} is already used")]
    DuplicateId(ObjectId),
    #[error("layer {0} is locked")]
    LayerLocked(ObjectId),
    #[error("layer holds {actual} content, not {expected}")]
    WrongLayerKind {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("a screen needs at least one layer")]
    LastLayer,
}

/// Free-form information about the document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentMetadata {
    pub title: String,
    pub author: String,
    pub description: String,
}

/// A design: metadata, a theme and one or more screens.
///
/// All object ids (screens, layers, components) are unique across the
/// whole document so any object can be addressed by id alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub meta: DocumentMetadata,
    #[serde(default)]
    pub theme: Theme,
    screens: Vec<Screen>,
}

impl Document {
    pub const DEFAULT_SIZE: Size = Size::new(80, 24);

    /// A document with one screen called `main`.
    pub fn new(size: Size) -> Result<Self, DocumentError> {
        let screen = Screen::new(ObjectId::slugify("main"), "Main", size)?;
        Ok(Self {
            meta: DocumentMetadata::default(),
            theme: Theme::default(),
            screens: vec![screen],
        })
    }

    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    pub fn screens_mut(&mut self) -> &mut Vec<Screen> {
        &mut self.screens
    }

    pub fn screen(&self, id: &ObjectId) -> Option<&Screen> {
        self.screens.iter().find(|s| &s.id == id)
    }

    pub fn screen_mut(&mut self, id: &ObjectId) -> Option<&mut Screen> {
        self.screens.iter_mut().find(|s| &s.id == id)
    }

    /// The first screen; every document has at least one.
    pub fn first_screen(&self) -> &Screen {
        &self.screens[0]
    }

    /// Iterates all layers of all screens with their screen.
    pub fn layers(&self) -> impl Iterator<Item = (&Screen, &Layer)> {
        self.screens
            .iter()
            .flat_map(|s| s.layers().iter().map(move |l| (s, l)))
    }

    pub fn layer(&self, id: &ObjectId) -> Option<&Layer> {
        self.layers().find(|(_, l)| &l.id == id).map(|(_, l)| l)
    }

    /// The screen containing layer `id`.
    pub fn screen_of_layer(&self, id: &ObjectId) -> Option<&Screen> {
        self.layers().find(|(_, l)| &l.id == id).map(|(s, _)| s)
    }

    pub fn layer_mut(&mut self, id: &ObjectId) -> Option<&mut Layer> {
        self.screens.iter_mut().find_map(|s| s.layer_mut(id))
    }

    /// Iterates all components of all interface layers.
    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.layers()
            .filter_map(|(_, l)| l.components())
            .flat_map(tree::iter)
    }

    pub fn component(&self, id: &ObjectId) -> Option<&Component> {
        self.layers()
            .filter_map(|(_, l)| l.components())
            .find_map(|roots| tree::find(roots, id))
    }

    pub fn component_mut(&mut self, id: &ObjectId) -> Option<&mut Component> {
        self.screens
            .iter_mut()
            .flat_map(|s| s.layers_mut().iter_mut())
            .filter_map(Layer::components_mut)
            .find_map(|roots| tree::find_mut(roots, id))
    }

    /// The layer holding component `id`.
    pub fn layer_of_component(&self, id: &ObjectId) -> Option<&Layer> {
        self.layers()
            .map(|(_, l)| l)
            .find(|l| l.components().is_some_and(|r| tree::find(r, id).is_some()))
    }

    pub fn layer_of_component_mut(&mut self, id: &ObjectId) -> Option<&mut Layer> {
        self.screens
            .iter_mut()
            .flat_map(|s| s.layers_mut().iter_mut())
            .find(|l| l.components().is_some_and(|r| tree::find(r, id).is_some()))
    }

    /// Whether any screen, layer or component uses `id`.
    pub fn has_id(&self, id: &ObjectId) -> bool {
        self.screens.iter().any(|s| &s.id == id)
            || self.layers().any(|(_, l)| &l.id == id)
            || self.component(id).is_some()
    }

    /// All ids in the document, in document order.
    pub fn ids(&self) -> Vec<ObjectId> {
        let mut out = Vec::new();
        for s in &self.screens {
            out.push(s.id.clone());
            for l in s.layers() {
                out.push(l.id.clone());
                if let Some(roots) = l.components() {
                    out.extend(tree::iter(roots).map(|c| c.id.clone()));
                }
            }
        }
        out
    }

    /// Ids used more than once. A loaded document with duplicates is
    /// rejected by the project loader.
    pub fn duplicate_ids(&self) -> Vec<ObjectId> {
        let mut seen = std::collections::BTreeSet::new();
        let mut dups = Vec::new();
        for id in self.ids() {
            if !seen.insert(id.clone()) && !dups.contains(&id) {
                dups.push(id);
            }
        }
        dups
    }

    /// Adds a screen; its id must be unused.
    pub fn add_screen(&mut self, screen: Screen) -> Result<(), DocumentError> {
        if self.has_id(&screen.id) {
            return Err(DocumentError::DuplicateId(screen.id));
        }
        self.screens.push(screen);
        Ok(())
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SIZE).unwrap_or_else(|_| unreachable!("DEFAULT_SIZE is non-zero"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;

    #[test]
    fn new_document_has_artwork_and_interface_layers() {
        let doc = Document::new(Size::new(10, 5)).unwrap();
        let s = doc.first_screen();
        assert_eq!(s.id, id!("main"));
        assert_eq!(s.layers().len(), 2);
        assert_eq!(s.layers()[0].id, id!("main-artwork"));
        assert_eq!(s.layers()[1].id, id!("main-ui"));
        assert!(doc.duplicate_ids().is_empty());
    }

    #[test]
    fn component_lookup_across_layers() {
        let mut doc = Document::new(Size::new(10, 5)).unwrap();
        let ui = id!("main-ui");
        doc.layer_mut(&ui).unwrap().components_mut().unwrap().push(
            Component::new(id!("root"), "panel")
                .with_children(vec![Component::new(id!("child"), "label")]),
        );
        assert_eq!(doc.component(&id!("child")).unwrap().kind, "label");
        assert_eq!(doc.layer_of_component(&id!("child")).unwrap().id, ui);
        assert!(doc.has_id(&id!("child")));
        assert!(!doc.has_id(&id!("ghost")));
        doc.component_mut(&id!("child")).unwrap().kind = "button".into();
        assert_eq!(doc.component(&id!("child")).unwrap().kind, "button");
    }

    #[test]
    fn duplicate_ids_are_detected() {
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        let roots = doc
            .layer_mut(&id!("main-ui"))
            .unwrap()
            .components_mut()
            .unwrap();
        roots.push(Component::new(id!("a"), "label"));
        roots.push(Component::new(id!("a"), "label"));
        assert_eq!(doc.duplicate_ids(), vec![id!("a")]);
        assert!(
            doc.add_screen(Screen::new(id!("main"), "Dup", Size::new(2, 2)).unwrap())
                .is_err()
        );
    }
}
