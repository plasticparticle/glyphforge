//! Symbols and libraries in the editor: turning a component into a
//! symbol, inserting instances, detaching an instance to edit it, pushing
//! the edit back to the master, and importing or exporting libraries.

use glyphforge_core::component::tree;
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::symbol::{self, Symbol};
use glyphforge_core::{Component, ObjectId, Value};

use super::prompt::{Prompt, PromptKind};
use super::{App, StatusKind};

/// Property a detached instance remembers its symbol in.
const FROM_SYMBOL: &str = "from_symbol";

impl App {
    /// Where `id` sits: parent and index on the active layer.
    fn slot_of(&self, id: &ObjectId) -> Option<(Option<ObjectId>, usize)> {
        let roots = self.active_layer()?.components()?;
        tree::locate(roots, id)
    }

    /// Replaces component `id` by an instance of `symbol` with the same id
    /// and placement.
    fn replace_with_instance(&self, id: &ObjectId, symbol: &ObjectId) -> Option<Vec<Operation>> {
        let c = self.doc().component(id)?;
        let (parent, index) = self.slot_of(id)?;
        let instance = Component::new(id.clone(), "instance")
            .with_prop("symbol", symbol.as_str())
            .with_layout(c.layout);
        Some(vec![
            Operation::DeleteComponent { target: id.clone() },
            Operation::CreateComponent {
                component: instance,
                parent,
                layer: Some(self.editor.layer.clone()),
                index: Some(index),
            },
        ])
    }

    /// The selected component becomes a symbol; it is replaced by an
    /// instance of it in place.
    pub fn make_symbol(&mut self) {
        let Some(id) = self.selection().cloned() else {
            return self.set_status(StatusKind::Info, "Select a component to make a symbol of");
        };
        let Some(c) = self.doc().component(&id).cloned() else {
            return;
        };
        if c.kind == "instance" {
            return self.set_status(StatusKind::Info, "This is already an instance of a symbol");
        }
        let ids = self.doc().ids();
        let symbol_id =
            ObjectId::slugify(&format!("{id}-symbol")).unique_among(|i| ids.contains(i));
        let name = c
            .prop_str("title")
            .or_else(|| c.prop_str("text"))
            .unwrap_or(id.as_str())
            .to_owned();
        let mut master = c;
        master.props.remove(FROM_SYMBOL);
        let mut ops = vec![Operation::SetSymbol {
            symbol: Symbol {
                id: symbol_id.clone(),
                name: name.clone(),
                description: String::new(),
                component: master,
            },
        }];
        let Some(replace) = self.replace_with_instance(&id, &symbol_id) else {
            return;
        };
        ops.extend(replace);
        if self.apply(Patch::new(ops), "Make symbol") {
            self.select(Some(id));
            self.set_status(
                StatusKind::Info,
                format!("Symbol {name:?} ({symbol_id}); insert copies with \"Insert Symbol\""),
            );
        }
    }

    pub fn open_insert_symbol(&mut self) {
        if self.doc().symbols.is_empty() {
            return self.set_status(
                StatusKind::Info,
                "No symbols yet: select a component and run \"Make Symbol\"",
            );
        }
        let candidates = self
            .doc()
            .symbols
            .iter()
            .map(|s| format!("{} · {}", s.id, s.name))
            .collect();
        self.prompt = Some(Prompt::new(
            PromptKind::InsertSymbol,
            "Insert symbol",
            candidates,
        ));
    }

    pub(super) fn submit_insert_symbol(&mut self, value: &str) {
        let id = value.split(" · ").next().unwrap_or("").trim();
        let Some(symbol) = self
            .doc()
            .symbols
            .iter()
            .find(|s| s.id.as_str() == id)
            .cloned()
        else {
            return self.set_status(StatusKind::Warning, format!("No symbol {id:?}"));
        };
        let ids = self.doc().ids();
        let instance_id = ObjectId::slugify(id).unique_among(|i| ids.contains(i));
        let size = |d: glyphforge_core::layout::Dimension, fallback: u16| match d {
            glyphforge_core::layout::Dimension::Fixed(n) => n,
            _ => fallback,
        };
        let cursor = self.editor.cursor;
        let layout = glyphforge_core::layout::Layout::absolute(
            cursor.x,
            cursor.y,
            size(symbol.component.layout.width, 20),
            size(symbol.component.layout.height, 4),
        );
        let instance = Component::new(instance_id.clone(), "instance")
            .with_prop("symbol", symbol.id.as_str())
            .with_layout(layout);
        if self.apply(
            Patch::single(Operation::CreateComponent {
                component: instance,
                parent: None,
                layer: Some(self.editor.layer.clone()),
                index: None,
            }),
            "Insert symbol",
        ) {
            self.select(Some(instance_id));
            self.set_status(StatusKind::Info, format!("Inserted {}", symbol.name));
        }
    }

