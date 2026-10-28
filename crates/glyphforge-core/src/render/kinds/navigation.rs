//! Navigation: tabs, breadcrumbs, menus, toolbars and status bars.
//!
//! Items are lists of text; `Label|Key` gives an item a key hint.

use crate::component::Component;
use crate::document::{Canvas, Cell, Rect, Size};
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit;
use crate::render::{ComponentRenderer, RenderContext, painter};
use crate::value::Value;

fn items_prop(initial: &[&str]) -> PropSpec {
    PropSpec::new(
        "items",
        PropKind::List,
        "entries; \"Label|Key\" adds a key hint",
    )
    .initial(Value::List(initial.iter().map(|s| (*s).into()).collect()))
}

fn selected_prop() -> PropSpec {
    PropSpec::new(
        "selected",
        PropKind::Index,
        "index or label of the current item",
    )
    .initial(Value::Int(0))
}

/// Splits `Label|Key`.
fn split(item: &str) -> (&str, Option<&str>) {
    match item.split_once('|') {
        Some((l, k)) => (l.trim(), Some(k.trim())),
        None => (item, None),
    }
}

/// A row of tabs; the selected one is highlighted and underlined.
#[derive(Debug)]
pub struct Tabs;

impl ComponentRenderer for Tabs {
    fn kind(&self) -> &'static str {
        "tabs"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("tabs", "Tabs", Category::Navigation)
            .summary("Row of tabs with one selected")
            .keywords(&["tab bar", "pages", "sections"])
            .size(None)
            .prop(items_prop(&["Overview", "Logs", "Settings"]))
            .prop(selected_prop())
            .prop(
                PropSpec::new(
                    "style",
                    PropKind::choice(&["line", "boxed", "pills"]),
                    "look",
                )
                .default("line"),
            )
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let w: u16 = items
            .iter()
            .map(|i| painter::text_width(split(i).0) + 3)
            .sum();
        let h = if kit::str_or(c, "style", "line") == "line" {
            2
        } else {
            1
        };
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
        let items = kit::strings(c, ctx, "items");
        let current = kit::selected_index(c, &items).unwrap_or(0);
        let look = kit::str_or(c, "style", "line");
        let mut x = inner.x;
        if look == "line" && inner.height > 1 {
            let rule: String = std::iter::repeat_n("─", usize::from(inner.width)).collect();
            kit::put(
                canvas,
                inner.x,
                inner.y + 1,
                &rule,
                kit::style(ctx, "border-muted", "surface"),
                inner,
            );
        }
        for (i, item) in items.iter().enumerate() {
            let (label, _) = split(item);
            let active = i == current;
            let (text, style) = match (look, active) {
                ("pills", true) => (
                    format!(" {label} "),
                    kit::style(ctx, "selection-foreground", "primary"),
                ),
                ("boxed", true) => (
                    format!("[{label}]"),
                    kit::bold(kit::style(ctx, "primary", "surface")),
                ),
                (_, true) => (
                    format!(" {label} "),
                    kit::bold(kit::style(ctx, "primary", "surface")),
                ),
                (_, false) => (
                    format!(" {label} "),
                    kit::style(ctx, "foreground-muted", "surface"),
                ),
            };
            let w = kit::put(canvas, x, inner.y, &text, style, inner);
            if look == "line" && active && inner.height > 1 {
                let bar: String = std::iter::repeat_n("━", usize::from(w)).collect();
                kit::put(
                    canvas,
                    x,
                    inner.y + 1,
                    &bar,
                    kit::style(ctx, "primary", "surface"),
                    inner,
                );
            }
            x += w + 1;
        }
    }
}

/// `Home › Projects › Glyphforge`, the last item emphasised.
#[derive(Debug)]
pub struct Breadcrumbs;

impl ComponentRenderer for Breadcrumbs {
    fn kind(&self) -> &'static str {
        "breadcrumbs"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("breadcrumbs", "Breadcrumbs", Category::Navigation)
            .summary("Path of locations")
            .keywords(&["path", "location", "trail"])
            .size(None)
            .prop(items_prop(&["Home", "Projects", "Glyphforge"]))
            .prop(PropSpec::new("separator", PropKind::Text, "between items").default("›"))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let sep = painter::text_width(kit::str_or(c, "separator", "›")) + 2;
        let w: u16 = items.iter().map(|i| painter::text_width(i)).sum::<u16>()
            + sep * items.len().saturating_sub(1) as u16;
        Some(Size::new(w, 1))
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
        let sep = format!(" {} ", kit::str_or(c, "separator", "›"));
        let mut x = inner.x;
        for (i, item) in items.iter().enumerate() {
            let last = i + 1 == items.len();
            let style = if last {
                kit::bold(kit::style(ctx, "foreground", "surface"))
            } else {
                kit::style(ctx, "foreground-muted", "surface")
            };
            x += kit::put(canvas, x, inner.y, item, style, inner);
            if !last {
                x += kit::put(
                    canvas,
                    x,
                    inner.y,
                    &sep,
                    kit::style(ctx, "border", "surface"),
                    inner,
                );
            }
        }
    }
}

/// Vertical menu: one item per row, the selected one highlighted, key
/// hints right-aligned; `-` is a separator.
#[derive(Debug)]
pub struct Menu;

