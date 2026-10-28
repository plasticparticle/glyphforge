//! Arranging the selection: align, distribute, equal size, and snapping
//! while dragging. The geometry lives in `glyphforge_core::align`; this
//! module turns its rectangles into `move` and `resize` operations.

use glyphforge_core::align::{self, Axis, Edge, Guide};
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::{ObjectId, Rect};

use super::{App, StatusKind};

/// Grid sizes `Shift+G` cycles through.
const GRIDS: [u16; 4] = [0, 2, 4, 8];

impl App {
    /// Screen rects of the selected components, primary last.
    fn selection_rects(&mut self) -> Vec<(ObjectId, Rect)> {
        let ids = self.selected_ids();
        let layout = self.layout();
        ids.into_iter()
            .filter_map(|id| layout.rect(&id).map(|r| (id, r)))
            .collect()
    }

    /// Issues `move`/`resize` for every rect that changed, as one patch.
    fn apply_rects(&mut self, before: &[(ObjectId, Rect)], after: &[Rect], label: &str) {
        let mut ops = Vec::new();
        for ((id, old), new) in before.iter().zip(after) {
            if (old.x, old.y) != (new.x, new.y) {
                let inner = self.parent_inner(id);
                ops.push(Operation::Move {
                    target: id.clone(),
                    x: new.x.saturating_sub(inner.x),
                    y: new.y.saturating_sub(inner.y),
                });
            }
            if (old.width, old.height) != (new.width, new.height) {
                ops.push(Operation::Resize {
                    target: id.clone(),
                    width: new.width.max(1),
                    height: new.height.max(1),
                });
            }
        }
        if ops.is_empty() {
            self.set_status(StatusKind::Info, format!("{label}: already in place"));
        } else if self.apply(Patch::new(ops), label) {
            self.set_status(StatusKind::Info, label.to_owned());
        }
    }

    /// Aligns the selection on `edge` of its bounds; a single component
    /// aligns inside its parent (or the screen).
    pub fn align_selection(&mut self, edge: Edge) {
        let before = self.selection_rects();
        let label = format!("Align {}", edge.name());
        match before.as_slice() {
            [] => self.set_status(StatusKind::Info, "Select components to align"),
            [(id, rect)] => {
                let parent = self.parent_inner(id);
                let after = align::align(&[parent, *rect], edge);
                self.apply_rects(&before, &after[1..], &label);
            }
            many => {
                let rects: Vec<Rect> = many.iter().map(|(_, r)| *r).collect();
                let after = align::align(&rects, edge);
                self.apply_rects(&before, &after, &label);
            }
        }
    }

    pub fn distribute_selection(&mut self, axis: Axis) {
        let before = self.selection_rects();
        if before.len() < 3 {
            self.set_status(
                StatusKind::Info,
                "Select at least three components to distribute (Shift+click)",
            );
            return;
        }
        let rects: Vec<Rect> = before.iter().map(|(_, r)| *r).collect();
        let after = align::distribute(&rects, axis);
        let label = match axis {
            Axis::Horizontal => "Distribute horizontally",
            Axis::Vertical => "Distribute vertically",
        };
        self.apply_rects(&before, &after, label);
    }

    /// Gives every selected component the primary selection's size.
    pub fn equal_size_selection(&mut self, width: bool, height: bool) {
        let before = self.selection_rects();
        let Some((_, reference)) = before.last().cloned() else {
            self.set_status(StatusKind::Info, "Select components to resize");
            return;
        };
        if before.len() < 2 {
            self.set_status(
                StatusKind::Info,
                "Select two or more components; the last one sets the size",
            );
            return;
        }
        let rects: Vec<Rect> = before.iter().map(|(_, r)| *r).collect();
        let after = align::equal_size(&rects, reference, width, height);
        let label = match (width, height) {
            (true, false) => "Equal width",
            (false, true) => "Equal height",
            _ => "Equal size",
        };
        self.apply_rects(&before, &after, label);
    }

    pub fn toggle_snap(&mut self) {
        self.snap.neighbours = !self.snap.neighbours;
        let state = if self.snap.neighbours { "on" } else { "off" };
        self.set_status(StatusKind::Info, format!("Snap to components {state}"));
    }

    pub fn cycle_snap_grid(&mut self) {
        let i = GRIDS.iter().position(|g| *g == self.snap.grid).unwrap_or(0);
        self.snap.grid = GRIDS[(i + 1) % GRIDS.len()];
        let text = match self.snap.grid {
            0 => "Snap grid off".to_owned(),
            g => format!("Snap grid {g} cells"),
        };
        self.set_status(StatusKind::Info, text);
    }

