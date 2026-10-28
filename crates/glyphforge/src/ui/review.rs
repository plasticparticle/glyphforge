//! The review panel for agent changes: the list on the left, the
//! highlighted entry's operations on the right, docked at the bottom so
//! the canvas above shows the previewed result.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::{App, ReviewItem};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(review) = &app.review else { return };
    let height = (review.items.len() as u16 + 4)
        .clamp(6, 14)
        .min(area.height);
    let popup = Rect::new(
        area.x,
        area.bottom().saturating_sub(height + 1),
        area.width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(" Agent changes · ⏎ accept  x reject / revert  ↑↓ choose  Esc close ")
        .title_style(styles::accent(app))
        .style(styles::text(app));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    super::paint_frame(app, frame, popup, true);
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Min(10)]).areas(inner);
    let width = usize::from(list.width);
    let lines: Vec<Line<'_>> = review
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let title: String = item.title().chars().take(width.saturating_sub(1)).collect();
            let style = if i == review.selected {
                styles::selection(app)
            } else if matches!(item, ReviewItem::Pending { .. }) {
                styles::text(app)
            } else {
                styles::muted(app)
            };
            Line::from(Span::styled(format!("{title:<width$}"), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), list);
    let Some(item) = review.items.get(review.selected) else {
        return;
    };
    let state = match item {
        ReviewItem::Pending { .. } if review.preview.is_some() => "pending · previewed above",
        ReviewItem::Pending { .. } => "pending · does not apply to this document",
        ReviewItem::Applied { .. } => "applied",
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {} ", item.agent()), styles::accent(app)),
        Span::styled(state, styles::muted(app)),
    ])];
    lines.extend(
        item.changes()
            .into_iter()
            .map(|c| Line::from(Span::styled(format!("  {c}"), styles::text(app)))),
    );
    frame.render_widget(Paragraph::new(lines), detail);
}
