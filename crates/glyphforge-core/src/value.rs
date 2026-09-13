//! Property values of semantic components.
//!
//! A JSON-like value model without floats, so values are totally ordered
//! and comparable: important for deterministic patches and diffs.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Str(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<u16> for Value {
    fn from(i: u16) -> Self {
        Self::Int(i64::from(i))
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Self::List(v.into_iter().map(Into::into).collect())
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Str(s) => f.write_str(s),
            Self::List(_) | Self::Map(_) => {
                f.write_str(&serde_json::to_string(self).unwrap_or_default())
            }
        }
    }
}

/// Ordered property map. `BTreeMap` keeps serialisation deterministic.
pub type Properties = BTreeMap<String, Value>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_is_untagged_json() {
        let v: Value =
            serde_json::from_str(r#"{"title":"Metrics","rows":[1,2],"open":true,"x":null}"#)
                .unwrap();
        let m = v.as_map().unwrap();
        assert_eq!(m["title"].as_str(), Some("Metrics"));
        assert_eq!(m["rows"].as_list().unwrap().len(), 2);
        assert_eq!(m["open"].as_bool(), Some(true));
        assert!(m["x"].is_null());
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"open":true,"rows":[1,2],"title":"Metrics","x":null}"#
        );
    }

    #[test]
    fn display_is_plain_for_scalars() {
        assert_eq!(Value::from("a").to_string(), "a");
        assert_eq!(Value::from(3i64).to_string(), "3");
        assert_eq!(Value::from(vec![1i64, 2]).to_string(), "[1,2]");
    }
}
