//! Component descriptors: what a kind is, which properties it reads and
//! what a new instance starts with.
//!
//! Descriptors are data, not behaviour. The inspector edits properties by
//! their [`PropKind`], the add prompt and the command palette list kinds by
//! title, category and keywords, the validator flags properties of the
//! wrong type, and `glyphforge kinds --json` hands the same table to agents.

use serde::Serialize;

use crate::boxdraw::BorderFamily;
use crate::document::Size;
use crate::value::Value;

/// Where a kind appears in palettes and listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Layout,
    Text,
    Input,
    Navigation,
    Data,
    Viewer,
    Feedback,
    Chart,
    Media,
}

impl Category {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Layout => "Layout",
            Self::Text => "Text",
            Self::Input => "Inputs",
            Self::Navigation => "Navigation",
            Self::Data => "Data",
            Self::Viewer => "Viewers",
            Self::Feedback => "Feedback",
            Self::Chart => "Charts",
            Self::Media => "Media",
        }
    }
}

/// The type of a property value, with what the inspector needs to edit it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PropKind {
    Text,
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    /// A theme token (`primary`) or a literal (`#7aa2f7`, `ansi:4`).
    Color,
    /// One of a fixed set of strings.
    Choice {
        options: &'static [&'static str],
    },
    /// A border family name.
    Border,
    /// The id of another object in the document (a layer, a screen).
    Reference,
    /// A list of values (rows, items, series).
    List,
    /// Free-form structured data.
    Map,
}

impl PropKind {
    pub const fn int(min: i64, max: i64) -> Self {
        Self::Int { min, max }
    }

    pub const fn choice(options: &'static [&'static str]) -> Self {
        Self::Choice { options }
    }

    /// Short name for listings (`text`, `bool`, `0..100`, `left|right`).
    pub fn label(&self) -> String {
        match self {
            Self::Text => "text".into(),
            Self::Bool => "bool".into(),
            Self::Int { min, max } => format!("{min}..{max}"),
            Self::Color => "color".into(),
            Self::Choice { options } => options.join("|"),
            Self::Border => "border".into(),
            Self::Reference => "id".into(),
            Self::List => "list".into(),
            Self::Map => "map".into(),
        }
    }

    /// Whether `value` has this type. `Null` always fits (unset).
    pub fn accepts(&self, value: &Value) -> bool {
        match (self, value) {
            (_, Value::Null)
            | (Self::Text | Self::Color | Self::Reference, Value::Str(_))
            | (Self::Bool, Value::Bool(_))
            | (Self::List, Value::List(_))
            | (Self::Map, Value::Map(_)) => true,
            (Self::Int { min, max }, Value::Int(n)) => (min..=max).contains(&n),
            (Self::Choice { options }, Value::Str(s)) => options.contains(&s.as_str()),
            (Self::Border, Value::Str(s)) => BorderFamily::parse(s).is_some(),
            _ => false,
        }
    }

    /// Parses the text a person typed into a value of this type.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        let value = match self {
            Self::Bool => match text {
                "true" | "yes" | "on" | "1" => Value::Bool(true),
                "false" | "no" | "off" | "0" => Value::Bool(false),
                _ => return Err(format!("expected true or false, got {text:?}")),
            },
            Self::Int { .. } => Value::Int(
                text.parse()
                    .map_err(|_| format!("expected a whole number, got {text:?}"))?,
            ),
            Self::List | Self::Map => serde_json::from_str(text)
                .map_err(|e| format!("expected JSON ({e}), got {text:?}"))?,
            _ => Value::Str(text.to_owned()),
        };
        if self.accepts(&value) {
            Ok(value)
        } else {
            Err(format!("{text:?} is not one of {}", self.label()))
        }
    }

    /// The value after `current` when cycling with a key (booleans flip,
    /// choices advance, numbers step by one within their range).
    pub fn cycle(&self, current: Option<&Value>, forward: bool) -> Option<Value> {
        match self {
            Self::Bool => Some(Value::Bool(!matches!(current, Some(Value::Bool(true))))),
            Self::Choice { options } => {
                let i = current
                    .and_then(Value::as_str)
                    .and_then(|s| options.iter().position(|o| *o == s));
                let n = options.len();
                let next = match (i, forward) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                };
                Some(Value::Str(options[next].into()))
            }
            Self::Border => {
                let names = BorderFamily::NAMES;
                let i = current
                    .and_then(Value::as_str)
                    .and_then(|s| names.iter().position(|o| *o == s));
                let n = names.len();
                let next = match (i, forward) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                };
                Some(Value::Str(names[next].into()))
            }
            Self::Color => {
                let tokens = crate::theme::COLOR_TOKENS;
                let i = current
                    .and_then(Value::as_str)
                    .and_then(|s| tokens.iter().position(|t| *t == s));
                let n = tokens.len();
                let next = match (i, forward) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                };
                Some(Value::Str(tokens[next].into()))
            }
            Self::Int { min, max } => {
                let v = current.and_then(Value::as_int).unwrap_or(*min);
                let v = if forward { v + 1 } else { v - 1 };
                Some(Value::Int(v.clamp(*min, *max)))
            }
            _ => None,
        }
    }
}

