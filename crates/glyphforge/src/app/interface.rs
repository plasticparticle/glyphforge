//! Interface Mode: selecting, moving, resizing, creating and editing
//! components. Every mutation is a patch operation on the session.

use glyphforge_core::component::tree;
use glyphforge_core::history::Origin;
use glyphforge_core::layout::{LayoutResult, Placement};
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::{ObjectId, Position, Rect};

use super::prompt::{Prompt, PromptKind, parse_property};
use super::{App, DesignMode, StatusKind};
use crate::actions::Direction;

/// Reparent prompt entry for "no parent".
const ROOT: &str = "(top level)";

/// `id` for one component, `N components` for more.
fn describe(ids: &[ObjectId]) -> String {
    match ids {
        [one] => one.to_string(),
        many => format!("{} components", many.len()),
    }
}

/// An in-progress mouse drag on a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    Move,
    Resize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drag {
    pub target: ObjectId,
    pub mode: DragMode,
    /// Document position where the drag started.
    pub origin: Position,
    /// Offset of the component inside its parent when the drag started.
    pub start_offset: (u16, u16),
    pub start_size: (u16, u16),
}

impl App {
    pub fn design_mode(&self) -> DesignMode {
        if self.on_artwork_layer() {
            DesignMode::Subcell
        } else if self.on_raster_layer() {
            DesignMode::Pixel
        } else {
            DesignMode::Interface
        }
    }

    /// Solved layout of the active layer, cached per frame.
    pub fn layout(&mut self) -> &LayoutResult {
        if self.layout_cache.is_none() {
            let result = self
                .session
                .layout(
                    &self.editor.screen,
                    &self.editor.layer,
                    self.editor.preview_size,
                )
                .unwrap_or_default();
            self.layout_cache = Some(result);
        }
        self.layout_cache
            .as_ref()
            .unwrap_or_else(|| unreachable!("layout cache populated"))
    }

    pub fn selection(&self) -> Option<&ObjectId> {
        self.editor
            .selection
            .as_ref()
            .filter(|id| self.doc().component(id).is_some())
    }

    /// Components of the active interface layer in document order.
    fn layer_components(&self) -> Vec<ObjectId> {
        self.active_layer()
            .and_then(|l| l.components())
            .map(|roots| tree::iter(roots).map(|c| c.id.clone()).collect())
            .unwrap_or_default()
    }

    pub fn select(&mut self, id: Option<ObjectId>) {
        self.editor.also_selected.clear();
        self.editor.selection = id;
        if let Some(id) = self.editor.selection.clone() {
            if let Some(rect) = self.layout().rect(&id) {
                let size = self.doc_size();
                self.editor
                    .viewport
                    .ensure_visible(Position::new(rect.x, rect.y), size);
            }
        }
    }

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
        let next = match self
            .selection()
            .and_then(|s| ids.iter().position(|i| i == s))
        {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        let id = ids[next].clone();
        let kind = self
            .doc()
            .component(&id)
            .map(|c| c.kind.clone())
            .unwrap_or_default();
        self.select(Some(id.clone()));
        self.set_status(StatusKind::Info, format!("Selected {id} ({kind})"));
    }

    /// The deepest component whose rect contains `pos`.
    pub fn component_at(&mut self, pos: Position) -> Option<ObjectId> {
        let ids = self.layer_components();
        let layout = self.layout();
        ids.into_iter()
            .rev()
            .find(|id| layout.rect(id).is_some_and(|r| r.contains(pos)))
    }

    pub fn select_at(&mut self, pos: Position) {
        let hit = self.component_at(pos);
        self.select(hit);
    }

