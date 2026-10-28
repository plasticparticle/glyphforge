//! Deterministic layout validation. Heuristic design suggestions are a
//! separate, later concern and never mixed into these results.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::component::Component;
use crate::document::{Document, Rect, Screen, Size};
use crate::id::ObjectId;
use crate::layout::{self, Dimension, Placement};
use crate::render::{Registry, RegistryMeasure, RenderContext, painter};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable code such as `duplicate-id`.
    pub code: &'static str,
    pub screen: Option<ObjectId>,
    pub target: Option<ObjectId>,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{sev}: ")?;
        if let Some(t) = &self.target {
            write!(f, "{t}: ")?;
        }
        write!(f, "{} [{}]", self.message, self.code)
    }
}

/// Validates every screen at its own size.
pub fn validate(doc: &Document, registry: &Registry) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for id in doc.duplicate_ids() {
        out.push(Diagnostic {
            severity: Severity::Error,
            code: "duplicate-id",
            screen: None,
            target: Some(id.clone()),
            message: format!("id {id} is used more than once"),
        });
    }
    if doc.theme.missing_tokens().len() > 4 {
        out.push(Diagnostic {
            severity: Severity::Warning,
            code: "theme-tokens",
            screen: None,
            target: Some(doc.theme.id.clone()),
            message: format!(
                "theme defines few tokens; missing {}",
                doc.theme.missing_tokens().join(", ")
            ),
        });
    }
    for (link, action) in crate::prototype::broken_links(doc) {
        out.push(Diagnostic {
            severity: Severity::Warning,
            code: "broken-link",
            screen: Some(link.screen),
            target: Some(link.component),
            message: format!("on_activate: {} points at nothing", action.describe()),
        });
    }
    for screen in doc.screens() {
        out.extend(validate_props(screen, registry));
        out.extend(validate_screen(screen, &doc.theme, registry, screen.size()));
    }
    out
}

/// Properties whose value does not fit the kind's descriptor (a string
/// where a number belongs, an unknown choice). Independent of size.
fn validate_props(screen: &Screen, registry: &Registry) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let components = screen
        .layers()
        .iter()
        .filter_map(|l| l.components())
        .flat_map(|roots| roots.iter().flat_map(Component::iter));
    for c in components {
        let Some(descriptor) = registry.descriptor(&c.kind) else {
            continue;
        };
        for (name, value) in &c.props {
            let Some(spec) = descriptor.prop_spec(name) else {
                continue;
            };
            if !spec.kind.accepts(value) {
                out.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "prop-type",
                    screen: Some(screen.id.clone()),
                    target: Some(c.id.clone()),
                    message: format!(
                        "{name} = {} does not fit {} ({})",
                        serde_json::to_string(value).unwrap_or_default(),
                        spec.kind.label(),
                        spec.help
                    ),
                });
            }
        }
    }
    out
}

