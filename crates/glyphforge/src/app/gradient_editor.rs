//! The gradient editor: a bar of the gradient with its stops, edited by
//! keyboard or by dragging the stop markers. Saving writes one
//! `set_gradient` patch, so the whole edit is one undo step.

use glyphforge_core::ObjectId;
use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::raster::{ColorSpace, Gradient, GradientStop};

use super::prompt::{Prompt, PromptKind};
use super::{App, StatusKind};

/// The open gradient editor: a working copy and the selected stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradientEditor {
    pub gradient: Gradient,
    pub selected: usize,
    /// The stop being dragged with the mouse.
    pub dragging: bool,
}

impl GradientEditor {
    fn stop(&self) -> Option<GradientStop> {
        self.gradient.stops.get(self.selected).copied()
    }

    /// Keeps stops sorted, following the selected one.
    fn resort(&mut self) {
        let Some(chosen) = self.stop() else { return };
        self.gradient.stops.sort_by_key(|s| s.at);
        self.selected = self
            .gradient
            .stops
            .iter()
            .position(|s| *s == chosen)
            .unwrap_or(0);
    }

    pub fn select(&mut self, forward: bool) {
        let n = self.gradient.stops.len();
        self.selected = if forward {
            (self.selected + 1) % n
        } else {
            (self.selected + n - 1) % n
        };
    }

    /// Moves the selected stop by `delta` percent.
    pub fn nudge(&mut self, delta: i16) {
        if let Some(s) = self.gradient.stops.get_mut(self.selected) {
            s.at = (i16::from(s.at) + delta).clamp(0, 100) as u8;
        }
        self.resort();
    }

    /// Puts the selected stop at `at` percent (mouse drag).
    pub fn place(&mut self, at: u8) {
        if let Some(s) = self.gradient.stops.get_mut(self.selected) {
            s.at = at.min(100);
        }
        self.resort();
    }

    /// Adds a stop halfway to the next one, coloured like the gradient
    /// there, and selects it.
    pub fn add(&mut self) {
        let stops = &self.gradient.stops;
        let a = stops[self.selected].at;
        let b = stops.get(self.selected + 1).map_or(100, |s| s.at);
        let (a, b) = if a == b && a == 100 {
            (stops[self.selected.saturating_sub(1)].at, a)
        } else {
            (a, b)
        };
        let at = a + (b - a) / 2;
        let color = self.gradient.sample(f32::from(at) / 100.0);
        self.gradient.stops.push(GradientStop { at, color });
        self.gradient.stops.sort_by_key(|s| s.at);
        self.selected = self
            .gradient
            .stops
            .iter()
            .position(|s| s.at == at && s.color == color)
            .unwrap_or(0);
    }

    /// Removes the selected stop; a gradient keeps at least two.
    pub fn remove(&mut self) -> bool {
        if self.gradient.stops.len() <= 2 {
            return false;
        }
        self.gradient.stops.remove(self.selected);
        self.selected = self.selected.min(self.gradient.stops.len() - 1);
        true
    }

    pub fn reverse(&mut self) {
        for s in &mut self.gradient.stops {
            s.at = 100 - s.at;
        }
        self.gradient.stops.reverse();
        self.selected = self.gradient.stops.len() - 1 - self.selected;
    }

    pub fn toggle_space(&mut self) {
        self.gradient.space = match self.gradient.space {
            ColorSpace::Rgb => ColorSpace::Oklab,
            ColorSpace::Oklab => ColorSpace::Rgb,
        };
    }

    /// The stop nearest to `at` percent.
    pub fn nearest(&self, at: u8) -> usize {
        self.gradient
            .stops
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| s.at.abs_diff(at))
            .map_or(0, |(i, _)| i)
    }
}

impl App {
    /// Opens the editor on the current gradient (presets are copied).
    pub fn open_gradient_editor(&mut self) {
        let Some(gradient) = self.current_gradient() else {
            return self.open_gradient_prompt();
        };
        self.gradient_editor = Some(GradientEditor {
            gradient,
            selected: 0,
            dragging: false,
        });
    }

    /// Saves the working copy into the document (one undo step).
    pub fn save_gradient_editor(&mut self) {
        let Some(editor) = self.gradient_editor.take() else {
            return;
        };
        let mut gradient = editor.gradient;
        // A preset becomes a document gradient under a free id.
        if self.doc().gradient(&gradient.id).is_none() && self.doc().has_id(&gradient.id) {
            let doc = self.doc();
            gradient.id = gradient.id.unique_among(|i| doc.has_id(i));
        }
        let id: ObjectId = gradient.id.clone();
        let name = gradient.name.clone();
        if self.apply(
            Patch::single(Operation::SetGradient { gradient }),
            "Edit gradient",
        ) {
            self.paint.gradient = Some(id);
            self.set_status(StatusKind::Info, format!("Saved gradient {name}"));
        }
    }

    /// Opens the colour prompt for the selected stop.
    pub fn edit_gradient_stop_color(&mut self) {
        let Some(stop) = self.gradient_editor.as_ref().and_then(GradientEditor::stop) else {
            return;
        };
        let mut candidates: Vec<String> =
            self.paint.recent.iter().map(ToString::to_string).collect();
        candidates.extend(super::paint::COLOR_NAMES.iter().map(|n| (*n).to_owned()));
        self.prompt = Some(
            Prompt::new(
                PromptKind::GradientStopColor,
                "Stop colour: name, 0-255 or #rrggbb",
                candidates,
            )
            .with_input(stop.color.to_string()),
        );
    }

    pub(super) fn submit_gradient_stop_color(&mut self, value: &str) {
        let Some(color) = super::paint::parse_color(value) else {
            return self.set_status(StatusKind::Warning, format!("Unknown colour {value:?}"));
        };
        if let Some(e) = self.gradient_editor.as_mut() {
            if let Some(s) = e.gradient.stops.get_mut(e.selected) {
                s.color = color;
            }
        }
    }

    /// Opens the text form of the gradient (name and stops).
    pub fn edit_gradient_as_text(&mut self) {
        self.gradient_editor = None;
        self.open_gradient_prompt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphforge_core::Color;

    fn editor() -> GradientEditor {
        GradientEditor {
            gradient: Gradient::new(
                ObjectId::slugify("g"),
                "G",
                vec![
                    GradientStop {
                        at: 0,
                        color: Color::Rgb(0, 0, 0),
                    },
                    GradientStop {
                        at: 100,
                        color: Color::Rgb(255, 255, 255),
                    },
                ],
            )
            .unwrap(),
            selected: 0,
            dragging: false,
        }
    }

    #[test]
    fn stops_add_move_remove_and_stay_sorted() {
        let mut e = editor();
        e.add();
        assert_eq!(e.gradient.stops.len(), 3);
        assert_eq!(e.gradient.stops[e.selected].at, 50);
        e.nudge(60);
        assert_eq!(e.gradient.stops.last().unwrap().at, 100);
        assert_eq!(e.selected, 1, "the moved stop stays selected after sorting");
        e.place(10);
        assert_eq!(e.gradient.stops[1].at, 10);
        assert_eq!(e.nearest(12), 1);
        assert!(e.remove());
        assert!(!e.remove(), "two stops remain");
        e.reverse();
        assert_eq!(e.gradient.stops[0].color, Color::Rgb(255, 255, 255));
        e.toggle_space();
        assert_eq!(e.gradient.space, ColorSpace::Oklab);
    }
}
