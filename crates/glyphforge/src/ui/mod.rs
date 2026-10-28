//! Widgets and layout. Everything here reads `&App` and never mutates it.

pub mod canvas_view;
pub mod color_picker;
pub mod commands;
pub mod header;
pub mod help;
pub mod layout;
pub mod menu;
pub mod palette;
pub mod panels;
pub mod prompt;
pub mod status_bar;

use glyphforge_core::Color;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::App;
use crate::render::to_ratatui_color;

pub fn render(app: &App, frame: &mut Frame<'_>) {
    let area = frame.area();
    let layout = layout::compute(area, &app.ui);
    let depth = app.color_depth;

    // Paint the panel background everywhere first.
    let base = Style::default()
        .fg(to_ratatui_color(app.ui_theme.color("foreground"), depth))
        .bg(to_ratatui_color(app.ui_theme.color("surface"), depth));
    frame.render_widget(Block::default().style(base), area);

    header::render(app, frame, layout.header);
    if let Some(left) = layout.left {
        panels::render_left(app, frame, left);
    }
    canvas_view::render(app, frame, layout.canvas_block, layout.canvas);
    if let Some(right) = layout.right {
        panels::render_right(app, frame, right);
    }
    status_bar::render(app, frame, layout.status);
    if app.ui.help_open {
        help::render(app, frame, area);
    }
    color_picker::render(app, frame, area);
    menu::render(app, frame, area);
    prompt::render(app, frame, area);
    commands::render(app, frame, area);
}

/// Glyphs a frame is drawn with; other cells on the border (titles) keep
/// their own colour.
const FRAME_GLYPHS: &str = "─│┌┐└┘╭╮╰╯═║╔╗╚╝━┃┏┓┗┛├┤┬┴┼╞╡╤╧╪";

/// Recolours the frame around `area` along the theme's frame gradient,
/// from the top-left to the bottom-right of the screen. Themes without a
/// gradient keep the flat border colour.
pub fn paint_frame(app: &App, frame: &mut Frame<'_>, area: Rect, focused: bool) {
    let Some((from, to)) = app.ui_theme.frame_gradient() else {
        return;
    };
    let (Some(start), Some(end)) = (from.to_rgb(), to.to_rgb()) else {
        return;
    };
    let screen = frame.area();
    let (span_x, span_y) = (
        f32::from(screen.width.saturating_sub(1)).max(1.0),
        f32::from(screen.height.saturating_sub(1)).max(1.0),
    );
    let depth = app.color_depth;
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let on_border = x == area.left()
                || y == area.top()
                || x + 1 == area.right()
                || y + 1 == area.bottom();
            if !on_border {
                continue;
            }
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            if !FRAME_GLYPHS.contains(cell.symbol()) {
                continue;
            }
            // Diagonal position, so the gradient runs across the whole UI
            // rather than restarting in every panel.
            let t = f32::midpoint(
                f32::from(x - screen.x) / span_x,
                f32::from(y - screen.y) / span_y,
            );
            let color =
                Color::Rgb(start.0, start.1, start.2).mix(Color::Rgb(end.0, end.1, end.2), t);
            let mut style = Style::default().fg(to_ratatui_color(color, depth));
            if !focused {
                style = style.add_modifier(ratatui::style::Modifier::DIM);
            }
            cell.set_style(style);
        }
    }
}

/// Style helpers shared by widgets. Every colour comes from a UI theme
/// token, never from a literal.
pub(crate) mod styles {
    use ratatui::style::{Modifier, Style};

    use crate::app::App;
    use crate::render::to_ratatui_color;

    fn tok(app: &App, token: &str) -> ratatui::style::Color {
        to_ratatui_color(app.ui_theme.color(token), app.color_depth)
    }