    /// Rect of the selected component's parent content area (or the
    /// screen), used to express positions relative to the parent.
    pub(super) fn parent_inner(&mut self, id: &ObjectId) -> Rect {
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

    /// Current offset of `id` inside its parent and its size.
    pub(super) fn geometry(&mut self, id: &ObjectId) -> Option<((u16, u16), (u16, u16))> {
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

    pub(super) fn apply(&mut self, patch: Patch, label: &str) -> bool {
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

    pub fn move_selection(&mut self, dir: Direction) -> bool {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return false;
        }
        let mut ops = Vec::new();
        for id in ids {
            let Some(((x, y), _)) = self.geometry(&id) else {
                continue;
            };
            let (nx, ny) = match dir {
                Direction::Up => (x, y.saturating_sub(1)),
                Direction::Down => (x, y.saturating_add(1)),
                Direction::Left => (x.saturating_sub(1), y),
                Direction::Right => (x.saturating_add(1), y),
            };
            if (nx, ny) != (x, y) {
                ops.push(Operation::Move {
                    target: id,
                    x: nx,
                    y: ny,
                });
            }
        }
        ops.is_empty() || self.apply(Patch::new(ops), "Move component")
    }

    pub fn resize_selection(&mut self, dir: Direction) -> bool {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return false;
        }
        let mut ops = Vec::new();
        for id in ids {
            let Some((_, (w, h))) = self.geometry(&id) else {
                continue;
            };
            let (nw, nh) = match dir {
                Direction::Up => (w, h.saturating_sub(1).max(1)),
                Direction::Down => (w, h.saturating_add(1)),
                Direction::Left => (w.saturating_sub(1).max(1), h),
                Direction::Right => (w.saturating_add(1), h),
            };
            if (nw, nh) != (w, h) {
                ops.push(Operation::Resize {
                    target: id,
                    width: nw,
                    height: nh,
                });
            }
        }
        ops.is_empty() || self.apply(Patch::new(ops), "Resize component")
    }

    pub fn delete_selection(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            self.set_status(StatusKind::Info, "Nothing selected");
            return;
        }
        // Children go with their parent: skip ids inside another target.
        let doc = self.doc();
        let ops: Vec<Operation> = ids
            .iter()
            .filter(|id| {
                !ids.iter().any(|other| {
                    other != *id
                        && doc
                            .component(other)
                            .is_some_and(|c| c.iter().any(|d| &d.id == *id))
                })
            })
            .map(|id| Operation::DeleteComponent { target: id.clone() })
            .collect();
        let count = ops.len();
        if self.apply(Patch::new(ops), "Delete component") {
            self.select(None);
            let text = match ids.last() {
                Some(id) if count == 1 => format!("Deleted {id}"),
                _ => format!("Deleted {count} components"),
            };
            self.set_status(StatusKind::Info, text);
        }
    }

    /// Shift+click: adds the component under `pos` to the selection, or
    /// removes it when it is already selected.
    pub fn toggle_select_at(&mut self, pos: Position) {
        let Some(id) = self.component_at(pos) else {
            return;
        };
        let primary = self.editor.selection.clone();
        if primary.as_ref() == Some(&id) {
            self.editor.selection = self.editor.also_selected.pop();
        } else if let Some(i) = self.editor.also_selected.iter().position(|a| *a == id) {
            self.editor.also_selected.remove(i);
        } else {
            if let Some(p) = primary.filter(|p| self.doc().component(p).is_some()) {
                self.editor.also_selected.push(p);
            }
            self.editor.selection = Some(id);
        }
        let n = self.selected_ids().len();
        self.set_status(StatusKind::Info, format!("{n} selected"));
    }

    pub fn select_all(&mut self) {
        let mut ids = self.layer_components();
        let Some(last) = ids.pop() else {
            self.set_status(StatusKind::Info, "No components on this layer");
            return;
        };
        self.editor.selection = Some(last);
        self.editor.also_selected = ids;
        let n = self.selected_ids().len();
        self.set_status(StatusKind::Info, format!("{n} selected"));
    }

    fn parent_of(&self, id: &ObjectId) -> Option<ObjectId> {
        self.active_layer()
            .and_then(|l| l.components())
            .and_then(|roots| tree::locate(roots, id))
            .and_then(|(parent, _)| parent)
    }

    pub fn select_parent(&mut self) {
        match self.selection().and_then(|id| self.parent_of(id)) {
            Some(p) => {
                self.select(Some(p.clone()));
                self.set_status(StatusKind::Info, format!("Selected {p}"));
            }
            None => self.set_status(StatusKind::Info, "Top level: no parent"),
        }
    }

    pub fn select_first_child(&mut self) {
        let child = self
            .selection()
            .and_then(|id| self.doc().component(id))
            .and_then(|c| c.children.first())
            .map(|c| c.id.clone());
        match child {
            Some(c) => {
                self.select(Some(c.clone()));
                self.set_status(StatusKind::Info, format!("Selected {c}"));
            }
            None => self.set_status(StatusKind::Info, "No children"),
        }
    }

