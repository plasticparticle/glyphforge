//! Semantic components: the primary content of Interface Mode.
//!
//! A component keeps its identity, kind, properties, layout and children.
//! Cells are derived from it by the renderer, never stored in it.

use serde::{Deserialize, Serialize};

use crate::id::ObjectId;
use crate::layout::Layout;
use crate::value::{Properties, Value};

/// A rule that adapts a component to the available terminal width.
///
/// `set` overrides properties, `hide` removes the component from layout
/// and rendering. Rules are evaluated in order; later rules win.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ResponsiveRule {
    /// Applies when the screen is narrower than this many columns.
    pub max_width: Option<u16>,
    /// Applies when the screen is at least this many columns wide.
    pub min_width: Option<u16>,
    pub hide: bool,
    pub set: Properties,
    /// Layout overrides (only the fields present in the JSON object).
    pub layout: Option<Layout>,
}

impl ResponsiveRule {
    pub fn applies(&self, width: u16) -> bool {
        self.max_width.is_none_or(|m| width < m) && self.min_width.is_none_or(|m| width >= m)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: ObjectId,
    /// Registry key such as `panel`, `label`, `table`. Unknown kinds are
    /// preserved and rendered as a placeholder.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Properties::is_empty")]
    pub props: Properties,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responsive: Vec<ResponsiveRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Component>,
}

impl Component {
    pub fn new(id: ObjectId, kind: impl Into<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            props: Properties::new(),
            layout: Layout::default(),
            responsive: Vec::new(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    #[must_use]
    pub fn with_prop(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.props.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_children(mut self, children: Vec<Component>) -> Self {
        self.children = children;
        self
    }

    pub fn prop(&self, key: &str) -> Option<&Value> {
        self.props.get(key)
    }

    pub fn prop_str(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(Value::as_str)
    }

    pub fn prop_int(&self, key: &str) -> Option<i64> {
        self.props.get(key).and_then(Value::as_int)
    }

    pub fn prop_bool(&self, key: &str) -> Option<bool> {
        self.props.get(key).and_then(Value::as_bool)
    }

    /// Depth-first iteration over this component and all descendants.
    pub fn iter(&self) -> impl Iterator<Item = &Component> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let c = stack.pop()?;
            stack.extend(c.children.iter().rev());
            Some(c)
        })
    }

    pub fn find(&self, id: &ObjectId) -> Option<&Component> {
        self.iter().find(|c| &c.id == id)
    }

    pub fn find_mut(&mut self, id: &ObjectId) -> Option<&mut Component> {
        if &self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(id))
    }

    /// Returns the component with the responsive rules for `width`
    /// applied, or `None` when a rule hides it. Children are resolved
    /// recursively.
    pub fn resolve_responsive(&self, width: u16) -> Option<Component> {
        let mut out = self.clone();
        for rule in self.responsive.iter().filter(|r| r.applies(width)) {
            if rule.hide {
                return None;
            }
            for (k, v) in &rule.set {
                if v.is_null() {
                    out.props.remove(k);
                } else {
                    out.props.insert(k.clone(), v.clone());
                }
            }
            if let Some(layout) = rule.layout {
                out.layout = layout;
            }
        }
        out.responsive.clear();
        out.children = self
            .children
            .iter()
            .filter_map(|c| c.resolve_responsive(width))
            .collect();
        Some(out)
    }
}

/// Helpers over a forest of root components (a layer's content).
pub mod tree {
    use super::Component;
    use crate::id::ObjectId;

    pub fn iter(roots: &[Component]) -> impl Iterator<Item = &Component> {
        roots.iter().flat_map(Component::iter)
    }