    pub fn text(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "foreground"))
            .bg(tok(app, "surface"))
    }

    /// Secondary text. The muted token is used only when it provably
    /// contrasts with the background; the terminal's own palette is not
    /// knowable here, and ANSI 8 ("bright black") is nearly invisible on the
    /// dark backgrounds most terminal themes use. Otherwise the normal
    /// foreground is dimmed, which every palette renders legibly.
    pub fn muted(app: &App) -> Style {
        if muted_contrasts(&app.ui_theme) {
            text(app).fg(tok(app, "foreground-muted"))
        } else {
            text(app).add_modifier(Modifier::DIM)
        }
    }

    /// Whether the muted colour stands out from the background. Unknown
    /// colours (the terminal's defaults) count as too close to tell.
    pub(super) fn muted_contrasts(theme: &glyphforge_core::Theme) -> bool {
        /// Minimum difference in perceived brightness, 0..=255.
        const MIN_DIFFERENCE: i32 = 70;
        let brightness = |c: glyphforge_core::Color| {
            c.to_rgb().map(|(r, g, b)| {
                (i32::from(r) * 299 + i32::from(g) * 587 + i32::from(b) * 114) / 1000
            })
        };
        match (
            brightness(theme.color("foreground-muted")),
            brightness(theme.color("background")),
        ) {
            (Some(muted), Some(background)) => (muted - background).abs() >= MIN_DIFFERENCE,
            _ => false,
        }
    }

    pub fn accent(app: &App) -> Style {
        text(app)
            .fg(tok(app, "primary"))
            .add_modifier(Modifier::BOLD)
    }

    pub fn border(app: &App, focused: bool) -> Style {
        text(app).fg(tok(app, if focused { "focus" } else { "border-muted" }))
    }

    pub fn error(app: &App) -> Style {
        text(app).fg(tok(app, "error")).add_modifier(Modifier::BOLD)
    }

    pub fn warning(app: &App) -> Style {
        text(app).fg(tok(app, "warning"))
    }

    pub fn selection(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "selection-foreground"))
            .bg(tok(app, "selection"))
    }

    pub fn paper(app: &App) -> Style {
        Style::default()
            .fg(tok(app, "foreground"))
            .bg(tok(app, "background"))
    }
}

#[cfg(test)]
mod tests {
    use super::styles;
    use glyphforge_core::{Color, Theme};
    use ratatui::style::Modifier;

    #[test]
    fn gradient_frames_fade_across_the_screen() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut app = crate::app::test_app(glyphforge_core::Size::new(10, 4));
        // Gradients need the colours a real terminal would have.
        app.color_depth = crate::terminal::ColorDepth::TrueColor;
        let draw = |app: &mut crate::app::App| {
            let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
            app.update_layout(ratatui::layout::Rect::new(0, 0, 60, 20));
            terminal.draw(|f| super::render(app, f)).unwrap();
            let buffer = terminal.backend().buffer().clone();
            // Colours of the frame glyphs, in drawing order.
            (0..20)
                .flat_map(|y| (0..60).map(move |x| (x, y)))
                .filter(|(x, y)| super::FRAME_GLYPHS.contains(buffer[(*x, *y)].symbol()))
                .map(|(x, y)| (x + y, buffer[(x, y)].fg))
                .collect::<Vec<_>>()
        };

        app.set_ui_theme("cobalt");
        let flat = draw(&mut app);
        assert!(!flat.is_empty(), "frames are drawn");
        let flat_colours: std::collections::HashSet<_> = flat.iter().map(|(_, c)| c).collect();
        assert!(
            flat_colours.len() <= 2,
            "a flat theme uses one border colour per focus state"
        );

        app.set_ui_theme("aurora");
        let gradient = draw(&mut app);
        let gradient_colours: std::collections::HashSet<_> =
            gradient.iter().map(|(_, c)| c).collect();
        assert!(
            gradient_colours.len() > 20,
            "the frame fades across the screen: {} colours",
            gradient_colours.len()
        );
        // The corner nearest the top left and the one nearest the bottom
        // right sit at opposite ends of the fade.
        let first = gradient.iter().min_by_key(|(d, _)| *d).map(|(_, c)| *c);
        let last = gradient.iter().max_by_key(|(d, _)| *d).map(|(_, c)| *c);
        assert_ne!(first, last, "the fade runs across the screen");
    }

    #[test]
    fn muted_text_stays_legible_in_any_palette() {
        // The built-in theme leaves fore- and background to the terminal, so
        // the muted colour cannot be checked and the foreground is dimmed.
        assert!(!styles::muted_contrasts(&Theme::terminal()));
        let app = crate::app::test_app(glyphforge_core::Size::new(4, 2));
        let muted = styles::muted(&app);
        assert!(muted.add_modifier.contains(Modifier::DIM));
        assert_eq!(
            muted.fg,
            styles::text(&app).fg,
            "dimmed foreground, not grey"
        );

        // Themes that name both colours use the muted one as it is.
        for theme in [Theme::minimal_dark(), Theme::minimal_light()] {
            assert!(styles::muted_contrasts(&theme), "{}", theme.name);
        }

        // A theme whose muted colour sits on the background is refused.
        let mut murky = Theme::minimal_dark();
        murky
            .colors
            .insert("foreground-muted".into(), Color::Rgb(0x1a, 0x1c, 0x20));
        assert!(!styles::muted_contrasts(&murky));
    }
}
