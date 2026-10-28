//! Mouse workflow beyond plain clicks: resize handles on every edge and
//! corner, rubber-band selection, double clicks, right-click context
//! menus and z-order. Everything ends in ordinary actions and patches.

use glyphforge_core::component::tree;
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::{ObjectId, Position, Rect};

use super::{App, DesignMode, LayerAction, StatusKind};
use crate::actions::{Action, InspectorAction};

/// Which edges a resize drag moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Edges {
    pub const fn any(self) -> bool {
        self.left || self.top || self.right || self.bottom
    }
}

/// The resize handle of `rect` at `pos`: the four corners, and the middle
/// of each edge when the rect is long enough to have one.
pub fn handle_at(rect: Rect, pos: Position) -> Edges {
    if rect.width < 2 || rect.height < 2 {
        return Edges::default();
    }
    let (l, t) = (rect.x, rect.y);
    let (r, b) = ((rect.right() - 1) as u16, (rect.bottom() - 1) as u16);
    let (mx, my) = (l + (rect.width - 1) / 2, t + (rect.height - 1) / 2);
    let at = |x: u16, y: u16| pos == Position::new(x, y);
    let wide = rect.width >= 5;
    let tall = rect.height >= 3;
    Edges {
        left: at(l, t) || at(l, b) || (tall && at(l, my)),
        top: at(l, t) || at(r, t) || (wide && at(mx, t)),
        right: at(r, t) || at(r, b) || (tall && at(r, my)),
        bottom: at(l, b) || at(r, b) || (wide && at(mx, b)),
    }
}

/// Handle positions of `rect` for drawing: corners, then edge middles.
pub fn handles(rect: Rect) -> Vec<Position> {
    if rect.width < 2 || rect.height < 2 {
        return Vec::new();
    }
    let (l, t) = (rect.x, rect.y);
    let (r, b) = ((rect.right() - 1) as u16, (rect.bottom() - 1) as u16);
    let (mx, my) = (l + (rect.width - 1) / 2, t + (rect.height - 1) / 2);
    let mut out = vec![
        Position::new(l, t),
        Position::new(r, t),
        Position::new(l, b),
        Position::new(r, b),
    ];
    if rect.width >= 5 {
        out.extend([Position::new(mx, t), Position::new(mx, b)]);
    }
    if rect.height >= 3 {
        out.extend([Position::new(l, my), Position::new(r, my)]);
    }
    out
}

/// Where a component moves in its siblings' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZOrder {
    Forward,
    Backward,
    Front,
    Back,
}

/// A right-click menu: entries with the action each runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu {
    /// Terminal cell the menu opens at.
    pub at: (u16, u16),
    pub title: String,
    pub entries: Vec<(String, Action)>,
    pub selected: usize,
}

/// What the right mouse button was pressed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextTarget {
    Canvas(Position),
    Layer(ObjectId),
}

impl App {
    // --- rubber band -----------------------------------------------------------

    /// Starts a rubber-band selection on empty canvas (Interface Mode).
    pub fn begin_band(&mut self, pos: Position) {
        self.band = Some((pos, pos));
    }

    pub fn band_to(&mut self, pos: Position) -> bool {
        match self.band.as_mut() {
            Some(band) => {
                band.1 = pos;
                true
            }
            None => false,
        }
    }

    /// Selects every component lying fully inside the band.
    pub fn end_band(&mut self) -> bool {
        let Some((a, b)) = self.band.take() else {
            return false;
        };
        let band = glyphforge_core::paint::rect_between(a, b);
        if band.width < 2 && band.height < 2 {
            return true;
        }
        let ids: Vec<ObjectId> = {
            let order: Vec<ObjectId> = self
                .active_layer()
                .and_then(|l| l.components())
                .map(|roots| tree::iter(roots).map(|c| c.id.clone()).collect())
                .unwrap_or_default();
            let layout = self.layout();
            order
                .into_iter()
                .filter(|id| {
                    layout
                        .rect(id)
                        .is_some_and(|r| !r.is_empty() && r.intersection(band) == r)
                })
                .collect()
        };
        let mut ids = ids;
        let primary = ids.pop();
        self.select(primary);
        self.editor.also_selected = ids;
        let n = self.selected_ids().len();
        self.set_status(StatusKind::Info, format!("{n} selected"));
        true
    }

