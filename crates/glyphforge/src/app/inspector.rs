//! The interactive inspector: the selection's layout and properties as
//! rows that can be stepped through, cycled and edited without typing
//! patch JSON. Property rows come from the kind's descriptor, so every
//! kind gets the right editor (toggle, choice, colour, number, text).
//!
//! Edits apply to every selected component; layout edits are `set_layout`
//! operations and property edits `set_property`, all in one patch.

use glyphforge_core::layout::{Align, Dimension, Direction, Edges, Layout, Placement};
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::render::{Descriptor, PropKind};
use glyphforge_core::theme::COLOR_TOKENS;
use glyphforge_core::{Component, ObjectId, Value};

use super::prompt::{Prompt, PromptKind, parse_property};
use super::{App, StatusKind};

/// What an inspector row edits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldKey {
    Placement,
    X,
    Y,
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
    Padding,
    Margin,
    Direction,
    Gap,
    Columns,
    Align,
    Prop(String),
}

/// One inspector row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: FieldKey,
    pub label: String,
    /// The current value as shown; empty when unset.
    pub value: String,
    /// The value the renderer assumes when unset.
    pub default: Option<String>,
    /// Whether Space/arrows cycle the value (toggles and choices).
    pub cyclic: bool,
    /// Where the row starts a new group (layout, container, properties).
    pub group: &'static str,
    /// The value is a colour (token or literal): shown with a swatch.
    pub swatch: bool,
}

const DIRECTIONS: [&str; 4] = ["vertical", "horizontal", "stack", "grid"];
const ALIGNS: [&str; 4] = ["stretch", "start", "center", "end"];

fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::Horizontal => "horizontal",
        Direction::Vertical => "vertical",
        Direction::Stack => "stack",
        Direction::Grid => "grid",
    }
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Start => "start",
        Align::Center => "center",
        Align::End => "end",
        Align::Stretch => "stretch",
    }
}

fn parse_direction(s: &str) -> Option<Direction> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

fn parse_align(s: &str) -> Option<Align> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