/// One property a kind reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropSpec {
    pub name: &'static str,
    #[serde(flatten)]
    pub kind: PropKind,
    /// What the renderer assumes when the property is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// What a freshly added instance gets, so it is visible at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<Value>,
    pub help: &'static str,
}

impl PropSpec {
    pub const fn new(name: &'static str, kind: PropKind, help: &'static str) -> Self {
        Self {
            name,
            kind,
            default: None,
            initial: None,
            help,
        }
    }

    #[must_use]
    pub fn default(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Sets the value a new instance starts with.
    #[must_use]
    pub fn initial(mut self, value: impl Into<Value>) -> Self {
        self.initial = Some(value.into());
        self
    }
}

/// Everything known about a component kind apart from how it paints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Descriptor {
    pub kind: &'static str,
    pub title: &'static str,
    pub category: Category,
    pub summary: &'static str,
    /// Extra words the palette matches (`btn` for button).
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub keywords: &'static [&'static str],
    /// Whether children make sense (they are allowed everywhere, but the
    /// add prompt and reparenting prefer containers).
    pub container: bool,
    /// Size of a new instance placed absolutely, or `None` when the kind
    /// sizes itself to its content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_size: Option<(u16, u16)>,
    pub props: Vec<PropSpec>,
}

impl Descriptor {
    pub fn new(kind: &'static str, title: &'static str, category: Category) -> Self {
        Self {
            kind,
            title,
            category,
            summary: "",
            keywords: &[],
            container: false,
            default_size: Some((20, 6)),
            props: Vec::new(),
        }
    }

    #[must_use]
    pub const fn summary(mut self, summary: &'static str) -> Self {
        self.summary = summary;
        self
    }

    #[must_use]
    pub const fn keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.keywords = keywords;
        self
    }

    #[must_use]
    pub const fn container(mut self) -> Self {
        self.container = true;
        self
    }

    /// Size for a new absolutely placed instance; `None` for content size.
    #[must_use]
    pub const fn size(mut self, size: Option<(u16, u16)>) -> Self {
        self.default_size = size;
        self
    }

    #[must_use]
    pub fn prop(mut self, spec: PropSpec) -> Self {
        self.props.push(spec);
        self
    }

    #[must_use]
    pub fn props(mut self, specs: impl IntoIterator<Item = PropSpec>) -> Self {
        self.props.extend(specs);
        self
    }

    pub fn prop_spec(&self, name: &str) -> Option<&PropSpec> {
        self.props.iter().find(|p| p.name == name)
    }

    pub fn default_size(&self) -> Option<Size> {
        self.default_size.map(|(w, h)| Size::new(w, h))
    }

    /// The properties a new instance starts with.
    pub fn initial_props(&self) -> Vec<(&'static str, Value)> {
        self.props
            .iter()
            .filter_map(|p| p.initial.clone().map(|v| (p.name, v)))
            .collect()
    }

    /// Whether `query` (lowercase words) matches kind, title or keywords.
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!(
            "{} {} {} {}",
            self.kind,
            self.title.to_lowercase(),
            self.category.name().to_lowercase(),
            self.keywords.join(" ")
        );
        query
            .to_lowercase()
            .split_whitespace()
            .all(|w| hay.contains(w))
    }
}

