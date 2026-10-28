//! The Esc menu: a small popup with the things you reach for when you stop
//! drawing — themes, help, saving and quitting.

use std::path::PathBuf;

use glyphforge_core::{Color, Theme};

use super::{App, StatusKind, UiThemeOrigin};
use crate::actions::{Action, Direction, MenuAction, MenuPage};

/// The theme choice that follows Omarchy, or the terminal's own palette.
pub const SYSTEM_THEME: &str = "system";

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    /// Shown dimmed on the right: a key binding or a short explanation.
    pub hint: String,
    pub action: Action,
    /// Marked as the current choice.
    pub active: bool,
    /// Colours previewed in front of the label (theme entries).
    pub swatch: Vec<Color>,
}

/// What leaving the theme page with Esc puts back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeRestore {
    /// Nothing to undo: not previewing, or the theme was kept with Enter.
    Kept,
    /// The choice the page started with: a theme id or [`SYSTEM_THEME`].
    Previous(String),
}

/// The open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub page: MenuPage,
    pub selected: usize,
    /// What Esc puts back after a live preview.
    pub restore: ThemeRestore,
}

impl Menu {
    pub const fn new(page: MenuPage) -> Self {
        Self {
            page,
            selected: 0,
            restore: ThemeRestore::Kept,
        }
    }
}

impl App {
    pub(super) fn dispatch_menu(&mut self, action: MenuAction) {
        match action {
            MenuAction::Open => self.menu = Some(Menu::new(MenuPage::Main)),
            MenuAction::Close => self.close_menu(),
            MenuAction::Page(page) => self.open_page(page),
            MenuAction::Move(dir) => self.move_menu(dir),
            MenuAction::Click(index) => {
                if let Some(menu) = self.menu.as_mut() {
                    menu.selected = index;
                }
                self.dispatch_menu(MenuAction::Select);
            }
            MenuAction::Select => {
                if let Some(action) = self.selected_menu_action() {
                    // Menu actions stay in the menu (paging, trying out
                    // themes); everything else closes it first.
                    if !matches!(action, Action::Menu(_)) {
                        self.menu = None;
                    }
                    self.dispatch(action);
                }
            }
            MenuAction::SetTheme(choice) => {
                // Enter keeps the previewed theme: remember it and leave
                // the theme page.
                self.set_ui_theme(&choice);
                if let Some(menu) = self.menu.as_mut() {
                    menu.restore = ThemeRestore::Kept;
                }
                if self.menu.is_some() {
                    self.open_page(MenuPage::Main);
                }
            }
        }
    }

    /// Opens a page. The theme page starts on the current theme and
    /// remembers it, so moving the selection can preview themes live.
    fn open_page(&mut self, page: MenuPage) {
        let mut menu = Menu::new(page);
        if page != MenuPage::Theme {
            self.menu = Some(menu);
            return;
        }
        menu.restore = ThemeRestore::Previous(
            self.ui_theme_choice
                .clone()
                .unwrap_or_else(|| SYSTEM_THEME.to_owned()),
        );
        self.menu = Some(menu);
        let active = self.menu_entries().iter().position(|e| e.active);
        if let (Some(index), Some(menu)) = (active, self.menu.as_mut()) {
            menu.selected = index;
        }
    }

    /// Closes the menu, or leaves the theme page putting back the theme
    /// that was active before the preview started.
    fn close_menu(&mut self) {
        match self.menu.as_ref().map(|m| m.restore.clone()) {
            Some(ThemeRestore::Previous(choice)) => {
                self.apply_ui_theme(&choice, false);
                self.open_page(MenuPage::Main);
            }
            _ => self.menu = None,
        }
    }

    fn move_menu(&mut self, dir: Direction) {
        let count = self.menu_entries().len().max(1);
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        menu.selected = match dir {
            Direction::Up => (menu.selected + count - 1) % count,
            Direction::Down => (menu.selected + 1) % count,
            Direction::Left => return self.close_menu(),
            Direction::Right => menu.selected,
        };
        self.preview_selected_theme();
    }

