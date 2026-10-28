//! Data views: lists, trees, tables, data grids, property grids and
//! key/value lists. All read their content from properties or from the
//! document's fixtures through `bind`.

use crate::component::Component;
use crate::document::{Canvas, Cell, Rect, Size};
use crate::layout::Edges;
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit;
use crate::render::{ComponentRenderer, RenderContext, painter};
use crate::value::Value;

fn list_value(items: &[&str]) -> Value {
    Value::List(items.iter().map(|s| (*s).into()).collect())
}

/// Vertical list with a selection marker.
#[derive(Debug)]
pub struct List;

impl ComponentRenderer for List {
    fn kind(&self) -> &'static str {
        "list"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("list", "List", Category::Data)
            .summary("Items, one per row, with a selection")
            .keywords(&["items", "listbox", "picker", "rows"])
            .size(Some((24, 6)))
            .prop(
                PropSpec::new("items", PropKind::List, "the entries")
                    .initial(list_value(&["alpha", "beta", "gamma"])),
            )
            .prop(
                PropSpec::new("selected", PropKind::Index, "highlighted entry")
                    .initial(Value::Int(0)),
            )
            .prop(PropSpec::new("marker", PropKind::Text, "before the selected entry").default("›"))
            .prop(kit::bind_prop())
            .prop(kit::state_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let w = items
            .iter()
            .map(|i| painter::text_width(i))
            .max()
            .unwrap_or(4)
            + 2;
        Some(Size::new(w, items.len().max(1) as u16))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let items = kit::strings(c, ctx, "items");
        if items.is_empty() || kit::state(c) == kit::State::Empty {
            kit::put(
                canvas,
                inner.x,
                inner.y,
                "(no items)",
                kit::dim(kit::style(ctx, "foreground-muted", "surface")),
                inner,
            );
            return;
        }
        let current = kit::selected_index(c, &items);
        let marker = kit::str_or(c, "marker", "›");
        // Keep the selection in view.
        let rows = usize::from(inner.height);
        let start = current.map_or(0, |s| s.saturating_sub(rows.saturating_sub(1)));
        for (row, (i, item)) in items.iter().enumerate().skip(start).take(rows).enumerate() {
            let y = inner.y + row as u16;
            let selected = current == Some(i);
            let style = if selected {
                kit::style(ctx, "selection-foreground", "selection")
            } else {
                kit::style(ctx, "foreground", "surface")
            };
            if selected {
                painter::fill(
                    canvas,
                    Rect::new(inner.x, y, inner.width, 1),
                    &Cell::blank(style),
                );
            }
            let lead = if selected {
                format!("{marker} ")
            } else {
                "  ".to_owned()
            };
            kit::put(canvas, inner.x, y, &format!("{lead}{item}"), style, inner);
        }
    }
}

/// Tree from indented items (two spaces per level); nodes with deeper
/// items after them are expanded (`▾`), a leading `+` marks a collapsed one.
#[derive(Debug)]
pub struct Tree;

