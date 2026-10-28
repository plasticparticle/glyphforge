//! Reviewing agent changes: proposals waiting next to the file (from
//! `glyphforge propose`) and agent transactions already in the history.
//! The highlighted proposal is previewed on the canvas; accepting applies
//! it as an agent transaction, rejecting deletes it, and an applied agent
//! transaction can be reverted while later edits stay.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use glyphforge_core::history::Origin;
use glyphforge_core::proposal::{self, Proposal};
use glyphforge_core::{Canvas, render};

use super::{App, StatusKind};

/// How often the proposals directory is checked.
const POLL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewItem {
    /// A proposal waiting for a decision.
    Pending { path: PathBuf, proposal: Proposal },
    /// An agent transaction in the history (index into the undo stack).
    Applied {
        index: usize,
        label: String,
        agent: String,
        changes: Vec<String>,
    },
}

impl ReviewItem {
    pub fn title(&self) -> String {
        match self {
            Self::Pending { proposal, .. } => {
                let what = if proposal.message.is_empty() {
                    format!("{} operations", proposal.patch.operations.len())
                } else {
                    proposal.message.clone()
                };
                format!("● {}: {what}", proposal.agent)
            }
            Self::Applied { label, .. } => format!("✓ {label}"),
        }
    }

    /// One line per operation.
    pub fn changes(&self) -> Vec<String> {
        match self {
            Self::Pending { proposal, .. } => proposal
                .patch
                .operations
                .iter()
                .map(glyphforge_core::patch::Operation::summary)
                .collect(),
            Self::Applied { changes, .. } => changes.clone(),
        }
    }

    pub fn agent(&self) -> &str {
        match self {
            Self::Pending { proposal, .. } => &proposal.agent,
            Self::Applied { agent, .. } => agent,
        }
    }
}

/// The open review popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub items: Vec<ReviewItem>,
    pub selected: usize,
    /// The screen with the highlighted proposal applied.
    pub preview: Option<Canvas>,
}

impl App {
    fn review_items(&self) -> Vec<ReviewItem> {
        let mut items: Vec<ReviewItem> = self
            .session
            .path()
            .map(proposal::list)
            .unwrap_or_default()
            .into_iter()
            .map(|(path, proposal)| ReviewItem::Pending { path, proposal })
            .collect();
        let stack = self.session.history().undo_stack();
        for (index, t) in stack.iter().enumerate().rev() {
            if let Origin::Agent { name, .. } = &t.origin {
                items.push(ReviewItem::Applied {
                    index,
                    label: t.label.clone(),
                    agent: name.clone(),
                    changes: t
                        .forward
                        .operations
                        .iter()
                        .map(glyphforge_core::patch::Operation::summary)
                        .collect(),
                });
            }
        }
        items
    }

    pub fn open_review(&mut self) {
        let items = self.review_items();
        if items.is_empty() {
            let hint = if self.session.path().is_some() {
                "No agent changes to review (agents propose with `glyphforge propose FILE PATCH`)"
            } else {
                "No agent changes to review; save the design so agents can propose changes to it"
            };
            self.set_status(StatusKind::Info, hint);
            return;
        }
        self.review = Some(Review {
            items,
            selected: 0,
            preview: None,
        });
        self.refresh_review_preview();
    }

    /// Renders the active screen with the highlighted proposal applied.
    fn refresh_review_preview(&mut self) {
        let preview = self
            .review
            .as_ref()
            .and_then(|r| match r.items.get(r.selected) {
                Some(ReviewItem::Pending { proposal, .. }) => {
                    let mut doc = self.doc().clone();
                    proposal.patch.apply(&mut doc).ok()?;
                    let screen = doc
                        .screen(&self.editor.screen)
                        .unwrap_or_else(|| doc.first_screen());
                    render::render_screen_full(
                        screen,
                        &glyphforge_core::render::Scene::of(&doc),
                        self.session.registry(),
                        self.editor.preview_size,
                    )
                    .ok()
                }
                _ => None,
            });
        if let Some(r) = self.review.as_mut() {
            r.preview = preview;
        }
    }

    pub fn review_move(&mut self, down: bool) {
        if let Some(r) = self.review.as_mut() {
            let n = r.items.len().max(1);
            r.selected = if down {
                (r.selected + 1) % n
            } else {
                (r.selected + n - 1) % n
            };
        }
        self.refresh_review_preview();
    }

    fn selected_review_item(&self) -> Option<ReviewItem> {
        self.review
            .as_ref()
            .and_then(|r| r.items.get(r.selected).cloned())
    }

