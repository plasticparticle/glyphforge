//! The README screenshot: drives the editor like a person would (tools,
//! brushes, colours, mouse strokes), renders the whole UI through Ratatui's
//! test backend and writes it as an SVG. Run on demand:
//!
//! ```sh
//! GLYPHFORGE_SCREENSHOT=docs/screenshot.svg cargo test -p glyphforge screenshot -- --ignored
//! ```

use glyphforge_core::{Color, Position, Size};
use std::fmt::Write as _;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color as RColor, Modifier};

use super::brushes::{BrushKind, presets};
use super::{App, ColorTarget, Tool, test_app};
use crate::actions::{Action, PixelAction};
use crate::terminal::ColorDepth;

fn brush(name: &str) -> BrushKind {
    presets()
        .find(|p| p.name == name)
        .map_or_else(|| panic!("no brush {name}"), |p| p.kind)
}

fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap_or_default()
}

fn stroke(app: &mut App, points: &[(u16, u16)]) {
    let p = |(x, y): (u16, u16)| Position::new(x, y);
    app.dispatch(Action::ToolPress(p(points[0])));
    for &q in &points[1..] {
        app.dispatch(Action::ToolDrag(p(q)));
    }
    app.dispatch(Action::ToolRelease);
}

fn color(app: &mut App, fg: &str, bg: Option<&str>) {
    app.dispatch(Action::SetColor(ColorTarget::Foreground, hex(fg)));
    app.dispatch(Action::SetColor(
        ColorTarget::Background,
        bg.map_or(Color::Default, hex),
    ));
}

fn text(app: &mut App, x: u16, y: u16, s: &str) {
    app.dispatch(Action::SelectTool(Tool::Text));
    app.dispatch(Action::CursorTo(Position::new(x, y)));
    app.dispatch(Action::InsertText(s.into()));
}

/// Draws a small scene on the artwork layer and a pixel layer.
fn draw(app: &mut App) {
    // A pixel layer in half blocks: an outlined box filled with a gradient.
    let art = app.editor.layer.clone();
    app.dispatch(Action::Pixel(PixelAction::NewLayer));
    if let Some(p) = app.prompt.as_mut() {
        p.input = "half".into();
    }
    app.dispatch(Action::PromptSubmit);
    let px = |x: u16, y: u16| Position::new(x, y);
    color(app, "#ffb454", None);
    app.dispatch(Action::SelectTool(Tool::Rect));
    app.dispatch(Action::Pixel(PixelAction::Press(px(44, 4), false)));
    app.dispatch(Action::Pixel(PixelAction::Drag(px(67, 27))));
    app.dispatch(Action::Pixel(PixelAction::Release));
    app.dispatch(Action::SelectTool(Tool::Gradient));
    app.dispatch(Action::Pixel(PixelAction::SelectGradient(
        glyphforge_core::ObjectId::new("fire").unwrap_or_else(|_| unreachable!()),
    )));
    app.dispatch(Action::Pixel(PixelAction::Press(px(56, 26), false)));
    app.dispatch(Action::Pixel(PixelAction::Drag(px(56, 5))));
    app.dispatch(Action::Pixel(PixelAction::Release));
    // Back to the artwork layer for cells.
    app.activate_layer(&art);

    // A double-framed window with a title.
    app.dispatch(Action::SelectTool(Tool::Box));
    color(app, "#ffb454", None);
    app.dispatch(Action::SetBoxStyle(
        glyphforge_core::boxdraw::BorderFamily::Double,
    ));
    stroke(app, &[(1, 1), (40, 20)]);
    color(app, "#14161b", Some("#ffb454"));
    text(app, 3, 1, " forge.sketch ");

    // Rounded lines that join by themselves: a little flow diagram.
    app.dispatch(Action::SetBrushKind(brush("rounded line")));
    app.dispatch(Action::SelectTool(Tool::Rect));
    color(app, "#8ab4f8", None);
    stroke(app, &[(4, 3), (14, 5)]);
    stroke(app, &[(26, 3), (37, 5)]);
    color(app, "#9ccc65", None);
    stroke(app, &[(15, 8), (25, 10)]);
    app.dispatch(Action::SelectTool(Tool::Brush));
    color(app, "#c4a7e7", None);
    stroke(app, &[(9, 6), (9, 9), (14, 9)]);
    stroke(app, &[(31, 6), (31, 9), (26, 9)]);
    color(app, "#d7dae0", None);
    text(app, 6, 4, "input");
    text(app, 28, 4, "theme");
    text(app, 17, 9, "render");

    // A bar chart painted with the shade ramp.
    app.dispatch(Action::SetBrushKind(brush("shade ramp")));
    app.dispatch(Action::SelectTool(Tool::Brush));
    for (i, h) in [3u16, 5, 4, 7, 6, 8, 5, 7].iter().enumerate() {
        let x = 4 + i as u16 * 2;
        color(app, if i % 2 == 0 { "#ff7b72" } else { "#ffca6b" }, None);
        // Two passes darken the shade.
        for _ in 0..2 {
            stroke(app, &[(x, 18), (x, 18 - h + 1)]);
        }
    }
    color(app, "#7c8291", None);
    text(app, 4, 19, "▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔");

    // UI stamps: a button and a toggle.
    app.dispatch(Action::SetBrushKind(brush("rounded boxed button")));
    color(app, "#14161b", Some("#9ccc65"));
    app.paint.label = "Export".into();
    app.dispatch(Action::SelectTool(Tool::Brush));
    stroke(app, &[(24, 15)]);
    app.dispatch(Action::SetBrushKind(brush("toggle on")));
    color(app, "#ffb454", None);
    app.paint.label = "Snap".into();
    stroke(app, &[(24, 13)]);

    // Leave the Brush tool with a line brush selected, as mid-drawing.
    app.dispatch(Action::SetBrushKind(brush("rounded line")));
    app.dispatch(Action::SelectTool(Tool::Brush));
    app.dispatch(Action::CursorTo(Position::new(32, 17)));
}

