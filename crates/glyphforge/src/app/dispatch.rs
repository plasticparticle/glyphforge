//! `App::dispatch`: the only place application behaviour lives. Every
//! document mutation goes through the session's patch/history mechanism.

use glyphforge_core::api::Session;
use glyphforge_core::history::Origin;
use glyphforge_core::patch::{CellWrite, Operation, Patch};
use glyphforge_core::{Cell, Document, Grapheme, Layer, Position, Size};

use super::{App, EditorMode, Focus, StatusKind, UiThemeOrigin, delta};
use crate::actions::{Action, Direction, descriptor_of};

impl App {
    /// Applies an action to the application state. Recoverable errors are
    /// reported in the status bar.
    pub fn dispatch(&mut self, action: Action) {
        tracing::trace!(?action, "dispatch");
        if !matches!(action, Action::Quit) {
            self.quit_armed = false;
        }
        if !matches!(action, Action::NewDocument) {
            self.new_armed = false;
        }
        // Continuous edits form one transaction; anything else closes it.
        // A mouse drag keeps its own transaction open until the button is released.
        if self.drag.is_none()
            && !matches!(
                action,
                Action::InsertText(_) | Action::Backspace | Action::DeleteForward
            )
        {
            self.commit_edits();
        }
        match action {
            Action::SelectNext => self.select_step(true),
            Action::SelectPrev => self.select_step(false),
            Action::SelectById => self.open_select_by_id(),
            Action::SelectAt(pos) => {
                self.ui.focus = Focus::Canvas;
                self.set_cursor(pos, None);
                self.select_at(pos);
            }
            Action::ExtendSelectAt(pos) => {
                self.ui.focus = Focus::Canvas;
                self.set_cursor(pos, None);
                self.extend_at(pos);
            }
            Action::ExtendSelectNext => self.extend_step(true),
            Action::ExtendSelectPrev => self.extend_step(false),
            Action::SelectAll => self.select_all(),
            Action::AlignSelection(mode) => self.align_selection(mode),
            Action::DistributeSelection(axis) => self.distribute_selection(axis),
            Action::EqualizeSelection(axis) => self.equalize_selection(axis),
            Action::ReparentSelection => self.reparent_selection(),
            Action::ToggleSnap => self.toggle_snap(),
            Action::MoveSelection(dir) => {
                if !self.move_selection(dir) {
                    self.move_cursor(dir);
                }
            }
            Action::ResizeSelection(dir) => {
                if !self.resize_selection(dir) {
                    self.set_status(StatusKind::Info, "Select a component to resize it");
                }
            }
            Action::AddComponent => self.open_add_component(),
            Action::EditProperty => self.open_edit_property(),
            Action::DeleteSelection => self.delete_selection(),
            Action::SaveDocumentAs => self.open_save_as(),
            Action::PromptInput(c) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.insert(c);
                }
            }
            Action::PromptBackspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.backspace();
                }
            }
            Action::PromptNext => {
                if let Some(p) = self.prompt.as_mut() {
                    p.next();
                }
            }
            Action::PromptPrev => {
                if let Some(p) = self.prompt.as_mut() {
                    p.prev();
                }
            }
            Action::PromptComplete => {
                if let Some(p) = self.prompt.as_mut() {
                    p.complete();
                }
            }
            Action::PromptSubmit => self.submit_prompt(),
            Action::PromptCancel => self.prompt = None,
            Action::Quit => self.quit(),
            Action::ToggleHelp => self.ui.help_open = !self.ui.help_open,
            Action::Cancel => self.cancel(),
            Action::ReloadTheme => {
                self.load_ui_theme();
                let text = format!(
                    "UI theme: {} ({})",
                    self.ui_theme.name,
                    self.ui_theme_origin.name()
                );
                self.set_status(StatusKind::Info, text);
            }
            Action::UseOmarchyTheme => self.use_omarchy_theme(),
            Action::ToggleLeftPanel => {
                self.ui.show_left_panel = !self.ui.show_left_panel;
                self.refocus_if_hidden();
            }
            Action::ToggleRightPanel => {
                self.ui.show_right_panel = !self.ui.show_right_panel;
                self.refocus_if_hidden();
            }
            Action::TogglePanels => {
                let any = self.ui.show_left_panel || self.ui.show_right_panel;
                self.ui.show_left_panel = !any;
                self.ui.show_right_panel = !any;
                self.refocus_if_hidden();
            }
            Action::FocusNext => self.cycle_focus(true),
            Action::FocusPrev => self.cycle_focus(false),
            Action::ScrollView(dir) => {
                let (dx, dy) = delta(dir);
                let size = self.doc_size();
                self.editor.viewport.scroll_by(dx, dy, size);
            }
            Action::LayerNext => self.cycle_layer(true),
            Action::LayerPrev => self.cycle_layer(false),
            Action::CursorMove(dir) => self.move_cursor(dir),
            Action::CursorLineStart => {
                self.set_cursor(Position::new(0, self.editor.cursor.y), None);
            }
            Action::CursorLineEnd => {
                let x = self.doc_size().width.saturating_sub(1);
                self.set_cursor(Position::new(x, self.editor.cursor.y), None);
            }
            Action::CursorPageUp => {
                let page = self.editor.viewport.size.height.max(1);
                let y = self.editor.cursor.y.saturating_sub(page);
                self.set_cursor(Position::new(self.editor.cursor.x, y), None);
            }
            Action::CursorPageDown => {
                let page = self.editor.viewport.size.height.max(1);
                let y = self.editor.cursor.y.saturating_add(page);
                self.set_cursor(Position::new(self.editor.cursor.x, y), None);
            }
            Action::CursorTo(pos) => {
                self.ui.focus = Focus::Canvas;
                self.set_cursor(pos, None);
            }
            Action::InsertText(text) => self.insert_text(&text),
            Action::NewLine => {
                let y = self.editor.cursor.y.saturating_add(1);
                let x = self.editor.line_start_x;
                self.set_cursor(Position::new(x, y), Some(x));
            }
            Action::Backspace => self.backspace(),
            Action::DeleteForward => self.delete_forward(),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::NewDocument => self.new_document(),
            Action::SaveDocument => self.save(),
            Action::OpenDocument
            | Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::CommandPalette => {
                let title = descriptor_of(&action).map_or("This action", |d| d.title);
                self.set_status(
                    StatusKind::Warning,
                    format!("{title} is not implemented yet"),
                );
            }
        }
    }

    fn quit(&mut self) {
        if self.dirty() && !self.quit_armed {
            self.quit_armed = true;
            self.set_status(
                StatusKind::Warning,
                "Unsaved changes will be lost. Press the quit key again to quit anyway.",
            );
            return;
        }
        self.should_quit = true;
    }

    fn new_document(&mut self) {
        if self.dirty() && !self.new_armed {
            self.new_armed = true;
            self.set_status(
                StatusKind::Warning,
                "Unsaved changes will be lost. Press the new-document key again to discard them.",
            );
            return;
        }
        let size = Size::new(
            self.config.canvas.default_width,
            self.config.canvas.default_height,
        );
        match Document::new(size) {
            Ok(doc) => {
                self.replace_session(Session::new(doc));
                self.set_status(
                    StatusKind::Info,
                    format!("New {}×{} document", size.width, size.height),
                );
            }
            Err(e) => self.report_error(&e),
        }
    }

    fn save(&mut self) {
        match self.session.save() {
            Ok(path) => self.set_status(StatusKind::Info, format!("Saved {}", path.display())),
            Err(glyphforge_core::api::ApiError::NoPath) => self.set_status(
                StatusKind::Warning,
                "No file path yet: start Glyphforge with a file name (glyphforge design.glyph). Save As arrives with the file dialogs.",
            ),
            Err(e) => self.report_error(&e),
        }
    }

    fn undo(&mut self) {
        match self.session.undo() {
            Ok(Some(label)) => {
                self.mark_edited();
                self.clamp_cursor();
                self.set_status(StatusKind::Info, format!("Undo: {label}"));
            }
            Ok(None) => self.set_status(StatusKind::Info, "Nothing to undo"),
            Err(e) => self.report_error(&e),
        }
    }

    fn redo(&mut self) {
        match self.session.redo() {
            Ok(Some(label)) => {
                self.mark_edited();
                self.clamp_cursor();
                self.set_status(StatusKind::Info, format!("Redo: {label}"));
            }
            Ok(None) => self.set_status(StatusKind::Info, "Nothing to redo"),
            Err(e) => self.report_error(&e),
        }
    }

    fn use_omarchy_theme(&mut self) {
        if self.ui_theme_origin != UiThemeOrigin::Omarchy {
            self.set_status(
                StatusKind::Warning,
                "No Omarchy theme is loaded (not running under Omarchy, or theme.source = builtin)",
            );
            return;
        }
        let theme = self.ui_theme.clone();
        let name = theme.name.clone();
        match self.session.apply_patch(
            Patch::single(Operation::SetTheme { theme }),
            "Use Omarchy theme",
            Origin::User,
        ) {
            Ok(()) => {
                self.mark_edited();
                self.set_status(StatusKind::Info, format!("Document theme set to {name}"));
            }
            Err(e) => self.report_error(&e),
        }
    }

    fn cancel(&mut self) {
        if self.prompt.is_some() {
            self.prompt = None;
        } else if self.ui.help_open {
            self.ui.help_open = false;
        } else if !self.editor.selection.is_empty() {
            self.editor.selection.clear();
        } else if self.ui.vim_navigation && self.editor.mode == EditorMode::Insert {
            self.editor.mode = EditorMode::Normal;
        } else {
            self.status = None;
        }
    }

    /// Enters insert mode (Vim navigation only).
    pub fn enter_insert_mode(&mut self) {
        self.editor.mode = EditorMode::Insert;
    }

    /// Focus moves to the canvas when the focused panel gets hidden.
    fn refocus_if_hidden(&mut self) {
        let hidden = match self.ui.focus {
            Focus::LeftPanel => !self.ui.show_left_panel,
            Focus::RightPanel => !self.ui.show_right_panel,
            Focus::Canvas => false,
        };
        if hidden {
            self.ui.focus = Focus::Canvas;
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let mut regions = vec![Focus::Canvas];
        if self.ui.show_left_panel {
            regions.insert(0, Focus::LeftPanel);
        }
        if self.ui.show_right_panel {
            regions.push(Focus::RightPanel);
        }
        let n = regions.len();
        let current = regions
            .iter()
            .position(|f| *f == self.ui.focus)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % n
        } else {
            (current + n - 1) % n
        };
        self.ui.focus = regions[next];
    }

    fn cycle_layer(&mut self, up: bool) {
        let layers = self.screen().layers();
        let n = layers.len();
        let current = layers
            .iter()
            .position(|l| l.id == self.editor.layer)
            .unwrap_or(0);
        let next = if up {
            (current + 1) % n
        } else {
            (current + n - 1) % n
        };
        let layer = &layers[next];
        let msg = format!("Layer: {} ({})", layer.name, layer.kind().name());
        self.editor.layer = layer.id.clone();
        self.editor.selection.clear();
        self.layout_cache_clear();
        self.clamp_cursor();
        self.set_status(StatusKind::Info, msg);
    }

    pub(super) fn layout_cache_clear(&mut self) {
        self.mark_edited();
    }

    pub(super) fn move_cursor(&mut self, dir: Direction) {
        let (dx, dy) = delta(dir);
        let c = self.editor.cursor;
        let mut target = Position::new(
            (i32::from(c.x) + dx).max(0) as u16,
            (i32::from(c.y) + dy).max(0) as u16,
        );
        // Never land on the tail of a wide glyph: step over it.
        if dir == Direction::Right && self.cell_at(target).is_some_and(Cell::is_wide_tail) {
            target.x = target.x.saturating_add(1);
            if target.x >= self.doc_size().width {
                return;
            }
        }
        self.set_cursor(target, None);
    }

    fn clamp_cursor(&mut self) {
        self.set_cursor(self.editor.cursor, Some(self.editor.line_start_x));
    }

    /// Clamps `pos` into the screen, snaps off wide-glyph tails, scrolls
    /// the viewport and optionally records a new line-start column.
    pub(super) fn set_cursor(&mut self, pos: Position, line_start: Option<u16>) {
        let size = self.doc_size();
        let mut pos = Position::new(
            pos.x.min(size.width.saturating_sub(1)),
            pos.y.min(size.height.saturating_sub(1)),
        );
        if self.cell_at(pos).is_some_and(Cell::is_wide_tail) {
            pos.x = pos.x.saturating_sub(1);
        }
        self.editor.cursor = pos;
        self.editor.line_start_x = line_start.unwrap_or(pos.x);
        self.editor.viewport.ensure_visible(pos, size);
    }

    /// Whether the active layer accepts cell edits; reports why not.
    fn editable_cells_layer(&mut self) -> bool {
        match self.active_layer() {
            None => {
                self.set_status(StatusKind::Error, "Active layer no longer exists");
                false
            }
            Some(l) if l.locked => {
                let name = l.name.clone();
                self.set_status(StatusKind::Warning, format!("Layer {name:?} is locked"));
                false
            }
            Some(l) if l.cells().is_none() => {
                let name = l.name.clone();
                self.set_status(
                    StatusKind::Warning,
                    format!("Layer {name:?} holds components; switch to an artwork layer to type (Ctrl+PgUp/PgDn)"),
                );
                false
            }
            Some(_) => true,
        }
    }

    /// Records one cell write in the open edit transaction.
    fn write_cell(&mut self, pos: Position, cell: Cell) -> bool {
        if !self.session.has_open_transaction() {
            if let Err(e) = self.session.begin("Type") {
                self.report_error(&e);
                return false;
            }
        }
        let op = Operation::SetCells {
            layer: self.editor.layer.clone(),
            cells: vec![CellWrite {
                x: pos.x,
                y: pos.y,
                cell,
            }],
        };
        match self.session.record(op) {
            Ok(()) => {
                self.mark_edited();
                true
            }
            Err(e) => {
                let text = e.to_string();
                if text.contains("last column") {
                    self.set_status(
                        StatusKind::Warning,
                        "No room for a double-width glyph in the last column",
                    );
                } else {
                    self.report_error(&e);
                }
                false
            }
        }
    }

    /// Closes the open edit transaction, if any.
    pub fn commit_edits(&mut self) {
        if self.session.has_open_transaction() {
            self.session.end();
        }
    }

    fn insert_text(&mut self, text: &str) {
        if !self.editable_cells_layer() {
            return;
        }
        let line_start = self.editor.line_start_x;
        for grapheme in Grapheme::split_text(text) {
            let width = u16::from(grapheme.width());
            let cell = Cell::glyph(grapheme, self.editor.style);
            if !self.write_cell(self.editor.cursor, cell) {
                break;
            }
            let next_x = self.editor.cursor.x.saturating_add(width);
            if next_x < self.doc_size().width {
                let y = self.editor.cursor.y;
                self.set_cursor(Position::new(next_x, y), Some(line_start));
            }
        }
    }

    fn backspace(&mut self) {
        if self.editor.cursor.x == 0 || !self.editable_cells_layer() {
            return;
        }
        let line_start = self.editor.line_start_x;
        let y = self.editor.cursor.y;
        let x = self.editor.cursor.x - 1;
        self.set_cursor(Position::new(x, y), Some(line_start));
        self.write_cell(self.editor.cursor, Cell::EMPTY);
    }

    fn delete_forward(&mut self) {
        if !self.editable_cells_layer() {
            return;
        }
        self.write_cell(self.editor.cursor, Cell::EMPTY);
    }

    /// Whether the active layer is an artwork layer.
    pub fn on_artwork_layer(&self) -> bool {
        self.active_layer().and_then(Layer::cells).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use glyphforge_core::{CellContent, Size};

    fn glyph_at(app: &App, x: u16, y: u16) -> String {
        match &app.cell_at(Position::new(x, y)).unwrap().content {
            CellContent::Empty => ".".into(),
            CellContent::Glyph(g) => g.as_str().into(),
            CellContent::WideTail => "<".into(),
        }
    }

    fn app(w: u16, h: u16) -> App {
        let mut app = test_app(Size::new(w, h));
        app.editor.viewport.size = Size::new(w, h);
        app
    }

    #[test]
    fn typing_advances_cursor_and_marks_dirty() {
        let mut app = app(10, 3);
        app.dispatch(Action::InsertText("ab".into()));
        assert_eq!(glyph_at(&app, 0, 0), "a");
        assert_eq!(glyph_at(&app, 1, 0), "b");
        assert_eq!(app.editor.cursor, Position::new(2, 0));
        assert!(app.dirty());
    }

    #[test]
    fn typing_is_one_undo_step_until_cursor_moves() {
        let mut app = app(10, 3);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::InsertText("b".into()));
        app.dispatch(Action::CursorMove(Direction::Down));
        app.dispatch(Action::InsertText("c".into()));
        app.dispatch(Action::Undo);
        assert_eq!(glyph_at(&app, 0, 1), ".");
        assert_eq!(glyph_at(&app, 1, 0), "b", "first word survives one undo");
        app.dispatch(Action::Undo);
        assert_eq!(glyph_at(&app, 0, 0), ".");
        assert!(!app.dirty());
        app.dispatch(Action::Redo);
        assert_eq!(glyph_at(&app, 1, 0), "b");
        assert!(app.status.as_ref().unwrap().text.starts_with("Redo"));
    }

    #[test]
    fn wide_glyph_advances_two_and_cursor_skips_tail() {
        let mut app = app(10, 3);
        app.dispatch(Action::InsertText("漢".into()));
        assert_eq!(app.editor.cursor, Position::new(2, 0));
        app.dispatch(Action::CursorMove(Direction::Left));
        assert_eq!(app.editor.cursor, Position::new(0, 0));
        app.dispatch(Action::CursorMove(Direction::Right));
        assert_eq!(app.editor.cursor, Position::new(2, 0));
    }

    #[test]
    fn wide_glyph_at_edge_is_refused_with_warning() {
        let mut app = app(3, 1);
        app.dispatch(Action::CursorLineEnd);
        app.dispatch(Action::InsertText("漢".into()));
        assert_eq!(glyph_at(&app, 2, 0), ".");
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
        app.dispatch(Action::CursorLineStart);
        assert!(!app.dirty(), "a failed write leaves no transaction behind");
    }

    #[test]
    fn backspace_erases_left_and_delete_erases_here() {
        let mut app = app(10, 3);
        app.dispatch(Action::InsertText("abc".into()));
        app.dispatch(Action::Backspace);
        assert_eq!(glyph_at(&app, 2, 0), ".");
        assert_eq!(app.editor.cursor, Position::new(2, 0));
        app.dispatch(Action::CursorLineStart);
        app.dispatch(Action::DeleteForward);
        assert_eq!(glyph_at(&app, 0, 0), ".");
        assert_eq!(glyph_at(&app, 1, 0), "b");
    }

    #[test]
    fn new_line_returns_to_line_start_column() {
        let mut app = app(10, 3);
        app.dispatch(Action::CursorTo(Position::new(3, 0)));
        app.dispatch(Action::InsertText("xy".into()));
        app.dispatch(Action::NewLine);
        assert_eq!(app.editor.cursor, Position::new(3, 1));
    }

    #[test]
    fn cursor_is_clamped_and_viewport_follows() {
        let mut app = test_app(Size::new(100, 50));
        app.editor.viewport.size = Size::new(10, 5);
        app.dispatch(Action::CursorTo(Position::new(500, 500)));
        assert_eq!(app.editor.cursor, Position::new(99, 49));
        assert_eq!(app.editor.viewport.offset, Position::new(90, 45));
    }

    #[test]
    fn quit_requires_confirmation_when_dirty() {
        let mut app = app(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::Quit);
        assert!(!app.should_quit());
        app.dispatch(Action::Quit);
        assert!(app.should_quit());
    }

    #[test]
    fn other_actions_disarm_quit() {
        let mut app = app(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::Quit);
        app.dispatch(Action::CursorMove(Direction::Left));
        app.dispatch(Action::Quit);
        assert!(!app.should_quit());
    }

    #[test]
    fn focus_cycles_over_visible_regions() {
        let mut app = app(5, 1);
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::RightPanel);
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::LeftPanel);
        app.dispatch(Action::FocusPrev);
        assert_eq!(app.ui.focus, Focus::RightPanel);
        app.dispatch(Action::ToggleRightPanel);
        assert_eq!(app.ui.focus, Focus::Canvas);
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::LeftPanel);
    }

    #[test]
    fn interface_layer_refuses_typing_with_hint() {
        let mut app = app(5, 1);
        app.dispatch(Action::LayerNext);
        assert!(!app.on_artwork_layer());
        app.dispatch(Action::InsertText("a".into()));
        assert!(!app.dirty());
        assert!(app.status.as_ref().unwrap().text.contains("components"));
        app.dispatch(Action::LayerPrev);
        assert!(app.on_artwork_layer());
    }

    #[test]
    fn new_document_needs_confirmation_when_dirty() {
        let mut app = app(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::NewDocument);
        assert!(app.dirty());
        app.dispatch(Action::NewDocument);
        assert!(!app.dirty());
        assert_eq!(app.doc_size(), Size::new(80, 24));
    }

    #[test]
    fn save_without_path_warns_and_with_path_writes() {
        let mut app = app(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::SaveDocument);
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
        let dir = std::env::temp_dir().join(format!("glyphforge-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.glyph");
        app.session.set_path(path.clone());
        app.dispatch(Action::SaveDocument);
        assert!(!app.dirty());
        assert!(path.exists());
        let reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.document(), app.doc());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn use_omarchy_theme_requires_omarchy() {
        let mut app = app(5, 1);
        app.dispatch(Action::UseOmarchyTheme);
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
        app.ui_theme = Theme_minimal();
        app.ui_theme_origin = UiThemeOrigin::Omarchy;
        app.dispatch(Action::UseOmarchyTheme);
        assert_eq!(app.doc().theme.id.as_str(), "minimal-dark");
        app.dispatch(Action::Undo);
        assert_eq!(app.doc().theme.id.as_str(), "terminal");
    }

    #[allow(non_snake_case)]
    fn Theme_minimal() -> glyphforge_core::Theme {
        glyphforge_core::Theme::minimal_dark()
    }

    fn interface_app() -> App {
        let mut app = app(40, 12);
        app.dispatch(Action::LayerNext); // main-ui
        assert_eq!(app.design_mode(), crate::app::DesignMode::Interface);
        app
    }

    fn id_of(name: &str) -> glyphforge_core::ObjectId {
        glyphforge_core::ObjectId::new(name).unwrap()
    }

    fn submit(app: &mut App, text: &str) {
        app.prompt.as_mut().unwrap().input = text.to_owned();
        app.dispatch(Action::PromptSubmit);
    }

    #[test]
    fn add_select_move_resize_delete_with_undo() {
        let mut app = interface_app();
        app.dispatch(Action::CursorTo(Position::new(3, 2)));
        app.dispatch(Action::AddComponent);
        assert!(app.prompt.is_some());
        submit(&mut app, "panel Metrics");
        let id = app.primary().cloned().unwrap();
        assert_eq!(id.as_str(), "metrics");
        assert_eq!(
            app.layout().rect(&id),
            Some(glyphforge_core::Rect::new(3, 2, 20, 6))
        );

        app.dispatch(Action::MoveSelection(Direction::Right));
        app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(app.layout().rect(&id).unwrap().x, 4);
        assert_eq!(app.layout().rect(&id).unwrap().y, 3);
        app.dispatch(Action::ResizeSelection(Direction::Right));
        app.dispatch(Action::ResizeSelection(Direction::Up));
        assert_eq!(app.layout().rect(&id).unwrap().width, 21);
        assert_eq!(app.layout().rect(&id).unwrap().height, 5);

        app.dispatch(Action::Undo);
        assert_eq!(
            app.layout().rect(&id).unwrap().height,
            6,
            "each nudge is one undo step"
        );

        app.dispatch(Action::EditProperty);
        submit(&mut app, "title=Latency");
        assert_eq!(
            app.doc().component(&id).unwrap().prop_str("title"),
            Some("Latency")
        );

        app.dispatch(Action::DeleteSelection);
        assert!(app.doc().component(&id).is_none());
        assert!(app.selection().is_empty());
        app.dispatch(Action::Undo);
        assert!(app.doc().component(&id).is_some(), "delete is undoable");
    }

    #[test]
    fn arrows_move_cursor_when_nothing_is_selected() {
        let mut app = interface_app();
        app.dispatch(Action::MoveSelection(Direction::Right));
        assert_eq!(app.editor.cursor, Position::new(1, 0));
    }

    #[test]
    fn select_cycles_and_hit_tests() {
        let mut app = interface_app();
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel a");
        app.dispatch(Action::Cancel); // clear selection so the next panel is a root
        app.dispatch(Action::CursorTo(Position::new(25, 0)));
        app.dispatch(Action::AddComponent);
        submit(&mut app, "label b");
        app.dispatch(Action::Cancel);
        assert!(app.selection().is_empty());
        app.dispatch(Action::SelectNext);
        assert_eq!(app.primary().unwrap().as_str(), "a");
        app.dispatch(Action::SelectNext);
        assert_eq!(app.primary().unwrap().as_str(), "b");
        app.dispatch(Action::SelectNext);
        assert_eq!(app.primary().unwrap().as_str(), "a", "wraps around");
        app.dispatch(Action::SelectPrev);
        assert_eq!(app.primary().unwrap().as_str(), "b");
        app.dispatch(Action::SelectAt(Position::new(5, 5)));
        assert_eq!(app.primary().unwrap().as_str(), "a");
        app.dispatch(Action::SelectAt(Position::new(39, 11)));
        assert!(app.selection().is_empty(), "clicking empty space deselects");
    }

    #[test]
    fn child_is_added_inside_selection_and_found_by_id() {
        let mut app = interface_app();
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel box");
        app.dispatch(Action::CursorTo(Position::new(2, 2)));
        app.dispatch(Action::AddComponent);
        submit(&mut app, "label inner");
        let doc = app.doc();
        assert_eq!(
            doc.component(&glyphforge_core::ObjectId::slugify("box"))
                .unwrap()
                .children[0]
                .id
                .as_str(),
            "inner"
        );
        // Deepest component wins the hit test.
        app.dispatch(Action::SelectAt(Position::new(2, 2)));
        assert_eq!(app.primary().unwrap().as_str(), "inner");
        // Jump by id from an artwork layer switches layers.
        app.dispatch(Action::LayerPrev);
        assert!(app.on_artwork_layer());
        app.dispatch(Action::SelectById);
        submit(&mut app, "box");
        assert!(!app.on_artwork_layer());
        assert_eq!(app.primary().unwrap().as_str(), "box");
    }

    #[test]
    fn mouse_drag_moves_and_resizes_as_single_transactions() {
        let mut app = interface_app();
        app.snap_enabled = false;
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel p");
        let id = app.primary().cloned().unwrap();
        let before = app.session.history().undo_len();
        assert!(app.begin_drag(Position::new(5, 3)));
        app.drag_to(Position::new(7, 4));
        app.drag_to(Position::new(9, 5));
        app.end_drag();
        assert_eq!(app.layout().rect(&id).unwrap().x, 4);
        assert_eq!(app.layout().rect(&id).unwrap().y, 2);
        assert_eq!(
            app.session.history().undo_len(),
            before + 1,
            "one transaction per drag"
        );
        // Bottom-right corner resizes: rect is 4,2 20x6 -> corner at 23,7.
        assert!(app.begin_drag(Position::new(23, 7)));
        app.drag_to(Position::new(25, 8));
        app.end_drag();
        assert_eq!(app.layout().rect(&id).unwrap().width, 22);
        assert_eq!(app.layout().rect(&id).unwrap().height, 7);
        assert!(
            !app.begin_drag(Position::new(39, 11)),
            "drag outside the selection does nothing"
        );
    }

    #[test]
    fn dragging_snaps_to_the_canvas_and_reports_a_guide() {
        use glyphforge_core::geometry::Guide;
        let mut app = interface_app();
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel p");
        let id = app.primary().cloned().unwrap();
        assert!(app.snap_enabled);
        // The canvas is 40x12, the panel 20x6: dragging its centre within
        // one cell of the canvas centre snaps it there.
        assert!(app.begin_drag(Position::new(2, 2)));
        app.drag_to(Position::new(6, 4));
        assert_eq!(
            app.layout().rect(&id).unwrap().y,
            3,
            "snapped onto the centre row"
        );
        assert!(app.active_guides().contains(&Guide::Horizontal(6)));
        app.end_drag();
        assert!(app.active_guides().is_empty());

        app.dispatch(Action::ToggleSnap);
        assert!(!app.snap_enabled);
        assert!(app.begin_drag(Position::new(6, 4)));
        app.drag_to(Position::new(6, 5));
        assert_eq!(
            app.layout().rect(&id).unwrap().y,
            4,
            "without snapping the delta is exact"
        );
        app.end_drag();
    }

    #[test]
    fn extend_selection_and_select_all() {
        let mut app = interface_app();
        for name in ["a", "b", "c"] {
            app.dispatch(Action::Cancel);
            app.dispatch(Action::AddComponent);
            submit(&mut app, &format!("panel {name}"));
        }
        app.dispatch(Action::Cancel);
        app.dispatch(Action::SelectNext);
        assert_eq!(app.selection().len(), 1);
        app.dispatch(Action::ExtendSelectNext);
        assert_eq!(app.selection().len(), 2);
        assert_eq!(app.primary().unwrap().as_str(), "b");
        app.dispatch(Action::ExtendSelectPrev);
        assert_eq!(
            app.selection().len(),
            3,
            "extending backwards wraps to the last one"
        );
        app.dispatch(Action::Cancel);
        assert!(app.selection().is_empty());
        app.dispatch(Action::SelectAll);
        assert_eq!(app.selection().len(), 3);
        // Clicking one component replaces the whole selection.
        app.dispatch(Action::SelectAt(Position::new(1, 1)));
        assert_eq!(app.selection().len(), 1);
        // Shift-clicking adds and removes.
        app.dispatch(Action::ExtendSelectAt(Position::new(1, 1)));
        assert!(
            app.selection().is_empty(),
            "shift-clicking a selected component removes it"
        );
    }

    #[test]
    fn nudging_and_deleting_apply_to_the_whole_selection_in_one_step() {
        let mut app = interface_app();
        for (name, x) in [("a", 0u16), ("b", 22)] {
            app.dispatch(Action::Cancel);
            app.dispatch(Action::CursorTo(Position::new(x, 0)));
            app.dispatch(Action::AddComponent);
            submit(&mut app, &format!("panel {name}"));
        }
        app.dispatch(Action::SelectAll);
        let before = app.session.history().undo_len();
        app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(
            app.session.history().undo_len(),
            before + 1,
            "one undo step for both"
        );
        assert_eq!(app.layout().rect(&id_of("a")).unwrap().y, 1);
        assert_eq!(app.layout().rect(&id_of("b")).unwrap().y, 1);
        app.dispatch(Action::Undo);
        assert_eq!(app.layout().rect(&id_of("a")).unwrap().y, 0);

        app.dispatch(Action::SelectAll);
        app.dispatch(Action::EditProperty);
        submit(&mut app, "title=Shared");
        assert_eq!(
            app.doc().component(&id_of("b")).unwrap().prop_str("title"),
            Some("Shared")
        );

        app.dispatch(Action::SelectAll);
        app.dispatch(Action::DeleteSelection);
        assert!(app.doc().components().next().is_none());
        assert!(app.selection().is_empty());
        app.dispatch(Action::Undo);
        assert_eq!(
            app.doc().components().count(),
            2,
            "one undo brings both back"
        );
    }

    #[test]
    fn align_distribute_and_match_size_need_two_components() {
        use glyphforge_core::geometry::{Align, Axis};
        let mut app = interface_app();
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel only");
        app.dispatch(Action::AlignSelection(Align::Left));
        assert!(
            app.status.as_ref().unwrap().text.contains("at least two"),
            "{:?}",
            app.status
        );

        for (name, x, y) in [("a", 0u16, 0u16), ("b", 10, 4), ("c", 30, 8)] {
            app.dispatch(Action::Cancel);
            app.dispatch(Action::CursorTo(Position::new(x, y)));
            app.dispatch(Action::AddComponent);
            submit(&mut app, &format!("label {name}"));
        }
        app.dispatch(Action::Cancel);
        app.dispatch(Action::SelectAt(Position::new(0, 0)));
        app.dispatch(Action::ExtendSelectAt(Position::new(10, 4)));
        app.dispatch(Action::ExtendSelectAt(Position::new(30, 8)));
        assert_eq!(app.selection().len(), 3);

        app.dispatch(Action::AlignSelection(Align::Left));
        for name in ["a", "b", "c"] {
            assert_eq!(app.layout().rect(&id_of(name)).unwrap().x, 0, "{name}");
        }
        app.dispatch(Action::Undo);
        assert_eq!(
            app.layout().rect(&id_of("b")).unwrap().x,
            10,
            "one undo for the whole align"
        );

        app.dispatch(Action::DistributeSelection(Axis::Vertical));
        assert_eq!(app.layout().rect(&id_of("b")).unwrap().y, 4);
        app.dispatch(Action::EqualizeSelection(Axis::Horizontal));
        let width = app.layout().rect(&id_of("a")).unwrap().width;
        assert_eq!(app.layout().rect(&id_of("b")).unwrap().width, width);
    }

    #[test]
    fn reparent_moves_the_selection_under_the_cursor() {
        let mut app = interface_app();
        app.dispatch(Action::AddComponent);
        submit(&mut app, "panel host");
        app.dispatch(Action::Cancel);
        app.dispatch(Action::CursorTo(Position::new(25, 0)));
        app.dispatch(Action::AddComponent);
        submit(&mut app, "label guest");
        // The cursor sits inside the host, so the guest moves into it.
        app.dispatch(Action::CursorTo(Position::new(2, 2)));
        app.dispatch(Action::SelectById);
        submit(&mut app, "guest");
        app.dispatch(Action::CursorTo(Position::new(2, 2)));
        app.dispatch(Action::ReparentSelection);
        assert_eq!(
            app.doc().component(&id_of("host")).unwrap().children[0]
                .id
                .as_str(),
            "guest"
        );
        // Reparenting onto empty space lifts it back to the top level.
        app.dispatch(Action::CursorTo(Position::new(38, 11)));
        app.dispatch(Action::ReparentSelection);
        assert!(
            app.doc()
                .component(&id_of("host"))
                .unwrap()
                .children
                .is_empty()
        );
        app.dispatch(Action::Undo);
        assert_eq!(
            app.doc().component(&id_of("host")).unwrap().children.len(),
            1
        );
    }

    #[test]
    fn save_as_prompt_writes_file() {
        let mut app = app(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        let dir = std::env::temp_dir().join(format!("glyphforge-saveas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved.glyph");
        app.dispatch(Action::SaveDocumentAs);
        submit(&mut app, &path.display().to_string());
        assert!(path.exists());
        assert!(!app.dirty());
        assert_eq!(app.title(), "saved.glyph");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn composite_cache_invalidates_on_edit() {
        let mut app = app(5, 1);
        assert!(app.composite().cells().iter().all(Cell::is_empty));
        app.dispatch(Action::InsertText("a".into()));
        assert_eq!(
            app.composite()
                .get(Position::ORIGIN)
                .unwrap()
                .as_glyph()
                .unwrap()
                .as_str(),
            "a"
        );
    }
}
