//! Side panels: keys/tools on the left, layers and cell properties on the right.

use glyphforge_core::{CellContent, Color, LayerKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::styles;
use crate::actions::{Action, Direction};
use crate::app::{App, Focus, Tool};

fn block<'a>(app: &App, title: &'a str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, focused))
        .title(format!(" {title} "))
        .title_style(if focused {
            styles::accent(app)
        } else {
            styles::text(app)
        })
        .style(styles::text(app))
}

pub fn render_left(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.ui.focus == Focus::LeftPanel;
    if app.on_artwork_layer() || app.on_raster_layer() {
        super::palette::render(app, frame, area, focused);
        return;
    }
    let [mode_area, keys_area] =
        Layout::vertical([Constraint::Length(5), Constraint::Min(3)]).areas(area);

    let brush_key = app
        .keymap
        .chords_for(&Action::SelectTool(Tool::Brush))
        .into_iter()
        .find(|c| c.mods.alt)
        .map_or_else(|| "Ctrl+PgUp".to_owned(), |c| c.to_string());
    let mode_lines = vec![
        Line::from(vec![
            Span::styled("Select", styles::accent(app)),
            Span::styled(" (interface)", styles::muted(app)),
        ]),
        Line::from(Span::styled("a add  ⏎ edit  del", styles::text(app))),
        Line::from(vec![
            Span::styled(brush_key, styles::accent(app)),
            Span::styled(" paint tools", styles::muted(app)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(mode_lines).block(block(app, "Tool", focused)),
        mode_area,
    );
    super::paint_frame(app, frame, mode_area, focused);

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
    let shortcuts = interface_shortcuts;
    let lines: Vec<Line<'_>> = shortcuts
        .iter()
        .map(|(label, action)| {
            let keys = app.keymap.chords_for(action);
            let key = match (label, keys.first()) {
                (&"nudge", _) => "arrows".to_owned(),
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
    super::paint_frame(app, frame, keys_area, focused);
}

/// The right panel's layer list and property block. In Interface Mode the
/// inspector takes what the layer list does not need.
pub fn right_areas(app: &App, area: Rect) -> [Rect; 2] {
    if app.on_artwork_layer() || app.on_raster_layer() {
        let props = if app.on_artwork_layer() { 9 } else { 10 };
        return Layout::vertical([Constraint::Min(4), Constraint::Length(props)]).areas(area);
    }
    let layers = (app.screen().layers().len() as u16 + 3).clamp(5, (area.height / 3).max(5));
    Layout::vertical([Constraint::Length(layers), Constraint::Min(3)]).areas(area)
}

/// Where the inspector's rows are: the first visible row index, the
/// screen row of the first row and how many rows fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectorGeometry {
    pub start: usize,
    pub top: u16,
    pub visible: usize,
    pub area: Rect,
}

/// Lines above the rows: kind and placement, id.
const INSPECTOR_HEADER: u16 = 2;

pub fn inspector_geometry(app: &App, props_area: Rect) -> InspectorGeometry {
    let inner = Rect::new(
        props_area.x + 1,
        props_area.y + 1,
        props_area.width.saturating_sub(2),
        props_area.height.saturating_sub(2),
    );
    let visible = usize::from(inner.height.saturating_sub(INSPECTOR_HEADER)).max(1);
    let row = app.inspector_row();
    let start = row.saturating_sub(visible - 1);
    InspectorGeometry {
        start,
        top: inner.y + INSPECTOR_HEADER,
        visible,
        area: inner,
    }
}

/// The inspector row at a terminal cell, if any.
pub fn inspector_hit(app: &App, right: Rect, cell: (u16, u16)) -> Option<usize> {
    let [_, props] = right_areas(app, right);
    let g = inspector_geometry(app, props);
    if !g.area.contains(cell.into()) || cell.1 < g.top {
        return None;
    }
    let row = g.start + usize::from(cell.1 - g.top);
    (row < app.inspector_fields().len()).then_some(row)
}

/// What a click in the layer list refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerHit {
    ScreenPrev,
    ScreenNext,
    Screen,
    Activate(glyphforge_core::ObjectId),
    Visibility(glyphforge_core::ObjectId),
    Lock(glyphforge_core::ObjectId),
}

/// The layer list element under a terminal cell. Row 0 is the screen
/// switcher, then the layers top-most first; column 1 is the eye, column 2
/// the lock.
pub fn layers_hit(app: &App, right: Rect, cell: (u16, u16)) -> Option<LayerHit> {
    let [layers, _] = right_areas(app, right);
    let inner = Rect::new(
        layers.x + 1,
        layers.y + 1,
        layers.width.saturating_sub(2),
        layers.height.saturating_sub(2),
    );
    if !inner.contains(cell.into()) {
        return None;
    }
    let (x, y) = (cell.0 - inner.x, cell.1 - inner.y);
    if y == 0 {
        return Some(if x < 2 {
            LayerHit::ScreenPrev
        } else if x + 2 >= inner.width {
            LayerHit::ScreenNext
        } else {
            LayerHit::Screen
        });
    }
    let layer = app.screen().layers().iter().rev().nth(usize::from(y - 1))?;
    let id = layer.id.clone();
    Some(match x {
        1 => LayerHit::Visibility(id),
        2 => LayerHit::Lock(id),
        _ => LayerHit::Activate(id),
    })
}

pub fn render_right(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.ui.focus == Focus::RightPanel;
    let layers_focused = app.ui.focus == Focus::Layers;
    let [layers_area, props_area] = right_areas(app, area);

    // The screen switcher, then the layers of the active screen, top-most
    // first.
    let screens = app.doc().screens();
    let index = screens
        .iter()
        .position(|s| s.id == app.editor.screen)
        .unwrap_or(0);
    let width = usize::from(layers_area.width.saturating_sub(2));
    let name = format!("{} {}/{}", app.screen().name, index + 1, screens.len());
    let mut lines: Vec<Line<'_>> = vec![Line::from(vec![
        Span::styled("◂ ", styles::accent(app)),
        Span::styled(
            format!("{name:^w$}", w = width.saturating_sub(4)),
            styles::text(app),
        ),
        Span::styled(" ▸", styles::accent(app)),
    ])];
    lines.extend(app.screen().layers().iter().rev().map(|layer| {
        let active = layer.id == app.editor.layer;
        let marker = if active { "▸" } else { " " };
        let vis = if layer.visible { "●" } else { "○" };
        let lock = if layer.locked { "L" } else { " " };
        let kind = match layer.kind() {
            LayerKind::Artwork => "art",
            LayerKind::Interface => "ui ",
            LayerKind::Raster => "px ",
        };
        let style = if active && layers_focused {
            styles::selection(app)
        } else if active {
            styles::accent(app)
        } else {
            styles::text(app)
        };
        Line::from(vec![
            Span::styled(marker, styles::accent(app)),
            Span::styled(vis, styles::muted(app)),
            Span::styled(format!("{lock} "), styles::warning(app)),
            Span::styled(format!("{kind} "), styles::muted(app)),
            Span::styled(layer.name.clone(), style),
        ])
    }));
    let title = if layers_focused {
        "Layers · ␣ show l lock s solo n new d dup m merge"
    } else {
        "Screens · Layers"
    };
    frame.render_widget(
        Paragraph::new(lines).block(block(app, title, layers_focused)),
        layers_area,
    );
    super::paint_frame(app, frame, layers_area, layers_focused);

    if app.on_raster_layer() {
        render_pixel_info(app, frame, props_area, focused);
        return;
    }
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
    super::paint_frame(app, frame, props_area, focused);
}

/// Pixel layer settings and the pixel under the pixel cursor.
fn render_pixel_info(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let Some(raster) = app.raster() else { return };
    let row = |k: &str, v: String| {
        Line::from(vec![
            Span::styled(format!("{k:<7}"), styles::muted(app)),
            Span::styled(v, styles::text(app)),
        ])
    };
    let p = app.pixel_cursor();
    let pixel = raster.pixels.get(p.x, p.y).unwrap_or_default();
    let (cw, ch) = raster.mode.cell_px();
    let lines = vec![
        row("pixel", format!("{},{} ({}×{}/cell)", p.x, p.y, cw, ch)),
        row("value", pixel.to_string()),
        row(
            "mode",
            format!("{} {}", raster.mode.sample(), raster.mode.name()),
        ),
        row("dither", raster.dither.name().to_owned()),
        row("colors", raster.palette.name().to_owned()),
        row(
            "shadow",
            raster.shadow.map_or_else(
                || "off".to_owned(),
                |s| format!("{},{} soft {} {}%", s.dx, s.dy, s.softness, s.opacity),
            ),
        ),
        row("brush", format!("{} px", app.paint.pixel_size)),
        row(
            "mouse",
            if app.pixel_mouse.is_some() {
                "pixel-precise"
            } else {
                "per cell"
            }
            .to_owned(),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Pixel layer", focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

/// The inspector: identity and an editable row per layout field and
/// property of the selection.
fn render_inspector(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let Some(id) = app.selection().cloned() else {
        let lines = vec![
            Line::from(Span::styled("nothing selected", styles::muted(app))),
            Line::from(Span::styled("] [ cycle, click, Ctrl+F", styles::muted(app))),
            Line::from(Span::styled("a adds a component", styles::muted(app))),
            Line::from(Span::styled("shift+click selects more", styles::muted(app))),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(block(app, "Inspector", focused)),
            area,
        );
        super::paint_frame(app, frame, area, focused);
        return;
    };
    let Some(c) = app.doc().component(&id) else {
        return;
    };
    let count = app.selected_ids().len();
    let rect = app.cached_layout().and_then(|l| l.rect(&id));
    let mut lines = vec![
        Line::from(vec![
            Span::styled(c.kind.clone(), styles::accent(app)),
            Span::styled(
                rect.map_or_else(String::new, |r| {
                    format!("  {}×{} at {},{}", r.width, r.height, r.x, r.y)
                }),
                styles::muted(app),
            ),
        ]),
        Line::from(vec![
            Span::styled(id.to_string(), styles::text(app)),
            Span::styled(
                if count > 1 {
                    format!("  +{} selected", count - 1)
                } else if c.children.is_empty() {
                    String::new()
                } else {
                    format!("  {} children", c.children.len())
                },
                styles::muted(app),
            ),
        ]),
    ];
    let g = inspector_geometry(app, area);
    let fields = app.inspector_fields();
    let selected = app.inspector_row();
    let label_w = 8;
    let value_w = usize::from(g.area.width).saturating_sub(label_w);
    let mut last_group = fields.get(g.start.wrapping_sub(1)).map(|f| f.group);
    for (i, f) in fields.iter().enumerate().skip(g.start).take(g.visible) {
        let current = i == selected && focused;
        let new_group = last_group != Some(f.group);
        last_group = Some(f.group);
        let label_style = if current {
            styles::selection(app)
        } else if new_group {
            styles::accent(app)
        } else {
            styles::muted(app)
        };
        let (value, value_style) = if f.value.is_empty() {
            (
                f.default
                    .as_ref()
                    .map_or_else(|| "·".to_owned(), |d| format!("· {d}")),
                styles::muted(app),
            )
        } else {
            (f.value.clone(), styles::text(app))
        };
        let swatch_w = if f.swatch { 3 } else { 0 };
        let mut value: String = value
            .chars()
            .take(value_w.saturating_sub(2 + swatch_w))
            .collect();
        if f.cyclic && current {
            value = format!("‹{value}›");
        }
        let mut spans = vec![Span::styled(format!("{:<label_w$}", f.label), label_style)];
        if f.swatch {
            // The colour the document theme resolves the value (or the
            // default) to.
            let token = if f.value.is_empty() {
                f.default.as_deref()
            } else {
                Some(f.value.as_str())
            };
            if let Some(token) = token {
                let color = app.doc().theme.resolve(token);
                spans.push(Span::styled(
                    "██",
                    ratatui::style::Style::default()
                        .fg(crate::render::to_ratatui_color(color, app.color_depth)),
                ));
                spans.push(Span::raw(" "));
            }
        }
        spans.push(Span::styled(
            value,
            if current {
                styles::selection(app)
            } else {
                value_style
            },
        ));
        lines.push(Line::from(spans));
    }
    let title = if focused {
        "Inspector · ⏎ edit ␣ cycle del reset"
    } else {
        "Inspector"
    };
    frame.render_widget(
        Paragraph::new(lines).block(block(app, title, focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}