    /// On the theme page, shows the theme the cursor is on without
    /// remembering it yet.
    fn preview_selected_theme(&mut self) {
        if self.menu.as_ref().is_none_or(|m| m.page != MenuPage::Theme) {
            return;
        }
        if let Some(Action::Menu(MenuAction::SetTheme(choice))) = self.selected_menu_action() {
            self.apply_ui_theme(&choice, false);
        }
    }

    fn selected_menu_action(&self) -> Option<Action> {
        let menu = self.menu.as_ref()?;
        self.menu_entries()
            .into_iter()
            .nth(menu.selected)
            .map(|e| e.action)
    }

    /// The first key bound to `action`, for the hint column.
    fn key_hint(&self, action: &Action) -> String {
        self.keymap
            .chords_for(action)
            .first()
            .map_or_else(String::new, ToString::to_string)
    }

    /// The entries of the open menu page.
    pub fn menu_entries(&self) -> Vec<MenuEntry> {
        let entry = |label: &str, hint: String, action: Action| MenuEntry {
            label: label.to_owned(),
            hint,
            action,
            active: false,
            swatch: Vec::new(),
        };
        match self.menu.as_ref().map(|m| m.page) {
            Some(MenuPage::Main) => {
                let theme = format!("{} ›", self.ui_theme.name);
                vec![
                    entry("Resume", "Esc".to_owned(), Action::Menu(MenuAction::Close)),
                    entry(
                        "Theme",
                        theme,
                        Action::Menu(MenuAction::Page(MenuPage::Theme)),
                    ),
                    entry(
                        "Help",
                        self.key_hint(&Action::ToggleHelp),
                        Action::ToggleHelp,
                    ),
                    entry(
                        "Save",
                        self.key_hint(&Action::SaveDocument),
                        Action::SaveDocument,
                    ),
                    entry(
                        "Save as",
                        self.key_hint(&Action::SaveDocumentAs),
                        Action::SaveDocumentAs,
                    ),
                    entry(
                        "New document",
                        self.key_hint(&Action::NewDocument),
                        Action::NewDocument,
                    ),
                    entry("Quit", self.key_hint(&Action::Quit), Action::Quit),
                ]
            }
            Some(MenuPage::Theme) => {
                let system = self.ui_theme_choice.is_none();
                let mut entries = vec![MenuEntry {
                    active: system,
                    hint: match self.ui_theme_origin {
                        UiThemeOrigin::Omarchy => "follows Omarchy".to_owned(),
                        _ => "follows the terminal".to_owned(),
                    },
                    ..entry(
                        "System",
                        String::new(),
                        Action::Menu(MenuAction::SetTheme(SYSTEM_THEME.to_owned())),
                    )
                }];
                entries.extend(Theme::builtin().into_iter().map(|theme| {
                    let id = theme.id.to_string();
                    MenuEntry {
                        label: theme.name.clone(),
                        hint: if theme.id.as_str() == "terminal" {
                            "the terminal's own 16 colours".to_owned()
                        } else {
                            String::new()
                        },
                        action: Action::Menu(MenuAction::SetTheme(id.clone())),
                        active: !system && self.ui_theme_choice.as_deref() == Some(id.as_str()),
                        swatch: ["primary", "secondary", "foreground", "surface"]
                            .iter()
                            .map(|t| theme.color(t))
                            .collect(),
                    }
                }));
                entries.push(entry(
                    "Use in document",
                    self.key_hint(&Action::UseUiThemeInDocument),
                    Action::UseUiThemeInDocument,
                ));
                entries.push(entry(
                    "Cancel",
                    "Esc".to_owned(),
                    Action::Menu(MenuAction::Close),
                ));
                entries
            }
            None => Vec::new(),
        }
    }

    /// Applies a theme choice: a built-in theme id, or [`SYSTEM_THEME`] to
    /// follow Omarchy or the terminal. The choice is remembered for the
    /// next start.
    pub fn set_ui_theme(&mut self, choice: &str) {
        self.apply_ui_theme(choice, true);
    }

