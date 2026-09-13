//! Themes and semantic style tokens.
//!
//! Components reference tokens such as `surface` or `primary`; a theme
//! resolves them to colours. The same document renders in any theme.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::boxdraw::BorderFamily;
use crate::document::Color;
use crate::id::ObjectId;

/// The semantic colour tokens every theme should define.
pub const COLOR_TOKENS: &[&str] = &[
    "background",
    "surface",
    "surface-alt",
    "border",
    "border-muted",
    "foreground",
    "foreground-muted",
    "primary",
    "secondary",
    "success",
    "warning",
    "error",
    "info",
    "selection",
    "selection-foreground",
    "focus",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spacing {
    pub padding: u16,
    pub gap: u16,
}

impl Default for Spacing {
    fn default() -> Self {
        Self { padding: 1, gap: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub id: ObjectId,
    pub name: String,
    /// Token name -> colour. Missing tokens fall back per [`Theme::color`].
    pub colors: BTreeMap<String, Color>,
    #[serde(default)]
    pub border: BorderFamily,
    #[serde(default)]
    pub spacing: Spacing,
}

impl Theme {
    /// Resolves a token, falling back along a small chain so partial
    /// themes still render sensibly: `surface*` -> `background`,
    /// `border*`/`foreground-muted`/`secondary` -> `foreground`, status
    /// colours -> `primary` -> `foreground` -> terminal default.
    pub fn color(&self, token: &str) -> Color {
        if let Some(c) = self.colors.get(token) {
            return *c;
        }
        let fallback = match token {
            "surface" | "surface-alt" | "selection-foreground" => "background",
            "border" | "border-muted" | "foreground-muted" | "secondary" | "primary" => {
                "foreground"
            }
            "success" | "warning" | "error" | "info" | "selection" | "focus" => "primary",
            _ => return Color::Default,
        };
        self.color(fallback)
    }

    /// Resolves either a token name or a literal colour (`#rrggbb`,
    /// `ansi:N`, `default`).
    pub fn resolve(&self, token_or_literal: &str) -> Color {
        if self.colors.contains_key(token_or_literal) || COLOR_TOKENS.contains(&token_or_literal) {
            return self.color(token_or_literal);
        }
        token_or_literal
            .parse()
            .unwrap_or_else(|_| self.color(token_or_literal))
    }

    /// Tokens listed in [`COLOR_TOKENS`] that this theme does not define.
    pub fn missing_tokens(&self) -> Vec<&'static str> {
        COLOR_TOKENS
            .iter()
            .copied()
            .filter(|t| !self.colors.contains_key(*t))
            .collect()
    }

    fn from_pairs(id: &str, name: &str, border: BorderFamily, pairs: &[(&str, Color)]) -> Self {
        Self {
            id: ObjectId::new(id).unwrap_or_else(|_| ObjectId::slugify(id)),
            name: name.to_owned(),
            colors: pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
            border,
            spacing: Spacing::default(),
        }
    }

    /// Uses only the terminal's own palette: works everywhere, looks like
    /// whatever the user's terminal looks like.
    pub fn terminal() -> Self {
        Self::from_pairs(
            "terminal",
            "Terminal",
            BorderFamily::Rounded,
            &[
                ("background", Color::Default),
                ("surface", Color::Default),
                ("surface-alt", Color::Default),
                ("border", Color::BRIGHT_BLACK),
                ("border-muted", Color::BRIGHT_BLACK),
                ("foreground", Color::Default),
                ("foreground-muted", Color::BRIGHT_BLACK),
                ("primary", Color::BRIGHT_BLUE),
                ("secondary", Color::BRIGHT_MAGENTA),
                ("success", Color::GREEN),
                ("warning", Color::YELLOW),
                ("error", Color::RED),
                ("info", Color::CYAN),
                ("selection", Color::BRIGHT_BLUE),
                ("selection-foreground", Color::BLACK),
                ("focus", Color::BRIGHT_BLUE),
            ],
        )
    }

    /// A restrained dark theme; original colours, no vendor palette.
    pub fn minimal_dark() -> Self {
        let rgb = Color::from_hex;
        let c = |h: &str| rgb(h).unwrap_or(Color::Default);
        Self::from_pairs(
            "minimal-dark",
            "Minimal Dark",
            BorderFamily::Rounded,
            &[
                ("background", c("#14161b")),
                ("surface", c("#1b1e25")),
                ("surface-alt", c("#22262f")),
                ("border", c("#3a3f4b")),
                ("border-muted", c("#2a2e37")),
                ("foreground", c("#d7dae0")),
                ("foreground-muted", c("#7c8291")),
                ("primary", c("#8ab4f8")),
                ("secondary", c("#c4a7e7")),
                ("success", c("#9ccc65")),
                ("warning", c("#ffca6b")),
                ("error", c("#ff7b72")),
                ("info", c("#79c0ff")),
                ("selection", c("#2f4f7f")),
                ("selection-foreground", c("#ffffff")),
                ("focus", c("#8ab4f8")),
            ],
        )
    }

    /// A restrained light theme.
    pub fn minimal_light() -> Self {
        let c = |h: &str| Color::from_hex(h).unwrap_or(Color::Default);
        Self::from_pairs(
            "minimal-light",
            "Minimal Light",
            BorderFamily::Rounded,
            &[
                ("background", c("#fafafa")),
                ("surface", c("#ffffff")),
                ("surface-alt", c("#f0f1f3")),
                ("border", c("#c9ccd1")),
                ("border-muted", c("#e1e3e7")),
                ("foreground", c("#24292f")),
                ("foreground-muted", c("#6e7781")),
                ("primary", c("#0969da")),
                ("secondary", c("#8250df")),
                ("success", c("#1a7f37")),
                ("warning", c("#9a6700")),
                ("error", c("#cf222e")),
                ("info", c("#0550ae")),
                ("selection", c("#b6d4fe")),
                ("selection-foreground", c("#24292f")),
                ("focus", c("#0969da")),
            ],
        )
    }

    /// All themes shipped with Glyphforge.
    pub fn builtin() -> Vec<Self> {
        vec![
            Self::terminal(),
            Self::minimal_dark(),
            Self::minimal_light(),
        ]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_define_every_token() {
        for t in Theme::builtin() {
            assert!(
                t.missing_tokens().is_empty(),
                "{} misses {:?}",
                t.name,
                t.missing_tokens()
            );
        }
    }

    #[test]
    fn fallback_chain() {
        let t = Theme {
            id: ObjectId::slugify("partial"),
            name: "Partial".into(),
            colors: [
                ("background".to_owned(), Color::BLACK),
                ("foreground".to_owned(), Color::WHITE),
            ]
            .into(),
            border: BorderFamily::Single,
            spacing: Spacing::default(),
        };
        assert_eq!(t.color("surface"), Color::BLACK);
        assert_eq!(t.color("error"), Color::WHITE);
        assert_eq!(t.color("border-muted"), Color::WHITE);
        assert_eq!(t.color("unknown-token"), Color::Default);
    }

    #[test]
    fn resolve_accepts_literals_and_tokens() {
        let t = Theme::minimal_dark();
        assert_eq!(t.resolve("#ff0000"), Color::Rgb(255, 0, 0));
        assert_eq!(t.resolve("ansi:3"), Color::Indexed(3));
        assert_eq!(t.resolve("primary"), t.color("primary"));
        assert_eq!(t.resolve("default"), Color::Default);
    }

    #[test]
    fn serde_round_trip() {
        let t = Theme::minimal_light();
        let json = serde_json::to_string_pretty(&t).unwrap();
        let back: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }
}
