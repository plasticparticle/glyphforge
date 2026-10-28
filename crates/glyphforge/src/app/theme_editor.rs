//! Themes beyond the built-in ones: custom theme files in the config
//! directory (`themes/<id>.toml`), the theme editor (every token with a
//! swatch, edited live), and choosing the document's theme.
//!
//! Editing the editor theme previews on the chrome and saves a theme file;
//! editing the document theme previews on the canvas and saves as one
//! undoable `set_theme` patch.

use std::path::{Path, PathBuf};

use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::theme::COLOR_TOKENS;
use glyphforge_core::{Color, ObjectId, Theme};

use super::prompt::{Prompt, PromptKind};
use super::{App, StatusKind, UiThemeOrigin};

/// Optional tokens the editor offers after the required ones.
const EXTRA_TOKENS: [&str; 2] = ["frame-from", "frame-to"];

/// Which theme the editor changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeTarget {
    /// The editor chrome.
    Ui,
    /// The design's own theme.
    Document,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeEditor {
    pub target: ThemeTarget,
    pub theme: Theme,
    pub selected: usize,
    /// The chrome theme to restore when an editor-theme edit is cancelled.
    restore: Option<(Theme, UiThemeOrigin)>,
}

/// Every token the editor lists.
pub fn editor_tokens() -> Vec<&'static str> {
    COLOR_TOKENS.iter().copied().chain(EXTRA_TOKENS).collect()
}

/// Reads every `*.toml` theme in `dir`; returns the themes and a warning
/// per file that could not be read.
pub fn load_themes(dir: &Path) -> (Vec<Theme>, Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    paths.sort();
    let mut themes = Vec::new();
    let mut warnings = Vec::new();
    for path in paths {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| toml::from_str::<Theme>(&t).map_err(|e| e.to_string()))
        {
            Ok(theme) => themes.push(theme),
            Err(e) => warnings.push(format!("Theme {} ignored: {e}", path.display())),
        }
    }
    (themes, warnings)
}

/// Writes `theme` to `dir/<id>.toml`.
pub fn save_theme(dir: &Path, theme: &Theme) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.toml", theme.id));
    let text = toml::to_string_pretty(theme).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

impl App {
    /// Built-in themes followed by the user's theme files.
    pub fn all_themes(&self) -> Vec<Theme> {
        let mut out = Theme::builtin();
        for t in &self.custom_themes {
            out.retain(|b| b.id != t.id);
            out.push(t.clone());
        }
        out
    }

    pub fn theme_by_id(&self, id: &str) -> Option<Theme> {
        self.all_themes().into_iter().find(|t| t.id.as_str() == id)
    }

    /// Reads the theme files from the themes directory.
    pub fn load_custom_themes(&mut self) {
        let Some(dir) = self.themes_dir.clone() else {
            return;
        };
        let (themes, warnings) = load_themes(&dir);
        self.custom_themes = themes;
        for w in warnings {
            self.set_status(StatusKind::Warning, w);
        }
    }

    pub fn open_theme_editor(&mut self, target: ThemeTarget) {
        self.menu = None;
        let theme = match target {
            ThemeTarget::Ui => self.ui_theme.clone(),
            ThemeTarget::Document => self.doc().theme.clone(),
        };
        let restore =
            (target == ThemeTarget::Ui).then(|| (self.ui_theme.clone(), self.ui_theme_origin));
        self.theme_editor = Some(ThemeEditor {
            target,
            theme,
            selected: 0,
            restore,
        });
    }

    pub fn theme_editor_move(&mut self, down: bool) {
        let n = editor_tokens().len();
        if let Some(e) = self.theme_editor.as_mut() {
            e.selected = if down {
                (e.selected + 1) % n
            } else {
                (e.selected + n - 1) % n
            };
        }
    }

    /// Enter: a colour prompt for the highlighted token.
    pub fn theme_editor_edit(&mut self) {
        let Some(e) = &self.theme_editor else { return };
        let token = editor_tokens()[e.selected];
        let current = e
            .theme
            .colors
            .get(token)
            .map_or_else(String::new, ToString::to_string);
        let mut candidates: Vec<String> =
            self.paint.recent.iter().map(ToString::to_string).collect();
        candidates.extend(super::paint::COLOR_NAMES.iter().map(|n| (*n).to_owned()));
        candidates.push("default".to_owned());
        self.prompt = Some(
            Prompt::new(
                PromptKind::ThemeToken,
                format!("{token}: name, 0-255, #rrggbb (empty: unset)"),
                candidates,
            )
            .with_input(current),
        );
    }

    pub(super) fn submit_theme_token(&mut self, value: &str) {
        let value = value.trim();
        let Some(e) = self.theme_editor.as_mut() else {
            return;
        };
        let token = editor_tokens()[e.selected];
        if value.is_empty() {
            e.theme.colors.remove(token);
        } else if let Some(color) = super::paint::parse_color(value) {
            e.theme.colors.insert(token.to_owned(), color);
        } else {
            return self.set_status(StatusKind::Warning, format!("Unknown colour {value:?}"));
        }
        self.preview_theme_edit();
    }

    /// Sets the highlighted token to a colour (from the colour picker).
    pub fn theme_editor_set(&mut self, color: Color) {
        let Some(e) = self.theme_editor.as_mut() else {
            return;
        };
        let token = editor_tokens()[e.selected];
        e.theme.colors.insert(token.to_owned(), color);
        self.preview_theme_edit();
    }

