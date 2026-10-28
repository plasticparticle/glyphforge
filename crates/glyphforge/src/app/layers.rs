//! Layers and screens: the interactive layer list (activate, show/hide,
//! lock, isolate, reorder, create, rename, duplicate, merge, delete), the
//! screen switcher and screen operations, responsive preview sizes, the
//! minimap toggle and Find. Every document change is a patch.

use glyphforge_core::patch::{CellWrite, Operation, Patch};
use glyphforge_core::{Layer, LayerKind, ObjectId, Position, Screen, Size};

use super::prompt::{Prompt, PromptKind};
use super::{App, StatusKind};

/// Preview sizes F7 steps through (after the screen's own size).
pub const PREVIEW_SIZES: [(u16, u16); 5] = [(60, 20), (80, 24), (100, 30), (120, 40), (160, 50)];

/// What Find can jump to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindTarget {
    Component(ObjectId),
    Screen(ObjectId),
    Layer(ObjectId),
    /// Text on an artwork layer: layer and position of the first glyph.
    Text(ObjectId, Position),
}

/// Layer list actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LayerAction {
    New,
    Rename,
    Duplicate,
    Delete,
    MergeDown,
    /// Shows or hides a layer (the active one when `None`).
    ToggleVisible(Option<ObjectId>),
    ToggleLock(Option<ObjectId>),
    /// Hides every other layer, or restores them.
    Isolate,
    /// Moves the active layer up (`true`) or down the stack.
    Raise(bool),
}

/// Screen actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScreenAction {
    Step(bool),
    Switch(ObjectId),
    New,
    Rename,
    Resize,
    Duplicate,
    Delete,
}

/// Parses `WxH` (also `W×H` and `W H`).
pub fn parse_size(text: &str) -> Option<Size> {
    let mut parts = text
        .split(|c: char| c == 'x' || c == '×' || c == 'X' || c.is_whitespace() || c == ',')
        .filter(|p| !p.is_empty());
    let w: u16 = parts.next()?.parse().ok()?;
    let h: u16 = parts.next()?.parse().ok()?;
    (w > 0 && h > 0 && parts.next().is_none()).then(|| Size::new(w, h))
}

/// A copy of `layer` with a fresh id (and fresh component ids), unique
/// against `taken`.
fn copy_layer(layer: &Layer, id: &ObjectId, taken: &mut Vec<ObjectId>) -> Layer {
    let mut copy = layer.clone();
    copy.id = id.unique_among(|i| taken.contains(i));
    taken.push(copy.id.clone());
    if let Some(roots) = copy.components_mut() {
        for c in roots.iter_mut() {
            super::selection::reid(c, taken);
        }
    }
    copy
}

impl App {
    pub(super) fn dispatch_layer(&mut self, action: LayerAction) {
        match action {
            LayerAction::New => self.open_new_layer(),
            LayerAction::Rename => {
                let name = self
                    .active_layer()
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                self.prompt = Some(
                    Prompt::new(PromptKind::RenameLayer, "Rename layer", vec![]).with_input(name),
                );
            }
            LayerAction::Duplicate => self.duplicate_layer(),
            LayerAction::Delete => self.delete_layer(),
            LayerAction::MergeDown => self.merge_layer_down(),
            LayerAction::ToggleVisible(id) => {
                let id = id.unwrap_or_else(|| self.editor.layer.clone());
                let Some(visible) = self.screen().layer(&id).map(|l| l.visible) else {
                    return;
                };
                self.update_layer(&id, None, Some(!visible), None, "Toggle layer visibility");
            }
            LayerAction::ToggleLock(id) => {
                let id = id.unwrap_or_else(|| self.editor.layer.clone());
                let Some(locked) = self.screen().layer(&id).map(|l| l.locked) else {
                    return;
                };
                self.update_layer(&id, None, None, Some(!locked), "Toggle layer lock");
            }
            LayerAction::Isolate => self.isolate_layer(),
            LayerAction::Raise(up) => self.raise_layer(up),
        }
    }

    fn update_layer(
        &mut self,
        id: &ObjectId,
        name: Option<String>,
        visible: Option<bool>,
        locked: Option<bool>,
        label: &str,
    ) -> bool {
        let ok = self.apply(
            Patch::single(Operation::UpdateLayer {
                target: id.clone(),
                name,
                visible,
                locked,
            }),
            label,
        );
        if ok {
            self.set_status(StatusKind::Info, label.to_owned());
        }
        ok
    }

