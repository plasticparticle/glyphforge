//! The tool palette in the left panel on artwork and pixel layers: tools
//! with box styles or pixel modes, the brush, paged brushes (or pixel
//! settings and gradients), colour swatches. Rendering and mouse
//! hit-testing share one geometry, so a click lands on what is drawn.

use glyphforge_core::Color;
use glyphforge_core::boxdraw::BorderFamily;
use glyphforge_core::raster::RasterMode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::styles;
use crate::actions::{Action, Attr, Context, PixelAction};
use crate::app::{
    App, BOX_STYLES, BrushKind, ColorTarget, PaletteEntry, Tool, color_label, is_light,
};
use crate::input::KeyCode;
use crate::render::{to_ratatui_color, to_ratatui_style};

/// Width of one entry in the two-column tool grid.
const TOOL_WIDTH: u16 = 10;
/// Width of a colour swatch.
const SWATCH_WIDTH: u16 = 2;
/// Rows of brush entries below the page line.
const BRUSH_ROWS: u16 = 3;
/// Columns before the colour name in the brush block (` █  fg ██ `).
const BRUSH_GLYPH_WIDTH: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteLayout {
    pub tools: Rect,
    pub brush: Rect,
    pub brushes: Rect,
    pub colors: Rect,
    pub keys: Rect,
}

pub fn layout(area: Rect) -> PaletteLayout {
    let [tools, brush, brushes, colors, keys] = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(4),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Min(0),
    ])
    .areas(area);
    PaletteLayout {
        tools,
        brush,
        brushes,
        colors,
        keys,
    }
}

/// What a click on the palette refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteHit {
    Tool(Tool),
    BoxStyle(BorderFamily),
    PixelMode(RasterMode),
    PixelSize,
    Attr(Attr),
    BrushSize,
    /// An entry of the brushes block.
    Entry(Action),
    PagePrev,
    PageNext,
    Color(Color),
    MoreColors,
    BrushGlyph,
    Foreground,
    Background,
    Swap,
    StampLabel,
}

impl PaletteHit {
    /// The action for a left (or right) click. Right clicks on colours
    /// choose the background.
    pub fn action(self, right: bool) -> Action {
        let target = if right {
            ColorTarget::Background
        } else {
            ColorTarget::Foreground
        };
        match self {
            Self::Tool(t) => Action::SelectTool(t),
            Self::BoxStyle(s) => Action::SetBoxStyle(s),
            Self::PixelMode(m) => Action::Pixel(PixelAction::SetMode(m)),
            Self::PixelSize | Self::BrushSize if right => Action::Pixel(PixelAction::SizeDown),
            Self::PixelSize | Self::BrushSize => Action::Pixel(PixelAction::SizeUp),
            Self::Attr(attr) => Action::ToggleAttr(attr),
            Self::Entry(action) => action,
            Self::PagePrev => Action::BrushPagePrev,
            Self::PageNext => Action::BrushPageNext,
            Self::Color(c) => Action::SetColor(target, c),
            Self::MoreColors => Action::OpenColorPicker(target),
            Self::BrushGlyph => Action::EditBrushGlyph,
            Self::Foreground => Action::OpenColorPicker(ColorTarget::Foreground),
            Self::Background => Action::OpenColorPicker(ColorTarget::Background),
            Self::Swap => Action::SwapColors,
            Self::StampLabel => Action::EditStampLabel,
        }
    }
}