    /// The band being dragged, for the overlay.
    pub fn band_rect(&self) -> Option<Rect> {
        self.band
            .map(|(a, b)| glyphforge_core::paint::rect_between(a, b))
    }

    // --- double click ------------------------------------------------------------

    /// Double click on the canvas: edits the text of the component under
    /// the pointer (its `text`, `label` or `title`, whichever it reads).
    pub fn double_click_canvas(&mut self, pos: Position) {
        if self.design_mode() != DesignMode::Interface {
            return;
        }
        let Some(id) = self.component_at(pos) else {
            return;
        };
        self.select(Some(id));
        let fields = self.inspector_fields();
        let row = ["text", "label", "title"].iter().find_map(|name| {
            fields
                .iter()
                .position(|f| f.key == super::FieldKey::Prop((*name).to_owned()))
        });
        if let Some(row) = row {
            self.ui.inspector_row = row;
            self.dispatch(Action::Inspector(InspectorAction::Edit));
        } else {
            self.ui.focus = super::Focus::RightPanel;
            self.set_status(StatusKind::Info, "No text to edit: the inspector has focus");
        }
    }

    // --- context menus -----------------------------------------------------------

    pub fn open_context_menu(&mut self, target: ContextTarget, at: (u16, u16)) {
        let (title, entries) = match target {
            ContextTarget::Layer(id) => {
                self.activate_layer(&id);
                let name = self
                    .active_layer()
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                (
                    name,
                    vec![
                        (
                            "Show / hide".to_owned(),
                            Action::Layer(LayerAction::ToggleVisible(None)),
                        ),
                        (
                            "Lock / unlock".to_owned(),
                            Action::Layer(LayerAction::ToggleLock(None)),
                        ),
                        ("Isolate".to_owned(), Action::Layer(LayerAction::Isolate)),
                        ("Rename…".to_owned(), Action::Layer(LayerAction::Rename)),
                        (
                            "Duplicate".to_owned(),
                            Action::Layer(LayerAction::Duplicate),
                        ),
                        (
                            "Merge down".to_owned(),
                            Action::Layer(LayerAction::MergeDown),
                        ),
                        ("Raise".to_owned(), Action::Layer(LayerAction::Raise(true))),
                        ("Lower".to_owned(), Action::Layer(LayerAction::Raise(false))),
                        ("New layer…".to_owned(), Action::Layer(LayerAction::New)),
                        (
                            "Delete layer".to_owned(),
                            Action::Layer(LayerAction::Delete),
                        ),
                    ],
                )
            }
            ContextTarget::Canvas(pos) => match self.design_mode() {
                DesignMode::Interface => self.component_menu(pos),
                DesignMode::Subcell => {
                    if !self.paint.marquee.is_some_and(|r| r.contains(pos)) {
                        // Without a selection a right click picks colours.
                        self.dispatch(Action::PickAt(pos));
                        return;
                    }
                    (
                        "Selection".to_owned(),
                        vec![
                            ("Copy".to_owned(), Action::Copy),
                            ("Cut".to_owned(), Action::Cut),
                            ("Clear".to_owned(), Action::DeleteForward),
                            (
                                "Flip horizontally".to_owned(),
                                Action::FlipSelection(glyphforge_core::align::Axis::Horizontal),
                            ),
                            (
                                "Flip vertically".to_owned(),
                                Action::FlipSelection(glyphforge_core::align::Axis::Vertical),
                            ),
                            ("Swap colours".to_owned(), Action::InvertSelection),
                            ("Make component".to_owned(), Action::SelectionToComponent),
                        ],
                    )
                }
                DesignMode::Pixel => return,
            },
        };
        self.context_menu = Some(ContextMenu {
            at,
            title,
            entries,
            selected: 0,
        });
    }

