//! The theme editor popup: every token with a swatch and its value.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::{App, ThemeTarget, editor_tokens};
use crate::render::to_ratatui_color;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(editor) = &app.theme_editor else {
        return;
    };
    let tokens = editor_tokens();
    let width = 52.min(area.width);
    let height = (tokens.len() as u16 + 4).min(area.height);
    // Docked right so the canvas (and the chrome) stay visible.
    let popup = Rect::new(
        area.right().saturating_sub(width + 1),
        area.y + 1,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let visible = usize::from(height.saturating_sub(4));
    let start = editor.selected.saturating_sub(visible.saturating_sub(1));
    let mut lines: Vec<Line<'_>> = tokens
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, token)| {
            let set = editor.theme.colors.get(*token);
            let color = editor.theme.color(token);
            let current = i == editor.selected;
            let label = if current {
                styles::selection(app)
            } else {
                styles::text(app)
            };
            Line::from(vec![
                Span::styled(
                    "██ ",
                    Style::default().fg(to_ratatui_color(color, app.color_depth)),
                ),
                Span::styled(format!("{token:<22}"), label),
                Span::styled(
                    set.map_or_else(|| "· fallback".to_owned(), ToString::to_string),
                    if set.is_some() {
                        styles::text(app)
                    } else {
                        styles::muted(app)
                    },
                ),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "⏎ type  c picker  n name  s save  Esc cancel",
        styles::muted(app),
    )));
    let what = match editor.target {
        ThemeTarget::Ui => "editor",
        ThemeTarget::Document => "design",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" {} · {what} theme ", editor.theme.name))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), popup);
    super::paint_frame(app, frame, popup, true);
}
