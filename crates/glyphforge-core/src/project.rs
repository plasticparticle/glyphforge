//! The `.glyph` project file: a versioned JSON envelope around a
//! [`Document`], with a migration chain for older schema versions.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::document::Document;

pub const FORMAT: &str = "glyphforge";
pub const SCHEMA_VERSION: u32 = 1;
pub const EXTENSION: &str = "glyph";

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a Glyphforge project (missing or wrong \"format\")")]
    NotAProject,
    #[error("project has no \"schema_version\"; unversioned files are not supported")]
    Unversioned,
    #[error("schema version {0} is newer than this build supports ({SCHEMA_VERSION})")]
    TooNew(u32),
    #[error("migration from schema version {0} failed: {1}")]
    Migration(u32, String),
    #[error("document is invalid: {0}")]
    Invalid(String),
}

#[derive(Serialize)]
struct Envelope<'a> {
    format: &'static str,
    schema_version: u32,
    #[serde(flatten)]
    document: &'a Document,
}

#[derive(Deserialize)]
struct Header {
    format: Option<String>,
    schema_version: Option<u32>,
}

/// Serialises a document as pretty JSON with sorted keys.
pub fn to_json(doc: &Document) -> Result<String, ProjectError> {
    let env = Envelope {
        format: FORMAT,
        schema_version: SCHEMA_VERSION,
        document: doc,
    };
    let mut text = serde_json::to_string_pretty(&env)?;
    text.push('\n');
    Ok(text)
}

/// Parses a project file, migrating older schema versions.
pub fn from_json(text: &str) -> Result<Document, ProjectError> {
    let header: Header = serde_json::from_str(text)?;
    if header.format.as_deref() != Some(FORMAT) {
        return Err(ProjectError::NotAProject);
    }
    let version = header.schema_version.ok_or(ProjectError::Unversioned)?;
    if version > SCHEMA_VERSION {
        return Err(ProjectError::TooNew(version));
    }
    let mut value: Json = serde_json::from_str(text)?;
    let mut current = version;
    while current < SCHEMA_VERSION {
        value = migrate_step(value, current)?;
        current += 1;
    }
    let doc: Document = serde_json::from_value(value)?;
    let dups = doc.duplicate_ids();
    if !dups.is_empty() {
        let list: Vec<String> = dups.iter().map(ToString::to_string).collect();
        return Err(ProjectError::Invalid(format!(
            "duplicate ids: {}",
            list.join(", ")
        )));
    }
    if doc.screens().is_empty() {
        return Err(ProjectError::Invalid("document has no screens".into()));
    }
    Ok(doc)
}

/// Migrates a document JSON value from `from` to `from + 1`.
///
/// Version 1 is the first version, so the chain is empty; every future
/// bump adds one arm here and a fixture test.
#[allow(
    clippy::needless_pass_by_value,
    reason = "future migrations consume and rebuild the value"
)]
fn migrate_step(value: Json, from: u32) -> Result<Json, ProjectError> {
    Err(ProjectError::Migration(
        from,
        format!(
            "no migration registered for version {from} (value has {} keys)",
            value.as_object().map_or(0, serde_json::Map::len)
        ),
    ))
}

pub fn load(path: &Path) -> Result<Document, ProjectError> {
    let text = std::fs::read_to_string(path).map_err(|source| ProjectError::Read {
        path: path.display().to_string(),
        source,
    })?;
    from_json(&text)
}

/// Writes atomically: to a temporary sibling first, then renamed over the
/// target, so a crash never leaves a half-written project.
pub fn save(doc: &Document, path: &Path) -> Result<(), ProjectError> {
    let text = to_json(doc)?;
    let display = path.display().to_string();
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    std::fs::write(&tmp, text).map_err(|source| ProjectError::Write {
        path: display.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| ProjectError::Write {
        path: display,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::document::{Cell, CellStyle, Grapheme, Position, Size};
    use crate::id;
    use crate::layout::Layout;
    use crate::patch::{Operation, Patch};

    fn sample() -> Document {
        let mut doc = Document::new(Size::new(20, 5)).unwrap();
        doc.meta.title = "Sample".into();
        let art = doc
            .layer_mut(&id!("main-artwork"))
            .unwrap()
            .cells_mut()
            .unwrap();
        art.put(
            Position::new(1, 4),
            Cell::glyph(Grapheme::new("漢").unwrap(), CellStyle::DEFAULT),
        )
        .unwrap();
        Patch::single(Operation::CreateComponent {
            component: Component::new(id!("metrics"), "panel")
                .with_prop("title", "Metrics")
                .with_layout(Layout::absolute(0, 0, 10, 3)),
            parent: None,
            layer: None,
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        doc
    }

    #[test]
    fn round_trip() {
        let doc = sample();
        let text = to_json(&doc).unwrap();
        assert!(text.starts_with("{\n  \"format\": \"glyphforge\",\n  \"schema_version\": 1,"));
        let back = from_json(&text).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn a_layout_change_is_a_small_diff() {
        let doc = sample();
        let before = to_json(&doc).unwrap();
        let mut changed = doc.clone();
        Patch::single(Operation::Resize {
            target: id!("metrics"),
            width: 14,
            height: 3,
        })
        .apply(&mut changed)
        .unwrap();
        let after = to_json(&changed).unwrap();
        let differing = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(before.lines().count(), after.lines().count());
        assert_eq!(differing, 1, "only the width line changes");
        assert!(after.contains("\"width\": \"fixed(14)\""));
    }

    #[test]
    fn rejects_unversioned_foreign_and_newer_files() {
        assert!(matches!(
            from_json(r#"{"format":"glyphforge","screens":[]}"#),
            Err(ProjectError::Unversioned)
        ));
        assert!(matches!(
            from_json(r#"{"schema_version":1}"#),
            Err(ProjectError::NotAProject)
        ));
        assert!(matches!(
            from_json(r#"{"format":"glyphforge","schema_version":99}"#),
            Err(ProjectError::TooNew(99))
        ));
        assert!(matches!(from_json("nope"), Err(ProjectError::Json(_))));
    }

    #[test]
    fn minimal_v1_fixture_loads_with_defaults() {
        let text = r#"{
          "format": "glyphforge",
          "schema_version": 1,
          "screens": [
            { "id": "main", "name": "Main", "size": {"width": 10, "height": 2},
              "layers": [
                { "id": "ui", "name": "UI", "kind": "interface",
                  "components": [ { "id": "hello", "kind": "label", "props": {"text": "hi"} } ] }
              ] }
          ]
        }"#;
        let doc = from_json(text).unwrap();
        assert_eq!(doc.theme.id, id!("terminal"));
        assert_eq!(
            doc.component(&id!("hello")).unwrap().prop_str("text"),
            Some("hi")
        );
        assert!(doc.first_screen().layers()[0].visible);
    }

    #[test]
    fn duplicate_ids_are_rejected_on_load() {
        let text = r#"{"format":"glyphforge","schema_version":1,"screens":[{"id":"main","name":"M","size":{"width":2,"height":1},
          "layers":[{"id":"main","name":"L","kind":"interface"}]}]}"#;
        assert!(matches!(from_json(text), Err(ProjectError::Invalid(_))));
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("glyphforge-project-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.glyph");
        let doc = sample();
        save(&doc, &path).unwrap();
        assert!(!dir.join("sample.glyph.tmp").exists());
        assert_eq!(load(&path).unwrap(), doc);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
