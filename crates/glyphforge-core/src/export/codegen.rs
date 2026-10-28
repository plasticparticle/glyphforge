//! Code generators: a screen's component tree as starter code for a TUI
//! framework. They reproduce the structure (containers, directions, sizes)
//! and map each kind to the framework's closest widget; what has no
//! counterpart becomes a placeholder with a comment. The output is a
//! starting point for a developer, not a finished program.

use std::fmt::Write as _;

use crate::component::Component;
use crate::document::{Document, Screen};
use crate::id::ObjectId;
use crate::layout::{Dimension, Direction, Placement};
use crate::render::RenderContext;
use crate::render::kit;

/// A target framework.
pub trait Exporter {
    /// The `--format` name.
    fn id(&self) -> &'static str;
    /// File extension of the generated source.
    fn extension(&self) -> &'static str;
    /// Source code for `screen`.
    fn generate(&self, doc: &Document, screen: &Screen) -> String;
}

/// Every code generator.
pub fn exporters() -> Vec<Box<dyn Exporter>> {
    vec![
        Box::new(Ratatui),
        Box::new(Textual),
        Box::new(BubbleTea),
        Box::new(Ink),
    ]
}

/// The generator with this `--format` name.
pub fn exporter(id: &str) -> Option<Box<dyn Exporter>> {
    exporters().into_iter().find(|e| e.id() == id)
}

/// The interface roots of a screen's visible layers, bottom to top.
fn roots(screen: &Screen) -> Vec<&Component> {
    screen
        .layers()
        .iter()
        .filter(|l| l.visible)
        .filter_map(|l| l.components())
        .flatten()
        .collect()
}

/// The text a component shows.
fn label(c: &Component) -> String {
    ["text", "title", "label", "value", "placeholder"]
        .iter()
        .find_map(|k| c.prop_str(k))
        .unwrap_or("")
        .to_owned()
}

fn ident(id: &ObjectId) -> String {
    id.as_str().replace(['-', '.'], "_")
}

/// Strings of a list property, or a bound fixture.
fn items(doc: &Document, c: &Component, key: &str) -> Vec<String> {
    let theme = &doc.theme;
    let ctx = RenderContext {
        theme,
        screen: None,
        data: Some(&doc.data),
        symbols: &doc.symbols,
        depth: 0,
    };
    kit::strings(c, &ctx, key)
}

fn numbers(doc: &Document, c: &Component, key: &str) -> Vec<i64> {
    let ctx = RenderContext {
        theme: &doc.theme,
        screen: None,
        data: Some(&doc.data),
        symbols: &doc.symbols,
        depth: 0,
    };
    kit::numbers(c, &ctx, key)
}

fn rust_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

// --- Ratatui -----------------------------------------------------------------

/// Rust with Ratatui: `Layout` splits for flow containers, `Rect`s for
/// absolutely placed components, one widget per leaf.
#[derive(Debug)]
pub struct Ratatui;

impl Ratatui {
    fn constraint(d: Dimension, horizontal: bool, c: &Component) -> String {
        const ONE_LINE: &[&str] = &[
            "label",
            "heading",
            "button",
            "divider",
            "input",
            "password",
            "search",
            "checkbox",
            "radio",
            "toggle",
            "select",
            "breadcrumbs",
            "toolbar",
            "statusbar",
            "progress",
            "gauge",
            "sparkline",
            "keyhint",
            "toast",
            "badge",
            "tag",
            "spinner",
        ];
        if d == Dimension::Content && !horizontal && ONE_LINE.contains(&c.kind.as_str()) {
            return "Constraint::Length(1)".to_owned();
        }
        match d {
            Dimension::Fixed(n) => format!("Constraint::Length({n})"),
            Dimension::Percent(p) => format!("Constraint::Percentage({p})"),
            Dimension::Flex(n) => format!("Constraint::Fill({n})"),
            Dimension::Fill => "Constraint::Fill(1)".to_owned(),
            Dimension::Content => "Constraint::Min(1)".to_owned(),
        }
    }

