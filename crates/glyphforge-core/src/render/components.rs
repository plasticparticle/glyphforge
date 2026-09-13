//! Component renderers and their registry.
//!
//! Adding a component kind means adding a renderer struct and registering
//! it in [`Registry::builtin`]; nothing else changes.

use std::collections::BTreeMap;

use crate::boxdraw::BorderFamily;
use crate::component::Component;
use crate::document::{Attributes, Canvas, Cell, CellStyle, Color, Rect, Size};
use crate::layout::Edges;
use crate::render::{RenderContext, painter};
use crate::theme::Theme;

/// Renders one kind of component.
pub trait ComponentRenderer: std::fmt::Debug + Send + Sync {
    fn kind(&self) -> &'static str;

    /// Natural content size without chrome, if any.
    fn intrinsic_size(&self, _component: &Component, _ctx: &RenderContext<'_>) -> Option<Size> {
        None
    }

    /// A sensible size for a freshly created instance placed absolutely,
    /// or `None` when the kind sizes itself to its content.
    fn default_size(&self) -> Option<Size> {
        Some(Size::new(20, 6))
    }

    /// Properties a new instance starts with, so it is visible at once.
    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        Vec::new()
    }

    /// Cells consumed by borders and similar chrome.
    fn chrome_inset(&self, _component: &Component, _ctx: &RenderContext<'_>) -> Edges {
        Edges::ZERO
    }

    /// Paints the component into `canvas`. `rect` is the outer rect,
    /// `inner` the content rect (outer minus chrome and padding).
    fn render(
        &self,
        component: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    );
}

#[derive(Debug)]
pub struct Registry {
    renderers: BTreeMap<&'static str, Box<dyn ComponentRenderer>>,
    fallback: Placeholder,
}

impl Registry {
    pub fn empty() -> Self {
        Self {
            renderers: BTreeMap::new(),
            fallback: Placeholder,
        }
    }

    /// The renderers shipped with Glyphforge.
    pub fn builtin() -> Self {
        let mut r = Self::empty();
        r.register(Box::new(Group));
        r.register(Box::new(Panel));
        r.register(Box::new(Label));
        r.register(Box::new(Heading));
        r.register(Box::new(Button));
        r.register(Box::new(Divider));
        r.register(Box::new(Artwork));
        r
    }

    pub fn register(&mut self, renderer: Box<dyn ComponentRenderer>) {
        self.renderers.insert(renderer.kind(), renderer);
    }

    pub fn knows(&self, kind: &str) -> bool {
        self.renderers.contains_key(kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.renderers.keys().copied()
    }

    /// The renderer for `kind`, or the placeholder for unknown kinds.
    pub fn renderer(&self, kind: &str) -> &dyn ComponentRenderer {
        self.renderers
            .get(kind)
            .map_or(&self.fallback as &dyn ComponentRenderer, |r| r.as_ref())
    }
}

/// Style helpers shared by renderers.
fn style_of(c: &Component, theme: &Theme, fg_default: &str, bg_default: &str) -> CellStyle {
    let fg = c
        .prop_str("color")
        .map_or_else(|| theme.color(fg_default), |t| theme.resolve(t));
    let bg = c
        .prop_str("background")
        .map_or_else(|| theme.color(bg_default), |t| theme.resolve(t));
    let mut attrs = Attributes::NONE;
    if c.prop_bool("bold") == Some(true) {
        attrs.bold = true;
    }
    if c.prop_bool("dim") == Some(true) {
        attrs.dim = true;
    }
    if c.prop_bool("italic") == Some(true) {
        attrs.italic = true;
    }
    if c.prop_bool("underline") == Some(true) {
        attrs.underline = true;
    }
    CellStyle { fg, bg, attrs }
}

fn border_of(c: &Component, theme: &Theme) -> BorderFamily {
    c.prop_str("border")
        .and_then(BorderFamily::parse)
        .unwrap_or(theme.border)
}

fn text_of(c: &Component) -> &str {
    c.prop_str("text")
        .or_else(|| c.prop_str("label"))
        .unwrap_or("")
}

fn align_x(c: &Component, text_width: u16, inner: Rect) -> u16 {
    let free = inner.width.saturating_sub(text_width);
    match c.prop_str("align") {
        Some("center") => inner.x + free / 2,
        Some("right") => inner.x + free,
        _ => inner.x,
    }
}

/// Invisible container.
#[derive(Debug)]
pub struct Group;

impl ComponentRenderer for Group {
    fn kind(&self) -> &'static str {
        "group"
    }

