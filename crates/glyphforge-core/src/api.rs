//! The application API: every operation the TUI, the CLI and a future MCP
//! server share. Nothing here knows about terminals or user interfaces.

use std::path::{Path, PathBuf};

use crate::component::Component;
use crate::component::tree;
use crate::document::Rect;
use crate::document::{Canvas, CanvasError, Document, Size};
use crate::geometry::{self, SnapTargets};
use crate::history::{History, HistoryError, Origin};
use crate::id::ObjectId;
use crate::layout::{Layout, LayoutResult, Placement};
use crate::patch::{Operation, Patch};
use crate::project::{self, ProjectError};
use crate::render::{self, Registry, RenderContext};
use crate::validate::{self, Diagnostic};
use crate::value::Value;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error("no screen with id {0}")]
    NoSuchScreen(ObjectId),
    #[error("no component with id {0}")]
    NoSuchComponent(ObjectId),
    #[error("the document has no file path yet")]
    NoPath,
    #[error("{0} and {1} are on different layers")]
    DifferentLayers(ObjectId, ObjectId),
}

/// What a geometry command changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeometryOutcome {
    /// Components actually moved or resized.
    pub changed: usize,
    /// Components left alone because their parent lays them out.
    pub skipped: Vec<ObjectId>,
}

/// A document with its history, file binding and renderer registry.
#[derive(Debug)]
pub struct Session {
    doc: Document,
    history: History,
    registry: Registry,
    path: Option<PathBuf>,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            history: History::default(),
            registry: Registry::builtin(),
            path: None,
        }
    }

    #[must_use]
    pub fn with_path(mut self, path: Option<PathBuf>) -> Self {
        self.path = path;
        self
    }

    pub fn open(path: &Path) -> Result<Self, ApiError> {
        let doc = project::load(path)?;
        Ok(Self::new(doc).with_path(Some(path.to_path_buf())))
    }

    // --- inspection -----------------------------------------------------

    pub const fn document(&self) -> &Document {
        &self.doc
    }

    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    pub const fn history(&self) -> &History {
        &self.history
    }

    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    pub fn component(&self, id: &ObjectId) -> Result<&Component, ApiError> {
        self.doc
            .component(id)
            .ok_or_else(|| ApiError::NoSuchComponent(id.clone()))
    }

    /// Components matching an optional kind, in document order.
    pub fn query_components(&self, kind: Option<&str>) -> Vec<&Component> {
        self.doc
            .components()
            .filter(|c| kind.is_none_or(|k| c.kind == k))
            .collect()
    }

    // --- mutation --------------------------------------------------------

    /// Applies a patch as one undoable transaction.
    pub fn apply_patch(
        &mut self,
        patch: Patch,
        label: impl Into<String>,
        origin: Origin,
    ) -> Result<(), ApiError> {
        self.history.apply(&mut self.doc, patch, label, origin)?;
        Ok(())
    }

    fn single(&mut self, op: Operation, label: &str) -> Result<(), ApiError> {
        self.apply_patch(Patch::single(op), label, Origin::User)
    }

    pub fn create_component(
        &mut self,
        component: Component,
        parent: Option<ObjectId>,
        layer: Option<ObjectId>,
        index: Option<usize>,
    ) -> Result<(), ApiError> {
        self.single(
            Operation::CreateComponent {
                component,
                parent,
                layer,
                index,
            },
            "Create component",
        )
    }

    pub fn update_component(
        &mut self,
        id: &ObjectId,
        props: Vec<(String, Value)>,
    ) -> Result<(), ApiError> {
        let ops = props
            .into_iter()
            .map(|(property, value)| Operation::SetProperty {
                target: id.clone(),
                property,
                value,
            })
            .collect();
        self.apply_patch(Patch::new(ops), "Update component", Origin::User)
    }

    pub fn move_component(&mut self, id: &ObjectId, x: u16, y: u16) -> Result<(), ApiError> {
        self.single(
            Operation::Move {
                target: id.clone(),
                x,
                y,
            },
            "Move component",
        )
    }

    pub fn resize_component(
        &mut self,
        id: &ObjectId,
        width: u16,
        height: u16,
    ) -> Result<(), ApiError> {
        self.single(
            Operation::Resize {
                target: id.clone(),
                width,
                height,
            },
            "Resize component",
        )
    }

    pub fn delete_component(&mut self, id: &ObjectId) -> Result<(), ApiError> {
        self.single(
            Operation::DeleteComponent { target: id.clone() },
            "Delete component",
        )
    }

    /// Opens a transaction for continuous edits (strokes, typing).
    pub fn begin(&mut self, label: impl Into<String>) -> Result<(), ApiError> {
        self.history.begin(label, Origin::User)?;
        Ok(())
    }

    pub fn record(&mut self, op: Operation) -> Result<(), ApiError> {
        self.history.record(&mut self.doc, op)?;
        Ok(())
    }

    /// Commits the open transaction and returns its label, if it changed
    /// anything.
    pub fn end(&mut self) -> Option<String> {
        self.history.end().map(|t| t.label.clone())
    }

    pub fn has_open_transaction(&self) -> bool {
        self.history.has_open_transaction()
    }

    pub fn undo(&mut self) -> Result<Option<String>, ApiError> {
        Ok(self.history.undo(&mut self.doc)?)
    }

    pub fn redo(&mut self) -> Result<Option<String>, ApiError> {
        Ok(self.history.redo(&mut self.doc)?)
    }

    // --- rendering and validation ---------------------------------------

    /// Renders a screen, optionally at another size for responsive previews.
    pub fn render(&self, screen: &ObjectId, size: Option<Size>) -> Result<Canvas, ApiError> {
        let s = self
            .doc
            .screen(screen)
            .ok_or_else(|| ApiError::NoSuchScreen(screen.clone()))?;
        Ok(render::render_screen(
            s,
            &self.doc.theme,
            &self.registry,
            size,
        )?)
    }

    /// Solved layout of one interface layer of `screen`, at the screen's
    /// size or `size`. An artwork layer yields an empty result.
    pub fn layout(
        &self,
        screen: &ObjectId,
        layer: &ObjectId,
        size: Option<Size>,
    ) -> Result<LayoutResult, ApiError> {
        let s = self
            .doc
            .screen(screen)
            .ok_or_else(|| ApiError::NoSuchScreen(screen.clone()))?;
        let Some(roots) = s.layer(layer).and_then(crate::document::Layer::components) else {
            return Ok(LayoutResult::default());
        };
        let ctx = RenderContext {
            theme: &self.doc.theme,
            screen: Some(s),
        };
        Ok(render::solve_components(
            roots,
            size.unwrap_or(s.size()),
            &ctx,
            &self.registry,
        ))
    }

    /// Creates a component of `kind` with the renderer's default size and
    /// properties, placed absolutely at (`x`, `y`) under `parent` or as a
    /// root of `layer`. The id is derived from `name` and made unique.
    pub fn add_component(
        &mut self,
        kind: &str,
        name: Option<&str>,
        x: u16,
        y: u16,
        parent: Option<ObjectId>,
        layer: Option<ObjectId>,
    ) -> Result<ObjectId, ApiError> {
        let renderer = self.registry.renderer(kind);
        let base = ObjectId::slugify(name.unwrap_or(kind));
        let id = base.unique_among(|c| self.doc.has_id(c));
        let mut layout = match renderer.default_size() {
            Some(size) => Layout::absolute(x, y, size.width, size.height),
            None => Layout {
                placement: crate::layout::Placement::Absolute { x, y },
                width: crate::layout::Dimension::Content,
                height: crate::layout::Dimension::Content,
                ..Layout::default()
            },
        };
        if kind == "panel" || kind == "group" {
            layout.container.direction = crate::layout::Direction::Vertical;
        }
        let mut component = Component::new(id.clone(), kind).with_layout(layout);
        for (k, v) in renderer.default_props() {
            component.props.insert(k.to_owned(), v);
        }
        self.single(
            Operation::CreateComponent {
                component,
                parent,
                layer,
                index: None,
            },
            &format!("Add {kind}"),
        )?;
        Ok(id)
    }

    // --- geometry --------------------------------------------------------

    /// Builds geometry items for `ids`, which must all live on one layer.
    /// The result is in document order, so the outcome never depends on
    /// the order the user clicked things in.
    fn geometry_items(&self, ids: &[ObjectId]) -> Result<Vec<geometry::Item>, ApiError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let first = &ids[0];
        let layer_id = self
            .doc
            .layer_of_component(first)
            .map(|l| l.id.clone())
            .ok_or_else(|| ApiError::NoSuchComponent(first.clone()))?;
        for id in &ids[1..] {
            let other = self
                .doc
                .layer_of_component(id)
                .map(|l| l.id.clone())
                .ok_or_else(|| ApiError::NoSuchComponent(id.clone()))?;
            if other != layer_id {
                return Err(ApiError::DifferentLayers(first.clone(), id.clone()));
            }
        }
        let screen = self
            .doc
            .screen_of_layer(&layer_id)
            .ok_or_else(|| ApiError::NoSuchScreen(layer_id.clone()))?;
        let roots = self
            .doc
            .layer(&layer_id)
            .and_then(crate::document::Layer::components)
            .unwrap_or_default();
        let ctx = RenderContext {
            theme: &self.doc.theme,
            screen: Some(screen),
        };
        let solved = render::solve_components(roots, screen.size(), &ctx, &self.registry);
        let order: Vec<&ObjectId> = tree::iter(roots).map(|c| &c.id).collect();
        let rank = |id: &ObjectId| order.iter().position(|o| *o == id).unwrap_or(usize::MAX);

        let screen_rect = Rect::from_size(screen.size());
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            let component =
                tree::find(roots, id).ok_or_else(|| ApiError::NoSuchComponent(id.clone()))?;
            let rect = solved
                .rect(id)
                .ok_or_else(|| ApiError::NoSuchComponent(id.clone()))?;
            let parent_inner = tree::locate(roots, id)
                .and_then(|(parent, _)| parent)
                .and_then(|p| solved.inner(&p))
                .unwrap_or(screen_rect);
            items.push(geometry::Item {
                id: id.clone(),
                rect,
                parent_inner,
                movable: matches!(component.layout.placement, Placement::Absolute { .. }),
            });
        }
        items.sort_by_key(|i| rank(&i.id));
        Ok(items)
    }

    /// Applies a geometry plan as one undoable transaction.
    fn apply_plan(
        &mut self,
        plan: geometry::Plan,
        label: &str,
    ) -> Result<GeometryOutcome, ApiError> {
        let changed = plan.operations.len();
        if changed > 0 {
            self.apply_patch(Patch::new(plan.operations), label, Origin::User)?;
        }
        Ok(GeometryOutcome {
            changed,
            skipped: plan.skipped,
        })
    }

    pub fn align_components(
        &mut self,
        ids: &[ObjectId],
        mode: geometry::Align,
    ) -> Result<GeometryOutcome, ApiError> {
        let items = self.geometry_items(ids)?;
        let plan = geometry::align(&items, mode);
        self.apply_plan(plan, &format!("Align {}", mode.label()))
    }

    pub fn distribute_components(
        &mut self,
        ids: &[ObjectId],
        axis: geometry::Axis,
    ) -> Result<GeometryOutcome, ApiError> {
        let items = self.geometry_items(ids)?;
        let plan = geometry::distribute(&items, axis);
        self.apply_plan(plan, &format!("Distribute {}", axis.name()))
    }

    pub fn equalize_components(
        &mut self,
        ids: &[ObjectId],
        axis: geometry::Axis,
    ) -> Result<GeometryOutcome, ApiError> {
        let items = self.geometry_items(ids)?;
        let plan = geometry::equalize(&items, axis);
        let what = if axis == geometry::Axis::Horizontal {
            "widths"
        } else {
            "heights"
        };
        self.apply_plan(plan, &format!("Equalize {what}"))
    }

    /// Moves components under `parent` (or to the layer's roots) in one
    /// transaction.
    pub fn reparent_components(
        &mut self,
        ids: &[ObjectId],
        parent: Option<&ObjectId>,
    ) -> Result<(), ApiError> {
        let ops = ids
            .iter()
            .map(|id| Operation::Reparent {
                target: id.clone(),
                parent: parent.cloned(),
                index: None,
            })
            .collect();
        self.apply_patch(Patch::new(ops), "Reparent", Origin::User)
    }

    /// Snap lines for a drag on `layer`: the screen edges plus every
    /// component rectangle except the dragged ones and their children.
    pub fn snap_targets(
        &self,
        screen: &ObjectId,
        layer: &ObjectId,
        exclude: &[ObjectId],
    ) -> Result<SnapTargets, ApiError> {
        let s = self
            .doc
            .screen(screen)
            .ok_or_else(|| ApiError::NoSuchScreen(screen.clone()))?;
        let mut targets = SnapTargets::default();
        targets.add_rect(Rect::from_size(s.size()));
        let Some(roots) = s.layer(layer).and_then(crate::document::Layer::components) else {
            targets.normalize();
            return Ok(targets);
        };
        let mut hidden: Vec<ObjectId> = Vec::new();
        for id in exclude {
            if let Some(c) = tree::find(roots, id) {
                hidden.extend(c.iter().map(|d| d.id.clone()));
            }
        }
        let ctx = RenderContext {
            theme: &self.doc.theme,
            screen: Some(s),
        };
        let solved = render::solve_components(roots, s.size(), &ctx, &self.registry);
        for c in tree::iter(roots) {
            if hidden.contains(&c.id) {
                continue;
            }
            if let Some(rect) = solved.rect(&c.id) {
                targets.add_rect(rect);
            }
        }
        targets.normalize();
        Ok(targets)
    }

    /// Plain-text preview of a screen.
    pub fn render_text(&self, screen: &ObjectId, size: Option<Size>) -> Result<String, ApiError> {
        let canvas = self.render(screen, size)?;
        Ok(render::to_text_lines(&canvas).join("\n"))
    }

    pub fn validate(&self) -> Vec<Diagnostic> {
        validate::validate(&self.doc, &self.registry)
    }

    pub fn validate_at(&self, screen: &ObjectId, size: Size) -> Result<Vec<Diagnostic>, ApiError> {
        let s = self
            .doc
            .screen(screen)
            .ok_or_else(|| ApiError::NoSuchScreen(screen.clone()))?;
        Ok(validate::validate_screen(
            s,
            &self.doc.theme,
            &self.registry,
            size,
        ))
    }

    // --- persistence -----------------------------------------------------

    pub fn save(&mut self) -> Result<PathBuf, ApiError> {
        let path = self.path.clone().ok_or(ApiError::NoPath)?;
        self.save_as(&path)?;
        Ok(path)
    }

    pub fn save_as(&mut self, path: &Path) -> Result<(), ApiError> {
        project::save(&self.doc, path)?;
        self.path = Some(path.to_path_buf());
        self.history.mark_saved();
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, ApiError> {
        Ok(project::to_json(&self.doc)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;
    use crate::layout::Layout;

    #[test]
    fn agent_loop_inspect_modify_render() {
        let mut s = Session::new(Document::new(Size::new(30, 5)).unwrap());
        let panel = Component::new(id!("metrics"), "panel")
            .with_prop("title", "Metrics")
            .with_layout(Layout::absolute(0, 0, 12, 3));
        s.create_component(panel, None, None, None).unwrap();
        let patch: Patch = serde_json::from_str(
            r#"{"operations":[{"op":"resize","target":"metrics","width":16,"height":3},
                              {"op":"set_property","target":"metrics","property":"border","value":"double"}]}"#,
        )
        .unwrap();
        s.apply_patch(
            patch,
            "Agent: widen metrics",
            Origin::Agent {
                name: "planner".into(),
                description: "widen".into(),
            },
        )
        .unwrap();
        let text = s.render_text(&id!("main"), None).unwrap();
        assert!(text.starts_with("╔ Metrics ═════╗"), "{text}");
        assert_eq!(s.query_components(Some("panel")).len(), 1);
        assert!(s.validate().is_empty());
        s.undo().unwrap();
        assert!(
            s.render_text(&id!("main"), None)
                .unwrap()
                .starts_with("╭ Metrics ─")
        );
        assert!(s.is_dirty());
        assert!(matches!(s.save(), Err(ApiError::NoPath)));
    }

    #[test]
    fn add_component_uses_defaults_and_unique_ids() {
        let mut s = Session::new(Document::new(Size::new(40, 10)).unwrap());
        let a = s.add_component("panel", None, 1, 1, None, None).unwrap();
        let b = s.add_component("panel", None, 2, 2, None, None).unwrap();
        let c = s
            .add_component("label", Some("CPU Load"), 0, 0, Some(a.clone()), None)
            .unwrap();
        assert_eq!(a.as_str(), "panel");
        assert_eq!(b.as_str(), "panel-2");
        assert_eq!(c.as_str(), "cpu-load");
        assert_eq!(s.component(&a).unwrap().prop_str("title"), Some("Panel"));
        let layout = s.layout(&id!("main"), &id!("main-ui"), None).unwrap();
        assert_eq!(layout.rect(&a).unwrap().width, 20);
        assert_eq!(layout.rect(&c).unwrap().width, 5, "label sizes to its text");
        assert_eq!(s.history().undo_len(), 3);
    }

    #[test]
    fn align_and_distribute_are_single_transactions() {
        let mut s = Session::new(Document::new(Size::new(60, 20)).unwrap());
        for (name, x) in [("a", 0u16), ("b", 10), ("c", 40)] {
            let id = s
                .add_component("panel", Some(name), x, 0, None, None)
                .unwrap();
            s.resize_component(&id, 6, 3).unwrap();
        }
        let ids = [id!("a"), id!("b"), id!("c")];
        let before = s.history().undo_len();
        let outcome = s.align_components(&ids, geometry::Align::Top).unwrap();
        assert_eq!(outcome.changed, 0, "they already share a top edge");
        assert_eq!(
            s.history().undo_len(),
            before,
            "an empty plan is not a transaction"
        );

        let outcome = s
            .distribute_components(&ids, geometry::Axis::Horizontal)
            .unwrap();
        assert_eq!(outcome.changed, 1);
        assert_eq!(s.history().undo_len(), before + 1);
        let layout = s.layout(&id!("main"), &id!("main-ui"), None).unwrap();
        assert_eq!(layout.rect(&id!("b")).unwrap().x, 20);

        let outcome = s
            .equalize_components(&ids, geometry::Axis::Horizontal)
            .unwrap();
        assert_eq!(outcome.changed, 0, "all three are already 6 wide");
        s.resize_component(&id!("a"), 12, 3).unwrap();
        let outcome = s
            .equalize_components(&ids, geometry::Axis::Horizontal)
            .unwrap();
        assert_eq!(outcome.changed, 2);
        // One undo puts both back.
        s.undo().unwrap();
        let layout = s.layout(&id!("main"), &id!("main-ui"), None).unwrap();
        assert_eq!(layout.rect(&id!("b")).unwrap().width, 6);
    }

    #[test]
    fn geometry_reports_components_their_parent_lays_out() {
        let mut s = Session::new(Document::new(Size::new(40, 10)).unwrap());
        let parent = s
            .add_component("panel", Some("box"), 0, 0, None, None)
            .unwrap();
        let flow = Component::new(id!("child"), "label").with_prop("text", "x");
        s.create_component(flow, Some(parent), None, None).unwrap();
        let free = s
            .add_component("panel", Some("free"), 20, 5, None, None)
            .unwrap();
        let outcome = s
            .align_components(&[id!("child"), free.clone()], geometry::Align::Left)
            .unwrap();
        assert_eq!(outcome.skipped, vec![id!("child")]);
        // The flow child stays put but still defines the bounding box, so
        // the free panel lines up with it. That is the point: you can
        // align a floating panel to a laid-out label.
        assert_eq!(outcome.changed, 1);
        let layout = s.layout(&id!("main"), &id!("main-ui"), None).unwrap();
        assert_eq!(
            layout.rect(&free).unwrap().x,
            layout.rect(&id!("child")).unwrap().x
        );
    }

    #[test]
    fn geometry_refuses_components_from_different_layers() {
        let mut s = Session::new(Document::new(Size::new(40, 10)).unwrap());
        let a = s
            .add_component("panel", Some("a"), 0, 0, None, None)
            .unwrap();
        let layer = crate::Layer::interface(id!("second"), "Second");
        s.apply_patch(
            Patch::single(Operation::CreateLayer {
                layer,
                screen: None,
                index: None,
            }),
            "add layer",
            Origin::User,
        )
        .unwrap();
        let b = s
            .add_component("panel", Some("b"), 0, 0, None, Some(id!("second")))
            .unwrap();
        assert!(matches!(
            s.align_components(&[a, b], geometry::Align::Left),
            Err(ApiError::DifferentLayers(..))
        ));
    }

    #[test]
    fn reparent_and_snap_targets() {
        let mut s = Session::new(Document::new(Size::new(40, 10)).unwrap());
        let host = s
            .add_component("panel", Some("host"), 0, 0, None, None)
            .unwrap();
        let guest = s
            .add_component("panel", Some("guest"), 20, 4, None, None)
            .unwrap();
        s.reparent_components(std::slice::from_ref(&guest), Some(&host))
            .unwrap();
        assert_eq!(s.component(&host).unwrap().children[0].id, guest);

        // Snap targets exclude the dragged subtree but include the screen.
        let targets = s
            .snap_targets(&id!("main"), &id!("main-ui"), std::slice::from_ref(&host))
            .unwrap();
        assert_eq!(targets.vertical, vec![0, 20, 40]);
        let all = s.snap_targets(&id!("main"), &id!("main-ui"), &[]).unwrap();
        assert!(
            all.vertical.len() > 3,
            "sibling edges are included: {:?}",
            all.vertical
        );
        s.undo().unwrap();
        assert!(s.component(&host).unwrap().children.is_empty());
    }

    #[test]
    fn unknown_ids_are_errors() {
        let mut s = Session::new(Document::default());
        assert!(matches!(
            s.component(&id!("nope")),
            Err(ApiError::NoSuchComponent(_))
        ));
        assert!(matches!(
            s.render(&id!("nope"), None),
            Err(ApiError::NoSuchScreen(_))
        ));
        assert!(s.move_component(&id!("nope"), 1, 1).is_err());
    }
}
