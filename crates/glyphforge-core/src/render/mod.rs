//! Rendering: semantic components and artwork layers to a cell canvas.

pub mod components;
pub mod painter;

use crate::component::Component;
use crate::document::{Canvas, CanvasError, Rect, Screen, Size};
use crate::layout::{self, LayoutResult, Measure};
use crate::theme::Theme;

pub use components::{ComponentRenderer, Registry};

#[cfg(test)]
mod artwork_tests {
    use super::*;
    use crate::document::{Cell, CellStyle, Document, Grapheme, Position};
    use crate::id;
    use crate::layout::Layout;
    use crate::patch::{Operation, Patch};

    #[test]
    fn artwork_component_places_a_layer_region_by_layout() {
        let mut doc = Document::new(Size::new(20, 4)).unwrap();
        let art = doc
            .layer_mut(&id!("main-artwork"))
            .unwrap()
            .cells_mut()
            .unwrap();
        for (i, ch) in "LOGO".chars().enumerate() {
            art.put(
                Position::new(i as u16, 0),
                Cell::glyph(Grapheme::from_char(ch).unwrap(), CellStyle::DEFAULT),
            )
            .unwrap();
        }
        // The source layer is hidden: it is an asset, not part of the screen.
        Patch::new(vec![
            Operation::UpdateLayer {
                target: id!("main-artwork"),
                name: None,
                visible: Some(false),
                locked: None,
            },
            Operation::CreateComponent {
                component: Component::new(id!("logo"), "artwork")
                    .with_prop("layer", "main-artwork")
                    .with_prop("width", 4i64)
                    .with_prop("height", 1i64)
                    .with_layout(Layout::absolute(10, 2, 4, 1)),
                parent: None,
                layer: None,
                index: None,
            },
        ])
        .apply(&mut doc)
        .unwrap();
        let canvas =
            render_screen(doc.first_screen(), &doc.theme, &Registry::builtin(), None).unwrap();
        let lines = to_text_lines(&canvas);
        assert_eq!(lines[0], "", "hidden source layer is not drawn");
        assert_eq!(lines[2], "          LOGO");
    }
}

/// What renderers may consult: the theme and, when rendering inside a
/// screen, the screen itself (for artwork references).
#[derive(Debug, Clone, Copy)]
pub struct RenderContext<'a> {
    pub theme: &'a Theme,
    pub screen: Option<&'a Screen>,
}

impl<'a> RenderContext<'a> {
    pub const fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            screen: None,
        }
    }

    /// The cells of an artwork layer of the current screen, if any.
    pub fn artwork(&self, layer: &crate::id::ObjectId) -> Option<&'a Canvas> {
        self.screen
            .and_then(|s| s.layer(layer))
            .and_then(crate::document::Layer::cells)
    }
}

/// The measure the layout solver uses: it asks the renderer registry.
#[derive(Debug, Clone, Copy)]
pub struct RegistryMeasure<'a> {
    pub registry: &'a Registry,
    pub ctx: RenderContext<'a>,
}

impl Measure for RegistryMeasure<'_> {
    fn intrinsic_size(&self, component: &Component) -> Option<Size> {
        self.registry
            .renderer(&component.kind)
            .intrinsic_size(component, &self.ctx)
    }

    fn chrome_inset(&self, component: &Component) -> layout::Edges {
        self.registry
            .renderer(&component.kind)
            .chrome_inset(component, &self.ctx)
    }
}

/// Lays out and renders one interface layer's components into a fresh
/// transparent canvas of `size`. Responsive rules are resolved for the
/// width first. Parents are painted before children.
pub fn render_components(
    roots: &[Component],
    size: Size,
    ctx: &RenderContext<'_>,
    registry: &Registry,
) -> Result<(Canvas, LayoutResult), CanvasError> {
    let mut canvas = Canvas::new(size)?;
    let resolved: Vec<Component> = roots
        .iter()
        .filter_map(|c| c.resolve_responsive(size.width))
        .collect();
    let measure = RegistryMeasure {
        registry,
        ctx: *ctx,
    };
    let result = layout::solve(&resolved, Rect::from_size(size), &measure);
    for c in resolved.iter().flat_map(Component::iter) {
        let (Some(rect), Some(inner)) = (result.rect(&c.id), result.inner(&c.id)) else {
            continue;
        };
        if rect.is_empty() {
            continue;
        }
        registry
            .renderer(&c.kind)
            .render(c, rect, inner, ctx, &mut canvas);
    }
    Ok((canvas, result))
}

