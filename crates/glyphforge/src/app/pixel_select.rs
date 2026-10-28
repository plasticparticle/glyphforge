//! Pixel-precise selection on pixel layers with the Select tool: a marquee
//! in pixel coordinates, dragging to move, copy, cut, paste and clear.
//! Moves and pastes are one `set_pixels` patch each.

use glyphforge_core::patch::{Operation, Patch};
use glyphforge_core::raster::Pixel;
use glyphforge_core::raster::draw::PixelWrite;
use glyphforge_core::{Canvas, Position, Rect, paint};

use super::{App, StatusKind};

/// Pixels cut or copied from a pixel layer, relative to their top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelClip {
    pub width: u16,
    pub height: u16,
    /// Painted pixels only; transparent ones leave what is beneath.
    pub pixels: Vec<(u16, u16, Pixel)>,
}

/// The selected pixels being dragged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelMove {
    pub grab: Position,
    pub at: Position,
    pub clip: PixelClip,
}

impl PixelMove {
    fn origin(&self, from: Rect) -> Position {
        let dx = i32::from(self.at.x) - i32::from(self.grab.x);
        let dy = i32::from(self.at.y) - i32::from(self.grab.y);
        Position::new(
            (i32::from(from.x) + dx).max(0) as u16,
            (i32::from(from.y) + dy).max(0) as u16,
        )
    }
}

impl App {
    fn pixel_clip(&self, rect: Rect) -> Option<PixelClip> {
        let raster = self.raster()?;
        let pixels = rect
            .positions()
            .filter_map(|p| {
                raster
                    .pixels
                    .get(p.x, p.y)
                    .filter(|px| !px.is_clear())
                    .map(|px| (p.x - rect.x, p.y - rect.y, px))
            })
            .collect();
        Some(PixelClip {
            width: rect.width,
            height: rect.height,
            pixels,
        })
    }

    fn clip_pixel_writes(&self, clip: &PixelClip, origin: Position) -> Vec<PixelWrite> {
        let Some(raster) = self.raster() else {
            return Vec::new();
        };
        let (w, h) = (raster.pixels.width(), raster.pixels.height());
        clip.pixels
            .iter()
            .filter_map(|&(x, y, pixel)| {
                let (px, py) = (origin.x.checked_add(x)?, origin.y.checked_add(y)?);
                (px < w && py < h).then_some(PixelWrite {
                    x: px,
                    y: py,
                    pixel,
                })
            })
            .collect()
    }

    fn clear_pixel_writes(&self, rect: Rect) -> Vec<PixelWrite> {
        let Some(raster) = self.raster() else {
            return Vec::new();
        };
        rect.positions()
            .filter(|p| raster.pixels.get(p.x, p.y).is_some_and(|px| !px.is_clear()))
            .map(|p| PixelWrite {
                x: p.x,
                y: p.y,
                pixel: Pixel::Clear,
            })
            .collect()
    }

    fn apply_pixels(&mut self, pixels: Vec<PixelWrite>, label: &str) -> bool {
        !pixels.is_empty()
            && self.apply(
                Patch::single(Operation::SetPixels {
                    layer: self.editor.layer.clone(),
                    pixels,
                }),
                label,
            )
    }

    pub(super) fn pixel_select_press(&mut self, p: Position) {
        if let Some(rect) = self.paint.pixel_marquee.filter(|r| r.contains(p)) {
            if let Some(clip) = self.pixel_clip(rect) {
                self.paint.pixel_moving = Some(PixelMove {
                    grab: p,
                    at: p,
                    clip,
                });
                return;
            }
        }
        self.paint.pixel_marquee = None;
        self.paint.pixel_anchor = Some(p);
        self.paint.pixel_end = Some(p);
    }

    pub(super) fn pixel_select_drag(&mut self, p: Position) -> bool {
        match self.paint.pixel_moving.as_mut() {
            Some(m) => {
                m.at = p;
                true
            }
            None => false,
        }
    }

    /// Finishes a move or a marquee; `false` when the Select tool had
    /// nothing in progress.
    pub(super) fn pixel_select_release(&mut self) -> bool {
        if let Some(m) = self.paint.pixel_moving.take() {
            let Some(from) = self.paint.pixel_marquee else {
                return true;
            };
            let to = m.origin(from);
            if to != Position::new(from.x, from.y) && self.editable_raster() {
                let mut writes = self.clear_pixel_writes(from);
                writes.extend(self.clip_pixel_writes(&m.clip, to));
                if self.apply_pixels(writes, "Move pixels") {
                    self.paint.pixel_marquee = Some(Rect::new(to.x, to.y, from.width, from.height));
                }
            }
            return true;
        }
        let Some(anchor) = self.paint.pixel_anchor.take() else {
            return false;
        };
        let end = self.paint.pixel_end.take().unwrap_or(anchor);
        let rect = paint::rect_between(anchor, end);
        self.paint.pixel_marquee = Some(rect);
        self.set_status(
            StatusKind::Info,
            format!(
                "Selected {}×{} pixels: drag to move, Ctrl+C/X copy/cut, Delete clears",
                rect.width, rect.height
            ),
        );
        true
    }

    /// Ctrl+A on a pixel layer.
    pub(super) fn select_all_pixels(&mut self) {
        let Some((w, h)) = self.raster().map(|r| (r.pixels.width(), r.pixels.height())) else {
            return;
        };
        self.paint.tool = super::Tool::Select;
        self.paint.pixel_marquee = Some(Rect::new(0, 0, w, h));
        self.set_status(StatusKind::Info, format!("Selected all {w}×{h} pixels"));
    }

