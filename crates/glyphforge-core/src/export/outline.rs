//! The compact text outline of a document (`glyphforge inspect`): one
//! line per screen, layer and component, for people and agents.

use std::fmt::Write as _;

use crate::component::Component;
use crate::document::Document;
use crate::layout::{Dimension, Placement};

/// A compact, agent-readable outline of the document.
pub fn outline(doc: &Document) -> String {
    let mut s = String::new();
    let title = if doc.meta.title.is_empty() {
        "untitled"
    } else {
        &doc.meta.title
    };
    let _ = writeln!(s, "document: {title} (theme: {})", doc.theme.name);
    for g in &doc.gradients {
        let _ = writeln!(s, "gradient {} \"{}\": {}", g.id, g.name, g.stops_text());
    }
    for (key, value) in &doc.data {
        let shape = match value {
            crate::value::Value::List(items) => format!("{} items", items.len()),
            crate::value::Value::Map(m) => {
                format!(
                    "fields {}",
                    m.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            }
            other => other.to_string(),
        };
        let _ = writeln!(s, "data {key}: {shape}");
    }
    for screen in doc.screens() {
        let _ = writeln!(
            s,
            "screen {} \"{}\" {}×{}",
            screen.id,
            screen.name,
            screen.size().width,
            screen.size().height
        );
        for layer in screen.layers() {
            let flags = format!(
                "{}{}",
                if layer.visible { "" } else { " hidden" },
                if layer.locked { " locked" } else { "" }
            );
            let _ = writeln!(
                s,
                "  layer {} \"{}\" [{}]{flags}",
                layer.id,
                layer.name,
                layer.kind().name()
            );
            if let Some(cells) = layer.cells() {
                let opaque = cells.cells().iter().filter(|c| !c.is_empty()).count();
                let _ = writeln!(s, "    {opaque} painted cells");
            }
            if let Some(raster) = layer.raster() {
                let shadow = raster
                    .shadow
                    .map_or_else(String::new, |sh| format!(", shadow {},{}", sh.dx, sh.dy));
                let _ = writeln!(
                    s,
                    "    {} painted pixels ({} {}×{}, dither {}, {}{shadow})",
                    raster.pixels.painted(),
                    raster.mode.name(),
                    raster.pixels.width(),
                    raster.pixels.height(),
                    raster.dither.name(),
                    raster.palette.name()
                );
            }
            if let Some(roots) = layer.components() {
                for c in roots {
                    outline_component(c, 2, &mut s);
                }
            }
        }
    }
    s
}

fn outline_component(c: &Component, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let mut line = format!("{indent}{} {}", c.id, c.kind);
    match c.layout.placement {
        Placement::Absolute { x, y } => {
            let _ = write!(line, " @{x},{y}");
        }
        Placement::Flow => {}
    }
    let dim = |d: Dimension| match d {
        Dimension::Fill => None,
        other => Some(other.to_string()),
    };
    if let Some(w) = dim(c.layout.width) {
        let _ = write!(line, " width={w}");
    }
    if let Some(h) = dim(c.layout.height) {
        let _ = write!(line, " height={h}");
    }
    for (k, v) in &c.props {
        let _ = write!(line, " {k}={v:?}");
    }
    if !c.responsive.is_empty() {
        let _ = write!(line, " responsive={}", c.responsive.len());
    }
    line.push('\n');
    out.push_str(&line);
    for child in &c.children {
        outline_component(child, depth + 1, out);
    }
}