/// Renders a screen: visible layers bottom to top, artwork composited as
/// is, interface layers rendered through the registry. `size` overrides
/// the screen size for responsive previews (artwork is clipped).
pub fn render_screen(
    screen: &Screen,
    theme: &Theme,
    registry: &Registry,
    size: Option<Size>,
) -> Result<Canvas, CanvasError> {
    let size = size.unwrap_or(screen.size());
    let ctx = RenderContext {
        theme,
        screen: Some(screen),
    };
    let mut out = Canvas::new(size)?;
    for layer in screen.layers().iter().filter(|l| l.visible) {
        if let Some(cells) = layer.cells() {
            out.composite_over(cells);
        } else if let Some(roots) = layer.components() {
            let (canvas, _) = render_components(roots, size, &ctx, registry)?;
            out.composite_over(&canvas);
        }
    }
    Ok(out)
}

/// Plain-text lines of a canvas (transparent cells become spaces, trailing
/// spaces trimmed). Useful for previews, tests and agents.
pub fn to_text_lines(canvas: &Canvas) -> Vec<String> {
    (0..canvas.height())
        .filter_map(|y| canvas.row(y))
        .map(|row| {
            let line: String = row
                .iter()
                .map(|c| match &c.content {
                    crate::document::CellContent::Glyph(g) => g.as_str(),
                    crate::document::CellContent::Empty => " ",
                    crate::document::CellContent::WideTail => "",
                })
                .collect();
            line.trim_end().to_owned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cell, CellStyle, Document, Grapheme, Position};
    use crate::id;
    use crate::layout::Layout;

    #[test]
    fn screen_renders_artwork_under_components() {
        let mut doc = Document::new(Size::new(12, 4)).unwrap();
        let art = doc
            .layer_mut(&id!("main-artwork"))
            .unwrap()
            .cells_mut()
            .unwrap();
        for (i, ch) in "ARTWORK".chars().enumerate() {
            art.put(
                Position::new(i as u16, 3),
                Cell::glyph(Grapheme::from_char(ch).unwrap(), CellStyle::DEFAULT),
            )
            .unwrap();
        }
        let ui = doc
            .layer_mut(&id!("main-ui"))
            .unwrap()
            .components_mut()
            .unwrap();
        ui.push(
            Component::new(id!("p"), "panel")
                .with_prop("title", "Hi")
                .with_layout(Layout::absolute(0, 0, 8, 3)),
        );
        let canvas =
            render_screen(doc.first_screen(), &doc.theme, &Registry::builtin(), None).unwrap();
        let lines = to_text_lines(&canvas);
        assert_eq!(lines[0], "╭ Hi ──╮");
        assert_eq!(lines[1], "│      │");
        assert_eq!(lines[2], "╰──────╯");
        assert_eq!(lines[3], "ARTWORK");
    }

    #[test]
    fn size_override_previews_responsive_rules() {
        let mut doc = Document::new(Size::new(40, 3)).unwrap();
        let ui = doc
            .layer_mut(&id!("main-ui"))
            .unwrap()
            .components_mut()
            .unwrap();
        let mut label = Component::new(id!("l"), "label").with_prop("text", "wide");
        label.responsive.push(crate::component::ResponsiveRule {
            max_width: Some(20),
            hide: true,
            ..Default::default()
        });
        ui.push(label);
        let full =
            render_screen(doc.first_screen(), &doc.theme, &Registry::builtin(), None).unwrap();
        assert_eq!(to_text_lines(&full)[0], "wide");
        let narrow = render_screen(
            doc.first_screen(),
            &doc.theme,
            &Registry::builtin(),
            Some(Size::new(10, 3)),
        )
        .unwrap();
        assert_eq!(to_text_lines(&narrow)[0], "");
    }
}
