//! Prototypes: navigation actions on components and the screen graph.
//!
//! A component's `on_activate` property says what happens when it is
//! clicked in prototype mode:
//!
//! - `goto:<screen>`: show another screen
//! - `open:<component>` / `close:<component>` / `toggle:<component>`:
//!   show or hide a component (a modal, a dialog, a panel); hidden ones
//!   carry `hidden: true`
//! - `select:<component>=<index>`: select an item of tabs or a list
//! - `back`: return to the previous screen
//!
//! Several actions can be chained with `;` (`close:dialog;goto:main`).

use crate::component::Component;
use crate::document::Document;
use crate::id::ObjectId;

/// One navigation action.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NavAction {
    GoTo(ObjectId),
    Open(ObjectId),
    Close(ObjectId),
    Toggle(ObjectId),
    Select(ObjectId, i64),
    Back,
}

impl NavAction {
    /// Parses one action; `None` for text that is not one.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text == "back" {
            return Some(Self::Back);
        }
        let (verb, arg) = text.split_once(':')?;
        let id = |s: &str| ObjectId::new(s.trim()).ok();
        Some(match verb.trim() {
            "goto" | "go" => Self::GoTo(id(arg)?),
            "open" | "show" => Self::Open(id(arg)?),
            "close" | "hide" => Self::Close(id(arg)?),
            "toggle" => Self::Toggle(id(arg)?),
            "select" => {
                let (target, index) = arg.split_once('=')?;
                Self::Select(id(target)?, index.trim().parse().ok()?)
            }
            _ => return None,
        })
    }

    /// All actions of an `on_activate` value.
    pub fn parse_all(text: &str) -> Vec<Self> {
        text.split(';').filter_map(Self::parse).collect()
    }

    pub fn describe(&self) -> String {
        match self {
            Self::GoTo(s) => format!("go to {s}"),
            Self::Open(c) => format!("open {c}"),
            Self::Close(c) => format!("close {c}"),
            Self::Toggle(c) => format!("toggle {c}"),
            Self::Select(c, i) => format!("select item {i} of {c}"),
            Self::Back => "back".to_owned(),
        }
    }
}

/// A component's navigation: where it lives and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub screen: ObjectId,
    pub component: ObjectId,
    pub label: String,
    pub actions: Vec<NavAction>,
}

/// Every component with `on_activate`, screen by screen.
pub fn links(doc: &Document) -> Vec<Link> {
    let mut out = Vec::new();
    for screen in doc.screens() {
        for layer in screen.layers() {
            let Some(roots) = layer.components() else {
                continue;
            };
            for c in roots.iter().flat_map(Component::iter) {
                let Some(value) = c.prop_str("on_activate") else {
                    continue;
                };
                let actions = NavAction::parse_all(value);
                if actions.is_empty() {
                    continue;
                }
                let label = ["label", "text", "title"]
                    .iter()
                    .find_map(|k| c.prop_str(k))
                    .unwrap_or(c.id.as_str())
                    .to_owned();
                out.push(Link {
                    screen: screen.id.clone(),
                    component: c.id.clone(),
                    label,
                    actions,
                });
            }
        }
    }
    out
}

/// Problems with navigation: targets that do not exist.
pub fn broken_links(doc: &Document) -> Vec<(Link, NavAction)> {
    let mut out = Vec::new();
    for link in links(doc) {
        for a in &link.actions {
            let ok = match a {
                NavAction::GoTo(s) => doc.screen(s).is_some(),
                NavAction::Open(c)
                | NavAction::Close(c)
                | NavAction::Toggle(c)
                | NavAction::Select(c, _) => doc.component(c).is_some(),
                NavAction::Back => true,
            };
            if !ok {
                out.push((link.clone(), a.clone()));
            }
        }
    }
    out
}

/// The screen graph as text: one line per screen and its outgoing links.
pub fn overview(doc: &Document) -> Vec<String> {
    let all = links(doc);
    let mut lines = Vec::new();
    for screen in doc.screens() {
        let outgoing: Vec<&Link> = all.iter().filter(|l| l.screen == screen.id).collect();
        let incoming = all
            .iter()
            .filter(|l| {
                l.actions
                    .iter()
                    .any(|a| *a == NavAction::GoTo(screen.id.clone()))
            })
            .count();
        lines.push(format!(
            "{} ({}) ← {incoming} link(s)",
            screen.name, screen.id
        ));
        for l in outgoing {
            let what: Vec<String> = l.actions.iter().map(NavAction::describe).collect();
            lines.push(format!(
                "  [{}] {} ─▶ {}",
                l.component,
                l.label,
                what.join(", then ")
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Screen, Size};
    use crate::id;
    use crate::patch::{Operation, Patch};

    #[test]
    fn actions_parse_and_the_graph_lists_links() {
        assert_eq!(
            NavAction::parse("goto:settings"),
            Some(NavAction::GoTo(id!("settings")))
        );
        assert_eq!(
            NavAction::parse("select:tabs=2"),
            Some(NavAction::Select(id!("tabs"), 2))
        );
        assert_eq!(NavAction::parse_all("close:d; back").len(), 2);
        assert_eq!(NavAction::parse("jump"), None);
        let mut doc = Document::new(Size::new(20, 5)).unwrap();
        let ui = doc.first_screen().layers()[1].id.clone();
        Patch::new(vec![
            Operation::CreateScreen {
                screen: Screen::new(id!("settings"), "Settings", Size::new(20, 5)).unwrap(),
                index: None,
            },
            Operation::CreateComponent {
                component: Component::new(id!("go"), "button")
                    .with_prop("label", "Settings")
                    .with_prop("on_activate", "goto:settings"),
                parent: None,
                layer: Some(ui.clone()),
                index: None,
            },
            Operation::CreateComponent {
                component: Component::new(id!("bad"), "button")
                    .with_prop("on_activate", "open:nowhere"),
                parent: None,
                layer: Some(ui),
                index: None,
            },
        ])
        .apply(&mut doc)
        .unwrap();
        let lines = overview(&doc);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("[go] Settings ─▶ go to settings")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("Settings (settings) ← 1 link"))
        );
        assert_eq!(broken_links(&doc).len(), 1);
    }

    #[test]
    fn hidden_components_are_left_out() {
        let c = Component::new(id!("m"), "modal").with_prop("hidden", true);
        assert!(c.resolve_responsive(80).is_none());
    }
}