    fn open_new_layer(&mut self) {
        self.prompt = Some(Prompt::new(
            PromptKind::NewLayer,
            "New layer above the active one: kind [name]",
            vec!["artwork".into(), "interface".into(), "pixel".into()],
        ));
    }

    pub(super) fn submit_new_layer(&mut self, value: &str) {
        let mut words = value.split_whitespace();
        let kind = words.next().unwrap_or("artwork");
        let rest = words.collect::<Vec<_>>().join(" ");
        if kind == "pixel" {
            return self.submit_new_pixel_layer(&rest);
        }
        let name = if rest.is_empty() {
            match kind {
                "interface" => "Interface".to_owned(),
                _ => "Artwork".to_owned(),
            }
        } else {
            rest
        };
        let ids = self.doc().ids();
        let id = ObjectId::slugify(&name).unique_among(|i| ids.contains(i));
        let layer = match kind {
            "interface" => Layer::interface(id.clone(), &name),
            "artwork" => match Layer::artwork(id.clone(), &name, self.screen().size()) {
                Ok(l) => l,
                Err(e) => return self.report_error(&e),
            },
            other => {
                return self.set_status(
                    StatusKind::Warning,
                    format!("Unknown layer kind {other:?}: artwork, interface or pixel"),
                );
            }
        };
        let index = self.screen().layer_index(&self.editor.layer).map(|i| i + 1);
        if self.apply(
            Patch::single(Operation::CreateLayer {
                layer,
                screen: Some(self.editor.screen.clone()),
                index,
            }),
            "New layer",
        ) {
            self.activate_layer(&id);
        }
    }

    pub(super) fn submit_rename_layer(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let id = self.editor.layer.clone();
        self.update_layer(&id, Some(name.to_owned()), None, None, "Rename layer");
    }

    fn duplicate_layer(&mut self) {
        let Some(layer) = self.active_layer().cloned() else {
            return;
        };
        let mut taken = self.doc().ids();
        let copy = copy_layer(&layer, &layer.id, &mut taken);
        let mut copy = copy;
        copy.name = format!("{} copy", layer.name);
        let id = copy.id.clone();
        let index = self.screen().layer_index(&layer.id).map(|i| i + 1);
        if self.apply(
            Patch::single(Operation::CreateLayer {
                layer: copy,
                screen: Some(self.editor.screen.clone()),
                index,
            }),
            "Duplicate layer",
        ) {
            self.activate_layer(&id);
        }
    }

    fn delete_layer(&mut self) {
        let id = self.editor.layer.clone();
        let index = self.screen().layer_index(&id).unwrap_or(0);
        if self.apply(
            Patch::single(Operation::DeleteLayer { target: id.clone() }),
            "Delete layer",
        ) {
            let layers = self.screen().layers();
            let next = layers[index.saturating_sub(1).min(layers.len() - 1)]
                .id
                .clone();
            self.activate_layer(&next);
            self.set_status(
                StatusKind::Info,
                format!("Deleted layer {id} (Ctrl+Z restores it)"),
            );
        }
    }

    /// Merges the active layer into the one below it: artwork cells are
    /// painted over the lower artwork layer, components move to the lower
    /// interface layer. One undo step.
    fn merge_layer_down(&mut self) {
        let screen = self.screen();
        let Some(i) = screen.layer_index(&self.editor.layer) else {
            return;
        };
        let Some(lower) = i.checked_sub(1).map(|j| &screen.layers()[j]) else {
            self.set_status(
                StatusKind::Info,
                "The bottom layer has nothing to merge into",
            );
            return;
        };
        let upper = &screen.layers()[i];
        let lower_id = lower.id.clone();
        let mut ops = Vec::new();
        match (upper.kind(), lower.kind()) {
            (LayerKind::Artwork, LayerKind::Artwork) => {
                let Some(cells) = upper.cells() else { return };
                let writes: Vec<CellWrite> = cells
                    .bounds()
                    .positions()
                    .filter_map(|p| {
                        let c = cells.get(p)?;
                        (!c.is_empty() && !c.is_wide_tail()).then(|| CellWrite {
                            x: p.x,
                            y: p.y,
                            cell: c.clone(),
                        })
                    })
                    .collect();
                if !writes.is_empty() {
                    ops.push(Operation::SetCells {
                        layer: lower_id.clone(),
                        cells: writes,
                    });
                }
            }
            (LayerKind::Interface, LayerKind::Interface) => {
                for c in upper.components().unwrap_or_default() {
                    ops.push(Operation::Reparent {
                        target: c.id.clone(),
                        parent: None,
                        layer: Some(lower_id.clone()),
                        index: None,
                    });
                }
            }
            (a, b) => {
                self.set_status(
                    StatusKind::Warning,
                    format!(
                        "Cannot merge a {} layer into a {} layer",
                        a.name(),
                        b.name()
                    ),
                );
                return;
            }
        }
        ops.push(Operation::DeleteLayer {
            target: upper.id.clone(),
        });
        if self.apply(Patch::new(ops), "Merge layer down") {
            self.activate_layer(&lower_id);
            self.set_status(StatusKind::Info, format!("Merged into {lower_id}"));
        }
    }