    /// Enter: applies the highlighted proposal as an agent transaction.
    pub fn review_accept(&mut self) {
        match self.selected_review_item() {
            Some(ReviewItem::Pending { path, proposal }) => {
                self.commit_edits();
                match self.session.apply_proposal(&proposal) {
                    Ok(()) => {
                        self.mark_edited();
                        if let Err(e) = proposal::remove(&path) {
                            self.report_error(&e);
                        } else {
                            self.set_status(
                                StatusKind::Info,
                                format!("Accepted {}'s change (Ctrl+Z undoes it)", proposal.agent),
                            );
                        }
                    }
                    Err(e) => self.report_error(&e),
                }
                self.reload_review();
            }
            Some(ReviewItem::Applied { .. }) => {
                self.set_status(StatusKind::Info, "Already applied; x reverts it");
            }
            None => {}
        }
    }

    /// x: rejects a proposal (deletes it) or reverts an applied change.
    pub fn review_reject(&mut self) {
        match self.selected_review_item() {
            Some(ReviewItem::Pending { path, proposal }) => {
                match proposal::remove(&path) {
                    Ok(()) => self.set_status(
                        StatusKind::Info,
                        format!("Rejected {}'s proposal", proposal.agent),
                    ),
                    Err(e) => self.report_error(&e),
                }
                self.reload_review();
            }
            Some(ReviewItem::Applied { index, .. }) => {
                self.commit_edits();
                match self.session.revert(index) {
                    Ok(label) => {
                        self.mark_edited();
                        self.set_status(StatusKind::Info, label);
                    }
                    Err(e) => self.report_error(&e),
                }
                self.reload_review();
            }
            None => {}
        }
    }

    fn reload_review(&mut self) {
        let items = self.review_items();
        self.pending_proposals = items
            .iter()
            .filter(|i| matches!(i, ReviewItem::Pending { .. }))
            .count();
        if items.is_empty() {
            self.review = None;
            return;
        }
        if let Some(r) = self.review.as_mut() {
            r.selected = r.selected.min(items.len() - 1);
            r.items = items;
        }
        self.refresh_review_preview();
    }

    /// Called from the run loop: notices new proposals for the open file.
    /// Returns whether something changed that needs a redraw.
    pub fn poll_proposals(&mut self) -> bool {
        if self.last_proposal_poll.is_some_and(|t| t.elapsed() < POLL) {
            return false;
        }
        self.last_proposal_poll = Some(Instant::now());
        let Some(path) = self.session.path() else {
            return false;
        };
        let count = proposal::list(path).len();
        if count == self.pending_proposals {
            return false;
        }
        let more = count > self.pending_proposals;
        self.pending_proposals = count;
        if more {
            self.set_status(
                StatusKind::Info,
                format!("{count} agent proposal(s) waiting: F9 reviews them"),
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::patch::{Operation, Patch};
    use glyphforge_core::{Component, ObjectId, Size};

    #[test]
    fn accept_reject_and_revert_agent_changes() {
        let dir = std::env::temp_dir().join(format!("glyphforge-review-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("d.glyph");
        let mut app = test_app(Size::new(30, 6));
        app.session.save_as(&file).unwrap();
        let ui = app.doc().first_screen().layers()[1].id.clone();
        let add = |id: &str| Proposal {
            agent: "bot".into(),
            message: format!("add {id}"),
            created: 0,
            patch: Patch::single(Operation::CreateComponent {
                component: Component::new(ObjectId::new(id).unwrap(), "label")
                    .with_prop("text", id),
                parent: None,
                layer: Some(ui.clone()),
                index: None,
            }),
        };
        proposal::submit(&file, &add("one")).unwrap();
        proposal::submit(&file, &add("two")).unwrap();
        assert!(app.poll_proposals());
        assert_eq!(app.pending_proposals, 2);
        app.dispatch(Action::Review);
        let review = app.review.as_ref().unwrap();
        assert_eq!(review.items.len(), 2);
        assert!(review.preview.is_some(), "the first proposal is previewed");
        app.review_accept();
        assert!(
            app.doc()
                .component(&ObjectId::new("one").unwrap())
                .is_some()
        );
        app.review_reject();
        assert_eq!(proposal::list(&file).len(), 0, "both decided");
        assert!(
            app.doc()
                .component(&ObjectId::new("two").unwrap())
                .is_none()
        );
        // The applied change is listed and can be reverted.
        let review = app.review.as_ref().unwrap();
        assert!(matches!(review.items[0], ReviewItem::Applied { .. }));
        app.review_reject();
        assert!(
            app.doc()
                .component(&ObjectId::new("one").unwrap())
                .is_none()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
