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

    /// Current offset of `id` inside its parent and its size.
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

    pub fn move_selection(&mut self, dir: Direction) -> bool {
        let Some(id) = self.selection().cloned() else {
            return false;
        };
        let Some(((x, y), _)) = self.geometry(&id) else {
            return false;
        };
        let (nx, ny) = match dir {
            Direction::Up => (x, y.saturating_sub(1)),
            Direction::Down => (x, y.saturating_add(1)),
            Direction::Left => (x.saturating_sub(1), y),
            Direction::Right => (x.saturating_add(1), y),
        };
        if (nx, ny) == (x, y) {
            return true;
        }
        self.apply(
            Patch::single(Operation::Move {
                target: id,
                x: nx,
                y: ny,
            }),
            "Move component",
        )
    }

    pub fn resize_selection(&mut self, dir: Direction) -> bool {
        let Some(id) = self.selection().cloned() else {
            return false;
        };
        let Some((_, (w, h))) = self.geometry(&id) else {
            return false;
        };
        let (nw, nh) = match dir {
            Direction::Up => (w, h.saturating_sub(1).max(1)),
            Direction::Down => (w, h.saturating_add(1)),
            Direction::Left => (w.saturating_sub(1).max(1), h),
            Direction::Right => (w.saturating_add(1), h),
        };
        if (nw, nh) == (w, h) {
            return true;
        }
        self.apply(
            Patch::single(Operation::Resize {
                target: id,
                width: nw,
                height: nh,
            }),
            "Resize component",
        )
    }

    pub fn delete_selection(&mut self) {
        let Some(id) = self.selection().cloned() else {
            self.set_status(StatusKind::Info, "Nothing selected");
            return;
        };
        if self.apply(
            Patch::single(Operation::DeleteComponent { target: id.clone() }),
            "Delete component",
        ) {
            self.editor.selection = None;
            self.set_status(StatusKind::Info, format!("Deleted {id}"));
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
        let op = match drag.mode {
            DragMode::Move => Operation::Move {
                target: drag.target.clone(),
                x: (i32::from(drag.start_offset.0) + dx).max(0) as u16,
                y: (i32::from(drag.start_offset.1) + dy).max(0) as u16,
            },
            DragMode::Resize => Operation::Resize {
                target: drag.target.clone(),
                width: (i32::from(drag.start_size.0) + dx).max(1) as u16,
                height: (i32::from(drag.start_size.1) + dy).max(1) as u16,
            },
        };
        match self.session.record(op) {
            Ok(()) => self.mark_edited(),
            Err(e) => self.report_error(&e),
        }
    }

    pub fn end_drag(&mut self) {
        if self.drag.take().is_some() {
            self.commit_edits();
        }
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
            PromptKind::SelectById => match ObjectId::new(&value) {
                Ok(id) if self.doc().component(&id).is_some() => {
                    self.jump_to_component(&id);
                }
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
        self.select(Some(id.clone()));
        let kind = self
            .doc()
            .component(id)
            .map(|c| c.kind.clone())
            .unwrap_or_default();
        self.set_status(StatusKind::Info, format!("Selected {id} ({kind})"));
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

    /// Placement summary for the inspector.
    pub fn placement_text(&self, id: &ObjectId) -> String {
        match self.doc().component(id).map(|c| c.layout.placement) {
            Some(Placement::Absolute { x, y }) => format!("absolute {x},{y}"),
            Some(Placement::Flow) => "flow".to_owned(),
            None => String::new(),
        }
    }
}