    /// Hides every other layer of the screen; again restores what was
    /// visible before.
    fn isolate_layer(&mut self) {
        let active = self.editor.layer.clone();
        let restore = self.editor.isolation.take();
        let ops: Vec<Operation> = if let Some(saved) = restore {
            saved
                .into_iter()
                .filter(|(id, _)| self.screen().layer(id).is_some())
                .map(|(target, visible)| Operation::UpdateLayer {
                    target,
                    name: None,
                    visible: Some(visible),
                    locked: None,
                })
                .collect()
        } else {
            let saved: Vec<(ObjectId, bool)> = self
                .screen()
                .layers()
                .iter()
                .map(|l| (l.id.clone(), l.visible))
                .collect();
            let ops = saved
                .iter()
                .map(|(id, _)| Operation::UpdateLayer {
                    target: id.clone(),
                    name: None,
                    visible: Some(*id == active),
                    locked: None,
                })
                .collect();
            self.editor.isolation = Some(saved);
            ops
        };
        let isolating = self.editor.isolation.is_some();
        let label = if isolating {
            "Isolate layer"
        } else {
            "Show layers again"
        };
        if self.apply(Patch::new(ops), label) {
            self.set_status(StatusKind::Info, label.to_owned());
        } else {
            self.editor.isolation = None;
        }
    }

    fn raise_layer(&mut self, up: bool) {
        let id = self.editor.layer.clone();
        let n = self.screen().layers().len();
        let Some(i) = self.screen().layer_index(&id) else {
            return;
        };
        let target = if up { i + 1 } else { i.wrapping_sub(1) };
        if target >= n {
            return;
        }
        self.apply(
            Patch::single(Operation::ReorderLayer {
                target: id,
                index: target,
            }),
            if up { "Raise layer" } else { "Lower layer" },
        );
    }

    // --- screens ---------------------------------------------------------------

    pub(super) fn dispatch_screen(&mut self, action: ScreenAction) {
        match action {
            ScreenAction::Step(forward) => {
                let screens = self.doc().screens();
                let n = screens.len();
                let i = screens
                    .iter()
                    .position(|s| s.id == self.editor.screen)
                    .unwrap_or(0);
                let next = screens[if forward {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                }]
                .id
                .clone();
                self.switch_screen(&next);
            }
            ScreenAction::Switch(id) => self.switch_screen(&id),
            ScreenAction::New => {
                let size = self.screen().size();
                self.prompt = Some(
                    Prompt::new(PromptKind::NewScreen, "New screen: name [WxH]", vec![])
                        .with_input(format!("Screen {}×{}", size.width, size.height)),
                );
            }
            ScreenAction::Rename => {
                let name = self.screen().name.clone();
                self.prompt = Some(
                    Prompt::new(PromptKind::RenameScreen, "Rename screen", vec![]).with_input(name),
                );
            }
            ScreenAction::Resize => {
                let size = self.screen().size();
                let candidates = PREVIEW_SIZES
                    .iter()
                    .map(|(w, h)| format!("{w}x{h}"))
                    .collect();
                self.prompt = Some(
                    Prompt::new(PromptKind::ResizeScreen, "Resize screen: WxH", candidates)
                        .with_input(format!("{}x{}", size.width, size.height)),
                );
            }
            ScreenAction::Duplicate => self.duplicate_screen(),
            ScreenAction::Delete => {
                let id = self.editor.screen.clone();
                let index = self
                    .doc()
                    .screens()
                    .iter()
                    .position(|s| s.id == id)
                    .unwrap_or(0);
                if self.apply(
                    Patch::single(Operation::DeleteScreen { target: id.clone() }),
                    "Delete screen",
                ) {
                    let screens = self.doc().screens();
                    let next = screens[index.min(screens.len() - 1)].id.clone();
                    self.switch_screen(&next);
                    self.set_status(StatusKind::Info, format!("Deleted screen {id}"));
                }
            }
        }
    }