    fn widget(doc: &Document, c: &Component) -> String {
        let text = rust_str(&label(c));
        match c.kind.as_str() {
            "panel" | "modal" | "dialog" => format!("Block::bordered().title({text})"),
            "button" => format!("Paragraph::new(format!(\"[ {{}} ]\", {text}))"),
            "heading" => format!("Paragraph::new({text}).bold()"),
            "label" => format!("Paragraph::new({text})"),
            "list" | "menu" | "tree" => {
                let list: Vec<String> =
                    items(doc, c, "items").iter().map(|i| rust_str(i)).collect();
                format!("List::new([{}]).highlight_symbol(\"› \")", list.join(", "))
            }
            "tabs" => {
                let list: Vec<String> =
                    items(doc, c, "items").iter().map(|i| rust_str(i)).collect();
                format!(
                    "Tabs::new([{}]).select({})",
                    list.join(", "),
                    c.prop_int("selected").unwrap_or(0)
                )
            }
            "table" | "datagrid" => {
                let cols: Vec<String> = items(doc, c, "columns")
                    .iter()
                    .map(|i| rust_str(i))
                    .collect();
                format!(
                    "Table::new(rows, [Constraint::Fill(1); {}]).header(Row::new([{}]).bold())",
                    cols.len().max(1),
                    cols.join(", ")
                )
            }
            "progress" | "gauge" => format!(
                "Gauge::default().percent({}).label({})",
                c.prop_int("value").unwrap_or(0).clamp(0, 100),
                rust_str(c.prop_str("label").unwrap_or(""))
            ),
            "sparkline" => {
                let data: Vec<String> = numbers(doc, c, "values")
                    .iter()
                    .map(|n| format!("{}", n.max(&0)))
                    .collect();
                format!("Sparkline::default().data(&[{}])", data.join(", "))
            }
            "barchart" => "BarChart::default() /* TODO: bars from data */".to_owned(),
            "checkbox" | "radio" | "toggle" => format!(
                "Paragraph::new(format!(\"{} {{}}\", {}))",
                if c.prop_bool("checked") == Some(true) {
                    "[x]"
                } else {
                    "[ ]"
                },
                text
            ),
            "divider" => "Block::new().borders(Borders::TOP)".to_owned(),
            "group" => "Block::new()".to_owned(),
            _ => format!("Paragraph::new({text}) /* {} */", c.kind),
        }
    }

    fn emit(s: &mut String, doc: &Document, c: &Component, area: &str, depth: usize) {
        let pad = "    ".repeat(depth);
        let name = ident(&c.id);
        let _ = writeln!(s, "{pad}// {} ({})", c.id, c.kind);
        if !c.children.is_empty() {
            let _ = writeln!(s, "{pad}let {name}_block = {};", Self::widget(doc, c));
            let _ = writeln!(s, "{pad}let {name}_inner = {name}_block.inner({area});");
            let _ = writeln!(s, "{pad}frame.render_widget({name}_block, {area});");
            Self::children(s, doc, c, &format!("{name}_inner"), depth);
        } else {
            let _ = writeln!(
                s,
                "{pad}frame.render_widget({}, {area});",
                Self::widget(doc, c)
            );
        }
    }

    fn children(s: &mut String, doc: &Document, c: &Component, area: &str, depth: usize) {
        let pad = "    ".repeat(depth);
        let flow: Vec<&Component> = c
            .children
            .iter()
            .filter(|k| k.layout.placement == Placement::Flow)
            .collect();
        let horizontal = c.layout.container.direction == Direction::Horizontal;
        if !flow.is_empty() {
            let name = ident(&c.id);
            let constraints: Vec<String> = flow
                .iter()
                .map(|k| {
                    Self::constraint(
                        if horizontal {
                            k.layout.width
                        } else {
                            k.layout.height
                        },
                        horizontal,
                        k,
                    )
                })
                .collect();
            let _ = writeln!(
                s,
                "{pad}let {name}_areas = Layout::{}([{}]).spacing({}).split({area});",
                if horizontal { "horizontal" } else { "vertical" },
                constraints.join(", "),
                c.layout.container.gap
            );
            for (i, k) in flow.iter().enumerate() {
                Self::emit(s, doc, k, &format!("{name}_areas[{i}]"), depth);
            }
        }
        for k in c
            .children
            .iter()
            .filter(|k| k.layout.placement != Placement::Flow)
        {
            Self::absolute(s, doc, k, area, depth);
        }
    }

