//! The command palette popup: an input line and the best matches, each
//! with its group and key. Rendering and mouse hit-testing share one
//! geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::App;

/// Most entries shown at once.
const ROWS: u16 = 12;

/// The popup and where its entry rows start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteGeometry {
    pub popup: Rect,
    /// Screen row of the first entry.
    pub first: u16,
    pub rows: u16,
    /// Index of the first entry shown.
    pub start: usize,
}

pub fn geometry(app: &App, area: Rect) -> PaletteGeometry {
    let n = app.palette_matches().len() as u16;
    let rows = n.clamp(1, ROWS).min(area.height.saturating_sub(4));
    let width = 72.min(area.width);
    let height = (rows + 3).min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    let selected = app.palette.as_ref().map_or(0, |p| p.selected);
    let start = selected.saturating_sub(usize::from(rows.max(1)) - 1);
    PaletteGeometry {
        popup,
        first: popup.y + 2,
        rows,
        start,
    }
}

/// The match index under a terminal cell, if any.
pub fn hit(app: &App, area: Rect, cell: (u16, u16)) -> Option<usize> {
    let g = geometry(app, area);
    let inside = g.popup.contains(cell.into()) && cell.1 >= g.first && cell.1 < g.first + g.rows;
    let i = g.start + usize::from(cell.1.checked_sub(g.first)?);
    (inside && i < app.palette_matches().len()).then_some(i)
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(palette) = &app.palette else { return };
    let g = geometry(app, area);
    frame.render_widget(Clear, g.popup);
    let inner_w = usize::from(g.popup.width.saturating_sub(2));
    let mut lines = vec![Line::from(vec![
        Span::styled("› ", styles::accent(app)),
        Span::styled(palette.input.clone(), styles::text(app)),
        Span::styled("▏", styles::accent(app)),
    ])];
    let matches = app.palette_matches();
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            " no matching command",
            styles::muted(app),
        )));
    }
    for (i, c) in matches
        .iter()
        .enumerate()
        .skip(g.start)
        .take(usize::from(g.rows))
    {
        let current = i == palette.selected;
        let right = if c.key.is_empty() {
            c.group.clone()
        } else {
            format!("{}  {}", c.group, c.key)
        };
        let title_w = inner_w.saturating_sub(right.chars().count() + 3);
        let title: String = c.title.chars().take(title_w).collect();
        let (title_style, right_style) = if current {
            (styles::selection(app), styles::selection(app))
        } else {
            (styles::text(app), styles::muted(app))
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {title:<title_w$} "), title_style),
            Span::styled(format!("{right} "), right_style),
        ]));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" Commands · {} ", matches.len()))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), g.popup);
    super::paint_frame(app, frame, g.popup, true);
    let cursor_x = g.popup.x + 3 + palette.input.chars().count() as u16;
    if cursor_x < g.popup.right() {
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, g.popup.y + 1));
    }
}
