//! TUIForge's own key model, independent of the terminal backend.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Insert,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[allow(
    dead_code,
    reason = "constructor API; used by tests now and by tools/palette later"
)]
impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
    };
    pub const ALT: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
    };
    pub const SHIFT: Self = Self {
        ctrl: false,
        alt: false,
        shift: true,
    };

    pub const fn is_none(self) -> bool {
        !self.ctrl && !self.alt && !self.shift
    }
}

/// A key press as received from the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Modifiers,
}

impl Key {
    pub const fn new(code: KeyCode, mods: Modifiers) -> Self {
        Self { code, mods }
    }

    /// The character this key would insert as text, if any: printable
    /// characters without Ctrl/Alt.
    pub fn text_char(self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if !self.mods.ctrl && !self.mods.alt && !c.is_control() => Some(c),
            _ => None,
        }
    }

    /// The normalised chord used for keymap lookup: alphabetic characters
    /// are lower-cased and their case is folded into the Shift modifier, so
    /// `Shift+S`, `S` and (with the kitty protocol) `shift+s` all map to
    /// the same chord. `Shift+Tab` becomes [`KeyCode::BackTab`].
    pub fn chord(self) -> KeyChord {
        let mut mods = self.mods;
        let code = match self.code {
            KeyCode::Char(c) if c.is_alphabetic() => {
                if c.is_uppercase() {
                    mods.shift = true;
                }
                KeyCode::Char(c.to_lowercase().next().unwrap_or(c))
            }
            KeyCode::Char(c) => {
                // Shift is meaningless for symbols: the terminal already sent
                // the shifted character.
                mods.shift = false;
                KeyCode::Char(c)
            }
            KeyCode::Tab if mods.shift => {
                mods.shift = false;
                KeyCode::BackTab
            }
            other => other,
        };
        KeyChord { code, mods }
    }
}

/// A normalised key combination, as written in configuration files
/// (`ctrl+shift+s`, `f1`, `esc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: Modifiers,
}

#[allow(
    dead_code,
    reason = "constructor API; used by tests now and by tools/palette later"
)]
impl KeyChord {
    pub const fn new(code: KeyCode, mods: Modifiers) -> Self {
        Self { code, mods }
    }

    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: Modifiers::NONE,
        }
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: Modifiers::CTRL,
        }
    }
}

/// Errors from parsing a key chord string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyParseError {
    #[error("empty key chord")]
    Empty,
    #[error("unknown key {0:?}")]
    UnknownKey(String),
    #[error("unknown modifier {0:?}")]
    UnknownModifier(String),
}

