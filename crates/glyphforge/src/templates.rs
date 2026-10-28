//! Built-in templates: the showcase examples, compiled into the binary so
//! `glyphforge new --template` and the editor work without the sources.
//! `scripts/build_examples.py` regenerates them through the CLI.

/// Name, description and project JSON of every template.
pub const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "system-monitor",
        "Hosts, sparklines and services (minimal dark)",
        include_str!("../../../examples/dashboard.glyph"),
    ),
    (
        "developer-dashboard",
        "Deploys, pipelines, traffic and logs (cobalt)",
        include_str!("../../../examples/developer-dashboard.glyph"),
    ),
    (
        "git-client",
        "Branches, history and a diff (verdant)",
        include_str!("../../../examples/git-client.glyph"),
    ),
    (
        "ai-assistant",
        "Chat, code and context for a coding agent (nebula)",
        include_str!("../../../examples/ai-assistant.glyph"),
    ),
    (
        "database-explorer",
        "Schemas, a query and a result grid (amber CRT)",
        include_str!("../../../examples/database-explorer.glyph"),
    ),
    (
        "settings",
        "A preferences form with every control (paper, light)",
        include_str!("../../../examples/settings.glyph"),
    ),
];

/// The project JSON of a template.
pub fn template(name: &str) -> Option<&'static str> {
    TEMPLATES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, json)| *json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphforge_core::render::Registry;

    #[test]
    fn every_template_loads_validates_and_renders_at_two_sizes() {
        let registry = Registry::builtin();
        for (name, _, json) in TEMPLATES {
            let doc = glyphforge_core::project::from_json(json).unwrap();
            let errors: Vec<_> = glyphforge_core::validate::validate(&doc, &registry)
                .into_iter()
                .filter(|d| d.severity == glyphforge_core::validate::Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
            // Stored canonically: saving again changes nothing.
            assert_eq!(
                glyphforge_core::project::to_json(&doc).unwrap(),
                *json,
                "{name}"
            );
            let session = glyphforge_core::api::Session::new(doc);
            let screen = session.document().first_screen().id.clone();
            for size in [None, Some(glyphforge_core::Size::new(80, 24))] {
                let text = session.render_text(&screen, size).unwrap();
                assert!(!text.trim().is_empty(), "{name} renders");
            }
        }
    }
}
