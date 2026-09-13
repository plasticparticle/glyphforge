//! Widgets and layout. Everything here reads `&App` and never mutates it.

pub mod canvas_view;
pub mod header;
pub mod help;
pub mod layout;
pub mod panels;
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
        .fg(to_ratatui_color(app.theme.foreground, depth))
        .bg(to_ratatui_color(app.theme.panel_background, depth));
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
}

/// Style helpers shared by widgets.
pub(crate) mod styles {
    use ratatui::style::{Modifier, Style};

    use crate::app::App;
    use crate::render::to_ratatui_color;

    pub fn text(app: &App) -> Style {
        Style::default()
            .fg(to_ratatui_color(app.theme.foreground, app.color_depth))
            .bg(to_ratatui_color(
                app.theme.panel_background,
                app.color_depth,
            ))
    }

    pub fn muted(app: &App) -> Style {
        text(app).fg(to_ratatui_color(app.theme.muted, app.color_depth))
    }

    pub fn accent(app: &App) -> Style {
        text(app)
            .fg(to_ratatui_color(app.theme.accent, app.color_depth))
            .add_modifier(Modifier::BOLD)
    }

    pub fn border(app: &App, focused: bool) -> Style {
        let c = if focused {
            app.theme.accent
        } else {
            app.theme.border
        };
        text(app).fg(to_ratatui_color(c, app.color_depth))
    }

    pub fn error(app: &App) -> Style {
        text(app)
            .fg(to_ratatui_color(app.theme.error, app.color_depth))
            .add_modifier(Modifier::BOLD)
    }

    pub fn warning(app: &App) -> Style {
        text(app).fg(to_ratatui_color(app.theme.warning, app.color_depth))
    }
}
