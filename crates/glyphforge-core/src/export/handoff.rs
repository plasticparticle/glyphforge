//! The hand-off format: a YAML outline of a design for implementers and
//! code-generating agents. It carries the semantic tree (ids, kinds,
//! properties, layout) together with the solved rectangle of every
//! component, the theme tokens and the data fixtures. Strings are written
//! as JSON strings, which are valid YAML scalars.

use std::fmt::Write as _;

use crate::component::Component;
use crate::document::Document;
use crate::layout::{Direction, Placement};
use crate::render::{Registry, RegistryMeasure, RenderContext};
use crate::value::Value;

fn q(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned())
}

fn value(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// The YAML hand-off of the whole document.
pub fn handoff(doc: &Document, registry: &Registry) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Glyphforge hand-off: the semantic design with solved geometry."
    );
    let _ = writeln!(s, "format: glyphforge-handoff");
    let _ = writeln!(s, "version: 1");
    if !doc.meta.title.is_empty() {
        let _ = writeln!(s, "title: {}", q(&doc.meta.title));
    }
    let _ = writeln!(s, "theme:");
    let _ = writeln!(s, "  name: {}", q(&doc.theme.name));
    let _ = writeln!(s, "  border: {}", doc.theme.border.name());
    let _ = writeln!(s, "  tokens:");
    for (token, color) in &doc.theme.colors {
        let _ = writeln!(s, "    {token}: {}", q(&color.to_string()));
    }
    if !doc.data.is_empty() {
        let _ = writeln!(s, "data:");
        for (k, v) in &doc.data {
            let _ = writeln!(s, "  {k}: {}", value(v));
        }
    }
    if !doc.symbols.is_empty() {
        let _ = writeln!(s, "symbols:");
        for sym in &doc.symbols {
            let _ = writeln!(s, "  - id: {}", sym.id);
            let _ = writeln!(s, "    name: {}", q(&sym.name));
            let _ = writeln!(s, "    root: {}", sym.component.kind);
        }
    }
    let _ = writeln!(s, "screens:");
    for screen in doc.screens() {
        let size = screen.size();
        let _ = writeln!(s, "  - id: {}", screen.id);
        let _ = writeln!(s, "    name: {}", q(&screen.name));
        let _ = writeln!(
            s,
            "    size: {{width: {}, height: {}}}",
            size.width, size.height
        );
        let _ = writeln!(s, "    layers:");
        for layer in screen.layers() {
            let _ = writeln!(s, "      - id: {}", layer.id);
            let _ = writeln!(s, "        kind: {}", layer.kind().name());
            if !layer.visible {
                let _ = writeln!(s, "        visible: false");
            }
            let Some(roots) = layer.components() else {
                if let Some(cells) = layer.cells() {
                    let _ = writeln!(
                        s,
                        "        cells: {} painted",
                        cells.cells().iter().filter(|c| !c.is_empty()).count()
                    );
                }
                continue;
            };
            let measure = RegistryMeasure {
                registry,
                ctx: RenderContext {
                    theme: &doc.theme,
                    screen: Some(screen),
                    data: Some(&doc.data),
                    symbols: &doc.symbols,
                    depth: 0,
                },
            };
            let result =
                crate::layout::solve(roots, crate::document::Rect::from_size(size), &measure);
            let _ = writeln!(s, "        components:");
            for c in roots {
                write_component(&mut s, c, &result, 5);
            }
        }
    }
    s
}

fn write_component(
    s: &mut String,
    c: &Component,
    result: &crate::layout::LayoutResult,
    depth: usize,
) {
    let pad = "  ".repeat(depth);
    let _ = writeln!(s, "{pad}- id: {}", c.id);
    let _ = writeln!(s, "{pad}  kind: {}", c.kind);
    if let Some(r) = result.rect(&c.id) {
        let _ = writeln!(
            s,
            "{pad}  rect: {{x: {}, y: {}, width: {}, height: {}}}",
            r.x, r.y, r.width, r.height
        );
    }
    let l = &c.layout;
    let mut layout = vec![
        format!("width: {}", l.width),
        format!("height: {}", l.height),
    ];
    if let Placement::Absolute { x, y } = l.placement {
        layout.push(format!("at: [{x}, {y}]"));
    }
    if !c.children.is_empty() {
        let dir = match l.container.direction {
            Direction::Horizontal => "horizontal",
            Direction::Vertical => "vertical",
            Direction::Stack => "stack",
            Direction::Grid => "grid",
        };
        layout.push(format!("direction: {dir}"));
        if l.container.gap > 0 {
            layout.push(format!("gap: {}", l.container.gap));
        }
    }
    let _ = writeln!(s, "{pad}  layout: {{{}}}", layout.join(", "));
    if !c.props.is_empty() {
        let _ = writeln!(s, "{pad}  props:");
        for (k, v) in &c.props {
            let _ = writeln!(s, "{pad}    {k}: {}", value(v));
        }
    }
    if !c.children.is_empty() {
        let _ = writeln!(s, "{pad}  children:");
        for child in &c.children {
            write_component(s, child, result, depth + 2);
        }
    }
}