fn inner(r: Rect) -> Rect {
    Rect {
        x: r.x.saturating_add(1),
        y: r.y.saturating_add(1),
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

/// Row and column of `cell` inside the border of `block`, if inside.
fn local(block: Rect, cell: (u16, u16)) -> Option<(u16, u16)> {
    let i = inner(block);
    i.contains(cell.into())
        .then(|| (cell.0 - i.x, cell.1 - i.y))
}

/// The palette element under a terminal cell.
pub fn hit(app: &App, area: Rect, cell: (u16, u16)) -> Option<PaletteHit> {
    if !app.on_artwork_layer() && !app.on_raster_layer() {
        return None;
    }
    let l = layout(area);
    if let Some((x, y)) = local(l.tools, cell) {
        let tool_rows = Tool::ALL.len().div_ceil(2) as u16;
        if y == tool_rows && app.on_raster_layer() {
            return match x / SWATCH_WIDTH {
                i @ 0..=4 => RasterMode::ALL
                    .get(usize::from(i))
                    .copied()
                    .map(PaletteHit::PixelMode),
                _ => Some(PaletteHit::PixelSize),
            };
        }
        if y == tool_rows + 1 && app.on_artwork_layer() {
            return Some(
                Attr::PALETTE
                    .get(usize::from(x / SWATCH_WIDTH))
                    .map_or(PaletteHit::BrushSize, |(attr, _)| PaletteHit::Attr(*attr)),
            );
        }
        if y == tool_rows {
            return BOX_STYLES
                .get(usize::from(x / SWATCH_WIDTH))
                .copied()
                .map(PaletteHit::BoxStyle);
        }
        let index = usize::from(y) * 2 + usize::from(x / TOOL_WIDTH);
        return (x / TOOL_WIDTH < 2)
            .then(|| Tool::ALL.get(index).copied().map(PaletteHit::Tool))
            .flatten();
    }
    if let Some((x, y)) = local(l.brush, cell) {
        return Some(match (y, x < BRUSH_GLYPH_WIDTH) {
            (0, true) if matches!(app.paint.kind, BrushKind::Stamp(_)) => PaletteHit::StampLabel,
            (0, true) => PaletteHit::BrushGlyph,
            (0, false) => PaletteHit::Foreground,
            (_, true) => PaletteHit::Swap,
            (_, false) => PaletteHit::Background,
        });
    }
    if let Some((x, y)) = local(l.brushes, cell) {
        let width = inner(l.brushes).width;
        if y == 0 {
            return Some(if x < 2 {
                PaletteHit::PagePrev
            } else if x + 2 >= width {
                PaletteHit::PageNext
            } else {
                PaletteHit::BrushGlyph
            });
        }
        let page = app.palette_pages().into_iter().nth(app.palette_page())?;
        let samples: Vec<&str> = page.entries.iter().map(|e| e.label.as_str()).collect();
        let index = pack(&samples, width)
            .into_iter()
            .find(|e| e.row + 1 == y && x >= e.x && x < e.x + e.width)?
            .index;
        return page
            .entries
            .into_iter()
            .nth(index)
            .map(|e| PaletteHit::Entry(e.action));
    }
    if let Some((x, y)) = local(l.colors, cell) {
        let col = x / SWATCH_WIDTH;
        return match (y, col) {
            (0, 0..=7) => Some(PaletteHit::Color(Color::Indexed(col as u8))),
            (0, 8) => Some(PaletteHit::Color(Color::Default)),
            (1, 0..=7) => Some(PaletteHit::Color(Color::Indexed(8 + col as u8))),
            (1, 8) => Some(PaletteHit::MoreColors),
            (2, _) => app
                .paint
                .recent
                .get(usize::from(col))
                .copied()
                .map(PaletteHit::Color),
            _ => None,
        };
    }
    None
}

/// Where a brush entry sits in the brushes block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub index: usize,
    pub row: u16,
    pub x: u16,
    pub width: u16,
}

/// Lays entries out left to right, one space apart, wrapping at `width`.
pub fn pack(samples: &[&str], width: u16) -> Vec<Placed> {
    let (mut row, mut x) = (0, 0);
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let w: u16 = glyphforge_core::Grapheme::split_text(sample)
                .iter()
                .map(|g| u16::from(g.width()))
                .sum();
            if x > 0 && x + w > width {
                row += 1;
                x = 0;
            }
            let placed = Placed {
                index,
                row,
                x,
                width: w,
            };
            x += w + 1;
            placed
        })
        .collect()
}

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

/// A colour swatch of `SWATCH_WIDTH` cells; `mark` is drawn on top in a
/// contrasting colour.
fn swatch(app: &App, color: Color, mark: &str) -> Span<'static> {
    if color == Color::Default {
        let text = if mark.trim().is_empty() {
            "∅ ".to_owned()
        } else {
            format!("{mark:<2}")
        };
        return Span::styled(text, styles::muted(app).add_modifier(Modifier::BOLD));
    }
    let contrast = if is_light(color) {
        Color::Indexed(0)
    } else {
        Color::Indexed(15)
    };
    Span::styled(
        format!("{mark:<2}"),
        Style::default()
            .bg(to_ratatui_color(color, app.color_depth))
            .fg(to_ratatui_color(contrast, app.color_depth))
            .add_modifier(Modifier::BOLD),
    )
}

/// `f`/`b` marks on swatches showing the current colours.
fn color_mark(app: &App, color: Color) -> &'static str {
    let s = app.editor.style;
    match (s.fg == color, s.bg == color) {
        (true, true) => "fb",
        (true, false) => "f",
        (false, true) => " b",
        (false, false) => "",
    }
}

