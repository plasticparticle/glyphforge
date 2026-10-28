//! The right-click context menu: a small popup at the pointer. Rendering
//! and mouse hit-testing share one geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::App;

/// The popup: placed at the pointer, moved inside the screen.
pub fn geometry(app: &App, area: Rect) -> Option<Rect> {
    let menu = app.context_menu.as_ref()?;
    let widest = menu
        .entries
        .iter()
        .map(|(t, _)| t.chars().count())
        .chain(std::iter::once(menu.title.chars().count() + 2))
        .max()
        .unwrap_or(8) as u16;
    let width = (widest + 4).min(area.width);
    let height = (menu.entries.len() as u16 + 2).min(area.height);
    let x = menu.at.0.min(area.right().saturating_sub(width));
    let y = menu.at.1.min(area.bottom().saturating_sub(height));
    Some(Rect::new(x, y, width, height))
}

/// The entry under a terminal cell.
pub fn hit(app: &App, area: Rect, cell: (u16, u16)) -> Option<usize> {
    let popup = geometry(app, area)?;
    let inner = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    inner
        .contains(cell.into())
        .then(|| usize::from(cell.1 - inner.y))
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let (Some(menu), Some(popup)) = (&app.context_menu, geometry(app, area)) else {
        return;
    };
    frame.render_widget(Clear, popup);
    let width = usize::from(popup.width.saturating_sub(2));
    let lines: Vec<Line<'_>> = menu
        .entries
        .iter()
        .enumerate()
        .map(|(i, (title, _))| {
            let style = if i == menu.selected {
                styles::selection(app)
            } else {
                styles::text(app)
            };
            Line::from(Span::styled(format!(" {title:<w$}", w = width - 1), style))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" {} ", menu.title))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), popup);
    super::paint_frame(app, frame, popup, true);
}
