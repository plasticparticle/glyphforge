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

/// Optional tokens for gradient frames; not part of [`COLOR_TOKENS`], so
/// themes without them stay complete.
pub const FRAME_FROM: &str = "frame-from";
pub const FRAME_TO: &str = "frame-to";

/// A theme colour from a hex literal; invalid literals fall back to the
/// terminal default.
fn c(hex: &str) -> Color {
    Color::from_hex(hex).unwrap_or(Color::Default)
}

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

    /// The two colours of the theme's frame gradient, when it has one:
    /// box borders fade from the first to the second across the screen.
    /// Themes without the optional `frame-from`/`frame-to` tokens draw
    /// their frames in a single colour.
    pub fn frame_gradient(&self) -> Option<(Color, Color)> {
        let from = *self.colors.get(FRAME_FROM)?;
        let to = *self.colors.get(FRAME_TO)?;
        Some((from, to))
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

    /// Northern lights: teal fading to violet frames.
    pub fn aurora() -> Self {
        Self::from_pairs(
            "aurora",
            "Aurora",
            BorderFamily::Rounded,
            &[
                ("background", c("#07131a")),
                ("surface", c("#0c1f28")),
                ("surface-alt", c("#132c38")),
                ("border", c("#2f6f7a")),
                ("border-muted", c("#1d4a55")),
                ("foreground", c("#dcf3ef")),
                ("foreground-muted", c("#89b3b6")),
                ("primary", c("#5de4c7")),
                ("secondary", c("#a78bfa")),
                ("success", c("#5de4c7")),
                ("warning", c("#ffd479")),
                ("error", c("#ff7a93")),
                ("info", c("#7cc5ff")),
                ("selection", c("#1d4f52")),
                ("selection-foreground", c("#f0fffc")),
                ("focus", c("#5de4c7")),
                (FRAME_FROM, c("#5de4c7")),
                (FRAME_TO, c("#a78bfa")),
            ],
        )
    }

    /// Hot metal: amber fading to crimson frames.
    pub fn ember() -> Self {
        Self::from_pairs(
            "ember",
            "Ember",
            BorderFamily::Heavy,
            &[
                ("background", c("#1a0f0a")),
                ("surface", c("#241510")),
                ("surface-alt", c("#311d16")),
                ("border", c("#8a4a2b")),
                ("border-muted", c("#58301c")),
                ("foreground", c("#ffe8d6")),
                ("foreground-muted", c("#c09177")),
                ("primary", c("#ff9f45")),
                ("secondary", c("#ff5f6d")),
                ("success", c("#b5d33d")),
                ("warning", c("#ffc14d")),
                ("error", c("#ff5f6d")),
                ("info", c("#ffb37a")),
                ("selection", c("#5c2a1c")),
                ("selection-foreground", c("#fff3e8")),
                ("focus", c("#ff9f45")),
                (FRAME_FROM, c("#ffc14d")),
                (FRAME_TO, c("#ff4d6d")),
            ],
        )
    }

    /// A built-in theme by id.
    pub fn builtin_by_id(id: &str) -> Option<Self> {
        Self::builtin().into_iter().find(|t| t.id.as_str() == id)
    }

    /// The default dark theme: warm greys with an amber accent.
    pub fn forge_dark() -> Self {
        Self::from_pairs(
            "forge-dark",
            "Forge Dark",
            BorderFamily::Rounded,
            &[
                ("background", c("#16181d")),
                ("surface", c("#1d2026")),
                ("surface-alt", c("#252932")),
                ("border", c("#3b414f")),
                ("border-muted", c("#2a2f3a")),
                ("foreground", c("#dfe3ea")),
                ("foreground-muted", c("#8d95a5")),
                ("primary", c("#ffa657")),
                ("secondary", c("#c792ea")),
                ("success", c("#8bd17c")),
                ("warning", c("#ffcc66")),
                ("error", c("#ff6b6b")),
                ("info", c("#64b6f7")),
                ("selection", c("#33405a")),
                ("selection-foreground", c("#ffffff")),
                ("focus", c("#ffa657")),
            ],
        )
    }

    /// A light theme: warm paper with ink and a rust accent.
    pub fn paper() -> Self {
        Self::from_pairs(
            "paper",
            "Paper",
            BorderFamily::Rounded,
            &[
                ("background", c("#fbfaf7")),
                ("surface", c("#f3f1ec")),
                ("surface-alt", c("#e7e3da")),
                ("border", c("#bdb7ab")),
                ("border-muted", c("#d8d3c8")),
                ("foreground", c("#2c2a26")),
                ("foreground-muted", c("#6b665d")),
                ("primary", c("#b4531f")),
                ("secondary", c("#7a4fbf")),
                ("success", c("#2f7d32")),
                ("warning", c("#9a6700")),
                ("error", c("#c0392b")),
                ("info", c("#1f6fb2")),
                ("selection", c("#dbe5f3")),
                ("selection-foreground", c("#15191f")),
                ("focus", c("#1f6fb2")),
            ],
        )
    }

    /// Deep blue with a bright azure accent.
    pub fn cobalt() -> Self {
        Self::from_pairs(
            "cobalt",
            "Cobalt",
            BorderFamily::Rounded,
            &[
                ("background", c("#091a2b")),
                ("surface", c("#0f2740")),
                ("surface-alt", c("#16344f")),
                ("border", c("#2c5f8f")),
                ("border-muted", c("#1b3f61")),
                ("foreground", c("#d7e7f7")),
                ("foreground-muted", c("#8badcb")),
                ("primary", c("#4ea6ff")),
                ("secondary", c("#9d8cff")),
                ("success", c("#43c59e")),
                ("warning", c("#f0b429")),
                ("error", c("#ff7a7a")),
                ("info", c("#6fd3ff")),
                ("selection", c("#1d4d7a")),
                ("selection-foreground", c("#ffffff")),
                ("focus", c("#4ea6ff")),
            ],
        )
    }

    /// Forest greens with a leaf accent.
    pub fn verdant() -> Self {
        Self::from_pairs(
            "verdant",
            "Verdant",
            BorderFamily::Rounded,
            &[
                ("background", c("#0d1a12")),
                ("surface", c("#13241a")),
                ("surface-alt", c("#1b3222")),
                ("border", c("#2f5c3b")),
                ("border-muted", c("#1f3e29")),
                ("foreground", c("#d8ecd9")),
                ("foreground-muted", c("#8cb294")),
                ("primary", c("#6fcf72")),
                ("secondary", c("#b7d96b")),
                ("success", c("#6fcf72")),
                ("warning", c("#e0b44a")),
                ("error", c("#e36a63")),
                ("info", c("#5fc0b0")),
                ("selection", c("#27512f")),
                ("selection-foreground", c("#f2fff3")),
                ("focus", c("#6fcf72")),
            ],
        )
    }

    /// Teal water with a cyan accent.
    pub fn lagoon() -> Self {
        Self::from_pairs(
            "lagoon",
            "Lagoon",
            BorderFamily::Rounded,
            &[
                ("background", c("#07191d")),
                ("surface", c("#0d262c")),
                ("surface-alt", c("#13363e")),
                ("border", c("#2a6b74")),
                ("border-muted", c("#1a474f")),
                ("foreground", c("#d4f1f4")),
                ("foreground-muted", c("#85b9c1")),
                ("primary", c("#39c7d6")),
                ("secondary", c("#6ad5b5")),
                ("success", c("#4ec9a0")),
                ("warning", c("#e8c468")),
                ("error", c("#f07178")),
                ("info", c("#7fdbe8")),
                ("selection", c("#14505a")),
                ("selection-foreground", c("#eafcff")),
                ("focus", c("#39c7d6")),
            ],
        )
    }

    /// Retro amber CRT, in double-line DOS frames.
    pub fn amber_crt() -> Self {
        Self::from_pairs(
            "amber-crt",
            "Amber CRT",
            BorderFamily::Double,
            &[
                ("background", c("#140c02")),
                ("surface", c("#1c1205")),
                ("surface-alt", c("#26190a")),
                ("border", c("#7a4f12")),
                ("border-muted", c("#4a3008")),
                ("foreground", c("#ffb000")),
                ("foreground-muted", c("#bb7f08")),
                ("primary", c("#ffcc33")),
                ("secondary", c("#ff8c1a")),
                ("success", c("#ffb000")),
                ("warning", c("#ffcc33")),
                ("error", c("#ff6a3d")),
                ("info", c("#ffd480")),
                ("selection", c("#5c3c0a")),
                ("selection-foreground", c("#ffe9b0")),
                ("focus", c("#ffcc33")),
            ],
        )
    }

    /// Retro green phosphor, in plain single-line frames.
    pub fn phosphor() -> Self {
        Self::from_pairs(
            "phosphor",
            "Phosphor",
            BorderFamily::Single,
            &[
                ("background", c("#020d02")),
                ("surface", c("#061606")),
                ("surface-alt", c("#0b220b")),
                ("border", c("#1f6b1f")),
                ("border-muted", c("#124212")),
                ("foreground", c("#33ff33")),
                ("foreground-muted", c("#25bb25")),
                ("primary", c("#7cff7c")),
                ("secondary", c("#33ff99")),
                ("success", c("#33ff33")),
                ("warning", c("#d7ff33")),
                ("error", c("#ff5f5f")),
                ("info", c("#33ffd7")),
                ("selection", c("#155215")),
                ("selection-foreground", c("#ccffcc")),
                ("focus", c("#7cff7c")),
            ],
        )
    }

    /// Violet night with neon magenta, the fancy one.
    pub fn nebula() -> Self {
        Self::from_pairs(
            "nebula",
            "Nebula",
            BorderFamily::Rounded,
            &[
                ("background", c("#120a1f")),
                ("surface", c("#1a0f2b")),
                ("surface-alt", c("#241739")),
                ("border", c("#5a3d8c")),
                ("border-muted", c("#35235a")),
                ("foreground", c("#ece6ff")),
                ("foreground-muted", c("#a594d1")),
                ("primary", c("#c77dff")),
                ("secondary", c("#ff6ec7")),
                ("success", c("#5eead4")),
                ("warning", c("#ffd166")),
                ("error", c("#ff5d8f")),
                ("info", c("#7aa2ff")),
                ("selection", c("#3b2566")),
                ("selection-foreground", c("#ffffff")),
                ("focus", c("#ff6ec7")),
                (FRAME_FROM, c("#7aa2ff")),
                (FRAME_TO, c("#ff6ec7")),
            ],
        )
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

    /// All themes shipped with Glyphforge, in menu order.
    pub fn builtin() -> Vec<Self> {
        vec![
            Self::forge_dark(),
            Self::paper(),
            Self::cobalt(),
            Self::verdant(),
            Self::lagoon(),
            Self::amber_crt(),
            Self::phosphor(),
            Self::nebula(),
            Self::aurora(),
            Self::ember(),
            Self::minimal_dark(),
            Self::minimal_light(),
            Self::terminal(),
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

    #[test]
    fn gradient_frames_are_optional_and_paired() {
        let plain = Theme::cobalt();
        assert_eq!(plain.frame_gradient(), None);
        assert!(
            plain.missing_tokens().is_empty(),
            "the extra tokens are optional"
        );
        for theme in [Theme::aurora(), Theme::ember(), Theme::nebula()] {
            let (from, to) = theme.frame_gradient().expect(&theme.name);
            assert_ne!(from, to, "{}", theme.name);
        }
        let mut half = Theme::cobalt();
        half.colors.insert(FRAME_FROM.to_owned(), Color::BLUE);
        assert_eq!(half.frame_gradient(), None, "both ends are needed");
    }

    #[test]
    fn built_in_themes_are_complete_unique_and_legible() {
        let themes = Theme::builtin();
        assert_eq!(themes.len(), 13);
        let mut ids: Vec<String> = themes.iter().map(|t| t.id.to_string()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), themes.len(), "ids are unique");
        let brightness = |c: Color| {
            c.to_rgb().map(|(r, g, b)| {
                (i32::from(r) * 299 + i32::from(g) * 587 + i32::from(b) * 114) / 1000
            })
        };
        for theme in &themes {
            assert!(
                theme.missing_tokens().is_empty(),
                "{} misses tokens",
                theme.name
            );
            assert_eq!(
                Theme::builtin_by_id(theme.id.as_str()).as_ref(),
                Some(theme)
            );
            // The terminal theme deliberately leaves the colours to the
            // terminal; the designed ones must be readable on their own.
            if theme.id.as_str() == "terminal" {
                continue;
            }
            let background = brightness(theme.color("background")).unwrap_or(0);
            for token in ["foreground", "foreground-muted", "primary"] {
                let value = brightness(theme.color(token)).unwrap_or(background);
                assert!(
                    (value - background).abs() >= 70,
                    "{}: {token} is too close to the background",
                    theme.name
                );
            }
            let selection = brightness(theme.color("selection")).unwrap_or(0);
            let on_selection = brightness(theme.color("selection-foreground")).unwrap_or(0);
            assert!(
                (selection - on_selection).abs() >= 70,
                "{}: selection is unreadable",
                theme.name
            );
        }
    }
}
