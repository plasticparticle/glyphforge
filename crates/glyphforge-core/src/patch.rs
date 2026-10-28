//! Semantic patches: the one mutation mechanism for humans and agents.
//!
//! A patch is a list of operations addressed by object id. Applying a patch
//! validates and applies every operation in order and returns the inverse
//! patch; if any operation fails, the ones already applied are rolled back
//! so a patch is transactional. Inverse patches are what undo replays.

use serde::{Deserialize, Serialize};

use crate::component::{Component, tree};
use crate::document::{
    Cell, CellChange, Document, DocumentError, Layer, LayerKind, Position, Screen, Size,
};
use crate::id::ObjectId;
use crate::layout::{Dimension, Layout, Placement};
use crate::raster::draw::PixelWrite;
use crate::raster::{Dither, DropShadow, Gradient, PaletteLimit, RasterLayer, RasterMode};
use crate::theme::Theme;
use crate::value::Value;

/// A drop shadow change in [`Operation::UpdateRaster`]: `"off"` or
/// `{"set": {...}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShadowUpdate {
    Off,
    Set(DropShadow),
}

impl ShadowUpdate {
    pub const fn from_option(shadow: Option<DropShadow>) -> Self {
        match shadow {
            Some(s) => Self::Set(s),
            None => Self::Off,
        }
    }

