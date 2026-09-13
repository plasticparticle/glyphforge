//! The shipped example projects must load, validate and render.

use std::path::PathBuf;

use glyphforge_core::api::Session;
use glyphforge_core::validate::Severity;
use glyphforge_core::{ObjectId, Size};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn dashboard_example_loads_validates_and_renders_responsively() {
    let session = Session::open(&example("dashboard.glyph")).expect("example loads");
    let doc = session.document();
    assert_eq!(doc.theme.name, "Minimal Dark");
    assert!(doc.component(&ObjectId::new("services").unwrap()).is_some());

    let errors: Vec<_> = session
        .validate()
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let main = ObjectId::new("main").unwrap();
    let full = session.render_text(&main, None).unwrap();
    assert!(
        full.starts_with("╭ glyphforge ──────────╮ System Monitor"),
        "{full}"
    );
    assert!(full.contains("▄▀▄ glyph"), "logo artwork placed by layout");

    // At 80 columns the sidebar (and with it the logo) is hidden and the
    // metric grid folds to two columns; nothing loses its space.
    let narrow = session.render_text(&main, Some(Size::new(80, 24))).unwrap();
    assert!(!narrow.contains("Navigation"));
    assert!(!narrow.contains("▄▀▄"));
    assert!(narrow.contains("╭ Network"));
    let at_80 = session.validate_at(&main, Size::new(80, 24)).unwrap();
    assert!(at_80.is_empty(), "{at_80:?}");
}

#[test]
fn dashboard_example_round_trips_byte_for_byte() {
    let path = example("dashboard.glyph");
    let text = std::fs::read_to_string(&path).unwrap();
    let session = Session::open(&path).unwrap();
    assert_eq!(
        session.to_json().unwrap(),
        text,
        "the example is stored in canonical form"
    );
}
