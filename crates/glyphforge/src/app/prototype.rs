//! Prototype mode: click through the design. Components with
//! `on_activate` become live: they switch screens, open and close modals,
//! select tabs. What a prototype opens or selects is editor state applied
//! to a copy of the screen; the document never changes.

use std::collections::BTreeMap;

use glyphforge_core::prototype::NavAction;
use glyphforge_core::render::{self, Scene};
use glyphforge_core::{Canvas, Component, ObjectId, Position, Rect, Screen, Value};

use super::problems::{Problem, Problems};
use super::{App, StatusKind};

/// Running prototype state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prototype {
    /// Components opened (`false`) or closed (`true`) by actions.
    pub hidden: BTreeMap<ObjectId, bool>,
    /// Selections made by actions (tabs, lists).
    pub selected: BTreeMap<ObjectId, i64>,
    /// Screens to go back to.
    pub back: Vec<ObjectId>,
    /// Index into the screen's live components (keyboard focus).
    pub focus: usize,
}

fn apply_overrides(c: &mut Component, proto: &Prototype) {
    if let Some(h) = proto.hidden.get(&c.id) {
        c.props.insert("hidden".into(), Value::Bool(*h));
    }
    if let Some(n) = proto.selected.get(&c.id) {
        c.props.insert("selected".into(), Value::Int(*n));
    }
    for child in &mut c.children {
        apply_overrides(child, proto);
    }
}

impl App {
    pub fn toggle_prototype(&mut self) {
        if self.prototype.take().is_some() {
            self.mark_edited();
            self.set_status(StatusKind::Info, "Prototype stopped");
            return;
        }
        self.prototype = Some(Prototype::default());
        self.select(None);
        self.mark_edited();
        let n = self.live_components().len();
        self.set_status(
            StatusKind::Info,
            format!("Prototype: click (or Tab, Enter) the {n} live component(s); Backspace goes back, F12 stops"),
        );
    }

    /// The active screen with the prototype's overrides applied.
    fn prototype_screen(&self) -> Option<Screen> {
        let proto = self.prototype.as_ref()?;
        let mut screen = self.screen().clone();
        for layer in screen.layers_mut() {
            if let Some(roots) = layer.components_mut() {
                for c in roots.iter_mut() {
                    apply_overrides(c, proto);
                }
            }
        }
        Some(screen)
    }

    /// The rendered prototype screen (replaces the composite while running).
    pub(super) fn prototype_canvas(&self) -> Option<Canvas> {
        let screen = self.prototype_screen()?;
        render::render_screen_full(
            &screen,
            &Scene::of(self.doc()),
            self.session.registry(),
            self.editor.preview_size,
        )
        .ok()
    }

    /// Components with `on_activate` on the prototype screen and where
    /// they are, in paint order (the last one is on top).
    pub fn live_components(&self) -> Vec<(ObjectId, Rect)> {
        let Some(screen) = self.prototype_screen() else {
            return Vec::new();
        };
        let size = self.doc_size();
        let doc = self.doc();
        let ctx = glyphforge_core::render::RenderContext {
            theme: &doc.theme,
            screen: Some(&screen),
            data: Some(&doc.data),
            symbols: &doc.symbols,
            depth: 0,
        };
        let mut out = Vec::new();
        for layer in screen.layers().iter().filter(|l| l.visible) {
            let Some(roots) = layer.components() else {
                continue;
            };
            let result = render::solve_components(roots, size, &ctx, self.session.registry());
            for c in roots.iter().flat_map(Component::iter) {
                if c.prop_str("on_activate").is_none() {
                    continue;
                }
                if let Some(r) = result.rect(&c.id).filter(|r| !r.is_empty()) {
                    out.push((c.id.clone(), r));
                }
            }
        }
        out
    }

    /// A click: the top-most live component under the pointer acts.
    pub fn prototype_click(&mut self, pos: Position) {
        let hit = self
            .live_components()
            .into_iter()
            .rev()
            .find(|(_, r)| r.contains(pos))
            .map(|(id, _)| id);
        if let Some(id) = hit {
            self.activate(&id);
        }
    }

    pub fn prototype_focus(&mut self, forward: bool) {
        let n = self.live_components().len();
        if let (Some(p), true) = (self.prototype.as_mut(), n > 0) {
            p.focus = if forward {
                (p.focus + 1) % n
            } else {
                (p.focus + n - 1) % n
            };
        }
    }

    /// Enter: activates the focused live component.
    pub fn prototype_enter(&mut self) {
        let live = self.live_components();
        let Some(i) = self.prototype.as_ref().map(|p| p.focus) else {
            return;
        };
        if let Some((id, _)) = live.get(i.min(live.len().saturating_sub(1))).cloned() {
            self.activate(&id);
        }
    }

    pub fn prototype_back(&mut self) {
        let Some(screen) = self.prototype.as_mut().and_then(|p| p.back.pop()) else {
            return self.set_status(StatusKind::Info, "Nothing to go back to");
        };
        self.show_screen_in_prototype(&screen);
    }

    fn show_screen_in_prototype(&mut self, screen: &ObjectId) {
        let saved = self.prototype.take();
        self.switch_screen(screen);
        self.prototype = saved.map(|mut p| {
            p.focus = 0;
            p
        });
        self.mark_edited();
    }

