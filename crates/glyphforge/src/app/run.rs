//! The event loop.

use std::time::Duration;

use crossterm::event::{self, Event, MouseButton, MouseEvent, MouseEventKind};

use super::{App, EditorMode, Focus};
use crate::actions::{
    Action, Context, Direction, InspectorAction, MenuAction, MenuPage, PixelAction, vim_navigation,
};
use crate::app::ColorTarget;
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
        if let Some(text) = app.take_system_clipboard() {
            crate::terminal::clipboard::copy(&text);
        }
        if app.poll_proposals() || app.maybe_autosave() {
            needs_redraw = true;
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
        Event::Resize(_, _) => {
            // The cell size in pixels changes with the font size.
            if app.pixel_mouse.is_some() {
                if let Some(cell) = crate::terminal::cell_pixel_size() {
                    app.pixel_mouse = Some(cell);
                }
            }
            true
        }
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
    if popup_key(app, key) {
        return;
    }
    if app.prototype.is_some() && app.prompt.is_none() && app.menu.is_none() {
        match key.code {
            KeyCode::Tab => app.prototype_focus(true),
            KeyCode::BackTab => app.prototype_focus(false),
            KeyCode::Enter | KeyCode::Char(' ') => app.prototype_enter(),
            KeyCode::Backspace => app.prototype_back(),
            KeyCode::Esc | KeyCode::F(12) => app.toggle_prototype(),
            _ => {
                // Global keys (quit, help, palette) still work.
                if let Some(action) = app.keymap.lookup(key.chord(), Context::Global).cloned() {
                    if matches!(
                        action,
                        Action::Quit | Action::ToggleHelp | Action::CommandPalette
                    ) {
                        app.dispatch(action);
                    }
                }
            }
        }
        return;
    }
    if app.prompt.is_none() && app.picker.is_none() && app.theme_editor.is_some() {
        theme_editor_key(app, key);
        return;
    }
    if app.prompt.is_none() && app.gradient_editor.is_some() {
        gradient_editor_key(app, key);
        return;
    }
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
    if app.picker.is_some() {
        let target = app
            .picker
            .as_ref()
            .map_or(ColorTarget::Foreground, |p| p.target);
        let action = match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Action::Cancel,
            KeyCode::Enter | KeyCode::Char(' ') => Action::PickerSubmit,
            KeyCode::Tab | KeyCode::BackTab => Action::PickerSwitchTarget,
            KeyCode::Up | KeyCode::Char('k') => Action::PickerMove(Direction::Up),
            KeyCode::Down | KeyCode::Char('j') => Action::PickerMove(Direction::Down),
            KeyCode::Left | KeyCode::Char('h') => Action::PickerMove(Direction::Left),
            KeyCode::Right | KeyCode::Char('l') => Action::PickerMove(Direction::Right),
            KeyCode::Char('#' | '/') => Action::ColorPrompt(target),
            KeyCode::Char('d') => Action::SetColor(target, glyphforge_core::Color::Default),
            KeyCode::Char('r' | 'm') => Action::PickerToggleSpectrum,
            _ => return,
        };
        app.dispatch(action);
        return;
    }
    if app.menu.is_some() {
        let action = match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(10) => MenuAction::Close,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ' | 'l') => MenuAction::Select,
            KeyCode::Up | KeyCode::Char('k') => MenuAction::Move(Direction::Up),
            KeyCode::Down | KeyCode::Char('j') => MenuAction::Move(Direction::Down),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                MenuAction::Page(MenuPage::Main)
            }
            _ => return,
        };
        app.dispatch(Action::Menu(action));
        return;
    }
    if app.ui.help_open {
        if matches!(key.code, KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q')) {
            app.ui.help_open = false;
        }
        return;
    }
    if let Some(action) = inspector_key(app, key) {
        app.dispatch(Action::Inspector(action));
        return;
    }
    if let Some(action) = layers_key(app, key) {
        app.dispatch(action);
        return;
    }
    let chord = key.chord();
    let context = app.key_context();
    let bound = app.keymap.lookup(chord, context).cloned();
    // Text insertion only exists on artwork layers with the Text tool;
    // interface layers and painting tools treat plain keys as commands.
    let inserting = app.ui.focus == Focus::Canvas
        && app.editor.mode == EditorMode::Insert
        && context == Context::Global
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
    if let Some(c) = key.text_char() {
        if inserting {
            app.dispatch(Action::InsertText(c.to_string()));
        } else if context == Context::Paint && app.ui.focus == Focus::Canvas {
            // Any unbound character becomes the brush glyph.
            app.dispatch(Action::SetBrushGlyph(c.to_string()));
        }
    }
}

