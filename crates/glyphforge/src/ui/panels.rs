//! Side panels: keys/tools on the left, layers and cell properties on the right.

use glyphforge_core::{CellContent, Color, LayerKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

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

    let interface = !app.on_artwork_layer();
    let mode_lines = if interface {
        vec![
            Line::from(vec![
                Span::styled("Select", styles::accent(app)),
                Span::styled(" (interface)", styles::muted(app)),
            ]),
            Line::from(Span::styled("a add  ⏎ edit  del", styles::text(app))),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Type", styles::accent(app)),
                Span::styled(" (artwork)", styles::muted(app)),
            ]),
            Line::from(Span::styled(
                format!("mode {}", app.editor.mode.title()),
                styles::text(app),
            )),
        ]
    };
    frame.render_widget(
        Paragraph::new(mode_lines).block(block(app, "Tool", focused)),
        mode_area,
    );

    let interface_shortcuts: &[(&str, Action)] = &[
        ("next/prev", Action::SelectNext),
        ("find by id", Action::SelectById),
        ("add", Action::AddComponent),
        ("edit prop", Action::EditProperty),
        ("nudge", Action::MoveSelection(Direction::Right)),
        ("resize", Action::ResizeSelection(Direction::Right)),
        ("delete", Action::DeleteSelection),
        ("layer", Action::LayerNext),
        ("undo", Action::Undo),
        ("save", Action::SaveDocument),
        ("quit", Action::Quit),
    ];
    let artwork_shortcuts: &[(&str, Action)] = &[
        ("move", Action::CursorMove(Direction::Right)),
        ("help", Action::ToggleHelp),
        ("left panel", Action::ToggleLeftPanel),
        ("right panel", Action::ToggleRightPanel),
        ("all panels", Action::TogglePanels),
        ("focus", Action::FocusNext),
        ("scroll", Action::ScrollView(Direction::Down)),
        ("layer", Action::LayerNext),
        ("undo", Action::Undo),
        ("save", Action::SaveDocument),
        ("quit", Action::Quit),
    ];
    let shortcuts = if interface {
        interface_shortcuts
    } else {
        artwork_shortcuts
    };
    let lines: Vec<Line<'_>> = shortcuts
        .iter()
        .map(|(label, action)| {
            let keys = app.keymap.chords_for(action);
            let key = match (label, keys.first()) {
                (&"move" | &"nudge", _) => "arrows".to_owned(),
                (&"resize", _) => "shift+arrows".to_owned(),
                (&"next/prev", _) => "] [".to_owned(),
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
    let props_height = if app.on_artwork_layer() { 9 } else { 14 };
    let [layers_area, props_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(props_height)]).areas(area);

    // Layers of the active screen, top-most first.
    let lines: Vec<Line<'_>> = app
        .screen()
        .layers()
        .iter()
        .rev()
        .map(|layer| {
            let active = layer.id == app.editor.layer;
            let marker = if active { "▸" } else { " " };
            let vis = if layer.visible { "●" } else { "○" };
            let lock = if layer.locked { "L" } else { " " };
            let kind = match layer.kind() {
                LayerKind::Artwork => "art",
                LayerKind::Interface => "ui ",
            };
            let style = if active {
                styles::accent(app)
            } else {
                styles::text(app)
            };
            Line::from(vec![
                Span::styled(format!("{marker}{vis}{lock} "), styles::muted(app)),
                Span::styled(format!("{kind} "), styles::muted(app)),
                Span::styled(layer.name.clone(), style),
            ])
        })
        .collect();
    let title = format!("Layers · {}", app.screen().name);
    frame.render_widget(
        Paragraph::new(lines).block(block(app, &title, focused)),
        layers_area,
    );

    if !app.on_artwork_layer() {
        render_inspector(app, frame, props_area, focused);
        return;
    }

    // Properties of the cell under the cursor (active layer).
    let cursor = app.editor.cursor;
    let cell = app.cell_at(cursor);
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

/// The inspector: identity, geometry and properties of the selection.
fn render_inspector(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let row = |k: &str, v: String| {
        Line::from(vec![
            Span::styled(format!("{k:<7}"), styles::muted(app)),
            Span::styled(v, styles::text(app)),
        ])
    };
    let multi = app.selection().len() > 1;
    let lines: Vec<Line<'_>> = match app.primary().cloned() {
        None => vec![
            Line::from(Span::styled("nothing selected", styles::muted(app))),
            Line::from(Span::styled("] [ cycle, click, Ctrl+F", styles::muted(app))),
            Line::from(Span::styled("{ } extend, Ctrl+A all", styles::muted(app))),
            Line::from(Span::styled("a adds a component", styles::muted(app))),
        ],
        Some(_) if multi => {
            let n = app.selection().len();
            let mut lines = vec![Line::from(Span::styled(
                format!("{n} components"),
                styles::accent(app),
            ))];
            if let Some(r) = app.selection_bounds() {
                lines.push(row("x y", format!("{} {}", r.x, r.y)));
                lines.push(row("w h", format!("{} {}", r.width, r.height)));
            }
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "alt+arrows align",
                styles::muted(app),
            )));
            lines.push(Line::from(Span::styled(
                "alt+d distribute",
                styles::muted(app),
            )));
            lines.push(Line::from(Span::styled(
                "alt+w match size",
                styles::muted(app),
            )));
            lines.push(Line::from(Span::styled(
                "alt+p reparent",
                styles::muted(app),
            )));
            for id in app.selection() {
                lines.push(Line::from(Span::styled(
                    format!("  {id}"),
                    styles::text(app),
                )));
            }
            lines
        }
        Some(id) => {
            let Some(c) = app.doc().component(&id) else {
                return;
            };
            let rect = app.cached_layout().and_then(|l| l.rect(&id));
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(c.kind.clone(), styles::accent(app)),
                    Span::styled(format!("  {}", app.placement_text(&id)), styles::muted(app)),
                ]),
                row("id", id.to_string()),
            ];
            if let Some(r) = rect {
                lines.push(row("x y", format!("{} {}", r.x, r.y)));
                lines.push(row("w h", format!("{} {}", r.width, r.height)));
            }
            lines.push(row("width", c.layout.width.to_string()));
            lines.push(row("height", c.layout.height.to_string()));
            if !c.children.is_empty() {
                lines.push(row("kids", c.children.len().to_string()));
            }
            for (k, v) in &c.props {
                let text = v.to_string();
                let max = usize::from(area.width.saturating_sub(10));
                let shown: String = text.chars().take(max).collect();
                lines.push(row(k, shown));
            }
            lines
        }
    };
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Inspector", focused)),
        area,
    );
}
