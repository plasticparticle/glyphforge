//! `App::dispatch`: the only place application behaviour lives.

use tuiforge_core::{CanvasError, Cell, DocumentError, Grapheme, Position};

use super::{App, EditorMode, Focus, StatusKind};
use crate::actions::{Action, Direction, descriptor_of};

impl App {
    /// Applies an action to the application state. Recoverable errors are
    /// reported in the status bar.
    pub fn dispatch(&mut self, action: Action) {
        tracing::trace!(?action, "dispatch");
        if !matches!(action, Action::Quit) {
            self.quit_armed = false;
        }
        match action {
            Action::Quit => self.quit(),
            Action::ToggleHelp => self.ui.help_open = !self.ui.help_open,
            Action::Cancel => self.cancel(),
            Action::ReloadTheme => {
                self.load_theme();
                let text = format!("Theme: {} ({})", self.theme.name, self.theme.origin);
                self.set_status(StatusKind::Info, text);
            }
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
            Action::CursorMove(dir) => self.move_cursor(dir),
            Action::CursorLineStart => {
                self.set_cursor(Position::new(0, self.editor.cursor.y), None);
            }
            Action::CursorLineEnd => {
                let x = self.doc.width().saturating_sub(1);
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
            Action::NewDocument
            | Action::OpenDocument
            | Action::SaveDocument
            | Action::SaveDocumentAs
            | Action::Undo
            | Action::Redo
            | Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::DeleteSelection
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
        if self.dirty && !self.quit_armed {
            self.quit_armed = true;
            self.set_status(
                StatusKind::Warning,
                "Unsaved changes will be lost. Press the quit key again to quit anyway.",
            );
            return;
        }
        self.should_quit = true;
    }

    fn cancel(&mut self) {
        if self.ui.help_open {
            self.ui.help_open = false;
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

    fn move_cursor(&mut self, dir: Direction) {
        let (dx, dy) = delta(dir);
        let c = self.editor.cursor;
        let mut target = Position::new(
            (i32::from(c.x) + dx).max(0) as u16,
            (i32::from(c.y) + dy).max(0) as u16,
        );
        // Never land on the tail of a wide glyph: step over it.
        if dir == Direction::Right && self.doc.cell_at(target).is_some_and(Cell::is_wide_tail) {
            target.x = target.x.saturating_add(1);
            if target.x >= self.doc.width() {
                return;
            }
        }
        self.set_cursor(target, None);
    }

    /// Clamps `pos` into the document, snaps off wide-glyph tails, scrolls
    /// the viewport and optionally records a new line-start column.
    fn set_cursor(&mut self, pos: Position, line_start: Option<u16>) {
        let size = self.doc_size();
        let mut pos = Position::new(
            pos.x.min(size.width.saturating_sub(1)),
            pos.y.min(size.height.saturating_sub(1)),
        );
        if self.doc.cell_at(pos).is_some_and(Cell::is_wide_tail) {
            pos.x = pos.x.saturating_sub(1);
        }
        self.editor.cursor = pos;
        self.editor.line_start_x = line_start.unwrap_or(pos.x);
        self.editor.viewport.ensure_visible(pos, size);
    }

    fn insert_text(&mut self, text: &str) {
        if self.doc.active_layer().locked {
            self.set_status(
                StatusKind::Warning,
                format!("Layer {:?} is locked", self.doc.active_layer().name),
            );
            return;
        }
        let line_start = self.editor.line_start_x;
        for grapheme in Grapheme::split_text(text) {
            let width = u16::from(grapheme.width());
            let cell = Cell::glyph(grapheme, self.editor.style);
            match self.doc.put_cell(self.editor.cursor, cell) {
                Ok(changes) => {
                    if !changes.is_empty() {
                        self.mark_edited();
                    }
                }
                Err(DocumentError::Canvas(CanvasError::WideGlyphAtEdge { .. })) => {
                    self.set_status(
                        StatusKind::Warning,
                        "No room for a double-width glyph in the last column",
                    );
                    break;
                }
                Err(e) => {
                    self.report_error(&e);
                    break;
                }
            }
            let next_x = self.editor.cursor.x.saturating_add(width);
            if next_x < self.doc.width() {
                let y = self.editor.cursor.y;
                self.set_cursor(Position::new(next_x, y), Some(line_start));
            }
        }
    }

    fn backspace(&mut self) {
        if self.editor.cursor.x == 0 {
            return;
        }
        let line_start = self.editor.line_start_x;
        let y = self.editor.cursor.y;
        let x = self.editor.cursor.x - 1;
        self.set_cursor(Position::new(x, y), Some(line_start));
        self.clear_at_cursor();
    }

    fn delete_forward(&mut self) {
        self.clear_at_cursor();
    }

    fn clear_at_cursor(&mut self) {
        match self.doc.put_cell(self.editor.cursor, Cell::EMPTY) {
            Ok(changes) => {
                if !changes.is_empty() {
                    self.mark_edited();
                }
            }
            Err(e) => self.report_error(&e),
        }
    }
}

const fn delta(dir: Direction) -> (i32, i32) {
    match dir {
        Direction::Up => (0, -1),
        Direction::Down => (0, 1),
        Direction::Left => (-1, 0),
        Direction::Right => (1, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use tuiforge_core::{CellContent, Size};

    fn glyph_at(app: &App, x: u16, y: u16) -> String {
        match &app.doc.cell_at(Position::new(x, y)).unwrap().content {
            CellContent::Empty => ".".into(),
            CellContent::Glyph(g) => g.as_str().into(),
            CellContent::WideTail => "<".into(),
        }
    }

    #[test]
    fn typing_advances_cursor_and_marks_dirty() {
        let mut app = test_app(Size::new(10, 3));
        app.editor.viewport.size = Size::new(10, 3);
        app.dispatch(Action::InsertText("ab".into()));
        assert_eq!(glyph_at(&app, 0, 0), "a");
        assert_eq!(glyph_at(&app, 1, 0), "b");
        assert_eq!(app.editor.cursor, Position::new(2, 0));
        assert!(app.dirty);
    }

    #[test]
    fn wide_glyph_advances_two_and_cursor_skips_tail() {
        let mut app = test_app(Size::new(10, 3));
        app.editor.viewport.size = Size::new(10, 3);
        app.dispatch(Action::InsertText("漢".into()));
        assert_eq!(app.editor.cursor, Position::new(2, 0));
        app.dispatch(Action::CursorMove(Direction::Left));
        assert_eq!(
            app.editor.cursor,
            Position::new(0, 0),
            "left from after a wide glyph lands on its head"
        );
        app.dispatch(Action::CursorMove(Direction::Right));
        assert_eq!(
            app.editor.cursor,
            Position::new(2, 0),
            "right from a head skips the tail"
        );
    }

    #[test]
    fn wide_glyph_at_edge_is_refused_with_warning() {
        let mut app = test_app(Size::new(3, 1));
        app.editor.viewport.size = Size::new(3, 1);
        app.dispatch(Action::CursorLineEnd);
        app.dispatch(Action::InsertText("漢".into()));
        assert_eq!(glyph_at(&app, 2, 0), ".");
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
        assert!(!app.dirty);
    }

    #[test]
    fn backspace_erases_left_and_delete_erases_here() {
        let mut app = test_app(Size::new(10, 3));
        app.editor.viewport.size = Size::new(10, 3);
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
        let mut app = test_app(Size::new(10, 3));
        app.editor.viewport.size = Size::new(10, 3);
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
        app.dispatch(Action::CursorMove(Direction::Right));
        assert_eq!(app.editor.cursor, Position::new(99, 49));
    }

    #[test]
    fn quit_requires_confirmation_when_dirty() {
        let mut app = test_app(Size::new(5, 1));
        app.editor.viewport.size = Size::new(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::Quit);
        assert!(!app.should_quit());
        app.dispatch(Action::Quit);
        assert!(app.should_quit());
    }

    #[test]
    fn other_actions_disarm_quit() {
        let mut app = test_app(Size::new(5, 1));
        app.editor.viewport.size = Size::new(5, 1);
        app.dispatch(Action::InsertText("a".into()));
        app.dispatch(Action::Quit);
        app.dispatch(Action::CursorMove(Direction::Left));
        app.dispatch(Action::Quit);
        assert!(!app.should_quit());
    }

    #[test]
    fn quit_is_immediate_when_clean() {
        let mut app = test_app(Size::new(5, 1));
        app.dispatch(Action::Quit);
        assert!(app.should_quit());
    }

    #[test]
    fn focus_cycles_over_visible_regions() {
        let mut app = test_app(Size::new(5, 1));
        assert_eq!(app.ui.focus, Focus::Canvas);
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::RightPanel);
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::LeftPanel);
        app.dispatch(Action::FocusPrev);
        assert_eq!(app.ui.focus, Focus::RightPanel);
        app.dispatch(Action::ToggleRightPanel);
        assert_eq!(
            app.ui.focus,
            Focus::Canvas,
            "hiding the focused panel refocuses the canvas"
        );
        app.dispatch(Action::FocusNext);
        assert_eq!(app.ui.focus, Focus::LeftPanel);
    }

    #[test]
    fn toggle_panels_hides_and_restores_both() {
        let mut app = test_app(Size::new(5, 1));
        app.dispatch(Action::TogglePanels);
        assert!(!app.ui.show_left_panel && !app.ui.show_right_panel);
        app.dispatch(Action::TogglePanels);
        assert!(app.ui.show_left_panel && app.ui.show_right_panel);
    }

    #[test]
    fn unimplemented_actions_report_instead_of_panicking() {
        let mut app = test_app(Size::new(5, 1));
        app.dispatch(Action::Undo);
        assert!(app.status.as_ref().is_some_and(|s| s.text.contains("Undo")));
    }

    #[test]
    fn locked_layer_blocks_typing() {
        let mut app = test_app(Size::new(5, 1));
        app.doc.layer_at_mut(0).unwrap().locked = true;
        app.dispatch(Action::InsertText("a".into()));
        assert_eq!(glyph_at(&app, 0, 0), ".");
        assert!(!app.dirty);
    }

    #[test]
    fn composite_cache_invalidates_on_edit() {
        let mut app = test_app(Size::new(5, 1));
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