impl ComponentRenderer for Tree {
    fn kind(&self) -> &'static str {
        "tree"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("tree", "Tree", Category::Data)
            .summary("Nested items with guides")
            .keywords(&["hierarchy", "outline", "explorer", "files", "folders"])
            .size(Some((28, 8)))
            .prop(
                PropSpec::new(
                    "items",
                    PropKind::List,
                    "entries, indented two spaces per level",
                )
                .initial(list_value(&[
                    "src",
                    "  main.rs",
                    "  app",
                    "    mod.rs",
                    "+target",
                    "Cargo.toml",
                ])),
            )
            .prop(PropSpec::new(
                "selected",
                PropKind::Index,
                "highlighted row",
            ))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let w = items
            .iter()
            .map(|i| painter::text_width(i) + 4)
            .max()
            .unwrap_or(6);
        Some(Size::new(w, items.len().max(1) as u16))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let raw = kit::strings(c, ctx, "items");
        let depth = |s: &str| (s.len() - s.trim_start().len()) / 2;
        let nodes: Vec<(usize, String)> = raw
            .iter()
            .map(|s| (depth(s), s.trim().to_owned()))
            .collect();
        let current = c
            .prop("selected")
            .and_then(|_| kit::selected_index(c, &raw));
        let guide = kit::style(ctx, "border", "surface");
        for (i, (d, name)) in nodes.iter().enumerate().take(usize::from(inner.height)) {
            let y = inner.y + i as u16;
            // A level continues (`│`) while a later sibling at that depth
            // follows before the branch ends.
            let later = |depth: usize| {
                nodes[i + 1..]
                    .iter()
                    .take_while(|(dd, _)| *dd >= depth)
                    .any(|(dd, _)| *dd == depth)
            };
            let mut prefix = String::new();
            for level in 0..*d {
                prefix.push_str(match (level + 1 == *d, later(level + 1)) {
                    (true, true) => "├─",
                    (true, false) => "└─",
                    (false, true) => "│ ",
                    (false, false) => "  ",
                });
            }
            let collapsed = name.starts_with('+');
            let has_children = nodes.get(i + 1).is_some_and(|(dd, _)| dd > d);
            let (icon, label) = match (collapsed, has_children) {
                (true, _) => ("▸ ", &name[1..]),
                (false, true) => ("▾ ", name.as_str()),
                _ => ("  ", name.as_str()),
            };
            let mut x = inner.x + kit::put(canvas, inner.x, y, &prefix, guide, inner);
            x += kit::put(
                canvas,
                x,
                y,
                icon,
                kit::style(ctx, "primary", "surface"),
                inner,
            );
            let style = if current == Some(i) {
                kit::style(ctx, "selection-foreground", "selection")
            } else if has_children || collapsed {
                kit::bold(kit::style(ctx, "foreground", "surface"))
            } else {
                kit::style(ctx, "foreground", "surface")
            };
            kit::put(canvas, x, y, label, style, inner);
        }
    }
}

/// Tables: `table` (header, rule, rows) and `datagrid` (full grid lines,
/// numbers right-aligned).
#[derive(Debug)]
pub struct Table {
    grid: bool,
}

impl Table {
    pub const TABLE: Self = Self { grid: false };
    pub const DATAGRID: Self = Self { grid: true };

    fn columns(c: &Component, ctx: &RenderContext<'_>) -> Vec<String> {
        kit::strings(c, ctx, "columns")
    }

    /// Column widths: content widths, the last column taking the rest.
    fn widths(&self, columns: &[String], rows: &[Vec<String>], total: u16) -> Vec<u16> {
        let n = columns
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if n == 0 {
            return Vec::new();
        }
        let mut w: Vec<u16> = (0..n)
            .map(|i| {
                let head = columns.get(i).map_or(0, |h| painter::text_width(h));
                rows.iter()
                    .filter_map(|r| r.get(i))
                    .map(|c| painter::text_width(c))
                    .max()
                    .unwrap_or(0)
                    .max(head)
            })
            .collect();
        let seps = if self.grid {
            n as u16 + 1
        } else {
            n.saturating_sub(1) as u16 * 2
        };
        let used: u16 = w.iter().sum::<u16>() + seps;
        if let Some(last) = w.last_mut() {
            if used < total {
                *last += total - used;
            }
        }
        w
    }
}

fn numeric(s: &str) -> bool {
    let t = s.trim_end_matches(['%', 's']).replace([',', '.', '_'], "");
    !t.is_empty()
        && t.trim_start_matches(['-', '+'])
            .chars()
            .all(|c| c.is_ascii_digit())
}

