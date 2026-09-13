//! The canvas: the document seen through the viewport.

use ratatui::Frame;
use ratatui::layout::{Position as RPosition, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::{Block, BorderType, Borders};

use super::styles;
use crate::app::{App, Focus};
use crate::render::draw_cells;

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
    if canvas_area.width == 0 || canvas_area.height == 0 {
        return;
    }

    let depth = app.color_depth;
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
    if let Some(cells) = app.cached_composite() {
        draw_cells(cells, viewport, canvas_area, buf, depth, paper);
    } else if let Ok(cells) = app.session.render(&app.editor.screen, None) {
        draw_cells(&cells, viewport, canvas_area, buf, depth, paper);
    }

    // Selection overlay: highlight the selected component's outline and
    // mark the bottom-right resize handle.
    if let Some(rect) = app
        .selection()
        .and_then(|id| app.cached_layout().and_then(|l| l.rect(id)))
    {
        let sel = styles::selection(app);
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
                let is_corner =
                    u32::from(pos.x) + 1 == rect.right() && u32::from(pos.y) + 1 == rect.bottom();
                if is_corner && rect.width > 1 && rect.height > 1 {
                    cell.set_symbol("◆");
                }
            }
        }
    }

    if focused && !app.ui.help_open && app.prompt.is_none() {
        let c = app.editor.cursor;
        if viewport.contains(c) {
            let sx = canvas_area.x + (c.x - viewport.x);
            let sy = canvas_area.y + (c.y - viewport.y);
            frame.set_cursor_position(RPosition::new(sx, sy));
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
    fn panels_render_layer_list_and_keys() {
        let mut app = test_app(Size::new(10, 3));
        let rows = screen(&mut app, 100, 14);
        let all = rows.join("\n");
        assert!(all.contains("Artwork"), "{all}");
        assert!(all.contains("UI"), "{all}");
        assert!(all.contains("Layers"), "{all}");
        assert!(all.contains("Keys"), "{all}");
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
