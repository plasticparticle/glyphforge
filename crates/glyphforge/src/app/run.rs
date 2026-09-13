//! The event loop.

use std::time::Duration;

use crossterm::event::{self, Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::{App, EditorMode, Focus};
use crate::actions::{Action, Context, Direction, vim_navigation};
use crate::config::ThemeSource;
use crate::input::{Key, KeyCode, key_from_crossterm};
use crate::render::screen_to_document;
use crate::terminal::TerminalGuard;
use crate::ui;

/// Errors that end the session.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

const TICK: Duration = Duration::from_millis(250);

/// Runs the application until it asks to quit.
pub fn run(app: &mut App, guard: &mut TerminalGuard) -> Result<(), RunError> {
    let terminal = guard.terminal_mut();
    terminal.clear()?;
    let mut needs_redraw = true;
    while !app.should_quit() {
        if needs_redraw {
            let size = terminal.size()?;
            app.update_layout(ratatui::layout::Rect::new(0, 0, size.width, size.height));
            terminal.draw(|frame| ui::render(app, frame))?;
            needs_redraw = false;
        }
        if !event::poll(TICK)? {
            continue;
        }
        let ev = event::read()?;
        needs_redraw = handle_event(app, &ev);
    }
    Ok(())
}

/// Handles one terminal event. Returns whether a redraw is needed.
fn handle_event(app: &mut App, ev: &Event) -> bool {
    match ev {
        Event::Key(k) => {
            if let Some(key) = key_from_crossterm(k) {
                handle_key(app, key);
            }
            true
        }
        Event::Mouse(m) => handle_mouse(app, *m),
        Event::Paste(text) => {
            if app.ui.focus == Focus::Canvas && app.editor.mode == EditorMode::Insert {
                app.dispatch(Action::InsertText(text.clone()));
            }
            true
        }
        Event::Resize(_, _) => true,
        Event::FocusGained => {
            if matches!(
                app.config.theme.source,
                ThemeSource::Auto | ThemeSource::Omarchy
            ) && app.is_omarchy
            {
                app.load_ui_theme();
            }
            true
        }
        Event::FocusLost => false,
    }
}

/// Routes a key: help overlay first, then the keymap, then Vim navigation,
/// then text insertion.
pub(crate) fn handle_key(app: &mut App, key: Key) {
    if app.prompt.is_some() {
        let action = match key.code {
            KeyCode::Esc => Action::PromptCancel,
            KeyCode::Enter => Action::PromptSubmit,
            KeyCode::Backspace => Action::PromptBackspace,
            KeyCode::Down => Action::PromptNext,
            KeyCode::Up => Action::PromptPrev,
            KeyCode::Tab => Action::PromptComplete,
            _ => match key.text_char() {
                Some(c) => Action::PromptInput(c),
                None => return,
            },
        };
        app.dispatch(action);
        return;
    }
    if app.ui.help_open {
        if matches!(key.code, KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q')) {
            app.ui.help_open = false;
        }
        return;
    }
    let chord = key.chord();
    let context = if app.on_artwork_layer() {
        Context::Global
    } else {
        Context::Interface
    };
    let bound = app.keymap.lookup(chord, context).cloned();
    // Text insertion only exists on artwork layers; interface layers treat
    // plain keys as commands.
    let inserting = app.ui.focus == Focus::Canvas
        && app.editor.mode == EditorMode::Insert
        && app.on_artwork_layer();

    // In insert mode plain printable keys insert text even if a binding
    // exists for the bare character (bindings on bare characters are meant
    // for normal mode).
    if inserting && key.mods.is_none() && key.text_char().is_some() {
        if let Some(action) = bound.filter(|_| !matches!(chord.code, KeyCode::Char(_))) {
            app.dispatch(action);
            return;
        }
        if let Some(c) = key.text_char() {
            app.dispatch(Action::InsertText(c.to_string()));
        }
        return;
    }
    if let Some(action) = bound {
        app.dispatch(action);
        return;
    }
    if app.ui.vim_navigation
        && app.ui.focus == Focus::Canvas
        && app.editor.mode == EditorMode::Normal
    {
        if let Some(action) = vim_navigation(&key) {
            app.dispatch(action);
        } else if key.code == KeyCode::Char('i') && key.mods.is_none() {
            app.enter_insert_mode();
        }
        return;
    }
    if inserting {
        if let Some(c) = key.text_char() {
            app.dispatch(Action::InsertText(c.to_string()));
        }
    }
}

fn handle_mouse(app: &mut App, m: MouseEvent) -> bool {
    if !app.config.mouse.enabled {
        return false;
    }
    let layout = ui::layout::compute(app.last_frame_area(), &app.ui);
    let doc_pos = screen_to_document(
        (m.column, m.row),
        layout.canvas,
        app.editor.viewport.rect(),
        glyphforge_core::Rect::from_size(app.doc_size()),
    );
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(pos) = doc_pos {
                if app.on_artwork_layer() {
                    app.dispatch(Action::CursorTo(pos));
                } else if m.modifiers.contains(KeyModifiers::SHIFT) {
                    app.dispatch(Action::ExtendSelectAt(pos));
                } else if !app.begin_drag(pos) {
                    app.dispatch(Action::SelectAt(pos));
                    // A fresh selection can be dragged right away.
                    app.begin_drag(pos);
                }
            } else if layout
                .left
                .is_some_and(|r| r.contains((m.column, m.row).into()))
            {
                app.ui.focus = Focus::LeftPanel;
            } else if layout
                .right
                .is_some_and(|r| r.contains((m.column, m.row).into()))
            {
                app.ui.focus = Focus::RightPanel;
            } else if layout.canvas_block.contains((m.column, m.row).into()) {
                app.ui.focus = Focus::Canvas;
            }
            true
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.drag.is_some() {
                if let Some(pos) = doc_pos {
                    app.drag_to(pos);
                }
                true
            } else {
                false
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let had = app.drag.is_some();
            app.end_drag();
            had
        }
        MouseEventKind::ScrollUp => {
            app.dispatch(Action::ScrollView(Direction::Up));
            true
        }
        MouseEventKind::ScrollDown => {
            app.dispatch(Action::ScrollView(Direction::Down));
            true
        }
        MouseEventKind::ScrollLeft => {
            app.dispatch(Action::ScrollView(Direction::Left));
            true
        }
        MouseEventKind::ScrollRight => {
            app.dispatch(Action::ScrollView(Direction::Right));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use crate::input::Modifiers;
    use glyphforge_core::{Position, Size};

    fn key(c: char) -> Key {
        Key::new(KeyCode::Char(c), Modifiers::NONE)
    }

    #[test]
    fn plain_characters_insert_text() {
        let mut app = test_app(Size::new(10, 2));
        app.editor.viewport.size = Size::new(10, 2);
        handle_key(&mut app, key('x'));
        assert_eq!(app.editor.cursor, Position::new(1, 0));
        assert!(app.dirty());
    }

    #[test]
    fn bound_chords_dispatch_actions() {
        let mut app = test_app(Size::new(10, 2));
        handle_key(&mut app, Key::new(KeyCode::F(2), Modifiers::NONE));
        assert!(!app.ui.show_left_panel);
        handle_key(&mut app, Key::new(KeyCode::Char('q'), Modifiers::CTRL));
        assert!(app.should_quit());
    }

    #[test]
    fn help_overlay_swallows_keys_until_closed() {
        let mut app = test_app(Size::new(10, 2));
        handle_key(&mut app, Key::new(KeyCode::F(1), Modifiers::NONE));
        assert!(app.ui.help_open);
        handle_key(&mut app, key('x'));
        assert!(!app.dirty());
        handle_key(&mut app, Key::new(KeyCode::Esc, Modifiers::NONE));
        assert!(!app.ui.help_open);
    }

    #[test]
    fn vim_navigation_moves_in_normal_mode_and_inserts_after_i() {
        let mut app = test_app(Size::new(10, 2));
        app.editor.viewport.size = Size::new(10, 2);
        app.ui.vim_navigation = true;
        app.editor.mode = EditorMode::Normal;
        handle_key(&mut app, key('l'));
        assert_eq!(app.editor.cursor, Position::new(1, 0));
        assert!(!app.dirty());
        handle_key(&mut app, key('i'));
        assert_eq!(app.editor.mode, EditorMode::Insert);
        handle_key(&mut app, key('l'));
        assert!(app.dirty(), "in insert mode 'l' is text");
        handle_key(&mut app, Key::new(KeyCode::Esc, Modifiers::NONE));
        assert_eq!(app.editor.mode, EditorMode::Normal);
    }

    #[test]
    fn prompt_captures_keys_until_closed() {
        let mut app = test_app(Size::new(10, 2));
        app.editor.viewport.size = Size::new(10, 2);
        handle_key(&mut app, Key::new(KeyCode::Char('f'), Modifiers::CTRL));
        assert!(app.prompt.is_none(), "no components yet: prompt not opened");
        app.dispatch(Action::SaveDocumentAs);
        assert!(app.prompt.is_some());
        handle_key(&mut app, key('x'));
        assert!(app.prompt.as_ref().unwrap().input.ends_with('x'));
        assert!(!app.dirty(), "typing went to the prompt, not the canvas");
        handle_key(&mut app, Key::new(KeyCode::Esc, Modifiers::NONE));
        assert!(app.prompt.is_none());
    }

    #[test]
    fn interface_layer_treats_plain_keys_as_commands() {
        let mut app = test_app(Size::new(10, 2));
        app.editor.viewport.size = Size::new(10, 2);
        app.dispatch(Action::LayerNext);
        handle_key(&mut app, key('a'));
        assert!(app.prompt.is_some(), "'a' opens the add-component prompt");
        assert!(!app.dirty());
    }

    #[test]
    fn typing_is_ignored_when_a_panel_has_focus() {
        let mut app = test_app(Size::new(10, 2));
        app.ui.focus = Focus::RightPanel;
        handle_key(&mut app, key('x'));
        assert!(!app.dirty());
    }
}