    fn render(&self, _: &Component, _: Rect, _: Rect, _: &RenderContext<'_>, _: &mut Canvas) {}
}

/// A bordered surface with an optional title.
///
/// Props: `title`, `border` (family), `color`, `background` (token or
/// literal, defaults `foreground`/`surface`), `border_color`.
#[derive(Debug)]
pub struct Panel;

impl ComponentRenderer for Panel {
    fn kind(&self) -> &'static str {
        "panel"
    }

    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        vec![("title", "Panel".into())]
    }

    fn chrome_inset(&self, c: &Component, ctx: &RenderContext<'_>) -> Edges {
        match border_of(c, ctx.theme) {
            BorderFamily::None => Edges::ZERO,
            BorderFamily::Minimal => Edges::symmetric(1, 0),
            _ => Edges::all(1),
        }
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        _inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let style = style_of(c, theme, "foreground", "surface");
        painter::fill(canvas, rect, &Cell::blank(style));
        let family = border_of(c, theme);
        let border_style = CellStyle {
            fg: c
                .prop_str("border_color")
                .map_or_else(|| theme.color("border"), |t| theme.resolve(t)),
            ..style
        };
        painter::draw_border(canvas, rect, family, border_style);
        if let Some(title) = c.prop_str("title").filter(|t| !t.is_empty()) {
            if family.is_visible() && rect.width > 4 {
                let max = rect.width - 4;
                let text = format!(" {} ", painter::truncate(title, max));
                let title_style = CellStyle {
                    attrs: Attributes {
                        bold: true,
                        ..Attributes::NONE
                    },
                    ..style
                };
                painter::draw_text(canvas, rect.x + 1, rect.y, &text, title_style, rect);
            }
        }
    }
}

/// Single-line text. Props: `text`, `color`, `background`, `align`
/// (`left`|`center`|`right`), `bold`, `dim`, `italic`, `underline`.
#[derive(Debug)]
pub struct Label;

impl ComponentRenderer for Label {
    fn kind(&self) -> &'static str {
        "label"
    }

    fn default_size(&self) -> Option<Size> {
        None
    }

    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        vec![("text", "Label".into())]
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(painter::text_width(text_of(c)), 1))
    }

    fn render(
        &self,
        c: &Component,
        _rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let style = style_of(c, theme, "foreground", "surface");
        let text = painter::truncate(text_of(c), inner.width);
        let x = align_x(c, painter::text_width(&text), inner);
        painter::draw_text(canvas, x, inner.y, &text, style, inner);
    }
}

/// Emphasised single-line text in the primary colour.
#[derive(Debug)]
pub struct Heading;

impl ComponentRenderer for Heading {
    fn kind(&self) -> &'static str {
        "heading"
    }

    fn default_size(&self) -> Option<Size> {
        None
    }

    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        vec![("text", "Heading".into())]
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(painter::text_width(text_of(c)), 1))
    }

    fn render(
        &self,
        c: &Component,
        _rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let mut style = style_of(c, theme, "primary", "surface");
        style.attrs.bold = true;
        let text = painter::truncate(text_of(c), inner.width);
        let x = align_x(c, painter::text_width(&text), inner);
        painter::draw_text(canvas, x, inner.y, &text, style, inner);
    }
}

/// `[ Label ]`. Props: `label`, `primary` (bool), `disabled` (bool),
/// `focused` (bool, preview state).
#[derive(Debug)]
pub struct Button;

