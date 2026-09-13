//! Semantic patches: the one mutation mechanism for humans and agents.
//!
//! A patch is a list of operations addressed by object id. Applying a patch
//! validates and applies every operation in order and returns the inverse
//! patch; if any operation fails, the ones already applied are rolled back
//! so a patch is transactional. Inverse patches are what undo replays.

use serde::{Deserialize, Serialize};

use crate::component::{Component, tree};
use crate::document::{Cell, CellChange, Document, DocumentError, Layer, LayerKind, Position};
use crate::id::ObjectId;
use crate::layout::{Dimension, Layout, Placement};
use crate::theme::Theme;
use crate::value::Value;

/// One cell write inside a [`Operation::SetCells`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellWrite {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

impl From<&CellChange> for CellWrite {
    fn from(c: &CellChange) -> Self {
        Self {
            x: c.pos.x,
            y: c.pos.y,
            cell: c.before.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Places a component absolutely at (`x`, `y`) inside its parent.
    Move {
        target: ObjectId,
        x: u16,
        y: u16,
    },
    /// Gives a component a fixed size.
    Resize {
        target: ObjectId,
        width: u16,
        height: u16,
    },
    /// Sets a property; `Value::Null` removes it.
    SetProperty {
        target: ObjectId,
        property: String,
        value: Value,
    },
    /// Sets one size axis without touching the other, so equalising
    /// widths does not silently freeze a `fill` height.
    SetSize {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<Dimension>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<Dimension>,
    },
    /// Replaces the whole layout of a component.
    SetLayout {
        target: ObjectId,
        layout: Layout,
    },
    /// Moves a component to another parent within the same layer, or to
    /// the layer's roots when `parent` is absent. `index` counts
    /// positions after the component has been taken out.
    Reparent {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Adds a component under `parent`, or as a root of `layer`.
    CreateComponent {
        component: Component,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    DeleteComponent {
        target: ObjectId,
    },
    /// Writes cells on an artwork layer.
    SetCells {
        layer: ObjectId,
        cells: Vec<CellWrite>,
    },
    /// Adds a layer to `screen` at `index` (top when omitted).
    CreateLayer {
        layer: Layer,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        screen: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    DeleteLayer {
        target: ObjectId,
    },
    UpdateLayer {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        locked: Option<bool>,
    },
    /// Moves a layer to z-index `index` within its screen (0 = bottom).
    ReorderLayer {
        target: ObjectId,
        index: usize,
    },
    SetTheme {
        theme: Theme,
    },
}

impl Operation {
    /// The object this operation addresses, for diagnostics.
    pub fn target(&self) -> Option<&ObjectId> {
        match self {
            Self::Move { target, .. }
            | Self::Resize { target, .. }
            | Self::SetSize { target, .. }
            | Self::SetProperty { target, .. }
            | Self::SetLayout { target, .. }
            | Self::Reparent { target, .. }
            | Self::DeleteComponent { target }
            | Self::DeleteLayer { target }
            | Self::UpdateLayer { target, .. }
            | Self::ReorderLayer { target, .. } => Some(target),
            Self::CreateComponent { component, .. } => Some(&component.id),
            Self::SetCells { layer, .. } => Some(layer),
            Self::CreateLayer { layer, .. } => Some(&layer.id),
            Self::SetTheme { .. } => None,
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Move { .. } => "move",
            Self::Resize { .. } => "resize",
            Self::SetProperty { .. } => "set_property",
            Self::SetSize { .. } => "set_size",
            Self::SetLayout { .. } => "set_layout",
            Self::Reparent { .. } => "reparent",
            Self::CreateComponent { .. } => "create_component",
            Self::DeleteComponent { .. } => "delete_component",
            Self::SetCells { .. } => "set_cells",
            Self::CreateLayer { .. } => "create_layer",
            Self::DeleteLayer { .. } => "delete_layer",
            Self::UpdateLayer { .. } => "update_layer",
            Self::ReorderLayer { .. } => "reorder_layer",
            Self::SetTheme { .. } => "set_theme",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Patch {
    pub operations: Vec<Operation>,
}

impl Patch {
    pub fn new(operations: Vec<Operation>) -> Self {
        Self { operations }
    }

    pub fn single(op: Operation) -> Self {
        Self {
            operations: vec![op],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Applies all operations transactionally and returns the inverse
    /// patch. On failure the document is left unchanged.
    pub fn apply(&self, doc: &mut Document) -> Result<Patch, PatchError> {
        let mut inverses = Vec::with_capacity(self.operations.len());
        for (index, op) in self.operations.iter().enumerate() {
            match apply_one(doc, op) {
                Ok(inv) => inverses.push(inv),
                Err(error) => {
                    for inv in inverses.iter().rev() {
                        // Rolling back replays inverses of operations that
                        // just succeeded; a failure here would mean the
                        // document model is inconsistent, which the tests
                        // guard against.
                        let _ = apply_one(doc, inv);
                    }
                    return Err(PatchError::Operation {
                        index,
                        op: op.name(),
                        error,
                    });
                }
            }
        }
        inverses.reverse();
        Ok(Patch {
            operations: inverses,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchError {
    #[error("operation {index} ({op}) failed: {error}")]
    Operation {
        index: usize,
        op: &'static str,
        error: OperationError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperationError {
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("{0} is not a component")]
    NotAComponent(ObjectId),
    #[error("{0} is not a layer")]
    NotALayer(ObjectId),
    #[error("{0} is not a screen")]
    NotAScreen(ObjectId),
    #[error("cannot resolve a target layer: give `parent` or `layer`")]
    NoTargetLayer,
    #[error("index {index} is out of range (max {max})")]
    IndexOutOfRange { index: usize, max: usize },
    #[error("cell ({x}, {y}) is outside layer {layer}")]
    CellOutOfBounds { layer: ObjectId, x: u16, y: u16 },
    #[error(
        "{parent} cannot become the parent of {target}: it is {target} itself or one of its children"
    )]
    CircularParent { target: ObjectId, parent: ObjectId },
    #[error("{other} is not on the same layer as {target}")]
    NotInSameLayer { target: ObjectId, other: ObjectId },
}

fn component_mut<'a>(
    doc: &'a mut Document,
    id: &ObjectId,
) -> Result<&'a mut Component, OperationError> {
    let layer = doc
        .layer_of_component(id)
        .ok_or_else(|| OperationError::NotAComponent(id.clone()))?;
    if layer.locked {
        return Err(DocumentError::LayerLocked(layer.id.clone()).into());
    }
    doc.component_mut(id)
        .ok_or_else(|| OperationError::NotAComponent(id.clone()))
}

fn apply_one(doc: &mut Document, op: &Operation) -> Result<Operation, OperationError> {
    match op {
        Operation::Move { .. }
        | Operation::Resize { .. }
        | Operation::SetSize { .. }
        | Operation::SetLayout { .. }
        | Operation::Reparent { .. }
        | Operation::SetProperty { .. }
        | Operation::CreateComponent { .. }
        | Operation::DeleteComponent { .. } => apply_component_op(doc, op),
        Operation::SetCells { layer, cells } => apply_set_cells(doc, layer, cells),
        Operation::CreateLayer { .. }
        | Operation::DeleteLayer { .. }
        | Operation::UpdateLayer { .. }
        | Operation::ReorderLayer { .. } => apply_layer_op(doc, op),
        Operation::SetTheme { theme } => {
            let before = std::mem::replace(&mut doc.theme, theme.clone());
            Ok(Operation::SetTheme { theme: before })
        }
    }
}

fn apply_component_op(doc: &mut Document, op: &Operation) -> Result<Operation, OperationError> {
    match op {
        Operation::Move { target, x, y } => {
            let c = component_mut(doc, target)?;
            let before = c.layout;
            c.layout.placement = Placement::Absolute { x: *x, y: *y };
            Ok(Operation::SetLayout {
                target: target.clone(),
                layout: before,
            })
        }
        Operation::Resize {
            target,
            width,
            height,
        } => {
            let c = component_mut(doc, target)?;
            let before = c.layout;
            c.layout.width = Dimension::Fixed(*width);
            c.layout.height = Dimension::Fixed(*height);
            Ok(Operation::SetLayout {
                target: target.clone(),
                layout: before,
            })
        }
        Operation::SetLayout { target, layout } => {
            let c = component_mut(doc, target)?;
            let before = std::mem::replace(&mut c.layout, *layout);
            Ok(Operation::SetLayout {
                target: target.clone(),
                layout: before,
            })
        }
        Operation::SetSize {
            target,
            width,
            height,
        } => {
            let c = component_mut(doc, target)?;
            let before = c.layout;
            if let Some(w) = width {
                c.layout.width = *w;
            }
            if let Some(h) = height {
                c.layout.height = *h;
            }
            Ok(Operation::SetLayout {
                target: target.clone(),
                layout: before,
            })
        }
        Operation::Reparent {
            target,
            parent,
            index,
        } => apply_reparent(doc, target, parent.as_ref(), *index),
        Operation::SetProperty {
            target,
            property,
            value,
        } => {
            let c = component_mut(doc, target)?;
            let before = if value.is_null() {
                c.props.remove(property)
            } else {
                c.props.insert(property.clone(), value.clone())
            };
            Ok(Operation::SetProperty {
                target: target.clone(),
                property: property.clone(),
                value: before.unwrap_or(Value::Null),
            })
        }
        Operation::CreateComponent {
            component,
            parent,
            layer,
            index,
        } => {
            for c in component.iter() {
                if doc.has_id(&c.id) {
                    return Err(DocumentError::DuplicateId(c.id.clone()).into());
                }
            }
            let layer_id = match (parent, layer) {
                (Some(p), _) => doc
                    .layer_of_component(p)
                    .map(|l| l.id.clone())
                    .ok_or_else(|| OperationError::NotAComponent(p.clone()))?,
                (None, Some(l)) => l.clone(),
                (None, None) => {
                    let mut candidates = doc
                        .layers()
                        .filter(|(_, l)| l.kind() == LayerKind::Interface);
                    match (candidates.next(), candidates.next()) {
                        (Some((_, l)), None) => l.id.clone(),
                        _ => return Err(OperationError::NoTargetLayer),
                    }
                }
            };
            let layer = doc
                .layer_mut(&layer_id)
                .ok_or_else(|| OperationError::NotALayer(layer_id.clone()))?;
            if layer.locked {
                return Err(DocumentError::LayerLocked(layer_id).into());
            }
            let roots = layer
                .components_mut()
                .ok_or(DocumentError::WrongLayerKind {
                    expected: "interface",
                    actual: "artwork",
                })?;
            let idx = index.unwrap_or(usize::MAX);
            if !tree::insert(roots, parent.as_ref(), idx, component.clone()) {
                return Err(OperationError::NotAComponent(
                    parent.clone().unwrap_or_else(|| layer_id.clone()),
                ));
            }
            Ok(Operation::DeleteComponent {
                target: component.id.clone(),
            })
        }
        Operation::DeleteComponent { target } => {
            let layer = doc
                .layer_of_component(target)
                .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
            if layer.locked {
                return Err(DocumentError::LayerLocked(layer.id.clone()).into());
            }
            let layer_id = layer.id.clone();
            let layer = doc
                .layer_mut(&layer_id)
                .ok_or_else(|| OperationError::NotALayer(layer_id.clone()))?;
            let roots = layer
                .components_mut()
                .ok_or(DocumentError::WrongLayerKind {
                    expected: "interface",
                    actual: "artwork",
                })?;
            let (parent, index) = tree::locate(roots, target)
                .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
            let removed = tree::remove(roots, target)
                .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
            Ok(Operation::CreateComponent {
                component: removed,
                parent,
                layer: Some(layer_id),
                index: Some(index),
            })
        }
        _ => unreachable!("dispatched by apply_one"),
    }
}

/// Moves `target` under `new_parent` inside its own layer.
fn apply_reparent(
    doc: &mut Document,
    target: &ObjectId,
    new_parent: Option<&ObjectId>,
    index: Option<usize>,
) -> Result<Operation, OperationError> {
    let layer = doc
        .layer_of_component(target)
        .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    if layer.locked {
        return Err(DocumentError::LayerLocked(layer.id.clone()).into());
    }
    let layer_id = layer.id.clone();
    let layer = doc
        .layer_mut(&layer_id)
        .ok_or_else(|| OperationError::NotALayer(layer_id.clone()))?;
    let roots = layer
        .components_mut()
        .ok_or(DocumentError::WrongLayerKind {
            expected: "interface",
            actual: "artwork",
        })?;
    let (old_parent, old_index) =
        tree::locate(roots, target).ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    if let Some(parent) = new_parent {
        if parent == target {
            return Err(OperationError::CircularParent {
                target: target.clone(),
                parent: parent.clone(),
            });
        }
        let subtree = tree::find(roots, target)
            .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
        if subtree.find(parent).is_some() {
            return Err(OperationError::CircularParent {
                target: target.clone(),
                parent: parent.clone(),
            });
        }
        if tree::find(roots, parent).is_none() {
            return Err(OperationError::NotInSameLayer {
                target: target.clone(),
                other: parent.clone(),
            });
        }
    }
    let component =
        tree::remove(roots, target).ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    let idx = index.unwrap_or(usize::MAX);
    if !tree::insert(roots, new_parent, idx, component) {
        return Err(OperationError::NotAComponent(
            new_parent.cloned().unwrap_or_else(|| target.clone()),
        ));
    }
    Ok(Operation::Reparent {
        target: target.clone(),
        parent: old_parent,
        index: Some(old_index),
    })
}

fn apply_set_cells(
    doc: &mut Document,
    layer: &ObjectId,
    cells: &[CellWrite],
) -> Result<Operation, OperationError> {
    let l = doc
        .layer_mut(layer)
        .ok_or_else(|| OperationError::NotALayer(layer.clone()))?;
    if l.locked {
        return Err(DocumentError::LayerLocked(layer.clone()).into());
    }
    let canvas = l.cells_mut().ok_or(DocumentError::WrongLayerKind {
        expected: "artwork",
        actual: "interface",
    })?;
    let mut before = Vec::new();
    for w in cells {
        let pos = Position::new(w.x, w.y);
        if !canvas.size().contains(pos) {
            // Roll back this operation's own writes before failing.
            for b in before.iter().rev() {
                let b: &CellWrite = b;
                canvas.restore(Position::new(b.x, b.y), b.cell.clone());
            }
            return Err(OperationError::CellOutOfBounds {
                layer: layer.clone(),
                x: w.x,
                y: w.y,
            });
        }
        match canvas.put(pos, w.cell.clone()) {
            Ok(changes) => before.extend(changes.iter().map(CellWrite::from)),
            Err(e) => {
                for b in before.iter().rev() {
                    canvas.restore(Position::new(b.x, b.y), b.cell.clone());
                }
                return Err(DocumentError::from(e).into());
            }
        }
    }
    before.reverse();
    Ok(Operation::SetCells {
        layer: layer.clone(),
        cells: before,
    })
}

fn apply_layer_op(doc: &mut Document, op: &Operation) -> Result<Operation, OperationError> {
    match op {
        Operation::CreateLayer {
            layer,
            screen,
            index,
        } => {
            if doc.has_id(&layer.id) {
                return Err(DocumentError::DuplicateId(layer.id.clone()).into());
            }
            let screen_id = screen
                .clone()
                .unwrap_or_else(|| doc.first_screen().id.clone());
            let s = doc
                .screen_mut(&screen_id)
                .ok_or_else(|| OperationError::NotAScreen(screen_id.clone()))?;
            let mut layer = layer.clone();
            layer.resize(s.size()).map_err(DocumentError::from)?;
            let max = s.layers().len();
            let idx = index.unwrap_or(max);
            if idx > max {
                return Err(OperationError::IndexOutOfRange { index: idx, max });
            }
            s.layers_mut().insert(idx, layer.clone());
            Ok(Operation::DeleteLayer { target: layer.id })
        }
        Operation::DeleteLayer { target } => {
            let screen_id = doc
                .screen_of_layer(target)
                .map(|s| s.id.clone())
                .ok_or_else(|| OperationError::NotALayer(target.clone()))?;
            let s = doc
                .screen_mut(&screen_id)
                .ok_or_else(|| OperationError::NotAScreen(screen_id.clone()))?;
            if s.layers().len() == 1 {
                return Err(DocumentError::LastLayer.into());
            }
            let index = s
                .layer_index(target)
                .ok_or_else(|| OperationError::NotALayer(target.clone()))?;
            let removed = s.layers_mut().remove(index);
            Ok(Operation::CreateLayer {
                layer: removed,
                screen: Some(screen_id),
                index: Some(index),
            })
        }
        Operation::UpdateLayer {
            target,
            name,
            visible,
            locked,
        } => {
            let l = doc
                .layer_mut(target)
                .ok_or_else(|| OperationError::NotALayer(target.clone()))?;
            let inverse = Operation::UpdateLayer {
                target: target.clone(),
                name: name.as_ref().map(|_| l.name.clone()),
                visible: visible.map(|_| l.visible),
                locked: locked.map(|_| l.locked),
            };
            if let Some(n) = name {
                l.name.clone_from(n);
            }
            if let Some(v) = visible {
                l.visible = *v;
            }
            if let Some(k) = locked {
                l.locked = *k;
            }
            Ok(inverse)
        }
        Operation::ReorderLayer { target, index } => {
            let screen_id = doc
                .screen_of_layer(target)
                .map(|s| s.id.clone())
                .ok_or_else(|| OperationError::NotALayer(target.clone()))?;
            let s = doc
                .screen_mut(&screen_id)
                .ok_or_else(|| OperationError::NotAScreen(screen_id.clone()))?;
            let max = s.layers().len() - 1;
            if *index > max {
                return Err(OperationError::IndexOutOfRange { index: *index, max });
            }
            let from = s
                .layer_index(target)
                .ok_or_else(|| OperationError::NotALayer(target.clone()))?;
            let layer = s.layers_mut().remove(from);
            s.layers_mut().insert(*index, layer);
            Ok(Operation::ReorderLayer {
                target: target.clone(),
                index: from,
            })
        }
        _ => unreachable!("dispatched by apply_one"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CellStyle, Grapheme, Size};
    use crate::id;

    fn doc_with_panel() -> Document {
        let mut doc = Document::new(Size::new(80, 24)).unwrap();
        let panel = Component::new(id!("metrics"), "panel")
            .with_prop("title", "Metrics")
            .with_layout(Layout::absolute(25, 3, 40, 10))
            .with_children(vec![
                Component::new(id!("cpu"), "label").with_prop("text", "CPU"),
            ]);
        Patch::single(Operation::CreateComponent {
            component: panel,
            parent: None,
            layer: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        doc
    }

    #[test]
    fn example_patch_from_spec_parses() {
        let json = r#"{"operations":[
            {"op":"move","target":"sidebar","x":2,"y":0},
            {"op":"resize","target":"metrics","width":48,"height":12},
            {"op":"set_property","target":"metrics","property":"border","value":"rounded"}
        ]}"#;
        let p: Patch = serde_json::from_str(json).unwrap();
        assert_eq!(p.operations.len(), 3);
        assert_eq!(p.operations[0].name(), "move");
        assert_eq!(p.operations[2].target(), Some(&id!("metrics")));
    }

    #[test]
    fn move_resize_set_property_and_their_inverses() {
        let mut doc = doc_with_panel();
        let original = doc.clone();
        let patch = Patch::new(vec![
            Operation::Move {
                target: id!("metrics"),
                x: 2,
                y: 0,
            },
            Operation::Resize {
                target: id!("metrics"),
                width: 48,
                height: 12,
            },
            Operation::SetProperty {
                target: id!("metrics"),
                property: "border".into(),
                value: "double".into(),
            },
            Operation::SetProperty {
                target: id!("metrics"),
                property: "title".into(),
                value: Value::Null,
            },
        ]);
        let inverse = patch.apply(&mut doc).unwrap();
        let c = doc.component(&id!("metrics")).unwrap();
        assert_eq!(c.layout.placement, Placement::Absolute { x: 2, y: 0 });
        assert_eq!(c.layout.width, Dimension::Fixed(48));
        assert_eq!(c.prop_str("border"), Some("double"));
        assert!(c.prop("title").is_none());
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn failed_patch_rolls_back_everything() {
        let mut doc = doc_with_panel();
        let original = doc.clone();
        let patch = Patch::new(vec![
            Operation::Move {
                target: id!("metrics"),
                x: 0,
                y: 0,
            },
            Operation::Move {
                target: id!("ghost"),
                x: 0,
                y: 0,
            },
        ]);
        let err = patch.apply(&mut doc).unwrap_err();
        assert!(matches!(err, PatchError::Operation { index: 1, .. }));
        assert_eq!(doc, original);
    }

    #[test]
    fn create_and_delete_are_inverse_and_keep_position() {
        let mut doc = doc_with_panel();
        let original = doc.clone();
        let del = Patch::single(Operation::DeleteComponent { target: id!("cpu") });
        let inverse = del.apply(&mut doc).unwrap();
        assert!(doc.component(&id!("cpu")).is_none());
        match &inverse.operations[0] {
            Operation::CreateComponent { parent, index, .. } => {
                assert_eq!(parent, &Some(id!("metrics")));
                assert_eq!(index, &Some(0));
            }
            other => panic!("unexpected inverse {other:?}"),
        }
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn duplicate_id_on_create_is_rejected() {
        let mut doc = doc_with_panel();
        let patch = Patch::single(Operation::CreateComponent {
            component: Component::new(id!("cpu"), "label"),
            parent: None,
            layer: None,
            index: None,
        });
        assert!(patch.apply(&mut doc).is_err());
    }

    #[test]
    fn set_cells_writes_and_inverts_including_wide_glyph_repair() {
        let mut doc = Document::new(Size::new(6, 1)).unwrap();
        let original = doc.clone();
        let art = id!("main-artwork");
        let g = |s: &str| Cell::glyph(Grapheme::new(s).unwrap(), CellStyle::DEFAULT);
        let p1 = Patch::single(Operation::SetCells {
            layer: art.clone(),
            cells: vec![CellWrite {
                x: 0,
                y: 0,
                cell: g("漢"),
            }],
        });
        let inv1 = p1.apply(&mut doc).unwrap();
        let p2 = Patch::single(Operation::SetCells {
            layer: art.clone(),
            cells: vec![CellWrite {
                x: 1,
                y: 0,
                cell: g("b"),
            }],
        });
        let inv2 = p2.apply(&mut doc).unwrap();
        let canvas = doc.layer(&art).unwrap().cells().unwrap();
        assert!(
            canvas
                .get(Position::new(0, 0))
                .unwrap()
                .as_glyph()
                .unwrap()
                .as_str()
                == " "
        );
        inv2.apply(&mut doc).unwrap();
        inv1.apply(&mut doc).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn set_cells_out_of_bounds_fails_atomically() {
        let mut doc = Document::new(Size::new(3, 1)).unwrap();
        let original = doc.clone();
        let g = Cell::glyph(Grapheme::new("x").unwrap(), CellStyle::DEFAULT);
        let p = Patch::single(Operation::SetCells {
            layer: id!("main-artwork"),
            cells: vec![
                CellWrite {
                    x: 0,
                    y: 0,
                    cell: g.clone(),
                },
                CellWrite {
                    x: 9,
                    y: 0,
                    cell: g,
                },
            ],
        });
        assert!(p.apply(&mut doc).is_err());
        assert_eq!(doc, original);
    }

    #[test]
    fn locked_layer_blocks_component_and_cell_edits() {
        let mut doc = doc_with_panel();
        Patch::single(Operation::UpdateLayer {
            target: id!("main-ui"),
            name: None,
            visible: None,
            locked: Some(true),
        })
        .apply(&mut doc)
        .unwrap();
        let err = Patch::single(Operation::Move {
            target: id!("metrics"),
            x: 1,
            y: 1,
        })
        .apply(&mut doc)
        .unwrap_err();
        assert!(err.to_string().contains("locked"), "{err}");
    }

    #[test]
    fn layer_operations_and_inverses() {
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        let original = doc.clone();
        let patch = Patch::new(vec![
            Operation::CreateLayer {
                layer: Layer::interface(id!("overlay"), "Overlay"),
                screen: None,
                index: None,
            },
            Operation::ReorderLayer {
                target: id!("overlay"),
                index: 0,
            },
            Operation::UpdateLayer {
                target: id!("overlay"),
                name: Some("Bottom".into()),
                visible: Some(false),
                locked: None,
            },
        ]);
        let inverse = patch.apply(&mut doc).unwrap();
        let s = doc.first_screen();
        assert_eq!(s.layers()[0].id, id!("overlay"));
        assert_eq!(s.layers()[0].name, "Bottom");
        assert!(!s.layers()[0].visible);
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc, original);
        // Deleting the last layer is refused.
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        Patch::single(Operation::DeleteLayer {
            target: id!("main-ui"),
        })
        .apply(&mut doc)
        .unwrap();
        assert!(
            Patch::single(Operation::DeleteLayer {
                target: id!("main-artwork")
            })
            .apply(&mut doc)
            .is_err()
        );
    }

    #[test]
    fn set_theme_is_invertible() {
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        let inverse = Patch::single(Operation::SetTheme {
            theme: Theme::minimal_dark(),
        })
        .apply(&mut doc)
        .unwrap();
        assert_eq!(doc.theme.id, id!("minimal-dark"));
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc.theme.id, id!("terminal"));
    }

    #[test]
    fn set_size_touches_one_axis_only() {
        let mut doc = doc_with_panel();
        Patch::single(Operation::SetLayout {
            target: id!("metrics"),
            layout: Layout {
                width: Dimension::Fixed(10),
                height: Dimension::Fill,
                ..Layout::default()
            },
        })
        .apply(&mut doc)
        .unwrap();
        let inverse = Patch::single(Operation::SetSize {
            target: id!("metrics"),
            width: Some(Dimension::Fixed(24)),
            height: None,
        })
        .apply(&mut doc)
        .unwrap();
        let layout = doc.component(&id!("metrics")).unwrap().layout;
        assert_eq!(layout.width, Dimension::Fixed(24));
        assert_eq!(
            layout.height,
            Dimension::Fill,
            "the other axis is untouched"
        );
        inverse.apply(&mut doc).unwrap();
        assert_eq!(
            doc.component(&id!("metrics")).unwrap().layout.width,
            Dimension::Fixed(10)
        );
    }

    #[test]
    fn reparent_moves_a_component_and_inverts() {
        let mut doc = doc_with_panel();
        Patch::single(Operation::CreateComponent {
            component: Component::new(id!("other"), "panel"),
            parent: None,
            layer: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        let original = doc.clone();
        let inverse = Patch::single(Operation::Reparent {
            target: id!("cpu"),
            parent: Some(id!("other")),
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        assert!(doc.component(&id!("metrics")).unwrap().children.is_empty());
        assert_eq!(
            doc.component(&id!("other")).unwrap().children[0].id,
            id!("cpu")
        );
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn reparent_to_roots_and_index_clamping() {
        let mut doc = doc_with_panel();
        Patch::single(Operation::Reparent {
            target: id!("cpu"),
            parent: None,
            index: Some(99),
        })
        .apply(&mut doc)
        .unwrap();
        let roots = doc.layer(&id!("main-ui")).unwrap().components().unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[1].id, id!("cpu"));
    }

    #[test]
    fn reparent_rejects_cycles_and_foreign_parents() {
        let mut doc = doc_with_panel();
        let err = Patch::single(Operation::Reparent {
            target: id!("metrics"),
            parent: Some(id!("cpu")),
            index: None,
        })
        .apply(&mut doc)
        .unwrap_err();
        assert!(
            err.to_string().contains("cannot become the parent"),
            "{err}"
        );
        let err = Patch::single(Operation::Reparent {
            target: id!("metrics"),
            parent: Some(id!("metrics")),
            index: None,
        })
        .apply(&mut doc)
        .unwrap_err();
        assert!(
            err.to_string().contains("cannot become the parent"),
            "{err}"
        );
        let err = Patch::single(Operation::Reparent {
            target: id!("cpu"),
            parent: Some(id!("ghost")),
            index: None,
        })
        .apply(&mut doc)
        .unwrap_err();
        assert!(err.to_string().contains("not on the same layer"), "{err}");
    }

    #[test]
    fn patch_serde_round_trip() {
        let p = Patch::new(vec![
            Operation::SetLayout {
                target: id!("a"),
                layout: Layout::absolute(1, 2, 3, 4),
            },
            Operation::UpdateLayer {
                target: id!("l"),
                name: None,
                visible: Some(true),
                locked: None,
            },
        ]);
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"op\":\"set_layout\""));
        assert!(
            !json.contains("\"name\""),
            "absent options are omitted: {json}"
        );
        let back: Patch = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
