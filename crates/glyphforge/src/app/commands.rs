//! The command palette (Ctrl+Space, Ctrl+P): every action by name, fuzzy
//! matched over titles, keywords and aliases and weighted by recent use,
//! plus entries that depend on the context: component kinds to add,
//! layers to switch to, themes, and what the current selection offers
//! (flip, swap colours or turn cells into a component on artwork layers).

use glyphforge_core::align::Axis;
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::{Component, ObjectId, Theme, Value, paint};

use super::{App, DesignMode, StatusKind};
use crate::actions::{Action, Category, Context, MenuAction, descriptors};

/// How many recently run commands are remembered for ranking.
const RECENT: usize = 12;

/// One palette entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub title: String,
    /// What the entry belongs to (category, "Add", "Layer", ...).
    pub group: String,
    /// The first key bound to it, if any.
    pub key: String,
    /// Lower-case keywords and aliases matched after the title.
    pub words: String,
    pub action: Action,
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPalette {
    pub input: String,
    pub selected: usize,
    pub commands: Vec<Command>,
}

impl CommandPalette {
    /// Indices of matching commands, best first. Recently used commands
    /// rank higher; with no input they come first in order of use.
    pub fn matches(&self, recent: &[String]) -> Vec<usize> {
        let query = self.input.trim().to_lowercase();
        let boost = |c: &Command| {
            recent
                .iter()
                .position(|r| *r == c.title)
                .map_or(0, |i| 40 - 3 * i32::try_from(i).unwrap_or(13))
        };
        let mut scored: Vec<(i32, usize)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                if query.is_empty() {
                    return Some((boost(c), i));
                }
                let title = fuzzy_score(&query, &c.title.to_lowercase());
                let words = fuzzy_score(&query, &c.words).map(|s| s / 2);
                title.max(words).map(|s| (s + boost(c), i))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, i)| i).collect()
    }
}

/// Subsequence match of `query` in `text` (both lower case): points per
/// matched character, more for runs and for word starts, fewer for gaps.
/// `None` when not every query character is found in order.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.chars().collect();
    let mut score = 0;
    let mut pos = 0;
    let mut last: Option<usize> = None;
    for q in query.chars().filter(|c| !c.is_whitespace()) {
        let found = (pos..text.len()).find(|&i| text[i] == q)?;
        score += 10;
        if found == 0 || !text[found - 1].is_alphanumeric() {
            score += 10;
        }
        match last {
            Some(l) if l + 1 == found => score += 18,
            Some(l) => score -= i32::try_from(found - l - 1).unwrap_or(8).min(8),
            None => score -= i32::try_from(found).unwrap_or(10).min(10),
        }
        last = Some(found);
        pos = found + 1;
    }
    Some(score)
}

/// Actions the palette does not list: cursor steps and other keys that
/// only make sense as keys.
fn listed(category: Category, name: &str) -> bool {
    category != Category::Cursor
        && !name.starts_with("move-selection-")
        && !name.starts_with("pixel-cursor-")
        && !matches!(
            name,
            "backspace" | "new-line" | "delete-forward" | "cancel" | "command-palette"
        )
}

impl App {
    /// The entries for the current state.
    fn command_entries(&self) -> Vec<Command> {
        let context = self.key_context();
        let mut out: Vec<Command> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for d in descriptors() {
            let applies = d.context == Context::Global || d.context == context;
            if !d.implemented || !applies || !listed(d.category, d.name) {
                continue;
            }
            if !seen.insert(d.title) {
                continue;
            }
            let key = self
                .keymap
                .chords_for(&d.action)
                .first()
                .map(ToString::to_string)
                .unwrap_or_default();
            let mut words: Vec<&str> = d.keywords.to_vec();
            words.extend(d.aliases);
            out.push(Command {
                title: d.title.to_owned(),
                group: d.category.title().to_owned(),
                key,
                words: words.join(" ").to_lowercase(),
                action: d.action.clone(),
            });
        }
        // Context first: what the selection offers.
        let mut context_entries = Vec::new();
        if self.design_mode() == DesignMode::Subcell && self.paint.marquee.is_some() {
            for (title, action, words) in [
                (
                    "Flip Selection Horizontally",
                    Action::FlipSelection(Axis::Horizontal),
                    "mirror reverse left right",
                ),
                (
                    "Flip Selection Vertically",
                    Action::FlipSelection(Axis::Vertical),
                    "mirror upside down",
                ),
                (
                    "Swap Colours In Selection",
                    Action::InvertSelection,
                    "invert foreground background reverse",
                ),
                (
                    "Make Component From Selection",
                    Action::SelectionToComponent,
                    "artwork convert interface asset logo",
                ),
            ] {
                context_entries.push(Command {
                    title: title.to_owned(),
                    group: "Selection".to_owned(),
                    key: String::new(),
                    words: words.to_owned(),
                    action,
                });
            }
        }
        if self.design_mode() == DesignMode::Interface {
            for d in self.session.registry().descriptors() {
                let words = format!(
                    "{} {} {} component new insert",
                    d.kind,
                    d.category.name(),
                    d.keywords.join(" ")
                );
                out.push(Command {
                    title: format!("Add {}", d.title),
                    group: d.category.name().to_owned(),
                    key: String::new(),
                    words: words.to_lowercase(),
                    action: Action::AddKind(d.kind.to_owned()),
                });
            }
        }
        for layer in self.screen().layers() {
            if layer.id == self.editor.layer {
                continue;
            }
            out.push(Command {
                title: format!("Go To Layer {}", layer.name),
                group: "Layer".to_owned(),
                key: String::new(),
                words: format!("layer switch activate {} {}", layer.id, layer.kind().name()),
                action: Action::ActivateLayer(layer.id.clone()),
            });
        }
        for screen in self.doc().screens() {
            if screen.id == self.editor.screen {
                continue;
            }
            out.push(Command {
                title: format!("Go To Screen {}", screen.name),
                group: "Screen".to_owned(),
                key: String::new(),
                words: format!("screen switch page {}", screen.id),
                action: Action::Screen(super::ScreenAction::Switch(screen.id.clone())),
            });
        }
        for theme in Theme::builtin() {
            out.push(Command {
                title: format!("Editor Theme {}", theme.name),
                group: "Theme".to_owned(),
                key: String::new(),
                words: format!("theme colours appearance {}", theme.id),
                action: Action::Menu(MenuAction::SetTheme(theme.id.to_string())),
            });
        }
        context_entries.extend(out);
        context_entries
    }

