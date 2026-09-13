//! Stable, human-readable object identifiers.
//!
//! Components, layers, screens, symbols and themes are all addressed by an
//! [`ObjectId`] such as `sidebar` or `server-table`. Ids are what humans and
//! agents use to talk about a design, so they are slugs, not UUIDs, and they
//! are unique within a document.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Errors from constructing an [`ObjectId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("identifier is empty")]
    Empty,
    #[error("identifier {0:?} is too long (max 64 characters)")]
    TooLong(String),
    #[error(
        "identifier {0:?} must start with a letter or digit and contain only a-z, 0-9, '-', '_', '.'"
    )]
    InvalidChars(String),
}

/// A validated slug: `[a-z0-9][a-z0-9._-]*`, at most 64 characters.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(String);

impl ObjectId {
    pub const MAX_LEN: usize = 64;

    /// Validates `s` as an identifier without changing it.
    pub fn new(s: &str) -> Result<Self, IdError> {
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(IdError::TooLong(s.to_owned()));
        }
        let mut chars = s.chars();
        let first = chars.next().ok_or(IdError::Empty)?;
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(IdError::InvalidChars(s.to_owned()));
        }
        if !chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
        {
            return Err(IdError::InvalidChars(s.to_owned()));
        }
        Ok(Self(s.to_owned()))
    }

    /// Derives an identifier from free text: `"Server Table"` becomes
    /// `server-table`. Falls back to `item` when nothing usable remains.
    pub fn slugify(text: &str) -> Self {
        let mut out = String::new();
        let mut last_dash = true;
        for c in text.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                out.push(c);
                last_dash = false;
            } else if !last_dash {
                out.push('-');
                last_dash = true;
            }
            if out.len() >= Self::MAX_LEN {
                break;
            }
        }
        let trimmed = out.trim_end_matches('-');
        if trimmed.is_empty() {
            return Self("item".to_owned());
        }
        Self(trimmed.to_owned())
    }

    /// Returns an id not contained in `taken`, appending `-2`, `-3`, ... to
    /// this id as needed.
    #[must_use]
    pub fn unique_among(&self, taken: impl Fn(&ObjectId) -> bool) -> Self {
        if !taken(self) {
            return self.clone();
        }
        for n in 2u32.. {
            let mut base = self.0.clone();
            base.truncate(Self::MAX_LEN - 1 - n.to_string().len());
            let candidate = Self(format!("{base}-{n}"));
            if !taken(&candidate) {
                return candidate;
            }
        }
        unreachable!("an unused suffix always exists")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ObjectId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for ObjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ObjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::new(&s).map_err(serde::de::Error::custom)
    }
}

/// Shorthand for tests and builders: panics on an invalid literal.
#[macro_export]
macro_rules! id {
    ($s:literal) => {
        $crate::id::ObjectId::new($s).expect("valid identifier literal")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs_and_rejects_others() {
        assert!(ObjectId::new("sidebar").is_ok());
        assert!(ObjectId::new("server-table_2.v1").is_ok());
        assert_eq!(ObjectId::new(""), Err(IdError::Empty));
        assert!(matches!(ObjectId::new("-x"), Err(IdError::InvalidChars(_))));
        assert!(matches!(
            ObjectId::new("Sidebar"),
            Err(IdError::InvalidChars(_))
        ));
        assert!(matches!(
            ObjectId::new("a b"),
            Err(IdError::InvalidChars(_))
        ));
        assert!(matches!(
            ObjectId::new(&"a".repeat(65)),
            Err(IdError::TooLong(_))
        ));
    }

    #[test]
    fn slugify_produces_stable_ids() {
        assert_eq!(ObjectId::slugify("Server Table").as_str(), "server-table");
        assert_eq!(
            ObjectId::slugify("  CPU / Memory!! ").as_str(),
            "cpu-memory"
        );
        assert_eq!(ObjectId::slugify("漢字").as_str(), "item");
        assert_eq!(ObjectId::slugify("MetricCard").as_str(), "metriccard");
    }

    #[test]
    fn unique_among_appends_counter() {
        let taken = ["panel", "panel-2"];
        let id = id!("panel").unique_among(|c| taken.contains(&c.as_str()));
        assert_eq!(id.as_str(), "panel-3");
        assert_eq!(id!("free").unique_among(|_| false).as_str(), "free");
    }

    #[test]
    fn serde_is_a_plain_string() {
        let json = serde_json::to_string(&id!("cpu-chart")).unwrap();
        assert_eq!(json, "\"cpu-chart\"");
        assert!(serde_json::from_str::<ObjectId>("\"Bad Id\"").is_err());
    }
}