    fn absolute(out: &mut String, doc: &Document, c: &Component, area: &str, depth: usize) {
        let pad = "    ".repeat(depth);
        let (col, row) = match c.layout.placement {
            Placement::Absolute { x, y } => (x, y),
            Placement::Flow => (0, 0),
        };
        let dim = |d: Dimension| match d {
            Dimension::Fixed(n) => n.to_string(),
            _ => format!("{area}.width"),
        };
        let name = ident(&c.id);
        let _ = writeln!(
            out,
            "{pad}let {name}_area = Rect::new({area}.x + {col}, {area}.y + {row}, {}, {}).intersection({area});",
            dim(c.layout.width),
            match c.layout.height {
                Dimension::Fixed(n) => n.to_string(),
                _ => format!("{area}.height"),
            }
        );
        Self::emit(out, doc, c, &format!("{name}_area"), depth);
    }
}

impl Exporter for Ratatui {
    fn id(&self) -> &'static str {
        "ratatui"
    }

    fn extension(&self) -> &'static str {
        "rs"
    }

    fn generate(&self, doc: &Document, screen: &Screen) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "//! {} — generated by Glyphforge from screen `{}`.",
            screen.name, screen.id
        );
        let _ = writeln!(
            s,
            "//! Starter code: wire up state, rows and events yourself.\n"
        );
        let _ = writeln!(s, "use ratatui::prelude::*;");
        let _ = writeln!(s, "use ratatui::widgets::*;\n");
        let _ = writeln!(s, "pub fn draw(frame: &mut Frame, rows: Vec<Row<'_>>) {{");
        let _ = writeln!(s, "    let area = frame.area();");
        for c in roots(screen) {
            match c.layout.placement {
                Placement::Flow => Self::emit(&mut s, doc, c, "area", 1),
                Placement::Absolute { .. } => Self::absolute(&mut s, doc, c, "area", 1),
            }
        }
        let _ = writeln!(s, "    let _ = rows;");
        let _ = writeln!(s, "}}");
        s
    }
}

// --- Textual -------------------------------------------------------------------

/// Python with Textual: containers in `compose`, sizes in the CSS.
#[derive(Debug)]
pub struct Textual;

impl Textual {
    fn widget(doc: &Document, c: &Component) -> String {
        let text = rust_str(&label(c));
        let id = format!("id={}", rust_str(c.id.as_str()));
        match c.kind.as_str() {
            "button" => format!("Button({text}, {id})"),
            "input" | "search" => format!("Input(placeholder={text}, {id})"),
            "password" => format!("Input(placeholder={text}, password=True, {id})"),
            "textarea" => format!("TextArea({id})"),
            "checkbox" => format!(
                "Checkbox({text}, {}, {id})",
                if c.prop_bool("checked") == Some(true) {
                    "True"
                } else {
                    "False"
                }
            ),
            "radio" => format!("RadioButton({text}, {id})"),
            "toggle" => format!("Switch({id})"),
            "select" => {
                let opts: Vec<String> = items(doc, c, "options")
                    .iter()
                    .map(|o| format!("({}, {})", rust_str(o), rust_str(o)))
                    .collect();
                format!("Select([{}], {id})", opts.join(", "))
            }
            "tabs" => {
                let tabs: Vec<String> =
                    items(doc, c, "items").iter().map(|i| rust_str(i)).collect();
                format!("Tabs({}, {id})", tabs.join(", "))
            }
            "table" | "datagrid" => format!("DataTable({id})"),
            "list" | "menu" => {
                let list: Vec<String> = items(doc, c, "items")
                    .iter()
                    .map(|i| format!("ListItem(Label({}))", rust_str(i)))
                    .collect();
                format!("ListView({}, {id})", list.join(", "))
            }
            "tree" => format!("Tree({text}, {id})"),
            "log" => format!("Log({id})"),
            "markdown" => format!(
                "Markdown({}, {id})",
                rust_str(c.prop_str("text").unwrap_or(""))
            ),
            "progress" | "gauge" => format!(
                "ProgressBar(total={}, {id})",
                c.prop_int("max").unwrap_or(100)
            ),
            "sparkline" => format!("Sparkline({:?}, {id})", numbers(doc, c, "values")),
            "heading" => format!("Label({text}, classes=\"heading\", {id})"),
            _ => format!("Static({text}, {id})"),
        }
    }

