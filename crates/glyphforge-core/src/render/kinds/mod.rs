//! The component library beyond the basics: inputs, navigation, data
//! views, viewers, feedback and charts. Each kind is a renderer with a
//! descriptor; [`register`] adds them all to a registry.

mod charts;
mod data;
mod feedback;
mod inputs;
mod navigation;
mod viewers;

use super::Registry;

/// Adds every library kind to `registry`.
pub fn register(registry: &mut Registry) {
    use charts::{BarChart, Heatmap, Histogram, LineChart, Meter, Sparkline};
    use data::{KeyValue, List, Table, Tree};
    use feedback::{EmptyState, KeyHint, Modal, Notification, Pill, Spinner, Tooltip};
    use inputs::{CheckControl, Field, Select, TextArea};
    use navigation::{Breadcrumbs, Menu, StatusBar, Tabs, Toolbar};
    use viewers::{Code, Diff, Log, Markdown};

    registry.register(Box::new(Field::INPUT));
    registry.register(Box::new(Field::PASSWORD));
    registry.register(Box::new(Field::SEARCH));
    registry.register(Box::new(TextArea));
    registry.register(Box::new(CheckControl::CHECKBOX));
    registry.register(Box::new(CheckControl::RADIO));
    registry.register(Box::new(CheckControl::TOGGLE));
    registry.register(Box::new(Select));
    registry.register(Box::new(Tabs));
    registry.register(Box::new(Breadcrumbs));
    registry.register(Box::new(Menu));
    registry.register(Box::new(Toolbar));
    registry.register(Box::new(StatusBar));
    registry.register(Box::new(List));
    registry.register(Box::new(Tree));
    registry.register(Box::new(Table::TABLE));
    registry.register(Box::new(Table::DATAGRID));
    registry.register(Box::new(KeyValue::GRID));
    registry.register(Box::new(KeyValue::LIST));
    registry.register(Box::new(Log));
    registry.register(Box::new(Code));
    registry.register(Box::new(Markdown));
    registry.register(Box::new(Diff));
    registry.register(Box::new(Modal::MODAL));
    registry.register(Box::new(Modal::DIALOG));
    registry.register(Box::new(Notification));
    registry.register(Box::new(Pill::TOAST));
    registry.register(Box::new(Pill::BADGE));
    registry.register(Box::new(Pill::TAG));
    registry.register(Box::new(Tooltip));
    registry.register(Box::new(EmptyState));
    registry.register(Box::new(KeyHint));
    registry.register(Box::new(Spinner));
    registry.register(Box::new(Sparkline));
    registry.register(Box::new(BarChart));
    registry.register(Box::new(Histogram));
    registry.register(Box::new(LineChart));
    registry.register(Box::new(Meter::PROGRESS));
    registry.register(Box::new(Meter::GAUGE));
    registry.register(Box::new(Heatmap));
}

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::document::Size;
    use crate::id;
    use crate::layout::Layout;
    use crate::render::{Registry, RenderContext, render_components, to_text_lines};
    use crate::value::Value;

    fn render_at(c: Component, w: u16, h: u16) -> Vec<String> {
        let theme = crate::theme::Theme::terminal();
        let ctx = RenderContext::new(&theme);
        let c = c.with_layout(Layout::absolute(0, 0, w, h));
        let (canvas, _) =
            render_components(&[c], Size::new(w, h), &ctx, &Registry::builtin()).unwrap();
        to_text_lines(&canvas)
    }

    /// A component of `kind` with its descriptor's initial properties.
    fn sample(kind: &str) -> Component {
        let registry = Registry::builtin();
        let d = registry.descriptor(kind).unwrap();
        let mut c = Component::new(id!("c"), kind);
        for (k, v) in d.initial_props() {
            c = c.with_prop(k, v);
        }
        c
    }

    #[test]
    fn every_kind_renders_at_any_size_and_its_descriptor_is_consistent() {
        let registry = Registry::builtin();
        let kinds: Vec<&str> = registry.kinds().collect();
        assert!(kinds.len() >= 47, "{} kinds", kinds.len());
        for kind in kinds {
            let d = registry.descriptor(kind).unwrap();
            assert_eq!(d.kind, kind);
            assert!(!d.summary.is_empty(), "{kind} has a summary");
            for p in &d.props {
                if let Some(v) = &p.initial {
                    assert!(p.kind.accepts(v), "{kind}.{} initial {v:?}", p.name);
                }
                if let Some(v) = &p.default {
                    assert!(p.kind.accepts(v), "{kind}.{} default {v:?}", p.name);
                }
            }
            for (w, h) in [(1, 1), (3, 2), (40, 12), (120, 40)] {
                let _ = render_at(sample(kind), w, h);
            }
        }
    }

    #[test]
    fn inputs_render_values_placeholders_and_marks() {
        let lines = render_at(sample("input").with_prop("label", "Name"), 24, 1);
        assert!(lines[0].starts_with("Name  Type here…"), "{lines:?}");
        let lines = render_at(sample("password").with_prop("value", "abc"), 12, 1);
        assert!(lines[0].contains("•••"), "{lines:?}");
        assert_eq!(render_at(sample("checkbox"), 14, 1)[0], "[x] Checkbox");
        assert_eq!(
            render_at(sample("radio").with_prop("checked", false), 10, 1)[0],
            "( ) Radio"
        );
        let open = render_at(sample("select").with_prop("open", true), 10, 4);
        assert!(open[0].contains("One") && open[0].contains('▾'), "{open:?}");
        assert!(open[3].contains("Three"));
    }

    #[test]
    fn navigation_and_data_views() {
        let tabs = render_at(sample("tabs").with_prop("selected", "Logs"), 30, 2);
        assert!(tabs[0].contains("Overview") && tabs[0].contains("Logs"));
        assert!(tabs[1].contains('━'), "selected tab underlined: {tabs:?}");
        assert_eq!(
            render_at(sample("breadcrumbs"), 30, 1)[0],
            "Home › Projects › Glyphforge"
        );
        let table = render_at(sample("table"), 30, 5);
        assert!(table[0].starts_with("Name"), "{table:?}");
        assert!(table[1].starts_with('─'));
        assert!(table[2].starts_with("nginx"));
        let grid = render_at(sample("datagrid"), 30, 7);
        assert!(
            grid[0].starts_with('┌') && grid[2].starts_with('├'),
            "{grid:?}"
        );
        assert!(grid[3].contains("12%"));
        let tree = render_at(sample("tree"), 24, 6);
        assert_eq!(tree[0], "▾ src");
        assert_eq!(tree[1], "├─  main.rs");
        assert_eq!(tree[2], "└─▾ app");
        assert_eq!(tree[3], "  └─  mod.rs");
        assert_eq!(tree[4], "▸ target");
        let kv = render_at(sample("keyvalue"), 30, 3);
        assert_eq!(kv[0], "Host: db-01");
    }

    #[test]
    fn data_binding_reads_document_fixtures() {
        let theme = crate::theme::Theme::terminal();
        let mut data = std::collections::BTreeMap::new();
        data.insert(
            "servers".to_owned(),
            Value::Map(
                [
                    (
                        "columns".to_owned(),
                        Value::List(vec!["Host".into(), "Load".into()]),
                    ),
                    (
                        "rows".to_owned(),
                        Value::List(vec![Value::Map(
                            [
                                ("Host".to_owned(), Value::from("web-1")),
                                ("Load".to_owned(), Value::Int(7)),
                            ]
                            .into_iter()
                            .collect(),
                        )]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );
        let ctx = RenderContext {
            theme: &theme,
            screen: None,
            data: Some(&data),
            symbols: &[],
            depth: 0,
        };
        let c = Component::new(id!("t"), "table")
            .with_prop("bind", "servers")
            .with_layout(Layout::absolute(0, 0, 20, 3));
        let (canvas, _) =
            render_components(&[c], Size::new(20, 3), &ctx, &Registry::builtin()).unwrap();
        let lines = to_text_lines(&canvas);
        assert!(lines[0].starts_with("Host"), "{lines:?}");
        assert!(
            lines[2].starts_with("web-1") && lines[2].contains('7'),
            "{lines:?}"
        );
    }

    #[test]
    fn charts_in_every_style() {
        let spark = render_at(sample("sparkline").with_prop("values", "0 4 8"), 3, 1);
        assert_eq!(spark[0], " ▄█");
        let ascii = render_at(
            sample("sparkline")
                .with_prop("values", "0 4 8")
                .with_prop("style", "ascii"),
            3,
            1,
        );
        assert_eq!(ascii[0], " -#");
        let progress = render_at(sample("progress").with_prop("value", Value::Int(50)), 14, 1);
        // Nine columns of bar: half is four and a half cells.
        assert_eq!(progress[0], "████▌░░░░  50%");
        let gauge = render_at(sample("gauge").with_prop("value", Value::Int(95)), 20, 1);
        assert!(
            gauge[0].starts_with("CPU ") && gauge[0].ends_with(" 95%"),
            "{gauge:?}"
        );
        let line = render_at(sample("linechart"), 30, 6);
        assert!(
            line.iter()
                .any(|l| l.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)))
        );
        let bars = render_at(
            sample("barchart").with_prop("orientation", "horizontal"),
            30,
            4,
        );
        assert!(
            bars[1].starts_with("web") && bars[1].contains('█'),
            "{bars:?}"
        );
    }

    #[test]
    fn viewers_and_feedback() {
        let code = render_at(sample("code"), 40, 5);
        assert_eq!(code[0], "1 fn main() {");
        let md = render_at(sample("markdown"), 40, 6);
        assert_eq!(md[0], "Release notes");
        assert_eq!(md[3], " • the table component");
        let dialog = render_at(sample("dialog"), 40, 7);
        assert!(dialog[0].starts_with("╔═ Delete file? "), "{dialog:?}");
        assert!(dialog[4].contains("[ Delete ]"), "{dialog:?}");
        assert_eq!(render_at(sample("badge"), 5, 1)[0], " 3");
        assert_eq!(
            render_at(sample("keyhint"), 30, 1)[0],
            "q quit  ? help  / search"
        );
        let empty = render_at(sample("empty"), 30, 5);
        assert!(empty[2].contains("No results"), "{empty:?}");
    }
}
