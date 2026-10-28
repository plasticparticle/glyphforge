//! Component renderers and their registry.
//!
//! Adding a component kind means adding a renderer struct and registering
//! it in [`Registry::builtin`]; nothing else changes.

use std::collections::BTreeMap;

use crate::boxdraw::BorderFamily;
use crate::component::Component;
use crate::document::{Attributes, Canvas, Cell, CellStyle, Color, Rect, Size};
use crate::layout::Edges;
use crate::render::descriptor::{
    Category, Descriptor, PropKind, PropSpec, align_prop, state_props, style_props,
};
use crate::render::{RenderContext, painter};
use crate::theme::Theme;

/// Renders one kind of component.
pub trait ComponentRenderer: std::fmt::Debug + Send + Sync {
    fn kind(&self) -> &'static str;

    /// Natural content size without chrome, if any.
    fn intrinsic_size(&self, _component: &Component, _ctx: &RenderContext<'_>) -> Option<Size> {
        None
    }

    /// What this kind is: title, category, properties and defaults.
    fn descriptor(&self) -> Descriptor {
        Descriptor::new(self.kind(), self.kind(), Category::Layout)
    }

    /// A sensible size for a freshly created instance placed absolutely,
    /// or `None` when the kind sizes itself to its content.
    fn default_size(&self) -> Option<Size> {
        self.descriptor().default_size()
    }

