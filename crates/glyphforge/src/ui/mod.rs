//! Widgets and layout. Everything here reads `&App` and never mutates it.

pub mod canvas_view;
pub mod header;
pub mod help;
pub mod layout;
pub mod panels;
pub mod prompt;
pub mod status_bar;

use ratatui::Frame;
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::App;
use crate::render::to_ratatui_color;

pub fn render(app: &App, frame: &mut Frame<'_>) {
    let area = frame.area();
    let layout = layout::compute(area, &app.ui);
    let depth = app.color_depth;

    // Paint the panel background everywhere first.
    let base = Style::default()
        .fg(to_ratatui_color(app.ui_theme.color("foreground"), depth))
        .bg(to_ratatui_color(app.ui_theme.color("surface"), depth));
    frame.render_widget(Block::default().style(base), area);

    header::render(app, frame, layout.header);
    if let Some(left) = layout.left {
        panels::render_left(app, frame, left);
    }
    canvas_view::render(app, frame, layout.canvas_block, layout.canvas);
    if let Some(right) = layout.right {
        panels::render_right(app, frame, right);
    }
    status_bar::render(app, frame, layout.status);
    if app.ui.help_open {
        help::render(app, frame, area);
    }
    prompt::render(app, frame, area);
}

/// Style helpers shared by widgets. Every colour comes from a UI theme
/// token, never from a literal.
pub(crate) mod styles {
    use ratatui::style::{Modifier, Style};

    use crate::app::App;
    use crate::render::to_ratatui_color;

    fn tok(app: &App, token: &str) -> ratatui::style::Color {
        to_ratatui_color(app.ui_theme.color(token), app.color_depth)
    }

    pub fn text(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "foreground"))
            .bg(tok(app, "surface"))
    }

    pub fn muted(app: &App) -> Style {
        text(app).fg(tok(app, "foreground-muted"))
    }

    pub fn accent(app: &App) -> Style {
        text(app)
            .fg(tok(app, "primary"))
            .add_modifier(Modifier::BOLD)
    }

    pub fn border(app: &App, focused: bool) -> Style {
        text(app).fg(tok(app, if focused { "focus" } else { "border-muted" }))
    }

    pub fn error(app: &App) -> Style {
        text(app).fg(tok(app, "error")).add_modifier(Modifier::BOLD)
    }

    pub fn warning(app: &App) -> Style {
        text(app).fg(tok(app, "warning"))
    }

    pub fn selection(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "selection-foreground"))
            .bg(tok(app, "selection"))
    }

    /// Snap guides: visible but quieter than the selection.
    pub fn guide(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "primary"))
            .bg(tok(app, "background"))
            .add_modifier(Modifier::DIM)
    }

    pub fn paper(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "foreground"))
            .bg(tok(app, "background"))
    }
}