    /// Applies a theme, remembering it for the next start when `persist`
    /// (a live preview does not).
    fn apply_ui_theme(&mut self, choice: &str, persist: bool) {
        if choice == SYSTEM_THEME {
            self.ui_theme_choice = None;
            self.load_ui_theme();
            let text = format!("Theme: system ({})", self.ui_theme.name);
            self.set_status(StatusKind::Info, text);
        } else {
            let Some(theme) = Theme::builtin_by_id(choice) else {
                return self.set_status(StatusKind::Warning, format!("No theme {choice:?}"));
            };
            let text = format!("Theme: {}", theme.name);
            self.ui_theme = theme;
            self.ui_theme_origin = UiThemeOrigin::Named;
            self.ui_theme_choice = Some(choice.to_owned());
            self.set_status(StatusKind::Info, text);
        }
        if persist {
            self.remember_ui_theme();
        }
    }

    /// The file remembering the chosen theme between runs.
    fn ui_theme_file(&self) -> Option<PathBuf> {
        self.state_dir.as_ref().map(|d| d.join("ui-theme"))
    }

    fn remember_ui_theme(&self) {
        let Some(path) = self.ui_theme_file() else {
            return;
        };
        let choice = self.ui_theme_choice.as_deref().unwrap_or(SYSTEM_THEME);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, format!("{choice}\n")) {
            tracing::warn!("cannot remember the theme in {}: {e}", path.display());
        }
    }

    /// Applies the configured theme, or the one remembered from last time.
    pub(super) fn restore_ui_theme(&mut self) {
        let configured = self.config.theme.ui.clone();
        let remembered = || {
            let path = self.ui_theme_file()?;
            let text = std::fs::read_to_string(path).ok()?;
            Some(text.trim().to_owned()).filter(|s| !s.is_empty())
        };
        let Some(choice) = configured.or_else(remembered) else {
            return;
        };
        if choice == SYSTEM_THEME {
            return;
        }
        if let Some(theme) = Theme::builtin_by_id(&choice) {
            self.ui_theme = theme;
            self.ui_theme_origin = UiThemeOrigin::Named;
            self.ui_theme_choice = Some(choice);
        } else {
            tracing::warn!("unknown theme {choice:?}; using the system theme");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::Size;

    fn menu(action: MenuAction) -> Action {
        Action::Menu(action)
    }

    fn labels(app: &App) -> Vec<String> {
        app.menu_entries().into_iter().map(|e| e.label).collect()
    }

    #[test]
    fn escape_opens_and_closes_the_menu() {
        let mut app = test_app(Size::new(8, 2));
        app.dispatch(Action::Cancel);
        assert_eq!(labels(&app).first().map(String::as_str), Some("Resume"));
        app.dispatch(Action::Cancel);
        assert!(app.menu.is_none());

        // Esc keeps cancelling what is in progress before it opens the menu.
        app.dispatch(Action::SaveDocumentAs);
        app.dispatch(Action::Cancel);
        assert!(app.prompt.is_none() && app.menu.is_none());
    }

    #[test]
    fn navigating_wraps_and_choosing_closes_the_menu() {
        let mut app = test_app(Size::new(8, 2));
        app.dispatch(menu(MenuAction::Open));
        app.dispatch(menu(MenuAction::Move(Direction::Up)));
        assert_eq!(app.menu.as_ref().unwrap().selected, labels(&app).len() - 1);
        app.dispatch(menu(MenuAction::Move(Direction::Down)));
        assert_eq!(app.menu.as_ref().unwrap().selected, 0);

        // "Help" opens the overlay and leaves the menu.
        let help = labels(&app).iter().position(|l| l == "Help").unwrap();
        app.dispatch(menu(MenuAction::Click(help)));
        assert!(app.menu.is_none() && app.ui.help_open);

        // "Quit" quits.
        app.dispatch(Action::ToggleHelp);
        app.dispatch(menu(MenuAction::Open));
        let quit = labels(&app).iter().position(|l| l == "Quit").unwrap();
        app.dispatch(menu(MenuAction::Click(quit)));
        assert!(app.should_quit());
    }

    #[test]
    fn scrolling_previews_themes_and_escape_puts_the_old_one_back() {
        let mut app = test_app(Size::new(8, 2));
        app.set_ui_theme("cobalt");
        app.dispatch(menu(MenuAction::Open));
        app.dispatch(menu(MenuAction::Page(MenuPage::Theme)));
        let before = app.ui_theme.id.clone();

        // Moving the selection shows the theme at once, without keeping it.
        app.dispatch(menu(MenuAction::Move(Direction::Down)));
        let previewed = app.ui_theme.id.clone();
        assert_ne!(previewed, before, "the theme follows the selection");
        assert_eq!(app.ui_theme_choice.as_deref(), Some(previewed.as_str()));

        // Esc leaves the page and puts the old theme back.
        app.dispatch(menu(MenuAction::Close));
        assert_eq!(app.ui_theme.id, before);
        assert_eq!(app.menu.as_ref().map(|m| m.page), Some(MenuPage::Main));

        // Enter keeps the previewed one instead.
        app.dispatch(menu(MenuAction::Page(MenuPage::Theme)));
        app.dispatch(menu(MenuAction::Move(Direction::Down)));
        let kept = app.ui_theme.id.clone();
        app.dispatch(menu(MenuAction::Select));
        assert_eq!(app.ui_theme.id, kept);
        app.dispatch(menu(MenuAction::Page(MenuPage::Theme)));
        app.dispatch(menu(MenuAction::Close));
        assert_eq!(
            app.ui_theme.id, kept,
            "a kept theme survives a later cancel"
        );
    }

    #[test]
    fn the_theme_page_applies_and_remembers_a_theme() {
        let dir = std::env::temp_dir().join(format!("glyphforge-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = test_app(Size::new(8, 2));
        app.state_dir = Some(dir.clone());

        app.dispatch(menu(MenuAction::Open));
        let theme_entry = labels(&app).iter().position(|l| l == "Theme").unwrap();
        app.dispatch(menu(MenuAction::Click(theme_entry)));
        assert!(app.menu.is_some(), "the menu stays open on its own pages");
        let entries = app.menu_entries();
        assert_eq!(entries.first().map(|e| e.label.as_str()), Some("System"));
        assert!(entries[0].active, "the system theme is the default");
        assert!(
            entries
                .iter()
                .any(|e| e.label == "Cobalt" && e.swatch.len() == 4)
        );

        let cobalt = entries.iter().position(|e| e.label == "Cobalt").unwrap();
        app.dispatch(menu(MenuAction::Click(cobalt)));
        assert_eq!(app.ui_theme.id.as_str(), "cobalt");
        assert_eq!(app.ui_theme_origin, UiThemeOrigin::Named);
        assert_eq!(
            app.menu.as_ref().map(|m| m.page),
            Some(MenuPage::Main),
            "choosing a theme returns to the menu"
        );

        // Re-opening the page starts on the theme in use.
        app.dispatch(menu(MenuAction::Page(MenuPage::Theme)));
        assert_eq!(app.menu.as_ref().unwrap().selected, cobalt);
        assert!(app.menu_entries()[cobalt].active);
        assert_eq!(
            std::fs::read_to_string(dir.join("ui-theme"))
                .unwrap()
                .trim(),
            "cobalt"
        );

        // A new session restores it.
        let mut restored = test_app(Size::new(8, 2));
        restored.state_dir = Some(dir.clone());
        restored.restore_ui_theme();
        assert_eq!(restored.ui_theme.id.as_str(), "cobalt");

        // The configured theme wins over the remembered one.
        let mut configured = test_app(Size::new(8, 2));
        configured.state_dir = Some(dir.clone());
        configured.config.theme.ui = Some("phosphor".to_owned());
        configured.restore_ui_theme();
        assert_eq!(configured.ui_theme.id.as_str(), "phosphor");

        // Back to the system theme.
        app.set_ui_theme(SYSTEM_THEME);
        assert_eq!(app.ui_theme.id.as_str(), "terminal");
        assert!(app.ui_theme_choice.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.join("ui-theme"))
                .unwrap()
                .trim(),
            "system"
        );
        app.set_ui_theme("nope");
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