    pub(super) fn clear_pixel_marquee(&mut self) -> bool {
        let Some(rect) = self.paint.pixel_marquee else {
            return false;
        };
        if self.editable_raster() {
            let writes = self.clear_pixel_writes(rect);
            self.apply_pixels(writes, "Clear pixels");
        }
        true
    }

    /// Copy (and cut) the selected pixels.
    pub(super) fn copy_pixels(&mut self, cut: bool) {
        let Some(rect) = self.paint.pixel_marquee else {
            return self.set_status(StatusKind::Info, "Select pixels first: the Select tool (v)");
        };
        let Some(clip) = self.pixel_clip(rect) else {
            return;
        };
        self.clipboard = Some(super::selection::Clipboard::Pixels(clip));
        if cut {
            self.clear_pixel_marquee();
        }
        let verb = if cut { "Cut" } else { "Copied" };
        self.set_status(
            StatusKind::Info,
            format!("{verb} {}×{} pixels", rect.width, rect.height),
        );
    }

    /// Pastes pixels at the selection's corner or the pixel cursor.
    pub(super) fn paste_pixels(&mut self, clip: &PixelClip) {
        if !self.editable_raster() {
            return;
        }
        let origin = self
            .paint
            .pixel_marquee
            .map_or_else(|| self.pixel_cursor(), |r| Position::new(r.x, r.y));
        let writes = self.clip_pixel_writes(clip, origin);
        if self.apply_pixels(writes, "Paste pixels") {
            self.paint.tool = super::Tool::Select;
            self.paint.pixel_marquee = Some(Rect::new(origin.x, origin.y, clip.width, clip.height));
            self.set_status(StatusKind::Info, "Pasted: drag the selection to move it");
        }
    }

    /// The screen with the dragged pixels at their new place.
    pub(super) fn pixel_move_preview(&self) -> Option<Canvas> {
        let m = self.paint.pixel_moving.as_ref()?;
        let from = self.paint.pixel_marquee?;
        let mut screen = self.screen().clone();
        let writes = {
            let mut w = self.clear_pixel_writes(from);
            w.extend(self.clip_pixel_writes(&m.clip, m.origin(from)));
            w
        };
        let raster = screen.layer_mut(&self.editor.layer)?.raster_mut()?;
        for w in writes {
            raster.pixels.set(w.x, w.y, w.pixel);
        }
        glyphforge_core::render::render_screen(
            &screen,
            &self.doc().theme,
            self.session.registry(),
            self.editor.preview_size,
        )
        .ok()
    }

    /// The pixel marquee as the cells it touches, for the overlay.
    pub fn pixel_marquee_cells(&self) -> Option<Rect> {
        if self.paint.tool != super::Tool::Select {
            return None;
        }
        let rect = if let (Some(a), Some(b)) = (self.paint.pixel_anchor, self.paint.pixel_end) {
            paint::rect_between(a, b)
        } else {
            let r = self.paint.pixel_marquee?;
            match &self.paint.pixel_moving {
                Some(m) => {
                    let o = m.origin(r);
                    Rect::new(o.x, o.y, r.width, r.height)
                }
                None => r,
            }
        };
        let (cw, ch) = self.raster()?.mode.cell_px();
        let x0 = rect.x / cw;
        let y0 = rect.y / ch;
        let x1 = (rect.x + rect.width).div_ceil(cw);
        let y1 = (rect.y + rect.height).div_ceil(ch);
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::{Action, PixelAction};
    use crate::app::{Tool, test_app};
    use glyphforge_core::raster::{Pixel, RasterMode};
    use glyphforge_core::{Color, Position, Size};

    #[test]
    fn select_move_copy_paste_pixels() {
        let mut app = test_app(Size::new(10, 4));
        app.dispatch(Action::Pixel(PixelAction::NewLayer));
        app.prompt.as_mut().unwrap().input = "half".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.raster().unwrap().mode, RasterMode::Half);
        let red = Pixel::Solid(Color::Rgb(255, 0, 0));
        let layer = app.editor.layer.clone();
        app.apply(
            glyphforge_core::patch::Patch::single(glyphforge_core::patch::Operation::SetPixels {
                layer,
                pixels: vec![glyphforge_core::raster::draw::PixelWrite {
                    x: 1,
                    y: 1,
                    pixel: red,
                }],
            }),
            "Paint",
        );
        app.dispatch(Action::SelectTool(Tool::Select));
        let press = |app: &mut crate::app::App, x, y| {
            app.dispatch(Action::Pixel(PixelAction::Press(
                Position::new(x, y),
                false,
            )));
        };
        press(&mut app, 0, 0);
        app.dispatch(Action::Pixel(PixelAction::Drag(Position::new(2, 2))));
        app.dispatch(Action::Pixel(PixelAction::Release));
        assert!(app.paint.pixel_marquee.is_some());
        // Drag the selection three pixels right.
        press(&mut app, 1, 1);
        app.dispatch(Action::Pixel(PixelAction::Drag(Position::new(4, 1))));
        assert!(app.pixel_preview().is_some(), "the move is previewed");
        app.dispatch(Action::Pixel(PixelAction::Release));
        let at = |app: &crate::app::App, x, y| app.raster().unwrap().pixels.get(x, y);
        assert_eq!(at(&app, 1, 1), Some(Pixel::Clear));
        assert_eq!(at(&app, 4, 1), Some(red));
        app.dispatch(Action::Copy);
        app.paint.pixel_marquee = None;
        app.paint.sub = (0, 0);
        app.dispatch(Action::CursorTo(Position::new(0, 0)));
        app.dispatch(Action::Paste);
        assert_eq!(at(&app, 1, 1), Some(red), "pasted at the pixel cursor");
        app.dispatch(Action::SelectAll);
        app.dispatch(Action::DeleteForward);
        assert_eq!(app.raster().unwrap().pixels.painted(), 0);
    }
}