    fn component_menu(&mut self, pos: Position) -> (String, Vec<(String, Action)>) {
        let hit = self.component_at(pos);
        let Some(id) = hit else {
            self.select(None);
            return (
                "Layer".to_owned(),
                vec![
                    ("Add component…".to_owned(), Action::AddComponent),
                    ("Paste".to_owned(), Action::Paste),
                    ("Select all".to_owned(), Action::SelectAll),
                    ("Command palette…".to_owned(), Action::CommandPalette),
                ],
            );
        };
        if !self.selected_ids().contains(&id) {
            self.select(Some(id.clone()));
        }
        let many = self.selected_ids().len() > 1;
        let mut entries = vec![
            ("Edit text…".to_owned(), Action::DoubleClick(pos)),
            (
                "Inspect".to_owned(),
                Action::Inspector(InspectorAction::Click(0)),
            ),
            ("Duplicate".to_owned(), Action::Duplicate),
            ("Copy".to_owned(), Action::Copy),
            ("Cut".to_owned(), Action::Cut),
            ("Move into…".to_owned(), Action::Reparent),
            ("Bring to front".to_owned(), Action::ZOrder(ZOrder::Front)),
            ("Bring forward".to_owned(), Action::ZOrder(ZOrder::Forward)),
            ("Send backward".to_owned(), Action::ZOrder(ZOrder::Backward)),
            ("Send to back".to_owned(), Action::ZOrder(ZOrder::Back)),
        ];
        if many {
            use glyphforge_core::align::{Axis, Edge};
            entries.extend([
                ("Align left".to_owned(), Action::Align(Edge::Left)),
                ("Align top".to_owned(), Action::Align(Edge::Top)),
                (
                    "Distribute horizontally".to_owned(),
                    Action::Distribute(Axis::Horizontal),
                ),
                (
                    "Equal size".to_owned(),
                    Action::EqualSize {
                        width: true,
                        height: true,
                    },
                ),
            ]);
        }
        entries.push(("Delete".to_owned(), Action::DeleteSelection));
        let title = if many {
            format!("{} components", self.selected_ids().len())
        } else {
            id.to_string()
        };
        (title, entries)
    }

    pub fn context_move(&mut self, down: bool) {
        if let Some(m) = self.context_menu.as_mut() {
            let n = m.entries.len();
            m.selected = if down {
                (m.selected + 1) % n
            } else {
                (m.selected + n - 1) % n
            };
        }
    }

    /// Runs the highlighted entry (or the one at `index`) and closes.
    pub fn context_run(&mut self, index: Option<usize>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let i = index.unwrap_or(menu.selected);
        if let Some((_, action)) = menu.entries.into_iter().nth(i) {
            self.dispatch(action);
        }
    }

    // --- z-order -----------------------------------------------------------------

