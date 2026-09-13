//! The prompt popup: one input line plus up to eight suggestions.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::App;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(prompt) = &app.prompt else { return };
    let matches = prompt.matches();
    let shown = matches.len().min(8);
    let height = (3 + shown as u16).min(area.height);
    let width = 60.min(area.width);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", styles::accent(app)),
        Span::styled(prompt.input.clone(), styles::text(app)),
        Span::styled("▏", styles::accent(app)),
    ])];
    let window_start = prompt.selected.saturating_sub(shown.saturating_sub(1));
    for (i, m) in matches.iter().enumerate().skip(window_start).take(shown) {
        let style = if i == prompt.selected {
            styles::selection(app)
        } else {
            styles::muted(app)
        };
        lines.push(Line::from(Span::styled(
            format!(" {m:<width$}", width = width as usize - 3),
            style,
        )));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" {} ", prompt.title))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), popup);
    let cursor_x = popup.x + 3 + prompt.input.chars().count() as u16;
    if cursor_x < popup.right() {
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, popup.y + 1));
    }
}