    fn compose(s: &mut String, doc: &Document, c: &Component, depth: usize) {
        let pad = "    ".repeat(depth);
        if c.children.is_empty() {
            let _ = writeln!(s, "{pad}yield {}", Self::widget(doc, c));
            return;
        }
        let container = match c.layout.container.direction {
            Direction::Horizontal => "Horizontal",
            Direction::Grid => "Grid",
            _ => "Vertical",
        };
        let _ = writeln!(s, "{pad}with {container}(id={}):", rust_str(c.id.as_str()));
        for k in &c.children {
            Self::compose(s, doc, k, depth + 1);
        }
    }

    fn css(s: &mut String, c: &Component) {
        let size = |d: Dimension| match d {
            Dimension::Fixed(n) => Some(n.to_string()),
            Dimension::Percent(p) => Some(format!("{p}%")),
            Dimension::Flex(n) => Some(format!("{n}fr")),
            Dimension::Fill => Some("1fr".to_owned()),
            Dimension::Content => Some("auto".to_owned()),
        };
        let mut rules = Vec::new();
        if let Some(w) = size(c.layout.width) {
            rules.push(format!("width: {w};"));
        }
        if let Some(h) = size(c.layout.height) {
            rules.push(format!("height: {h};"));
        }
        if matches!(c.kind.as_str(), "panel" | "modal" | "dialog") {
            rules.push("border: round $primary;".to_owned());
            if let Some(t) = c.prop_str("title") {
                rules.push(format!("border-title-align: left; /* title: {t} */"));
            }
        }
        if let Placement::Absolute { x, y } = c.layout.placement {
            rules.push(format!("offset: {x} {y};"));
        }
        let _ = writeln!(s, "    #{} {{ {} }}", c.id, rules.join(" "));
        for k in &c.children {
            Self::css(s, k);
        }
    }
}

impl Exporter for Textual {
    fn id(&self) -> &'static str {
        "textual"
    }

    fn extension(&self) -> &'static str {
        "py"
    }

    fn generate(&self, doc: &Document, screen: &Screen) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "\"\"\"{} — generated by Glyphforge from screen `{}`.\"\"\"\n",
            screen.name, screen.id
        );
        let _ = writeln!(s, "from textual.app import App, ComposeResult");
        let _ = writeln!(
            s,
            "from textual.containers import Grid, Horizontal, Vertical"
        );
        let _ = writeln!(s, "from textual.widgets import *\n\n");
        let class: String = screen
            .name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut ch = w.chars();
                ch.next()
                    .map(|f| f.to_uppercase().collect::<String>() + ch.as_str())
                    .unwrap_or_default()
            })
            .collect();
        let _ = writeln!(s, "class {class}App(App):");
        let _ = writeln!(s, "    CSS = \"\"\"");
        for c in roots(screen) {
            Self::css(&mut s, c);
        }
        let _ = writeln!(s, "    \"\"\"\n");
        let _ = writeln!(s, "    def compose(self) -> ComposeResult:");
        for c in roots(screen) {
            Self::compose(&mut s, doc, c, 2);
        }
        let _ = writeln!(s, "\n\nif __name__ == \"__main__\":");
        let _ = writeln!(s, "    {class}App().run()");
        s
    }
}

// --- Bubble Tea -------------------------------------------------------------------

/// Go with Bubble Tea and Lip Gloss: `View` joins styled blocks.
#[derive(Debug)]
pub struct BubbleTea;

impl BubbleTea {
    fn view(doc: &Document, c: &Component, depth: usize) -> String {
        let pad = "\t".repeat(depth);
        let mut style = String::from("lipgloss.NewStyle()");
        if let Dimension::Fixed(w) = c.layout.width {
            let _ = write!(style, ".Width({w})");
        }
        if let Dimension::Fixed(h) = c.layout.height {
            let _ = write!(style, ".Height({h})");
        }
        if matches!(c.kind.as_str(), "panel" | "modal" | "dialog") {
            style.push_str(".Border(lipgloss.RoundedBorder())");
        }
        if c.kind == "heading" {
            style.push_str(".Bold(true)");
        }
        let body = if c.children.is_empty() {
            match c.kind.as_str() {
                "list" | "menu" => {
                    let list: Vec<String> =
                        items(doc, c, "items").iter().map(|i| rust_str(i)).collect();
                    format!("strings.Join([]string{{{}}}, \"\\n\")", list.join(", "))
                }
                "button" => format!("\"[ \" + {} + \" ]\"", rust_str(&label(c))),
                _ => rust_str(&label(c)),
            }
        } else {
            let join = if c.layout.container.direction == Direction::Horizontal {
                "lipgloss.JoinHorizontal(lipgloss.Top"
            } else {
                "lipgloss.JoinVertical(lipgloss.Left"
            };
            let kids: Vec<String> = c
                .children
                .iter()
                .map(|k| Self::view(doc, k, depth + 1))
                .collect();
            format!("{join},\n{}\n{pad})", kids.join(",\n"))
        };
        format!("{pad}{style}.Render( // {}\n{pad}\t{body})", c.id)
    }
}