    /// The selected instance becomes ordinary components (fresh ids) that
    /// remember their symbol, ready to edit and push back.
    pub fn detach_instance(&mut self) {
        let Some(id) = self.selection().cloned() else {
            return;
        };
        let Some(c) = self.doc().component(&id).cloned() else {
            return;
        };
        let Some(mut root) = symbol::expand(&c, &self.doc().symbols) else {
            return self.set_status(
                StatusKind::Info,
                "Select an instance of a symbol to detach it",
            );
        };
        let mut taken: Vec<ObjectId> = self.doc().ids().into_iter().filter(|i| *i != id).collect();
        for child in &mut root.children {
            super::selection::reid(child, &mut taken);
        }
        root.id = id.clone();
        root.layout = c.layout;
        if let Some(s) = c.prop_str("symbol") {
            root.props.insert(FROM_SYMBOL.to_owned(), Value::from(s));
        }
        let Some((parent, index)) = self.slot_of(&id) else {
            return;
        };
        let ops = vec![
            Operation::DeleteComponent { target: id.clone() },
            Operation::CreateComponent {
                component: root,
                parent,
                layer: Some(self.editor.layer.clone()),
                index: Some(index),
            },
        ];
        if self.apply(Patch::new(ops), "Detach instance") {
            self.select(Some(id));
            self.set_status(
                StatusKind::Info,
                "Detached: edit it, then \"Push To Symbol\" updates every instance",
            );
        }
    }

    /// The selected detached component replaces its symbol's master and
    /// becomes an instance again; every other instance follows.
    pub fn push_to_symbol(&mut self) {
        let Some(id) = self.selection().cloned() else {
            return;
        };
        let Some(c) = self.doc().component(&id).cloned() else {
            return;
        };
        let Some(symbol_id) = c.prop_str(FROM_SYMBOL).and_then(|s| ObjectId::new(s).ok()) else {
            return self.set_status(
                StatusKind::Info,
                "Select a detached instance to push its changes",
            );
        };
        let Some(old) = self
            .doc()
            .symbols
            .iter()
            .find(|s| s.id == symbol_id)
            .cloned()
        else {
            return self.set_status(
                StatusKind::Warning,
                format!("Symbol {symbol_id} no longer exists"),
            );
        };
        let mut master = c;
        master.props.remove(FROM_SYMBOL);
        master.id = old.component.id.clone();
        master.layout = old.component.layout;
        let mut ops = vec![Operation::SetSymbol {
            symbol: Symbol {
                component: master,
                ..old
            },
        }];
        let Some(replace) = self.replace_with_instance(&id, &symbol_id) else {
            return;
        };
        ops.extend(replace);
        if self.apply(Patch::new(ops), "Push to symbol") {
            self.select(Some(id));
            self.set_status(
                StatusKind::Info,
                format!("Updated symbol {symbol_id} and its instances"),
            );
        }
    }

    pub fn open_library_prompt(&mut self, import: bool) {
        let (kind, title) = if import {
            (
                PromptKind::ImportLibrary,
                "Import library (.glyph): symbols, gradients, palettes, data",
            )
        } else {
            (PromptKind::ExportLibrary, "Export library to (.glyph)")
        };
        self.prompt = Some(Prompt::new(kind, title, vec![]));
    }

