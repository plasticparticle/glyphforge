//! Input model: keys, key chords, keymaps and conversion from crossterm.

pub mod key;
pub mod keymap;

use crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[allow(unused_imports, reason = "re-export for tests and later modules")]
pub use key::KeyChord;
pub use key::{Key, KeyCode, Modifiers};
pub use keymap::Keymap;

/// Converts a crossterm key event into Glyphforge's key model. Release and
/// repeat events (kitty protocol) are ignored and yield `None`.
pub fn key_from_crossterm(ev: &KeyEvent) -> Option<Key> {
    if ev.kind == KeyEventKind::Release {
        return None;
    }
    let code = match ev.code {
        CtKeyCode::Char(c) => KeyCode::Char(c),
        CtKeyCode::Enter => KeyCode::Enter,
        CtKeyCode::Esc => KeyCode::Esc,
        CtKeyCode::Backspace => KeyCode::Backspace,
        CtKeyCode::Delete => KeyCode::Delete,
        CtKeyCode::Insert => KeyCode::Insert,
        CtKeyCode::Tab => KeyCode::Tab,
        CtKeyCode::BackTab => KeyCode::BackTab,
        CtKeyCode::Up => KeyCode::Up,
        CtKeyCode::Down => KeyCode::Down,
        CtKeyCode::Left => KeyCode::Left,
        CtKeyCode::Right => KeyCode::Right,
        CtKeyCode::Home => KeyCode::Home,
        CtKeyCode::End => KeyCode::End,
        CtKeyCode::PageUp => KeyCode::PageUp,
        CtKeyCode::PageDown => KeyCode::PageDown,
        CtKeyCode::F(n) => KeyCode::F(n),
        _ => return None,
    };
    let mods = Modifiers {
        ctrl: ev.modifiers.contains(KeyModifiers::CONTROL),
        alt: ev.modifiers.contains(KeyModifiers::ALT),
        shift: ev.modifiers.contains(KeyModifiers::SHIFT),
    };
    Some(Key::new(code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_key_events_and_ignores_releases() {
        let press = KeyEvent::new(CtKeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(
            key_from_crossterm(&press),
            Some(Key::new(KeyCode::Char('q'), Modifiers::CTRL))
        );
        let mut release = press;
        release.kind = KeyEventKind::Release;
        assert_eq!(key_from_crossterm(&release), None);
    }

    #[test]
    fn nul_byte_ctrl_space_maps_to_ctrl_space_chord() {
        // Legacy terminals deliver Ctrl+Space as NUL; crossterm reports it as
        // Char(' ') with CONTROL, which is exactly the chord we bind.
        let ev = KeyEvent::new(CtKeyCode::Char(' '), KeyModifiers::CONTROL);
        assert_eq!(
            key_from_crossterm(&ev).map(Key::chord),
            Some(KeyChord::ctrl(' '))
        );
    }
}