/// While the inspector has focus, plain navigation keys work on its rows;
/// everything else (Tab, Ctrl+Z, ...) falls through to the keymap.
fn inspector_key(app: &App, key: Key) -> Option<InspectorAction> {
    if app.ui.focus != Focus::RightPanel
        || app.design_mode() != super::DesignMode::Interface
        || app.selection().is_none()
        || !(key.mods.is_none() || key.mods == crate::input::Modifiers::SHIFT)
    {
        return None;
    }
    Some(match key.code {
        KeyCode::Up | KeyCode::Char('k') => InspectorAction::Move(false),
        KeyCode::Down | KeyCode::Char('j') => InspectorAction::Move(true),
        KeyCode::Enter | KeyCode::Char('e') => InspectorAction::Edit,
        KeyCode::Char(' ' | 'l') | KeyCode::Right => InspectorAction::Cycle(true),
        KeyCode::Left | KeyCode::Char('h') => InspectorAction::Cycle(false),
        KeyCode::Delete | KeyCode::Backspace => InspectorAction::Clear,
        _ => return None,
    })
}

/// Keys of the theme editor: arrows choose a token, Enter types its
/// colour, `c` opens the colour picker, `s` saves.
fn theme_editor_key(app: &mut App, key: Key) {
    match key.code {
        KeyCode::Esc => app.theme_editor_cancel(),
        KeyCode::Up | KeyCode::Char('k') => app.theme_editor_move(false),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.theme_editor_move(true),
        KeyCode::Enter | KeyCode::Char('e') => app.theme_editor_edit(),
        KeyCode::Char('c' | ' ') => app.open_color_picker(ColorTarget::Foreground),
        KeyCode::Char('n') => app.theme_editor_rename(),
        KeyCode::Char('s') => app.theme_editor_save(),
        _ => {}
    }
}

/// Keys of the gradient editor: arrows choose a stop, Shift+arrows (or
/// `<` `>` by ten) move it, Enter sets its colour.
fn gradient_editor_key(app: &mut App, key: Key) {
    let shift = key.mods == crate::input::Modifiers::SHIFT;
    match key.code {
        KeyCode::Esc => app.dispatch(Action::Cancel),
        KeyCode::Char('s') => app.save_gradient_editor(),
        KeyCode::Enter | KeyCode::Char('c') => app.edit_gradient_stop_color(),
        KeyCode::Char('t') => app.edit_gradient_as_text(),
        _ => {
            let Some(e) = app.gradient_editor.as_mut() else {
                return;
            };
            match key.code {
                KeyCode::Left if shift => e.nudge(-1),
                KeyCode::Right if shift => e.nudge(1),
                KeyCode::Char('H') => e.nudge(-1),
                KeyCode::Char('L') => e.nudge(1),
                KeyCode::Char('<') => e.nudge(-10),
                KeyCode::Char('>') => e.nudge(10),
                KeyCode::Left | KeyCode::Char('h') => e.select(false),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => e.select(true),
                KeyCode::Char('a' | '+') => e.add(),
                KeyCode::Char('x' | '-') | KeyCode::Delete => {
                    e.remove();
                }
                KeyCode::Char('o') => e.toggle_space(),
                KeyCode::Char('r') => e.reverse(),
                _ => {}
            }
        }
    }
}