/// The key that selects `action` in the current context: the bare letter
/// while painting, otherwise the Alt chord.
fn key_label(app: &App, action: &Action) -> String {
    let painting = app.key_context() == Context::Paint;
    let chords = app.keymap.chords_for(action);
    let bare = chords
        .iter()
        .find(|c| !c.mods.ctrl && !c.mods.alt && matches!(c.code, KeyCode::Char(_)));
    let alt = chords.iter().find(|c| c.mods.alt);
    let chord = if painting { bare.or(alt) } else { alt.or(bare) };
    chord.map_or_else(String::new, |c| match (c.code, c.mods.shift, painting) {
        (KeyCode::Char(' '), ..) => c.to_string(),
        (KeyCode::Char(ch), false, true) => ch.to_string(),
        (KeyCode::Char(ch), true, true) => ch.to_uppercase().to_string(),
        _ => c.to_string(),
    })
}

/// The single-letter paint key of `action`, or a blank.
fn bare_key(app: &App, action: &Action) -> String {
    app.keymap
        .chords_for(action)
        .iter()
        .find_map(|c| match c.code {
            KeyCode::Char(ch) if c.mods.is_none() => Some(ch.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| " ".to_owned())
}

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let l = layout(area);
    render_tools(app, frame, l.tools, focused);
    render_brush(app, frame, l.brush, focused);
    render_brushes(app, frame, l.brushes, focused);
    render_colors(app, frame, l.colors, focused);
    render_keys(app, frame, l.keys, focused);
}

/// Tools, two columns, then the box styles.
fn render_tools(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let text = styles::text(app);
    let muted = styles::muted(app);
    let mut tool_lines: Vec<Line<'_>> = Tool::ALL
        .chunks(2)
        .map(|pair| {
            let spans = pair
                .iter()
                .flat_map(|&tool| {
                    let active = app.paint.tool == tool;
                    let key = bare_key(app, &Action::SelectTool(tool));
                    let style = if active { styles::selection(app) } else { text };
                    let key_style = if active { style } else { styles::accent(app) };
                    [
                        Span::styled(key, key_style),
                        Span::styled(format!(" {:<8}", tool.title()), style),
                    ]
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();
    if let Some(raster) = app.raster() {
        let mut modes: Vec<Span<'_>> = RasterMode::ALL
            .iter()
            .map(|&mode| {
                let style = if raster.mode == mode {
                    styles::selection(app)
                } else {
                    text
                };
                Span::styled(format!("{} ", mode.sample()), style)
            })
            .collect();
        modes.push(Span::styled(" size ", muted));
        modes.push(Span::styled(
            app.paint.pixel_size.to_string(),
            styles::accent(app),
        ));
        tool_lines.push(Line::from(modes));
    } else {
        let mut styles_line: Vec<Span<'_>> = BOX_STYLES
            .iter()
            .map(|&family| {
                let style = if app.paint.box_style == family {
                    styles::selection(app)
                } else {
                    text
                };
                Span::styled(format!("{} ", family.glyphs().top_left), style)
            })
            .collect();
        styles_line.push(Span::styled(
            format!(" {}", app.paint.box_style.name()),
            muted,
        ));
        tool_lines.push(Line::from(styles_line));
        // Attributes of the brush style, then the brush size.
        let mut attrs = app.editor.style.attrs;
        let mut attr_line: Vec<Span<'_>> = Attr::PALETTE
            .iter()
            .map(|&(attr, mark)| {
                let style = if *attr.flag(&mut attrs) {
                    styles::selection(app)
                } else {
                    muted
                };
                Span::styled(format!("{mark} "), style)
            })
            .collect();
        attr_line.push(Span::styled(" size ", muted));
        attr_line.push(Span::styled(
            app.paint.size.to_string(),
            styles::accent(app),
        ));
        tool_lines.push(Line::from(attr_line));
    }
    let title = format!("Tools · {}", app.paint.tool.title());
    frame.render_widget(
        Paragraph::new(tool_lines).block(block(app, &title, focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

/// Brush: glyph in its colours, fg and bg.
fn render_brush(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let text = styles::text(app);
    let muted = styles::muted(app);
    let style = app.editor.style;
    let name_width = usize::from(inner(area).width.saturating_sub(10));
    let short = |c: Color| color_label(c).chars().take(name_width).collect::<String>();
    let brush_lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {} ", app.brush_sample()),
                to_ratatui_style(style, app.color_depth),
            ),
            Span::styled(" fg ", muted),
            swatch(app, style.fg, ""),
            Span::styled(format!(" {}", short(style.fg)), text),
        ]),
        Line::from(vec![
            Span::styled(" ⇄ ", styles::accent(app)),
            Span::styled(" bg ", muted),
            swatch(app, style.bg, ""),
            Span::styled(format!(" {}", short(style.bg)), text),
        ]),
    ];
    let title = match app.paint.kind {
        BrushKind::Glyph => "Brush".to_owned(),
        BrushKind::Stamp(_) => format!("{} \"{}\"", app.brush_name(), app.paint.label),
        _ => app.brush_name(),
    };
    let title: String = title
        .chars()
        .take(usize::from(area.width.saturating_sub(4)))
        .collect();
    frame.render_widget(
        Paragraph::new(brush_lines).block(block(app, &title, focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

/// Brushes: page line, then the page's entries.
fn render_brushes(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let text = styles::text(app);
    let muted = styles::muted(app);
    let width = inner(area).width;
    let pages = app.palette_pages();
    let page_index = app.palette_page();
    let Some(page) = pages.get(page_index) else {
        return;
    };
    let counter = format!("{}/{}", page_index + 1, pages.len());
    let name_width = usize::from(width.saturating_sub(4)).saturating_sub(counter.len() + 1);
    let name: String = page.name.chars().take(name_width).collect();
    let mut lines = vec![Line::from(vec![
        Span::styled("‹ ", styles::accent(app)),
        Span::styled(format!("{name:<name_width$} "), text),
        Span::styled(counter, muted),
        Span::styled(" ›", styles::accent(app)),
    ])];
    let samples: Vec<&str> = page.entries.iter().map(|e| e.label.as_str()).collect();
    let mut rows: Vec<Vec<Span<'_>>> = vec![Vec::new(); usize::from(BRUSH_ROWS)];
    let mut cursor = vec![0u16; usize::from(BRUSH_ROWS)];
    for placed in pack(&samples, width) {
        let Some(row) = rows.get_mut(usize::from(placed.row)) else {
            break;
        };
        let at = &mut cursor[usize::from(placed.row)];
        if placed.x > *at {
            row.push(Span::styled(" ".repeat(usize::from(placed.x - *at)), text));
        }
        row.extend(entry_spans(app, &page.entries[placed.index]));
        *at = placed.x + placed.width;
    }
    lines.extend(rows.into_iter().map(Line::from));
    let title = if app.on_raster_layer() {
        "Pixels"
    } else {
        "Brushes"
    };
    frame.render_widget(
        Paragraph::new(lines).block(block(app, title, focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

/// An entry's label, with its swatch colours behind the first characters.
fn entry_spans(app: &App, entry: &PaletteEntry) -> Vec<Span<'static>> {
    let base = if entry.active {
        styles::selection(app)
    } else {
        styles::text(app)
    };
    let graphemes = glyphforge_core::Grapheme::split_text(&entry.label);
    graphemes
        .iter()
        .enumerate()
        .map(|(i, g)| match entry.swatch.get(i) {
            Some(color) => Span::styled(
                g.as_str().to_owned(),
                Style::default()
                    .bg(to_ratatui_color(*color, app.color_depth))
                    .fg(to_ratatui_color(color.contrast(), app.color_depth)),
            ),
            None => Span::styled(g.as_str().to_owned(), base),
        })
        .collect()
}

/// Colours: ANSI 16, default, more, recent.
fn render_colors(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let muted = styles::muted(app);
    let base_row = |start: u8, last: Span<'static>| {
        let mut spans: Vec<Span<'_>> = (start..start + 8)
            .map(|i| swatch(app, Color::Indexed(i), color_mark(app, Color::Indexed(i))))
            .collect();
        spans.push(last);
        Line::from(spans)
    };
    let recent = if app.paint.recent.is_empty() {
        Line::from(Span::styled("right-click: bg", muted))
    } else {
        Line::from(
            app.paint
                .recent
                .iter()
                .map(|&c| swatch(app, c, color_mark(app, c)))
                .collect::<Vec<_>>(),
        )
    };
    let color_lines = vec![
        base_row(
            0,
            swatch(app, Color::Default, color_mark(app, Color::Default)),
        ),
        base_row(8, Span::styled("+ ", styles::accent(app))),
        recent,
    ];
    frame.render_widget(
        Paragraph::new(color_lines).block(block(app, "Colours", focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

/// Keys for the active tool.
fn render_keys(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let text = styles::text(app);
    let muted = styles::muted(app);
    let row = |key: String, label: &str| {
        Line::from(vec![
            Span::styled(format!("{key:<7}"), styles::accent(app)),
            Span::styled(label.to_owned(), text),
        ])
    };
    let mut lines = vec![Line::from(Span::styled(app.paint.tool.hint(), muted))];
    if app.on_raster_layer() {
        lines.push(row("⇧arrow".to_owned(), "move by pixel"));
        lines.push(row(
            key_label(app, &Action::Pixel(PixelAction::Settings)),
            "layer settings",
        ));
        lines.push(row(
            key_label(app, &Action::Pixel(PixelAction::CycleMode)),
            "pixel mode",
        ));
        lines.push(row(
            key_label(app, &Action::Pixel(PixelAction::CycleDither)),
            "dithering",
        ));
        lines.push(row(
            key_label(app, &Action::Pixel(PixelAction::ToggleShadow)),
            "drop shadow",
        ));
        lines.push(row(
            key_label(app, &Action::Pixel(PixelAction::EditGradient)),
            "edit gradient",
        ));
    }
    if app.paint.tool == Tool::Text {
        lines.push(row("Alt+key".to_owned(), "choose tool"));
    } else {
        lines.push(row(key_label(app, &Action::ApplyTool), "apply here"));
    }
    lines.push(row(
        key_label(app, &Action::OpenColorPicker(ColorTarget::Foreground)),
        "fg colour",
    ));
    lines.push(row(
        key_label(app, &Action::OpenColorPicker(ColorTarget::Background)),
        "bg colour",
    ));
    lines.push(row(key_label(app, &Action::SwapColors), "swap colours"));
    lines.push(row(key_label(app, &Action::EditBrushGlyph), "search brush"));
    if matches!(app.paint.kind, BrushKind::Stamp(_)) {
        lines.push(row(key_label(app, &Action::EditStampLabel), "stamp label"));
    }
    let pages = format!(
        "{} {}",
        key_label(app, &Action::BrushPagePrev),
        key_label(app, &Action::BrushPageNext)
    );
    lines.push(row(pages, "brush page"));
    lines.push(row(key_label(app, &Action::CycleBoxStyle), "box style"));
    lines.push(row("rclick".to_owned(), "pick from canvas"));
    for (label, action) in [("undo", Action::Undo), ("layer", Action::LayerNext)] {
        let key = app
            .keymap
            .chords_for(&action)
            .first()
            .map_or_else(String::new, ToString::to_string);
        lines.push(row(key, label));
    }
    frame.render_widget(
        Paragraph::new(lines).block(block(app, "Keys", focused)),
        area,
    );
    super::paint_frame(app, frame, area, focused);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{BRUSH_PAGES, test_app};
    use glyphforge_core::Size;

    fn pages_fit(app: &App) {
        let l = layout(Rect::new(0, 1, crate::ui::layout::LEFT_PANEL_WIDTH, 40));
        let width = inner(l.brushes).width;
        for page in app.palette_pages() {
            let samples: Vec<&str> = page.entries.iter().map(|e| e.label.as_str()).collect();
            let placed = pack(&samples, width);
            assert!(
                placed
                    .iter()
                    .all(|p| p.row < BRUSH_ROWS && p.x + p.width <= width),
                "{} does not fit: {placed:?}",
                page.name
            );
        }
    }

    #[test]
    fn every_page_fits_the_brushes_block() {
        let mut app = test_app(Size::new(10, 3));
        pages_fit(&app);
        app.dispatch(Action::SelectTool(Tool::Gradient));
        assert!(app.on_raster_layer());
        pages_fit(&app);
        app.dispatch(Action::Pixel(PixelAction::ToggleShadow));
        pages_fit(&app);
    }

    #[test]
    fn hits_match_the_drawn_grid() {
        let app = test_app(Size::new(10, 3));
        let area = Rect::new(0, 1, 22, 32);
        let l = layout(area);
        let tools = inner(l.tools);
        assert_eq!(
            hit(&app, area, (tools.x, tools.y)),
            Some(PaletteHit::Tool(Tool::Text))
        );
        assert_eq!(
            hit(&app, area, (tools.x + TOOL_WIDTH, tools.y)),
            Some(PaletteHit::Tool(Tool::Brush))
        );
        assert_eq!(
            hit(&app, area, (tools.x + TOOL_WIDTH + 3, tools.y + 3)),
            Some(PaletteHit::Tool(Tool::Picker))
        );
        assert_eq!(
            hit(&app, area, (tools.x + 2, tools.y + 4)),
            Some(PaletteHit::Tool(Tool::Gradient))
        );
        assert_eq!(
            hit(&app, area, (tools.x + 4, tools.y + 5)),
            Some(PaletteHit::BoxStyle(BorderFamily::Double))
        );
        let brush = inner(l.brush);
        assert_eq!(
            hit(&app, area, (brush.x + 1, brush.y)),
            Some(PaletteHit::BrushGlyph)
        );
        assert_eq!(
            hit(&app, area, (brush.x + 6, brush.y)),
            Some(PaletteHit::Foreground)
        );
        assert_eq!(
            hit(&app, area, (brush.x + 6, brush.y + 1)),
            Some(PaletteHit::Background)
        );
        let brushes = inner(l.brushes);
        assert!(matches!(
            hit(&app, area, (brushes.x + 3, brushes.y + 1)),
            Some(PaletteHit::Entry(Action::SetBrushKind(BrushKind::Line(_))))
        ));
        assert_eq!(hit(&app, area, (brushes.x + 8, brushes.y + 1)), None, "gap");
        assert_eq!(
            hit(&app, area, (brushes.x, brushes.y)),
            Some(PaletteHit::PagePrev)
        );
        assert_eq!(
            hit(&app, area, (brushes.x + brushes.width - 1, brushes.y)),
            Some(PaletteHit::PageNext)
        );
        let mut paged = test_app(Size::new(10, 3));
        paged.paint.page = BRUSH_PAGES
            .iter()
            .position(|p| p.name == "Double ═")
            .unwrap();
        assert_eq!(
            hit(&paged, area, (brushes.x + 4, brushes.y + 1)),
            Some(PaletteHit::Entry(Action::SetBrushGlyph("╔".into())))
        );
        assert_eq!(
            hit(&paged, area, (brushes.x, brushes.y + 3)),
            None,
            "short page"
        );
        let colors = inner(l.colors);
        assert_eq!(
            hit(&app, area, (colors.x + 2, colors.y)),
            Some(PaletteHit::Color(Color::Indexed(1)))
        );
        assert_eq!(
            hit(&app, area, (colors.x + 16, colors.y)),
            Some(PaletteHit::Color(Color::Default))
        );
        assert_eq!(
            hit(&app, area, (colors.x + 15, colors.y + 1)),
            Some(PaletteHit::Color(Color::Indexed(15)))
        );
        assert_eq!(
            hit(&app, area, (colors.x + 17, colors.y + 1)),
            Some(PaletteHit::MoreColors)
        );
        assert_eq!(
            hit(&app, area, (colors.x, colors.y + 2)),
            None,
            "no recent colours yet"
        );
        assert_eq!(hit(&app, area, (area.x, area.y)), None, "borders are inert");
        assert_eq!(
            PaletteHit::Color(Color::Indexed(4)).action(true),
            Action::SetColor(ColorTarget::Background, Color::Indexed(4))
        );
    }

    #[test]
    fn pixel_palette_hits_modes_settings_and_gradients() {
        let mut app = test_app(Size::new(10, 3));
        app.dispatch(Action::SelectTool(Tool::Gradient));
        let area = Rect::new(0, 1, 22, 32);
        let l = layout(area);
        let tools = inner(l.tools);
        assert_eq!(
            hit(&app, area, (tools.x + 4, tools.y + 5)),
            Some(PaletteHit::PixelMode(RasterMode::Sextant))
        );
        assert_eq!(
            hit(&app, area, (tools.x + 12, tools.y + 5)),
            Some(PaletteHit::PixelSize)
        );
        let brushes = inner(l.brushes);
        assert_eq!(
            hit(&app, area, (brushes.x + 9, brushes.y + 1)),
            Some(PaletteHit::Entry(Action::Pixel(PixelAction::SetDither(
                glyphforge_core::raster::Dither::Ordered
            ))))
        );
        let gradients = app
            .palette_pages()
            .iter()
            .position(|p| p.name.starts_with("Gradients"))
            .unwrap();
        app.paint.pixel_page = gradients;
        let Some(PaletteHit::Entry(action)) = hit(&app, area, (brushes.x + 11, brushes.y + 1))
        else {
            panic!("second gradient entry");
        };
        app.dispatch(action);
        assert_eq!(app.current_gradient().unwrap().name, "Ocean");
    }
}