    /// Shows another screen, keeping a layer of the same kind active.
    pub fn switch_screen(&mut self, id: &ObjectId) {
        let Some(screen) = self.doc().screen(id) else {
            return;
        };
        let kind = self.active_layer().map(Layer::kind);
        let layer = screen
            .layers()
            .iter()
            .rev()
            .find(|l| l.visible && Some(l.kind()) == kind)
            .or_else(|| screen.layers().iter().rev().find(|l| l.visible))
            .unwrap_or(&screen.layers()[0])
            .id
            .clone();
        let name = screen.name.clone();
        self.editor.screen = id.clone();
        self.editor.layer = layer;
        self.editor.isolation = None;
        self.select(None);
        self.paint.marquee = None;
        self.mark_edited();
        self.editor.cursor = Position::ORIGIN;
        self.editor.viewport.offset = Position::ORIGIN;
        self.set_status(StatusKind::Info, format!("Screen: {name}"));
    }

    pub(super) fn submit_new_screen(&mut self, value: &str) {
        let value = value.trim();
        let (name, size) = match value
            .rsplit_once(' ')
            .and_then(|(n, s)| Some((n, parse_size(s)?)))
        {
            Some((n, s)) => (n.trim().to_owned(), s),
            None => (value.to_owned(), self.screen().size()),
        };
        let name = if name.is_empty() {
            "Screen".to_owned()
        } else {
            name
        };
        let ids = self.doc().ids();
        let id = ObjectId::slugify(&name).unique_among(|i| ids.contains(i));
        let mut screen = match Screen::new(id.clone(), &name, size) {
            Ok(s) => s,
            Err(e) => return self.report_error(&e),
        };
        let mut taken = ids;
        taken.push(id.clone());
        for l in screen.layers_mut() {
            l.id = l.id.unique_among(|i| taken.contains(i));
            taken.push(l.id.clone());
        }
        if self.apply(
            Patch::single(Operation::CreateScreen {
                screen,
                index: None,
            }),
            "New screen",
        ) {
            self.switch_screen(&id);
        }
    }

    pub(super) fn submit_rename_screen(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let target = self.editor.screen.clone();
        if self.apply(
            Patch::single(Operation::UpdateScreen {
                target,
                name: Some(name.to_owned()),
                width: None,
                height: None,
            }),
            "Rename screen",
        ) {
            self.set_status(StatusKind::Info, format!("Screen renamed to {name}"));
        }
    }

    pub(super) fn submit_resize_screen(&mut self, value: &str) {
        let Some(size) = parse_size(value) else {
            return self.set_status(StatusKind::Warning, format!("{value:?} is not WxH"));
        };
        let target = self.editor.screen.clone();
        if self.apply(
            Patch::single(Operation::UpdateScreen {
                target,
                name: None,
                width: Some(size.width),
                height: Some(size.height),
            }),
            "Resize screen",
        ) {
            self.editor.preview_size = None;
            self.dispatch(crate::actions::Action::CursorTo(self.editor.cursor));
            self.set_status(
                StatusKind::Info,
                format!(
                    "Screen is now {}×{} (Ctrl+Z restores)",
                    size.width, size.height
                ),
            );
        }
    }

    fn duplicate_screen(&mut self) {
        let source = self.screen().clone();
        let mut taken = self.doc().ids();
        let id =
            ObjectId::slugify(&format!("{}-copy", source.id)).unique_among(|i| taken.contains(i));
        taken.push(id.clone());
        let mut copy = source.clone();
        copy.id = id.clone();
        copy.name = format!("{} copy", source.name);
        let layers: Vec<Layer> = source
            .layers()
            .iter()
            .map(|l| {
                let new_id = ObjectId::slugify(&format!("{id}-{}", l.id));
                copy_layer(l, &new_id, &mut taken)
            })
            .collect();
        *copy.layers_mut() = layers;
        if self.apply(
            Patch::single(Operation::CreateScreen {
                screen: copy,
                index: None,
            }),
            "Duplicate screen",
        ) {
            self.switch_screen(&id);
        }
    }