/// While the gradient editor is open it owns the mouse: pressing on the
/// bar picks the nearest stop, dragging moves it.
fn handle_gradient_mouse(app: &mut App, m: MouseEvent) -> bool {
    let area = app.last_frame_area();
    let Some(g) = ui::gradient_editor::geometry(app, area) else {
        return false;
    };
    let at = ui::gradient_editor::percent_at(&g, m.column);
    let Some(e) = app.gradient_editor.as_mut() else {
        return false;
    };
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) if g.track.contains((m.column, m.row).into()) => {
            e.selected = e.nearest(at);
            e.dragging = true;
        }
        MouseEventKind::Drag(MouseButton::Left) if e.dragging => e.place(at),
        MouseEventKind::Up(MouseButton::Left) => e.dragging = false,
        MouseEventKind::Down(_) if !g.popup.contains((m.column, m.row).into()) => {
            app.dispatch(Action::Cancel);
        }
        _ => return false,
    }
    true
}

/// While the layer list has focus, plain keys manage layers and screens;
/// everything else (Tab, Ctrl+Z, ...) falls through to the keymap.
fn layers_key(app: &App, key: Key) -> Option<Action> {
    use crate::app::{LayerAction, ScreenAction};
    if app.ui.focus != Focus::Layers {
        return None;
    }
    let shift = key.mods == crate::input::Modifiers::SHIFT;
    if !(key.mods.is_none() || shift) {
        return None;
    }
    Some(match key.code {
        KeyCode::Up if shift => Action::Layer(LayerAction::Raise(true)),
        KeyCode::Down if shift => Action::Layer(LayerAction::Raise(false)),
        KeyCode::Char('K') => Action::Layer(LayerAction::Raise(true)),
        KeyCode::Char('J') => Action::Layer(LayerAction::Raise(false)),
        KeyCode::Up | KeyCode::Char('k') => Action::LayerNext,
        KeyCode::Down | KeyCode::Char('j') => Action::LayerPrev,
        KeyCode::Char(' ' | 'v') => Action::Layer(LayerAction::ToggleVisible(None)),
        KeyCode::Char('l') => Action::Layer(LayerAction::ToggleLock(None)),
        KeyCode::Char('s') => Action::Layer(LayerAction::Isolate),
        KeyCode::Enter | KeyCode::Char('r') => Action::Layer(LayerAction::Rename),
        KeyCode::Char('n') => Action::Layer(LayerAction::New),
        KeyCode::Char('d') => Action::Layer(LayerAction::Duplicate),
        KeyCode::Char('m') => Action::Layer(LayerAction::MergeDown),
        KeyCode::Delete | KeyCode::Char('x') => Action::Layer(LayerAction::Delete),
        KeyCode::Char('[') | KeyCode::Left => Action::Screen(ScreenAction::Step(false)),
        KeyCode::Char(']') | KeyCode::Right => Action::Screen(ScreenAction::Step(true)),
        KeyCode::Char('N') => Action::Screen(ScreenAction::New),
        KeyCode::Char('R') => Action::Screen(ScreenAction::Rename),
        KeyCode::Char('S') => Action::Screen(ScreenAction::Resize),
        _ => return None,
    })
}