    pub fn open_command_palette(&mut self) {
        self.prompt = None;
        self.menu = None;
        self.picker = None;
        self.palette = Some(CommandPalette {
            input: String::new(),
            selected: 0,
            commands: self.command_entries(),
        });
    }

    /// Matching entries of the open palette, best first.
    pub fn palette_matches(&self) -> Vec<&Command> {
        let Some(p) = &self.palette else {
            return Vec::new();
        };
        p.matches(&self.recent_commands)
            .into_iter()
            .map(|i| &p.commands[i])
            .collect()
    }

    pub fn palette_input(&mut self, c: char) {
        if let Some(p) = self.palette.as_mut() {
            p.input.push(c);
            p.selected = 0;
        }
    }

    pub fn palette_backspace(&mut self) {
        if let Some(p) = self.palette.as_mut() {
            p.input.pop();
            p.selected = 0;
        }
    }

    pub fn palette_move(&mut self, down: bool) {
        let n = self.palette_matches().len();
        if let Some(p) = self.palette.as_mut() {
            if n > 0 {
                p.selected = if down {
                    (p.selected + 1) % n
                } else {
                    (p.selected + n - 1) % n
                };
            }
        }
    }

    /// Runs the highlighted entry (or the one at `index`).
    pub fn palette_run(&mut self, index: Option<usize>) {
        let chosen = {
            let matches = self.palette_matches();
            let i = index.unwrap_or_else(|| self.palette.as_ref().map_or(0, |p| p.selected));
            matches.get(i).map(|c| (c.title.clone(), c.action.clone()))
        };
        self.palette = None;
        let Some((title, action)) = chosen else {
            return;
        };
        self.recent_commands.retain(|t| *t != title);
        self.recent_commands.insert(0, title);
        self.recent_commands.truncate(RECENT);
        self.dispatch(action);
    }

    // --- context actions -----------------------------------------------------

    pub fn flip_selection(&mut self, axis: Axis) {
        let (Some(rect), Some(canvas)) = (
            self.paint.marquee,
            self.active_layer().and_then(|l| l.cells()),
        ) else {
            self.set_status(StatusKind::Info, "Select cells first (Select tool)");
            return;
        };
        let writes = paint::flip_region(canvas, rect, axis == Axis::Horizontal);
        let label = match axis {
            Axis::Horizontal => "Flip horizontally",
            Axis::Vertical => "Flip vertically",
        };
        if self.editable_cells_layer() && self.apply_set_cells(writes, label) {
            self.set_status(StatusKind::Info, label.to_owned());
        }
    }

    pub fn invert_selection(&mut self) {
        let (Some(rect), Some(canvas)) = (
            self.paint.marquee,
            self.active_layer().and_then(|l| l.cells()),
        ) else {
            self.set_status(StatusKind::Info, "Select cells first (Select tool)");
            return;
        };
        let writes = paint::invert_region(canvas, rect);
        if self.editable_cells_layer() && self.apply_set_cells(writes, "Swap colours") {
            self.set_status(StatusKind::Info, "Swapped colours in the selection");
        }
    }

