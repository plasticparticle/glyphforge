use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::styles;
use crate::app::{App, StatusKind};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let c = app.editor.cursor;
    let size = app.doc_size();
    let vp = app.editor.viewport;
    let layer_text = app.active_layer().map_or_else(
        || "no layer".to_owned(),
        |l| {
            let idx = app.screen().layer_index(&l.id).map_or(0, |i| i + 1);
            format!(
                " {} [{}] ({}/{}) ",
                l.name,
                l.kind().name(),
                idx,
                app.screen().layers().len()
            )
        },
    );
    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.editor.mode.title()),
            styles::accent(app),
        ),
        Span::styled("│", styles::muted(app)),
        Span::styled(format!(" x:{} y:{} ", c.x, c.y), styles::text(app)),
        Span::styled("│", styles::muted(app)),
        Span::styled(
            format!(" {}×{} ", size.width, size.height),
            styles::text(app),
        ),
        Span::styled("│", styles::muted(app)),
        Span::styled(layer_text, styles::text(app)),
        Span::styled("│", styles::muted(app)),
        Span::styled(
            format!(" focus: {} ", app.ui.focus.title()),
            styles::muted(app),
        ),
    ];
    if vp.size.width < size.width || vp.size.height < size.height {
        spans.push(Span::styled("│", styles::muted(app)));
        spans.push(Span::styled(
            format!(" view +{},+{} ", vp.offset.x, vp.offset.y),
            styles::muted(app),
        ));
    }
    if let Some(status) = &app.status {
        spans.push(Span::styled("│ ", styles::muted(app)));
        let style = match status.kind {
            StatusKind::Info => styles::text(app),
            StatusKind::Warning => styles::warning(app),
            StatusKind::Error => styles::error(app),
        };
        spans.push(Span::styled(status.text.clone(), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(styles::text(app)),
        area,
    );
}
