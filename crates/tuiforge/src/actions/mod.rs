//! The central action model. Every user-visible behaviour is an `Action`;
//! keyboard, mouse, menus and the command palette all dispatch through it.

use std::sync::OnceLock;

use tuiforge_core::Position;

/// Direction for cursor and viewport movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Application actions. Variants without UI-geometry payloads are
/// "bindable" and have an [`ActionDescriptor`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    // Application
    Quit,
    ToggleHelp,
    Cancel,
    ReloadTheme,
    // View
    ToggleLeftPanel,
    ToggleRightPanel,
    TogglePanels,
    FocusNext,
    FocusPrev,
    ScrollView(Direction),
    // Cursor
    CursorMove(Direction),
    CursorLineStart,
    CursorLineEnd,
    CursorPageUp,
    CursorPageDown,
    CursorTo(Position),
    // Editing (Milestone 1 minimum; routed through history from M5)
    InsertText(String),
    NewLine,
    Backspace,
    DeleteForward,
    // Placeholders for the descriptor table only: dispatched actions that
    // are not implemented yet report "not implemented" in the status bar.
    // They exist so default key bindings and the help overlay are complete
    // and stable across milestones.
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    DeleteSelection,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Application,
    File,
    Edit,
    View,
    Cursor,
}

impl Category {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Cursor => "Cursor",
        }
    }
}

/// Metadata for a bindable action: its stable config name, title, search
/// keywords and aliases (command palette), category and default keys.
#[derive(Debug, Clone)]
pub struct ActionDescriptor {
    pub name: &'static str,
    pub title: &'static str,
    pub category: Category,
    #[allow(
        dead_code,
        reason = "consumed by the command palette fuzzy matcher (Milestone 9)"
    )]
    pub keywords: &'static [&'static str],
    #[allow(
        dead_code,
        reason = "consumed by the command palette fuzzy matcher (Milestone 9)"
    )]
    pub aliases: &'static [&'static str],
    pub default_keys: &'static [&'static str],
    pub action: Action,
    /// Whether dispatching does something in the current milestone.
    pub implemented: bool,
}

macro_rules! desc {
    ($name:literal, $title:literal, $cat:ident, $action:expr, keys: [$($k:literal),*], kw: [$($kw:literal),*], aliases: [$($al:literal),*], implemented: $impl:literal) => {
        ActionDescriptor {
            name: $name,
            title: $title,
            category: Category::$cat,
            keywords: &[$($kw),*],
            aliases: &[$($al),*],
            default_keys: &[$($k),*],
            action: $action,
            implemented: $impl,
        }
    };
}