/// Validates one screen at `size` (which may differ from its own size for
/// responsive checks).
pub fn validate_screen(
    screen: &Screen,
    theme: &Theme,
    registry: &Registry,
    size: Size,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let area = Rect::from_size(size);
    let measure = RegistryMeasure {
        registry,
        ctx: RenderContext {
            theme,
            screen: Some(screen),
            data: None,
            symbols: &[],
            depth: 0,
        },
    };
    for layer in screen.layers().iter().filter(|l| l.visible) {
        let Some(roots) = layer.components() else {
            continue;
        };
        let resolved: Vec<Component> = roots
            .iter()
            .filter_map(|c| c.resolve_responsive(size.width))
            .collect();
        let result = layout::solve(&resolved, area, &measure);
        let diag = |severity, code, target: &ObjectId, message: String| Diagnostic {
            severity,
            code,
            screen: Some(screen.id.clone()),
            target: Some(target.clone()),
            message,
        };
        for c in resolved.iter().flat_map(Component::iter) {
            let Some(rect) = result.rect(&c.id) else {
                continue;
            };
            let inner = result.inner(&c.id).unwrap_or(rect);
            if !registry.knows(&c.kind) {
                out.push(diag(
                    Severity::Warning,
                    "unknown-kind",
                    &c.id,
                    format!("kind {:?} has no renderer", c.kind),
                ));
            }
            if rect.is_empty() {
                out.push(diag(
                    Severity::Error,
                    "no-space",
                    &c.id,
                    format!("gets no space at {}×{}", size.width, size.height),
                ));
                continue;
            }
            // Requested fixed sizes that the screen cannot honour.
            if let (Placement::Absolute { x, y }, Dimension::Fixed(w)) =
                (c.layout.placement, c.layout.width)
            {
                if u32::from(x) + u32::from(w) > area.right() {
                    out.push(diag(
                        Severity::Warning,
                        "clipped",
                        &c.id,
                        format!(
                            "extends past the right edge at {}×{} (x {x} + width {w})",
                            size.width, size.height
                        ),
                    ));
                }
                if let Dimension::Fixed(h) = c.layout.height {
                    if u32::from(y) + u32::from(h) > area.bottom() {
                        out.push(diag(
                            Severity::Warning,
                            "clipped",
                            &c.id,
                            format!(
                                "extends past the bottom edge at {}×{} (y {y} + height {h})",
                                size.width, size.height
                            ),
                        ));
                    }
                }
            }
            // Truncated text (single-line kinds; viewers wrap or scroll).
            let multiline = matches!(
                c.kind.as_str(),
                "markdown" | "code" | "log" | "diff" | "textarea" | "dialog" | "notification"
            );
            let text = c
                .prop_str("title")
                .or_else(|| c.prop_str("text"))
                .or_else(|| c.prop_str("label"));
            if let Some(text) = text.filter(|t| !t.is_empty() && !multiline) {
                let available = if c.kind == "panel" {
                    rect.width.saturating_sub(4)
                } else if c.kind == "button" {
                    inner.width.saturating_sub(4)
                } else {
                    inner.width
                };
                if painter::text_width(text) > available {
                    out.push(diag(
                        Severity::Warning,
                        "truncated",
                        &c.id,
                        format!(
                            "{text:?} will be truncated at {}×{} ({} columns available)",
                            size.width, size.height, available
                        ),
                    ));
                }
            }
            // Overlapping absolute siblings.
            let siblings = c
                .children
                .iter()
                .filter(|s| matches!(s.layout.placement, Placement::Absolute { .. }))
                .collect::<Vec<_>>();
            for (i, a) in siblings.iter().enumerate() {
                for b in &siblings[i + 1..] {
                    if let (Some(ra), Some(rb)) = (result.rect(&a.id), result.rect(&b.id)) {
                        if !ra.intersection(rb).is_empty() {
                            out.push(diag(
                                Severity::Warning,
                                "overlap",
                                &a.id,
                                format!("overlaps {}", b.id),
                            ));
                        }
                    }
                }
            }
        }
        // Overlapping absolute roots.
        let roots_abs: Vec<&Component> = resolved
            .iter()
            .filter(|c| matches!(c.layout.placement, Placement::Absolute { .. }))
            .collect();
        for (i, a) in roots_abs.iter().enumerate() {
            for b in &roots_abs[i + 1..] {
                if let (Some(ra), Some(rb)) = (result.rect(&a.id), result.rect(&b.id)) {
                    if !ra.intersection(rb).is_empty() {
                        out.push(diag(
                            Severity::Warning,
                            "overlap",
                            &a.id,
                            format!("overlaps {}", b.id),
                        ));
                    }
                }
            }
        }
    }
    out.extend(validate_rendered(screen, theme, registry, size));
    out
}

/// Kinds a user operates; they must stay visible and reachable.
const INTERACTIVE: &[&str] = &[
    "button", "input", "password", "search", "textarea", "checkbox", "radio", "toggle", "select",
    "tabs", "menu", "list", "table", "datagrid", "tree",
];

/// Kinds that paint their whole rectangle (they hide what is beneath).
const OPAQUE: &[&str] = &[
    "panel",
    "modal",
    "dialog",
    "notification",
    "textarea",
    "menu",
];

/// Perceived brightness 0..=255, if the colour is known.
fn brightness(c: crate::document::Color) -> Option<i32> {
    c.to_rgb()
        .map(|(r, g, b)| (i32::from(r) * 299 + i32::from(g) * 587 + i32::from(b) * 114) / 1000)
}