impl ComponentRenderer for Menu {
    fn kind(&self) -> &'static str {
        "menu"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("menu", "Menu", Category::Navigation)
            .summary("Vertical list of commands with key hints")
            .keywords(&["commands", "context menu", "dropdown", "actions"])
            .size(None)
            .prop(items_prop(&[
                "Open|Ctrl+O",
                "Save|Ctrl+S",
                "-",
                "Quit|Ctrl+Q",
            ]))
            .prop(selected_prop())
            .prop(PropSpec::new("border", PropKind::Bool, "draw a frame").default(true))
            .prop(kit::bind_prop())
    }

    fn chrome_inset(&self, c: &Component, _: &RenderContext<'_>) -> crate::layout::Edges {
        if c.prop_bool("border") == Some(false) {
            crate::layout::Edges::ZERO
        } else {
            crate::layout::Edges::all(1)
        }
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let w = items
            .iter()
            .map(|i| {
                let (l, k) = split(i);
                painter::text_width(l) + k.map_or(0, |k| painter::text_width(k) + 3) + 2
            })
            .max()
            .unwrap_or(8);
        Some(Size::new(w, items.len() as u16))
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let base = kit::style(ctx, "foreground", "surface");
        painter::fill(canvas, rect, &Cell::blank(base));
        if c.prop_bool("border") != Some(false) {
            painter::draw_border(
                canvas,
                rect,
                ctx.theme.border,
                kit::style(ctx, "border", "surface"),
            );
        }
        let items = kit::strings(c, ctx, "items");
        let current = kit::selected_index(c, &items);
        for (i, item) in items.iter().enumerate().take(usize::from(inner.height)) {
            let y = inner.y + i as u16;
            if item == "-" {
                let rule: String = std::iter::repeat_n("─", usize::from(inner.width)).collect();
                kit::put(
                    canvas,
                    inner.x,
                    y,
                    &rule,
                    kit::style(ctx, "border-muted", "surface"),
                    inner,
                );
                continue;
            }
            let (label, key) = split(item);
            let selected = current == Some(i);
            let style = if selected {
                kit::style(ctx, "selection-foreground", "selection")
            } else {
                base
            };
            painter::fill(
                canvas,
                Rect::new(inner.x, y, inner.width, 1),
                &Cell::blank(style),
            );
            kit::put(canvas, inner.x + 1, y, label, style, inner);
            if let Some(key) = key {
                let kx = inner
                    .right()
                    .saturating_sub(u32::from(painter::text_width(key)) + 1)
                    as u16;
                let ks = if selected {
                    style
                } else {
                    kit::style(ctx, "foreground-muted", "surface")
                };
                kit::put(canvas, kx, y, key, ks, inner);
            }
        }
    }
}

/// A row of buttons; `|` alone separates groups.
#[derive(Debug)]
pub struct Toolbar;

impl ComponentRenderer for Toolbar {
    fn kind(&self) -> &'static str {
        "toolbar"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("toolbar", "Toolbar", Category::Navigation)
            .summary("Row of actions")
            .keywords(&["actions", "buttons", "bar", "ribbon"])
            .size(None)
            .prop(items_prop(&["New", "Open", "Save", "|", "Run"]))
            .prop(selected_prop())
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let items = kit::strings(c, ctx, "items");
        let w: u16 = items
            .iter()
            .map(|i| painter::text_width(split(i).0) + 3)
            .sum();
        Some(Size::new(w, 1))
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        painter::fill(
            canvas,
            Rect::new(rect.x, rect.y, rect.width, 1),
            &Cell::blank(kit::style(ctx, "foreground", "surface-alt")),
        );
        let items = kit::strings(c, ctx, "items");
        let current = c
            .prop("selected")
            .and_then(|_| kit::selected_index(c, &items));
        let mut x = inner.x;
        for (i, item) in items.iter().enumerate() {
            if item == "|" {
                x += kit::put(
                    canvas,
                    x,
                    inner.y,
                    "│ ",
                    kit::style(ctx, "border", "surface-alt"),
                    inner,
                );
                continue;
            }
            let (label, key) = split(item);
            let style = if current == Some(i) {
                kit::style(ctx, "selection-foreground", "primary")
            } else {
                kit::style(ctx, "foreground", "surface-alt")
            };
            let text = match key {
                Some(k) => format!(" {label} {k} "),
                None => format!(" {label} "),
            };
            x += kit::put(canvas, x, inner.y, &text, style, inner) + 1;
        }
    }
}

/// Bottom status bar: segments on the left, `right` text on the right.
#[derive(Debug)]
pub struct StatusBar;

impl ComponentRenderer for StatusBar {
    fn kind(&self) -> &'static str {
        "statusbar"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("statusbar", "Status Bar", Category::Navigation)
            .summary("Segments of status along the bottom")
            .keywords(&["footer", "status line", "modeline"])
            .size(Some((40, 1)))
            .prop(items_prop(&["NORMAL", "main.rs", "utf-8"]))
            .prop(
                PropSpec::new("right", PropKind::Text, "text on the right").initial("Ln 1, Col 1"),
            )
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let bar = kit::style(ctx, "foreground", "surface-alt");
        painter::fill(
            canvas,
            Rect::new(rect.x, rect.y, rect.width, 1),
            &Cell::blank(bar),
        );
        let mut x = inner.x;
        for (i, item) in kit::strings(c, ctx, "items").iter().enumerate() {
            let style = if i == 0 {
                kit::bold(kit::style(ctx, "selection-foreground", "primary"))
            } else {
                bar
            };
            x += kit::put(canvas, x, inner.y, &format!(" {item} "), style, inner);
            x += kit::put(
                canvas,
                x,
                inner.y,
                "│",
                kit::style(ctx, "border-muted", "surface-alt"),
                inner,
            );
        }
        if let Some(right) = c.prop_str("right") {
            let w = painter::text_width(right) + 1;
            let rx = inner.right().saturating_sub(u32::from(w)) as u16;
            if rx > x {
                kit::put(
                    canvas,
                    rx,
                    inner.y,
                    right,
                    kit::style(ctx, "foreground-muted", "surface-alt"),
                    inner,
                );
            }
        }
    }
}
