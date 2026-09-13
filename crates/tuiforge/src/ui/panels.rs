//! Side panels: keys/tools on the left, layers and cell properties on the right.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tuiforge_core::{CellContent, Color};

use super::styles;
use crate::actions::{Action, Direction};
use crate::app::{App, Focus};

fn block<'a>(app: &App, title: &'a str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, focused))
        .title(format!(" {title} "))
        .title_style(if focused {
            styles::accent(app)
        } else {
            styles::muted(app)
        })
        .style(styles::text(app))
}

pub fn render_left(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.ui.focus == Focus::LeftPanel;
    let [mode_area, keys_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(3)]).areas(area);

    let mode_lines = vec![
        Line::from(vec![
            Span::styled("Type", styles::accent(app)),
            Span::styled(" (M2: tools)", styles::muted(app)),
        ]),
        Line::from(Span::styled(
            format!("mode {}", app.editor.mode.title()),
            styles::text(app),
        )),
    ];
    frame.render_widget(
        Paragraph::new(mode_lines).block(block(app, "Tool", focused)),
        mode_area,
    );

    let shortcuts: &[(&str, Action)] = &[
        ("move", Action::CursorMove(Direction::Right)),
        ("help", Action::ToggleHelp),
        ("left panel", Action::ToggleLeftPanel),
        ("right panel", Action::ToggleRightPanel),
        ("all panels", Action::TogglePanels),
        ("focus", Action::FocusNext),
        ("scroll", Action::ScrollView(Direction::Down)),
        ("theme", Action::ReloadTheme),
        ("quit", Action::Quit),
    ];
    let lines: Vec<Line<'_>> = shortcuts
        .iter()
        .map(|(label, action)| {
            let keys = app.keymap.chords_for(action);
            let key = match (label, keys.first()) {
                (&"move", _) => "arrows".to_owned(),
                (_, Some(k)) => k.to_string(),
                (_, None) => "unbound".to_owned(),
            };
            Line::from(vec![
                Span::styled(format!("{key:<11}"), styles::accent(app)),
                Span::styled((*label).to_owned(), styles::text(app)),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Keys", focused)),
        keys_area,
    );
}

pub fn render_right(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.ui.focus == Focus::RightPanel;
    let [layers_area, props_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(9)]).areas(area);

    // Layers, top-most first.
    let active = app.doc.active_layer_index();
    let lines: Vec<Line<'_>> = app
        .doc
        .layers()
        .iter()
        .enumerate()
        .rev()
        .map(|(i, layer)| {
            let marker = if i == active { "▸" } else { " " };
            let vis = if layer.visible { "●" } else { "○" };
            let lock = if layer.locked { "L" } else { " " };
            let style = if i == active {
                styles::accent(app)
            } else {
                styles::text(app)
            };
            Line::from(vec![
                Span::styled(format!("{marker}{vis}{lock} "), styles::muted(app)),
                Span::styled(layer.name.clone(), style),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Layers", focused)),
        layers_area,
    );

    // Properties of the cell under the cursor (active layer).
    let cursor = app.editor.cursor;
    let cell = app.doc.cell_at(cursor);
    let (content, codepoints, width) = match cell.map(|c| &c.content) {
        Some(CellContent::Glyph(g)) => (
            g.as_str().to_owned(),
            g.as_str()
                .chars()
                .map(|c| format!("U+{:04X}", c as u32))
                .collect::<Vec<_>>()
                .join(" "),
            g.width().to_string(),
        ),
        Some(CellContent::WideTail) => ("(wide tail)".to_owned(), String::new(), "0".to_owned()),
        _ => ("(empty)".to_owned(), String::new(), "0".to_owned()),
    };
    let style = cell.map(|c| c.style).unwrap_or_default();
    let color_name = |c: Color| match c {
        Color::Default => "default".to_owned(),
        other => other.to_string(),
    };
    let mut attrs = Vec::new();
    if style.attrs.bold {
        attrs.push("bold");
    }
    if style.attrs.dim {
        attrs.push("dim");
    }
    if style.attrs.italic {
        attrs.push("italic");
    }
    if style.attrs.underline {
        attrs.push("underline");
    }
    if style.attrs.strikethrough {
        attrs.push("strike");
    }
    if style.attrs.reverse {
        attrs.push("reverse");
    }
    let row = |k: &str, v: String| {
        Line::from(vec![
            Span::styled(format!("{k:<6}"), styles::muted(app)),
            Span::styled(v, styles::text(app)),
        ])
    };
    let lines = vec![
        row("at", format!("{},{}", cursor.x, cursor.y)),
        row("glyph", content),
        row("code", codepoints),
        row("width", width),
        row("fg", color_name(style.fg)),
        row("bg", color_name(style.bg)),
        row(
            "attrs",
            if attrs.is_empty() {
                "none".to_owned()
            } else {
                attrs.join(",")
            },
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Cell", focused)),
        props_area,
    );
}
