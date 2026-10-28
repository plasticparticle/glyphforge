//! Design suggestions: heuristics about spacing, alignment, hierarchy and
//! density. These are opinions, kept apart from validation (which reports
//! facts) and always labelled as suggestions.

use serde::Serialize;

use crate::component::Component;
use crate::document::{Document, Rect};
use crate::id::ObjectId;
use crate::layout::{self, Placement};
use crate::render::{Registry, RegistryMeasure, RenderContext};

/// One suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Advice {
    /// Stable code: `spacing`, `alignment`, `hierarchy`, `density`.
    pub code: &'static str,
    pub screen: ObjectId,
    pub target: Option<ObjectId>,
    pub message: String,
}

impl std::fmt::Display for Advice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "suggestion: ")?;
        if let Some(t) = &self.target {
            write!(f, "{t}: ")?;
        }
        write!(f, "{} [{}]", self.message, self.code)
    }
}

const TEXT_KINDS: &[&str] = &[
    "label", "heading", "button", "log", "code", "markdown", "table", "datagrid", "list",
];

/// Suggestions for every screen of `doc`.
pub fn advise(doc: &Document, registry: &Registry) -> Vec<Advice> {
    let mut out = Vec::new();
    for screen in doc.screens() {
        let size = screen.size();
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
        let mut headings = 0;
        for layer in screen.layers().iter().filter(|l| l.visible) {
            let Some(roots) = layer.components() else {
                continue;
            };
            let result = layout::solve(roots, Rect::from_size(size), &measure);
            let advice = |code, target: &ObjectId, message: String| Advice {
                code,
                screen: screen.id.clone(),
                target: Some(target.clone()),
                message,
            };
            for c in roots.iter().flat_map(Component::iter) {
                if c.kind == "heading" || (c.kind == "panel" && c.prop_str("title").is_some()) {
                    headings += 1;
                }
                spacing(c, &result, &mut |m| out.push(advice("spacing", &c.id, m)));
                alignment(c, &result, &mut |m| out.push(advice("alignment", &c.id, m)));
                let heading_children = c.children.iter().filter(|k| k.kind == "heading").count();
                if heading_children > 2 {
                    out.push(advice(
                        "hierarchy",
                        &c.id,
                        format!("{heading_children} headings side by side compete; consider one heading and labels"),
                    ));
                }
                if TEXT_KINDS.contains(&c.kind.as_str()) {
                    if let Some(r) = result.inner(&c.id) {
                        let used = registry
                            .renderer(&c.kind)
                            .intrinsic_size(c, &measure.ctx)
                            .map_or(0, |s| u32::from(s.width) * u32::from(s.height));
                        let room = u32::from(r.width) * u32::from(r.height);
                        if room >= 40 && used * 100 > room * 90 {
                            out.push(advice(
                                "density",
                                &c.id,
                                "fills more than 90 % of its area; a little air helps scanning"
                                    .into(),
                            ));
                        }
                    }
                }
            }
        }
        if headings == 0 {
            out.push(Advice {
                code: "hierarchy",
                screen: screen.id.clone(),
                target: None,
                message: format!(
                    "screen {:?} has no heading or titled panel to orient the reader",
                    screen.name
                ),
            });
        }
    }
    out
}

/// Flow children placed with uneven gaps (margins of 0, 1 and 2 mixed).
fn spacing(c: &Component, result: &layout::LayoutResult, report: &mut dyn FnMut(String)) {
    let flow: Vec<&Component> = c
        .children
        .iter()
        .filter(|k| k.layout.placement == Placement::Flow)
        .collect();
    if flow.len() < 3 || c.layout.container.gap > 0 {
        return;
    }
    let vertical = c.layout.container.direction == layout::Direction::Vertical;
    let rects: Vec<Rect> = flow.iter().filter_map(|k| result.rect(&k.id)).collect();
    let gaps: Vec<i64> = rects
        .windows(2)
        .map(|w| {
            if vertical {
                i64::from(w[1].y) - i64::from(w[0].bottom() as u16)
            } else {
                i64::from(w[1].x) - i64::from(w[0].right() as u16)
            }
        })
        .collect();
    let distinct: std::collections::BTreeSet<i64> = gaps.iter().copied().collect();
    if distinct.len() > 1 {
        report(format!(
            "children are spaced unevenly ({}); a container gap keeps rhythm",
            gaps.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

/// Absolutely placed siblings whose left edges miss each other by one
/// cell: probably meant to line up.
fn alignment(c: &Component, result: &layout::LayoutResult, report: &mut dyn FnMut(String)) {
    let abs: Vec<(&ObjectId, Rect)> = c
        .children
        .iter()
        .filter(|k| matches!(k.layout.placement, Placement::Absolute { .. }))
        .filter_map(|k| result.rect(&k.id).map(|r| (&k.id, r)))
        .collect();
    for (i, (a, ra)) in abs.iter().enumerate() {
        for (b, rb) in &abs[i + 1..] {
            if ra.x.abs_diff(rb.x) == 1 {
                report(format!(
                    "{a} and {b} start one column apart; align them (Shift+H)"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Size;
    use crate::id;
    use crate::layout::{Dimension, Layout};

    #[test]
    fn suggestions_cover_spacing_alignment_and_hierarchy() {
        let mut doc = Document::new(Size::new(40, 12)).unwrap();
        let ui = doc.first_screen().layers()[1].id.clone();
        let line = |id: &str, margin_top: u16| {
            Component::new(ObjectId::new(id).unwrap(), "label")
                .with_prop("text", id)
                .with_layout(Layout {
                    height: Dimension::Fixed(1),
                    margin: crate::layout::Edges {
                        top: margin_top,
                        ..crate::layout::Edges::ZERO
                    },
                    ..Layout::default()
                })
        };
        let column = Component::new(id!("col"), "group")
            .with_layout(Layout::absolute(0, 0, 20, 10))
            .with_children(vec![line("a", 0), line("b", 1), line("c", 3)]);
        let free = Component::new(id!("free"), "group")
            .with_layout(Layout::absolute(20, 0, 20, 10))
            .with_children(vec![
                Component::new(id!("x"), "label")
                    .with_prop("text", "x")
                    .with_layout(Layout::absolute(2, 0, 5, 1)),
                Component::new(id!("y"), "label")
                    .with_prop("text", "y")
                    .with_layout(Layout::absolute(3, 2, 5, 1)),
            ]);
        crate::patch::Patch::new(vec![
            crate::patch::Operation::CreateComponent {
                component: column,
                parent: None,
                layer: Some(ui.clone()),
                index: None,
            },
            crate::patch::Operation::CreateComponent {
                component: free,
                parent: None,
                layer: Some(ui),
                index: None,
            },
        ])
        .apply(&mut doc)
        .unwrap();
        let codes: Vec<&str> = advise(&doc, &Registry::builtin())
            .iter()
            .map(|a| a.code)
            .collect();
        assert!(codes.contains(&"spacing"), "{codes:?}");
        assert!(codes.contains(&"alignment"), "{codes:?}");
        assert!(codes.contains(&"hierarchy"), "no heading: {codes:?}");
    }
}
