//! The Esc menu popup. Rendering and mouse hit-testing share one geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::actions::MenuPage;
use crate::app::{App, MenuEntry};
use crate::render::to_ratatui_color;

/// Width of one colour in a theme's preview swatch.
const SWATCH: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuGeometry {
    pub popup: Rect,
    /// Screen row of the first entry.
    pub first_row: u16,
    pub visible: u16,
}

pub fn geometry(area: Rect, entries: &[MenuEntry]) -> MenuGeometry {
    let widest = entries
        .iter()
        .map(|e| e.label.chars().count() + e.hint.chars().count() + e.swatch.len() * SWATCH)
        .max()
        .unwrap_or(0);
    let width = (widest as u16 + 10)
        .clamp(34, area.width.max(10))
        .min(area.width);
    // Border, the entries and the footer.
    let height = (entries.len() as u16 + 4).min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    MenuGeometry {
        popup,
        first_row: popup.y + 1,
        visible: height.saturating_sub(4),
    }
}

/// The entry under a terminal cell.
pub fn hit(g: &MenuGeometry, cell: (u16, u16)) -> Option<usize> {
    let (x, y) = cell;
    let inside = x > g.popup.x && x + 1 < g.popup.right();
    (inside && y >= g.first_row && y < g.first_row + g.visible)
        .then(|| usize::from(y - g.first_row))
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(menu) = &app.menu else { return };
    let entries = app.menu_entries();
    let g = geometry(area, &entries);
    frame.render_widget(Clear, g.popup);

    let text = styles::text(app);
    let muted = styles::muted(app);
    let inner_width = usize::from(g.popup.width.saturating_sub(4));
    let lines: Vec<Line<'_>> = entries
        .iter()
        .take(usize::from(g.visible))
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == menu.selected;
            let style = if selected {
                styles::selection(app)
            } else {
                text
            };
            let hint_style = if selected { style } else { muted };
            let marker = if entry.active { "●" } else { " " };
            let used = entry.label.chars().count()
                + entry.hint.chars().count()
                + entry.swatch.len() * SWATCH
                + 2;
            let mut spans = vec![
                Span::styled(
                    format!("{marker} "),
                    if selected { style } else { styles::accent(app) },
                ),
                Span::styled(entry.label.clone(), style),
                Span::styled(" ".repeat(inner_width.saturating_sub(used)), style),
            ];
            spans.extend(entry.swatch.iter().map(|color| {
                Span::styled(
                    " ".repeat(SWATCH),
                    Style::default().bg(to_ratatui_color(*color, app.color_depth)),
                )
            }));
            spans.push(Span::styled(entry.hint.clone(), hint_style));
            Line::from(spans)
        })
        .collect();

    let title = match menu.page {
        MenuPage::Main => " Menu ",
        MenuPage::Theme => " Theme ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(title)
        .title_style(styles::accent(app).add_modifier(Modifier::BOLD))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(lines).block(block), g.popup);
    super::paint_frame(app, frame, g.popup, true);

    let footer = Rect {
        x: g.popup.x + 2,
        y: g.popup.bottom().saturating_sub(2),
        width: g.popup.width.saturating_sub(4),
        height: 1,
    };
    if footer.y > g.first_row {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                match menu.page {
                    MenuPage::Main => "↑↓ move · ⏎ choose · Esc close",
                    MenuPage::Theme => "↑↓ preview · ⏎ keep · Esc cancel",
                },
                muted,
            ))),
            footer,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{Action, MenuAction};
    use crate::app::test_app;
    use glyphforge_core::Size;

    #[test]
    fn geometry_centres_and_hits_entries() {
        let mut app = test_app(Size::new(10, 3));
        app.dispatch(Action::Menu(MenuAction::Open));
        let entries = app.menu_entries();
        assert_eq!(entries.len(), 7);
        let area = Rect::new(0, 0, 80, 24);
        let g = geometry(area, &entries);
        assert_eq!(g.visible, entries.len() as u16);
        assert_eq!(hit(&g, (g.popup.x + 2, g.first_row)), Some(0));
        assert_eq!(hit(&g, (g.popup.x + 2, g.first_row + 6)), Some(6));
        assert_eq!(hit(&g, (g.popup.x + 2, g.first_row + 7)), None, "footer");
        assert_eq!(hit(&g, (g.popup.x, g.first_row)), None, "border");
        assert_eq!(hit(&g, (0, 0)), None, "outside");
    }
}