impl ComponentRenderer for Button {
    fn kind(&self) -> &'static str {
        "button"
    }

    fn default_size(&self) -> Option<Size> {
        None
    }

    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        vec![("label", "OK".into())]
    }

    fn intrinsic_size(&self, c: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(
            painter::text_width(text_of(c)).saturating_add(4),
            1,
        ))
    }

    fn render(
        &self,
        c: &Component,
        _rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let primary = c.prop_bool("primary") == Some(true);
        let disabled = c.prop_bool("disabled") == Some(true);
        let focused = c.prop_bool("focused") == Some(true);
        let mut style = if primary {
            CellStyle::new(theme.color("selection-foreground"), theme.color("primary"))
        } else {
            CellStyle::new(theme.color("foreground"), theme.color("surface-alt"))
        };
        if disabled {
            style.fg = theme.color("foreground-muted");
            style.attrs.dim = true;
        }
        if focused {
            style.attrs.bold = true;
            style.attrs.underline = true;
        }
        let label = painter::truncate(text_of(c), inner.width.saturating_sub(4));
        let text = format!("[ {label} ]");
        let x = align_x(c, painter::text_width(&text), inner);
        painter::draw_text(canvas, x, inner.y, &text, style, inner);
    }
}

/// A horizontal rule. Props: `border` (family), `color`.
#[derive(Debug)]
pub struct Divider;

impl ComponentRenderer for Divider {
    fn kind(&self) -> &'static str {
        "divider"
    }

    fn default_size(&self) -> Option<Size> {
        Some(Size::new(10, 1))
    }

    fn intrinsic_size(&self, _: &Component, _: &RenderContext<'_>) -> Option<Size> {
        Some(Size::new(0, 1))
    }

    fn render(
        &self,
        c: &Component,
        _rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let style = style_of(c, theme, "border-muted", "surface");
        let glyph = border_of(c, theme).glyphs().horizontal;
        let line: String = std::iter::repeat_n(glyph, usize::from(inner.width)).collect();
        painter::draw_text(canvas, inner.x, inner.y, &line, style, inner);
    }
}

/// A region of an artwork layer placed by layout: the bridge between
/// Interface Mode and Subcell Mode.
///
/// Props: `layer` (artwork layer id), `x`, `y`, `width`, `height` (source
/// region; defaults to the whole layer). The source layer is usually
/// hidden so it acts as an asset rather than screen content.
#[derive(Debug)]
pub struct Artwork;

impl Artwork {
    fn source(c: &Component, ctx: &RenderContext<'_>) -> Option<(Rect, &'static str)> {
        let layer = c.prop_str("layer")?;
        let id = crate::id::ObjectId::new(layer).ok()?;
        let canvas = ctx.artwork(&id)?;
        let num = |k: &str| c.prop_int(k).and_then(|v| u16::try_from(v).ok());
        let x = num("x").unwrap_or(0);
        let y = num("y").unwrap_or(0);
        let width = num("width").unwrap_or(canvas.width().saturating_sub(x));
        let height = num("height").unwrap_or(canvas.height().saturating_sub(y));
        Some((
            Rect::new(x, y, width, height).intersection(canvas.bounds()),
            "",
        ))
    }
}

impl ComponentRenderer for Artwork {
    fn kind(&self) -> &'static str {
        "artwork"
    }

    fn default_size(&self) -> Option<Size> {
        None
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        Self::source(c, ctx).map(|(r, _)| Size::new(r.width, r.height))
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let Some(layer) = c
            .prop_str("layer")
            .and_then(|l| crate::id::ObjectId::new(l).ok())
        else {
            Placeholder.render(c, rect, inner, ctx, canvas);
            return;
        };
        let (Some(source), Some((region, _))) = (ctx.artwork(&layer), Self::source(c, ctx)) else {
            Placeholder.render(c, rect, inner, ctx, canvas);
            return;
        };
        let clip = inner.intersection(canvas.bounds());
        for pos in region.positions() {
            let Some(cell) = source.get(pos) else {
                continue;
            };
            if cell.is_empty() || cell.is_wide_tail() {
                continue;
            }
            let target = crate::document::Position::new(
                inner.x.saturating_add(pos.x - region.x),
                inner.y.saturating_add(pos.y - region.y),
            );
            let w = u16::from(cell.width());
            if !clip.contains(target) || u32::from(target.x) + u32::from(w) > clip.right() {
                continue;
            }
            let _ = canvas.put(target, cell.clone());
        }
    }
}