    /// Properties a new instance starts with, so it is visible at once.
    fn default_props(&self) -> Vec<(&'static str, crate::value::Value)> {
        self.descriptor().initial_props()
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
        r.register(Box::new(Instance));
        super::kinds::register(&mut r);
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

    /// Descriptors of every registered kind, by category then title.
    pub fn descriptors(&self) -> Vec<Descriptor> {
        let mut all: Vec<Descriptor> = self.renderers.values().map(|r| r.descriptor()).collect();
        all.sort_by(|a, b| a.category.cmp(&b.category).then(a.title.cmp(b.title)));
        all
    }

    /// The descriptor of a registered kind.
    pub fn descriptor(&self, kind: &str) -> Option<Descriptor> {
        self.renderers.get(kind).map(|r| r.descriptor())
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("group", "Group", Category::Layout)
            .summary("Invisible container that arranges its children")
            .keywords(&["container", "box", "row", "column", "stack", "div"])
            .container()
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("panel", "Panel", Category::Layout)
            .summary("Bordered surface with an optional title")
            .keywords(&["frame", "box", "card", "window", "section"])
            .container()
            .prop(
                PropSpec::new("title", PropKind::Text, "caption in the top border")
                    .initial("Panel"),
            )
            .prop(PropSpec::new(
                "border",
                PropKind::Border,
                "border family (theme default)",
            ))
            .prop(PropSpec::new("border_color", PropKind::Color, "border colour").default("border"))
            .props(style_props())
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("label", "Label", Category::Text)
            .summary("Single line of text")
            .keywords(&["text", "caption", "string"])
            .size(None)
            .prop(PropSpec::new("text", PropKind::Text, "the text").initial("Label"))
            .prop(align_prop())
            .props(style_props())
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("heading", "Heading", Category::Text)
            .summary("Bold single-line title in the primary colour")
            .keywords(&["title", "header", "h1"])
            .size(None)
            .prop(PropSpec::new("text", PropKind::Text, "the text").initial("Heading"))
            .prop(align_prop())
            .props(style_props())
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("button", "Button", Category::Input)
            .summary("Pressable action `[ Label ]`")
            .keywords(&["btn", "action", "submit", "ok", "cancel"])
            .size(None)
            .prop(PropSpec::new("label", PropKind::Text, "caption").initial("OK"))
            .prop(PropSpec::new(
                "primary",
                PropKind::Bool,
                "emphasised default action",
            ))
            .prop(align_prop())
            .props(state_props())
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

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("divider", "Divider", Category::Layout)
            .summary("Horizontal rule")
            .keywords(&["separator", "rule", "line", "hr"])
            .size(Some((10, 1)))
            .prop(PropSpec::new(
                "border",
                PropKind::Border,
                "line family (theme default)",
            ))
            .prop(PropSpec::new("color", PropKind::Color, "line colour").default("border-muted"))
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

    fn descriptor(&self) -> Descriptor {
        let region = |name, help| PropSpec::new(name, PropKind::int(0, i64::from(u16::MAX)), help);
        Descriptor::new("artwork", "Artwork", Category::Media)
            .summary("Region of an artwork or pixel layer placed by layout")
            .keywords(&["image", "picture", "logo", "art", "raster", "pixels"])
            .size(None)
            .prop(PropSpec::new(
                "layer",
                PropKind::Reference,
                "source layer id",
            ))
            .prop(region("x", "source column"))
            .prop(region("y", "source row"))
            .prop(region("width", "source width (whole layer)"))
            .prop(region("height", "source height (whole layer)"))
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

/// The built-in registry, built once (instances render their master
/// through it).
fn builtin_registry() -> &'static Registry {
    static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(Registry::builtin)
}

/// Shows a symbol: the master expanded with this instance's overrides,
/// laid out inside the instance's rect.
#[derive(Debug)]
pub struct Instance;

impl ComponentRenderer for Instance {
    fn kind(&self) -> &'static str {
        "instance"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("instance", "Symbol Instance", Category::Layout)
            .summary("A copy of a symbol that follows its master")
            .keywords(&["symbol", "component", "reuse", "master", "copy"])
            .size(None)
            .prop(PropSpec::new("symbol", PropKind::Reference, "symbol id"))
            .prop(PropSpec::new(
                "overrides",
                PropKind::Map,
                "\"<child id>.<property>\" -> value for nested components",
            ))
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let id = c.prop_str("symbol")?;
        let master = &ctx.symbols.iter().find(|s| s.id.as_str() == id)?.component;
        let dim = |d: crate::layout::Dimension| match d {
            crate::layout::Dimension::Fixed(n) => Some(n),
            _ => None,
        };
        Some(Size::new(
            dim(master.layout.width).unwrap_or(20),
            dim(master.layout.height).unwrap_or(4),
        ))
    }

    fn render(
        &self,
        c: &Component,
        rect: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let root = (ctx.depth < 8)
            .then(|| crate::symbol::expand(c, ctx.symbols))
            .flatten();
        let Some(root) = root else {
            Placeholder.render(c, rect, inner, ctx, canvas);
            return;
        };
        let nested = RenderContext {
            depth: ctx.depth + 1,
            ..*ctx
        };
        let size = Size::new(inner.width, inner.height);
        let registry = builtin_registry();
        let Ok((art, _)) = crate::render::render_components(&[root], size, &nested, registry)
        else {
            return;
        };
        for pos in art.bounds().positions() {
            let Some(cell) = art.get(pos) else { continue };
            if cell.is_empty() || cell.is_wide_tail() {
                continue;
            }
            let target = crate::document::Position::new(inner.x + pos.x, inner.y + pos.y);
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
            vec![Component::new(id!("t"), "gizmo").with_layout(Layout::absolute(0, 0, 8, 3))];
        let lines = render(&roots, 8, 3);
        assert_eq!(lines[0], "+?gizmo+");
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

    #[test]
    fn artwork_component_shows_a_pixel_layer() {
        use crate::document::{Layer, Screen};
        use crate::raster::{Pixel, RasterMode};
        let mut screen = Screen::new(id!("s"), "S", Size::new(6, 2)).unwrap();
        let mut px = Layer::raster_layer(id!("px"), "Px", Size::new(3, 1), RasterMode::Half);
        px.visible = false;
        px.raster_mut()
            .unwrap()
            .pixels
            .set(0, 0, Pixel::Solid(Color::Rgb(255, 0, 0)));
        screen.layers_mut().push(px);
        let theme = crate::theme::Theme::terminal();
        let ctx = RenderContext {
            theme: &theme,
            screen: Some(&screen),
            data: None,
            symbols: &[],
            depth: 0,
        };
        let art = Component::new(id!("logo"), "artwork")
            .with_prop("layer", "px")
            .with_layout(Layout::absolute(2, 0, 3, 1));
        let (canvas, _) =
            render_components(&[art], Size::new(6, 2), &ctx, &Registry::builtin()).unwrap();
        let cell = canvas.get(crate::document::Position::new(2, 0)).unwrap();
        assert_eq!(
            cell.as_glyph().map(crate::document::Grapheme::as_str),
            Some("▀")
        );
        assert_eq!(cell.style.fg, Color::Rgb(255, 0, 0));
    }
}
