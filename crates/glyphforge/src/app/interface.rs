//! Interface Mode: selecting, moving, resizing, aligning, reparenting,
//! creating and editing components. Every mutation is a patch operation
//! on the session, so undo, agents and the CLI all see the same thing.

use std::fmt::Write as _;

use glyphforge_core::component::tree;
use glyphforge_core::geometry::{Align, Axis, Guide, SnapTargets, snap};
use glyphforge_core::history::Origin;
use glyphforge_core::layout::{LayoutResult, Placement};
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::{ObjectId, Position, Rect};

use super::prompt::{Prompt, PromptKind, parse_property};
use super::{App, DesignMode, StatusKind, delta};
use crate::actions::Direction;

/// How far a dragged component may be pulled onto a neighbouring line.
const SNAP_THRESHOLD: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    Move,
    Resize,
}

/// One component taking part in a drag, with the geometry it started from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragTarget {
    pub id: ObjectId,
    /// Offset inside the parent when the drag began.
    pub offset: (u16, u16),
    pub size: (u16, u16),
    /// Screen rectangle when the drag began.
    pub rect: Rect,
}

/// An in-progress mouse drag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drag {
    pub mode: DragMode,
    /// Document position where the drag started.
    pub origin: Position,
    pub targets: Vec<DragTarget>,
    /// Lines to snap to, collected once when the drag began.
    pub snap_targets: SnapTargets,
    /// Guides matched by the latest motion, for the overlay.
    pub guides: Vec<Guide>,
}

impl App {
    pub fn design_mode(&self) -> DesignMode {
        if self.on_artwork_layer() {
            DesignMode::Subcell
        } else {
            DesignMode::Interface
        }
    }

    /// Solved layout of the active layer, cached per frame.
    pub fn layout(&mut self) -> &LayoutResult {
        if self.layout_cache.is_none() {
            let result = self
                .session
                .layout(&self.editor.screen, &self.editor.layer, None)
                .unwrap_or_default();
            self.layout_cache = Some(result);
        }
        self.layout_cache
            .as_ref()
            .unwrap_or_else(|| unreachable!("layout cache populated"))
    }

    /// The selected components, in the order they were selected.
    pub fn selection(&self) -> &[ObjectId] {
        &self.editor.selection
    }

    /// The component the inspector describes: the most recent of the
    /// selection.
    pub fn primary(&self) -> Option<&ObjectId> {
        self.editor.selection.last()
    }

    pub fn is_selected(&self, id: &ObjectId) -> bool {
        self.editor.selection.iter().any(|s| s == id)
    }

    /// Drops selected ids that no longer exist (after undo or delete).
    pub(super) fn prune_selection(&mut self) {
        if self.editor.selection.is_empty() {
            return;
        }
        let doc = self.session.document();
        self.editor
            .selection
            .retain(|id| doc.component(id).is_some());
    }

