//! "Check Design": validation diagnostics and design suggestions of the
//! active screen in a panel; choosing one jumps to its component.

use glyphforge_core::ObjectId;

use super::{App, StatusKind};

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `error`, `warning` or `suggestion`.
    pub level: &'static str,
    pub code: &'static str,
    pub target: Option<ObjectId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problems {
    pub items: Vec<Problem>,
    pub selected: usize,
}

impl App {
    /// Validates the active screen (at the preview size, if any) and
    /// collects suggestions.
    pub fn check_design(&mut self) {
        let size = self.doc_size();
        let screen = self.editor.screen.clone();
        let mut items: Vec<Problem> = self
            .session
            .validate()
            .into_iter()
            .filter(|d| d.screen.as_ref().is_none_or(|s| *s == screen))
            .chain(self.session.validate_at(&screen, size).unwrap_or_default())
            .map(|d| Problem {
                level: match d.severity {
                    glyphforge_core::validate::Severity::Error => "error",
                    glyphforge_core::validate::Severity::Warning => "warning",
                },
                code: d.code,
                target: d.target,
                message: d.message,
            })
            .collect();
        items.dedup();
        items.extend(
            glyphforge_core::advice::advise(self.doc(), self.session.registry())
                .into_iter()
                .filter(|a| a.screen == screen)
                .map(|a| Problem {
                    level: "suggestion",
                    code: a.code,
                    target: a.target,
                    message: a.message,
                }),
        );
        if items.is_empty() {
            self.problems = None;
            return self.set_status(StatusKind::Info, "No problems and no suggestions");
        }
        let errors = items.iter().filter(|p| p.level == "error").count();
        self.set_status(
            StatusKind::Info,
            format!(
                "{} finding(s), {errors} error(s): Enter jumps to the component",
                items.len()
            ),
        );
        self.problems = Some(Problems { items, selected: 0 });
    }

    pub fn problems_move(&mut self, down: bool) {
        if let Some(p) = self.problems.as_mut() {
            let n = p.items.len();
            p.selected = if down {
                (p.selected + 1) % n
            } else {
                (p.selected + n - 1) % n
            };
        }
    }

    /// Enter: selects the component of the highlighted finding.
    pub fn problems_jump(&mut self) {
        let target = self
            .problems
            .as_ref()
            .and_then(|p| p.items.get(p.selected))
            .and_then(|i| i.target.clone());
        if let Some(id) = target.filter(|id| self.doc().component(id).is_some()) {
            self.jump_to_component(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::Size;

    #[test]
    fn check_design_lists_findings_and_jumps() {
        let mut app = test_app(Size::new(20, 4));
        app.dispatch(Action::LayerNext);
        let layer = app.editor.layer.clone();
        let id = app
            .session
            .add_component("label", Some("long"), 15, 0, None, Some(layer))
            .unwrap();
        app.session
            .update_component(
                &id,
                vec![("text".into(), "far too long for the screen".into())],
            )
            .unwrap();
        app.mark_edited();
        app.dispatch(Action::CheckDesign);
        let problems = app.problems.clone().unwrap();
        assert!(
            problems.items.iter().any(|p| p.code == "truncated"),
            "{problems:?}"
        );
        assert!(problems.items.iter().any(|p| p.level == "suggestion"));
        let i = problems
            .items
            .iter()
            .position(|p| p.target.as_ref() == Some(&id))
            .unwrap();
        app.problems.as_mut().unwrap().selected = i;
        app.problems_jump();
        assert_eq!(app.selection(), Some(&id));
    }
}
