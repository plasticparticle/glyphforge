//! Help overlay: all bindable actions with their current keys.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::actions::{Category, descriptors};
use crate::app::App;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    let categories = [
        Category::Application,
        Category::File,
        Category::Edit,
        Category::View,
        Category::Cursor,
    ];
    for cat in categories {
        lines.push(Line::from(Span::styled(
            cat.title().to_owned(),
            styles::accent(app),
        )));
        for d in descriptors().iter().filter(|d| d.category == cat) {
            let keys = app.keymap.chords_for(&d.action);
            let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
            let keys = if keys.is_empty() {
                "unbound".to_owned()
            } else {
                keys.join(", ")
            };
            let title_style = if d.implemented {
                styles::text(app)
            } else {
                styles::muted(app)
            };
            let suffix = if d.implemented {
                ""
            } else {
                "  (later milestone)"
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {keys:<22}"), styles::text(app)),
                Span::styled(format!("{}{suffix}", d.title), title_style),
            ]));
        }
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled(
        "Esc, F1 or q closes this help.",
        styles::muted(app),
    )));

    let height = (lines.len() as u16 + 2).min(area.height);
    let width = 64.min(area.width);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(" Keys ")
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
