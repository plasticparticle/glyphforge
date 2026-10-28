//! Rectangle selection on artwork layers (the Select tool), and the
//! clipboard for both editing models: cells on artwork layers, component
//! subtrees on interface layers. Copies also go to the system clipboard as
//! text (cells as rows of glyphs, components as JSON) through the run
//! loop, which owns the terminal.

use glyphforge_core::patch::{CellWrite, Operation, Patch};
use glyphforge_core::{Cell, Component, Position, Rect, paint};

use super::{App, DesignMode, StatusKind, Tool};

/// Cells cut or copied from an artwork layer, relative to their top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellClip {
    pub width: u16,
    pub height: u16,
    /// Non-empty cells (wide-glyph tails left out; `put` rebuilds them).
    pub cells: Vec<(u16, u16, Cell)>,
}

impl CellClip {
    /// The clip as plain text, one line per row, trailing blanks and
    /// trailing empty lines trimmed.
    pub fn text(&self) -> String {
        let mut rows =
            vec![vec![" ".to_owned(); usize::from(self.width)]; usize::from(self.height)];
        for (x, y, cell) in &self.cells {
            if let Some(g) = cell.as_glyph() {
                g.as_str()
                    .clone_into(&mut rows[usize::from(*y)][usize::from(*x)]);
                if g.width() == 2 && usize::from(*x) + 1 < usize::from(self.width) {
                    rows[usize::from(*y)][usize::from(*x) + 1] = String::new();
                }
            }
        }
        let mut lines: Vec<String> = rows
            .iter()
            .map(|r| r.concat().trim_end().to_owned())
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clipboard {
    Cells(CellClip),
    Components(Vec<Component>),
}

/// An in-progress drag of the selected cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellMove {
    pub grab: Position,
    pub at: Position,
    pub clip: CellClip,
}

impl CellMove {
    /// Top-left of the dragged cells.
    pub fn origin(&self, from: Rect) -> Position {
        let dx = i32::from(self.at.x) - i32::from(self.grab.x);
        let dy = i32::from(self.at.y) - i32::from(self.grab.y);
        Position::new(
            (i32::from(from.x) + dx).max(0) as u16,
            (i32::from(from.y) + dy).max(0) as u16,
        )
    }
}

impl App {
    /// The cells of `rect` on the active artwork layer.
    fn clip_of(&self, rect: Rect) -> Option<CellClip> {
        let canvas = self.active_layer()?.cells()?;
        let rect = rect.intersection(canvas.bounds());
        let cells = rect
            .positions()
            .filter_map(|p| {
                canvas
                    .get(p)
                    .filter(|c| !c.is_empty() && !c.is_wide_tail())
                    .map(|c| (p.x - rect.x, p.y - rect.y, c.clone()))
            })
            .collect();
        Some(CellClip {
            width: rect.width,
            height: rect.height,
            cells,
        })
    }

    /// Writes that place `clip` with its top-left at `origin`; empty cells
    /// of the clip are transparent and leave what is beneath.
    fn clip_writes(&self, clip: &CellClip, origin: Position) -> Vec<CellWrite> {
        let cells = clip.cells.iter().map(|(x, y, cell)| {
            (
                Position::new(origin.x.saturating_add(*x), origin.y.saturating_add(*y)),
                cell.clone(),
            )
        });
        paint::stamp_each(cells, self.doc_size())
    }

    fn clear_writes(&self, rect: Rect) -> Vec<CellWrite> {
        let positions: Vec<Position> = rect
            .intersection(Rect::from_size(self.doc_size()))
            .positions()
            .collect();
        paint::stamp(&positions, &Cell::EMPTY, self.doc_size())
    }

    // --- the Select tool -----------------------------------------------------

    pub(super) fn select_press(&mut self, pos: Position) {
        if let Some(rect) = self.paint.marquee.filter(|r| r.contains(pos)) {
            if let Some(clip) = self.clip_of(rect) {
                self.paint.moving = Some(CellMove {
                    grab: pos,
                    at: pos,
                    clip,
                });
                return;
            }
        }
        self.paint.marquee = None;
        self.paint.anchor = Some(pos);
    }

    pub(super) fn select_drag(&mut self, pos: Position) -> bool {
        match self.paint.moving.as_mut() {
            Some(m) => {
                m.at = pos;
                true
            }
            None => false,
        }
    }