impl Exporter for BubbleTea {
    fn id(&self) -> &'static str {
        "bubbletea"
    }

    fn extension(&self) -> &'static str {
        "go"
    }

    fn generate(&self, doc: &Document, screen: &Screen) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "// {} — generated by Glyphforge from screen `{}`.",
            screen.name, screen.id
        );
        let _ = writeln!(s, "package main\n");
        let _ = writeln!(
            s,
            "import (\n\t\"strings\"\n\n\ttea \"github.com/charmbracelet/bubbletea\"\n\t\"github.com/charmbracelet/lipgloss\"\n)\n"
        );
        let _ = writeln!(s, "type model struct{{}}\n");
        let _ = writeln!(s, "func (m model) Init() tea.Cmd {{ return nil }}\n");
        let _ = writeln!(
            s,
            "func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {{"
        );
        let _ = writeln!(
            s,
            "\tif k, ok := msg.(tea.KeyMsg); ok && k.String() == \"q\" {{\n\t\treturn m, tea.Quit\n\t}}"
        );
        let _ = writeln!(s, "\treturn m, nil\n}}\n");
        let _ = writeln!(s, "func (m model) View() string {{");
        let views: Vec<String> = roots(screen)
            .iter()
            .map(|c| Self::view(doc, c, 2))
            .collect();
        let _ = writeln!(
            s,
            "\t_ = strings.Join\n\treturn lipgloss.JoinVertical(lipgloss.Left,\n{}\n\t)",
            views.join(",\n")
        );
        let _ = writeln!(s, "}}\n");
        let _ = writeln!(s, "func main() {{ tea.NewProgram(model{{}}).Run() }}");
        s
    }
}

// --- Ink ------------------------------------------------------------------------------

/// TypeScript with Ink: `Box` flex containers and `Text` leaves.
#[derive(Debug)]
pub struct Ink;

impl Ink {
    fn jsx(doc: &Document, c: &Component, depth: usize) -> String {
        let pad = "  ".repeat(depth);
        let mut attrs = Vec::new();
        let size = |d: Dimension| match d {
            Dimension::Fixed(n) => Some(format!("{{{n}}}")),
            Dimension::Percent(p) => Some(format!("\"{p}%\"")),
            Dimension::Fill | Dimension::Flex(_) | Dimension::Content => None,
        };
        if let Some(w) = size(c.layout.width) {
            attrs.push(format!("width={w}"));
        }
        if let Some(h) = size(c.layout.height) {
            attrs.push(format!("height={h}"));
        }
        if matches!(c.layout.width, Dimension::Fill | Dimension::Flex(_)) {
            attrs.push("flexGrow={1}".to_owned());
        }
        if matches!(c.kind.as_str(), "panel" | "modal" | "dialog") {
            attrs.push("borderStyle=\"round\"".to_owned());
        }
        if c.layout.container.direction == Direction::Horizontal {
            attrs.push("flexDirection=\"row\"".to_owned());
        } else if !c.children.is_empty() {
            attrs.push("flexDirection=\"column\"".to_owned());
        }
        if c.layout.container.gap > 0 {
            attrs.push(format!("gap={{{}}}", c.layout.container.gap));
        }
        let open = format!("{pad}<Box {}>", attrs.join(" "));
        let mut out = format!("{pad}{{/* {} ({}) */}}\n{open}\n", c.id, c.kind);
        if c.children.is_empty() {
            let text = match c.kind.as_str() {
                "list" | "menu" => items(doc, c, "items").join("\n"),
                "button" => format!("[ {} ]", label(c)),
                _ => label(c),
            };
            let bold = if c.kind == "heading" { " bold" } else { "" };
            let _ = writeln!(out, "{pad}  <Text{bold}>{{{}}}</Text>", rust_str(&text));
        } else {
            for k in &c.children {
                out.push_str(&Self::jsx(doc, k, depth + 1));
            }
        }
        let _ = writeln!(out, "{pad}</Box>");
        out
    }
}