/// Checks on the rendered screen: text drawn in its background colour,
/// low contrast, wide glyphs cut by a component edge, components smaller
/// than their content, and controls hidden under later components.
pub fn validate_rendered(
    screen: &Screen,
    theme: &Theme,
    registry: &Registry,
    size: Size,
) -> Vec<Diagnostic> {
    use crate::document::Position;
    let mut out = Vec::new();
    let Ok(canvas) = crate::render::render_screen(screen, theme, registry, Some(size)) else {
        return out;
    };
    let ctx = RenderContext {
        theme,
        screen: Some(screen),
        data: None,
        symbols: &[],
        depth: 0,
    };
    let measure = RegistryMeasure { registry, ctx };
    // Components in paint order (layers bottom to top, parents first).
    let mut placed: Vec<(Component, Rect)> = Vec::new();
    for layer in screen.layers().iter().filter(|l| l.visible) {
        let Some(roots) = layer.components() else {
            continue;
        };
        let resolved: Vec<Component> = roots
            .iter()
            .filter_map(|c| c.resolve_responsive(size.width))
            .collect();
        let result = layout::solve(&resolved, Rect::from_size(size), &measure);
        for c in resolved.iter().flat_map(Component::iter) {
            if let Some(r) = result.rect(&c.id).filter(|r| !r.is_empty()) {
                placed.push((c.clone(), r));
            }
        }
    }
    let diag = |severity, code, target: &ObjectId, message: String| Diagnostic {
        severity,
        code,
        screen: Some(screen.id.clone()),
        target: Some(target.clone()),
        message,
    };
    for (i, (c, rect)) in placed.iter().enumerate() {
        // The cells this component owns: not covered by a later one.
        let owned = |p: Position| !placed[i + 1..].iter().any(|(_, r)| r.contains(p));
        let mut invisible = 0;
        let mut low = 0;
        let mut cut = None;
        for p in rect.positions().filter(|p| owned(*p)) {
            let Some(cell) = canvas.get(p) else { continue };
            // Readability applies to text: letters and digits, not frame
            // lines, blocks or Braille graphics.
            let Some(_) = cell
                .as_glyph()
                .filter(|g| g.as_str().chars().any(char::is_alphanumeric))
            else {
                continue;
            };
            let (fg, bg) = (theme.resolve(&cell.style.fg.to_string()), cell.style.bg);
            if fg == bg && fg != crate::document::Color::Default {
                invisible += 1;
            } else if let (Some(a), Some(b)) = (brightness(fg), brightness(bg)) {
                if (a - b).abs() < 40 {
                    low += 1;
                }
            }
            // A wide glyph whose second half leaves the owner.
            if cell.is_wide_head() && !rect.contains(Position::new(p.x + 1, p.y)) {
                cut.get_or_insert(p);
            }
        }
        if let Some(p) = cut {
            out.push(diag(
                Severity::Warning,
                "wide-glyph",
                &c.id,
                format!(
                    "a double-width glyph at {},{} crosses the edge at {}×{}",
                    p.x, p.y, size.width, size.height
                ),
            ));
        }
        if invisible > 0 {
            out.push(diag(
                Severity::Error,
                "invisible",
                &c.id,
                format!("{invisible} cell(s) of text in the same colour as their background"),
            ));
        } else if low > 0 {
            out.push(diag(
                Severity::Warning,
                "low-contrast",
                &c.id,
                format!("{low} cell(s) of text with too little contrast to read"),
            ));
        }
        // Smaller than the content needs (text kinds report truncation).
        if let Some(min) = registry.renderer(&c.kind).intrinsic_size(c, &ctx) {
            let chrome = registry.renderer(&c.kind).chrome_inset(c, &ctx);
            let need_h = min.height + chrome.vertical() + c.layout.padding.vertical();
            if rect.height < need_h && min.height > 0 {
                out.push(diag(
                    Severity::Warning,
                    "too-small",
                    &c.id,
                    format!("{} rows tall, its content needs {need_h}", rect.height),
                ));
            }
        }
        // Interactive components hidden under later ones.
        if INTERACTIVE.contains(&c.kind.as_str()) {
            let covered = placed[i + 1..].iter().any(|(k, r)| {
                OPAQUE.contains(&k.kind.as_str())
                    && r.intersection(*rect) == *rect
                    && !c.iter().any(|d| d.id == k.id)
            });
            if covered {
                out.push(diag(
                    Severity::Warning,
                    "unreachable",
                    &c.id,
                    format!(
                        "covered by a later component at {}×{}; nobody can see or click it",
                        size.width, size.height
                    ),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;
    use crate::layout::Layout;

    fn doc_with(roots: Vec<Component>, w: u16, h: u16) -> Document {
        let mut doc = Document::new(Size::new(w, h)).unwrap();
        *doc.layer_mut(&id!("main-ui"))
            .unwrap()
            .components_mut()
            .unwrap() = roots;
        doc
    }

    fn codes(doc: &Document) -> Vec<&'static str> {
        validate(doc, &Registry::builtin())
            .iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn clean_document_has_no_diagnostics() {
        let doc = doc_with(
            vec![Component::new(id!("p"), "panel").with_layout(Layout::absolute(0, 0, 10, 5))],
            20,
            10,
        );
        assert!(codes(&doc).is_empty());
    }

    #[test]
    fn truncation_is_reported_with_the_component_id() {
        let doc = doc_with(
            vec![
                Component::new(id!("metrics-table"), "panel")
                    .with_prop("title", "Request latency percentile")
                    .with_layout(Layout::absolute(0, 0, 12, 3)),
            ],
            80,
            24,
        );
        let d = &validate(&doc, &Registry::builtin())[0];
        assert_eq!(d.code, "truncated");
        assert_eq!(d.target, Some(id!("metrics-table")));
        assert!(d.to_string().contains("metrics-table"), "{d}");
    }

    #[test]
    fn clipping_overlap_and_unknown_kind() {
        let doc = doc_with(
            vec![
                Component::new(id!("a"), "panel").with_layout(Layout::absolute(0, 0, 10, 5)),
                Component::new(id!("b"), "panel").with_layout(Layout::absolute(5, 2, 30, 5)),
                Component::new(id!("t"), "wobbler").with_layout(Layout::absolute(0, 8, 5, 2)),
            ],
            20,
            10,
        );
        let c = codes(&doc);
        assert!(c.contains(&"overlap"));
        assert!(c.contains(&"clipped"));
        assert!(c.contains(&"unknown-kind"));
    }

    #[test]
    fn responsive_check_at_smaller_size_finds_no_space() {
        let doc = doc_with(
            vec![Component::new(id!("wide"), "panel").with_layout(Layout::absolute(90, 0, 10, 5))],
            120,
            40,
        );
        assert!(codes(&doc).is_empty());
        let small = validate_screen(
            doc.first_screen(),
            &doc.theme,
            &Registry::builtin(),
            Size::new(80, 24),
        );
        assert_eq!(small[0].code, "no-space");
    }

    #[test]
    fn duplicate_ids_are_errors() {
        let doc = doc_with(
            vec![
                Component::new(id!("a"), "label"),
                Component::new(id!("a"), "label"),
            ],
            10,
            2,
        );
        assert_eq!(
            validate(&doc, &Registry::builtin())[0].severity,
            Severity::Error
        );
    }

    #[test]
    fn property_of_the_wrong_type_is_flagged() {
        let roots = vec![
            Component::new(id!("l"), "label")
                .with_prop("text", "Hi")
                .with_prop("align", "middle")
                .with_prop("bold", "yes")
                .with_layout(Layout::absolute(0, 0, 10, 1)),
        ];
        let diags = validate(&doc_with(roots, 20, 5), &Registry::builtin());
        let codes: Vec<_> = diags.iter().filter(|d| d.code == "prop-type").collect();
        assert_eq!(codes.len(), 2, "{diags:?}");
        assert!(codes[0].message.starts_with("align = \"middle\""));
    }

    #[test]
    fn rendered_checks_find_invisible_small_and_hidden_controls() {
        let roots = vec![
            Component::new(id!("ghost"), "label")
                .with_prop("text", "boo")
                .with_prop("color", "surface")
                .with_prop("background", "surface")
                .with_layout(Layout::absolute(0, 0, 5, 1)),
            Component::new(id!("ok"), "button")
                .with_prop("label", "OK")
                .with_layout(Layout::absolute(0, 2, 8, 1)),
            Component::new(id!("cover"), "panel").with_layout(Layout::absolute(0, 2, 10, 3)),
            Component::new(id!("squashed"), "textarea").with_layout(Layout::absolute(12, 0, 8, 1)),
        ];
        let mut doc = doc_with(roots, 24, 6);
        doc.theme = crate::theme::Theme::builtin_by_id("forge-dark").unwrap();
        let codes = codes(&doc);
        assert!(codes.contains(&"invisible"), "{codes:?}");
        assert!(codes.contains(&"unreachable"), "{codes:?}");
    }
}