    pub const fn to_option(self) -> Option<DropShadow> {
        match self {
            Self::Off => None,
            Self::Set(s) => Some(s),
        }
    }
}

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
    /// Replaces the whole layout of a component.
    SetLayout {
        target: ObjectId,
        layout: Layout,
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
    /// Moves a component (with its children) under another parent, or to
    /// the roots of `layer` (default: the layer it is on), at `index`
    /// (default: last). Its layout is kept.
    Reparent {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<ObjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Writes cells on an artwork layer.
    SetCells {
        layer: ObjectId,
        cells: Vec<CellWrite>,
    },
    /// Writes pixels on a raster layer.
    SetPixels {
        layer: ObjectId,
        pixels: Vec<PixelWrite>,
    },
    /// Changes the render settings of a raster layer. Changing the mode
    /// resamples the pixels.
    UpdateRaster {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<RasterMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dither: Option<Dither>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        palette: Option<PaletteLimit>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shadow: Option<ShadowUpdate>,
    },
    /// Replaces the whole content of a raster layer (the inverse of a mode
    /// change).
    ReplaceRaster {
        target: ObjectId,
        raster: RasterLayer,
    },
    /// Adds a gradient, or replaces the one with the same id.
    SetGradient {
        gradient: Gradient,
    },
    DeleteGradient {
        target: ObjectId,
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
    /// Adds a screen (with its layers) at `index` (default: last).
    CreateScreen {
        screen: Screen,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    DeleteScreen {
        target: ObjectId,
    },
    /// Renames and/or resizes a screen. Resizing crops or extends every
    /// layer; its inverse restores the whole screen, so nothing cropped
    /// is lost to undo.
    UpdateScreen {
        target: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u16>,
    },
    /// Replaces a screen with the same id wholesale (the inverse of a
    /// resize).
    ReplaceScreen {
        screen: Screen,
    },
    /// Sets a data fixture (`Null` removes it).
    SetData {
        key: String,
        value: Value,
    },
    /// Adds a symbol, or replaces the one with the same id (every
    /// instance follows).
    SetSymbol {
        symbol: crate::symbol::Symbol,
    },
    DeleteSymbol {
        target: ObjectId,
    },
    /// Adds or replaces a glyph palette (empty glyphs remove it).
    SetPalette {
        name: String,
        glyphs: String,
    },
}

impl Operation {
    /// The object this operation addresses, for diagnostics.
    pub fn target(&self) -> Option<&ObjectId> {
        match self {
            Self::Move { target, .. }
            | Self::Resize { target, .. }
            | Self::SetProperty { target, .. }
            | Self::SetLayout { target, .. }
            | Self::DeleteComponent { target }
            | Self::Reparent { target, .. }
            | Self::DeleteLayer { target }
            | Self::UpdateLayer { target, .. }
            | Self::ReorderLayer { target, .. }
            | Self::UpdateRaster { target, .. }
            | Self::ReplaceRaster { target, .. }
            | Self::DeleteGradient { target }
            | Self::DeleteScreen { target }
            | Self::UpdateScreen { target, .. }
            | Self::DeleteSymbol { target } => Some(target),
            Self::SetSymbol { symbol } => Some(&symbol.id),
            Self::SetGradient { gradient } => Some(&gradient.id),
            Self::CreateComponent { component, .. } => Some(&component.id),
            Self::SetCells { layer, .. } | Self::SetPixels { layer, .. } => Some(layer),
            Self::CreateLayer { layer, .. } => Some(&layer.id),
            Self::SetTheme { .. } | Self::SetData { .. } | Self::SetPalette { .. } => None,
            Self::CreateScreen { screen, .. } | Self::ReplaceScreen { screen } => Some(&screen.id),
        }
    }

    /// A one-line, human-readable description for reviews and logs.
    pub fn summary(&self) -> String {
        let opt = |o: &Option<ObjectId>| {
            o.as_ref()
                .map_or_else(|| "top level".to_owned(), ToString::to_string)
        };
        match self {
            Self::Move { target, x, y } => format!("move {target} to {x},{y}"),
            Self::Resize {
                target,
                width,
                height,
            } => format!("resize {target} to {width}×{height}"),
            Self::SetProperty {
                target,
                property,
                value,
            } if value.is_null() => format!("remove {target}.{property}"),
            Self::SetProperty {
                target,
                property,
                value,
            } => format!(
                "set {target}.{property} = {}",
                serde_json::to_string(value).unwrap_or_default()
            ),
            Self::SetLayout { target, layout } => {
                format!("layout of {target}: {} × {}", layout.width, layout.height)
            }
            Self::CreateComponent {
                component, parent, ..
            } => format!(
                "add {} {} ({} nodes) under {}",
                component.kind,
                component.id,
                component.iter().count(),
                opt(parent)
            ),
            Self::DeleteComponent { target } => format!("delete {target}"),
            Self::Reparent { target, parent, .. } => {
                format!("move {target} into {}", opt(parent))
            }
            Self::SetCells { layer, cells } => format!("paint {} cells on {layer}", cells.len()),
            Self::SetPixels { layer, pixels } => {
                format!("paint {} pixels on {layer}", pixels.len())
            }
            Self::UpdateRaster { target, .. } => format!("change raster settings of {target}"),
            Self::ReplaceRaster { target, .. } => format!("replace raster {target}"),
            Self::SetGradient { gradient } => format!("set gradient {}", gradient.id),
            Self::DeleteGradient { target } => format!("delete gradient {target}"),
            Self::CreateLayer { layer, .. } => {
                format!("add {} layer {}", layer.kind().name(), layer.id)
            }
            Self::DeleteLayer { target } => format!("delete layer {target}"),
            Self::UpdateLayer {
                target,
                name,
                visible,
                locked,
            } => {
                let mut parts = Vec::new();
                if let Some(n) = name {
                    parts.push(format!("rename to {n:?}"));
                }
                if let Some(v) = visible {
                    parts.push(if *v { "show" } else { "hide" }.to_owned());
                }
                if let Some(l) = locked {
                    parts.push(if *l { "lock" } else { "unlock" }.to_owned());
                }
                format!("layer {target}: {}", parts.join(", "))
            }
            Self::ReorderLayer { target, index } => format!("move layer {target} to {index}"),
            Self::SetTheme { theme } => format!("use theme {}", theme.name),
            Self::CreateScreen { screen, .. } => format!("add screen {}", screen.id),
            Self::DeleteScreen { target } => format!("delete screen {target}"),
            Self::UpdateScreen {
                target,
                name,
                width,
                height,
            } => {
                let mut parts = Vec::new();
                if let Some(n) = name {
                    parts.push(format!("rename to {n:?}"));
                }
                if width.is_some() || height.is_some() {
                    parts.push(format!(
                        "resize to {}×{}",
                        width.map_or_else(|| "·".to_owned(), |w| w.to_string()),
                        height.map_or_else(|| "·".to_owned(), |h| h.to_string())
                    ));
                }
                format!("screen {target}: {}", parts.join(", "))
            }
            Self::ReplaceScreen { screen } => format!("replace screen {}", screen.id),
            Self::SetData { key, value } if value.is_null() => format!("remove data {key}"),
            Self::SetData { key, .. } => format!("set data {key}"),
            Self::SetSymbol { symbol } => format!("set symbol {}", symbol.id),
            Self::DeleteSymbol { target } => format!("delete symbol {target}"),
            Self::SetPalette { name, glyphs } if glyphs.is_empty() => {
                format!("remove palette {name:?}")
            }
            Self::SetPalette { name, .. } => format!("set palette {name:?}"),
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Move { .. } => "move",
            Self::Resize { .. } => "resize",
            Self::SetProperty { .. } => "set_property",
            Self::SetLayout { .. } => "set_layout",
            Self::CreateComponent { .. } => "create_component",
            Self::DeleteComponent { .. } => "delete_component",
            Self::Reparent { .. } => "reparent",
            Self::CreateScreen { .. } => "create_screen",
            Self::DeleteScreen { .. } => "delete_screen",
            Self::UpdateScreen { .. } => "update_screen",
            Self::ReplaceScreen { .. } => "replace_screen",
            Self::SetData { .. } => "set_data",
            Self::SetSymbol { .. } => "set_symbol",
            Self::DeleteSymbol { .. } => "delete_symbol",
            Self::SetPalette { .. } => "set_palette",
            Self::SetCells { .. } => "set_cells",
            Self::SetPixels { .. } => "set_pixels",
            Self::UpdateRaster { .. } => "update_raster",
            Self::ReplaceRaster { .. } => "replace_raster",
            Self::SetGradient { .. } => "set_gradient",
            Self::DeleteGradient { .. } => "delete_gradient",
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
    #[error("pixel ({x}, {y}) is outside raster layer {layer}")]
    PixelOutOfBounds { layer: ObjectId, x: u16, y: u16 },
    #[error("{0} is not a gradient")]
    NotAGradient(ObjectId),
    #[error("{0} is not a symbol")]
    NotASymbol(ObjectId),
    #[error("cannot move {target} into {parent}: it is inside {target}")]
    Cycle { target: ObjectId, parent: ObjectId },
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
        | Operation::SetLayout { .. }
        | Operation::SetProperty { .. }
        | Operation::CreateComponent { .. }
        | Operation::DeleteComponent { .. } => apply_component_op(doc, op),
        Operation::Reparent {
            target,
            parent,
            layer,
            index,
        } => apply_reparent(doc, target, parent.as_ref(), layer.as_ref(), *index),
        Operation::SetCells { layer, cells } => apply_set_cells(doc, layer, cells),
        Operation::SetPixels { .. }
        | Operation::UpdateRaster { .. }
        | Operation::ReplaceRaster { .. }
        | Operation::SetGradient { .. }
        | Operation::DeleteGradient { .. } => apply_raster_op(doc, op),
        Operation::CreateLayer { .. }
        | Operation::DeleteLayer { .. }
        | Operation::UpdateLayer { .. }
        | Operation::ReorderLayer { .. } => apply_layer_op(doc, op),
        Operation::SetTheme { theme } => {
            let before = std::mem::replace(&mut doc.theme, theme.clone());
            Ok(Operation::SetTheme { theme: before })
        }
        Operation::SetData { key, value } => {
            let before = if value.is_null() {
                doc.data.remove(key)
            } else {
                doc.data.insert(key.clone(), value.clone())
            };
            Ok(Operation::SetData {
                key: key.clone(),
                value: before.unwrap_or(Value::Null),
            })
        }
        Operation::SetSymbol { symbol } => {
            let existing = doc.symbols.iter().position(|s| s.id == symbol.id);
            if existing.is_none() && doc.has_id(&symbol.id) {
                return Err(DocumentError::DuplicateId(symbol.id.clone()).into());
            }
            Ok(if let Some(i) = existing {
                Operation::SetSymbol {
                    symbol: std::mem::replace(&mut doc.symbols[i], symbol.clone()),
                }
            } else {
                doc.symbols.push(symbol.clone());
                Operation::DeleteSymbol {
                    target: symbol.id.clone(),
                }
            })
        }
        Operation::DeleteSymbol { target } => {
            let i = doc
                .symbols
                .iter()
                .position(|s| &s.id == target)
                .ok_or_else(|| OperationError::NotASymbol(target.clone()))?;
            Ok(Operation::SetSymbol {
                symbol: doc.symbols.remove(i),
            })
        }
        Operation::SetPalette { name, glyphs } => {
            let existing = doc.palettes.iter().position(|p| &p.name == name);
            let before = existing
                .map(|i| doc.palettes[i].glyphs.clone())
                .unwrap_or_default();
            match (existing, glyphs.is_empty()) {
                (Some(i), true) => {
                    doc.palettes.remove(i);
                }
                (Some(i), false) => glyphs.clone_into(&mut doc.palettes[i].glyphs),
                (None, false) => doc.palettes.push(crate::symbol::GlyphPalette {
                    name: name.clone(),
                    glyphs: glyphs.clone(),
                }),
                (None, true) => {}
            }
            Ok(Operation::SetPalette {
                name: name.clone(),
                glyphs: before,
            })
        }
        Operation::CreateScreen { .. }
        | Operation::DeleteScreen { .. }
        | Operation::UpdateScreen { .. }
        | Operation::ReplaceScreen { .. } => apply_screen_op(doc, op),
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

fn interface_roots<'a>(
    doc: &'a mut Document,
    layer: &ObjectId,
) -> Result<&'a mut Vec<crate::component::Component>, OperationError> {
    let l = doc
        .layer_mut(layer)
        .ok_or_else(|| OperationError::NotALayer(layer.clone()))?;
    if l.locked {
        return Err(DocumentError::LayerLocked(layer.clone()).into());
    }
    let kind = l.kind();
    l.components_mut().ok_or_else(|| {
        DocumentError::WrongLayerKind {
            expected: "interface",
            actual: kind.name(),
        }
        .into()
    })
}

fn apply_reparent(
    doc: &mut Document,
    target: &ObjectId,
    parent: Option<&ObjectId>,
    layer: Option<&ObjectId>,
    index: Option<usize>,
) -> Result<Operation, OperationError> {
    let from_layer = doc
        .layer_of_component(target)
        .map(|l| l.id.clone())
        .ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    let to_layer = match (parent, layer) {
        (Some(p), _) => doc
            .layer_of_component(p)
            .map(|l| l.id.clone())
            .ok_or_else(|| OperationError::NotAComponent(p.clone()))?,
        (None, Some(l)) => l.clone(),
        (None, None) => from_layer.clone(),
    };
    if let Some(p) = parent {
        let inside = doc
            .component(target)
            .is_some_and(|c| c.iter().any(|d| &d.id == p));
        if inside {
            return Err(OperationError::Cycle {
                target: target.clone(),
                parent: p.clone(),
            });
        }
    }
    // Check the destination before removing anything.
    interface_roots(doc, &to_layer)?;
    let roots = interface_roots(doc, &from_layer)?;
    let (old_parent, old_index) =
        tree::locate(roots, target).ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    let removed =
        tree::remove(roots, target).ok_or_else(|| OperationError::NotAComponent(target.clone()))?;
    let roots = interface_roots(doc, &to_layer)?;
    if !tree::insert(roots, parent, index.unwrap_or(usize::MAX), removed) {
        return Err(OperationError::NotAComponent(
            parent.cloned().unwrap_or(to_layer),
        ));
    }
    Ok(Operation::Reparent {
        target: target.clone(),
        parent: old_parent,
        layer: Some(from_layer),
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

fn raster_mut<'a>(
    doc: &'a mut Document,
    id: &ObjectId,
) -> Result<&'a mut RasterLayer, OperationError> {
    let layer = doc
        .layer_mut(id)
        .ok_or_else(|| OperationError::NotALayer(id.clone()))?;
    if layer.locked {
        return Err(DocumentError::LayerLocked(id.clone()).into());
    }
    let actual = layer.kind().name();
    layer.raster_mut().ok_or(
        DocumentError::WrongLayerKind {
            expected: "raster",
            actual,
        }
        .into(),
    )
}

fn apply_raster_op(doc: &mut Document, op: &Operation) -> Result<Operation, OperationError> {
    match op {
        Operation::SetPixels { layer, pixels } => {
            let raster = raster_mut(doc, layer)?;
            let mut before: Vec<PixelWrite> = Vec::with_capacity(pixels.len());
            for w in pixels {
                if let Some(old) = raster.pixels.set(w.x, w.y, w.pixel) {
                    before.push(PixelWrite { pixel: old, ..*w });
                } else {
                    for b in before.iter().rev() {
                        raster.pixels.set(b.x, b.y, b.pixel);
                    }
                    return Err(OperationError::PixelOutOfBounds {
                        layer: layer.clone(),
                        x: w.x,
                        y: w.y,
                    });
                }
            }
            before.reverse();
            Ok(Operation::SetPixels {
                layer: layer.clone(),
                pixels: before,
            })
        }
        Operation::UpdateRaster {
            target,
            mode,
            dither,
            palette,
            shadow,
        } => {
            let raster = raster_mut(doc, target)?;
            let inverse = if mode.is_some_and(|m| m != raster.mode) {
                Operation::ReplaceRaster {
                    target: target.clone(),
                    raster: raster.clone(),
                }
            } else {
                Operation::UpdateRaster {
                    target: target.clone(),
                    mode: None,
                    dither: dither.map(|_| raster.dither),
                    palette: palette.map(|_| raster.palette),
                    shadow: shadow.map(|_| ShadowUpdate::from_option(raster.shadow)),
                }
            };
            if let Some(m) = mode {
                raster.set_mode(*m);
            }
            if let Some(d) = dither {
                raster.dither = *d;
            }
            if let Some(p) = palette {
                raster.palette = *p;
            }
            if let Some(s) = shadow {
                raster.shadow = s.to_option();
            }
            Ok(inverse)
        }
        Operation::ReplaceRaster { target, raster } => {
            let current = raster_mut(doc, target)?;
            let before = std::mem::replace(current, raster.clone());
            Ok(Operation::ReplaceRaster {
                target: target.clone(),
                raster: before,
            })
        }
        Operation::SetGradient { gradient } => {
            if let Some(existing) = doc.gradients.iter_mut().find(|g| g.id == gradient.id) {
                let before = std::mem::replace(existing, gradient.clone());
                return Ok(Operation::SetGradient { gradient: before });
            }
            if doc.has_id(&gradient.id) {
                return Err(DocumentError::DuplicateId(gradient.id.clone()).into());
            }
            doc.gradients.push(gradient.clone());
            Ok(Operation::DeleteGradient {
                target: gradient.id.clone(),
            })
        }
        Operation::DeleteGradient { target } => {
            let index = doc
                .gradients
                .iter()
                .position(|g| &g.id == target)
                .ok_or_else(|| OperationError::NotAGradient(target.clone()))?;
            let removed = doc.gradients.remove(index);
            Ok(Operation::SetGradient { gradient: removed })
        }
        _ => unreachable!("dispatched by apply_one"),
    }
}

fn apply_screen_op(doc: &mut Document, op: &Operation) -> Result<Operation, OperationError> {
    let index_of = |doc: &Document, id: &ObjectId| {
        doc.screens()
            .iter()
            .position(|s| &s.id == id)
            .ok_or_else(|| OperationError::NotAScreen(id.clone()))
    };
    match op {
        Operation::CreateScreen { screen, index } => {
            let mut probe = doc.clone();
            probe.add_screen(screen.clone())?;
            if let Some(dup) = probe.duplicate_ids().into_iter().next() {
                return Err(DocumentError::DuplicateId(dup).into());
            }
            let max = doc.screens().len();
            let idx = index.unwrap_or(max);
            if idx > max {
                return Err(OperationError::IndexOutOfRange { index: idx, max });
            }
            doc.screens_mut().insert(idx, screen.clone());
            Ok(Operation::DeleteScreen {
                target: screen.id.clone(),
            })
        }
        Operation::DeleteScreen { target } => {
            let i = index_of(doc, target)?;
            if doc.screens().len() == 1 {
                return Err(DocumentError::LastScreen.into());
            }
            let removed = doc.screens_mut().remove(i);
            Ok(Operation::CreateScreen {
                screen: removed,
                index: Some(i),
            })
        }
        Operation::UpdateScreen {
            target,
            name,
            width,
            height,
        } => {
            let i = index_of(doc, target)?;
            let before = doc.screens()[i].clone();
            let screen = &mut doc.screens_mut()[i];
            if let Some(n) = name {
                screen.name.clone_from(n);
            }
            if width.is_some() || height.is_some() {
                let size = Size::new(
                    width.unwrap_or(before.size().width),
                    height.unwrap_or(before.size().height),
                );
                screen.resize(size).map_err(DocumentError::from)?;
                return Ok(Operation::ReplaceScreen { screen: before });
            }
            Ok(Operation::UpdateScreen {
                target: target.clone(),
                name: name.as_ref().map(|_| before.name.clone()),
                width: None,
                height: None,
            })
        }
        Operation::ReplaceScreen { screen } => {
            let i = index_of(doc, &screen.id)?;
            let before = std::mem::replace(&mut doc.screens_mut()[i], screen.clone());
            if let Some(dup) = doc.duplicate_ids().into_iter().next() {
                doc.screens_mut()[i] = before;
                return Err(DocumentError::DuplicateId(dup).into());
            }
            Ok(Operation::ReplaceScreen { screen: before })
        }
        _ => unreachable!("dispatched by apply_one"),
    }
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
        assert_eq!(
            canvas
                .get(Position::new(0, 0))
                .unwrap()
                .as_glyph()
                .unwrap()
                .as_str(),
            " "
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

    #[test]
    fn raster_operations_apply_and_invert() {
        use crate::document::Color;
        use crate::raster::{GradientStop, Pixel};
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        let layer = Layer::raster_layer(id!("px"), "Pixels", Size::new(1, 1), RasterMode::Half);
        Patch::single(Operation::CreateLayer {
            layer,
            screen: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        let px = |doc: &Document, x, y| {
            doc.layer(&id!("px"))
                .unwrap()
                .raster()
                .unwrap()
                .pixels
                .get(x, y)
        };
        assert_eq!(
            doc.layer(&id!("px")).unwrap().raster().unwrap().cells(),
            Size::new(4, 2),
            "sized to the screen"
        );

        let red = Pixel::Solid(Color::Indexed(1));
        let set = Patch::single(Operation::SetPixels {
            layer: id!("px"),
            pixels: vec![PixelWrite {
                x: 3,
                y: 1,
                pixel: red,
            }],
        });
        let undo = set.apply(&mut doc).unwrap();
        assert_eq!(px(&doc, 3, 1), Some(red));
        undo.apply(&mut doc).unwrap();
        assert_eq!(px(&doc, 3, 1), Some(Pixel::Clear));

        let out = Patch::single(Operation::SetPixels {
            layer: id!("px"),
            pixels: vec![
                PixelWrite {
                    x: 0,
                    y: 0,
                    pixel: red,
                },
                PixelWrite {
                    x: 99,
                    y: 0,
                    pixel: red,
                },
            ],
        })
        .apply(&mut doc);
        assert!(out.is_err());
        assert_eq!(px(&doc, 0, 0), Some(Pixel::Clear), "rolled back");

        set.apply(&mut doc).unwrap();
        let before = doc.clone();
        let mode = Patch::single(Operation::UpdateRaster {
            target: id!("px"),
            mode: Some(RasterMode::Sextant),
            dither: Some(Dither::Shade),
            palette: None,
            shadow: Some(ShadowUpdate::Set(DropShadow::default())),
        });
        let undo = mode.apply(&mut doc).unwrap();
        let raster = doc.layer(&id!("px")).unwrap().raster().unwrap();
        assert_eq!(
            (raster.mode, raster.dither),
            (RasterMode::Sextant, Dither::Shade)
        );
        assert!(raster.shadow.is_some());
        undo.apply(&mut doc).unwrap();
        assert_eq!(doc, before, "a mode change undoes losslessly");
        let json = serde_json::to_string(&mode).unwrap();
        assert!(
            json.contains("\"op\":\"update_raster\"") && json.contains("\"set\""),
            "{json}"
        );

        let wrong = Patch::single(Operation::SetPixels {
            layer: id!("main-artwork"),
            pixels: vec![],
        });
        assert!(wrong.apply(&mut doc).is_err());

        let g = Gradient::new(
            id!("sunset"),
            "Sunset",
            vec![
                GradientStop {
                    at: 0,
                    color: Color::Indexed(1),
                },
                GradientStop {
                    at: 100,
                    color: Color::Indexed(3),
                },
            ],
        )
        .unwrap();
        let undo = Patch::single(Operation::SetGradient {
            gradient: g.clone(),
        })
        .apply(&mut doc)
        .unwrap();
        assert!(doc.has_id(&id!("sunset")));
        assert!(
            Patch::single(Operation::SetGradient {
                gradient: Gradient {
                    id: id!("px"),
                    ..g.clone()
                }
            })
            .apply(&mut doc)
            .is_err(),
            "gradient ids are document ids"
        );
        undo.apply(&mut doc).unwrap();
        assert!(doc.gradient(&id!("sunset")).is_none());
    }

    #[test]
    fn reparent_moves_subtree_and_inverts() {
        let mut doc = Document::new(Size::new(40, 10)).unwrap();
        let root = |id: &str| Component::new(ObjectId::new(id).unwrap(), "group");
        let setup = Patch::new(vec![
            Operation::CreateComponent {
                component: root("a").with_children(vec![root("a1")]),
                parent: None,
                layer: Some(id!("main-ui")),
                index: None,
            },
            Operation::CreateComponent {
                component: root("b"),
                parent: None,
                layer: Some(id!("main-ui")),
                index: None,
            },
        ]);
        setup.apply(&mut doc).unwrap();
        let before = doc.clone();
        let inverse = Patch::single(Operation::Reparent {
            target: id!("a"),
            parent: Some(id!("b")),
            layer: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        let roots = doc.layer(&id!("main-ui")).unwrap().components().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].children[0].id, id!("a"));
        assert_eq!(roots[0].children[0].children[0].id, id!("a1"));
        inverse.apply(&mut doc).unwrap();
        assert_eq!(doc, before, "inverse restores order and parent");
        // A component cannot move into its own subtree.
        let err = Patch::single(Operation::Reparent {
            target: id!("a"),
            parent: Some(id!("a1")),
            layer: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap_err();
        assert!(err.to_string().contains("inside"), "{err}");
        assert_eq!(doc, before);
    }

    #[test]
    fn screens_create_rename_resize_delete_and_invert() {
        let mut doc = Document::new(Size::new(10, 4)).unwrap();
        let original = doc.clone();
        let screen = Screen::new(id!("settings"), "Settings", Size::new(10, 4)).unwrap();
        let undo_create = Patch::single(Operation::CreateScreen {
            screen,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        assert_eq!(doc.screens().len(), 2);
        // Paint a cell that a resize will crop away.
        Patch::single(Operation::SetCells {
            layer: id!("settings-artwork"),
            cells: vec![CellWrite {
                x: 9,
                y: 3,
                cell: Cell::glyph(Grapheme::new("x").unwrap(), CellStyle::DEFAULT),
            }],
        })
        .apply(&mut doc)
        .unwrap();
        let before_resize = doc.clone();
        let undo_resize = Patch::single(Operation::UpdateScreen {
            target: id!("settings"),
            name: Some("Prefs".into()),
            width: Some(5),
            height: None,
        })
        .apply(&mut doc)
        .unwrap();
        let s = doc.screen(&id!("settings")).unwrap();
        assert_eq!((s.name.as_str(), s.size()), ("Prefs", Size::new(5, 4)));
        undo_resize.apply(&mut doc).unwrap();
        assert_eq!(doc, before_resize, "undo restores the cropped cell");
        // A duplicate id inside a new screen is refused.
        let clash = Screen::new(id!("other"), "Other", Size::new(4, 4)).unwrap();
        let mut clash = clash;
        clash.layers_mut()[0].id = id!("settings-ui");
        assert!(
            Patch::single(Operation::CreateScreen {
                screen: clash,
                index: None
            })
            .apply(&mut doc)
            .is_err()
        );
        let _ = undo_create;
        Patch::single(Operation::DeleteScreen {
            target: id!("settings"),
        })
        .apply(&mut doc)
        .unwrap();
        assert_eq!(doc, original);
        assert!(
            Patch::single(Operation::DeleteScreen {
                target: original.first_screen().id.clone()
            })
            .apply(&mut doc)
            .is_err(),
            "the last screen stays"
        );
    }

    #[test]
    fn summaries_name_targets() {
        assert_eq!(
            Operation::Move {
                target: id!("a"),
                x: 1,
                y: 2
            }
            .summary(),
            "move a to 1,2"
        );
        assert_eq!(
            Operation::SetProperty {
                target: id!("a"),
                property: "title".into(),
                value: "Hi".into()
            }
            .summary(),
            "set a.title = \"Hi\""
        );
    }

    #[test]
    fn data_fixtures_set_and_restore() {
        let mut doc = Document::new(Size::new(4, 2)).unwrap();
        let inverse = Patch::single(Operation::SetData {
            key: "rows".into(),
            value: Value::List(vec![Value::Int(1)]),
        })
        .apply(&mut doc)
        .unwrap();
        assert_eq!(doc.data["rows"], Value::List(vec![Value::Int(1)]));
        inverse.apply(&mut doc).unwrap();
        assert!(doc.data.is_empty());
    }
}