    fn preview_theme_edit(&mut self) {
        if let Some(e) = &self.theme_editor {
            if e.target == ThemeTarget::Ui {
                self.ui_theme = e.theme.clone();
            }
        }
    }

    pub fn theme_editor_rename(&mut self) {
        let Some(e) = &self.theme_editor else { return };
        let name = e.theme.name.clone();
        self.prompt =
            Some(Prompt::new(PromptKind::ThemeName, "Theme name", vec![]).with_input(name));
    }

    pub(super) fn submit_theme_name(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(e) = self.theme_editor.as_mut() {
            name.clone_into(&mut e.theme.name);
            e.theme.id = ObjectId::slugify(name);
        }
    }

    /// `s`: the editor theme becomes a theme file and the active theme;
    /// the document theme is set by one undoable patch.
    pub fn theme_editor_save(&mut self) {
        let Some(e) = self.theme_editor.take() else {
            return;
        };
        match e.target {
            ThemeTarget::Ui => {
                let Some(dir) = self.themes_dir.clone() else {
                    self.ui_theme = e.theme;
                    return self
                        .set_status(StatusKind::Warning, "No config directory to save themes in");
                };
                match save_theme(&dir, &e.theme) {
                    Ok(path) => {
                        self.load_custom_themes();
                        let id = e.theme.id.to_string();
                        self.set_ui_theme(&id);
                        self.set_status(
                            StatusKind::Info,
                            format!("Saved theme {} to {}", e.theme.name, path.display()),
                        );
                    }
                    Err(err) => {
                        self.set_status(StatusKind::Error, format!("Cannot save theme: {err}"));
                    }
                }
            }
            ThemeTarget::Document => {
                let name = e.theme.name.clone();
                if self.apply(
                    Patch::single(Operation::SetTheme { theme: e.theme }),
                    "Edit document theme",
                ) {
                    self.set_status(StatusKind::Info, format!("Document theme: {name}"));
                }
            }
        }
    }

    /// Esc: drops the edit and restores the chrome theme.
    pub fn theme_editor_cancel(&mut self) {
        if let Some(e) = self.theme_editor.take() {
            if let Some((theme, origin)) = e.restore {
                self.ui_theme = theme;
                self.ui_theme_origin = origin;
            }
            self.set_status(StatusKind::Info, "Theme edit cancelled");
        }
    }

    /// Sets the document's theme to a built-in or custom theme (undoable).
    pub fn set_document_theme(&mut self, id: &str) {
        let Some(theme) = self.theme_by_id(id) else {
            return self.set_status(StatusKind::Warning, format!("No theme {id:?}"));
        };
        let name = theme.name.clone();
        if self.apply(
            Patch::single(Operation::SetTheme { theme }),
            "Set document theme",
        ) {
            self.set_status(StatusKind::Info, format!("Document theme: {name}"));
        }
    }

    /// The theme the canvas should be drawn with while the document theme
    /// is being edited.
    pub fn document_theme_preview(&self) -> Option<&Theme> {
        self.theme_editor
            .as_ref()
            .filter(|e| e.target == ThemeTarget::Document)
            .map(|e| &e.theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use glyphforge_core::Size;

    #[test]
    fn theme_files_round_trip_and_load() {
        let dir = std::env::temp_dir().join(format!("glyphforge-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut theme = Theme::builtin_by_id("cobalt").unwrap();
        theme.id = ObjectId::slugify("my-cobalt");
        theme.name = "My Cobalt".into();
        save_theme(&dir, &theme).unwrap();
        std::fs::write(dir.join("broken.toml"), "not = [a theme").unwrap();
        let (themes, warnings) = load_themes(&dir);
        assert_eq!(themes, vec![theme]);
        assert_eq!(warnings.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn editing_the_document_theme_previews_then_applies_once() {
        let mut app = test_app(Size::new(10, 3));
        app.open_theme_editor(ThemeTarget::Document);
        let primary = editor_tokens()
            .iter()
            .position(|t| *t == "primary")
            .unwrap();
        app.theme_editor.as_mut().unwrap().selected = primary;
        app.submit_theme_token("#ff0000");
        assert_eq!(
            app.document_theme_preview().unwrap().color("primary"),
            Color::Rgb(255, 0, 0)
        );
        assert_ne!(app.doc().theme.color("primary"), Color::Rgb(255, 0, 0));
        app.theme_editor_save();
        assert_eq!(app.doc().theme.color("primary"), Color::Rgb(255, 0, 0));
        assert_eq!(app.session.history().undo_len(), 1);
        app.set_document_theme("paper");
        assert_eq!(app.doc().theme.id.as_str(), "paper");
    }

    #[test]
    fn cancelling_an_editor_theme_edit_restores_the_chrome() {
        let mut app = test_app(Size::new(10, 3));
        let before = app.ui_theme.clone();
        app.open_theme_editor(ThemeTarget::Ui);
        app.theme_editor_set(Color::Rgb(1, 2, 3));
        assert_ne!(app.ui_theme, before, "live preview");
        app.theme_editor_cancel();
        assert_eq!(app.ui_theme, before);
    }
}