    // --- preview sizes and minimap ---------------------------------------------

    /// F7: shows the screen at the next preview size (its own size, then
    /// the presets); responsive rules and layout follow. Edits still go to
    /// the document.
    pub fn cycle_preview_size(&mut self) {
        let own = self.screen().size();
        let sizes: Vec<Size> = PREVIEW_SIZES
            .iter()
            .map(|(w, h)| Size::new(*w, *h))
            .filter(|s| *s != own)
            .collect();
        let next = match self.editor.preview_size {
            None => sizes.first().copied(),
            Some(current) => sizes
                .iter()
                .position(|s| *s == current)
                .and_then(|i| sizes.get(i + 1))
                .copied(),
        };
        self.editor.preview_size = next;
        self.mark_edited();
        self.dispatch(crate::actions::Action::CursorTo(self.editor.cursor));
        let text = match next {
            Some(s) => format!(
                "Preview at {}×{} (F7 next size; the screen stays {}×{})",
                s.width, s.height, own.width, own.height
            ),
            None => format!("Screen size {}×{}", own.width, own.height),
        };
        self.set_status(StatusKind::Info, text);
    }

    /// Scrolls so that `pos` is in the middle of the view (minimap click).
    pub fn center_view_on(&mut self, pos: Position) {
        let view = self.editor.viewport.size;
        let doc = self.doc_size();
        self.editor.viewport.offset = Position::new(
            pos.x.saturating_sub(view.width / 2),
            pos.y.saturating_sub(view.height / 2),
        );
        self.editor.viewport.clamp(doc);
    }

    pub fn toggle_minimap(&mut self) {
        self.ui.minimap = !self.ui.minimap;
        let state = if self.ui.minimap { "on" } else { "off" };
        self.set_status(StatusKind::Info, format!("Minimap {state}"));
    }

    // --- find ------------------------------------------------------------------

