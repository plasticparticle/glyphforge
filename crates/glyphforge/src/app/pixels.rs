//! Pixel Mode: painting sub-cell pixels on raster layers, gradients,
//! dithering and drop shadows. Geometry and encoding live in
//! `glyphforge_core::raster`; every change is a patch operation.

use glyphforge_core::history::Origin;
use glyphforge_core::patch::{Operation, Patch, ShadowUpdate};
use glyphforge_core::raster::draw::{self, PixelWrite};
use glyphforge_core::raster::{
    Dither, DropShadow, Gradient, GradientShape, GradientStop, GradientStyle, PaletteLimit, Pixel,
    RasterLayer, RasterMode, gradient,
};
use glyphforge_core::render::render_screen;
use glyphforge_core::{Canvas, Color, Layer, LayerKind, ObjectId, Position};

use super::prompt::{Prompt, PromptKind};
use super::{App, Focus, StatusKind, Tool};
use crate::actions::{Direction, PixelAction};

/// Gradients offered before the document defines any of its own. Using one
/// copies it into the document.
pub fn preset_gradients() -> Vec<Gradient> {
    let make = |id: &str, name: &str, stops: &[(u8, Color)]| {
        Gradient::new(
            ObjectId::slugify(id),
            name,
            stops
                .iter()
                .map(|&(at, color)| GradientStop { at, color })
                .collect(),
        )
        .ok()
    };
    let hex = |s: &str| Color::from_hex(s).unwrap_or_default();
    [
        make(
            "sunset",
            "Sunset",
            &[(0, hex("#ff5f6d")), (100, hex("#ffc371"))],
        ),
        make(
            "ocean",
            "Ocean",
            &[
                (0, hex("#0f2027")),
                (50, hex("#2c5364")),
                (100, hex("#6dd5ed")),
            ],
        ),
        make(
            "fire",
            "Fire",
            &[
                (0, hex("#1a0000")),
                (40, hex("#b31217")),
                (75, hex("#ff8c00")),
                (100, hex("#ffe66d")),
            ],
        ),
        make(
            "mono",
            "Mono",
            &[(0, hex("#000000")), (100, hex("#ffffff"))],
        ),
        make(
            "neon",
            "Neon",
            &[
                (0, hex("#12c2e9")),
                (50, hex("#c471ed")),
                (100, hex("#f64f59")),
            ],
        ),
        make(
            "forest",
            "Forest",
            &[(0, hex("#0b3d0b")), (100, hex("#a8e063"))],
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The layer kind a tool paints on, if it needs a particular one.
const fn needed_kind(tool: Tool) -> Option<LayerKind> {
    match tool {
        Tool::Text => Some(LayerKind::Artwork),
        Tool::Gradient => Some(LayerKind::Raster),
        _ => None,
    }
}

impl App {
    pub(super) fn dispatch_pixel(&mut self, action: PixelAction) {
        match action {
            PixelAction::NewLayer => self.open_new_pixel_layer(),
            PixelAction::Settings => self.open_pixel_settings(),
            PixelAction::CycleMode => {
                if let Some(mode) = self.raster().map(|r| r.mode) {
                    let i = RasterMode::ALL.iter().position(|m| *m == mode).unwrap_or(0);
                    self.set_raster_mode(RasterMode::ALL[(i + 1) % RasterMode::ALL.len()]);
                }
            }
            PixelAction::CycleDither => {
                if let Some(d) = self.raster().map(|r| r.dither) {
                    let i = Dither::ALL.iter().position(|x| *x == d).unwrap_or(0);
                    self.update_raster(
                        None,
                        Some(Dither::ALL[(i + 1) % Dither::ALL.len()]),
                        None,
                        None,
                    );
                }
            }
            PixelAction::ToggleShadow => {
                if let Some(shadow) = self.raster().map(|r| r.shadow) {
                    let next = match shadow {
                        Some(_) => ShadowUpdate::Off,
                        None => ShadowUpdate::Set(DropShadow::default()),
                    };
                    self.update_raster(None, None, None, Some(next));
                }
            }
            PixelAction::SizeUp if !self.on_raster_layer() => {
                self.set_brush_size(self.paint.size.saturating_add(1));
            }
            PixelAction::SizeDown if !self.on_raster_layer() => {
                self.set_brush_size(self.paint.size.saturating_sub(1));
            }
            PixelAction::SizeUp => self.set_pixel_size(self.paint.pixel_size.saturating_add(1)),
            PixelAction::SizeDown => self.set_pixel_size(self.paint.pixel_size.saturating_sub(1)),
            PixelAction::Cursor(dir) => self.move_pixel_cursor(dir),
            PixelAction::EditGradient => self.open_gradient_prompt(),
            PixelAction::CycleGradientStyle => {
                let next = match self.paint.gradient_style {
                    GradientStyle::Smooth => GradientStyle::Hard(5),
                    GradientStyle::Hard(_) => GradientStyle::Dither,
                    GradientStyle::Dither => GradientStyle::Shade,
                    GradientStyle::Shade => GradientStyle::Smooth,
                };
                self.set_gradient_style(next);
            }
            PixelAction::Press(pos, whole_cell) => self.pixel_press(pos, whole_cell),
            PixelAction::Drag(pos) => self.pixel_drag(pos),
            PixelAction::Release => self.pixel_release(),
            PixelAction::Pick(pos) => self.pick_pixel(pos),
            PixelAction::SetMode(mode) => self.set_raster_mode(mode),
            PixelAction::SetDither(d) => {
                self.update_raster(None, Some(d), None, None);
            }
            PixelAction::SetPalette(p) => {
                self.update_raster(None, None, Some(p), None);
            }
            PixelAction::ShadowOffset(delta) => self.edit_shadow(|s| {
                s.dx = (s.dx + delta).clamp(-16, 16);
                s.dy = (s.dy + delta).clamp(-16, 16);
            }),
            PixelAction::ShadowSoftness(soft) => self.edit_shadow(|s| s.softness = soft),
            PixelAction::ShadowOpacity(opacity) => self.edit_shadow(|s| s.opacity = opacity),
            PixelAction::ShadowColorFromBrush => {
                let fg = self.editor.style.fg;
                self.edit_shadow(|s| s.color = fg);
            }
            PixelAction::SelectGradient(id) => {
                let name = self
                    .gradients()
                    .into_iter()
                    .find(|g| g.id == id)
                    .map(|g| g.name);
                self.paint.gradient = Some(id);
                if self.paint.tool != Tool::Gradient {
                    self.select_tool(Tool::Gradient);
                }
                if let Some(name) = name {
                    self.set_status(StatusKind::Info, format!("Gradient: {name}"));
                }
            }
            PixelAction::GradientStyle(style) => self.set_gradient_style(style),
            PixelAction::GradientShape(shape) => {
                self.paint.gradient_shape = shape;
                let name = match shape {
                    GradientShape::Linear => "linear",
                    GradientShape::Radial => "radial",
                };
                self.set_status(StatusKind::Info, format!("Gradient shape: {name}"));
            }
        }
    }

    pub fn on_raster_layer(&self) -> bool {
        self.active_layer().and_then(Layer::raster).is_some()
    }

    pub fn raster(&self) -> Option<&RasterLayer> {
        self.active_layer().and_then(Layer::raster)
    }

    /// Pixels per cell of the active raster layer.
    pub fn cell_px(&self) -> (u16, u16) {
        self.raster().map_or((1, 1), |r| r.mode.cell_px())
    }

    /// The keyboard pixel cursor: the cell cursor plus the sub-cell offset.
    pub fn pixel_cursor(&self) -> Position {
        let (cw, ch) = self.cell_px();
        let (sx, sy) = self.paint.sub;
        Position::new(
            self.editor.cursor.x * cw + sx.min(cw - 1),
            self.editor.cursor.y * ch + sy.min(ch - 1),
        )
    }

    fn set_pixel_cursor(&mut self, p: Position) {
        let (cw, ch) = self.cell_px();
        self.set_cursor(Position::new(p.x / cw, p.y / ch), None);
        self.paint.sub = (p.x % cw, p.y % ch);
    }

    fn move_pixel_cursor(&mut self, dir: Direction) {
        let p = self.pixel_cursor();
        let (cw, ch) = self.cell_px();
        let size = self.doc_size();
        let (max_x, max_y) = (size.width * cw - 1, size.height * ch - 1);
        let next = match dir {
            Direction::Up => Position::new(p.x, p.y.saturating_sub(1)),
            Direction::Down => Position::new(p.x, (p.y + 1).min(max_y)),
            Direction::Left => Position::new(p.x.saturating_sub(1), p.y),
            Direction::Right => Position::new((p.x + 1).min(max_x), p.y),
        };
        self.set_pixel_cursor(next);
    }

    fn set_pixel_size(&mut self, size: u8) {
        self.paint.pixel_size = size.clamp(1, 8);
        let text = format!("Pixel brush size {}", self.paint.pixel_size);
        self.set_status(StatusKind::Info, text);
    }

    // --- layers -----------------------------------------------------------------

    /// Makes sure the active layer suits `tool`: switches to the top-most
    /// visible unlocked layer of the needed kind, or adds one. Returns
    /// whether a layer was added, `None` when no suitable layer exists.
    pub(super) fn ensure_layer_for(&mut self, tool: Tool) -> Option<bool> {
        let active = self.active_layer().map(Layer::kind);
        let wanted = match (needed_kind(tool), active) {
            (Some(kind), Some(current)) if kind == current => return Some(false),
            (Some(kind), _) => kind,
            (None, Some(LayerKind::Artwork | LayerKind::Raster)) => return Some(false),
            (None, _) => LayerKind::Artwork,
        };
        let existing = self
            .screen()
            .layers()
            .iter()
            .rev()
            .find(|l| l.visible && !l.locked && l.kind() == wanted)
            .map(|l| l.id.clone());
        if let Some(id) = existing {
            self.editor.layer = id;
            self.editor.selection = None;
            self.mark_edited();
            return Some(false);
        }
        let added = match wanted {
            LayerKind::Raster => self.add_pixel_layer(RasterMode::default()),
            _ => self.add_paint_layer(),
        };
        added.then_some(true)
    }

    fn add_pixel_layer(&mut self, mode: RasterMode) -> bool {
        let ids = self.doc().ids();
        let id = ObjectId::slugify("pixels").unique_among(|i| ids.contains(i));
        let layer = Layer::raster_layer(id.clone(), "Pixels", self.doc_size(), mode);
        let op = Operation::CreateLayer {
            layer,
            screen: Some(self.editor.screen.clone()),
            index: None,
        };
        let added = self.apply(Patch::single(op), "Add pixel layer");
        if added {
            self.editor.layer = id;
            self.editor.selection = None;
        }
        added
    }

    fn open_new_pixel_layer(&mut self) {
        let modes = RasterMode::ALL.iter().map(|m| m.name().to_owned());
        let mut candidates: Vec<String> = modes.collect();
        candidates.rotate_left(2); // sextant first
        self.prompt = Some(Prompt::new(
            PromptKind::NewPixelLayer,
            "New pixel layer: sextant (2×3), quadrant (2×2), half (1×2) or braille (2×4)",
            candidates,
        ));
    }

    pub fn submit_new_pixel_layer(&mut self, value: &str) {
        let Some(mode) = RasterMode::parse(value.trim()) else {
            self.set_status(StatusKind::Warning, format!("Unknown pixel mode {value:?}"));
            return;
        };
        if self.add_pixel_layer(mode) {
            if matches!(self.paint.tool, Tool::Text) {
                self.paint.tool = Tool::Brush;
            }
            self.set_status(
                StatusKind::Info,
                format!(
                    "Added pixel layer ({}): paint with the mouse or Space",
                    mode.name()
                ),
            );
        }
    }

    // --- settings ------------------------------------------------------------------

    /// Whether the active layer is a raster layer that accepts edits.
    fn editable_raster(&mut self) -> bool {
        match self.active_layer() {
            Some(l) if l.locked => {
                let name = l.name.clone();
                self.set_status(StatusKind::Warning, format!("Layer {name:?} is locked"));
                false
            }
            Some(l) if l.raster().is_some() => true,
            _ => {
                self.set_status(
                    StatusKind::Warning,
                    "Not a pixel layer: Alt+P adds one, Ctrl+PgUp/PgDn switches layers",
                );
                false
            }
        }
    }

    fn update_raster(
        &mut self,
        mode: Option<RasterMode>,
        dither: Option<Dither>,
        palette: Option<PaletteLimit>,
        shadow: Option<ShadowUpdate>,
    ) -> bool {
        if !self.editable_raster() {
            return false;
        }
        let op = Operation::UpdateRaster {
            target: self.editor.layer.clone(),
            mode,
            dither,
            palette,
            shadow,
        };
        let ok = self.apply(Patch::single(op), "Pixel layer settings");
        if ok {
            self.set_status(StatusKind::Info, self.raster_summary());
        }
        ok
    }

    fn set_raster_mode(&mut self, mode: RasterMode) {
        let cursor = self.pixel_cursor();
        let (ow, oh) = self.cell_px();
        if self.update_raster(Some(mode), None, None, None) {
            let (nw, nh) = mode.cell_px();
            self.paint.sub = ((cursor.x % ow) * nw / ow, (cursor.y % oh) * nh / oh);
        }
    }

    fn edit_shadow(&mut self, change: impl FnOnce(&mut DropShadow)) {
        let Some(current) = self.raster().map(|r| r.shadow) else {
            self.editable_raster();
            return;
        };
        let mut shadow = current.unwrap_or_default();
        change(&mut shadow);
        self.update_raster(None, None, None, Some(ShadowUpdate::Set(shadow)));
    }

    /// One line describing the active raster layer's settings.
    pub fn raster_summary(&self) -> String {
        let Some(r) = self.raster() else {
            return String::new();
        };
        let shadow = r.shadow.map_or_else(
            || "off".to_owned(),
            |s| format!("{},{} soft {} {}%", s.dx, s.dy, s.softness, s.opacity),
        );
        format!(
            "{} · dither {} · {} · shadow {shadow}",
            r.mode.name(),
            r.dither.name(),
            r.palette.name()
        )
    }

    fn open_pixel_settings(&mut self) {
        let Some(r) = self.raster() else {
            self.editable_raster();
            return;
        };
        let mut input = format!(
            "mode={} dither={} palette={}",
            r.mode.name(),
            r.dither.name(),
            r.palette.name()
        );
        match r.shadow {
            Some(s) => {
                input = format!(
                    "{input} shadow=on shadow-x={} shadow-y={} soft={} opacity={} shadow-color={}",
                    s.dx, s.dy, s.softness, s.opacity, s.color
                );
            }
            None => input.push_str(" shadow=off"),
        }
        self.prompt = Some(
            Prompt::new(
                PromptKind::PixelSettings,
                "Pixel layer: mode, dither (none ordered shade ordered-shade), palette, shadow",
                vec![],
            )
            .with_input(input),
        );
    }

    pub fn submit_pixel_settings(&mut self, value: &str) {
        let Some(r) = self.raster() else { return };
        let (mut mode, mut dither, mut palette) = (None, None, None);
        let mut shadow = r.shadow;
        for token in value.split_whitespace() {
            let Some((key, val)) = token.split_once('=') else {
                return self.set_status(
                    StatusKind::Warning,
                    format!("Expected key=value, got {token:?}"),
                );
            };
            let num = || val.parse::<i16>().ok();
            let ok = match key {
                "mode" => RasterMode::parse(val).map(|m| mode = Some(m)).is_some(),
                "dither" => Dither::parse(val).map(|d| dither = Some(d)).is_some(),
                "palette" => PaletteLimit::parse(val)
                    .map(|p| palette = Some(p))
                    .is_some(),
                "shadow" => match val {
                    "on" => {
                        shadow = shadow.or_else(|| Some(DropShadow::default()));
                        true
                    }
                    "off" => {
                        shadow = None;
                        true
                    }
                    _ => false,
                },
                "shadow-x" | "shadow-y" | "soft" | "opacity" => num()
                    .map(|n| {
                        let s = shadow.get_or_insert_with(DropShadow::default);
                        match key {
                            "shadow-x" => s.dx = n.clamp(-16, 16),
                            "shadow-y" => s.dy = n.clamp(-16, 16),
                            "soft" => s.softness = n.clamp(0, 8) as u8,
                            _ => s.opacity = n.clamp(0, 100) as u8,
                        }
                    })
                    .is_some(),
                "shadow-color" => super::paint::parse_color(val)
                    .map(|c| shadow.get_or_insert_with(DropShadow::default).color = c)
                    .is_some(),
                _ => false,
            };
            if !ok {
                return self.set_status(StatusKind::Warning, format!("Invalid setting {token:?}"));
            }
        }
        self.update_raster(
            mode,
            dither,
            palette,
            Some(ShadowUpdate::from_option(shadow)),
        );
    }

    // --- gradients ------------------------------------------------------------------

    /// The document's gradients, then the presets it does not define.
    pub fn gradients(&self) -> Vec<Gradient> {
        let mut out = self.doc().gradients.clone();
        for preset in preset_gradients() {
            if !out.iter().any(|g| g.id == preset.id) {
                out.push(preset);
            }
        }
        out
    }

    pub fn current_gradient(&self) -> Option<Gradient> {
        let all = self.gradients();
        self.paint
            .gradient
            .as_ref()
            .and_then(|id| all.iter().find(|g| &g.id == id).cloned())
            .or_else(|| all.into_iter().next())
    }

    fn set_gradient_style(&mut self, style: GradientStyle) {
        self.paint.gradient_style = style;
        let name = match style {
            GradientStyle::Smooth => "smooth".to_owned(),
            GradientStyle::Hard(n) => format!("hard, {n} bands"),
            GradientStyle::Dither => "dithered sub-cells".to_owned(),
            GradientStyle::Shade => "shade glyphs".to_owned(),
        };
        self.set_status(StatusKind::Info, format!("Gradient style: {name}"));
    }

    fn open_gradient_prompt(&mut self) {
        let current = self
            .current_gradient()
            .map(|g| format!("{}: {}", g.name, g.stops_text()))
            .unwrap_or_default();
        let candidates = self
            .gradients()
            .iter()
            .map(|g| format!("{}: {}", g.name, g.stops_text()))
            .collect();
        self.prompt = Some(
            Prompt::new(
                PromptKind::Gradient,
                "Gradient: Name: colour position, colour position, ... (saved in the document)",
                candidates,
            )
            .with_input(current),
        );
    }

    pub fn submit_gradient(&mut self, value: &str) {
        let Some((name, stops)) = value.split_once(':').map(|(n, s)| (n.trim(), s.trim())) else {
            return self.set_status(
                StatusKind::Warning,
                "Expected Name: colour position, colour position",
            );
        };
        let stops = match Gradient::parse_stops(stops, super::paint::parse_color) {
            Ok(s) => s,
            Err(e) => return self.report_error(&e),
        };
        let base = ObjectId::slugify(name);
        let doc = self.doc();
        let id = if doc.gradient(&base).is_some() {
            base
        } else {
            base.unique_among(|i| doc.has_id(i))
        };
        let gradient = match Gradient::new(id.clone(), name, stops) {
            Ok(g) => g,
            Err(e) => return self.report_error(&e),
        };
        if self.apply(
            Patch::single(Operation::SetGradient { gradient }),
            "Save gradient",
        ) {
            self.paint.gradient = Some(id);
            self.set_status(StatusKind::Info, format!("Saved gradient {name}"));
        }
    }

    // --- painting -------------------------------------------------------------------

    /// The pixels a press or stroke point covers: the brush square, or the
    /// whole cell when the mouse cannot report sub-cell positions.
    fn footprint(&self, p: Position, whole_cell: bool) -> Vec<Position> {
        if whole_cell {
            let (cw, ch) = self.cell_px();
            let origin = Position::new(p.x / cw * cw, p.y / ch * ch);
            glyphforge_core::Rect::new(origin.x, origin.y, cw, ch)
                .positions()
                .collect()
        } else {
            draw::footprint(p, self.paint.pixel_size)
        }
    }

    fn brush_pixel(&self) -> Pixel {
        if self.paint.tool == Tool::Eraser {
            Pixel::Clear
        } else {
            Pixel::Solid(self.editor.style.fg)
        }
    }

    fn record_pixels(&mut self, pixels: Vec<PixelWrite>) {
        if pixels.is_empty() {
            return;
        }
        let op = Operation::SetPixels {
            layer: self.editor.layer.clone(),
            pixels,
        };
        match self.session.record(op) {
            Ok(()) => self.mark_edited(),
            Err(e) => self.report_error(&e),
        }
    }

    pub fn pixel_press(&mut self, p: Position, whole_cell: bool) {
        self.ui.focus = Focus::Canvas;
        self.set_pixel_cursor(p);
        match self.paint.tool {
            Tool::Brush | Tool::Eraser => {
                if !self.editable_raster() {
                    return;
                }
                let label = if self.paint.tool == Tool::Brush {
                    "Paint pixels"
                } else {
                    "Erase pixels"
                };
                if let Err(e) = self.session.begin(label) {
                    return self.report_error(&e);
                }
                self.paint.pixel_stroke = Some(p);
                self.paint.pixel_whole_cell = whole_cell;
                let positions = self.footprint(p, whole_cell);
                self.stroke_pixels(&positions);
            }
            Tool::Line | Tool::Rect | Tool::Box | Tool::Gradient => {
                if self.editable_raster() {
                    self.paint.pixel_anchor = Some(p);
                    self.paint.pixel_end = Some(p);
                }
            }
            Tool::Fill => self.fill_pixels(p),
            Tool::Picker => self.pick_pixel(p),
            Tool::Text | Tool::Select => self.set_status(
                StatusKind::Warning,
                format!("{} works on artwork layers", self.paint.tool.title()),
            ),
        }
    }

    fn stroke_pixels(&mut self, positions: &[Position]) {
        let Some(raster) = self.raster() else { return };
        let writes = draw::writes(&raster.pixels, positions, self.brush_pixel());
        self.record_pixels(writes);
    }

    pub fn pixel_drag(&mut self, p: Position) {
        if let Some(last) = self.paint.pixel_stroke {
            if last == p {
                return;
            }
            self.set_pixel_cursor(p);
            let whole = self.paint.pixel_whole_cell;
            let mut seen = std::collections::HashSet::new();
            let positions: Vec<Position> = glyphforge_core::paint::line(last, p)
                .into_iter()
                .flat_map(|q| self.footprint(q, whole))
                .filter(|q| seen.insert(*q))
                .collect();
            self.stroke_pixels(&positions);
            self.paint.pixel_stroke = Some(p);
        } else if self.paint.pixel_anchor.is_some() {
            self.set_pixel_cursor(p);
            self.paint.pixel_end = Some(p);
        }
    }

    pub fn pixel_release(&mut self) {
        if self.paint.pixel_stroke.take().is_some() {
            self.commit_edits();
        } else if let Some(anchor) = self.paint.pixel_anchor.take() {
            let end = self.paint.pixel_end.take().unwrap_or(anchor);
            self.commit_pixel_shape(anchor, end);
        }
    }

    /// Space on a raster layer.
    pub(super) fn apply_pixel_tool(&mut self) {
        let p = self.pixel_cursor();
        match self.paint.tool {
            Tool::Brush | Tool::Eraser => {
                if !self.editable_raster() {
                    return;
                }
                let Some(raster) = self.raster() else { return };
                let writes = draw::writes(
                    &raster.pixels,
                    &self.footprint(p, false),
                    self.brush_pixel(),
                );
                self.apply_pixel_writes(writes, None, "Paint pixels");
            }
            Tool::Line | Tool::Rect | Tool::Box | Tool::Gradient => {
                if let Some(anchor) = self.paint.pixel_anchor.take() {
                    self.paint.pixel_end = None;
                    self.commit_pixel_shape(anchor, p);
                } else if self.editable_raster() {
                    self.paint.pixel_anchor = Some(p);
                    self.paint.pixel_end = Some(p);
                    self.set_status(
                        StatusKind::Info,
                        format!(
                            "{} from pixel {},{}: move with Shift+arrows (or arrows per cell), Space again",
                            self.paint.tool.title(),
                            p.x,
                            p.y
                        ),
                    );
                }
            }
            Tool::Fill => self.fill_pixels(p),
            Tool::Picker => self.pick_pixel(p),
            Tool::Text | Tool::Select => self.set_status(
                StatusKind::Warning,
                format!("{} works on artwork layers", self.paint.tool.title()),
            ),
        }
    }

    /// The pixel writes (and a preset gradient to save) of the shape from
    /// `anchor` to `end` for the current tool.
    pub fn pixel_shape(
        &self,
        anchor: Position,
        end: Position,
    ) -> (Vec<PixelWrite>, Option<Gradient>) {
        let Some(raster) = self.raster() else {
            return (Vec::new(), None);
        };
        let size = self.paint.pixel_size;
        let pixel = Pixel::Solid(self.editor.style.fg);
        match self.paint.tool {
            Tool::Line => (
                draw::writes(&raster.pixels, &draw::stroke(anchor, end, size), pixel),
                None,
            ),
            Tool::Rect => (
                draw::writes(
                    &raster.pixels,
                    &draw::rect_outline(anchor, end, size),
                    pixel,
                ),
                None,
            ),
            Tool::Box => {
                let area: Vec<Position> = glyphforge_core::paint::rect_between(anchor, end)
                    .positions()
                    .collect();
                (draw::writes(&raster.pixels, &area, pixel), None)
            }
            Tool::Gradient => {
                let Some(g) = self.current_gradient() else {
                    return (Vec::new(), None);
                };
                let region = draw::flood(&raster.pixels, anchor);
                let pixels = gradient::fill(
                    &region,
                    raster.mode,
                    &g,
                    self.paint.gradient_shape,
                    self.paint.gradient_style,
                    (f32::from(anchor.x), f32::from(anchor.y)),
                    (f32::from(end.x), f32::from(end.y)),
                );
                let save = self.doc().gradient(&g.id).is_none().then_some(g);
                (draw::writes_each(&raster.pixels, &pixels), save)
            }
            _ => (Vec::new(), None),
        }
    }

    fn commit_pixel_shape(&mut self, anchor: Position, end: Position) {
        if !self.editable_raster() {
            return;
        }
        let (writes, save) = self.pixel_shape(anchor, end);
        let label = match self.paint.tool {
            Tool::Gradient => "Gradient fill".to_owned(),
            Tool::Box => "Fill rectangle".to_owned(),
            other => format!("Draw {}", other.title().to_ascii_lowercase()),
        };
        self.apply_pixel_writes(writes, save, &label);
    }

    fn apply_pixel_writes(
        &mut self,
        pixels: Vec<PixelWrite>,
        save: Option<Gradient>,
        label: &str,
    ) -> bool {
        if pixels.is_empty() {
            return false;
        }
        let mut ops = Vec::new();
        if let Some(gradient) = save {
            ops.push(Operation::SetGradient { gradient });
        }
        ops.push(Operation::SetPixels {
            layer: self.editor.layer.clone(),
            pixels,
        });
        match self
            .session
            .apply_patch(Patch::new(ops), label, Origin::User)
        {
            Ok(()) => {
                self.mark_edited();
                true
            }
            Err(e) => {
                self.report_error(&e);
                false
            }
        }
    }

    fn fill_pixels(&mut self, p: Position) {
        if !self.editable_raster() {
            return;
        }
        let Some(raster) = self.raster() else { return };
        let region = draw::flood(&raster.pixels, p);
        let writes = draw::writes(&raster.pixels, &region, Pixel::Solid(self.editor.style.fg));
        let n = writes.len();
        if self.apply_pixel_writes(writes, None, "Fill pixels") {
            self.set_status(StatusKind::Info, format!("Filled {n} pixels"));
        }
    }

    fn pick_pixel(&mut self, p: Position) {
        let pixel = self.raster().and_then(|r| r.pixels.get(p.x, p.y));
        match pixel {
            Some(Pixel::Solid(c) | Pixel::Shade { fg: c, .. }) => {
                self.set_color(super::ColorTarget::Foreground, c);
                if self.paint.tool == Tool::Picker {
                    self.paint.tool = match self.paint.previous_tool {
                        Tool::Picker => Tool::Brush,
                        other => other,
                    };
                }
            }
            _ => self.set_status(
                StatusKind::Info,
                "Nothing to pick: the pixel is transparent",
            ),
        }
    }

    /// The screen as it will look after the shape being dragged, for the
    /// canvas preview.
    pub fn pixel_preview(&self) -> Option<Canvas> {
        let anchor = self.paint.pixel_anchor?;
        let end = self.paint.pixel_end.unwrap_or(anchor);
        let (writes, _) = self.pixel_shape(anchor, end);
        let mut screen = self.screen().clone();
        let raster = screen.layer_mut(&self.editor.layer)?.raster_mut()?;
        for w in writes {
            raster.pixels.set(w.x, w.y, w.pixel);
        }
        render_screen(&screen, &self.doc().theme, self.session.registry(), None).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_app;
    use glyphforge_core::Size;

    fn app(w: u16, h: u16) -> App {
        let mut app = test_app(Size::new(w, h));
        app.editor.viewport.size = Size::new(w, h);
        app
    }

    fn px(app: &App, x: u16, y: u16) -> Pixel {
        app.raster().unwrap().pixels.get(x, y).unwrap()
    }

    fn pixel(action: PixelAction) -> Action {
        Action::Pixel(action)
    }

    fn rendered(app: &mut App) -> Vec<String> {
        glyphforge_core::render::to_text_lines(app.composite())
    }

    const RED: Color = Color::Rgb(255, 0, 0);

    #[test]
    fn pixel_layers_paint_sub_cells_and_undo_per_stroke() {
        let mut app = app(4, 2);
        app.dispatch(pixel(PixelAction::NewLayer));
        app.prompt.as_mut().unwrap().input = "quadrant".into();
        app.dispatch(Action::PromptSubmit);
        assert!(app.on_raster_layer());
        assert_eq!(app.design_mode(), crate::app::DesignMode::Pixel);
        app.dispatch(Action::SetColor(crate::app::ColorTarget::Foreground, RED));
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(pixel(PixelAction::Press(Position::new(0, 0), false)));
        app.dispatch(pixel(PixelAction::Drag(Position::new(3, 0))));
        app.dispatch(pixel(PixelAction::Release));
        assert_eq!(px(&app, 3, 0), Pixel::Solid(RED));
        assert_eq!(px(&app, 0, 1), Pixel::Clear);
        assert_eq!(
            rendered(&mut app)[0],
            "▀▀",
            "top pixel rows become upper halves"
        );
        let undo_len = app.session.history().undo_len();
        app.dispatch(Action::Undo);
        assert_eq!(px(&app, 3, 0), Pixel::Clear);
        assert_eq!(app.session.history().undo_len(), undo_len - 1);
        app.dispatch(Action::Redo);

        // A cell-precise press covers the whole cell.
        app.dispatch(pixel(PixelAction::Press(Position::new(5, 3), true)));
        app.dispatch(pixel(PixelAction::Release));
        assert_eq!(rendered(&mut app)[1], "  █");
    }

    #[test]
    fn keyboard_pixel_cursor_and_shapes() {
        let mut app = app(4, 2);
        app.dispatch(Action::SelectTool(Tool::Gradient));
        assert!(
            app.on_raster_layer(),
            "the gradient tool adds a pixel layer"
        );
        assert_eq!(app.raster().unwrap().mode, RasterMode::Sextant);
        app.dispatch(Action::SelectTool(Tool::Line));
        app.dispatch(Action::CursorTo(Position::new(1, 0)));
        app.dispatch(pixel(PixelAction::Cursor(Direction::Right)));
        app.dispatch(pixel(PixelAction::Cursor(Direction::Down)));
        assert_eq!(app.pixel_cursor(), Position::new(3, 1));
        app.dispatch(Action::ApplyTool);
        app.dispatch(pixel(PixelAction::Cursor(Direction::Down)));
        app.dispatch(pixel(PixelAction::Cursor(Direction::Down)));
        assert!(
            app.pixel_preview().is_some(),
            "shape previews while in progress"
        );
        app.dispatch(Action::ApplyTool);
        for y in 1..=3 {
            assert!(!px(&app, 3, y).is_clear(), "line pixel {y}");
        }
        app.dispatch(Action::SelectTool(Tool::Text));
        assert!(!app.on_raster_layer(), "text switches back to artwork");
    }

    #[test]
    fn settings_dither_and_shadow_are_undoable() {
        let mut app = app(4, 2);
        app.dispatch(pixel(PixelAction::NewLayer));
        app.dispatch(Action::PromptSubmit);
        app.dispatch(pixel(PixelAction::Settings));
        assert!(app.prompt.as_ref().unwrap().input.contains("mode=sextant"));
        app.prompt.as_mut().unwrap().input =
            "mode=half dither=ordered-shade palette=ansi16 shadow=on soft=2 opacity=50 shadow-color=blue".into();
        app.dispatch(Action::PromptSubmit);
        let r = app.raster().unwrap();
        assert_eq!(
            (r.mode, r.dither, r.palette),
            (RasterMode::Half, Dither::OrderedShade, PaletteLimit::Ansi16)
        );
        let shadow = r.shadow.unwrap();
        assert_eq!(
            (shadow.softness, shadow.opacity, shadow.color),
            (2, 50, Color::Indexed(4))
        );
        app.dispatch(Action::Undo);
        assert_eq!(app.raster().unwrap().mode, RasterMode::Sextant);
        app.dispatch(pixel(PixelAction::ToggleShadow));
        assert!(app.raster().unwrap().shadow.is_some());
        app.dispatch(pixel(PixelAction::ShadowOffset(1)));
        assert_eq!(app.raster().unwrap().shadow.unwrap().dx, 3);
        app.dispatch(pixel(PixelAction::CycleDither));
        assert_eq!(app.raster().unwrap().dither, Dither::Ordered);
        app.dispatch(pixel(PixelAction::Settings));
        app.prompt.as_mut().unwrap().input = "dither=sparkly".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );
    }

    #[test]
    fn gradient_fills_save_presets_and_custom_gradients() {
        let mut app = app(8, 2);
        app.dispatch(Action::SelectTool(Tool::Gradient));
        assert_eq!(app.current_gradient().unwrap().name, "Sunset");
        app.dispatch(pixel(PixelAction::GradientStyle(GradientStyle::Hard(2))));
        app.dispatch(pixel(PixelAction::Press(Position::new(0, 0), false)));
        app.dispatch(pixel(PixelAction::Drag(Position::new(15, 0))));
        app.dispatch(pixel(PixelAction::Release));
        assert!(
            app.doc().gradient(&ObjectId::slugify("sunset")).is_some(),
            "preset saved with the fill"
        );
        let left = px(&app, 0, 5);
        let right = px(&app, 15, 5);
        assert!(
            matches!(left, Pixel::Solid(_)) && left != right,
            "{left:?} {right:?}"
        );
        app.dispatch(Action::Undo);
        assert!(px(&app, 0, 0).is_clear());
        assert!(
            app.doc().gradient(&ObjectId::slugify("sunset")).is_none(),
            "one undo step"
        );

        app.dispatch(pixel(PixelAction::EditGradient));
        app.prompt.as_mut().unwrap().input = "Candy: #ff00ff 0, #00ffff 100".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.current_gradient().unwrap().name, "Candy");
        app.dispatch(pixel(PixelAction::GradientStyle(GradientStyle::Shade)));
        app.dispatch(pixel(PixelAction::GradientShape(GradientShape::Radial)));
        app.dispatch(Action::ApplyTool);
        app.dispatch(Action::CursorTo(Position::new(7, 1)));
        app.dispatch(Action::ApplyTool);
        let shaded = app
            .raster()
            .unwrap()
            .pixels
            .pixels()
            .iter()
            .filter(|p| matches!(p, Pixel::Shade { .. }))
            .count();
        assert!(shaded > 0);
        assert!(
            rendered(&mut app)
                .concat()
                .chars()
                .any(|c| "░▒▓".contains(c))
        );
    }

    #[test]
    fn fill_and_pick_on_pixels() {
        let mut app = app(2, 1);
        app.dispatch(pixel(PixelAction::NewLayer));
        app.dispatch(Action::PromptSubmit);
        app.dispatch(Action::SetColor(crate::app::ColorTarget::Foreground, RED));
        app.dispatch(Action::SelectTool(Tool::Fill));
        app.dispatch(pixel(PixelAction::Press(Position::new(1, 1), false)));
        assert!((0..4).all(|x| (0..3).all(|y| px(&app, x, y) == Pixel::Solid(RED))));
        app.dispatch(Action::ResetColors);
        app.dispatch(pixel(PixelAction::Pick(Position::new(2, 2))));
        assert_eq!(app.editor.style.fg, RED);
    }
}