    pub(super) fn select_release(&mut self) -> bool {
        let Some(m) = self.paint.moving.take() else {
            return false;
        };
        let Some(from) = self.paint.marquee else {
            return true;
        };
        let to = m.origin(from);
        if to == Position::new(from.x, from.y) || !self.editable_cells_layer() {
            return true;
        }
        let mut writes = self.clear_writes(from);
        writes.extend(self.clip_writes(&m.clip, to));
        if self.apply_cells(writes, "Move selection") {
            self.paint.marquee = Some(Rect::new(to.x, to.y, from.width, from.height));
        }
        true
    }

    /// Finishes a marquee drawn with the mouse or two presses of Space.
    pub(super) fn set_marquee(&mut self, anchor: Position, to: Position) {
        let rect = paint::rect_between(anchor, to);
        self.paint.marquee = Some(rect);
        self.set_status(
            StatusKind::Info,
            format!(
                "Selected {}×{} at {},{}: drag to move, Ctrl+C/X copy/cut, Delete clears",
                rect.width, rect.height, rect.x, rect.y
            ),
        );
    }

    /// Ctrl+A on an artwork layer.
    pub(super) fn select_all_cells(&mut self) {
        self.paint.tool = Tool::Select;
        let size = self.doc_size();
        self.set_marquee(
            Position::ORIGIN,
            Position::new(size.width.saturating_sub(1), size.height.saturating_sub(1)),
        );
    }

    /// Delete with a marquee: clears its cells.
    pub(super) fn clear_marquee(&mut self) -> bool {
        let Some(rect) = self.paint.marquee else {
            return false;
        };
        if self.editable_cells_layer() {
            let writes = self.clear_writes(rect);
            self.apply_cells(writes, "Clear selection");
        }
        true
    }

    fn apply_cells(&mut self, cells: Vec<CellWrite>, label: &str) -> bool {
        if cells.is_empty() {
            return false;
        }
        self.apply(
            Patch::single(Operation::SetCells {
                layer: self.editor.layer.clone(),
                cells,
            }),
            label,
        )
    }

    // --- clipboard -------------------------------------------------------------

    /// The top-level components of the selection (children of another
    /// selected component travel with it).
    fn selected_subtrees(&self) -> Vec<Component> {
        let ids = self.selected_ids();
        let doc = self.doc();
        ids.iter()
            .filter(|id| {
                !ids.iter().any(|other| {
                    other != *id
                        && doc
                            .component(other)
                            .is_some_and(|c| c.iter().any(|d| &d.id == *id))
                })
            })
            .filter_map(|id| doc.component(id).cloned())
            .collect()
    }

    pub fn copy(&mut self) {
        self.copy_or_cut(false);
    }

    pub fn cut(&mut self) {
        self.copy_or_cut(true);
    }

    fn copy_or_cut(&mut self, cut: bool) {
        let verb = if cut { "Cut" } else { "Copied" };
        match self.design_mode() {
            DesignMode::Subcell => {
                let Some(rect) = self.paint.marquee else {
                    self.set_status(
                        StatusKind::Info,
                        "Select cells first: the Select tool (v) or Ctrl+A",
                    );
                    return;
                };
                let Some(clip) = self.clip_of(rect) else {
                    return;
                };
                self.system_clipboard = Some(clip.text());
                self.clipboard = Some(Clipboard::Cells(clip));
                if cut {
                    self.clear_marquee();
                }
                self.set_status(
                    StatusKind::Info,
                    format!("{verb} {}×{} cells", rect.width, rect.height),
                );
            }
            DesignMode::Interface => {
                let trees = self.selected_subtrees();
                if trees.is_empty() {
                    self.set_status(StatusKind::Info, "Select components first");
                    return;
                }
                self.system_clipboard = serde_json::to_string_pretty(&trees).ok();
                let n = trees.len();
                self.clipboard = Some(Clipboard::Components(trees));
                if cut {
                    self.delete_selection();
                }
                self.set_status(StatusKind::Info, format!("{verb} {n} component(s)"));
            }
            DesignMode::Pixel => self.set_status(
                StatusKind::Info,
                "Copy and paste work on artwork and interface layers",
            ),
        }
    }