    /// Moves the selected components among their siblings (later siblings
    /// draw on top).
    pub fn z_order(&mut self, order: ZOrder) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            self.set_status(StatusKind::Info, "Select components first");
            return;
        }
        let mut ops = Vec::new();
        for id in &ids {
            let Some(roots) = self.active_layer().and_then(|l| l.components()) else {
                return;
            };
            let Some((parent, index)) = tree::locate(roots, id) else {
                continue;
            };
            let siblings = match &parent {
                Some(p) => tree::find(roots, p).map_or(0, |c| c.children.len()),
                None => roots.len(),
            };
            // Indices count the list without the moved component.
            let last = siblings.saturating_sub(1);
            let to = match order {
                ZOrder::Front => last,
                ZOrder::Back => 0,
                ZOrder::Forward => (index + 1).min(last),
                ZOrder::Backward => index.saturating_sub(1),
            };
            if to != index {
                ops.push(Operation::Reparent {
                    target: id.clone(),
                    parent,
                    layer: Some(self.editor.layer.clone()),
                    index: Some(to),
                });
            }
        }
        let label = match order {
            ZOrder::Front => "Bring to front",
            ZOrder::Back => "Send to back",
            ZOrder::Forward => "Bring forward",
            ZOrder::Backward => "Send backward",
        };
        if ops.is_empty() {
            self.set_status(StatusKind::Info, format!("{label}: already there"));
        } else if self.apply(Patch::new(ops), label) {
            self.set_status(StatusKind::Info, label.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use glyphforge_core::Size;

    fn ui_app() -> (App, Vec<ObjectId>) {
        let mut app = test_app(Size::new(40, 12));
        app.snap.neighbours = false;
        app.dispatch(Action::LayerNext);
        let layer = app.editor.layer.clone();
        let ids = [(2, 1), (10, 1), (30, 8)]
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                app.session
                    .add_component(
                        "panel",
                        Some(&format!("p{i}")),
                        *x,
                        *y,
                        None,
                        Some(layer.clone()),
                    )
                    .unwrap()
            })
            .collect();
        app.mark_edited();
        (app, ids)
    }

    #[test]
    fn handles_cover_corners_and_edge_middles() {
        let r = Rect::new(0, 0, 10, 5);
        assert_eq!(handles(r).len(), 8);
        let tl = handle_at(r, Position::new(0, 0));
        assert!(tl.left && tl.top && !tl.right);
        let mid_right = handle_at(r, Position::new(9, 2));
        assert!(mid_right.right && !mid_right.top && !mid_right.bottom);
        assert!(!handle_at(r, Position::new(4, 2)).any(), "inside moves");
        assert!(handles(Rect::new(0, 0, 1, 1)).is_empty());
    }

    #[test]
    fn dragging_the_top_left_handle_moves_and_resizes() {
        let (mut app, ids) = ui_app();
        app.select(Some(ids[0].clone()));
        let r = app.layout().rect(&ids[0]).unwrap();
        assert!(app.begin_drag(Position::new(r.x, r.y)));
        app.drag_to(Position::new(r.x - 1, r.y));
        app.end_drag();
        let after = app.layout().rect(&ids[0]).unwrap();
        assert_eq!((after.x, after.width), (r.x - 1, r.width + 1));
        assert_eq!(after.y, r.y);
    }

    #[test]
    fn band_selects_components_inside() {
        let (mut app, ids) = ui_app();
        app.begin_band(Position::new(0, 0));
        app.band_to(Position::new(35, 9));
        assert!(app.band_rect().is_some());
        app.end_band();
        assert_eq!(app.selected_ids(), vec![ids[0].clone(), ids[1].clone()]);
    }

    #[test]
    fn context_menu_and_z_order() {
        let (mut app, ids) = ui_app();
        let r = app.layout().rect(&ids[0]).unwrap();
        app.open_context_menu(
            ContextTarget::Canvas(Position::new(r.x + 3, r.y + 2)),
            (5, 5),
        );
        let menu = app.context_menu.clone().unwrap();
        assert_eq!(menu.title, "p0");
        let front = menu
            .entries
            .iter()
            .position(|(t, _)| t == "Bring to front")
            .unwrap();
        app.context_run(Some(front));
        let roots = app.active_layer().unwrap().components().unwrap();
        assert_eq!(roots.last().unwrap().id, ids[0]);
        app.dispatch(Action::ZOrder(ZOrder::Back));
        let roots = app.active_layer().unwrap().components().unwrap();
        assert_eq!(roots[0].id, ids[0]);
        // Double click edits the panel's title.
        app.double_click_canvas(Position::new(r.x + 3, r.y + 2));
        assert_eq!(
            app.prompt.as_ref().map(|p| p.kind.clone()),
            Some(super::super::prompt::PromptKind::Inspector(
                super::super::FieldKey::Prop("title".into())
            ))
        );
    }
}
