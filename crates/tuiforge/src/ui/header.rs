use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::styles;
use crate::app::App;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let title = format!("{}{}", app.title(), if app.dirty { "*" } else { "" });
    let theme = format!("theme: {} ({})", app.theme.name, app.theme.origin);
    let mut spans = vec![
        Span::styled(" TUIForge ", styles::accent(app)),
        Span::styled("│ ", styles::muted(app)),
        Span::styled(title, styles::text(app)),
        Span::styled(" │ ", styles::muted(app)),
        Span::styled(theme, styles::muted(app)),
        Span::styled(" │ ", styles::muted(app)),
        Span::styled(app.color_depth.to_string(), styles::muted(app)),
    ];
    if app.is_omarchy {
        spans.push(Span::styled(" │ Omarchy", styles::muted(app)));
    }
    let hint = "F1 help ";
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(used + hint.len());
    spans.push(Span::styled(" ".repeat(pad), styles::text(app)));
    spans.push(Span::styled(hint, styles::muted(app)));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(styles::text(app)),
        area,
    );
}
