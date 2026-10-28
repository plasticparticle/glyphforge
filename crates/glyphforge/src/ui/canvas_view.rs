//! The canvas: the document seen through the viewport.

use ratatui::Frame;
use ratatui::layout::{Position as RPosition, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::{Block, BorderType, Borders};

use glyphforge_core::Position;
use glyphforge_core::align::Guide;

use super::styles;
use crate::app::{App, Focus};
use crate::render::{draw_cells, to_ratatui_style};

pub fn render(app: &App, frame: &mut Frame<'_>, block_area: Rect, canvas_area: Rect) {
    let focused = app.ui.focus == Focus::Canvas;
    let size = app.doc_size();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(styles::border(app, focused))
        .title(format!(" {} {}×{} ", app.title(), size.width, size.height))
        .title_style(if focused {
            styles::accent(app)
        } else {
            styles::muted(app)
        })
        .style(styles::text(app));
    frame.render_widget(block, block_area);
    super::paint_frame(app, frame, block_area, focused);
    if canvas_area.width == 0 || canvas_area.height == 0 {
        return;
    }

    let depth = app.canvas_depth();
    let paper = styles::paper(app);
    let outside = styles::muted(app).add_modifier(Modifier::DIM);

    let viewport = app.editor.viewport.rect();
    let buf = frame.buffer_mut();
    // Area beyond the document edge.
    for y in canvas_area.top()..canvas_area.bottom() {
        for x in canvas_area.left()..canvas_area.right() {
            let dx = u32::from(viewport.x) + u32::from(x - canvas_area.x);
            let dy = u32::from(viewport.y) + u32::from(y - canvas_area.y);
            if dx >= u32::from(size.width) || dy >= u32::from(size.height) {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("·").set_style(outside);
                }
            }
        }
    }
    // Compositing happens once per edit; the cache is filled by `App::composite`.
    // Rendering takes `&App`, so we recompute here only if the cache is cold.
    if let Some(preview) = app.pixel_preview() {
        draw_cells(&preview, viewport, canvas_area, buf, depth, paper);
    } else if let Some(cells) = app.cached_composite() {
        draw_cells(cells, viewport, canvas_area, buf, depth, paper);
    } else if let Ok(cells) = app.session.render(&app.editor.screen, None) {
        draw_cells(&cells, viewport, canvas_area, buf, depth, paper);
    }

    draw_overlays(app, buf, canvas_area);

    if focused && !app.ui.help_open && app.prompt.is_none() && app.menu.is_none() {
        let c = app.editor.cursor;
        if viewport.contains(c) {
            let sx = canvas_area.x + (c.x - viewport.x);
            let sy = canvas_area.y + (c.y - viewport.y);
            frame.set_cursor_position(RPosition::new(sx, sy));
        }
    }
}