    /// Rects a drag of `moving` can snap to: every other component on the
    /// layer except the dragged one's own descendants, plus the screen.
    pub(super) fn snap_targets(&mut self, moving: &ObjectId) -> Vec<Rect> {
        let own: Vec<ObjectId> = self
            .doc()
            .component(moving)
            .map(|c| c.iter().map(|d| d.id.clone()).collect())
            .unwrap_or_default();
        let mut rects = vec![Rect::from_size(self.doc_size())];
        rects.extend(
            self.layout()
                .iter()
                .filter(|(id, _)| !own.contains(id))
                .map(|(_, r)| *r),
        );
        rects
    }

    /// Snaps a dragged rect (screen coordinates) and records the guides.
    pub(super) fn snap_drag(&mut self, moving: &ObjectId, rect: Rect) -> Rect {
        let targets = self.snap_targets(moving);
        let (snapped, guides) = align::snap(rect, &targets, self.snap);
        self.guides = guides;
        snapped
    }

    /// Guides currently shown (while a drag snaps).
    pub fn active_guides(&self) -> &[Guide] {
        if self.drag.is_some() {
            &self.guides
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::align::{Axis, Edge};
    use glyphforge_core::{ObjectId, Position, Size};

    fn app_with(rects: &[(u16, u16, u16, u16)]) -> (crate::app::App, Vec<ObjectId>) {
        let mut app = test_app(Size::new(60, 20));
        app.dispatch(Action::LayerNext);
        let layer = app.editor.layer.clone();
        let mut ids = Vec::new();
        for (i, (x, y, w, h)) in rects.iter().enumerate() {
            let id = app
                .session
                .add_component(
                    "group",
                    Some(&format!("g{i}")),
                    *x,
                    *y,
                    None,
                    Some(layer.clone()),
                )
                .unwrap();
            app.session.resize_component(&id, *w, *h).unwrap();
            ids.push(id);
        }
        app.mark_edited();
        app.select(ids.last().cloned());
        app.editor.also_selected = ids[..ids.len() - 1].to_vec();
        (app, ids)
    }

    #[test]
    fn align_distribute_and_equal_size_are_one_undo_step_each() {
        let (mut app, ids) = app_with(&[(2, 1, 4, 2), (20, 5, 6, 3), (10, 9, 2, 2)]);
        let undo_before = app.session.history().undo_len();
        app.dispatch(Action::Align(Edge::Left));
        for id in &ids {
            assert_eq!(app.layout().rect(id).unwrap().x, 2);
        }
        assert_eq!(app.session.history().undo_len(), undo_before + 1);
        app.dispatch(Action::Undo);
        app.dispatch(Action::Distribute(Axis::Horizontal));
        // Span 2..26, sizes 12, free 12 in two gaps of 6.
        assert_eq!(app.layout().rect(&ids[2]).unwrap().x, 12);
        app.dispatch(Action::EqualSize {
            width: true,
            height: true,
        });
        let primary = app.layout().rect(&ids[2]).unwrap();
        assert_eq!(app.layout().rect(&ids[0]).unwrap().width, primary.width);
        assert_eq!(app.layout().rect(&ids[1]).unwrap().height, primary.height);
    }

    #[test]
    fn single_component_aligns_inside_the_screen() {
        let (mut app, ids) = app_with(&[(5, 5, 10, 2)]);
        app.dispatch(Action::Align(Edge::Right));
        assert_eq!(app.layout().rect(&ids[0]).unwrap().x, 50);
        app.dispatch(Action::Align(Edge::Middle));
        assert_eq!(app.layout().rect(&ids[0]).unwrap().y, 9);
    }

    #[test]
    fn dragging_snaps_to_neighbour_edges_with_guides() {
        let (mut app, ids) = app_with(&[(10, 2, 8, 3), (30, 10, 4, 2)]);
        app.select(Some(ids[1].clone()));
        assert!(app.begin_drag(Position::new(31, 10)));
        // Moving left by 19 puts x at 11, one cell from the neighbour's
        // left edge at 10: within the threshold, so it snaps there.
        app.drag_to(Position::new(12, 10));
        assert_eq!(app.layout().rect(&ids[1]).unwrap().x, 10);
        assert!(!app.active_guides().is_empty());
        app.end_drag();
        assert!(app.active_guides().is_empty());
        app.dispatch(Action::ToggleSnap);
        assert!(!app.snap.neighbours);
        app.dispatch(Action::CycleSnapGrid);
        assert_eq!(app.snap.grid, 2);
    }
}