    pub(super) fn submit_import_library(&mut self, path: &str) {
        let path = path.trim();
        let library = match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| glyphforge_core::project::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(doc) => doc,
            Err(e) => {
                return self.set_status(StatusKind::Error, format!("Cannot read {path}: {e}"));
            }
        };
        let patch = symbol::import_library(self.doc(), &library);
        let n = library.symbols.len();
        if self.apply(patch, "Import library") {
            self.set_status(
                StatusKind::Info,
                format!(
                    "Imported {n} symbol(s), {} gradient(s), {} palette(s)",
                    library.gradients.len(),
                    library.palettes.len()
                ),
            );
        }
    }

    pub(super) fn submit_export_library(&mut self, path: &str) {
        let path = std::path::Path::new(path.trim());
        let lib = symbol::export_library(self.doc());
        let result = glyphforge_core::project::to_json(&lib)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.set_status(
                StatusKind::Info,
                format!("Library written to {}", path.display()),
            ),
            Err(e) => self.set_status(StatusKind::Error, format!("Cannot write library: {e}")),
        }
    }

    /// Prompt for an export: `format path`.
    pub fn open_export(&mut self) {
        let stem = self
            .session
            .path()
            .and_then(|p| p.file_stem())
            .map_or_else(|| "design".to_owned(), |s| s.to_string_lossy().into_owned());
        let candidates = [
            ("text", "txt"),
            ("ansi", "ans"),
            ("ansi256", "ans"),
            ("ansi16", "ans"),
            ("handoff", "yaml"),
            ("ratatui", "rs"),
            ("textual", "py"),
            ("bubbletea", "go"),
            ("ink", "tsx"),
        ]
        .iter()
        .map(|(f, ext)| format!("{f} {stem}.{ext}"))
        .collect();
        self.prompt = Some(Prompt::new(
            PromptKind::Export,
            "Export the screen: format path",
            candidates,
        ));
    }

    pub(super) fn submit_export(&mut self, value: &str) {
        let mut parts = value.split_whitespace();
        let (Some(format), Some(path)) = (parts.next(), parts.next()) else {
            return self.set_status(StatusKind::Warning, "Expected: format path");
        };
        let screen = self.editor.screen.clone();
        let result = crate::headless::export_text(&self.session, &screen, format)
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.set_status(StatusKind::Info, format!("Exported {format} to {path}")),
            Err(e) => self.set_status(StatusKind::Error, format!("Export failed: {e}")),
        }
    }

    /// Prompt for a template to start a new document from.
    pub fn open_template_prompt(&mut self) {
        let title = if self.dirty() {
            "New from template (unsaved changes will be lost)"
        } else {
            "New from template"
        };
        let candidates = crate::templates::TEMPLATES
            .iter()
            .map(|(name, description, _)| format!("{name} · {description}"))
            .collect();
        self.prompt = Some(Prompt::new(PromptKind::Template, title, candidates));
    }

    pub(super) fn submit_template(&mut self, value: &str) {
        let name = value.split(" · ").next().unwrap_or("").trim();
        let Some(json) = crate::templates::template(name) else {
            return self.set_status(StatusKind::Warning, format!("No template {name:?}"));
        };
        match glyphforge_core::project::from_json(json) {
            Ok(doc) => {
                self.replace_session(glyphforge_core::api::Session::new(doc));
                self.set_status(
                    StatusKind::Info,
                    format!("New document from {name}; Save As (Ctrl+Shift+S) to keep it"),
                );
            }
            Err(e) => self.report_error(&e),
        }
    }

    /// Prompt for a glyph palette: `Name: glyph glyph glyph`.
    pub fn open_palette_prompt(&mut self) {
        let candidates = self
            .doc()
            .palettes
            .iter()
            .map(|p| format!("{}: {}", p.name, p.glyphs))
            .collect();
        self.prompt = Some(Prompt::new(
            PromptKind::GlyphPalette,
            "Glyph palette: Name: glyphs separated by spaces (no glyphs removes it)",
            candidates,
        ));
    }

    pub(super) fn submit_palette(&mut self, value: &str) {
        let Some((name, glyphs)) = value.split_once(':') else {
            return self.set_status(StatusKind::Warning, "Expected Name: glyphs");
        };
        let glyphs = glyphs.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.apply(
            Patch::single(Operation::SetPalette {
                name: name.trim().to_owned(),
                glyphs,
            }),
            "Set glyph palette",
        ) {
            self.set_status(StatusKind::Info, format!("Palette {} saved", name.trim()));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::{ObjectId, Size};

    #[test]
    fn make_insert_detach_push_symbols() {
        let mut app = test_app(Size::new(40, 12));
        app.dispatch(Action::LayerNext);
        let layer = app.editor.layer.clone();
        let id = app
            .session
            .add_component("panel", Some("card"), 1, 1, None, Some(layer))
            .unwrap();
        app.mark_edited();
        app.select(Some(id.clone()));
        app.dispatch(Action::MakeSymbol);
        assert_eq!(app.doc().symbols.len(), 1);
        assert_eq!(app.doc().component(&id).unwrap().kind, "instance");
        let before = app.session.render_text(&app.editor.screen, None).unwrap();
        assert!(before.contains("Panel"), "{before}");
        // A second instance.
        app.dispatch(Action::CursorTo(glyphforge_core::Position::new(20, 5)));
        app.dispatch(Action::InsertSymbol);
        app.dispatch(Action::PromptSubmit);
        assert_eq!(
            app.doc()
                .components()
                .filter(|c| c.kind == "instance")
                .count(),
            2
        );
        // Detach the first, retitle it, push: both instances change.
        app.select(Some(id.clone()));
        app.dispatch(Action::DetachInstance);
        assert_eq!(app.doc().component(&id).unwrap().kind, "panel");
        app.session
            .update_component(&id, vec![("title".into(), "Renamed".into())])
            .unwrap();
        app.mark_edited();
        app.dispatch(Action::PushToSymbol);
        let after = app.session.render_text(&app.editor.screen, None).unwrap();
        assert_eq!(after.matches("Renamed").count(), 2, "{after}");
        assert!(app.doc().duplicate_ids().is_empty());
        // Export and import a library into a fresh document.
        let dir = std::env::temp_dir().join(format!("glyphforge-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = dir.join("lib.glyph");
        app.submit_export_library(&lib.display().to_string());
        let mut other = test_app(Size::new(20, 5));
        other.submit_import_library(&lib.display().to_string());
        assert_eq!(other.doc().symbols.len(), 1);
        assert_eq!(
            other.doc().symbols[0].id,
            ObjectId::new("card-symbol").unwrap()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