/// Text colour, background and attributes, read by most text renderers.
pub fn style_props() -> [PropSpec; 6] {
    [
        PropSpec::new("color", PropKind::Color, "text colour (token or literal)"),
        PropSpec::new(
            "background",
            PropKind::Color,
            "fill colour (token or literal)",
        ),
        PropSpec::new("bold", PropKind::Bool, "bold text"),
        PropSpec::new("dim", PropKind::Bool, "dimmed text"),
        PropSpec::new("italic", PropKind::Bool, "italic text"),
        PropSpec::new("underline", PropKind::Bool, "underlined text"),
    ]
}

/// The interaction states every control can be previewed in.
pub fn state_props() -> [PropSpec; 3] {
    [
        PropSpec::new("focused", PropKind::Bool, "preview the focused state"),
        PropSpec::new("disabled", PropKind::Bool, "preview the disabled state"),
        PropSpec::new("hovered", PropKind::Bool, "preview the hovered state"),
    ]
}

/// Horizontal text alignment.
pub fn align_prop() -> PropSpec {
    PropSpec::new(
        "align",
        PropKind::choice(&["left", "center", "right"]),
        "horizontal alignment",
    )
    .default("left")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prop_kinds_accept_and_parse() {
        let choice = PropKind::choice(&["left", "right"]);
        assert!(choice.accepts(&"left".into()));
        assert!(!choice.accepts(&"middle".into()));
        assert!(choice.parse("middle").is_err());
        assert_eq!(PropKind::Bool.parse("on"), Ok(Value::Bool(true)));
        assert_eq!(PropKind::int(0, 100).parse("42"), Ok(Value::Int(42)));
        assert!(PropKind::int(0, 100).parse("142").is_err());
        assert!(PropKind::Border.accepts(&"rounded".into()));
        assert!(!PropKind::Border.accepts(&"wavy".into()));
        assert_eq!(
            PropKind::List.parse(r#"["a", 1]"#),
            Ok(Value::List(vec!["a".into(), Value::Int(1)]))
        );
        assert!(PropKind::Text.accepts(&Value::Null), "unset always fits");
    }

    #[test]
    fn cycling_flips_and_wraps() {
        let choice = PropKind::choice(&["a", "b", "c"]);
        assert_eq!(choice.cycle(None, true), Some("a".into()));
        assert_eq!(choice.cycle(Some(&"c".into()), true), Some("a".into()));
        assert_eq!(choice.cycle(Some(&"a".into()), false), Some("c".into()));
        assert_eq!(
            PropKind::Bool.cycle(Some(&Value::Bool(true)), true),
            Some(Value::Bool(false))
        );
        assert_eq!(
            PropKind::int(0, 3).cycle(Some(&Value::Int(3)), true),
            Some(Value::Int(3))
        );
        assert_eq!(PropKind::Text.cycle(None, true), None);
        assert_eq!(
            PropKind::Color.cycle(Some(&"background".into()), true),
            Some("surface".into()),
            "colours cycle through the theme tokens"
        );
    }

    #[test]
    fn descriptor_matches_words_and_lists_initial_props() {
        let d = Descriptor::new("button", "Button", Category::Input)
            .keywords(&["btn", "action"])
            .prop(PropSpec::new("label", PropKind::Text, "caption").initial("OK"));
        assert!(d.matches("btn"));
        assert!(d.matches("Input but"));
        assert!(!d.matches("table"));
        assert_eq!(d.initial_props(), vec![("label", Value::from("OK"))]);
    }
}
