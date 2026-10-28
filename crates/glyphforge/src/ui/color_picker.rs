//! The colour picker popup: every ANSI 256 colour, the terminal default and
//! recent colours. Rendering and mouse hit-testing share one geometry.

use glyphforge_core::Color;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::{App, ColorTarget, color_label, is_light, picker_rows};
use crate::render::to_ratatui_color;

/// Where the picker and each of its swatch rows are on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerGeometry {
    pub popup: Rect,
    /// Left edge of the swatch grid.
    pub grid_x: u16,
    /// Screen row of each swatch row; rows that do not fit are omitted.
    pub row_y: Vec<u16>,
    pub swatch: u16,
    pub footer_y: u16,
}

/// Rows followed by a blank separator: the base colours, the last cube row
/// and everything after it.
const fn separated(row: usize) -> bool {
    row == 0 || row >= 6
}

pub fn geometry(area: Rect, rows: &[Vec<Color>]) -> PickerGeometry {
    let widest = rows.iter().map(Vec::len).max().unwrap_or(0) as u16;
    let swatch = if area.width >= widest * 2 + 4 { 2 } else { 1 };
    let width = (widest * swatch + 4).max(44).min(area.width);
    // Header line and a blank before the grid.
    let mut offsets = Vec::with_capacity(rows.len());
    let mut y = 2u16;
    for i in 0..rows.len() {
        offsets.push(y);
        y += if separated(i) { 2 } else { 1 };
    }
    let height = (y + 1 + 2).min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let inner_bottom = popup.bottom().saturating_sub(1);
    let row_y = offsets
        .into_iter()
        .map(|o| popup.y + 1 + o)
        .take_while(|y| *y < inner_bottom)
        .collect();
    PickerGeometry {
        popup,
        grid_x: popup.x + 2,
        row_y,
        swatch,
        footer_y: popup.y + 1 + y,
    }
}

/// Row and column of the swatch under a terminal cell.
pub fn hit(g: &PickerGeometry, rows: &[Vec<Color>], cell: (u16, u16)) -> Option<(usize, usize)> {
    let (x, y) = cell;
    let r = g.row_y.iter().position(|ry| *ry == y)?;
    let col = usize::from(x.checked_sub(g.grid_x)? / g.swatch);
    let inside = x < g.popup.right().saturating_sub(1);
    (inside && col < rows[r].len()).then_some((r, col))
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let Some(picker) = &app.picker else { return };
    let rows = picker_rows(&app.paint.recent);
    let g = geometry(area, &rows);
    frame.render_widget(Clear, g.popup);

    let style = app.editor.style;
    let target_label = |t: ColorTarget, color: Color| {
        let active = picker.target == t;
        let label_style = if active {
            styles::accent(app).add_modifier(Modifier::UNDERLINED)
        } else {
            styles::muted(app)
        };
        let short = if t == ColorTarget::Foreground {
            "fg"
        } else {
            "bg"
        };
        vec![
            Span::styled(format!("{short} "), label_style),
            Span::styled(
                "  ",
                Style::default().bg(to_ratatui_color(color, app.color_depth)),
            ),
            Span::styled(format!(" {:<16}", color_label(color)), styles::text(app)),
        ]
    };
    let mut header = target_label(ColorTarget::Foreground, style.fg);
    header.extend(target_label(ColorTarget::Background, style.bg));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" Choose {} colour ", picker.target.title()))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    frame.render_widget(Paragraph::new(Line::from(header)).block(block), g.popup);
    super::paint_frame(app, frame, g.popup, true);

    let depth = app.color_depth;
    let right_limit = g.popup.right().saturating_sub(1);
    let buf = frame.buffer_mut();
    for (r, &y) in g.row_y.iter().enumerate() {
        for (c, &color) in rows[r].iter().enumerate() {
            let x0 = g.grid_x + c as u16 * g.swatch;
            let selected = picker.row == r && picker.col == c;
            let marks: [&str; 2] = match (selected, g.swatch) {
                (true, 2) => ["[", "]"],
                (true, _) => ["◆", ""],
                (false, _) if color == Color::Default => ["∅", " "],
                _ => [" ", " "],
            };
            let cell_style = if color == Color::Default {
                if selected {
                    styles::selection(app)
                } else {
                    styles::muted(app)
                }
            } else {
                let contrast = if is_light(color) {
                    Color::Indexed(0)
                } else {
                    Color::Indexed(15)
                };
                Style::default()
                    .bg(to_ratatui_color(color, depth))
                    .fg(to_ratatui_color(contrast, depth))
                    .add_modifier(Modifier::BOLD)
            };
            for (i, mark) in marks.iter().take(usize::from(g.swatch)).enumerate() {
                let x = x0 + i as u16;
                if x >= right_limit {
                    break;
                }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(if mark.is_empty() { " " } else { mark })
                        .set_style(cell_style);
                }
            }
        }
    }

    if g.footer_y < g.popup.bottom().saturating_sub(1) {
        let current = rows
            .get(picker.row)
            .and_then(|r| r.get(picker.col))
            .map_or_else(String::new, |c| color_label(*c));
        let footer = Line::from(vec![
            Span::styled(format!("{current:<14}"), styles::accent(app)),
            Span::styled(
                "⏎ pick  Tab fg/bg  right-click other  # hex  d default  Esc",
                styles::muted(app),
            ),
        ]);
        let footer_area = Rect {
            x: g.grid_x,
            y: g.footer_y,
            width: right_limit.saturating_sub(g.grid_x),
            height: 1,
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_fits_and_hits_swatches() {
        let rows = picker_rows(&[]);
        let area = Rect::new(0, 0, 120, 40);
        let g = geometry(area, &rows);
        assert_eq!(g.swatch, 2);
        assert_eq!(g.row_y.len(), rows.len());
        assert!(g.popup.width <= 120 && g.popup.height <= 40);
        assert_eq!(hit(&g, &rows, (g.grid_x, g.row_y[0])), Some((0, 0)));
        assert_eq!(hit(&g, &rows, (g.grid_x + 5, g.row_y[0])), Some((0, 2)));
        assert_eq!(hit(&g, &rows, (g.grid_x + 71, g.row_y[1])), Some((1, 35)));
        assert_eq!(
            hit(&g, &rows, (g.grid_x + 60, g.row_y[0])),
            None,
            "row 0 is short"
        );
        assert_eq!(
            hit(&g, &rows, (g.grid_x, g.row_y[0] + 1)),
            None,
            "separator"
        );
        assert_eq!(rows[1][35], Color::Indexed(51));

        let narrow = geometry(Rect::new(0, 0, 50, 10), &rows);
        assert_eq!(narrow.swatch, 1);
        assert!(narrow.row_y.iter().all(|y| *y < narrow.popup.bottom() - 1));
    }
}
