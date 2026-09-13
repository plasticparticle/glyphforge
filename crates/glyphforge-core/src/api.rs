//! The application API: every operation the TUI, the CLI and a future MCP
//! server share. Nothing here knows about terminals or user interfaces.

use std::path::{Path, PathBuf};

use crate::component::Component;
use crate::document::{Canvas, CanvasError, Document, Size};
use crate::history::{History, HistoryError, Origin};
use crate::id::ObjectId;
use crate::layout::{Layout, LayoutResult};
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
