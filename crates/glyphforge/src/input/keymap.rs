//! Key chord -> action-name bindings, built from defaults plus config.

use std::collections::{BTreeMap, HashMap};

use super::key::{KeyChord, KeyParseError};
use crate::actions::{Action, Context, descriptors, find_descriptor};

/// Errors from applying user key bindings.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("invalid key chord {chord:?}: {source}")]
    Chord {
        chord: String,
        source: KeyParseError,
    },
    #[error("unknown action {action:?} bound to {chord:?}")]
    UnknownAction { chord: String, action: String },
}

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: HashMap<(Context, KeyChord), Action>,
}

impl Keymap {
    /// The built-in bindings from the action descriptors.
    pub fn defaults() -> Self {
        let mut bindings = HashMap::new();
        for d in descriptors() {
            for chord in d.default_keys {
                if let Ok(chord) = chord.parse::<KeyChord>() {
                    bindings.insert((d.context, chord), d.action.clone());
                }
            }
        }
        Self { bindings }
    }

    /// Applies user overrides. Every entry is validated; invalid entries are
    /// returned as errors and skipped, valid ones are applied, so one typo
    /// does not disable all custom bindings.
    pub fn apply_overrides(&mut self, overrides: &BTreeMap<String, String>) -> Vec<KeymapError> {
        let mut errors = Vec::new();
        for (chord_text, action_name) in overrides {
            let chord = match chord_text.parse::<KeyChord>() {
                Ok(c) => c,
                Err(source) => {
                    errors.push(KeymapError::Chord {
                        chord: chord_text.clone(),
                        source,
                    });
                    continue;
                }
            };
            if action_name == "none" {
                self.bindings.remove(&(Context::Global, chord));
                self.bindings.remove(&(Context::Interface, chord));
                continue;
            }
            match find_descriptor(action_name) {
                Some(d) => {
                    self.bindings.insert((d.context, chord), d.action.clone());
                }
                None => errors.push(KeymapError::UnknownAction {
                    chord: chord_text.clone(),
                    action: action_name.clone(),
                }),
            }
        }
        errors
    }

    /// Looks a chord up in `context` first, then in the global context.
    pub fn lookup(&self, chord: KeyChord, context: Context) -> Option<&Action> {
        self.bindings
            .get(&(context, chord))
            .or_else(|| self.bindings.get(&(Context::Global, chord)))
    }

    /// All chords bound to `action` in any context, sorted by their
    /// textual form so the output is stable.
    pub fn chords_for(&self, action: &Action) -> Vec<KeyChord> {
        let mut v: Vec<_> = self
            .bindings
            .iter()
            .filter(|(_, a)| *a == action)
            .map(|((_, c), _)| *c)
            .collect();
        v.sort_by_key(ToString::to_string);
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::key::KeyCode;

    #[test]
    fn defaults_contain_quit() {
        let km = Keymap::defaults();
        assert_eq!(
            km.lookup(KeyChord::ctrl('q'), Context::Global),
            Some(&Action::Quit)
        );
        assert_eq!(
            km.lookup(KeyChord::plain(KeyCode::F(1)), Context::Interface),
            Some(&Action::ToggleHelp)
        );
    }

    #[test]
    fn overrides_rebind_and_unbind() {
        let mut km = Keymap::defaults();
        let mut o = BTreeMap::new();
        o.insert("ctrl+x".to_owned(), "quit".to_owned());
        o.insert("ctrl+q".to_owned(), "none".to_owned());
        let errors = km.apply_overrides(&o);
        assert!(errors.is_empty());
        assert_eq!(
            km.lookup(KeyChord::ctrl('x'), Context::Global),
            Some(&Action::Quit)
        );
        assert_eq!(km.lookup(KeyChord::ctrl('q'), Context::Global), None);
    }

    #[test]
    fn invalid_overrides_are_reported_but_do_not_block_valid_ones() {
        let mut km = Keymap::defaults();
        let mut o = BTreeMap::new();
        o.insert("hyper+x".to_owned(), "quit".to_owned());
        o.insert("ctrl+e".to_owned(), "does-not-exist".to_owned());
        o.insert("ctrl+x".to_owned(), "quit".to_owned());
        let errors = km.apply_overrides(&o);
        assert_eq!(errors.len(), 2);
        assert_eq!(
            km.lookup(KeyChord::ctrl('x'), Context::Global),
            Some(&Action::Quit)
        );
    }

    #[test]
    fn interface_context_shadows_global_bindings() {
        let km = Keymap::defaults();
        let enter = KeyChord::plain(KeyCode::Enter);
        assert_eq!(km.lookup(enter, Context::Global), Some(&Action::NewLine));
        assert_eq!(
            km.lookup(enter, Context::Interface),
            Some(&Action::EditProperty)
        );
        let delete = KeyChord::plain(KeyCode::Delete);
        assert_eq!(
            km.lookup(delete, Context::Interface),
            Some(&Action::DeleteSelection)
        );
    }

    #[test]
    fn chords_for_action_are_stable() {
        let km = Keymap::defaults();
        let chords = km.chords_for(&Action::Redo);
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].to_string(), "Ctrl+Shift+Z");
        assert_eq!(chords[1].to_string(), "Ctrl+Y");
    }
}