/// Keys of the popups that own the keyboard while open: the review of
/// agent changes, the problems panel, context menus and the command
/// palette. Returns whether the key was handled.
fn popup_key(app: &mut App, key: Key) -> bool {
    if app.review.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(9) => app.review = None,
            KeyCode::Enter | KeyCode::Char('a') => app.review_accept(),
            KeyCode::Char('x' | 'r') | KeyCode::Delete => app.review_reject(),
            KeyCode::Up | KeyCode::Char('k') => app.review_move(false),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.review_move(true),
            _ => {}
        }
        return true;
    }
    if app.problems.is_some() && app.prompt.is_none() {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.problems_move(false),
            KeyCode::Down | KeyCode::Char('j') => app.problems_move(true),
            KeyCode::Enter => app.problems_jump(),
            KeyCode::Esc | KeyCode::F(5) => app.problems = None,
            _ => return false,
        }
        return true;
    }
    if app.context_menu.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.context_menu = None,
            KeyCode::Enter | KeyCode::Char(' ') => app.context_run(None),
            KeyCode::Up | KeyCode::Char('k') => app.context_move(false),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.context_move(true),
            _ => {}
        }
        return true;
    }
    if app.palette.is_some() {
        match key.code {
            KeyCode::Esc => app.dispatch(Action::Cancel),
            KeyCode::Enter => app.palette_run(None),
            KeyCode::Backspace => app.palette_backspace(),
            KeyCode::Down | KeyCode::Tab => app.palette_move(true),
            KeyCode::Up | KeyCode::BackTab => app.palette_move(false),
            _ if key.mods.ctrl && matches!(key.code, KeyCode::Char('n' | 'j')) => {
                app.palette_move(true);
            }
            _ if key.mods.ctrl && matches!(key.code, KeyCode::Char('p' | 'k')) => {
                app.palette_move(false);
            }
            _ => {
                if let Some(c) = key.text_char() {
                    app.palette_input(c);
                }
            }
        }
        return true;
    }
    false
}

/// With pixel-precise reports (mode 1016) the coordinates are pixels:
/// converts them to cells and returns where inside the cell the pointer is,
/// as fractions.
fn to_cells(app: &App, mut m: MouseEvent) -> (MouseEvent, Option<(f32, f32)>) {
    let Some((cw, ch)) = app.pixel_mouse else {
        return (m, None);
    };
    let fraction = (
        f32::from(m.column % cw) / f32::from(cw),
        f32::from(m.row % ch) / f32::from(ch),
    );
    m.column /= cw;
    m.row /= ch;
    (m, Some(fraction))
}

/// The raster pixel under the pointer, and whether the position is only
/// known per cell (then the centre pixel stands for the cell).
fn pixel_at(
    app: &App,
    pos: glyphforge_core::Position,
    sub: Option<(f32, f32)>,
) -> (glyphforge_core::Position, bool) {
    let (cw, ch) = app.cell_px();
    let (fx, fy, whole) = sub.map_or((0.5, 0.5, true), |(x, y)| (x, y, false));
    let offset = |f: f32, n: u16| ((f * f32::from(n)) as u16).min(n - 1);
    (
        glyphforge_core::Position::new(pos.x * cw + offset(fx, cw), pos.y * ch + offset(fy, ch)),
        whole,
    )
}