/// Everything drawn over the design: shape and move previews, the cell
/// marquee, component selection with handles, the rubber band, snap
/// guides and the minimap.
fn draw_overlays(app: &App, buf: &mut ratatui::buffer::Buffer, canvas_area: Rect) {
    let depth = app.canvas_depth();
    let paper = styles::paper(app);
    let viewport = app.editor.viewport.rect();
    // Shape preview: the line, rectangle or box that Space or releasing the
    // mouse button will draw.
    if let Some(anchor) = app.paint.anchor {
        for w in app.shape_writes(anchor, app.editor.cursor) {
            let pos = glyphforge_core::Position::new(w.x, w.y);
            let Some(glyph) = w.cell.as_glyph() else {
                continue;
            };
            if !viewport.contains(pos) {
                continue;
            }
            let sx = canvas_area.x + (pos.x - viewport.x);
            let sy = canvas_area.y + (pos.y - viewport.y);
            if let Some(cell) = buf.cell_mut((sx, sy)) {
                cell.set_symbol(glyph.as_str())
                    .set_style(to_ratatui_style(w.cell.style, depth));
            }
        }
    }

    // Selection overlay: highlight each selected component's outline and
    // mark the primary selection's bottom-right resize handle.
    let primary = app.selection().cloned();
    for id in app.selected_ids() {
        let Some(rect) = app.cached_layout().and_then(|l| l.rect(&id)) else {
            continue;
        };
        let is_primary = primary.as_ref() == Some(&id);
        let handles = crate::app::resize_handles(rect);
        let corners: Vec<Position> = handles.iter().take(4).copied().collect();
        let sel = if is_primary {
            styles::selection(app)
        } else {
            styles::accent(app)
        };
        for pos in rect.positions() {
            let on_edge = pos.x == rect.x
                || pos.y == rect.y
                || u32::from(pos.x) + 1 == rect.right()
                || u32::from(pos.y) + 1 == rect.bottom();
            if !on_edge || !viewport.contains(pos) {
                continue;
            }
            let sx = canvas_area.x + (pos.x - viewport.x);
            let sy = canvas_area.y + (pos.y - viewport.y);
            if let Some(cell) = buf.cell_mut((sx, sy)) {
                cell.set_style(sel);
                if is_primary && handles.contains(&pos) {
                    cell.set_symbol(if corners.contains(&pos) { "◆" } else { "◇" });
                }
            }
        }
    }

    // Cells being dragged by the Select tool: the source is blank, the
    // cells are drawn at their new place.
    let put = |buf: &mut ratatui::buffer::Buffer, pos: Position, symbol: &str, style| {
        if viewport.contains(pos) {
            let sx = canvas_area.x + (pos.x - viewport.x);
            let sy = canvas_area.y + (pos.y - viewport.y);
            if let Some(cell) = buf.cell_mut((sx, sy)) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    };
    if let Some((from, writes)) = app.move_preview() {
        for pos in from.positions() {
            put(buf, pos, " ", paper);
        }
        for w in writes {
            if let Some(g) = w.cell.as_glyph() {
                put(
                    buf,
                    Position::new(w.x, w.y),
                    g.as_str(),
                    to_ratatui_style(w.cell.style, depth),
                );
            }
        }
    }
    // The selection marquee: its edge cells keep their glyphs and take the
    // selection colours.
    if let Some(rect) = app.marquee_preview() {
        let sel = styles::selection(app);
        for pos in rect.positions() {
            let edge = pos.x == rect.x
                || pos.y == rect.y
                || u32::from(pos.x) + 1 == rect.right()
                || u32::from(pos.y) + 1 == rect.bottom();
            if edge && viewport.contains(pos) {
                let sx = canvas_area.x + (pos.x - viewport.x);
                let sy = canvas_area.y + (pos.y - viewport.y);
                if let Some(cell) = buf.cell_mut((sx, sy)) {
                    cell.set_style(sel);
                }
            }
        }
    }

    // Rubber band: its outline in the accent colour.
    if let Some(band) = app.band_rect() {
        let style = styles::accent(app);
        for pos in band.positions() {
            let edge = pos.x == band.x
                || pos.y == band.y
                || u32::from(pos.x) + 1 == band.right()
                || u32::from(pos.y) + 1 == band.bottom();
            if edge {
                let symbol = if pos.y == band.y || u32::from(pos.y) + 1 == band.bottom() {
                    "┈"
                } else {
                    "┊"
                };
                put(buf, pos, symbol, style);
            }
        }
    }

    // Snap guides while dragging: dotted lines in the accent colour.
    let guide_style = styles::accent(app);
    for guide in app.active_guides() {
        let (cells, symbol): (Vec<Position>, &str) = match *guide {
            Guide::Column { x, from, to } => {
                ((from..=to).map(|y| Position::new(x, y)).collect(), "┊")
            }
            Guide::Row { y, from, to } => ((from..=to).map(|x| Position::new(x, y)).collect(), "┈"),
        };
        for pos in cells.into_iter().filter(|p| viewport.contains(*p)) {
            let sx = canvas_area.x + (pos.x - viewport.x);
            let sy = canvas_area.y + (pos.y - viewport.y);
            if let Some(cell) = buf.cell_mut((sx, sy)) {
                cell.set_symbol(symbol).set_style(guide_style);
            }
        }
    }

    if let Some(m) = minimap_geometry(app, canvas_area) {
        draw_minimap(app, buf, &m);
    }
}

/// Where the minimap sits (bottom-right of the canvas) and how many
/// document cells each of its cells covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapGeometry {
    pub area: Rect,
    pub scale_x: u16,
    pub scale_y: u16,
}