impl FromStr for KeyChord {
    type Err = KeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(KeyParseError::Empty);
        }
        // A trailing "+" is the plus key itself ("ctrl++").
        let (mod_part, key_part) = match s.rsplit_once('+') {
            Some((m, "")) => (m.trim_end_matches('+'), "+"),
            Some((m, k)) => (m, k),
            None => ("", s),
        };
        let mut mods = Modifiers::NONE;
        for m in mod_part.split('+').filter(|m| !m.is_empty()) {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "c" => mods.ctrl = true,
                "alt" | "meta" | "opt" | "m" => mods.alt = true,
                "shift" | "s" => mods.shift = true,
                other => return Err(KeyParseError::UnknownModifier(other.to_owned())),
            }
        }
        let lower = key_part.to_ascii_lowercase();
        let code = match lower.as_str() {
            "enter" | "return" | "cr" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "backspace" | "bs" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" | "pgdown" => KeyCode::PageDown,
            "space" => KeyCode::Char(' '),
            "plus" => KeyCode::Char('+'),
            "minus" => KeyCode::Char('-'),
            _ => {
                if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                    if (1..=24).contains(&n) {
                        KeyCode::F(n)
                    } else {
                        return Err(KeyParseError::UnknownKey(key_part.to_owned()));
                    }
                } else {
                    let mut chars = key_part.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => KeyCode::Char(c),
                        _ => return Err(KeyParseError::UnknownKey(key_part.to_owned())),
                    }
                }
            }
        };
        // Normalise the same way `Key::chord` does.
        Ok(Key::new(code, mods).chord())
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.mods.alt {
            f.write_str("Alt+")?;
        }
        if self.mods.shift {
            f.write_str("Shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{}", c.to_uppercase()),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Backspace => f.write_str("Backspace"),
            KeyCode::Delete => f.write_str("Del"),
            KeyCode::Insert => f.write_str("Ins"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("Shift+Tab"),
            KeyCode::Up => f.write_str("↑"),
            KeyCode::Down => f.write_str("↓"),
            KeyCode::Left => f.write_str("←"),
            KeyCode::Right => f.write_str("→"),
            KeyCode::Home => f.write_str("Home"),
            KeyCode::End => f.write_str("End"),
            KeyCode::PageUp => f.write_str("PgUp"),
            KeyCode::PageDown => f.write_str("PgDn"),
            KeyCode::F(n) => write!(f, "F{n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_named_keys() {
        assert_eq!("ctrl+q".parse::<KeyChord>().unwrap(), KeyChord::ctrl('q'));
        assert_eq!(
            "Ctrl+Shift+S".parse::<KeyChord>().unwrap(),
            KeyChord::new(
                KeyCode::Char('s'),
                Modifiers {
                    ctrl: true,
                    alt: false,
                    shift: true
                }
            )
        );
        assert_eq!(
            "F1".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::F(1))
        );
        assert_eq!(
            "escape".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::Esc)
        );
        assert_eq!(
            "shift+tab".parse::<KeyChord>().unwrap(),
            KeyChord::plain(KeyCode::BackTab)
        );
        assert_eq!(
            "ctrl+space".parse::<KeyChord>().unwrap(),
            KeyChord::ctrl(' ')
        );
        assert_eq!("ctrl++".parse::<KeyChord>().unwrap(), KeyChord::ctrl('+'));
        assert_eq!(
            "alt+plus".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Char('+'), Modifiers::ALT)
        );
    }

    #[test]
    fn uppercase_char_folds_into_shift() {
        assert_eq!(
            "S".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Char('s'), Modifiers::SHIFT)
        );
        assert_eq!(
            "shift+s".parse::<KeyChord>().unwrap(),
            "S".parse::<KeyChord>().unwrap()
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!("".parse::<KeyChord>(), Err(KeyParseError::Empty)));
        assert!(matches!(
            "hyper+q".parse::<KeyChord>(),
            Err(KeyParseError::UnknownModifier(_))
        ));
        assert!(matches!(
            "f99".parse::<KeyChord>(),
            Err(KeyParseError::UnknownKey(_))
        ));
        assert!(matches!(
            "abc".parse::<KeyChord>(),
            Err(KeyParseError::UnknownKey(_))
        ));
    }

    #[test]
    fn key_chord_normalisation_matches_terminal_variants() {
        // Legacy terminal: uppercase char, no shift flag.
        let legacy = Key::new(KeyCode::Char('S'), Modifiers::CTRL).chord();
        // Kitty protocol: lowercase char with shift flag.
        let kitty = Key::new(
            KeyCode::Char('s'),
            Modifiers {
                ctrl: true,
                alt: false,
                shift: true,
            },
        )
        .chord();
        assert_eq!(legacy, kitty);
        assert_eq!(legacy, "ctrl+shift+s".parse::<KeyChord>().unwrap());
        // Symbols drop shift.
        assert_eq!(
            Key::new(KeyCode::Char('?'), Modifiers::SHIFT).chord(),
            KeyChord::plain(KeyCode::Char('?'))
        );
    }

    #[test]
    fn text_char_excludes_control_combos() {
        assert_eq!(
            Key::new(KeyCode::Char('a'), Modifiers::NONE).text_char(),
            Some('a')
        );
        assert_eq!(
            Key::new(KeyCode::Char('A'), Modifiers::SHIFT).text_char(),
            Some('A')
        );
        assert_eq!(
            Key::new(KeyCode::Char('a'), Modifiers::CTRL).text_char(),
            None
        );
        assert_eq!(Key::new(KeyCode::Enter, Modifiers::NONE).text_char(), None);
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(
            "ctrl+shift+s".parse::<KeyChord>().unwrap().to_string(),
            "Ctrl+Shift+S"
        );
        assert_eq!(KeyChord::plain(KeyCode::F(2)).to_string(), "F2");
        assert_eq!(KeyChord::ctrl(' ').to_string(), "Ctrl+Space");
    }
}
