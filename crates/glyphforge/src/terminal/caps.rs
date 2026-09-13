//! Terminal capability detection.

use std::fmt;

use crate::config::ColorMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorDepth {
    Ansi16,
    Ansi256,
    TrueColor,
}

impl fmt::Display for ColorDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ansi16 => "16 colours",
            Self::Ansi256 => "256 colours",
            Self::TrueColor => "TrueColor",
        })
    }
}

/// What the terminal is believed to support. Everything here is a best
/// guess from the environment plus one runtime probe; consumers must
/// degrade gracefully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub term: String,
    pub color_depth: ColorDepth,
    pub unicode: bool,
    pub mouse: bool,
    /// Kitty keyboard protocol (disambiguated Ctrl+Space, key release, ...).
    pub keyboard_enhancement: bool,
}

impl TerminalCapabilities {
    /// Infers capabilities from environment variables. `keyboard_enhancement`
    /// comes from a runtime probe and is passed in.
    pub fn infer(env: impl Fn(&str) -> Option<String>, keyboard_enhancement: bool) -> Self {
        let term = env("TERM").unwrap_or_default();
        let colorterm = env("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        let term_program = env("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();

        let color_depth = if env("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            ColorDepth::Ansi16
        } else if colorterm == "truecolor"
            || colorterm == "24bit"
            || term.contains("direct")
            || matches!(
                term_program.as_str(),
                "ghostty" | "alacritty" | "kitty" | "wezterm" | "foot"
            )
            || matches!(
                term.as_str(),
                "xterm-ghostty" | "alacritty" | "xterm-kitty" | "wezterm" | "foot" | "foot-extra"
            )
        {
            ColorDepth::TrueColor
        } else if term.contains("256color") {
            ColorDepth::Ansi256
        } else {
            ColorDepth::Ansi16
        };

        let locale = env("LC_ALL")
            .filter(|s| !s.is_empty())
            .or_else(|| env("LC_CTYPE").filter(|s| !s.is_empty()))
            .or_else(|| env("LANG"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let unicode = locale.contains("utf-8") || locale.contains("utf8");

        let mouse = !matches!(term.as_str(), "" | "dumb" | "linux");

        Self {
            term,
            color_depth,
            unicode,
            mouse,
            keyboard_enhancement,
        }
    }

    /// The colour depth to use given the user's preference.
    pub fn effective_color_depth(&self, preference: ColorMode) -> ColorDepth {
        match preference {
            ColorMode::Auto => self.color_depth,
            ColorMode::TrueColor => ColorDepth::TrueColor,
            ColorMode::Ansi256 => ColorDepth::Ansi256,
            ColorMode::Ansi16 => ColorDepth::Ansi16,
        }
    }
}

impl fmt::Display for TerminalCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "TERM:                 {}",
            if self.term.is_empty() {
                "(unset)"
            } else {
                &self.term
            }
        )?;
        writeln!(f, "colour depth:         {}", self.color_depth)?;
        writeln!(f, "unicode locale:       {}", self.unicode)?;
        writeln!(f, "mouse reporting:      {}", self.mouse)?;
        write!(f, "keyboard enhancement: {}", self.keyboard_enhancement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn colorterm_truecolor() {
        let caps = TerminalCapabilities::infer(
            env(&[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]),
            false,
        );
        assert_eq!(caps.color_depth, ColorDepth::TrueColor);
        assert!(caps.mouse);
    }

    #[test]
    fn term_256color_without_colorterm() {
        let caps = TerminalCapabilities::infer(env(&[("TERM", "screen-256color")]), false);
        assert_eq!(caps.color_depth, ColorDepth::Ansi256);
    }

    #[test]
    fn linux_console_is_16_colours_without_mouse() {
        let caps = TerminalCapabilities::infer(env(&[("TERM", "linux"), ("LANG", "C")]), false);
        assert_eq!(caps.color_depth, ColorDepth::Ansi16);
        assert!(!caps.mouse);
        assert!(!caps.unicode);
    }

    #[test]
    fn no_color_forces_16() {
        let caps = TerminalCapabilities::infer(
            env(&[
                ("TERM", "xterm"),
                ("COLORTERM", "truecolor"),
                ("NO_COLOR", "1"),
            ]),
            false,
        );
        assert_eq!(caps.color_depth, ColorDepth::Ansi16);
    }

    #[test]
    fn omarchy_terminals_are_truecolor_by_term_name() {
        for term in ["alacritty", "xterm-ghostty", "xterm-kitty", "foot"] {
            let caps = TerminalCapabilities::infer(env(&[("TERM", term)]), true);
            assert_eq!(caps.color_depth, ColorDepth::TrueColor, "{term}");
        }
    }

    #[test]
    fn lc_all_wins_over_lang() {
        let caps =
            TerminalCapabilities::infer(env(&[("LANG", "de_DE.UTF-8"), ("LC_ALL", "C")]), false);
        assert!(!caps.unicode);
        let caps =
            TerminalCapabilities::infer(env(&[("LANG", "de_DE.UTF-8"), ("LC_ALL", "")]), false);
        assert!(caps.unicode);
    }

    #[test]
    fn preference_overrides_detection() {
        let caps = TerminalCapabilities::infer(env(&[("TERM", "xterm")]), false);
        assert_eq!(
            caps.effective_color_depth(ColorMode::Auto),
            ColorDepth::Ansi16
        );
        assert_eq!(
            caps.effective_color_depth(ColorMode::TrueColor),
            ColorDepth::TrueColor
        );
    }
}