/// All bindable actions in a stable order.
pub fn descriptors() -> &'static [ActionDescriptor] {
    static TABLE: OnceLock<Vec<ActionDescriptor>> = OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            desc!("quit", "Quit", Application, Action::Quit, keys: ["ctrl+q"], kw: ["exit", "close"], aliases: ["q"], implemented: true),
            desc!("toggle-help", "Toggle Help", Application, Action::ToggleHelp, keys: ["f1"], kw: ["keys", "shortcuts", "bindings"], aliases: ["help"], implemented: true),
            desc!("cancel", "Cancel", Application, Action::Cancel, keys: ["esc"], kw: ["escape", "abort"], aliases: [], implemented: true),
            desc!("reload-theme", "Reload Theme", Application, Action::ReloadTheme, keys: ["ctrl+shift+t"], kw: ["omarchy", "colors", "palette"], aliases: [], implemented: true),
            desc!("command-palette", "Command Palette", Application, Action::CommandPalette, keys: ["ctrl+space", "ctrl+p"], kw: ["search", "actions", "run"], aliases: ["palette"], implemented: false),
            desc!("new-document", "New Document", File, Action::NewDocument, keys: ["ctrl+n"], kw: ["create", "canvas"], aliases: ["new"], implemented: false),
            desc!("open-document", "Open Document", File, Action::OpenDocument, keys: ["ctrl+o"], kw: ["load", "file"], aliases: ["open"], implemented: false),
            desc!("save-document", "Save Document", File, Action::SaveDocument, keys: ["ctrl+s"], kw: ["write", "file"], aliases: ["save"], implemented: false),
            desc!("save-document-as", "Save Document As", File, Action::SaveDocumentAs, keys: ["ctrl+shift+s"], kw: ["write", "file", "rename"], aliases: ["save as"], implemented: false),
            desc!("undo", "Undo", Edit, Action::Undo, keys: ["ctrl+z"], kw: ["history", "revert"], aliases: [], implemented: false),
            desc!("redo", "Redo", Edit, Action::Redo, keys: ["ctrl+y", "ctrl+shift+z"], kw: ["history", "repeat"], aliases: [], implemented: false),
            desc!("copy", "Copy", Edit, Action::Copy, keys: ["ctrl+c"], kw: ["clipboard", "selection"], aliases: [], implemented: false),
            desc!("cut", "Cut", Edit, Action::Cut, keys: ["ctrl+x"], kw: ["clipboard", "selection"], aliases: [], implemented: false),
            desc!("paste", "Paste", Edit, Action::Paste, keys: ["ctrl+v"], kw: ["clipboard", "insert"], aliases: [], implemented: false),
            desc!("delete-selection", "Delete Selection", Edit, Action::DeleteSelection, keys: ["ctrl+delete"], kw: ["clear", "remove"], aliases: [], implemented: false),
            desc!("backspace", "Erase Left", Edit, Action::Backspace, keys: ["backspace"], kw: ["delete", "erase"], aliases: [], implemented: true),
            desc!("delete-forward", "Erase At Cursor", Edit, Action::DeleteForward, keys: ["delete"], kw: ["delete", "erase", "clear"], aliases: [], implemented: true),
            desc!("new-line", "New Line", Edit, Action::NewLine, keys: ["enter"], kw: ["return", "next row"], aliases: [], implemented: true),
            desc!("toggle-left-panel", "Toggle Left Panel", View, Action::ToggleLeftPanel, keys: ["f2"], kw: ["tools", "sidebar", "hide", "show"], aliases: [], implemented: true),
            desc!("toggle-right-panel", "Toggle Right Panel", View, Action::ToggleRightPanel, keys: ["f3"], kw: ["layers", "properties", "sidebar", "hide", "show"], aliases: [], implemented: true),
            desc!("toggle-panels", "Toggle All Panels", View, Action::TogglePanels, keys: ["f4"], kw: ["zen", "focus", "fullscreen", "hide", "show"], aliases: ["zen mode"], implemented: true),
            desc!("focus-next", "Focus Next Region", View, Action::FocusNext, keys: ["tab"], kw: ["panel", "cycle"], aliases: [], implemented: true),
            desc!("focus-prev", "Focus Previous Region", View, Action::FocusPrev, keys: ["shift+tab"], kw: ["panel", "cycle"], aliases: [], implemented: true),
            desc!("scroll-up", "Scroll View Up", View, Action::ScrollView(Direction::Up), keys: ["ctrl+up"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-down", "Scroll View Down", View, Action::ScrollView(Direction::Down), keys: ["ctrl+down"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-left", "Scroll View Left", View, Action::ScrollView(Direction::Left), keys: ["ctrl+left"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-right", "Scroll View Right", View, Action::ScrollView(Direction::Right), keys: ["ctrl+right"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("cursor-up", "Cursor Up", Cursor, Action::CursorMove(Direction::Up), keys: ["up"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-down", "Cursor Down", Cursor, Action::CursorMove(Direction::Down), keys: ["down"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-left", "Cursor Left", Cursor, Action::CursorMove(Direction::Left), keys: ["left"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-right", "Cursor Right", Cursor, Action::CursorMove(Direction::Right), keys: ["right"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-line-start", "Cursor To Line Start", Cursor, Action::CursorLineStart, keys: ["home"], kw: ["move", "begin"], aliases: [], implemented: true),
            desc!("cursor-line-end", "Cursor To Line End", Cursor, Action::CursorLineEnd, keys: ["end"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-page-up", "Cursor Page Up", Cursor, Action::CursorPageUp, keys: ["pageup"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-page-down", "Cursor Page Down", Cursor, Action::CursorPageDown, keys: ["pagedown"], kw: ["move"], aliases: [], implemented: true),
        ]
    })
}

/// Finds a descriptor by its stable name.
pub fn find_descriptor(name: &str) -> Option<&'static ActionDescriptor> {
    descriptors().iter().find(|d| d.name == name)
}

/// The descriptor of a bindable action, if it has one.
pub fn descriptor_of(action: &Action) -> Option<&'static ActionDescriptor> {
    descriptors().iter().find(|d| d.action == *action)
}

/// Vim-style navigation chords, applied on top of the keymap when enabled.
pub fn vim_navigation(key: &crate::input::Key) -> Option<Action> {
    use crate::input::KeyCode;
    if !key.mods.is_none() {
        return None;
    }
    match key.code {
        KeyCode::Char('h') => Some(Action::CursorMove(Direction::Left)),
        KeyCode::Char('j') => Some(Action::CursorMove(Direction::Down)),
        KeyCode::Char('k') => Some(Action::CursorMove(Direction::Up)),
        KeyCode::Char('l') => Some(Action::CursorMove(Direction::Right)),
        KeyCode::Char('0') => Some(Action::CursorLineStart),
        KeyCode::Char('$') => Some(Action::CursorLineEnd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_kebab_case() {
        let mut seen = HashSet::new();
        for d in descriptors() {
            assert!(seen.insert(d.name), "duplicate action name {}", d.name);
            assert!(
                d.name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{} is not kebab-case",
                d.name
            );
        }
    }

    #[test]
    fn actions_are_unique() {
        let mut seen = HashSet::new();
        for d in descriptors() {
            assert!(
                seen.insert(d.action.clone()),
                "duplicate action {:?}",
                d.action
            );
        }
    }

    #[test]
    fn default_keys_parse() {
        for d in descriptors() {
            for k in d.default_keys {
                assert!(
                    k.parse::<crate::input::KeyChord>().is_ok(),
                    "{} has bad key {k}",
                    d.name
                );
            }
        }
    }

    #[test]
    fn find_and_reverse_lookup_agree() {
        let d = find_descriptor("toggle-panels").unwrap();
        assert_eq!(
            descriptor_of(&d.action).map(|x| x.name),
            Some("toggle-panels")
        );
        assert!(find_descriptor("nope").is_none());
        assert!(descriptor_of(&Action::CursorTo(Position::ORIGIN)).is_none());
    }
}