impl Exporter for Ink {
    fn id(&self) -> &'static str {
        "ink"
    }

    fn extension(&self) -> &'static str {
        "tsx"
    }

    fn generate(&self, doc: &Document, screen: &Screen) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "// {} — generated by Glyphforge from screen `{}`.",
            screen.name, screen.id
        );
        let _ = writeln!(s, "import React from \"react\";");
        let _ = writeln!(s, "import {{ Box, Text, render }} from \"ink\";\n");
        let _ = writeln!(s, "export function Screen() {{");
        let _ = writeln!(s, "  return (");
        let _ = writeln!(
            s,
            "    <Box flexDirection=\"column\" width={{{}}} height={{{}}}>",
            screen.size().width,
            screen.size().height
        );
        for c in roots(screen) {
            s.push_str(&Self::jsx(doc, c, 3));
        }
        let _ = writeln!(s, "    </Box>");
        let _ = writeln!(s, "  );\n}}\n");
        let _ = writeln!(s, "render(<Screen />);");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Size;
    use crate::id;
    use crate::layout::Layout;
    use crate::value::Value;

    fn doc() -> Document {
        let mut doc = Document::new(Size::new(60, 20)).unwrap();
        let ui = doc.first_screen().layers()[1].id.clone();
        let tree = Component::new(id!("window"), "group")
            .with_layout(Layout::default().with_direction(Direction::Horizontal))
            .with_children(vec![
                Component::new(id!("sidebar"), "panel")
                    .with_prop("title", "Nav")
                    .with_layout(Layout {
                        width: Dimension::Fixed(20),
                        ..Layout::default()
                    })
                    .with_children(vec![
                        Component::new(id!("menu"), "list")
                            .with_prop("items", Value::List(vec!["Home".into(), "Logs".into()])),
                    ]),
                Component::new(id!("content"), "panel").with_prop("title", "Main"),
            ]);
        crate::patch::Patch::single(crate::patch::Operation::CreateComponent {
            component: tree,
            parent: None,
            layer: Some(ui),
            index: None,
        })
        .apply(&mut doc)
        .unwrap();
        doc
    }

    #[test]
    fn every_exporter_mirrors_the_structure() {
        let doc = doc();
        let screen = doc.first_screen();
        let ratatui = Ratatui.generate(&doc, screen);
        assert!(
            ratatui.contains("Layout::horizontal([Constraint::Length(20), Constraint::Fill(1)])"),
            "{ratatui}"
        );
        assert!(ratatui.contains("Block::bordered().title(\"Nav\")"));
        assert!(ratatui.contains("List::new([\"Home\", \"Logs\"])"));
        let textual = Textual.generate(&doc, screen);
        assert!(
            textual.contains("with Horizontal(id=\"window\"):"),
            "{textual}"
        );
        assert!(textual.contains("#sidebar { width: 20;"));
        let go = BubbleTea.generate(&doc, screen);
        assert!(go.contains("lipgloss.JoinHorizontal"), "{go}");
        assert!(go.contains(".Width(20).Border(lipgloss.RoundedBorder())"));
        let ink = Ink.generate(&doc, screen);
        assert!(
            ink.contains("<Box width={20} borderStyle=\"round\""),
            "{ink}"
        );
        assert!(ink.contains("Home\\nLogs"));
        assert_eq!(exporters().len(), 4);
        assert!(exporter("ink").is_some());
    }

    #[test]
    fn handoff_lists_geometry_props_and_tokens() {
        let doc = doc();
        let yaml = super::super::handoff::handoff(&doc, &crate::render::Registry::builtin());
        assert!(yaml.contains("format: glyphforge-handoff"));
        assert!(yaml.contains("- id: sidebar"));
        assert!(
            yaml.contains("rect: {x: 0, y: 0, width: 20, height: 20}"),
            "{yaml}"
        );
        assert!(yaml.contains("title: \"Nav\""));
        assert!(yaml.contains("direction: horizontal"));
    }
}
