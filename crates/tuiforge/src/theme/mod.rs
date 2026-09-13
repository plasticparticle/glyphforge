//! Semantic UI theme roles. Widgets use roles, never literal colours.

use std::fmt;

use tuiforge_core::Color;

/// Where the active theme came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeOrigin {
    Builtin,
    Omarchy,
}

impl fmt::Display for ThemeOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Builtin => "builtin",
            Self::Omarchy => "omarchy",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTheme {
    pub name: String,
    pub origin: ThemeOrigin,
    pub background: Color,
    pub panel_background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection_fg: Color,
    pub selection_bg: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    pub border: Color,
    pub cursor: Color,
}

impl UiTheme {
    /// The built-in theme uses only the terminal's default colours and the
    /// ANSI 16 palette, so it adapts to any terminal colour scheme.
    pub fn builtin() -> Self {
        Self {
            name: "Terminal".into(),
            origin: ThemeOrigin::Builtin,
            background: Color::Default,
            panel_background: Color::Default,
            foreground: Color::Default,
            muted: Color::BRIGHT_BLACK,
            accent: Color::BRIGHT_BLUE,
            selection_fg: Color::BLACK,
            selection_bg: Color::BRIGHT_BLUE,
            error: Color::RED,
            warning: Color::YELLOW,
            success: Color::GREEN,
            info: Color::CYAN,
            border: Color::BRIGHT_BLACK,
            cursor: Color::Default,
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::builtin()
    }
}