fn hex_of((red, green, blue): (u8, u8, u8)) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

fn rgb(color: RColor, fallback: &str) -> String {
    match color {
        RColor::Rgb(red, green, blue) => hex_of((red, green, blue)),
        RColor::Indexed(index) => hex_of(Color::Indexed(index).to_rgb().unwrap_or((0, 0, 0))),
        _ => fallback.to_owned(),
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The terminal buffer as an SVG: one background rect and one glyph per
/// cell, so the grid stays exact whatever the font's advance.
fn svg(buf: &ratatui::buffer::Buffer, fg: &str, bg: &str) -> String {
    const W: f32 = 9.0;
    const H: f32 = 19.0;
    let area = buf.area;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         font-family=\"JetBrainsMono Nerd Font, DejaVu Sans Mono, monospace\" font-size=\"15\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{bg}\"/>\n",
        f32::from(area.width) * W,
        f32::from(area.height) * H
    );
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(x, y)];
            let (mut f, mut b) = (rgb(cell.fg, fg), rgb(cell.bg, bg));
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut f, &mut b);
            }
            let (px, py) = (f32::from(x) * W, f32::from(y) * H);
            if b != bg {
                let _ = writeln!(
                    out,
                    "<rect x=\"{px}\" y=\"{py}\" width=\"{}\" height=\"{H}\" fill=\"{b}\"/>",
                    W + 0.5
                );
            }
            let sym = cell.symbol();
            if sym.trim().is_empty() {
                continue;
            }
            let mut attrs = String::new();
            if cell.modifier.contains(Modifier::BOLD) {
                attrs.push_str(" font-weight=\"bold\"");
            }
            if cell.modifier.contains(Modifier::DIM) {
                attrs.push_str(" opacity=\"0.6\"");
            }
            let _ = writeln!(
                out,
                "<text x=\"{px}\" y=\"{}\" fill=\"{f}\"{attrs}>{}</text>",
                py + H * 0.76,
                escape(sym)
            );
        }
    }
    out.push_str("</svg>\n");
    out
}

#[test]
#[ignore = "writes the README screenshot; run with GLYPHFORGE_SCREENSHOT=path"]
fn screenshot() {
    let Ok(path) = std::env::var("GLYPHFORGE_SCREENSHOT") else {
        return;
    };
    let mut app = test_app(Size::new(70, 22));
    app.color_depth = ColorDepth::TrueColor;
    app.set_ui_theme("forge-dark");
    draw(&mut app);
    app.status = None;
    let (width, height) = (124, 34);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    app.update_layout(ratatui::layout::Rect::new(0, 0, width, height));
    terminal.draw(|f| crate::ui::render(&app, f)).unwrap();
    let theme = &app.ui_theme;
    let hexof = |token: &str| hex_of(theme.color(token).to_rgb().unwrap_or((20, 22, 27)));
    let out = svg(
        terminal.backend().buffer(),
        &hexof("foreground"),
        &hexof("surface"),
    );
    std::fs::write(path, out).unwrap();
}