/// Renders unknown kinds as a labelled outline so nothing disappears
/// silently.
#[derive(Debug)]
pub struct Placeholder;

impl ComponentRenderer for Placeholder {
    fn kind(&self) -> &'static str {
        "?"
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        _inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let theme = ctx.theme;
        let style = CellStyle {
            fg: theme.color("warning"),
            bg: Color::Default,
            attrs: Attributes {
                dim: true,
                ..Attributes::NONE
            },
        };
        painter::draw_border(canvas, rect, BorderFamily::Ascii, style);
        let text = painter::truncate(&format!("?{}", c.kind), rect.width.saturating_sub(2));
        painter::draw_text(canvas, rect.x + 1, rect.y, &text, style, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;
    use crate::layout::{Direction, Layout};
    use crate::render::{render_components, to_text_lines};

    fn render(roots: &[Component], w: u16, h: u16) -> Vec<String> {
        let theme = crate::theme::Theme::terminal();
        let ctx = RenderContext::new(&theme);
        let (canvas, _) =
            render_components(roots, Size::new(w, h), &ctx, &Registry::builtin()).unwrap();
        to_text_lines(&canvas)
    }

    #[test]
    fn panel_with_title_and_children_in_vertical_flow() {
        let panel = Component::new(id!("p"), "panel")
            .with_prop("title", "Metrics")
            .with_prop("border", "single")
            .with_layout(Layout::absolute(0, 0, 14, 5).with_direction(Direction::Vertical))
            .with_children(vec![
                Component::new(id!("h"), "heading")
                    .with_prop("text", "CPU")
                    .with_layout(Layout {
                        height: crate::layout::Dimension::Content,
                        ..Layout::default()
                    }),
                Component::new(id!("l"), "label")
                    .with_prop("text", "72 percent")
                    .with_prop("align", "right"),
            ]);
        let lines = render(&[panel], 14, 5);
        assert_eq!(lines[0], "┌ Metrics ───┐");
        assert_eq!(lines[1], "│CPU         │");
        assert_eq!(lines[2], "│  72 percent│");
        assert_eq!(lines[4], "└────────────┘");
    }

    #[test]
    fn long_title_is_truncated_with_ellipsis() {
        let panel = Component::new(id!("p"), "panel")
            .with_prop("title", "Request latency percentile")
            .with_layout(Layout::absolute(0, 0, 12, 3));
        let lines = render(&[panel], 12, 3);
        assert_eq!(lines[0], "╭ Request… ╮");
    }

    #[test]
    fn button_and_divider() {
        let roots = vec![
            Component::new(id!("b"), "button")
                .with_prop("label", "OK")
                .with_layout(Layout::absolute(0, 0, 10, 1)),
            Component::new(id!("d"), "divider").with_layout(Layout::absolute(0, 1, 6, 1)),
        ];
        let lines = render(&roots, 10, 2);
        assert_eq!(lines[0], "[ OK ]");
        assert_eq!(lines[1], "──────");
    }

    #[test]
    fn unknown_kind_gets_a_placeholder() {
        let roots =
            vec![Component::new(id!("t"), "table").with_layout(Layout::absolute(0, 0, 8, 3))];
        let lines = render(&roots, 8, 3);
        assert_eq!(lines[0], "+?table+");
        assert_eq!(lines[2], "+------+");
    }

    #[test]
    fn horizontal_layout_of_sidebar_and_content_panels() {
        let root = Component::new(id!("window"), "group")
            .with_layout(Layout::default().with_direction(Direction::Horizontal))
            .with_children(vec![
                Component::new(id!("sidebar"), "panel")
                    .with_prop("title", "Nav")
                    .with_layout(Layout {
                        width: crate::layout::Dimension::Fixed(8),
                        ..Layout::default()
                    }),
                Component::new(id!("content"), "panel").with_prop("title", "Main"),
            ]);
        let lines = render(&[root], 20, 3);
        assert_eq!(lines[0], "╭ Nav ─╮╭ Main ────╮");
        assert_eq!(lines[2], "╰──────╯╰──────────╯");
    }
}