    /// Ctrl+F: components (by id, kind and text), screens, layers and text
    /// on artwork layers, across the whole document.
    pub fn open_find(&mut self) {
        let mut targets = Vec::new();
        for screen in self.doc().screens() {
            targets.push((
                format!("screen {} · {}", screen.id, screen.name),
                FindTarget::Screen(screen.id.clone()),
            ));
            for layer in screen.layers() {
                targets.push((
                    format!("layer {} · {} on {}", layer.id, layer.name, screen.name),
                    FindTarget::Layer(layer.id.clone()),
                ));
                if let Some(roots) = layer.components() {
                    for c in glyphforge_core::component::tree::iter(roots) {
                        let text = ["text", "label", "title"]
                            .iter()
                            .find_map(|k| c.prop_str(k))
                            .map(|t| format!(" {t:?}"))
                            .unwrap_or_default();
                        targets.push((
                            format!("{} · {}{text}", c.id, c.kind),
                            FindTarget::Component(c.id.clone()),
                        ));
                    }
                }
                if let Some(cells) = layer.cells() {
                    for y in 0..cells.height() {
                        let row: String = (0..cells.width())
                            .map(|x| {
                                cells
                                    .get(Position::new(x, y))
                                    .and_then(|c| c.as_glyph())
                                    .map_or(" ", |g| g.as_str())
                            })
                            .collect();
                        let trimmed = row.trim();
                        if trimmed.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
                            continue;
                        }
                        let x = (0..cells.width())
                            .find(|x| {
                                cells
                                    .get(Position::new(*x, y))
                                    .is_some_and(|c| !c.is_empty())
                            })
                            .unwrap_or(0);
                        targets.push((
                            format!("text {trimmed:?} · {} {x},{y}", layer.name),
                            FindTarget::Text(layer.id.clone(), Position::new(x, y)),
                        ));
                    }
                }
            }
        }
        let candidates = targets.iter().map(|(t, _)| t.clone()).collect();
        self.find_targets = targets;
        self.prompt = Some(Prompt::new(
            PromptKind::Find,
            "Find: component, screen, layer or text",
            candidates,
        ));
    }

    pub(super) fn submit_find(&mut self, prompt: &Prompt) {
        let chosen = prompt.selected_match().map(str::to_owned);
        let target = chosen.and_then(|c| {
            self.find_targets
                .iter()
                .find(|(t, _)| *t == c)
                .map(|(_, target)| target.clone())
        });
        self.find_targets.clear();
        let Some(target) = target else {
            return self.set_status(StatusKind::Info, "Nothing found");
        };
        match target {
            FindTarget::Component(id) => self.jump_to_component(&id),
            FindTarget::Screen(id) => self.switch_screen(&id),
            FindTarget::Layer(ref id) | FindTarget::Text(ref id, _) => {
                let screen = self.doc().screen_of_layer(id).map(|s| s.id.clone());
                if let Some(screen) = screen.filter(|s| *s != self.editor.screen) {
                    self.switch_screen(&screen);
                }
                self.activate_layer(id);
                if let FindTarget::Text(_, pos) = target {
                    self.dispatch(crate::actions::Action::CursorTo(pos));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;

    fn submit(app: &mut App, text: &str) {
        if let Some(p) = app.prompt.as_mut() {
            p.input = text.to_owned();
        }
        app.dispatch(Action::PromptSubmit);
    }

    #[test]
    fn layer_list_actions_are_undoable_patches() {
        let mut app = test_app(Size::new(10, 3));
        let art = app.editor.layer.clone();
        app.dispatch(Action::InsertText("ab".into()));
        app.dispatch(Action::Layer(LayerAction::New));
        submit(&mut app, "artwork Top");
        let top = app.editor.layer.clone();
        assert_ne!(top, art);
        app.dispatch(Action::CursorTo(Position::ORIGIN));
        app.dispatch(Action::InsertText("X".into()));
        app.dispatch(Action::Layer(LayerAction::MergeDown));
        assert_eq!(app.editor.layer, art);
        assert!(app.screen().layer(&top).is_none());
        let glyph = |app: &App, x| {
            app.cell_at(Position::new(x, 0))
                .and_then(|c| c.as_glyph())
                .map(|g| g.as_str().to_owned())
        };
        assert_eq!(glyph(&app, 0).as_deref(), Some("X"), "upper cells win");
        assert_eq!(glyph(&app, 1).as_deref(), Some("b"));
        app.dispatch(Action::Undo);
        assert!(
            app.screen().layer(&top).is_some(),
            "merge undoes in one step"
        );
        // Isolate hides the others; again restores them.
        app.dispatch(Action::Layer(LayerAction::Isolate));
        assert_eq!(
            app.screen().layers().iter().filter(|l| l.visible).count(),
            1
        );
        app.dispatch(Action::Layer(LayerAction::Isolate));
        assert!(app.screen().layers().iter().all(|l| l.visible));
        app.dispatch(Action::Layer(LayerAction::ToggleLock(None)));
        assert!(app.active_layer().unwrap().locked);
        app.dispatch(Action::Layer(LayerAction::Rename));
        submit(&mut app, "Base");
        assert_eq!(app.active_layer().unwrap().name, "Base");
        app.dispatch(Action::Layer(LayerAction::Duplicate));
        assert_eq!(app.active_layer().unwrap().name, "Base copy");
        app.dispatch(Action::Layer(LayerAction::Raise(false)));
        assert_eq!(app.screen().layer_index(&app.editor.layer), Some(0));
    }

    #[test]
    fn screens_switch_resize_duplicate_and_find() {
        let mut app = test_app(Size::new(20, 6));
        app.dispatch(Action::InsertText("Hello there".into()));
        app.dispatch(Action::Screen(ScreenAction::New));
        submit(&mut app, "Settings 30x8");
        assert_eq!(app.screen().name, "Settings");
        assert_eq!(app.doc_size(), Size::new(30, 8));
        app.dispatch(Action::Screen(ScreenAction::Resize));
        submit(&mut app, "40x10");
        assert_eq!(app.doc_size(), Size::new(40, 10));
        app.dispatch(Action::Screen(ScreenAction::Duplicate));
        assert_eq!(app.doc().screens().len(), 3);
        assert!(app.doc().duplicate_ids().is_empty());
        // Find the text on the first screen and jump there.
        app.dispatch(Action::SelectById);
        if let Some(p) = app.prompt.as_mut() {
            p.input = "hello".into();
        }
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.screen().id, app.doc().first_screen().id);
        assert_eq!(app.editor.cursor, Position::new(0, 0));
        // Preview sizes do not change the document.
        app.dispatch(Action::CyclePreviewSize);
        assert_eq!(app.doc_size(), Size::new(60, 20));
        assert_eq!(app.screen().size(), Size::new(20, 6));
        assert_eq!(parse_size("80×24"), Some(Size::new(80, 24)));
        assert_eq!(parse_size("80"), None);
    }
}
