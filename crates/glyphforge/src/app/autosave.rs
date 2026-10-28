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

/// How often Omarchy's theme files are checked for changes.
const THEME_POLL: Duration = Duration::from_millis(800);

impl App {
    /// The file the Omarchy `theme-set` hook touches to ask for a reload.
    pub fn theme_trigger_file(&self) -> Option<PathBuf> {
        self.state_dir.as_ref().map(|d| d.join("reload-theme"))
    }

    /// Called from the run loop: reloads the chrome theme when Omarchy's
    /// theme changed (its files, or the hook's trigger file, are newer).
    /// Only while the theme follows the system. Returns whether to redraw.
    pub fn poll_omarchy_theme(&mut self) -> bool {
        use crate::config::ThemeSource;
        let follows = self.ui_theme_choice.is_none()
            && self.is_omarchy
            && matches!(
                self.config.theme.source,
                ThemeSource::Auto | ThemeSource::Omarchy
            );
        if !follows
            || self
                .last_theme_poll
                .is_some_and(|t| t.elapsed() < THEME_POLL)
        {
            return false;
        }
        self.last_theme_poll = Some(Instant::now());
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let stamp = [
            Some(self.omarchy_paths.theme_name_file()),
            Some(self.omarchy_paths.theme_colors_file()),
            self.theme_trigger_file(),
        ]
        .into_iter()
        .flatten()
        .filter_map(|p| modified(&p))
        .max();
        let changed = self.theme_stamp.is_some() && stamp != self.theme_stamp;
        self.theme_stamp = stamp;
        if changed {
            self.load_ui_theme();
            let name = self.ui_theme.name.clone();
            self.set_status(StatusKind::Info, format!("Omarchy theme changed: {name}"));
        }
        changed
    }

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

    #[test]
    fn omarchy_theme_changes_are_picked_up() {
        let dir =
            std::env::temp_dir().join(format!("glyphforge-theme-poll-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let theme_dir = dir.join("config/current/theme");
        std::fs::create_dir_all(&theme_dir).unwrap();
        std::fs::write(dir.join("config/current/theme.name"), "test-theme\n").unwrap();
        let colors = |accent: &str| {
            format!(
                "accent = \"{accent}\"\nforeground = \"#cccccc\"\nbackground = \"#111111\"\n{}",
                (0..16).fold(String::new(), |mut out, i| {
                    use std::fmt::Write as _;
                    let _ = writeln!(
                        out,
                        "color{i} = \"#{:02x}{:02x}{:02x}\"",
                        i * 10,
                        i * 9,
                        i * 8
                    );
                    out
                })
            )
        };
        std::fs::write(theme_dir.join("colors.toml"), colors("#ff0000")).unwrap();
        let mut app = test_app(Size::new(4, 2));
        app.omarchy_paths = crate::omarchy::OmarchyPaths {
            install_dir: dir.join("install"),
            config_dir: dir.join("config"),
        };
        app.is_omarchy = true;
        app.config.theme.source = crate::config::ThemeSource::Omarchy;
        app.state_dir = Some(dir.join("state"));
        app.load_ui_theme();
        assert!(
            !app.poll_omarchy_theme(),
            "first poll only records the stamp"
        );
        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(theme_dir.join("colors.toml"), colors("#00ff00")).unwrap();
        app.last_theme_poll = None;
        assert!(app.poll_omarchy_theme(), "a change reloads");
        assert_eq!(
            app.ui_theme.color("primary"),
            glyphforge_core::Color::Rgb(0, 255, 0)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