    pub fn paste(&mut self) {
        match (self.clipboard.clone(), self.design_mode()) {
            (None, _) => self.set_status(StatusKind::Info, "The clipboard is empty"),
            (Some(Clipboard::Cells(clip)), DesignMode::Subcell) => {
                if !self.editable_cells_layer() {
                    return;
                }
                let origin = self
                    .paint
                    .marquee
                    .map_or(self.editor.cursor, |r| Position::new(r.x, r.y));
                let writes = self.clip_writes(&clip, origin);
                if self.apply_cells(writes, "Paste") {
                    self.paint.tool = Tool::Select;
                    self.paint.marquee =
                        Some(Rect::new(origin.x, origin.y, clip.width, clip.height));
                    self.set_status(StatusKind::Info, "Pasted: drag the selection to move it");
                }
            }
            (Some(Clipboard::Components(trees)), DesignMode::Interface) => {
                self.paste_components(&trees, "Paste");
            }
            (Some(Clipboard::Cells(_)), _) => self.set_status(
                StatusKind::Info,
                "The clipboard holds cells: paste them on an artwork layer",
            ),
            (Some(Clipboard::Components(_)), _) => self.set_status(
                StatusKind::Info,
                "The clipboard holds components: paste them on an interface layer",
            ),
        }
    }

    /// Ctrl+D: copies the selected components next to themselves without
    /// touching the clipboard.
    pub fn duplicate(&mut self) {
        let trees = self.selected_subtrees();
        if trees.is_empty() {
            self.set_status(StatusKind::Info, "Select components to duplicate");
            return;
        }
        self.paste_components(&trees, "Duplicate");
    }

    /// Inserts copies of `trees` with fresh ids beside the primary
    /// selection (or as roots), absolutely placed ones offset by one cell.
    fn paste_components(&mut self, trees: &[Component], label: &str) {
        let parent = self.selection().and_then(|id| {
            self.active_layer()
                .and_then(|l| l.components())
                .and_then(|roots| glyphforge_core::component::tree::locate(roots, id))
                .and_then(|(parent, _)| parent)
        });
        let mut taken = self.doc().ids();
        let mut ops = Vec::new();
        let mut new_ids = Vec::new();
        for tree in trees {
            let mut copy = tree.clone();
            reid(&mut copy, &mut taken);
            if let glyphforge_core::layout::Placement::Absolute { x, y } = copy.layout.placement {
                copy.layout.placement = glyphforge_core::layout::Placement::Absolute {
                    x: x.saturating_add(1),
                    y: y.saturating_add(1),
                };
            }
            new_ids.push(copy.id.clone());
            ops.push(Operation::CreateComponent {
                component: copy,
                parent: parent.clone(),
                layer: Some(self.editor.layer.clone()),
                index: None,
            });
        }
        if self.apply(Patch::new(ops), label) {
            let primary = new_ids.pop();
            self.select(primary);
            self.editor.also_selected = new_ids;
            let n = self.selected_ids().len();
            self.set_status(StatusKind::Info, format!("{label}: {n} component(s)"));
        }
    }

    /// Text waiting for the system clipboard; the run loop sends it.
    pub fn take_system_clipboard(&mut self) -> Option<String> {
        self.system_clipboard.take()
    }

    /// The marquee being drawn or set, for the overlay.
    pub fn marquee_preview(&self) -> Option<Rect> {
        if self.paint.tool != Tool::Select || !self.on_artwork_layer() {
            return None;
        }
        match (self.paint.anchor, self.paint.marquee, &self.paint.moving) {
            (Some(a), _, _) => Some(paint::rect_between(a, self.editor.cursor)),
            (None, Some(r), Some(m)) => {
                let o = m.origin(r);
                Some(Rect::new(o.x, o.y, r.width, r.height))
            }
            (None, marquee, _) => marquee,
        }
    }