/// `1`, `1 2` (vertical horizontal) or `1 2 3 4` (top right bottom left).
pub fn parse_edges(text: &str) -> Option<Edges> {
    let n: Vec<u16> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    match n.as_slice() {
        [a] => Some(Edges::all(*a)),
        [v, h] => Some(Edges::symmetric(*v, *h)),
        [top, right, bottom, left] => Some(Edges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => None,
    }
}

pub fn edges_text(e: Edges) -> String {
    if e.top == e.bottom && e.left == e.right {
        if e.top == e.left {
            e.top.to_string()
        } else {
            format!("{} {}", e.top, e.left)
        }
    } else {
        format!("{} {} {} {}", e.top, e.right, e.bottom, e.left)
    }
}

fn opt_text(v: Option<u16>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

/// The next dimension when cycling: content, fill, fixed (at the current
/// size), percent, flex.
fn cycle_dimension(d: Dimension, current: u16, forward: bool) -> Dimension {
    let all = [
        Dimension::Content,
        Dimension::Fill,
        Dimension::Fixed(current.max(1)),
        Dimension::Percent(50),
        Dimension::Flex(1),
    ];
    let i = all
        .iter()
        .position(|a| std::mem::discriminant(a) == std::mem::discriminant(&d))
        .unwrap_or(0);
    let n = all.len();
    all[if forward {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    }]
}

/// Rows for a component: layout first, then container fields when it has
/// children or is a container kind, then its descriptor's properties and
/// any other properties it carries.
pub fn fields(c: &Component, descriptor: Option<&Descriptor>) -> Vec<Field> {
    let layout = &c.layout;
    let mut out = Vec::new();
    let mut push = |key, label: &str, value: String, cyclic, group| {
        out.push(Field {
            key,
            label: label.to_owned(),
            value,
            default: None,
            cyclic,
            group,
            swatch: false,
        });
    };
    let (placement, col, row) = match layout.placement {
        Placement::Flow => ("flow", String::new(), String::new()),
        Placement::Absolute { x, y } => ("absolute", x.to_string(), y.to_string()),
    };
    push(
        FieldKey::Placement,
        "place",
        placement.into(),
        true,
        "layout",
    );
    if layout.placement != Placement::Flow {
        push(FieldKey::X, "x", col, false, "layout");
        push(FieldKey::Y, "y", row, false, "layout");
    }
    push(
        FieldKey::Width,
        "width",
        layout.width.to_string(),
        true,
        "layout",
    );
    push(
        FieldKey::Height,
        "height",
        layout.height.to_string(),
        true,
        "layout",
    );
    push(
        FieldKey::MinWidth,
        "min w",
        opt_text(layout.min_width),
        false,
        "layout",
    );
    push(
        FieldKey::MaxWidth,
        "max w",
        opt_text(layout.max_width),
        false,
        "layout",
    );
    push(
        FieldKey::MinHeight,
        "min h",
        opt_text(layout.min_height),
        false,
        "layout",
    );
    push(
        FieldKey::MaxHeight,
        "max h",
        opt_text(layout.max_height),
        false,
        "layout",
    );
    push(
        FieldKey::Margin,
        "margin",
        edges_text(layout.margin),
        false,
        "layout",
    );
    let is_container = descriptor.is_some_and(|d| d.container) || !c.children.is_empty();
    if is_container {
        let container = layout.container;
        push(
            FieldKey::Direction,
            "flow",
            direction_name(container.direction).into(),
            true,
            "container",
        );
        push(
            FieldKey::Gap,
            "gap",
            container.gap.to_string(),
            false,
            "container",
        );
        if container.direction == Direction::Grid {
            push(
                FieldKey::Columns,
                "columns",
                container.columns.max(1).to_string(),
                false,
                "container",
            );
        }
        push(
            FieldKey::Align,
            "align",
            align_name(container.align).into(),
            true,
            "container",
        );
        push(
            FieldKey::Padding,
            "padding",
            edges_text(layout.padding),
            false,
            "container",
        );
    }
    let text = |v: &Value| match v {
        Value::Str(string) => string.clone(),
        other => other.to_string(),
    };
    if let Some(d) = descriptor {
        for spec in &d.props {
            out.push(Field {
                key: FieldKey::Prop(spec.name.to_owned()),
                label: spec.name.to_owned(),
                value: c.prop(spec.name).map(text).unwrap_or_default(),
                default: spec.default.as_ref().map(text),
                cyclic: matches!(
                    spec.kind,
                    PropKind::Bool | PropKind::Choice { .. } | PropKind::Border | PropKind::Color
                ),
                group: "properties",
                swatch: spec.kind == PropKind::Color,
            });
        }
    }
    for (name, value) in &c.props {
        if descriptor.is_some_and(|d| d.prop_spec(name).is_some()) {
            continue;
        }
        out.push(Field {
            key: FieldKey::Prop(name.clone()),
            label: name.clone(),
            value: text(value),
            default: None,
            cyclic: matches!(value, Value::Bool(_)),
            group: "properties",
            swatch: false,
        });
    }
    out
}

impl App {
    fn primary_component(&self) -> Option<&Component> {
        self.selection().and_then(|id| self.doc().component(id))
    }

    fn descriptor_of(&self, c: &Component) -> Option<Descriptor> {
        self.session.registry().descriptor(&c.kind)
    }

    /// Inspector rows of the primary selection.
    pub fn inspector_fields(&self) -> Vec<Field> {
        self.primary_component()
            .map(|c| fields(c, self.descriptor_of(c).as_ref()))
            .unwrap_or_default()
    }

    /// The highlighted row, clamped to the rows that exist.
    pub fn inspector_row(&self) -> usize {
        self.ui
            .inspector_row
            .min(self.inspector_fields().len().saturating_sub(1))
    }

    pub fn inspector_move(&mut self, down: bool) {
        let n = self.inspector_fields().len();
        if n == 0 {
            return;
        }
        let row = self.inspector_row();
        self.ui.inspector_row = if down {
            (row + 1) % n
        } else {
            (row + n - 1) % n
        };
    }

    /// Applies `edit` to the layout of every selected component.
    fn edit_layouts(&mut self, label: &str, edit: impl Fn(&mut Layout, (u16, u16), (u16, u16))) {
        let mut ops = Vec::new();
        for id in self.selected_ids() {
            let Some(layout) = self.doc().component(&id).map(|c| c.layout) else {
                continue;
            };
            let (offset, size) = self.geometry(&id).unwrap_or(((0, 0), (1, 1)));
            let mut next = layout;
            edit(&mut next, offset, size);
            if next != layout {
                ops.push(Operation::SetLayout {
                    target: id,
                    layout: next,
                });
            }
        }
        if !ops.is_empty() && self.apply(Patch::new(ops), label) {
            self.set_status(StatusKind::Info, label.to_owned());
        }
    }

    /// Sets a property on every selected component (`Null` removes it).
    fn edit_props(&mut self, name: &str, value: &Value) {
        let ops: Vec<Operation> = self
            .selected_ids()
            .into_iter()
            .filter(|id| {
                self.doc().component(id).is_some_and(|c| {
                    c.prop(name) != Some(value) && !(value.is_null() && c.prop(name).is_none())
                })
            })
            .map(|target| Operation::SetProperty {
                target,
                property: name.to_owned(),
                value: value.clone(),
            })
            .collect();
        let label = if value.is_null() {
            format!("Clear {name}")
        } else {
            format!("Set {name}")
        };
        if !ops.is_empty() && self.apply(Patch::new(ops), &label) {
            self.set_status(StatusKind::Info, label);
        }
    }

    /// Space and arrows on a row: toggles flip, choices advance.
    pub fn inspector_cycle(&mut self, forward: bool) {
        let fields = self.inspector_fields();
        let Some(field) = fields.get(self.inspector_row()).cloned() else {
            return;
        };
        match &field.key {
            FieldKey::Placement => self.edit_layouts("Change placement", |l, (x, y), _| {
                l.placement = match l.placement {
                    Placement::Flow => Placement::Absolute { x, y },
                    Placement::Absolute { .. } => Placement::Flow,
                };
            }),
            FieldKey::Width => self.edit_layouts("Change width", |l, _, (w, _)| {
                l.width = cycle_dimension(l.width, w, forward);
            }),
            FieldKey::Height => self.edit_layouts("Change height", |l, _, (_, h)| {
                l.height = cycle_dimension(l.height, h, forward);
            }),
            FieldKey::Direction => self.edit_layouts("Change flow", |l, _, _| {
                let i = DIRECTIONS
                    .iter()
                    .position(|d| *d == direction_name(l.container.direction))
                    .unwrap_or(0);
                let n = DIRECTIONS.len();
                let next = if forward {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                l.container.direction = parse_direction(DIRECTIONS[next]).unwrap_or_default();
            }),
            FieldKey::Align => self.edit_layouts("Change alignment", |l, _, _| {
                let i = ALIGNS
                    .iter()
                    .position(|a| *a == align_name(l.container.align))
                    .unwrap_or(0);
                let n = ALIGNS.len();
                let next = if forward {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                l.container.align = parse_align(ALIGNS[next]).unwrap_or_default();
            }),
            FieldKey::Gap => self.edit_layouts("Change gap", |l, _, _| {
                l.container.gap = step(l.container.gap, forward);
            }),
            FieldKey::Columns => self.edit_layouts("Change columns", |l, _, _| {
                l.container.columns = step(l.container.columns.max(1), forward).max(1);
            }),
            FieldKey::X | FieldKey::Y => {
                let horizontal = field.key == FieldKey::X;
                self.edit_layouts("Move component", |l, (x, y), _| {
                    let (x, y) = match l.placement {
                        Placement::Absolute { x, y } => (x, y),
                        Placement::Flow => (x, y),
                    };
                    l.placement = if horizontal {
                        Placement::Absolute {
                            x: step(x, forward),
                            y,
                        }
                    } else {
                        Placement::Absolute {
                            x,
                            y: step(y, forward),
                        }
                    };
                });
            }
            FieldKey::Prop(name) => {
                let Some(c) = self.primary_component() else {
                    return;
                };
                let current = c.prop(name).cloned();
                let kind = self
                    .descriptor_of(c)
                    .and_then(|d| d.prop_spec(name).map(|s| s.kind.clone()))
                    .unwrap_or(match current {
                        Some(Value::Bool(_)) => PropKind::Bool,
                        Some(Value::Int(_)) => PropKind::int(i64::MIN, i64::MAX),
                        _ => PropKind::Text,
                    });
                match kind.cycle(current.as_ref(), forward) {
                    Some(v) => self.edit_props(name, &v),
                    None => self.inspector_edit(),
                }
            }
            _ => self.inspector_edit(),
        }
    }

    /// Enter on a row: a prompt with the current value and suggestions.
    pub fn inspector_edit(&mut self) {
        let fields = self.inspector_fields();
        let Some(field) = fields.get(self.inspector_row()).cloned() else {
            return;
        };
        let candidates: Vec<String> = match &field.key {
            FieldKey::Placement => vec!["flow".into(), "absolute".into()],
            FieldKey::Width | FieldKey::Height => {
                ["content", "fill", "fixed(10)", "percent(50)", "flex(1)"]
                    .map(String::from)
                    .to_vec()
            }
            FieldKey::Direction => DIRECTIONS.map(String::from).to_vec(),
            FieldKey::Align => ALIGNS.map(String::from).to_vec(),
            FieldKey::Prop(name) => self
                .primary_component()
                .and_then(|c| self.descriptor_of(c))
                .and_then(|d| d.prop_spec(name).map(|s| s.kind.clone()))
                .map(|k| self.prop_candidates(&k))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let hint = match &field.key {
            FieldKey::Padding | FieldKey::Margin => " (1 · 1 2 · 1 2 3 4)",
            FieldKey::MinWidth | FieldKey::MaxWidth | FieldKey::MinHeight | FieldKey::MaxHeight => {
                " (empty: none)"
            }
            FieldKey::Prop(_) => " (empty: unset)",
            _ => "",
        };
        let count = self.selected_ids().len();
        let on = if count > 1 {
            format!("{count} components")
        } else {
            self.selection()
                .map(ToString::to_string)
                .unwrap_or_default()
        };
        self.prompt = Some(
            Prompt::new(
                PromptKind::Inspector(field.key),
                format!("{} of {on}{hint}", field.label),
                candidates,
            )
            .with_input(field.value),
        );
    }

    fn prop_candidates(&self, kind: &PropKind) -> Vec<String> {
        match kind {
            PropKind::Bool => vec!["true".into(), "false".into()],
            PropKind::Choice { options } => options.iter().map(|o| (*o).to_owned()).collect(),
            PropKind::Border => glyphforge_core::boxdraw::BorderFamily::NAMES
                .map(String::from)
                .to_vec(),
            PropKind::Color => COLOR_TOKENS.iter().map(|t| (*t).to_owned()).collect(),
            PropKind::Reference => self
                .screen()
                .layers()
                .iter()
                .map(|l| l.id.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Delete on a row: back to the default (unset property, no clamp,
    /// fill size, zero spacing).
    pub fn inspector_clear(&mut self) {
        let fields = self.inspector_fields();
        let Some(field) = fields.get(self.inspector_row()).cloned() else {
            return;
        };
        match field.key {
            FieldKey::Prop(name) => self.edit_props(&name, &Value::Null),
            key => self.edit_layouts("Reset layout field", move |l, _, _| {
                let d = Layout::default();
                match key {
                    FieldKey::Placement | FieldKey::X | FieldKey::Y => l.placement = d.placement,
                    FieldKey::Width => l.width = d.width,
                    FieldKey::Height => l.height = d.height,
                    FieldKey::MinWidth => l.min_width = None,
                    FieldKey::MaxWidth => l.max_width = None,
                    FieldKey::MinHeight => l.min_height = None,
                    FieldKey::MaxHeight => l.max_height = None,
                    FieldKey::Padding => l.padding = Edges::ZERO,
                    FieldKey::Margin => l.margin = Edges::ZERO,
                    FieldKey::Direction => l.container.direction = d.container.direction,
                    FieldKey::Gap => l.container.gap = 0,
                    FieldKey::Columns => l.container.columns = 0,
                    FieldKey::Align => l.container.align = d.container.align,
                    FieldKey::Prop(_) => {}
                }
            }),
        }
    }

    /// Applies the text typed into an inspector prompt.
    pub(super) fn submit_inspector(&mut self, key: &FieldKey, text: &str) {
        let text = text.trim();
        let bad = |app: &mut Self, what: &str| {
            app.set_status(StatusKind::Warning, format!("{text:?} is not {what}"));
        };
        let opt = |t: &str| -> Result<Option<u16>, ()> {
            if t.is_empty() || t == "none" {
                Ok(None)
            } else {
                t.parse().map(Some).map_err(|_| ())
            }
        };
        match key {
            FieldKey::Prop(name) => {
                if text.is_empty() {
                    self.edit_props(name, &Value::Null);
                    return;
                }
                let spec = self
                    .primary_component()
                    .and_then(|c| self.descriptor_of(c))
                    .and_then(|d| d.prop_spec(name).map(|s| s.kind.clone()));
                let value = match spec {
                    Some(kind) => kind.parse(text),
                    None => parse_property(&format!("{name}={text}"))
                        .map(|(_, v)| v)
                        .ok_or_else(|| "a value".to_owned()),
                };
                match value {
                    Ok(v) => self.edit_props(name, &v),
                    Err(e) => self.set_status(StatusKind::Warning, e),
                }
            }
            FieldKey::Placement => match text {
                "flow" | "absolute" => {
                    let flow = text == "flow";
                    self.edit_layouts("Change placement", move |l, (x, y), _| {
                        l.placement = if flow {
                            Placement::Flow
                        } else {
                            Placement::Absolute { x, y }
                        };
                    });
                }
                _ => bad(self, "flow or absolute"),
            },
            FieldKey::X | FieldKey::Y => {
                let Ok(n) = text.parse::<u16>() else {
                    return bad(self, "a column or row");
                };
                let horizontal = *key == FieldKey::X;
                self.edit_layouts("Move component", move |l, (x, y), _| {
                    let (x, y) = match l.placement {
                        Placement::Absolute { x, y } => (x, y),
                        Placement::Flow => (x, y),
                    };
                    l.placement = if horizontal {
                        Placement::Absolute { x: n, y }
                    } else {
                        Placement::Absolute { x, y: n }
                    };
                });
            }
            FieldKey::Width | FieldKey::Height => {
                let Ok(d) = text.parse::<Dimension>() else {
                    return bad(self, "content, fill, fixed(N), percent(N) or flex(N)");
                };
                let width = *key == FieldKey::Width;
                self.edit_layouts("Change size", move |l, _, _| {
                    if width {
                        l.width = d;
                    } else {
                        l.height = d;
                    }
                });
            }
            FieldKey::MinWidth | FieldKey::MaxWidth | FieldKey::MinHeight | FieldKey::MaxHeight => {
                let Ok(v) = opt(text) else {
                    return bad(self, "a number or empty");
                };
                let key = key.clone();
                self.edit_layouts("Change size limit", move |l, _, _| match key {
                    FieldKey::MinWidth => l.min_width = v,
                    FieldKey::MaxWidth => l.max_width = v,
                    FieldKey::MinHeight => l.min_height = v,
                    _ => l.max_height = v,
                });
            }
            FieldKey::Padding | FieldKey::Margin => {
                let Some(e) = parse_edges(text) else {
                    return bad(self, "1, 2 or 4 numbers");
                };
                let padding = *key == FieldKey::Padding;
                self.edit_layouts("Change spacing", move |l, _, _| {
                    if padding {
                        l.padding = e;
                    } else {
                        l.margin = e;
                    }
                });
            }
            FieldKey::Direction => {
                let Some(d) = parse_direction(text) else {
                    return bad(self, "vertical, horizontal, stack or grid");
                };
                self.edit_layouts("Change flow", move |l, _, _| l.container.direction = d);
            }
            FieldKey::Align => {
                let Some(a) = parse_align(text) else {
                    return bad(self, "stretch, start, center or end");
                };
                self.edit_layouts("Change alignment", move |l, _, _| l.container.align = a);
            }
            FieldKey::Gap | FieldKey::Columns => {
                let Ok(n) = text.parse::<u16>() else {
                    return bad(self, "a number");
                };
                let gap = *key == FieldKey::Gap;
                self.edit_layouts("Change spacing", move |l, _, _| {
                    if gap {
                        l.container.gap = n;
                    } else {
                        l.container.columns = n.max(1);
                    }
                });
            }
        }
    }

    /// Ids of the selection, primary last (so it is applied last).
    pub fn selected_ids(&self) -> Vec<ObjectId> {
        let mut ids: Vec<ObjectId> = self
            .editor
            .also_selected
            .iter()
            .filter(|id| self.doc().component(id).is_some())
            .cloned()
            .collect();
        if let Some(p) = self.selection() {
            ids.retain(|i| i != p);
            ids.push(p.clone());
        }
        ids
    }
}

fn step(v: u16, forward: bool) -> u16 {
    if forward {
        v.saturating_add(1)
    } else {
        v.saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use glyphforge_core::Size;

    fn app_with_panel() -> App {
        let mut app = test_app(Size::new(40, 12));
        app.dispatch(crate::actions::Action::LayerNext);
        let layer = app.editor.layer.clone();
        let id = app
            .session
            .add_component("panel", Some("box"), 2, 1, None, Some(layer))
            .unwrap();
        app.mark_edited();
        app.select(Some(id));
        app
    }

    fn row_of(app: &App, key: &FieldKey) -> usize {
        app.inspector_fields()
            .iter()
            .position(|f| &f.key == key)
            .unwrap()
    }

    #[test]
    fn edges_parse_and_print() {
        assert_eq!(parse_edges("1"), Some(Edges::all(1)));
        assert_eq!(parse_edges("1 2"), Some(Edges::symmetric(1, 2)));
        assert_eq!(
            parse_edges("1,2,3,4"),
            Some(Edges {
                top: 1,
                right: 2,
                bottom: 3,
                left: 4
            })
        );
        assert_eq!(parse_edges("1 2 3"), None);
        assert_eq!(edges_text(Edges::symmetric(1, 2)), "1 2");
        assert_eq!(edges_text(Edges::all(0)), "0");
    }

    #[test]
    fn fields_follow_descriptor_and_container() {
        let app = app_with_panel();
        let fields = app.inspector_fields();
        let labels: Vec<&str> = fields.iter().map(|f| f.label.as_str()).collect();
        assert!(labels.contains(&"x"), "absolute shows x/y: {labels:?}");
        assert!(labels.contains(&"flow"), "panels are containers");
        assert!(labels.contains(&"title"));
        assert!(labels.contains(&"border"));
        let title = fields.iter().find(|f| f.label == "title").unwrap();
        assert_eq!(title.value, "Panel");
    }

    #[test]
    fn cycling_and_editing_apply_patches() {
        let mut app = app_with_panel();
        let id = app.selection().cloned().unwrap();
        app.ui.inspector_row = row_of(&app, &FieldKey::Direction);
        app.inspector_cycle(true);
        assert_eq!(
            app.doc().component(&id).unwrap().layout.container.direction,
            Direction::Horizontal
        );
        app.ui.inspector_row = row_of(&app, &FieldKey::Prop("bold".into()));
        app.inspector_cycle(true);
        assert_eq!(
            app.doc().component(&id).unwrap().prop_bool("bold"),
            Some(true)
        );
        app.inspector_clear();
        assert_eq!(app.doc().component(&id).unwrap().prop("bold"), None);
        // Typed values go through the descriptor's parser.
        app.submit_inspector(&FieldKey::Prop("border".into()), "double");
        assert_eq!(
            app.doc().component(&id).unwrap().prop_str("border"),
            Some("double")
        );
        app.submit_inspector(&FieldKey::Prop("border".into()), "wavy");
        assert_eq!(
            app.doc().component(&id).unwrap().prop_str("border"),
            Some("double")
        );
        app.submit_inspector(&FieldKey::Padding, "1 2");
        assert_eq!(
            app.doc().component(&id).unwrap().layout.padding,
            Edges::symmetric(1, 2)
        );
        app.submit_inspector(&FieldKey::Width, "percent(50)");
        assert_eq!(
            app.doc().component(&id).unwrap().layout.width,
            Dimension::Percent(50)
        );
        // Each edit is one undo step.
        app.dispatch(crate::actions::Action::Undo);
        assert_eq!(
            app.doc().component(&id).unwrap().layout.width,
            Dimension::Fixed(20)
        );
    }

    #[test]
    fn edits_apply_to_every_selected_component() {
        let mut app = app_with_panel();
        let first = app.selection().cloned().unwrap();
        let layer = app.editor.layer.clone();
        let second = app
            .session
            .add_component("panel", Some("other"), 5, 5, None, Some(layer))
            .unwrap();
        app.mark_edited();
        app.select(Some(second.clone()));
        app.editor.also_selected = vec![first.clone()];
        app.submit_inspector(&FieldKey::Prop("title".into()), "Same");
        for id in [&first, &second] {
            assert_eq!(
                app.doc().component(id).unwrap().prop_str("title"),
                Some("Same")
            );
        }
        assert_eq!(app.session.history().undo_len(), 3, "one step for both");
    }

    #[test]
    fn dimension_cycle_order() {
        assert_eq!(
            cycle_dimension(Dimension::Content, 7, true),
            Dimension::Fill
        );
        assert_eq!(
            cycle_dimension(Dimension::Fill, 7, true),
            Dimension::Fixed(7)
        );
        assert_eq!(
            cycle_dimension(Dimension::Content, 7, false),
            Dimension::Flex(1)
        );
    }

    #[test]
    fn multi_select_reparent_and_tree_navigation() {
        use glyphforge_core::Position;
        let mut app = app_with_panel();
        let panel = app.selection().cloned().unwrap();
        let layer = app.editor.layer.clone();
        let label = app
            .session
            .add_component("label", Some("note"), 30, 8, None, Some(layer))
            .unwrap();
        app.mark_edited();
        // Shift+click adds the label to the panel selection.
        app.toggle_select_at(Position::new(30, 8));
        assert_eq!(app.selected_ids(), vec![panel.clone(), label.clone()]);
        app.toggle_select_at(Position::new(30, 8));
        assert_eq!(app.selected_ids(), vec![panel.clone()], "toggled off again");
        // Move the label into the panel: it keeps its screen position.
        app.select(Some(label.clone()));
        let before = app.layout().rect(&label).unwrap();
        app.open_reparent();
        let candidates = app.prompt.as_ref().unwrap().candidates.clone();
        assert!(candidates.contains(&panel.to_string()), "{candidates:?}");
        app.reparent_into(panel.as_str());
        let parent = app.doc().component(&panel).unwrap();
        assert_eq!(parent.children[0].id, label);
        assert_eq!(app.layout().rect(&label).unwrap().x, before.x);
        app.select_parent();
        assert_eq!(app.selection(), Some(&panel));
        app.select_first_child();
        assert_eq!(app.selection(), Some(&label));
        // Deleting a parent and its child together is one operation.
        app.select_all();
        assert_eq!(app.selected_ids().len(), 2);
        app.delete_selection();
        assert!(app.doc().component(&panel).is_none());
        app.dispatch(crate::actions::Action::Undo);
        assert!(app.doc().component(&label).is_some(), "undo restores both");
    }
}
