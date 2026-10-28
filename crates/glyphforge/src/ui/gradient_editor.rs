//! The gradient editor popup: the gradient as a bar, its stops as markers
//! below it, the selected stop's colour and position. Rendering and mouse
//! hit-testing share one geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::styles;
use crate::app::App;
use crate::render::to_ratatui_color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorGeometry {
    pub popup: Rect,
    /// The bar and the marker row below it: where a press picks a stop.
    pub track: Rect,
}

pub fn geometry(app: &App, area: Rect) -> Option<EditorGeometry> {
    app.gradient_editor.as_ref()?;
    let width = 64.min(area.width);
    let height = 8.min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    );
    let track = Rect::new(popup.x + 2, popup.y + 1, width.saturating_sub(4), 2);
    Some(EditorGeometry { popup, track })
}

/// The position along the gradient (0..=100) under a terminal column.
pub fn percent_at(g: &EditorGeometry, column: u16) -> u8 {
    let span = g.track.width.saturating_sub(1).max(1);
    let x = column.saturating_sub(g.track.x).min(span);
    (u32::from(x) * 100 / u32::from(span)) as u8
}

fn column_of(g: &EditorGeometry, at: u8) -> u16 {
    let span = g.track.width.saturating_sub(1).max(1);
    g.track.x + (u32::from(at) * u32::from(span) / 100) as u16
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let (Some(editor), Some(g)) = (&app.gradient_editor, geometry(app, area)) else {
        return;
    };
    frame.render_widget(Clear, g.popup);
    let gradient = &editor.gradient;
    let space = match gradient.space {
        glyphforge_core::raster::ColorSpace::Rgb => "rgb",
        glyphforge_core::raster::ColorSpace::Oklab => "oklab",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, true))
        .title(format!(" Gradient · {} · {space} ", gradient.name))
        .title_style(styles::accent(app))
        .style(styles::text(app));
    let selected = gradient.stops.get(editor.selected);
    let info = selected.map_or_else(String::new, |s| {
        format!(
            " stop {}/{}  {}  at {}%",
            editor.selected + 1,
            gradient.stops.len(),
            s.color,
            s.at
        )
    });
    let lines = vec![
        Line::raw(""),
        Line::raw(""),
        Line::from(Span::styled(info, styles::text(app))),
        Line::from(Span::styled(
            " ←→ stop  ⇧←→ move  < > by 10  ⏎ colour  a add  x remove",
            styles::muted(app),
        )),
        Line::from(Span::styled(
            " o rgb/oklab  r reverse  t as text  s save  Esc cancel",
            styles::muted(app),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), g.popup);
    super::paint_frame(app, frame, g.popup, true);
    let depth = app.color_depth;
    let buf = frame.buffer_mut();
    let span = g.track.width.saturating_sub(1).max(1);
    for i in 0..g.track.width {
        let t = f32::from(i) / f32::from(span);
        let color = to_ratatui_color(gradient.sample(t), depth);
        if let Some(cell) = buf.cell_mut((g.track.x + i, g.track.y)) {
            cell.set_symbol("█").set_style(Style::default().fg(color));
        }
    }
    for (i, stop) in gradient.stops.iter().enumerate() {
        let x = column_of(&g, stop.at);
        let style = if i == editor.selected {
            styles::selection(app)
        } else {
            styles::accent(app)
        };
        if let Some(cell) = buf.cell_mut((x, g.track.y + 1)) {
            cell.set_symbol("▲").set_style(style);
        }
    }
}