    /// While dragging the selection: the source to blank and the writes at
    /// the new place.
    pub fn move_preview(&self) -> Option<(Rect, Vec<CellWrite>)> {
        let m = self.paint.moving.as_ref()?;
        let from = self.paint.marquee?;
        Some((from, self.clip_writes(&m.clip, m.origin(from))))
    }
}

/// Gives `c` and its descendants ids not in `taken` (and records them).
pub(super) fn reid(c: &mut Component, taken: &mut Vec<glyphforge_core::ObjectId>) {
    let id = c.id.unique_among(|i| taken.contains(i));
    taken.push(id.clone());
    c.id = id;
    for child in &mut c.children {
        reid(child, taken);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::Size;

    fn text_at(app: &App, x: u16, y: u16) -> String {
        app.cell_at(Position::new(x, y))
            .and_then(Cell::as_glyph)
            .map_or_else(|| ".".to_owned(), |g| g.as_str().to_owned())
    }

    #[test]
    fn select_move_copy_paste_cells() {
        let mut app = test_app(Size::new(12, 4));
        app.dispatch(Action::InsertText("ab漢".into()));
        app.dispatch(Action::SelectTool(Tool::Select));
        app.dispatch(Action::ToolPress(Position::new(0, 0)));
        app.dispatch(Action::ToolDrag(Position::new(3, 0)));
        app.dispatch(Action::ToolRelease);
        // The cursor never rests on a wide tail: the marquee ends on 漢.
        assert_eq!(app.paint.marquee, Some(Rect::new(0, 0, 3, 1)));
        // Drag the selection two rows down: one undo step.
        let before = app.session.history().undo_len();
        app.dispatch(Action::ToolPress(Position::new(1, 0)));
        app.dispatch(Action::ToolDrag(Position::new(2, 2)));
        assert!(app.move_preview().is_some());
        app.dispatch(Action::ToolRelease);
        assert_eq!(text_at(&app, 0, 0), ".");
        assert_eq!(text_at(&app, 1, 2), "a");
        assert_eq!(text_at(&app, 3, 2), "漢");
        assert_eq!(app.session.history().undo_len(), before + 1);
        // Copy, paste at the cursor.
        app.dispatch(Action::Copy);
        assert_eq!(app.take_system_clipboard().as_deref(), Some("ab漢"));
        app.paint.marquee = None;
        app.dispatch(Action::CursorTo(Position::new(6, 0)));
        app.dispatch(Action::Paste);
        assert_eq!(text_at(&app, 6, 0), "a");
        assert_eq!(text_at(&app, 8, 0), "漢");
        // Cut clears; Delete clears too.
        app.dispatch(Action::Cut);
        assert_eq!(text_at(&app, 6, 0), ".");
        app.dispatch(Action::SelectAll);
        app.dispatch(Action::DeleteForward);
        assert_eq!(text_at(&app, 1, 2), ".");
    }

    #[test]
    fn copy_paste_and_duplicate_components_get_fresh_ids() {
        let mut app = test_app(Size::new(40, 10));
        app.dispatch(Action::LayerNext);
        let layer = app.editor.layer.clone();
        let id = app
            .session
            .add_component("panel", Some("card"), 1, 1, None, Some(layer))
            .unwrap();
        app.mark_edited();
        app.select(Some(id.clone()));
        app.dispatch(Action::Copy);
        assert!(
            app.take_system_clipboard()
                .unwrap()
                .contains("\"kind\": \"panel\"")
        );
        app.dispatch(Action::Paste);
        let pasted = app.selection().cloned().unwrap();
        assert_ne!(pasted, id);
        assert_eq!(
            app.layout().rect(&pasted).unwrap().x,
            app.layout().rect(&id).unwrap().x + 1
        );
        app.dispatch(Action::Duplicate);
        assert_eq!(app.doc().components().count(), 3);
        app.dispatch(Action::Undo);
        assert_eq!(app.doc().components().count(), 2);
    }

    #[test]
    fn clip_text_keeps_wide_glyphs_and_trims() {
        let clip = CellClip {
            width: 4,
            height: 2,
            cells: vec![
                (
                    0,
                    0,
                    Cell::glyph(
                        glyphforge_core::Grapheme::new("漢").unwrap(),
                        glyphforge_core::CellStyle::default(),
                    ),
                ),
                (
                    3,
                    1,
                    Cell::glyph(
                        glyphforge_core::Grapheme::new("x").unwrap(),
                        glyphforge_core::CellStyle::default(),
                    ),
                ),
            ],
        };
        assert_eq!(clip.text(), "漢\n   x");
    }

    #[test]
    fn brush_size_paints_squares_and_attributes_toggle() {
        let mut app = test_app(Size::new(10, 6));
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(Action::Pixel(crate::actions::PixelAction::SizeUp));
        app.dispatch(Action::Pixel(crate::actions::PixelAction::SizeUp));
        assert_eq!(app.paint.size, 3);
        app.dispatch(Action::ToggleAttr(crate::actions::Attr::Bold));
        app.dispatch(Action::CursorTo(Position::new(4, 2)));
        app.dispatch(Action::ApplyTool);
        for (x, y) in [(3, 1), (5, 3), (4, 2)] {
            let cell = app.cell_at(Position::new(x, y)).unwrap();
            assert_eq!(text_at(&app, x, y), "█");
            assert!(cell.style.attrs.bold);
        }
        assert_eq!(text_at(&app, 6, 2), ".");
        assert_eq!(app.session.history().undo_len(), 1, "one dab, one step");
    }
}