    /// Places an `artwork` component showing the selected cells on the
    /// top-most interface layer (adding one if needed), at the same spot.
    pub fn selection_to_component(&mut self) {
        let Some(rect) = self.paint.marquee else {
            self.set_status(StatusKind::Info, "Select cells first (Select tool)");
            return;
        };
        let source = self.editor.layer.clone();
        let target = self
            .screen()
            .layers()
            .iter()
            .rev()
            .find(|l| l.components().is_some() && !l.locked)
            .map(|l| l.id.clone());
        let mut ops = Vec::new();
        let ids = self.doc().ids();
        let layer = if let Some(id) = target {
            id
        } else {
            let id = ObjectId::slugify("ui").unique_among(|i| ids.contains(i));
            ops.push(Operation::CreateLayer {
                layer: glyphforge_core::Layer::interface(id.clone(), "Interface"),
                screen: Some(self.editor.screen.clone()),
                index: None,
            });
            id
        };
        let id = ObjectId::slugify(&format!("{source}-art")).unique_among(|i| ids.contains(i));
        let int = |v: u16| Value::Int(i64::from(v));
        let component = Component::new(id.clone(), "artwork")
            .with_prop("layer", source.as_str())
            .with_prop("x", int(rect.x))
            .with_prop("y", int(rect.y))
            .with_prop("width", int(rect.width))
            .with_prop("height", int(rect.height))
            .with_layout(glyphforge_core::layout::Layout::absolute(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
            ));
        ops.push(Operation::CreateComponent {
            component,
            parent: None,
            layer: Some(layer.clone()),
            index: None,
        });
        if self.apply(Patch::new(ops), "Make component from selection") {
            self.paint.marquee = None;
            self.editor.layer = layer;
            self.select(Some(id.clone()));
            self.set_status(
                StatusKind::Info,
                format!("Added artwork component {id}; hide the art layer to use it as an asset"),
            );
        }
    }

    fn apply_set_cells(
        &mut self,
        cells: Vec<glyphforge_core::patch::CellWrite>,
        label: &str,
    ) -> bool {
        !cells.is_empty()
            && self.apply(
                Patch::single(Operation::SetCells {
                    layer: self.editor.layer.clone(),
                    cells,
                }),
                label,
            )
    }

    /// Makes `layer` (on the active screen) the active layer.
    pub fn activate_layer(&mut self, layer: &ObjectId) {
        let Some(l) = self.screen().layer(layer) else {
            self.set_status(
                StatusKind::Warning,
                format!("No layer {layer} on this screen"),
            );
            return;
        };
        let msg = format!("Layer: {} ({})", l.name, l.kind().name());
        self.editor.layer = layer.clone();
        self.select(None);
        self.paint.anchor = None;
        self.paint.marquee = None;
        self.mark_edited();
        self.set_status(StatusKind::Info, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_app;
    use glyphforge_core::{Position, Size};

    #[test]
    fn fuzzy_prefers_word_starts_and_runs() {
        assert!(fuzzy_score("sv", "save document").is_some());
        assert!(fuzzy_score("xz", "save document").is_none());
        let start = fuzzy_score("sd", "save document").unwrap();
        let middle = fuzzy_score("sd", "the asd thing").unwrap();
        assert!(start > middle, "{start} vs {middle}");
        assert!(fuzzy_score("save", "save as").unwrap() > fuzzy_score("save", "s a v e").unwrap());
    }

    #[test]
    fn palette_finds_runs_and_remembers() {
        let mut app = test_app(Size::new(20, 5));
        app.dispatch(Action::CommandPalette);
        for c in "togall".chars() {
            app.palette_input(c);
        }
        assert_eq!(app.palette_matches()[0].title, "Toggle All Panels");
        app.palette_run(None);
        assert!(app.palette.is_none());
        assert!(!app.ui.show_left_panel);
        app.dispatch(Action::CommandPalette);
        assert_eq!(
            app.palette_matches()[0].title,
            "Toggle All Panels",
            "recent first with no input"
        );
    }

    #[test]
    fn context_entries_follow_mode_and_selection() {
        let mut app = test_app(Size::new(20, 5));
        app.dispatch(Action::InsertText("→┌".into()));
        app.dispatch(Action::SelectAll);
        app.dispatch(Action::CommandPalette);
        let titles: Vec<String> = app
            .palette_matches()
            .iter()
            .map(|c| c.title.clone())
            .collect();
        assert_eq!(titles[0], "Flip Selection Horizontally");
        assert!(!titles.iter().any(|t| t == "Add Panel"), "not on artwork");
        app.palette_run(None);
        let row: String = (0..20)
            .filter_map(|x| app.cell_at(Position::new(x, 0)))
            .filter_map(|c| c.as_glyph().map(|g| g.as_str().to_owned()))
            .collect();
        assert_eq!(row, "┐←");
        // Turn the selection into a component on the interface layer.
        app.dispatch(Action::SelectionToComponent);
        let id = app.selection().cloned().unwrap();
        assert_eq!(app.doc().component(&id).unwrap().kind, "artwork");
        assert_eq!(app.design_mode(), DesignMode::Interface);
        app.dispatch(Action::CommandPalette);
        assert!(app.palette_matches().iter().any(|c| c.title == "Add Panel"));
    }
}