    pub fn find<'a>(roots: &'a [Component], id: &ObjectId) -> Option<&'a Component> {
        roots.iter().find_map(|r| r.find(id))
    }

    pub fn find_mut<'a>(roots: &'a mut [Component], id: &ObjectId) -> Option<&'a mut Component> {
        roots.iter_mut().find_map(|r| r.find_mut(id))
    }

    /// The parent id and index of `id`, or `None` for roots and unknown ids.
    pub fn locate(roots: &[Component], id: &ObjectId) -> Option<(Option<ObjectId>, usize)> {
        if let Some(i) = roots.iter().position(|c| &c.id == id) {
            return Some((None, i));
        }
        for c in iter(roots) {
            if let Some(i) = c.children.iter().position(|ch| &ch.id == id) {
                return Some((Some(c.id.clone()), i));
            }
        }
        None
    }

    /// Removes and returns the component with `id`, wherever it is.
    pub fn remove(roots: &mut Vec<Component>, id: &ObjectId) -> Option<Component> {
        fn rec(nodes: &mut [Component], id: &ObjectId) -> Option<Component> {
            for n in nodes {
                if let Some(i) = n.children.iter().position(|c| &c.id == id) {
                    return Some(n.children.remove(i));
                }
                if let Some(found) = rec(&mut n.children, id) {
                    return Some(found);
                }
            }
            None
        }
        if let Some(i) = roots.iter().position(|c| &c.id == id) {
            return Some(roots.remove(i));
        }
        rec(roots, id)
    }

    /// Inserts `component` under `parent` (or at the roots) at `index`,
    /// clamped to the end. Returns `false` when the parent does not exist.
    pub fn insert(
        roots: &mut Vec<Component>,
        parent: Option<&ObjectId>,
        index: usize,
        component: Component,
    ) -> bool {
        match parent {
            None => {
                let i = index.min(roots.len());
                roots.insert(i, component);
                true
            }
            Some(p) => match find_mut(roots, p) {
                Some(node) => {
                    let i = index.min(node.children.len());
                    node.children.insert(i, component);
                    true
                }
                None => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;

    fn sample() -> Vec<Component> {
        vec![
            Component::new(id!("window"), "window").with_children(vec![
                Component::new(id!("sidebar"), "panel").with_children(vec![
                    Component::new(id!("menu"), "menu"),
                    Component::new(id!("status"), "label"),
                ]),
                Component::new(id!("content"), "group"),
            ]),
            Component::new(id!("logo"), "artwork"),
        ]
    }

    #[test]
    fn iteration_is_depth_first_in_document_order() {
        let ids: Vec<_> = tree::iter(&sample()).map(|c| c.id.to_string()).collect();
        assert_eq!(
            ids,
            vec!["window", "sidebar", "menu", "status", "content", "logo"]
        );
    }

    #[test]
    fn locate_remove_insert() {
        let mut roots = sample();
        assert_eq!(
            tree::locate(&roots, &id!("status")),
            Some((Some(id!("sidebar")), 1))
        );
        assert_eq!(tree::locate(&roots, &id!("logo")), Some((None, 1)));
        let removed = tree::remove(&mut roots, &id!("menu")).unwrap();
        assert_eq!(removed.kind, "menu");
        assert!(tree::find(&roots, &id!("menu")).is_none());
        assert!(tree::insert(&mut roots, Some(&id!("content")), 99, removed));
        assert_eq!(
            tree::locate(&roots, &id!("menu")),
            Some((Some(id!("content")), 0))
        );
        assert!(!tree::insert(
            &mut roots,
            Some(&id!("nope")),
            0,
            Component::new(id!("x"), "label")
        ));
    }

    #[test]
    fn responsive_rules_override_and_hide() {
        let mut sidebar = Component::new(id!("sidebar"), "panel").with_prop("title", "Navigation");
        sidebar.responsive = vec![
            ResponsiveRule {
                max_width: Some(100),
                set: [("title".to_owned(), Value::from("Nav"))].into(),
                ..Default::default()
            },
            ResponsiveRule {
                max_width: Some(60),
                hide: true,
                ..Default::default()
            },
        ];
        assert_eq!(
            sidebar.resolve_responsive(120).unwrap().prop_str("title"),
            Some("Navigation")
        );
        assert_eq!(
            sidebar.resolve_responsive(90).unwrap().prop_str("title"),
            Some("Nav")
        );
        assert!(sidebar.resolve_responsive(50).is_none());
    }

    #[test]
    fn serde_omits_empty_collections() {
        let c = Component::new(id!("a"), "label").with_prop("text", "hi");
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("children"));
        assert!(!json.contains("responsive"));
        let back: Component = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }
}