    /// Runs a component's `on_activate` actions.
    fn activate(&mut self, id: &ObjectId) {
        let Some(actions) = self.prototype_screen().and_then(|s| {
            s.layers()
                .iter()
                .filter_map(|l| l.components())
                .flat_map(|roots| roots.iter().flat_map(Component::iter))
                .find(|c| &c.id == id)
                .and_then(|c| c.prop_str("on_activate").map(NavAction::parse_all))
        }) else {
            return;
        };
        let mut said = Vec::new();
        for action in actions {
            said.push(action.describe());
            match action {
                NavAction::GoTo(screen) => {
                    if self.doc().screen(&screen).is_none() {
                        self.set_status(StatusKind::Warning, format!("No screen {screen}"));
                        return;
                    }
                    let here = self.editor.screen.clone();
                    if let Some(p) = self.prototype.as_mut() {
                        p.back.push(here);
                    }
                    self.show_screen_in_prototype(&screen);
                }
                NavAction::Back => self.prototype_back(),
                NavAction::Open(ref c) | NavAction::Close(ref c) | NavAction::Toggle(ref c) => {
                    let c = c.clone();
                    let hidden_now = self
                        .prototype
                        .as_ref()
                        .and_then(|p| p.hidden.get(&c).copied())
                        .or_else(|| self.doc().component(&c).and_then(|x| x.prop_bool("hidden")))
                        .unwrap_or(false);
                    let hide = match action {
                        NavAction::Open(_) => false,
                        NavAction::Close(_) => true,
                        _ => !hidden_now,
                    };
                    if let Some(p) = self.prototype.as_mut() {
                        p.hidden.insert(c, hide);
                    }
                }
                NavAction::Select(c, n) => {
                    if let Some(p) = self.prototype.as_mut() {
                        p.selected.insert(c, n);
                    }
                }
            }
        }
        self.mark_edited();
        self.set_status(StatusKind::Info, format!("{id}: {}", said.join(", then ")));
    }

    /// The screen graph: every screen and its links; Enter jumps to the
    /// linking component.
    pub fn open_overview(&mut self) {
        let mut items: Vec<Problem> = Vec::new();
        let links = glyphforge_core::prototype::links(self.doc());
        for line in glyphforge_core::prototype::overview(self.doc()) {
            let target = line
                .trim_start()
                .strip_prefix('[')
                .and_then(|r| r.split_once(']'))
                .and_then(|(id, _)| ObjectId::new(id).ok())
                .filter(|id| links.iter().any(|l| &l.component == id));
            items.push(Problem {
                level: if target.is_some() { "link" } else { "screen" },
                code: "graph",
                target,
                message: line.trim_start().to_owned(),
            });
        }
        self.problems = Some(Problems {
            title: "Screen graph · ⏎ go to the link  Esc close".to_owned(),
            items,
            selected: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::layout::Layout;
    use glyphforge_core::patch::{Operation, Patch};
    use glyphforge_core::{Component, Position, Screen, Size};

    #[test]
    fn prototype_navigates_screens_and_opens_modals() {
        let mut app = test_app(Size::new(30, 8));
        let ui = app.doc().first_screen().layers()[1].id.clone();
        let screen = Screen::new(
            glyphforge_core::ObjectId::new("settings").unwrap(),
            "Settings",
            Size::new(30, 8),
        )
        .unwrap();
        let settings_ui = screen.layers()[1].id.clone();
        let patch = Patch::new(vec![
            Operation::CreateScreen {
                screen,
                index: None,
            },
            Operation::CreateComponent {
                component: Component::new(glyphforge_core::ObjectId::new("go").unwrap(), "button")
                    .with_prop("label", "Settings")
                    .with_prop("on_activate", "goto:settings")
                    .with_layout(Layout::absolute(0, 0, 12, 1)),
                parent: None,
                layer: Some(ui),
                index: None,
            },
            Operation::CreateComponent {
                component: Component::new(
                    glyphforge_core::ObjectId::new("about").unwrap(),
                    "dialog",
                )
                .with_prop("title", "About")
                .with_prop("hidden", true)
                .with_layout(Layout::absolute(2, 2, 20, 5)),
                parent: None,
                layer: Some(settings_ui.clone()),
                index: None,
            },
            Operation::CreateComponent {
                component: Component::new(
                    glyphforge_core::ObjectId::new("info").unwrap(),
                    "button",
                )
                .with_prop("label", "Info")
                .with_prop("on_activate", "open:about")
                .with_layout(Layout::absolute(0, 0, 8, 1)),
                parent: None,
                layer: Some(settings_ui),
                index: None,
            },
        ]);
        app.apply(patch, "Setup");
        app.dispatch(Action::TogglePrototype);
        app.prototype_click(Position::new(2, 0));
        assert_eq!(app.screen().id.as_str(), "settings");
        let before = app.prototype_canvas().unwrap();
        app.prototype_enter();
        let after = app.prototype_canvas().unwrap();
        assert_ne!(before, after, "the dialog opened");
        assert_eq!(
            app.doc()
                .component(&glyphforge_core::ObjectId::new("about").unwrap())
                .unwrap()
                .prop_bool("hidden"),
            Some(true),
            "the document is unchanged"
        );
        app.prototype_back();
        assert_eq!(app.screen().id.as_str(), "main");
        app.dispatch(Action::ScreenOverview);
        assert!(
            app.problems
                .as_ref()
                .unwrap()
                .items
                .iter()
                .any(|p| p.level == "link")
        );
    }
}
