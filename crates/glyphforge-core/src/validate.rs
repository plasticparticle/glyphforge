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
    for screen in doc.screens() {
        out.extend(validate_screen(screen, &doc.theme, registry, screen.size()));
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
            // Truncated text.
            let text = c
                .prop_str("title")
                .or_else(|| c.prop_str("text"))
                .or_else(|| c.prop_str("label"));
            if let Some(text) = text.filter(|t| !t.is_empty()) {
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
                Component::new(id!("t"), "table").with_layout(Layout::absolute(0, 8, 5, 2)),
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
}