    /// Prompt for the container the selection moves into.
    pub fn open_reparent(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            self.set_status(StatusKind::Info, "Select a component to move it");
            return;
        }
        let doc = self.doc();
        let inside_selection = |c: &ObjectId| {
            ids.iter().any(|id| {
                doc.component(id)
                    .is_some_and(|s| s.iter().any(|d| &d.id == c))
            })
        };
        let registry = self.session.registry();
        let mut candidates = vec![ROOT.to_owned()];
        candidates.extend(
            self.layer_components()
                .into_iter()
                .filter(|c| !inside_selection(c))
                .filter(|c| {
                    doc.component(c).is_some_and(|c| {
                        !c.children.is_empty()
                            || registry.descriptor(&c.kind).is_some_and(|d| d.container)
                    })
                })
                .map(|c| c.to_string()),
        );
        self.prompt = Some(Prompt::new(
            PromptKind::Reparent,
            format!("Move {} into", describe(&ids)),
            candidates,
        ));
    }

    /// Moves the selection under `value` (a container id or `(top level)`),
    /// keeping absolutely placed components where they are on screen.
    pub(super) fn reparent_into(&mut self, value: &str) {
        let parent = if value == ROOT || value.is_empty() {
            None
        } else {
            match ObjectId::new(value) {
                Ok(id) if self.doc().component(&id).is_some() => Some(id),
                _ => {
                    self.set_status(StatusKind::Warning, format!("No component {value:?}"));
                    return;
                }
            }
        };
        let screen = Rect::from_size(self.doc_size());
        let target_inner = match &parent {
            Some(p) => self.layout().inner(p).unwrap_or(screen),
            None => screen,
        };
        let ids = self.selected_ids();
        let mut ops = Vec::new();
        for id in &ids {
            ops.push(Operation::Reparent {
                target: id.clone(),
                parent: parent.clone(),
                layer: Some(self.editor.layer.clone()),
                index: None,
            });
            let rect = self.layout().rect(id);
            if let (Some(c), Some(rect)) = (self.doc().component(id), rect) {
                if matches!(c.layout.placement, Placement::Absolute { .. }) {
                    let mut layout = c.layout;
                    layout.placement = Placement::Absolute {
                        x: rect.x.saturating_sub(target_inner.x),
                        y: rect.y.saturating_sub(target_inner.y),
                    };
                    ops.push(Operation::SetLayout {
                        target: id.clone(),
                        layout,
                    });
                }
            }
        }
        let into = parent
            .as_ref()
            .map_or("the top level".to_owned(), ToString::to_string);
        if self.apply(Patch::new(ops), "Move into container") {
            self.set_status(
                StatusKind::Info,
                format!("Moved {} into {into}", describe(&ids)),
            );
        }
    }

    // --- mouse drags ------------------------------------------------------

    /// Starts a drag at `pos` if a component is selected there; the
    /// bottom-right corner of the selection resizes, anywhere else moves.
    pub fn begin_drag(&mut self, pos: Position) -> bool {
        let Some(id) = self.selection().cloned() else {
            return false;
        };
        let Some(rect) = self.layout().rect(&id) else {
            return false;
        };
        if !rect.contains(pos) {
            return false;
        }
        let Some((offset, size)) = self.geometry(&id) else {
            return false;
        };
        let corner = Position::new((rect.right() - 1) as u16, (rect.bottom() - 1) as u16);
        let mode = if pos == corner && rect.width > 1 && rect.height > 1 {
            DragMode::Resize
        } else {
            DragMode::Move
        };
        let label = match mode {
            DragMode::Move => "Move component",
            DragMode::Resize => "Resize component",
        };
        if let Err(e) = self.session.begin(label) {
            self.report_error(&e);
            return false;
        }
        self.drag = Some(Drag {
            target: id,
            mode,
            origin: pos,
            start_offset: offset,
            start_size: size,
        });
        true
    }

    pub fn drag_to(&mut self, pos: Position) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        let dx = i32::from(pos.x) - i32::from(drag.origin.x);
        let dy = i32::from(pos.y) - i32::from(drag.origin.y);
        let inner = self.parent_inner(&drag.target);
        let (ox, oy) = (
            (i32::from(drag.start_offset.0) + dx).max(0) as u16,
            (i32::from(drag.start_offset.1) + dy).max(0) as u16,
        );
        let (w, h) = (
            (i32::from(drag.start_size.0) + dx).max(1) as u16,
            (i32::from(drag.start_size.1) + dy).max(1) as u16,
        );
        let op = match drag.mode {
            DragMode::Move => {
                let rect = Rect::new(
                    inner.x.saturating_add(ox),
                    inner.y.saturating_add(oy),
                    drag.start_size.0,
                    drag.start_size.1,
                );
                let snapped = self.snap_drag(&drag.target, rect);
                Operation::Move {
                    target: drag.target.clone(),
                    x: snapped.x.saturating_sub(inner.x),
                    y: snapped.y.saturating_sub(inner.y),
                }
            }
            DragMode::Resize => {
                // Snap the corner cell, then size up to it.
                let x0 = inner.x.saturating_add(drag.start_offset.0);
                let y0 = inner.y.saturating_add(drag.start_offset.1);
                let corner = Rect::new(x0 + w - 1, y0 + h - 1, 1, 1);
                let snapped = self.snap_drag(&drag.target, corner);
                Operation::Resize {
                    target: drag.target.clone(),
                    width: (snapped.x + 1).saturating_sub(x0).max(1),
                    height: (snapped.y + 1).saturating_sub(y0).max(1),
                }
            }
        };
        match self.session.record(op) {
            Ok(()) => self.mark_edited(),
            Err(e) => self.report_error(&e),
        }
    }

    pub fn end_drag(&mut self) {
        self.guides.clear();
        if self.drag.take().is_some() {
            self.commit_edits();
        }
    }

    // --- prompts ------------------------------------------------------------

    pub fn open_add_component(&mut self) {
        if self.design_mode() != DesignMode::Interface {
            self.set_status(
                StatusKind::Warning,
                "Switch to an interface layer to add components (Ctrl+PgUp/PgDn)",
            );
            return;
        }
        let kinds: Vec<String> = self.session.registry().kinds().map(str::to_owned).collect();
        let hint = match self.selection() {
            Some(id) => format!("Add component inside {id}: kind [name]"),
            None => "Add component: kind [name]".to_owned(),
        };
        self.prompt = Some(Prompt::new(PromptKind::AddComponent, hint, kinds));
    }

    pub fn open_edit_property(&mut self) {
        let Some(id) = self.selection().cloned() else {
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
        self.prompt = Some(
            Prompt::new(
                PromptKind::EditProperty,
                format!("Set property on {id} (key=value, empty value removes)"),
                vec![],
            )
            .with_input(input),
        );
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
            PromptKind::AddComponent => self.add_component_from(&value),
            PromptKind::EditProperty => self.edit_property_from(&value),
            PromptKind::Color(target) => self.submit_color_prompt(target, &value),
            PromptKind::BrushGlyph => self.submit_glyph_prompt(&prompt),
            PromptKind::StampLabel => self.set_stamp_label(&prompt.input),
            PromptKind::NewPixelLayer => self.submit_new_pixel_layer(&value),
            PromptKind::PixelSettings => self.submit_pixel_settings(&prompt.input),
            PromptKind::Gradient => self.submit_gradient(&prompt.input),
            PromptKind::Reparent => self.reparent_into(&value),
            PromptKind::NewLayer => self.submit_new_layer(&prompt.input),
            PromptKind::RenameLayer => self.submit_rename_layer(&prompt.input),
            PromptKind::NewScreen => self.submit_new_screen(&prompt.input),
            PromptKind::RenameScreen => self.submit_rename_screen(&prompt.input),
            PromptKind::ResizeScreen => self.submit_resize_screen(&value),
            PromptKind::Find => self.submit_find(&prompt),
            PromptKind::Inspector(key) => self.submit_inspector(&key, &prompt.input),
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
    pub(super) fn jump_to_component(&mut self, id: &ObjectId) {
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
        self.select(Some(id.clone()));
        let kind = self
            .doc()
            .component(id)
            .map(|c| c.kind.clone())
            .unwrap_or_default();
        self.set_status(StatusKind::Info, format!("Selected {id} ({kind})"));
    }

    pub(super) fn add_component_from(&mut self, value: &str) {
        let mut words = value.split_whitespace();
        let Some(kind) = words.next() else { return };
        let name = words.collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        let parent = self.selection().cloned();
        // Position: at the cursor, relative to the parent's content area.
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
                self.select(Some(id.clone()));
                self.set_status(StatusKind::Info, format!("Added {kind} {id}"));
            }
            Err(e) => self.report_error(&e),
        }
    }

    fn edit_property_from(&mut self, value: &str) {
        let Some(id) = self.selection().cloned() else {
            return;
        };
        let Some((key, val)) = parse_property(value) else {
            self.set_status(StatusKind::Warning, "Expected key=value");
            return;
        };
        let label = format!("Set {key}");
        if self.apply(
            Patch::single(Operation::SetProperty {
                target: id,
                property: key,
                value: val,
            }),
            &label,
        ) {
            self.set_status(StatusKind::Info, label);
        }
    }
}
