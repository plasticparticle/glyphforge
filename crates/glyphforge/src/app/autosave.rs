//! Autosave and recovery. While the document has unsaved changes it is
//! written every `[autosave] interval_seconds` to a hidden sibling
//! (`.design.glyph.autosave`, or `untitled.autosave.glyph` in the state
//! directory for unnamed documents). The original file is never touched.
//! Saving removes the autosave; on start, a newer autosave is offered for
//! recovery.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::prompt::{Prompt, PromptKind};
use super::{App, StatusKind};

/// The autosave file for a document at `path`.
pub fn autosave_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| "untitled".into(), |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!(".{name}.autosave"))
}

/// Whether `autosave` holds newer work than `original` (or the original
/// does not exist).
pub fn is_newer(autosave: &Path, original: &Path) -> bool {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (modified(autosave), modified(original)) {
        (Some(_), None) => true,
        (Some(a), Some(o)) => a > o,
        _ => false,
    }
}

const RESTORE: &str = "Restore the autosaved changes";
const DISCARD: &str = "Discard them and open the saved file";

impl App {
    /// Where the current document autosaves to.
    pub fn autosave_file(&self) -> Option<PathBuf> {
        match self.session.path() {
            Some(p) => Some(autosave_path(p)),
            None => self
                .state_dir
                .as_ref()
                .map(|d| d.join("untitled.autosave.glyph")),
        }
    }

    /// Called from the run loop: writes the autosave when it is due.
    /// Returns whether a status message changed.
    pub fn maybe_autosave(&mut self) -> bool {
        let cfg = &self.config.autosave;
        if !cfg.enabled || !self.dirty() {
            return false;
        }
        let interval = Duration::from_secs(cfg.interval_seconds.max(5));
        let due = self.last_autosave.is_none_or(|t| t.elapsed() >= interval);
        if !due {
            return false;
        }
        self.last_autosave = Some(Instant::now());
        self.write_autosave()
    }

    /// Writes the autosave now.
    pub fn write_autosave(&mut self) -> bool {
        let Some(path) = self.autosave_file() else {
            return false;
        };
        let result = self
            .session
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|json| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => {
                tracing::info!("autosaved to {}", path.display());
                false
            }
            Err(e) => {
                self.set_status(StatusKind::Warning, format!("Autosave failed: {e}"));
                true
            }
        }
    }

    /// Removes the autosave (after a save, or a clean quit).
    pub fn clear_autosave(&mut self) {
        if let Some(path) = self.autosave_file() {
            let _ = std::fs::remove_file(path);
        }
        self.last_autosave = None;
    }

    /// On start: offers to restore an autosave newer than the file.
    pub fn offer_recovery(&mut self) {
        let Some(original) = self.session.path().map(Path::to_path_buf) else {
            return;
        };
        let autosave = autosave_path(&original);
        if !is_newer(&autosave, &original) {
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::Recover,
            format!(
                "{} has autosaved changes that were never saved",
                original.display()
            ),
            vec![RESTORE.to_owned(), DISCARD.to_owned()],
        ));
    }

    pub(super) fn submit_recover(&mut self, choice: &str) {
        let Some(original) = self.session.path().map(Path::to_path_buf) else {
            return;
        };
        let autosave = autosave_path(&original);
        if choice == RESTORE {
            let restored = std::fs::read_to_string(&autosave)
                .map_err(|e| e.to_string())
                .and_then(|t| glyphforge_core::project::from_json(&t).map_err(|e| e.to_string()));
            match restored {
                Ok(doc) => {
                    // The recovered document replaces the open one in one
                    // undoable step and stays unsaved until you save.
                    let mut session =
                        glyphforge_core::api::Session::new(doc).with_path(Some(original));
                    session.mark_unsaved();
                    self.replace_session(session);
                    self.set_status(StatusKind::Info, "Restored the autosave; save to keep it");
                }
                Err(e) => {
                    self.set_status(StatusKind::Error, format!("Cannot read the autosave: {e}"));
                }
            }
        } else {
            let _ = std::fs::remove_file(&autosave);
            self.set_status(StatusKind::Info, "Discarded the autosave");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::Size;

    #[test]
    fn autosave_writes_a_sibling_and_recovery_restores_it() {
        let dir = std::env::temp_dir().join(format!("glyphforge-autosave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("d.glyph");
        let mut app = test_app(Size::new(10, 2));
        app.session.save_as(&file).unwrap();
        app.dispatch(Action::InsertText("hi".into()));
        app.config.autosave.interval_seconds = 5;
        app.maybe_autosave();
        let autosave = autosave_path(&file);
        assert_eq!(autosave, dir.join(".d.glyph.autosave"));
        assert!(autosave.exists());
        assert!(!app.maybe_autosave(), "not due again yet");
        // Make the autosave newer than the file, then start again.
        std::thread::sleep(Duration::from_millis(20));
        app.write_autosave();
        let mut next = test_app(Size::new(10, 2));
        next.replace_session(glyphforge_core::api::Session::open(&file).unwrap());
        next.offer_recovery();
        assert!(next.prompt.is_some());
        next.submit_recover(RESTORE);
        assert!(next.dirty(), "recovered changes stay unsaved");
        let cell = next.cell_at(glyphforge_core::Position::ORIGIN).unwrap();
        assert_eq!(
            cell.as_glyph().map(glyphforge_core::Grapheme::as_str),
            Some("h")
        );
        // Saving removes the autosave.
        next.dispatch(Action::SaveDocument);
        assert!(!autosave.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