impl ComponentRenderer for Table {
    fn kind(&self) -> &'static str {
        if self.grid { "datagrid" } else { "table" }
    }

    fn descriptor(&self) -> Descriptor {
        let d = if self.grid {
            Descriptor::new("datagrid", "Data Grid", Category::Data)
                .summary("Table with grid lines and aligned numbers")
                .keywords(&["spreadsheet", "grid", "cells", "table"])
        } else {
            Descriptor::new("table", "Table", Category::Data)
                .summary("Rows under a header")
                .keywords(&["rows", "columns", "records", "list"])
        };
        d.size(Some((40, 6)))
            .prop(
                PropSpec::new("columns", PropKind::List, "header cells")
                    .initial(list_value(&["Name", "CPU", "Memory"])),
            )
            .prop(
                PropSpec::new(
                    "rows",
                    PropKind::List,
                    "rows: lists of cells, or maps by column",
                )
                .initial(Value::List(vec![
                    list_value(&["nginx", "12%", "48M"]),
                    list_value(&["postgres", "31%", "512M"]),
                    list_value(&["redis", "3%", "9M"]),
                ])),
            )
            .prop(PropSpec::new(
                "selected",
                PropKind::Index,
                "highlighted row",
            ))
            .prop(PropSpec::new(
                "striped",
                PropKind::Bool,
                "alternate row background",
            ))
            .prop(kit::bind_prop())
            .prop(kit::state_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let columns = Self::columns(c, ctx);
        let rows = kit::rows(c, ctx, &columns);
        let w: u16 =
            self.widths(&columns, &rows, 0).iter().sum::<u16>() + columns.len() as u16 * 2 + 1;
        let h = rows.len() as u16 + if self.grid { 4 } else { 2 };
        Some(Size::new(w, h))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let columns = Self::columns(c, ctx);
        let rows = kit::rows(c, ctx, &columns);
        let widths = self.widths(&columns, &rows, inner.width);
        let line = kit::style(ctx, "border", "surface");
        let head = kit::bold(kit::style(ctx, "primary", "surface"));
        let selected = c.prop_int("selected").and_then(|n| usize::try_from(n).ok());
        let mut y = inner.y;
        let rule = |canvas: &mut Canvas, y: u16, l: &str, m: &str, r: &str| {
            let mut s = String::from(l);
            for (i, w) in widths.iter().enumerate() {
                s.push_str(&"─".repeat(usize::from(*w)));
                s.push_str(if i + 1 == widths.len() { r } else { m });
            }
            kit::put(canvas, inner.x, y, &s, line, inner);
        };
        if self.grid {
            rule(canvas, y, "┌", "┬", "┐");
            y += 1;
        }
        let row = |canvas: &mut Canvas,
                   y: u16,
                   cells: &[String],
                   style: crate::document::CellStyle,
                   numbers: bool| {
            let mut x = inner.x;
            if self.grid {
                x += kit::put(canvas, x, y, "│", line, inner);
            }
            for (i, w) in widths.iter().enumerate() {
                let cell = cells.get(i).map_or("", String::as_str);
                let text = if numbers && numeric(cell) {
                    format!("{cell:>width$}", width = usize::from(*w))
                } else {
                    kit::fit(cell, *w)
                };
                x += kit::put(canvas, x, y, &text, style, inner);
                let sep = if self.grid {
                    "│"
                } else if i + 1 < widths.len() {
                    "  "
                } else {
                    ""
                };
                x += kit::put(
                    canvas,
                    x,
                    y,
                    sep,
                    if self.grid { line } else { style },
                    inner,
                );
            }
        };
        row(canvas, y, &columns, head, false);
        y += 1;
        if self.grid {
            rule(canvas, y, "├", "┼", "┤");
        } else {
            let total: u16 = widths.iter().sum::<u16>() + widths.len().saturating_sub(1) as u16 * 2;
            kit::put(
                canvas,
                inner.x,
                y,
                &"─".repeat(usize::from(total)),
                line,
                inner,
            );
        }
        y += 1;
        if rows.is_empty() || kit::state(c) == kit::State::Empty {
            kit::put(
                canvas,
                inner.x + 1,
                y,
                "No rows",
                kit::dim(kit::style(ctx, "foreground-muted", "surface")),
                inner,
            );
            return;
        }
        if kit::state(c) == kit::State::Loading {
            kit::put(
                canvas,
                inner.x + 1,
                y,
                "⠋ Loading…",
                kit::style(ctx, "foreground-muted", "surface"),
                inner,
            );
            return;
        }
        let striped = c.prop_bool("striped") == Some(true);
        let bottom = inner.bottom().saturating_sub(u32::from(self.grid)) as u16;
        for (i, cells) in rows.iter().enumerate() {
            if y >= bottom {
                break;
            }
            let style = if selected == Some(i) {
                kit::style(ctx, "selection-foreground", "selection")
            } else if striped && i % 2 == 1 {
                kit::style(ctx, "foreground", "surface-alt")
            } else {
                kit::style(ctx, "foreground", "surface")
            };
            if selected == Some(i) || (striped && i % 2 == 1) {
                painter::fill(
                    canvas,
                    Rect::new(inner.x, y, inner.width, 1),
                    &Cell::blank(style),
                );
            }
            row(canvas, y, cells, style, self.grid);
            y += 1;
        }
        if self.grid && y < inner.bottom() as u16 {
            rule(canvas, y, "└", "┴", "┘");
        }
    }
}