    /// Components of the active interface layer in document order.
    fn layer_components(&self) -> Vec<ObjectId> {
        self.active_layer()
            .and_then(|l| l.components())
            .map(|roots| tree::iter(roots).map(|c| c.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Replaces the selection and scrolls the first item into view.
    pub fn select(&mut self, ids: Vec<ObjectId>) {
        self.editor.selection = ids;
        self.reveal_primary();
    }

    /// Adds or removes one component from the selection.
    pub fn toggle_selected(&mut self, id: ObjectId) {
        match self.editor.selection.iter().position(|s| *s == id) {
            Some(i) => {
                self.editor.selection.remove(i);
            }
            None => self.editor.selection.push(id),
        }
        self.reveal_primary();
    }

    fn reveal_primary(&mut self) {
        if let Some(id) = self.primary().cloned() {
            if let Some(rect) = self.layout().rect(&id) {
                let size = self.doc_size();
                self.editor
                    .viewport
                    .ensure_visible(Position::new(rect.x, rect.y), size);
            }
        }
    }

    /// Cycles the selection through the layer's components.
    pub fn select_step(&mut self, forward: bool) {
        let ids = self.layer_components();
        if ids.is_empty() {
            self.set_status(
                StatusKind::Info,
                "No components on this layer; press a to add one",
            );
            return;
        }
        let n = ids.len();
        let next = match self.primary().and_then(|s| ids.iter().position(|i| i == s)) {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        let id = ids[next].clone();
        self.select(vec![id.clone()]);
        self.describe_selection(&id);
    }

    /// Adds the next component after the primary to the selection.
    pub fn extend_step(&mut self, forward: bool) {
        let ids = self.layer_components();
        if ids.is_empty() {
            return;
        }
        if self.editor.selection.is_empty() {
            self.select_step(forward);
            return;
        }
        let n = ids.len();
        let start = self
            .primary()
            .and_then(|s| ids.iter().position(|i| i == s))
            .unwrap_or(0);
        for step in 1..=n {
            let i = if forward {
                (start + step) % n
            } else {
                (start + n - step) % n
            };
            if !self.is_selected(&ids[i]) {
                let id = ids[i].clone();
                self.editor.selection.push(id);
                self.reveal_primary();
                self.report_selection_count();
                return;
            }
        }
        self.set_status(
            StatusKind::Info,
            "All components on this layer are selected",
        );
    }

    pub fn select_all(&mut self) {
        let ids = self.layer_components();
        if ids.is_empty() {
            self.set_status(StatusKind::Info, "No components on this layer");
            return;
        }
        self.select(ids);
        self.report_selection_count();
    }

    fn describe_selection(&mut self, id: &ObjectId) {
        let kind = self
            .doc()
            .component(id)
            .map(|c| c.kind.clone())
            .unwrap_or_default();
        self.set_status(StatusKind::Info, format!("Selected {id} ({kind})"));
    }

    fn report_selection_count(&mut self) {
        let n = self.editor.selection.len();
        self.set_status(StatusKind::Info, format!("{n} components selected"));
    }

    /// The top-most component whose rectangle contains `pos`.
    pub fn component_at(&mut self, pos: Position) -> Option<ObjectId> {
        let ids = self.layer_components();
        let layout = self.layout();
        ids.into_iter()
            .rev()
            .find(|id| layout.rect(id).is_some_and(|r| r.contains(pos)))
    }

    pub fn select_at(&mut self, pos: Position) {
        match self.component_at(pos) {
            Some(id) => self.select(vec![id]),
            None => self.select(Vec::new()),
        }
    }

    pub fn extend_at(&mut self, pos: Position) {
        if let Some(id) = self.component_at(pos) {
            self.toggle_selected(id);
            self.report_selection_count();
        }
    }

    /// Rect of a component's parent content area, or the screen.
    fn parent_inner(&mut self, id: &ObjectId) -> Rect {
        let parent = self
            .active_layer()
            .and_then(|l| l.components())
            .and_then(|roots| tree::locate(roots, id))
            .and_then(|(parent, _)| parent);
        let screen = Rect::from_size(self.doc_size());
        match parent {
            Some(p) => self.layout().inner(&p).unwrap_or(screen),
            None => screen,
        }
    }

    /// Offset of `id` inside its parent and its current size.
    fn geometry(&mut self, id: &ObjectId) -> Option<((u16, u16), (u16, u16))> {
        let rect = self.layout().rect(id)?;
        let inner = self.parent_inner(id);
        Some((
            (
                rect.x.saturating_sub(inner.x),
                rect.y.saturating_sub(inner.y),
            ),
            (rect.width, rect.height),
        ))
    }

    fn apply(&mut self, patch: Patch, label: &str) -> bool {
        match self.session.apply_patch(patch, label, Origin::User) {
            Ok(()) => {
                self.mark_edited();
                true
            }
            Err(e) => {
                self.report_error(&e);
                false
            }
        }
    }

    /// Nudges every selected component by one cell. Returns `false` when
    /// nothing is selected, so the caller can move the cursor instead.
    pub fn move_selection(&mut self, dir: Direction) -> bool {
        if self.editor.selection.is_empty() {
            return false;
        }
        let (dx, dy) = delta(dir);
        let mut ops = Vec::new();
        for id in self.editor.selection.clone() {
            let Some(((x, y), _)) = self.geometry(&id) else {
                continue;
            };
            let nx = (i32::from(x) + dx).max(0) as u16;
            let ny = (i32::from(y) + dy).max(0) as u16;
            if (nx, ny) != (x, y) {
                ops.push(Operation::Move {
                    target: id,
                    x: nx,
                    y: ny,
                });
            }
        }
        if !ops.is_empty() {
            self.apply(Patch::new(ops), "Move components");
        }
        true
    }

    pub fn resize_selection(&mut self, dir: Direction) -> bool {
        if self.editor.selection.is_empty() {
            return false;
        }
        let (dx, dy) = delta(dir);
        let mut ops = Vec::new();
        for id in self.editor.selection.clone() {
            let Some((_, (w, h))) = self.geometry(&id) else {
                continue;
            };
            let nw = (i32::from(w) + dx).max(1) as u16;
            let nh = (i32::from(h) + dy).max(1) as u16;
            if (nw, nh) != (w, h) {
                ops.push(Operation::Resize {
                    target: id,
                    width: nw,
                    height: nh,
                });
            }
        }
        if !ops.is_empty() {
            self.apply(Patch::new(ops), "Resize components");
        }
        true
    }

    pub fn delete_selection(&mut self) {
        if self.editor.selection.is_empty() {
            self.set_status(StatusKind::Info, "Nothing selected");
            return;
        }
        let ids = self.editor.selection.clone();
        let ops = ids
            .iter()
            .map(|id| Operation::DeleteComponent { target: id.clone() })
            .collect();
        let label = if ids.len() == 1 {
            format!("Deleted {}", ids[0])
        } else {
            format!("Deleted {} components", ids.len())
        };
        if self.apply(Patch::new(ops), "Delete components") {
            self.editor.selection.clear();
            self.set_status(StatusKind::Info, label);
        }
    }

    // --- geometry commands -------------------------------------------------

    pub fn align_selection(&mut self, mode: Align) {
        if !self.require_multi("align") {
            return;
        }
        let ids = self.editor.selection.clone();
        match self.session.align_components(&ids, mode) {
            Ok(outcome) => {
                self.mark_edited();
                let mut text = format!(
                    "Aligned {} of {} on {}",
                    outcome.changed,
                    ids.len(),
                    mode.label()
                );
                if !outcome.skipped.is_empty() {
                    let _ = write!(text, "; {} laid out by a parent", outcome.skipped.len());
                }
                self.set_status(StatusKind::Info, text);
            }
            Err(e) => self.report_error(&e),
        }
    }

    pub fn distribute_selection(&mut self, axis: Axis) {
        if !self.require_multi("distribute") {
            return;
        }
        let ids = self.editor.selection.clone();
        match self.session.distribute_components(&ids, axis) {
            Ok(outcome) if outcome.changed == 0 => self.set_status(
                StatusKind::Info,
                format!(
                    "Nothing to distribute {}: three or more free components that do not overlap are needed",
                    axis.name()
                ),
            ),
            Ok(outcome) => {
                self.mark_edited();
                self.set_status(
                    StatusKind::Info,
                    format!("Distributed {} components {}", outcome.changed, axis.name()),
                );
            }
            Err(e) => self.report_error(&e),
        }
    }

    pub fn equalize_selection(&mut self, axis: Axis) {
        if !self.require_multi("resize") {
            return;
        }
        let ids = self.editor.selection.clone();
        match self.session.equalize_components(&ids, axis) {
            Ok(outcome) => {
                self.mark_edited();
                let what = if axis == Axis::Horizontal {
                    "width"
                } else {
                    "height"
                };
                self.set_status(
                    StatusKind::Info,
                    format!("Matched {what} on {} components", outcome.changed),
                );
            }
            Err(e) => self.report_error(&e),
        }
    }

    fn require_multi(&mut self, verb: &str) -> bool {
        if self.editor.selection.len() >= 2 {
            return true;
        }
        self.set_status(
            StatusKind::Info,
            format!("Select at least two components to {verb} them (] [ cycle, {{ }} extend, Ctrl+A all)"),
        );
        false
    }

    /// Moves the selection into the component under the cursor, or to the
    /// layer's top level when the cursor is over empty space.
    pub fn reparent_selection(&mut self) {
        if self.editor.selection.is_empty() {
            self.set_status(StatusKind::Info, "Nothing selected");
            return;
        }
        let cursor = self.editor.cursor;
        let host = self.component_at(cursor).filter(|id| !self.is_selected(id));
        let ids = self.editor.selection.clone();
        match self.session.reparent_components(&ids, host.as_ref()) {
            Ok(()) => {
                self.mark_edited();
                let text = match &host {
                    Some(h) => format!("Moved {} into {h}", describe(&ids)),
                    None => format!("Moved {} to the top level", describe(&ids)),
                };
                self.set_status(StatusKind::Info, text);
            }
            Err(e) => self.report_error(&e),
        }
    }

    pub fn toggle_snap(&mut self) {
        self.snap_enabled = !self.snap_enabled;
        let state = if self.snap_enabled { "on" } else { "off" };
        self.set_status(StatusKind::Info, format!("Snapping {state}"));
    }

    // --- mouse drags ------------------------------------------------------

    /// Starts a drag at `pos` when it lands on the selection. The
    /// bottom-right corner of a single selected component resizes,
    /// anything else moves the whole selection.
    pub fn begin_drag(&mut self, pos: Position) -> bool {
        if self.editor.selection.is_empty() {
            return false;
        }
        let ids = self.editor.selection.clone();
        let hit = ids
            .iter()
            .filter_map(|id| self.layout().rect(id).map(|r| (id.clone(), r)))
            .find(|(_, r)| r.contains(pos));
        let Some((hit_id, hit_rect)) = hit else {
            return false;
        };
        let corner = Position::new(
            (hit_rect.right() - 1) as u16,
            (hit_rect.bottom() - 1) as u16,
        );
        let single = ids.len() == 1;
        let mode = if single && pos == corner && hit_rect.width > 1 && hit_rect.height > 1 {
            DragMode::Resize
        } else {
            DragMode::Move
        };
        let drag_ids: Vec<ObjectId> = if mode == DragMode::Resize {
            vec![hit_id]
        } else {
            ids
        };

        let mut targets = Vec::with_capacity(drag_ids.len());
        for id in &drag_ids {
            let Some((offset, size)) = self.geometry(id) else {
                continue;
            };
            let Some(rect) = self.layout().rect(id) else {
                continue;
            };
            targets.push(DragTarget {
                id: id.clone(),
                offset,
                size,
                rect,
            });
        }
        if targets.is_empty() {
            return false;
        }
        let snap_targets = if self.snap_enabled {
            self.session
                .snap_targets(&self.editor.screen, &self.editor.layer, &drag_ids)
                .unwrap_or_default()
        } else {
            SnapTargets::default()
        };
        let label = match mode {
            DragMode::Move => "Move components",
            DragMode::Resize => "Resize component",
        };
        if let Err(e) = self.session.begin(label) {
            self.report_error(&e);
            return false;
        }
        self.drag = Some(Drag {
            mode,
            origin: pos,
            targets,
            snap_targets,
            guides: Vec::new(),
        });
        true
    }

    pub fn drag_to(&mut self, pos: Position) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        let raw_dx = i32::from(pos.x) - i32::from(drag.origin.x);
        let raw_dy = i32::from(pos.y) - i32::from(drag.origin.y);
        let lead = &drag.targets[0];

        // Snapping adjusts the delta once, then every target follows it.
        let (dx, dy, guides) = if drag.snap_targets.is_empty() {
            (raw_dx, raw_dy, Vec::new())
        } else {
            match drag.mode {
                DragMode::Move => {
                    let moved = Rect::new(
                        (i32::from(lead.rect.x) + raw_dx).max(0) as u16,
                        (i32::from(lead.rect.y) + raw_dy).max(0) as u16,
                        lead.rect.width,
                        lead.rect.height,
                    );
                    let s = snap(moved, &drag.snap_targets, SNAP_THRESHOLD);
                    (
                        i32::from(s.x) - i32::from(lead.rect.x),
                        i32::from(s.y) - i32::from(lead.rect.y),
                        s.guides,
                    )
                }
                DragMode::Resize => {
                    // Snap the corner the pointer is dragging.
                    // Exclusive edges without a lossy cast: both parts are u16.
                    let right = i32::from(lead.rect.x) + i32::from(lead.rect.width);
                    let bottom = i32::from(lead.rect.y) + i32::from(lead.rect.height);
                    let corner = Rect::new(
                        (right + raw_dx).max(0) as u16,
                        (bottom + raw_dy).max(0) as u16,
                        0,
                        0,
                    );
                    let s = snap(corner, &drag.snap_targets, SNAP_THRESHOLD);
                    (i32::from(s.x) - right, i32::from(s.y) - bottom, s.guides)
                }
            }
        };

        let mut ops = Vec::with_capacity(drag.targets.len());
        for t in &drag.targets {
            let op = match drag.mode {
                DragMode::Move => Operation::Move {
                    target: t.id.clone(),
                    x: (i32::from(t.offset.0) + dx).max(0) as u16,
                    y: (i32::from(t.offset.1) + dy).max(0) as u16,
                },
                DragMode::Resize => Operation::Resize {
                    target: t.id.clone(),
                    width: (i32::from(t.size.0) + dx).max(1) as u16,
                    height: (i32::from(t.size.1) + dy).max(1) as u16,
                },
            };
            ops.push(op);
        }
        if let Some(d) = self.drag.as_mut() {
            d.guides = guides;
        }
        for op in ops {
            if let Err(e) = self.session.record(op) {
                self.report_error(&e);
                return;
            }
        }
        self.mark_edited();
    }

    pub fn end_drag(&mut self) {
        if self.drag.take().is_some() {
            self.commit_edits();
        }
    }

    /// Guides to draw while a drag is snapping.
    pub fn active_guides(&self) -> &[Guide] {
        self.drag.as_ref().map_or(&[], |d| &d.guides)
    }

    // --- prompts ------------------------------------------------------------

    pub fn open_select_by_id(&mut self) {
        let ids: Vec<String> = self.doc().components().map(|c| c.id.to_string()).collect();
        if ids.is_empty() {
            self.set_status(StatusKind::Info, "The document has no components yet");
            return;
        }
        self.prompt = Some(Prompt::new(PromptKind::SelectById, "Select component", ids));
    }

    pub fn open_add_component(&mut self) {
        if self.design_mode() != DesignMode::Interface {
            self.set_status(
                StatusKind::Warning,
                "Switch to an interface layer to add components (Ctrl+PgUp/PgDn)",
            );
            return;
        }
        let kinds: Vec<String> = self.session.registry().kinds().map(str::to_owned).collect();
        let hint = match self.primary() {
            Some(id) if self.editor.selection.len() == 1 => {
                format!("Add component inside {id}: kind [name]")
            }
            _ => "Add component: kind [name]".to_owned(),
        };
        self.prompt = Some(Prompt::new(PromptKind::AddComponent, hint, kinds));
    }

    pub fn open_edit_property(&mut self) {
        let Some(id) = self.primary().cloned() else {
            self.set_status(
                StatusKind::Info,
                "Select a component first ([ and ] cycle, click, or Ctrl+F)",
            );
            return;
        };
        let keys: Vec<String> = self
            .doc()
            .component(&id)
            .map(|c| c.props.keys().cloned().collect())
            .unwrap_or_default();
        let first = keys.first().cloned().unwrap_or_default();
        let current = self
            .doc()
            .component(&id)
            .and_then(|c| c.prop(&first))
            .map(ToString::to_string)
            .unwrap_or_default();
        let input = if first.is_empty() {
            String::new()
        } else {
            format!("{first}={current}")
        };
        let title = if self.editor.selection.len() > 1 {
            format!(
                "Set property on {} selected (key=value, empty value removes)",
                self.editor.selection.len()
            )
        } else {
            format!("Set property on {id} (key=value, empty value removes)")
        };
        self.prompt = Some(Prompt::new(PromptKind::EditProperty, title, vec![]).with_input(input));
    }

    pub fn open_save_as(&mut self) {
        let current = self
            .session
            .path()
            .map_or_else(|| "untitled.glyph".to_owned(), |p| p.display().to_string());
        self.prompt = Some(Prompt::new(PromptKind::SaveAs, "Save as", vec![]).with_input(current));
    }

    pub fn submit_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let value = prompt.value();
        match prompt.kind {
            PromptKind::SelectById => match ObjectId::new(&value) {
                Ok(id) if self.doc().component(&id).is_some() => self.jump_to_component(&id),
                _ => self.set_status(StatusKind::Warning, format!("No component {value:?}")),
            },
            PromptKind::AddComponent => self.add_component_from(&value),
            PromptKind::EditProperty => self.edit_property_from(&value),
            PromptKind::SaveAs => {
                if value.is_empty() {
                    return;
                }
                let path = std::path::PathBuf::from(&value);
                match self.session.save_as(&path) {
                    Ok(()) => {
                        self.set_status(StatusKind::Info, format!("Saved {}", path.display()));
                    }
                    Err(e) => self.report_error(&e),
                }
            }
        }
    }

    /// Selects a component wherever it is: switches screen and layer.
    fn jump_to_component(&mut self, id: &ObjectId) {
        let Some(layer) = self.doc().layer_of_component(id).map(|l| l.id.clone()) else {
            return;
        };
        let Some(screen) = self.doc().screen_of_layer(&layer).map(|s| s.id.clone()) else {
            return;
        };
        if self.editor.screen != screen || self.editor.layer != layer {
            self.editor.screen = screen;
            self.editor.layer = layer;
            self.mark_edited();
        }
        self.select(vec![id.clone()]);
        self.describe_selection(id);
    }

    fn add_component_from(&mut self, value: &str) {
        let mut words = value.split_whitespace();
        let Some(kind) = words.next() else { return };
        let name = words.collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        // A single selected component becomes the parent; with several
        // selected the new component goes to the top level.
        let parent = if self.editor.selection.len() == 1 {
            self.primary().cloned()
        } else {
            None
        };
        let cursor = self.editor.cursor;
        let (x, y) = match &parent {
            Some(p) => {
                let inner = self.layout().inner(p).unwrap_or(Rect::new(0, 0, 0, 0));
                (
                    cursor.x.saturating_sub(inner.x),
                    cursor.y.saturating_sub(inner.y),
                )
            }
            None => (cursor.x, cursor.y),
        };
        let layer = self.editor.layer.clone();
        let kind = kind.to_owned();
        match self
            .session
            .add_component(&kind, name, x, y, parent, Some(layer))
        {
            Ok(id) => {
                self.mark_edited();
                self.select(vec![id.clone()]);
                self.set_status(StatusKind::Info, format!("Added {kind} {id}"));
            }
            Err(e) => self.report_error(&e),
        }
    }

    /// Sets one property on every selected component.
    fn edit_property_from(&mut self, value: &str) {
        if self.editor.selection.is_empty() {
            return;
        }
        let Some((key, val)) = parse_property(value) else {
            self.set_status(StatusKind::Warning, "Expected key=value");
            return;
        };
        let ids = self.editor.selection.clone();
        let ops = ids
            .iter()
            .map(|id| Operation::SetProperty {
                target: id.clone(),
                property: key.clone(),
                value: val.clone(),
            })
            .collect();
        let label = format!("Set {key}");
        if self.apply(Patch::new(ops), &label) {
            let text = if ids.len() == 1 {
                label
            } else {
                format!("{label} on {} components", ids.len())
            };
            self.set_status(StatusKind::Info, text);
        }
    }

    /// Placement summary for the inspector.
    pub fn placement_text(&self, id: &ObjectId) -> String {
        match self.doc().component(id).map(|c| c.layout.placement) {
            Some(Placement::Absolute { x, y }) => format!("absolute {x},{y}"),
            Some(Placement::Flow) => "flow".to_owned(),
            None => String::new(),
        }
    }

    /// Bounding box of the selection, for the inspector.
    pub fn selection_bounds(&self) -> Option<Rect> {
        let layout = self.cached_layout()?;
        let rects: Vec<Rect> = self
            .editor
            .selection
            .iter()
            .filter_map(|id| layout.rect(id))
            .collect();
        if rects.is_empty() {
            return None;
        }
        Some(glyphforge_core::geometry::bounds(rects))
    }
}

fn describe(ids: &[ObjectId]) -> String {
    match ids {
        [one] => one.to_string(),
        many => format!("{} components", many.len()),
    }
}