pub fn minimap_geometry(app: &App, canvas_area: Rect) -> Option<MinimapGeometry> {
    if !app.ui.minimap || canvas_area.width < 12 || canvas_area.height < 6 {
        return None;
    }
    let doc = app.doc_size();
    let max_w = (canvas_area.width / 3).clamp(4, 32);
    let max_h = (canvas_area.height / 3).clamp(2, 12);
    let scale_x = doc.width.div_ceil(max_w).max(1);
    let scale_y = doc.height.div_ceil(max_h).max(1);
    let w = doc.width.div_ceil(scale_x);
    let h = doc.height.div_ceil(scale_y);
    Some(MinimapGeometry {
        area: Rect::new(
            canvas_area.right() - w - 1,
            canvas_area.bottom() - h - 1,
            w,
            h,
        ),
        scale_x,
        scale_y,
    })
}

/// The document position a click on the minimap points at.
pub fn minimap_hit(app: &App, canvas_area: Rect, cell: (u16, u16)) -> Option<Position> {
    let m = minimap_geometry(app, canvas_area)?;
    m.area.contains(cell.into()).then(|| {
        Position::new(
            (cell.0 - m.area.x) * m.scale_x,
            (cell.1 - m.area.y) * m.scale_y,
        )
    })
}

/// Each minimap cell shows the first visible glyph of its block in its
/// colour (or a dot); the viewport is outlined in the selection colour.
fn draw_minimap(app: &App, buf: &mut ratatui::buffer::Buffer, m: &MinimapGeometry) {
    let Some(composite) = app.cached_composite() else {
        return;
    };
    let depth = app.canvas_depth();
    let viewport = app.editor.viewport.rect();
    let paper = styles::paper(app);
    for my in 0..m.area.height {
        for mx in 0..m.area.width {
            let block =
                glyphforge_core::Rect::new(mx * m.scale_x, my * m.scale_y, m.scale_x, m.scale_y);
            let sample = block.positions().find_map(|p| {
                composite
                    .get(p)
                    .filter(|c| c.as_glyph().is_some_and(|g| g.as_str() != " "))
            });
            let (symbol, mut style) = match sample {
                Some(c) => ("▪", to_ratatui_style(c.style, depth)),
                None => ("·", paper),
            };
            if !block.intersection(viewport).is_empty() {
                style = style.patch(styles::selection(app));
            }
            if let Some(cell) = buf.cell_mut((m.area.x + mx, m.area.y + my)) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glyphforge_core::{Position, Size};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::actions::Action;
    use crate::app::test_app;

    fn screen(app: &mut crate::app::App, w: u16, h: u16) -> Vec<String> {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        app.update_layout(ratatui::layout::Rect::new(0, 0, w, h));
        terminal.draw(|f| crate::ui::render(app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn renders_typed_text_inside_the_canvas_border() {
        let mut app = test_app(Size::new(10, 3));
        app.dispatch(Action::TogglePanels); // hide panels: canvas fills the width
        app.dispatch(Action::InsertText("Hi 漢".into()));
        let rows = screen(&mut app, 30, 8);
        assert!(rows[1].starts_with('╭'), "top border: {:?}", rows[1]);
        let content: String = rows[2].chars().take(5).collect();
        assert_eq!(content, "│Hi 漢", "content row: {:?}", rows[2]);
        // Beyond the 10-column document the filler pattern appears.
        assert!(rows[2].contains('·'));
        assert!(rows[7].contains("INSERT"), "status bar: {:?}", rows[7]);
        assert!(
            rows[7].contains("x:5 y:0"),
            "cursor after 'Hi 漢' is at column 5: {:?}",
            rows[7]
        );
        assert!(rows[0].contains("Glyphforge"));
    }

    #[test]
    fn viewport_scrolls_to_keep_cursor_visible() {
        let mut app = test_app(Size::new(200, 100));
        app.dispatch(Action::TogglePanels);
        app.dispatch(Action::CursorTo(Position::new(150, 60)));
        app.dispatch(Action::InsertText("Z".into()));
        let rows = screen(&mut app, 100, 10);
        // Canvas area is 98x6 (border). The cursor sits at 151,60 after typing.
        let vp = app.editor.viewport;
        assert_eq!(vp.size, Size::new(98, 6));
        assert_eq!(vp.offset, Position::new(151 - 98 + 1, 60 - 6 + 1));
        let status = rows.last().unwrap();
        assert!(
            status.contains("view +"),
            "status shows the scroll offset: {status:?}"
        );
        // 'Z' is drawn somewhere in the content rows.
        assert!(rows.iter().any(|r| r.contains('Z')));
    }

    #[test]
    fn panels_render_layer_list_and_palette() {
        let mut app = test_app(Size::new(10, 3));
        let rows = screen(&mut app, 100, 30);
        let all = rows.join("\n");
        assert!(all.contains("Artwork"), "{all}");
        assert!(all.contains("UI"), "{all}");
        assert!(all.contains("Layers"), "{all}");
        for block in ["Tools · Text", "Brush", "Brushes", "Colours", "Keys"] {
            assert!(all.contains(block), "{block}: {all}");
        }
        assert!(all.contains("b Brush"), "tool keys shown: {all}");
        assert!(all.contains("┼ single ╭ rounded"), "brush palette: {all}");
        assert!(
            all.contains("‹ Lines ┼") && all.contains("1/22 ›"),
            "page line: {all}"
        );
    }

    #[test]
    fn colour_picker_and_shape_preview_render() {
        use crate::app::{ColorTarget, Tool};
        let mut app = test_app(Size::new(20, 6));
        app.dispatch(Action::SelectTool(Tool::Rect));
        app.dispatch(Action::SetBrushGlyph("@".into()));
        app.dispatch(Action::ToolPress(Position::new(1, 1)));
        app.dispatch(Action::ToolDrag(Position::new(4, 3)));
        let rows = screen(&mut app, 120, 30);
        let all = rows.join("\n");
        assert!(all.contains("@@@@"), "rectangle previewed: {all}");
        assert!(!app.dirty());
        assert!(all.contains("SUBCELL RECT"), "{all}");
        app.dispatch(Action::ToolRelease);

        app.dispatch(Action::OpenColorPicker(ColorTarget::Background));
        let rows = screen(&mut app, 120, 30);
        let all = rows.join("\n");
        assert!(all.contains("Choose background colour"), "{all}");
        assert!(all.contains("Tab fg/bg"), "{all}");
        assert!(all.contains("[]"), "selected swatch marked: {all}");
    }

    #[test]
    fn help_overlay_lists_bindings() {
        let mut app = test_app(Size::new(10, 3));
        app.dispatch(Action::ToggleHelp);
        let rows = screen(&mut app, 100, 30);
        let all = rows.join("\n");
        assert!(all.contains("Ctrl+Q"), "{all}");
        assert!(all.contains("Quit"), "{all}");
    }

    #[test]
    fn components_render_inside_the_canvas() {
        use glyphforge_core::layout::Layout;
        use glyphforge_core::{Component, ObjectId};
        let mut app = test_app(Size::new(20, 5));
        app.dispatch(Action::TogglePanels);
        let panel = Component::new(ObjectId::slugify("metrics"), "panel")
            .with_prop("title", "Metrics")
            .with_layout(Layout::absolute(0, 0, 12, 3));
        app.session
            .create_component(panel, None, None, None)
            .unwrap();
        app.mark_edited();
        let rows = screen(&mut app, 30, 8);
        assert!(rows[2].contains("╭ Metrics ─╮"), "{:?}", rows[2]);
    }

    #[test]
    fn selection_overlay_and_inspector_render() {
        use glyphforge_core::layout::Layout;
        use glyphforge_core::{Component, ObjectId};
        let mut app = test_app(Size::new(30, 8));
        let panel = Component::new(ObjectId::slugify("metrics"), "panel")
            .with_prop("title", "Metrics")
            .with_layout(Layout::absolute(0, 0, 12, 3));
        app.session
            .create_component(panel, None, None, None)
            .unwrap();
        app.dispatch(Action::LayerNext);
        app.dispatch(Action::SelectNext);
        let rows = screen(&mut app, 100, 16);
        let all = rows.join("\n");
        assert!(all.contains("◆"), "resize handle drawn: {all}");
        assert!(all.contains("Inspector"), "{all}");
        assert!(
            all.contains("id     metrics") || all.contains("metrics"),
            "{all}"
        );
        assert!(all.contains("INTERFACE"), "{all}");
    }

    #[test]
    fn prompt_renders_with_suggestions() {
        let mut app = test_app(Size::new(30, 8));
        app.dispatch(Action::LayerNext);
        app.dispatch(Action::AddComponent);
        let rows = screen(&mut app, 100, 16);
        let all = rows.join("\n");
        assert!(all.contains("Add component"), "{all}");
        assert!(all.contains("panel") && all.contains("label"), "{all}");
    }

    #[test]
    fn tiny_terminal_renders_without_panic() {
        let mut app = test_app(Size::new(80, 24));
        let _ = screen(&mut app, 3, 3);
        let _ = screen(&mut app, 1, 1);
    }
}