fn handle_mouse(app: &mut App, m: MouseEvent) -> bool {
    if !app.config.mouse.enabled {
        return false;
    }
    let (m, sub) = to_cells(app, m);
    let cell = (m.column, m.row);
    if app.palette.is_some() {
        return handle_palette_mouse(app, m);
    }
    if app.context_menu.is_some() {
        return handle_context_mouse(app, m);
    }
    if app.gradient_editor.is_some() && app.prompt.is_none() {
        return handle_gradient_mouse(app, m);
    }
    if app.menu.is_some() {
        return handle_menu_mouse(app, m);
    }
    if app.picker.is_some() {
        return handle_picker_mouse(app, m);
    }
    let layout = ui::layout::compute(app.last_frame_area(), &app.ui);
    let doc_pos = screen_to_document(
        cell,
        layout.canvas,
        app.editor.viewport.rect(),
        glyphforge_core::Rect::from_size(app.doc_size()),
    );
    let in_left = layout.left.is_some_and(|r| r.contains(cell.into()));
    match m.kind {
        MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Right)) if in_left => {
            app.ui.focus = Focus::LeftPanel;
            let right = button == MouseButton::Right;
            if let Some(action) = layout
                .left
                .and_then(|area| ui::palette::hit(app, area, cell))
                .map(|hit| hit.action(right))
            {
                app.dispatch(action);
            }
            true
        }
        MouseEventKind::Down(MouseButton::Right) => right_click(app, &layout, doc_pos, sub, cell),
        MouseEventKind::Down(MouseButton::Left)
            if ui::canvas_view::minimap_hit(app, layout.canvas, cell).is_some() =>
        {
            if let Some(target) = ui::canvas_view::minimap_hit(app, layout.canvas, cell) {
                app.center_view_on(target);
            }
            true
        }
        MouseEventKind::Down(MouseButton::Left) if app.prototype.is_some() => {
            if let Some(pos) = doc_pos {
                app.prototype_click(pos);
            }
            true
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(pos) = doc_pos {
                if app.on_raster_layer() {
                    let (p, whole) = pixel_at(app, pos, sub);
                    app.dispatch(Action::Pixel(PixelAction::Press(p, whole)));
                } else if app.on_artwork_layer() && app.paint.tool == crate::app::Tool::Text {
                    app.dispatch(Action::CursorTo(pos));
                } else if app.on_artwork_layer() {
                    app.dispatch(Action::ToolPress(pos));
                } else if m.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) {
                    app.dispatch(Action::ToggleSelectAt(pos));
                } else if is_double_click(app, cell) {
                    app.dispatch(Action::DoubleClick(pos));
                } else if !app.begin_drag(pos) {
                    app.dispatch(Action::SelectAt(pos));
                    // A fresh selection can be dragged right away; empty
                    // space starts a rubber band.
                    if !app.begin_drag(pos) && app.selection().is_none() {
                        app.begin_band(pos);
                    }
                }
            } else if let Some(right) = layout.right.filter(|r| r.contains(cell.into())) {
                click_right_panel(app, right, cell);
            } else if layout.canvas_block.contains(cell.into()) {
                app.ui.focus = Focus::Canvas;
            }
            true
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let Some(pos) = doc_pos else { return false };
            if app.paint.pixel_stroke.is_some()
                || app.paint.pixel_anchor.is_some()
                || app.paint.pixel_moving.is_some()
            {
                let (p, _) = pixel_at(app, pos, sub);
                app.dispatch(Action::Pixel(PixelAction::Drag(p)));
                true
            } else if app.drag.is_some() {
                app.drag_to(pos);
                true
            } else if app.band_to(pos) {
                true
            } else if app.paint.stroke.is_some() || app.paint.anchor.is_some() {
                app.dispatch(Action::ToolDrag(pos));
                true
            } else {
                false
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.paint.pixel_stroke.is_some()
                || app.paint.pixel_anchor.is_some()
                || app.paint.pixel_moving.is_some()
            {
                app.dispatch(Action::Pixel(PixelAction::Release));
                return true;
            }
            if app.drag.is_some() {
                app.end_drag();
                true
            } else if app.end_band() {
                true
            } else if app.paint.stroke.is_some() || app.paint.anchor.is_some() {
                app.dispatch(Action::ToolRelease);
                true
            } else {
                false
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if app.on_artwork_layer()
                && layout.left.is_some_and(|area| {
                    ui::palette::layout(area).brushes.contains(cell.into())
                }) =>
        {
            app.dispatch(if m.kind == MouseEventKind::ScrollUp {
                Action::BrushPagePrev
            } else {
                Action::BrushPageNext
            });
            true
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let up = m.kind == MouseEventKind::ScrollUp;
            let dir = match (
                m.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                up,
            ) {
                (true, true) => Direction::Left,
                (true, false) => Direction::Right,
                (false, true) => Direction::Up,
                (false, false) => Direction::Down,
            };
            app.dispatch(Action::ScrollView(dir));
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

/// Right button: picks on pixel layers, otherwise opens the context menu
/// for the canvas or a layer row.
fn right_click(
    app: &mut App,
    layout: &ui::layout::ScreenLayout,
    doc_pos: Option<glyphforge_core::Position>,
    sub: Option<(f32, f32)>,
    cell: (u16, u16),
) -> bool {
    match doc_pos {
        Some(pos) if app.on_raster_layer() => {
            app.ui.focus = Focus::Canvas;
            let (p, _) = pixel_at(app, pos, sub);
            app.dispatch(Action::Pixel(PixelAction::Pick(p)));
            true
        }
        Some(pos) => {
            app.ui.focus = Focus::Canvas;
            app.open_context_menu(crate::app::ContextTarget::Canvas(pos), cell);
            true
        }
        None => match layout
            .right
            .and_then(|right| ui::panels::layers_hit(app, right, cell))
        {
            Some(ui::panels::LayerHit::Activate(id)) => {
                app.ui.focus = Focus::Layers;
                app.open_context_menu(crate::app::ContextTarget::Layer(id), cell);
                true
            }
            _ => false,
        },
    }
}

/// Whether a left click at `cell` completes a double click, and records
/// it for the next one.
fn is_double_click(app: &mut App, cell: (u16, u16)) -> bool {
    const WINDOW: Duration = Duration::from_millis(400);
    let now = std::time::Instant::now();
    let double = app
        .last_click
        .is_some_and(|(at, c)| c == cell && now.duration_since(at) <= WINDOW);
    app.last_click = if double { None } else { Some((now, cell)) };
    double
}

/// While a context menu is open it owns the mouse: a click runs an
/// entry, a click outside closes it, the wheel moves the selection.
fn handle_context_mouse(app: &mut App, m: MouseEvent) -> bool {
    let area = app.last_frame_area();
    match m.kind {
        MouseEventKind::ScrollUp => app.context_move(false),
        MouseEventKind::ScrollDown => app.context_move(true),
        MouseEventKind::Down(_) => match ui::context_menu::hit(app, area, (m.column, m.row)) {
            Some(i) => app.context_run(Some(i)),
            None => app.context_menu = None,
        },
        _ => return false,
    }
    true
}

/// A left click in the right panel: the layer list, else the inspector.
fn click_right_panel(app: &mut App, right: ratatui::layout::Rect, cell: (u16, u16)) {
    if let Some(hit) = ui::panels::layers_hit(app, right, cell) {
        use crate::app::{LayerAction, ScreenAction};
        use ui::panels::LayerHit;
        app.ui.focus = Focus::Layers;
        app.dispatch(match hit {
            LayerHit::ScreenPrev => Action::Screen(ScreenAction::Step(false)),
            LayerHit::ScreenNext => Action::Screen(ScreenAction::Step(true)),
            LayerHit::Screen => Action::Screen(ScreenAction::Rename),
            LayerHit::Activate(id) => Action::ActivateLayer(id),
            LayerHit::Visibility(id) => Action::Layer(LayerAction::ToggleVisible(Some(id))),
            LayerHit::Lock(id) => Action::Layer(LayerAction::ToggleLock(Some(id))),
        });
        return;
    }
    app.ui.focus = Focus::RightPanel;
    if app.design_mode() == super::DesignMode::Interface {
        if let Some(row) = ui::panels::inspector_hit(app, right, cell) {
            app.dispatch(Action::Inspector(InspectorAction::Click(row)));
        }
    }
}

/// While the command palette is open it owns the mouse: a click runs an
/// entry, a click outside closes it, the wheel moves the selection.
fn handle_palette_mouse(app: &mut App, m: MouseEvent) -> bool {
    let area = app.last_frame_area();
    match m.kind {
        MouseEventKind::ScrollUp => app.palette_move(false),
        MouseEventKind::ScrollDown => app.palette_move(true),
        MouseEventKind::Down(MouseButton::Left) => {
            match ui::commands::hit(app, area, (m.column, m.row)) {
                Some(i) => app.palette_run(Some(i)),
                None if !ui::commands::geometry(app, area)
                    .popup
                    .contains((m.column, m.row).into()) =>
                {
                    app.palette = None;
                }
                None => return false,
            }
        }
        _ => return false,
    }
    true
}

/// While the menu is open it owns the mouse: a click chooses an entry, a
/// click outside closes it, the wheel moves the selection.
fn handle_menu_mouse(app: &mut App, m: MouseEvent) -> bool {
    let cell = (m.column, m.row);
    match m.kind {
        MouseEventKind::ScrollUp => app.dispatch(Action::Menu(MenuAction::Move(Direction::Up))),
        MouseEventKind::ScrollDown => app.dispatch(Action::Menu(MenuAction::Move(Direction::Down))),
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right) => {
            let entries = app.menu_entries();
            let geometry = ui::menu::geometry(app.last_frame_area(), &entries);
            let index = ui::menu::hit(&geometry, cell).filter(|i| *i < entries.len());
            match index {
                Some(i) => app.dispatch(Action::Menu(MenuAction::Click(i))),
                None => app.dispatch(Action::Menu(MenuAction::Close)),
            }
        }
        _ => return false,
    }
    true
}

/// While the colour picker is open it owns the mouse: a left click picks for
/// its target, a right click for the other colour, a click outside closes it.
fn handle_picker_mouse(app: &mut App, m: MouseEvent) -> bool {
    let MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Right)) = m.kind else {
        return false;
    };
    let Some(target) = app.picker.as_ref().map(|p| p.target) else {
        return false;
    };
    let rows = app.picker_grid();
    let geometry = ui::color_picker::geometry(app.last_frame_area(), &rows);
    let target = if button == MouseButton::Right {
        target.other()
    } else {
        target
    };
    match ui::color_picker::hit(&geometry, &rows, (m.column, m.row)) {
        Some((r, c)) => app.dispatch(Action::SetColor(target, rows[r][c])),
        None if !geometry.popup.contains((m.column, m.row).into()) => {
            app.dispatch(Action::Cancel);
        }
        None => return false,
    }
    true
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
        assert!(app.prompt.is_some(), "Find lists screens and layers");
        handle_key(&mut app, Key::new(KeyCode::Esc, Modifiers::NONE));
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
    fn painting_tools_turn_letters_into_commands() {
        use crate::app::Tool;
        let mut app = test_app(Size::new(10, 2));
        app.editor.viewport.size = Size::new(10, 2);
        handle_key(&mut app, key('b'));
        assert!(app.dirty(), "the Text tool types 'b'");
        app.dispatch(Action::Undo);
        handle_key(&mut app, Key::new(KeyCode::Char('b'), Modifiers::ALT));
        assert_eq!(app.paint.tool, Tool::Brush, "Alt+B leaves the Text tool");
        handle_key(&mut app, key('%'));
        assert_eq!(
            app.paint.brush.as_str(),
            "%",
            "unbound characters set the brush"
        );
        assert!(!app.dirty());
        app.dispatch(Action::CursorLineStart);
        handle_key(&mut app, key(' '));
        assert_eq!(
            app.cell_at(Position::ORIGIN)
                .and_then(|c| c.as_glyph())
                .map(glyphforge_core::Grapheme::as_str),
            Some("%"),
            "Space paints at the cursor"
        );
        handle_key(&mut app, Key::new(KeyCode::Char('C'), Modifiers::NONE));
        assert_eq!(
            app.picker.as_ref().map(|p| p.target),
            Some(crate::app::ColorTarget::Background)
        );
        handle_key(&mut app, Key::new(KeyCode::Right, Modifiers::NONE));
        handle_key(&mut app, Key::new(KeyCode::Enter, Modifiers::NONE));
        assert_eq!(app.editor.style.bg, glyphforge_core::Color::Indexed(0));
        handle_key(&mut app, key('t'));
        assert_eq!(app.paint.tool, Tool::Text);
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_uses_the_palette_canvas_and_picker() {
        use crate::app::Tool;
        let mut app = test_app(Size::new(20, 6));
        app.update_layout(ratatui::layout::Rect::new(0, 0, 100, 30));
        let layout = ui::layout::compute(app.last_frame_area(), &app.ui);
        let left = layout.left.unwrap();
        let pal = ui::palette::layout(left);
        // Brush is the second entry of the first tool row.
        let down = MouseEventKind::Down(MouseButton::Left);
        handle_mouse(&mut app, mouse(down, pal.tools.x + 11, pal.tools.y + 1));
        assert_eq!(app.paint.tool, Tool::Brush);
        // Page the glyphs with the wheel and the arrows, pick a double line
        // glyph and the double box style.
        let wheel = MouseEventKind::ScrollDown;
        handle_mouse(&mut app, mouse(wheel, pal.brushes.x + 5, pal.brushes.y + 2));
        assert_eq!(app.paint.page, 1);
        handle_mouse(&mut app, mouse(down, pal.brushes.x + 1, pal.brushes.y + 1));
        assert_eq!(app.paint.page, 0, "‹ goes back");
        for _ in 0..10 {
            handle_key(&mut app, key(']'));
        }
        assert_eq!(crate::app::BRUSH_PAGES[app.paint.page].name, "Double ═");
        handle_mouse(&mut app, mouse(down, pal.brushes.x + 5, pal.brushes.y + 2));
        assert_eq!(app.paint.brush.as_str(), "╔");
        handle_mouse(&mut app, mouse(down, pal.tools.x + 5, pal.tools.y + 6));
        assert_eq!(
            app.paint.box_style,
            glyphforge_core::boxdraw::BorderFamily::Double
        );
        assert_eq!(app.paint.tool, Tool::Box);
        handle_mouse(&mut app, mouse(down, pal.tools.x + 11, pal.tools.y + 1));
        // Red as foreground, blue (right click) as background.
        let right = MouseEventKind::Down(MouseButton::Right);
        handle_mouse(&mut app, mouse(down, pal.colors.x + 3, pal.colors.y + 1));
        handle_mouse(&mut app, mouse(right, pal.colors.x + 9, pal.colors.y + 1));
        assert_eq!(app.editor.style.fg, glyphforge_core::Color::Indexed(1));
        assert_eq!(app.editor.style.bg, glyphforge_core::Color::Indexed(4));
        // Paint a stroke on the canvas.
        let (cx, cy) = (layout.canvas.x, layout.canvas.y);
        handle_mouse(&mut app, mouse(down, cx, cy));
        handle_mouse(
            &mut app,
            mouse(MouseEventKind::Drag(MouseButton::Left), cx + 3, cy),
        );
        handle_mouse(
            &mut app,
            mouse(MouseEventKind::Up(MouseButton::Left), cx + 3, cy),
        );
        assert_eq!(app.session.history().undo_len(), 1);
        let painted = app.cell_at(Position::new(3, 0)).unwrap();
        assert_eq!(painted.style.fg, glyphforge_core::Color::Indexed(1));
        // The "+" swatch opens the picker; a click on a swatch picks.
        handle_mouse(&mut app, mouse(down, pal.colors.x + 17, pal.colors.y + 2));
        assert!(app.picker.is_some());
        let rows = app.picker_grid();
        let g = ui::color_picker::geometry(app.last_frame_area(), &rows);
        handle_mouse(&mut app, mouse(down, g.grid_x, g.row_y[1]));
        assert!(app.picker.is_none());
        assert_eq!(app.editor.style.fg, glyphforge_core::Color::Indexed(16));
        // Right click on the canvas picks the painted cell back.
        app.dispatch(Action::ResetColors);
        handle_mouse(&mut app, mouse(right, cx + 1, cy));
        assert_eq!(app.editor.style.fg, glyphforge_core::Color::Indexed(1));
    }

    #[test]
    fn typing_is_ignored_when_a_panel_has_focus() {
        let mut app = test_app(Size::new(10, 2));
        app.ui.focus = Focus::RightPanel;
        handle_key(&mut app, key('x'));
        assert!(!app.dirty());
    }
}
