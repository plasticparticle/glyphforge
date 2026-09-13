//! A one-line text prompt with suggestions: used for "select by id", "add
//! component", "edit property" and "save as". The command palette builds
//! on the same widget.

use glyphforge_core::Value;

/// What happens when the prompt is submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    SelectById,
    AddComponent,
    EditProperty,
    SaveAs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub title: String,
    pub input: String,
    /// All candidates; `matches()` filters them by the current input.
    pub candidates: Vec<String>,
    pub selected: usize,
}

impl Prompt {
    pub fn new(kind: PromptKind, title: impl Into<String>, candidates: Vec<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            input: String::new(),
            candidates,
            selected: 0,
        }
    }

    #[must_use]
    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.input = input.into();
        self
    }

    /// Candidates matching the input: prefix matches first, then
    /// substring matches, each group in candidate order.
    pub fn matches(&self) -> Vec<&str> {
        let needle = self.input.trim().to_ascii_lowercase();
        let first_word = needle.split_whitespace().next().unwrap_or("").to_owned();
        if first_word.is_empty() {
            return self.candidates.iter().map(String::as_str).collect();
        }
        let mut prefix = Vec::new();
        let mut inner = Vec::new();
        for c in &self.candidates {
            let lower = c.to_ascii_lowercase();
            if lower.starts_with(&first_word) {
                prefix.push(c.as_str());
            } else if lower.contains(&first_word) {
                inner.push(c.as_str());
            }
        }
        prefix.extend(inner);
        prefix
    }

    /// The highlighted candidate, if any.
    pub fn selected_match(&self) -> Option<&str> {
        self.matches().get(self.selected).copied()
    }

    pub fn insert(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.selected = 0;
    }

    pub fn next(&mut self) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + 1) % n;
        }
    }

    pub fn prev(&mut self) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + n - 1) % n;
        }
    }

    /// Replaces the first word of the input with the highlighted candidate.
    pub fn complete(&mut self) {
        if let Some(m) = self.selected_match().map(str::to_owned) {
            let rest: Vec<&str> = self.input.split_whitespace().skip(1).collect();
            self.input = if rest.is_empty() {
                m
            } else {
                format!("{m} {}", rest.join(" "))
            };
        }
    }

    /// The value to act on: the highlighted candidate when the prompt has
    /// candidates and the input is empty or a prefix of it, otherwise the
    /// raw input.
    pub fn value(&self) -> String {
        let input = self.input.trim();
        if self.candidates.is_empty() {
            return input.to_owned();
        }
        match self.selected_match() {
            Some(m)
                if input.is_empty()
                    || m.to_ascii_lowercase()
                        .starts_with(&input.to_ascii_lowercase()) =>
            {
                m.to_owned()
            }
            _ => input.to_owned(),
        }
    }
}

/// Parses `key=value` for the property prompt. Values: `true`/`false`,
/// integers, otherwise strings; an empty value removes the property.
pub fn parse_property(input: &str) -> Option<(String, Value)> {
    let (key, value) = input.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let value = value.trim();
    let parsed = if value.is_empty() {
        Value::Null
    } else if value == "true" {
        Value::Bool(true)
    } else if value == "false" {
        Value::Bool(false)
    } else if let Ok(i) = value.parse::<i64>() {
        Value::Int(i)
    } else {
        Value::Str(value.trim_matches('"').to_owned())
    };
    Some((key.to_owned(), parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt() -> Prompt {
        Prompt::new(
            PromptKind::AddComponent,
            "Add",
            vec!["panel".into(), "label".into(), "heading".into()],
        )
    }

    #[test]
    fn matches_prefer_prefix_then_substring() {
        let mut p = prompt();
        assert_eq!(p.matches(), vec!["panel", "label", "heading"]);
        p.insert('a');
        assert_eq!(p.matches(), vec!["panel", "label", "heading"]);
        p.input = "he".into();
        assert_eq!(p.matches(), vec!["heading"]);
        p.input = "el".into();
        assert_eq!(p.matches(), vec!["panel", "label"]);
    }

    #[test]
    fn navigation_and_completion() {
        let mut p = prompt();
        p.next();
        assert_eq!(p.selected_match(), Some("label"));
        p.prev();
        p.prev();
        assert_eq!(p.selected_match(), Some("heading"));
        p.input = "he my-heading".into();
        p.selected = 0;
        p.complete();
        assert_eq!(p.input, "heading my-heading");
        assert_eq!(p.value(), "heading my-heading");
    }

    #[test]
    fn value_uses_candidate_when_input_is_a_prefix() {
        let mut p = prompt();
        assert_eq!(p.value(), "panel");
        p.input = "lab".into();
        assert_eq!(p.value(), "label");
        p.input = "zzz".into();
        assert_eq!(p.value(), "zzz");
        let free = Prompt::new(PromptKind::SaveAs, "Save", vec![]).with_input(" x.glyph ");
        assert_eq!(free.value(), "x.glyph");
    }

    #[test]
    fn property_parsing() {
        assert_eq!(
            parse_property("title=Metrics"),
            Some(("title".into(), Value::from("Metrics")))
        );
        assert_eq!(
            parse_property("bold = true"),
            Some(("bold".into(), Value::Bool(true)))
        );
        assert_eq!(
            parse_property("width=12"),
            Some(("width".into(), Value::Int(12)))
        );
        assert_eq!(
            parse_property("title="),
            Some(("title".into(), Value::Null))
        );
        assert_eq!(
            parse_property("text=\"quoted\""),
            Some(("text".into(), Value::from("quoted")))
        );
        assert_eq!(parse_property("no-equals"), None);
        assert_eq!(parse_property("=x"), None);
    }
}
