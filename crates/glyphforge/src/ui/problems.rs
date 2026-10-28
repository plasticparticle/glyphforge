//! The "Check Design" panel, docked at the bottom.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::App;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(problems) = &app.problems else {
        return;
    };
    let height = (problems.items.len() as u16 + 2)
        .clamp(4, 12)
        .min(area.height);
    let popup = Rect::new(
        area.x,
        area.bottom().saturating_sub(height + 1),
        area.width,
        height,
    );
    frame.render_widget(Clear, popup);
    let rows = usize::from(height.saturating_sub(2));
    let start = problems.selected.saturating_sub(rows.saturating_sub(1));
    let lines: Vec<Line<'_>> = problems
        .items
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .map(|(i, p)| {
            let level_style = match p.level {
                "error" => styles::error(app),
                "warning" => styles::warning(app),
                _ => styles::muted(app),
            };
            let text_style = if i == problems.selected {
                styles::selection(app)
            } else {
                styles::text(app)
            };
            let target = p
                .target
                .as_ref()
                .map_or_else(String::new, |t| format!("{t}: "));
            Line::from(vec![
                Span::styled(format!(" {:<10} ", p.level), level_style),
                Span::styled(format!("{target}{} [{}]", p.message, p.code), text_style),
            ])
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" {} ", problems.title))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), popup);
    super::paint_frame(app, frame, popup, true);
}