/// Property grid (`propertygrid`, key and value columns with a divider)
/// and key/value list (`keyvalue`, `Key: value` per row).
#[derive(Debug)]
pub struct KeyValue {
    grid: bool,
}

impl KeyValue {
    pub const GRID: Self = Self { grid: true };
    pub const LIST: Self = Self { grid: false };

    fn pairs(c: &Component, ctx: &RenderContext<'_>) -> Vec<(String, String)> {
        match kit::value(c, ctx, "items").as_deref() {
            Some(Value::Map(m)) => m.iter().map(|(k, v)| (k.clone(), kit::text(v))).collect(),
            _ => kit::strings(c, ctx, "items")
                .into_iter()
                .map(|i| match i.split_once('=').or_else(|| i.split_once(':')) {
                    Some((k, v)) => (k.trim().to_owned(), v.trim().to_owned()),
                    None => (i, String::new()),
                })
                .collect(),
        }
    }
}

impl ComponentRenderer for KeyValue {
    fn kind(&self) -> &'static str {
        if self.grid {
            "propertygrid"
        } else {
            "keyvalue"
        }
    }

    fn descriptor(&self) -> Descriptor {
        let d = if self.grid {
            Descriptor::new("propertygrid", "Property Grid", Category::Data)
                .summary("Two columns of names and values")
                .keywords(&["properties", "inspector", "settings", "details"])
        } else {
            Descriptor::new("keyvalue", "Key/Value List", Category::Data)
                .summary("`Key: value` rows")
                .keywords(&["details", "summary", "info", "facts"])
        };
        d.size(Some((30, 4)))
            .prop(
                PropSpec::new("items", PropKind::List, "\"Key=Value\" entries or a map").initial(
                    list_value(&["Host=db-01", "Region=eu-west", "Uptime=12d 4h"]),
                ),
            )
            .prop(PropSpec::new(
                "selected",
                PropKind::Index,
                "highlighted row",
            ))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let pairs = Self::pairs(c, ctx);
        let kw = pairs
            .iter()
            .map(|(k, _)| painter::text_width(k))
            .max()
            .unwrap_or(0);
        let vw = pairs
            .iter()
            .map(|(_, v)| painter::text_width(v))
            .max()
            .unwrap_or(0);
        Some(Size::new(kw + vw + 3, pairs.len().max(1) as u16))
    }

    fn chrome_inset(&self, _: &Component, _: &RenderContext<'_>) -> Edges {
        Edges::ZERO
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let pairs = Self::pairs(c, ctx);
        let kw = pairs
            .iter()
            .map(|(k, _)| painter::text_width(k))
            .max()
            .unwrap_or(0);
        let selected = c.prop_int("selected").and_then(|n| usize::try_from(n).ok());
        for (i, (k, v)) in pairs.iter().enumerate().take(usize::from(inner.height)) {
            let y = inner.y + i as u16;
            let sel = selected == Some(i);
            let (ks, vs) = if sel {
                let s = kit::style(ctx, "selection-foreground", "selection");
                painter::fill(
                    canvas,
                    Rect::new(inner.x, y, inner.width, 1),
                    &Cell::blank(s),
                );
                (s, s)
            } else {
                (
                    kit::style(ctx, "foreground-muted", "surface"),
                    kit::style(ctx, "foreground", "surface"),
                )
            };
            if self.grid {
                let x = inner.x + kit::put(canvas, inner.x, y, &kit::fit(k, kw), ks, inner);
                let x = x + kit::put(
                    canvas,
                    x,
                    y,
                    " │ ",
                    kit::style(ctx, "border", "surface"),
                    inner,
                );
                kit::put(canvas, x, y, v, vs, inner);
            } else {
                let x =
                    inner.x + kit::put(canvas, inner.x, y, &format!("{k}: "), kit::bold(ks), inner);
                kit::put(canvas, x, y, v, vs, inner);
            }
        }
    }
}
