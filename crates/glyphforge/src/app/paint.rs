//! Subcell Mode tools and colours: the brush, the tool palette, the colour
//! picker. Geometry comes from `glyphforge_core::paint`; every change to the
//! document is a `set_cells` operation on the session.

use glyphforge_core::boxdraw::BorderFamily;
use glyphforge_core::patch::{CellWrite, Operation, Patch};
use glyphforge_core::{Cell, CellContent, Color, Grapheme, Layer, ObjectId, Position};
use glyphforge_core::{paint, stamp};

use super::brushes::{
    BOX_STYLES, BRUSH_PAGES, BrushChoice, BrushKind, line_style_of, page_of, preset_of,
    search_candidates,
};
use super::prompt::{Prompt, PromptKind};
use super::{App, Focus, StatusKind};
use crate::actions::{Action, Direction};

/// A Subcell Mode tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Typing inserts text at the cursor.
    Text,
    /// Paints the brush glyph; drags draw connected strokes.
    Brush,
    /// Clears cells to transparent.
    Eraser,
    Line,
    /// Rectangle outline in the brush glyph.
    Rect,
    /// Rectangle frame in box-drawing glyphs of the chosen box style.
    Box,
    /// Flood-fills the connected area of identical cells.
    Fill,
    /// Takes glyph and colours from a cell, then returns to the last tool.
    Picker,
    /// Gradient fill on pixel layers: drag from start to end.
    Gradient,
    /// Rectangle selection on artwork layers: drag to select, drag the
    /// selection to move it.
    Select,
}

impl Tool {
    pub const ALL: [Self; 10] = [
        Self::Text,
        Self::Brush,
        Self::Eraser,
        Self::Line,
        Self::Rect,
        Self::Box,
        Self::Fill,
        Self::Picker,
        Self::Gradient,
        Self::Select,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
            Self::Line => "Line",
            Self::Rect => "Rect",
            Self::Box => "Box",
            Self::Fill => "Fill",
            Self::Picker => "Pick",
            Self::Gradient => "Gradient",
            Self::Select => "Select",
        }
    }

    /// Short usage hint for the palette.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Text => "type at the cursor",
            Self::Brush | Self::Eraser => "click, drag or Space",
            Self::Line | Self::Rect | Self::Box | Self::Gradient => "drag, or Space twice",
            Self::Fill | Self::Picker => "click or Space",
            Self::Select => "drag to select, drag inside to move",
        }
    }
}

/// Which of the two brush colours an action changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTarget {
    Foreground,
    Background,
}

impl ColorTarget {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Background => "background",
        }
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Foreground => Self::Background,
            Self::Background => Self::Foreground,
        }
    }
}

/// How many recently used colours are remembered.
pub const RECENT_COLORS: usize = 8;

/// Names accepted by the colour prompt for the ANSI 16 set, by index.
pub const COLOR_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];

/// Parses colour input: `default`, an ANSI name (`bright-blue`), an index
/// (`12`, `ansi:12`) or `#rrggbb`.
pub fn parse_color(input: &str) -> Option<Color> {
    let s = input.trim().to_ascii_lowercase();
    if let Some(i) = COLOR_NAMES
        .iter()
        .position(|n| *n == s || n.replace('-', " ") == s)
    {
        return Some(Color::Indexed(i as u8));
    }
    if let Ok(i) = s.parse::<u8>() {
        return Some(Color::Indexed(i));
    }
    s.parse().ok()
}

/// Human-readable colour name: ANSI names for the 16 base colours.
pub fn color_label(color: Color) -> String {
    match color {
        Color::Indexed(i) if i < 16 => COLOR_NAMES[usize::from(i)].to_owned(),
        other => other.to_string(),
    }
}

/// Whether text on this colour should be dark.
pub fn is_light(color: Color) -> bool {
    color.is_light()
}

/// The colour picker grid: base colours (with the terminal default first),
/// the 6×6×6 cube one red level per row, the grey ramp, recent colours.
pub fn picker_rows(recent: &[Color]) -> Vec<Vec<Color>> {
    let mut rows = vec![
        std::iter::once(Color::Default)
            .chain((0..16).map(Color::Indexed))
            .collect(),
    ];
    for r in 0..6u8 {
        rows.push(
            (0..36u8)
                .map(|gb| Color::Indexed(16 + r * 36 + gb))
                .collect(),
        );
    }
    rows.push((232..=255).map(Color::Indexed).collect());
    if !recent.is_empty() {
        rows.push(recent.to_vec());
    }
    rows
}

/// A truecolour from hue (degrees), saturation and lightness (percent).
pub fn hsl(hue: u16, saturation: u8, lightness: u8) -> Color {
    let hue = f32::from(hue % 360) / 360.0;
    let sat = f32::from(saturation.min(100)) / 100.0;
    let light = f32::from(lightness.min(100)) / 100.0;
    let high = if light < 0.5 {
        light * (1.0 + sat)
    } else {
        light + sat - light * sat
    };
    let low = 2.0 * light - high;
    let channel = |offset: f32| {
        let t = (hue + offset).rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            low + (high - low) * 6.0 * t
        } else if t < 0.5 {
            high
        } else if t < 2.0 / 3.0 {
            low + (high - low) * (2.0 / 3.0 - t) * 6.0
        } else {
            low
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color::Rgb(channel(1.0 / 3.0), channel(0.0), channel(-1.0 / 3.0))
}

/// Lightness of the spectrum rows, light to dark, at full saturation.
const SPECTRUM_LIGHTNESS: [u8; 9] = [88, 78, 68, 58, 50, 42, 34, 26, 18];

/// The truecolour picker grid: base colours, 36 hues (every 10°) at nine
/// lightness levels and once muted, 24 even greys, recent colours.
pub fn spectrum_rows(recent: &[Color]) -> Vec<Vec<Color>> {
    let mut rows = vec![
        std::iter::once(Color::Default)
            .chain((0..16).map(Color::Indexed))
            .collect(),
    ];
    for l in SPECTRUM_LIGHTNESS {
        rows.push((0..36).map(|i| hsl(i * 10, 100, l)).collect());
    }
    rows.push((0..36).map(|i| hsl(i * 10, 35, 55)).collect());
    rows.push(
        (0..24u16)
            .map(|i| {
                let v = (i * 255 / 23) as u8;
                Color::Rgb(v, v, v)
            })
            .collect(),
    );
    if !recent.is_empty() {
        rows.push(recent.to_vec());
    }
    rows
}

/// Open colour picker popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPicker {
    pub target: ColorTarget,
    pub row: usize,
    pub col: usize,
    /// Truecolour spectrum instead of the 256-colour grid.
    pub spectrum: bool,
}

/// Tool state that is not part of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintState {
    pub tool: Tool,
    /// The tool the picker returns to.
    pub previous_tool: Tool,
    pub brush: Grapheme,
    /// First corner of a shape being drawn; the cursor is the second.
    pub anchor: Option<Position>,
    /// Last painted position of an open mouse stroke.
    pub stroke: Option<Position>,
    /// Most recently chosen colours, newest first.
    pub recent: Vec<Color>,
    /// Index into [`BRUSH_PAGES`] shown by the palette.
    pub page: usize,
    /// What the brush paints; glyph brushes use `brush`.
    pub kind: BrushKind,
    /// Label of stamp brushes.
    pub label: String,
    /// Whether the open stroke has left its first cell.
    pub stroke_moved: bool,
    /// Pixel Mode: brush size in pixels.
    pub pixel_size: u8,
    /// Pixel Mode: the pixel cursor's offset inside the cursor cell.
    pub sub: (u16, u16),
    /// Pixel Mode: last pixel of an open stroke, and whether the stroke
    /// came from a cell-precise mouse.
    pub pixel_stroke: Option<Position>,
    pub pixel_whole_cell: bool,
    /// Pixel Mode: start and current end of a shape or gradient drag.
    pub pixel_anchor: Option<Position>,
    pub pixel_end: Option<Position>,
    /// Selected gradient (document or preset id); the first when `None`.
    pub gradient: Option<glyphforge_core::ObjectId>,
    pub gradient_style: glyphforge_core::raster::GradientStyle,
    pub gradient_shape: glyphforge_core::raster::GradientShape,
    /// Palette page on pixel layers.
    pub pixel_page: usize,
    /// Line style of the Box tool.
    pub box_style: BorderFamily,
    /// Selected cells (Select tool).
    pub marquee: Option<glyphforge_core::Rect>,
    /// The selected cells being dragged.
    pub moving: Option<super::selection::CellMove>,
    /// Brush size on artwork layers: a square of this many cells.
    pub size: u8,
}

impl Default for PaintState {
    fn default() -> Self {
        Self {
            tool: Tool::Text,
            previous_tool: Tool::Brush,
            brush: Grapheme::new("█").unwrap_or_else(|_| Grapheme::space()),
            anchor: None,
            stroke: None,
            recent: Vec::new(),
            page: 0,
            kind: BrushKind::Glyph,
            label: "OK".to_owned(),
            stroke_moved: false,
            pixel_size: 1,
            sub: (0, 0),
            pixel_stroke: None,
            pixel_whole_cell: false,
            pixel_anchor: None,
            pixel_end: None,
            gradient: None,
            gradient_style: glyphforge_core::raster::GradientStyle::Smooth,
            gradient_shape: glyphforge_core::raster::GradientShape::Linear,
            pixel_page: 0,
            box_style: BorderFamily::Single,
            marquee: None,
            moving: None,
            size: 1,
        }
    }
}

impl App {
    /// Tool, brush, colour and picker actions; called from `dispatch`.
    pub(super) fn dispatch_paint(&mut self, action: Action) {
        match action {
            Action::SelectTool(tool) => self.select_tool(tool),
            Action::ApplyTool => self.apply_tool(),
            Action::ToolPress(pos) => self.tool_press(pos),
            Action::ToolDrag(pos) => self.tool_drag(pos),
            Action::ToolRelease => self.tool_release(),
            Action::PickAt(pos) => {
                self.set_cursor(pos, None);
                self.pick_at(pos);
            }
            Action::SetBrushGlyph(text) => self.set_brush_glyph(&text),
            Action::EditBrushGlyph => self.open_glyph_prompt(),
            Action::SetColor(target, color) => {
                self.picker = None;
                self.set_color(target, color);
            }
            Action::OpenColorPicker(target) => self.open_color_picker(target),
            Action::ColorPrompt(target) => self.open_color_prompt(target),
            Action::SwapColors => self.swap_colors(),
            Action::ResetColors => self.reset_colors(),
            Action::PickerMove(dir) => self.picker_move(dir),
            Action::PickerSubmit => self.picker_submit(),
            Action::PickerSwitchTarget => self.picker_switch_target(),
            Action::PickerToggleSpectrum => self.picker_toggle_spectrum(),
            Action::BrushPageNext => self.step_brush_page(true),
            Action::BrushPagePrev => self.step_brush_page(false),
            Action::SetBrushKind(kind) => self.set_brush_kind(kind),
            Action::EditStampLabel => self.open_label_prompt(),
            Action::SetBoxStyle(style) => self.set_box_style(style),
            Action::CycleBoxStyle => {
                let i = BOX_STYLES
                    .iter()
                    .position(|s| *s == self.paint.box_style)
                    .map_or(0, |i| (i + 1) % BOX_STYLES.len());
                self.set_box_style(BOX_STYLES[i]);
            }
            other => tracing::debug!(?other, "not a paint action"),
        }
    }

    pub(super) fn set_brush_size(&mut self, size: u8) {
        self.paint.size = size.clamp(1, 9);
        let n = self.paint.size;
        self.set_status(StatusKind::Info, format!("Brush size {n}×{n}"));
    }

    pub fn toggle_attr(&mut self, attr: crate::actions::Attr) {
        let flag = attr.flag(&mut self.editor.style.attrs);
        *flag = !*flag;
        let state = if *flag { "on" } else { "off" };
        self.set_status(
            StatusKind::Info,
            format!("{} {state}", capitalised(attr.name())),
        );
    }

    /// `path` widened to the brush size: a square around each point, each
    /// cell once, in path order.
    fn sized(&self, path: &[Position]) -> Vec<Position> {
        let n = u16::from(self.paint.size.max(1));
        if n == 1 {
            return path.to_vec();
        }
        let size = self.doc_size();
        let back = (n - 1) / 2;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for p in path {
            for dy in 0..n {
                for dx in 0..n {
                    let (Some(x), Some(y)) =
                        ((p.x + dx).checked_sub(back), (p.y + dy).checked_sub(back))
                    else {
                        continue;
                    };
                    let q = Position::new(x, y);
                    if size.contains(q) && seen.insert(q) {
                        out.push(q);
                    }
                }
            }
        }
        out
    }

    /// The cell the brush paints: brush glyph in the current colours.
    pub fn brush_cell(&self) -> Cell {
        Cell::glyph(self.paint.brush.clone(), self.editor.style)
    }

    pub fn select_tool(&mut self, tool: Tool) {
        self.paint.anchor = None;
        self.paint.pixel_anchor = None;
        if tool != Tool::Select {
            self.paint.marquee = None;
        }
        if tool == Tool::Picker && self.paint.tool != Tool::Picker {
            self.paint.previous_tool = self.paint.tool;
        }
        self.paint.tool = tool;
        let Some(added) = self.ensure_layer_for(tool) else {
            return;
        };
        let mut text = format!("{} tool: {}", tool.title(), tool.hint());
        if added {
            let name = self
                .active_layer()
                .map(|l| l.name.clone())
                .unwrap_or_default();
            text = format!("{text} · added layer {name:?} to draw on (Ctrl+Z removes it)");
        }
        self.set_status(StatusKind::Info, text);
    }

    /// Adds an artwork layer on top of the screen (undoable) and makes it
    /// active, so painting works on designs that have only interface layers
    /// or hidden artwork assets.
    pub(super) fn add_paint_layer(&mut self) -> bool {
        let ids = self.doc().ids();
        let id = ObjectId::slugify("paint").unique_among(|i| ids.contains(i));
        let layer = match Layer::artwork(id.clone(), "Paint", self.doc_size()) {
            Ok(l) => l,
            Err(e) => {
                self.report_error(&e);
                return false;
            }
        };
        let op = Operation::CreateLayer {
            layer,
            screen: Some(self.editor.screen.clone()),
            index: None,
        };
        let added = self.apply(Patch::single(op), "Add paint layer");
        if added {
            self.editor.layer = id;
            self.editor.selection = None;
        }
        added
    }

    pub fn step_brush_page(&mut self, forward: bool) {
        let pages = self.palette_pages();
        let n = pages.len().max(1);
        let current = self.palette_page();
        let next = if forward {
            (current + 1) % n
        } else {
            (current + n - 1) % n
        };
        if self.on_raster_layer() {
            self.paint.pixel_page = next;
        } else {
            self.paint.page = next;
        }
        if let Some(page) = pages.get(next) {
            let title = BRUSH_PAGES
                .get(next)
                .filter(|_| !self.on_raster_layer())
                .map_or(page.name.as_str(), |p| p.title);
            let text = format!("Palette {}/{n}: {title}", next + 1);
            self.set_status(StatusKind::Info, text);
        }
    }

    /// Sets the Box tool's line style and activates the Box tool.
    pub fn set_box_style(&mut self, style: BorderFamily) {
        self.paint.box_style = style;
        if self.paint.tool != Tool::Box {
            self.select_tool(Tool::Box);
        }
        self.set_status(
            StatusKind::Info,
            format!("Box style: {} {}", style.glyphs().top_left, style.name()),
        );
    }

    pub fn set_brush_glyph(&mut self, text: &str) {
        match Grapheme::new(text) {
            Ok(g) => {
                self.set_status(StatusKind::Info, format!("Brush glyph: {}", g.as_str()));
                self.paint.brush = g;
                self.paint.kind = BrushKind::Glyph;
                self.switch_to_painting_tool();
            }
            Err(e) => self.report_error(&e),
        }
    }

    /// Selects a semantic brush (line, pattern or stamp).
    pub fn set_brush_kind(&mut self, kind: BrushKind) {
        self.paint.kind = kind;
        self.switch_to_painting_tool();
        let name = self.brush_name();
        let hint = match kind {
            BrushKind::Glyph => "",
            BrushKind::Line(_) => ": draw, lines join where they meet",
            BrushKind::Pattern(_) => ": paint, rectangle or fill",
            BrushKind::Stamp(_) => ": click to place, Rect drag sizes it, n edits the label",
        };
        self.set_status(StatusKind::Info, format!("Brush: {name}{hint}"));
    }

    /// Tools that do not paint with the brush give way to the Brush tool.
    fn switch_to_painting_tool(&mut self) {
        if matches!(self.paint.tool, Tool::Text | Tool::Eraser | Tool::Picker) {
            self.paint.tool = Tool::Brush;
        }
    }

    /// Human-readable brush name: the glyph, or the preset name.
    pub fn brush_name(&self) -> String {
        match self.paint.kind {
            BrushKind::Glyph => self.paint.brush.as_str().to_owned(),
            kind => preset_of(kind).map_or_else(String::new, |p| p.name.to_owned()),
        }
    }

    /// The first glyph of what the brush paints, for swatches.
    pub fn brush_sample(&self) -> String {
        match self.paint.kind {
            BrushKind::Glyph => self.paint.brush.as_str().to_owned(),
            kind => preset_of(kind)
                .and_then(|p| Grapheme::split_text(p.sample).into_iter().next())
                .map_or_else(|| " ".to_owned(), |g| g.as_str().to_owned()),
        }
    }

    pub fn open_label_prompt(&mut self) {
        self.prompt = Some(
            Prompt::new(
                PromptKind::StampLabel,
                "Stamp label (buttons, checkboxes, tabs; a number sets bars)",
                vec![],
            )
            .with_input(self.paint.label.clone()),
        );
    }

    pub fn set_stamp_label(&mut self, label: &str) {
        label.clone_into(&mut self.paint.label);
        self.set_status(StatusKind::Info, format!("Stamp label: {label:?}"));
    }

    pub fn set_color(&mut self, target: ColorTarget, color: Color) {
        match target {
            ColorTarget::Foreground => self.editor.style.fg = color,
            ColorTarget::Background => self.editor.style.bg = color,
        }
        self.remember_color(color);
        self.set_status(
            StatusKind::Info,
            format!("{}: {}", capitalised(target.title()), color_label(color)),
        );
    }

    fn remember_color(&mut self, color: Color) {
        if color == Color::Default {
            return;
        }
        self.paint.recent.retain(|c| *c != color);
        self.paint.recent.insert(0, color);
        self.paint.recent.truncate(RECENT_COLORS);
    }

    pub fn swap_colors(&mut self) {
        let s = &mut self.editor.style;
        std::mem::swap(&mut s.fg, &mut s.bg);
        self.set_status(StatusKind::Info, "Swapped foreground and background");
    }

    pub fn reset_colors(&mut self) {
        self.editor.style.fg = Color::Default;
        self.editor.style.bg = Color::Default;
        self.set_status(StatusKind::Info, "Colours reset to the terminal default");
    }

    // --- colour picker and prompts ------------------------------------------

    pub fn open_color_picker(&mut self, target: ColorTarget) {
        let current = match target {
            ColorTarget::Foreground => self.editor.style.fg,
            ColorTarget::Background => self.editor.style.bg,
        };
        let rows = picker_rows(&self.paint.recent);
        let spectrum = matches!(current, Color::Rgb(..));
        let rows = if spectrum {
            spectrum_rows(&self.paint.recent)
        } else {
            rows
        };
        let (row, col) = rows
            .iter()
            .enumerate()
            .find_map(|(r, cols)| cols.iter().position(|c| *c == current).map(|c| (r, c)))
            .unwrap_or((0, 0));
        self.prompt = None;
        self.picker = Some(ColorPicker {
            target,
            row,
            col,
            spectrum,
        });
    }

    /// The swatch rows of the open picker's grid.
    pub fn picker_grid(&self) -> Vec<Vec<Color>> {
        if self.picker.as_ref().is_some_and(|p| p.spectrum) {
            spectrum_rows(&self.paint.recent)
        } else {
            picker_rows(&self.paint.recent)
        }
    }

    /// Switches the picker between the 256-colour grid and the spectrum.
    pub fn picker_toggle_spectrum(&mut self) {
        if let Some(p) = self.picker.as_mut() {
            p.spectrum = !p.spectrum;
            p.row = 0;
            p.col = 0;
        }
    }

    pub fn picker_move(&mut self, dir: Direction) {
        let rows = self.picker_grid();
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        match dir {
            Direction::Up => p.row = p.row.saturating_sub(1),
            Direction::Down => p.row = (p.row + 1).min(rows.len() - 1),
            Direction::Left => p.col = p.col.saturating_sub(1),
            Direction::Right => p.col += 1,
        }
        p.col = p.col.min(rows[p.row].len() - 1);
    }

    pub fn picker_submit(&mut self) {
        let rows = self.picker_grid();
        let Some(p) = self.picker.take() else {
            return;
        };
        if let Some(color) = rows.get(p.row).and_then(|r| r.get(p.col)).copied() {
            self.set_color(p.target, color);
        }
    }

    pub fn picker_switch_target(&mut self) {
        if let Some(p) = self.picker.as_mut() {
            p.target = p.target.other();
        }
    }

    pub fn open_color_prompt(&mut self, target: ColorTarget) {
        self.picker = None;
        let mut candidates: Vec<String> =
            self.paint.recent.iter().map(ToString::to_string).collect();
        candidates.push("default".to_owned());
        candidates.extend(COLOR_NAMES.iter().map(|n| (*n).to_owned()));
        self.prompt = Some(Prompt::new(
            PromptKind::Color(target),
            format!(
                "{} colour: name, 0-255 or #rrggbb",
                capitalised(target.title())
            ),
            candidates,
        ));
    }

    pub fn submit_color_prompt(&mut self, target: ColorTarget, value: &str) {
        if value.is_empty() {
            return;
        }
        match parse_color(value) {
            Some(color) => self.set_color(target, color),
            None => self.set_status(
                StatusKind::Warning,
                format!("Unknown colour {value:?}: use a name, 0-255 or #rrggbb"),
            ),
        }
    }

    pub fn open_glyph_prompt(&mut self) {
        let candidates = search_candidates()
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        self.prompt = Some(Prompt::new(
            PromptKind::BrushGlyph,
            "Brush: type a glyph, or search (\"double\", \"button\", \"shade\")",
            candidates,
        ));
    }

    /// A single typed grapheme becomes the glyph brush; otherwise the
    /// highlighted candidate is selected.
    pub fn submit_glyph_prompt(&mut self, prompt: &Prompt) {
        let input = prompt.input.trim();
        let typed = Grapheme::split_text(input);
        let choice = if typed.len() == 1 {
            let glyph = typed[0].as_str().to_owned();
            if let Some(page) = BRUSH_PAGES.iter().position(|p| {
                p.entries()
                    .iter()
                    .any(|(_, c)| matches!(c, BrushChoice::Glyph(g) if *g == glyph))
            }) {
                self.paint.page = page;
            }
            return self.set_brush_glyph(&glyph);
        } else if let Some(m) = prompt.selected_match() {
            search_candidates()
                .into_iter()
                .find(|(text, _)| text == m)
                .map(|(_, choice)| choice)
        } else {
            None
        };
        if let Some(choice) = choice {
            if let Some(page) = page_of(choice) {
                self.paint.page = page;
            }
            self.choose_brush(choice);
        }
    }

    pub fn choose_brush(&mut self, choice: BrushChoice) {
        match choice {
            BrushChoice::Glyph(g) => self.set_brush_glyph(g),
            BrushChoice::Kind(kind) => self.set_brush_kind(kind),
        }
    }

    // --- applying tools -------------------------------------------------------

    /// Mouse button pressed on the canvas at `pos`.
    pub fn tool_press(&mut self, pos: Position) {
        if self.on_raster_layer() {
            let (cw, ch) = self.cell_px();
            return self.pixel_press(Position::new(pos.x * cw, pos.y * ch), true);
        }
        self.ui.focus = Focus::Canvas;
        self.set_cursor(pos, None);
        match self.paint.tool {
            Tool::Text => {}
            Tool::Brush if matches!(self.paint.kind, BrushKind::Stamp(_)) => {
                if self.editable_cells_layer() {
                    let writes = self.stamp_writes(pos, None);
                    self.apply_writes(writes, "Stamp");
                }
            }
            Tool::Brush | Tool::Eraser => {
                if !self.editable_cells_layer() {
                    return;
                }
                let label = if self.paint.tool == Tool::Brush {
                    "Paint"
                } else {
                    "Erase"
                };
                if let Err(e) = self.session.begin(label) {
                    self.report_error(&e);
                    return;
                }
                self.paint.stroke = Some(pos);
                self.paint.stroke_moved = false;
                // A line brush waits for the direction of the drag.
                if !self.drawing_lines() {
                    self.record_stroke(&[pos]);
                }
            }
            Tool::Line | Tool::Rect | Tool::Box => {
                if self.editable_cells_layer() {
                    self.paint.anchor = Some(pos);
                }
            }
            Tool::Fill => self.fill_at(pos),
            Tool::Picker => self.pick_at(pos),
            Tool::Gradient => {
                self.set_status(StatusKind::Warning, "Gradients work on pixel layers");
            }
            Tool::Select => self.select_press(pos),
        }
    }

    /// Mouse dragged to `pos` with the button held.
    pub fn tool_drag(&mut self, pos: Position) {
        if self.select_drag(pos) {
            return;
        }
        if let Some(last) = self.paint.stroke {
            if last == pos {
                return;
            }
            self.set_cursor(pos, None);
            let path = if self.drawing_lines() || self.detaching_lines() {
                paint::line4(last, pos)
            } else {
                paint::line(last, pos)
            };
            // The first point was painted by the previous event; repainting
            // it would darken shades twice. Lines need it to join.
            let from = usize::from(!self.drawing_lines());
            self.record_stroke(&path[from..]);
            self.paint.stroke = Some(pos);
            self.paint.stroke_moved = true;
        } else if self.paint.anchor.is_some() {
            self.set_cursor(pos, None);
        }
    }

    /// Mouse button released.
    pub fn tool_release(&mut self) {
        if self.select_release() {
            return;
        }
        if let Some(last) = self.paint.stroke.take() {
            if self.drawing_lines() && !self.paint.stroke_moved {
                self.record_stroke(&[last]);
            }
            self.commit_edits();
        } else if let Some(anchor) = self.paint.anchor.take() {
            if self.paint.tool == Tool::Select {
                self.set_marquee(anchor, self.editor.cursor);
            } else {
                self.commit_shape(anchor, self.editor.cursor);
            }
        }
    }

    /// Whether the Brush tool draws joining lines.
    fn drawing_lines(&self) -> bool {
        self.paint.tool == Tool::Brush && matches!(self.paint.kind, BrushKind::Line(_))
    }

    /// Whether the eraser unjoins neighbouring lines (with a line brush).
    fn detaching_lines(&self) -> bool {
        self.paint.tool == Tool::Eraser && matches!(self.paint.kind, BrushKind::Line(_))
    }

    /// Space: applies the tool at the cursor. Shapes take two presses, the
    /// first sets the anchor.
    pub fn apply_tool(&mut self) {
        if self.on_raster_layer() {
            return self.apply_pixel_tool();
        }
        let cursor = self.editor.cursor;
        match self.paint.tool {
            Tool::Text => self.dispatch(Action::InsertText(" ".to_owned())),
            Tool::Brush | Tool::Eraser => {
                if !self.editable_cells_layer() {
                    return;
                }
                let label = if self.paint.tool == Tool::Eraser {
                    "Erase"
                } else if matches!(self.paint.kind, BrushKind::Stamp(_)) {
                    "Stamp"
                } else {
                    "Paint"
                };
                if let Err(e) = self.session.begin(label) {
                    self.report_error(&e);
                    return;
                }
                self.record_stroke(&[cursor]);
                self.commit_edits();
            }
            Tool::Line | Tool::Rect | Tool::Box => match self.paint.anchor.take() {
                Some(anchor) => self.commit_shape(anchor, cursor),
                None => {
                    if self.editable_cells_layer() {
                        self.paint.anchor = Some(cursor);
                        self.set_status(
                            StatusKind::Info,
                            format!(
                                "{} from {},{}: move the cursor and press Space again (Esc cancels)",
                                self.paint.tool.title(),
                                cursor.x,
                                cursor.y
                            ),
                        );
                    }
                }
            },
            Tool::Fill => self.fill_at(cursor),
            Tool::Picker => self.pick_at(cursor),
            Tool::Gradient => {
                self.set_status(StatusKind::Warning, "Gradients work on pixel layers");
            }
            Tool::Select => {
                if let Some(anchor) = self.paint.anchor.take() {
                    self.set_marquee(anchor, cursor);
                } else {
                    self.paint.marquee = None;
                    self.paint.anchor = Some(cursor);
                    self.set_status(
                        StatusKind::Info,
                        "Select from here: move the cursor and press Space again",
                    );
                }
            }
        }
    }

    fn canvas(&self) -> Option<&glyphforge_core::Canvas> {
        self.active_layer().and_then(|l| l.cells())
    }

    /// Records what the Brush or Eraser tool does along `path` into the open
    /// transaction. Each write is computed against the canvas as the stroke
    /// has left it, so lines join with what the same stroke drew.
    fn record_stroke(&mut self, path: &[Position]) {
        let size = self.doc_size();
        let style = self.editor.style;
        if self.detaching_lines() {
            for &pos in path {
                let Some(canvas) = self.canvas() else { return };
                let mut writes = paint::detach(canvas, pos);
                writes.extend(paint::stamp(&[pos], &Cell::EMPTY, size));
                self.record_writes(writes);
            }
            return;
        }
        let wide = self.sized(path);
        let Some(canvas) = self.canvas() else { return };
        let writes = match (self.paint.tool, self.paint.kind) {
            (Tool::Eraser, _) => paint::stamp(&wide, &Cell::EMPTY, size),
            (_, BrushKind::Glyph) => paint::stamp(&wide, &self.brush_cell(), size),
            (_, BrushKind::Line(line)) => {
                let edges = paint::path_edges(path);
                paint::join_edges(canvas, path, &edges, line, style)
            }
            (_, BrushKind::Pattern(pattern)) => paint::pattern_writes(
                canvas,
                &wide,
                pattern,
                &self.paint.brush,
                style,
                Some(glyphforge_core::Rect::from_size(size)),
            ),
            (_, BrushKind::Stamp(_)) => path
                .first()
                .map(|p| self.stamp_writes(*p, None))
                .unwrap_or_default(),
        };
        self.record_writes(writes);
    }

    /// Cells of the stamp brush with its top-left corner at `origin`.
    fn stamp_writes(
        &self,
        origin: Position,
        size: Option<glyphforge_core::Size>,
    ) -> Vec<CellWrite> {
        let BrushKind::Stamp(kind) = self.paint.kind else {
            return Vec::new();
        };
        let cells = stamp::render(kind, &self.paint.label, self.editor.style, size)
            .into_iter()
            .map(|(dx, dy, cell)| {
                (
                    Position::new(origin.x.saturating_add(dx), origin.y.saturating_add(dy)),
                    cell,
                )
            });
        paint::stamp_each(cells, self.doc_size())
    }

    /// Cells of the shape from `anchor` to `to` for the current tool and
    /// brush, merged with lines already on the canvas where lines join.
    pub fn shape_writes(&self, anchor: Position, to: Position) -> Vec<CellWrite> {
        let size = self.doc_size();
        let style = self.editor.style;
        let Some(canvas) = self.canvas() else {
            return Vec::new();
        };
        let rect = paint::rect_between(anchor, to);
        let glyphs = |positions: &[Position]| match self.paint.kind {
            BrushKind::Pattern(pattern) => {
                paint::pattern_writes(canvas, positions, pattern, &self.paint.brush, style, None)
            }
            _ => paint::stamp(positions, &self.brush_cell(), size),
        };
        match (self.paint.tool, self.paint.kind) {
            (Tool::Line | Tool::Rect, BrushKind::Stamp(_)) => {
                let origin = Position::new(rect.x, rect.y);
                self.stamp_writes(
                    origin,
                    Some(glyphforge_core::Size::new(rect.width, rect.height)),
                )
            }
            (Tool::Line, BrushKind::Line(line)) => {
                let path = paint::elbow(anchor, to);
                paint::join_edges(canvas, &path, &paint::path_edges(&path), line, style)
            }
            (Tool::Rect, BrushKind::Line(line)) => {
                paint::join_edges(canvas, &[], &paint::rect_edges(rect), line, style)
            }
            (Tool::Line, _) => glyphs(&paint::line(anchor, to)),
            (Tool::Rect, _) => glyphs(&paint::rect_outline(rect)),
            (Tool::Box, _) => match line_style_of(self.paint.box_style) {
                Some(line) => paint::join_edges(canvas, &[], &paint::rect_edges(rect), line, style),
                None => paint::box_outline(rect, self.paint.box_style, style)
                    .into_iter()
                    .filter(|w| size.contains(Position::new(w.x, w.y)))
                    .collect(),
            },
            _ => Vec::new(),
        }
    }

    fn commit_shape(&mut self, anchor: Position, to: Position) {
        if !self.editable_cells_layer() {
            return;
        }
        let writes = self.shape_writes(anchor, to);
        let label = format!("Draw {}", self.paint.tool.title().to_ascii_lowercase());
        self.apply_writes(writes, &label);
    }

    fn fill_at(&mut self, pos: Position) {
        if !self.editable_cells_layer() {
            return;
        }
        let Some(canvas) = self.canvas() else {
            return;
        };
        let region = paint::flood_region(canvas, pos);
        let writes = match self.paint.kind {
            BrushKind::Pattern(pattern) => paint::pattern_writes(
                canvas,
                &region,
                pattern,
                &self.paint.brush,
                self.editor.style,
                None,
            ),
            _ => paint::stamp(&region, &self.brush_cell(), self.doc_size()),
        };
        let n = writes.len();
        if self.apply_writes(writes, "Fill") {
            self.set_status(StatusKind::Info, format!("Filled {n} cells"));
        }
    }

    /// Takes glyph and colours from the active layer's cell, or from the
    /// rendered screen where that cell is transparent.
    pub fn pick_at(&mut self, pos: Position) {
        let own = self.cell_at(pos).filter(|c| !c.is_empty()).cloned();
        let cell = own.or_else(|| self.composite().get(pos).cloned());
        let Some(cell) = cell.filter(|c| !c.is_empty()) else {
            self.set_status(StatusKind::Info, "Nothing to pick: the cell is transparent");
            return;
        };
        if let CellContent::Glyph(g) = &cell.content {
            self.paint.brush = g.clone();
            self.paint.kind = BrushKind::Glyph;
        }
        self.editor.style.fg = cell.style.fg;
        self.editor.style.bg = cell.style.bg;
        self.remember_color(cell.style.bg);
        self.remember_color(cell.style.fg);
        if self.paint.tool == Tool::Picker {
            self.paint.tool = match self.paint.previous_tool {
                Tool::Picker => Tool::Brush,
                other => other,
            };
        }
        self.set_status(
            StatusKind::Info,
            format!(
                "Picked {} fg {} bg {}",
                self.paint.brush.as_str(),
                color_label(cell.style.fg),
                color_label(cell.style.bg)
            ),
        );
    }

    /// Records writes into the open stroke transaction.
    fn record_writes(&mut self, cells: Vec<CellWrite>) {
        if cells.is_empty() {
            return;
        }
        let op = Operation::SetCells {
            layer: self.editor.layer.clone(),
            cells,
        };
        match self.session.record(op) {
            Ok(()) => self.mark_edited(),
            Err(e) => self.report_error(&e),
        }
    }

    /// Applies writes as one undoable patch.
    fn apply_writes(&mut self, cells: Vec<CellWrite>, label: &str) -> bool {
        if cells.is_empty() {
            return false;
        }
        let op = Operation::SetCells {
            layer: self.editor.layer.clone(),
            cells,
        };
        self.apply(Patch::single(op), label)
    }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
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

    fn glyph_at(app: &App, x: u16, y: u16) -> String {
        app.cell_at(Position::new(x, y))
            .and_then(Cell::as_glyph)
            .map_or_else(|| ".".to_owned(), |g| g.as_str().to_owned())
    }

    fn row(app: &App, y: u16) -> String {
        (0..app.doc_size().width)
            .map(|x| glyph_at(app, x, y))
            .collect()
    }

    #[test]
    fn colour_input_forms() {
        assert_eq!(parse_color("bright-blue"), Some(Color::Indexed(12)));
        assert_eq!(parse_color("Bright Blue"), Some(Color::Indexed(12)));
        assert_eq!(parse_color("200"), Some(Color::Indexed(200)));
        assert_eq!(parse_color("ansi:3"), Some(Color::Indexed(3)));
        assert_eq!(parse_color("#7aa2f7"), Some(Color::Rgb(0x7a, 0xa2, 0xf7)));
        assert_eq!(parse_color("default"), Some(Color::Default));
        assert_eq!(parse_color("mauve"), None);
        assert_eq!(color_label(Color::Indexed(9)), "bright-red");
        assert_eq!(color_label(Color::Indexed(99)), "ansi:99");
    }

    #[test]
    fn picker_grid_covers_all_256_colours() {
        let rows = picker_rows(&[]);
        let mut all: Vec<Color> = rows.concat();
        assert_eq!(all.len(), 257, "256 indexed + default");
        all.sort_by_key(|c| match c {
            Color::Indexed(i) => i32::from(*i),
            _ => -1,
        });
        all.dedup();
        assert_eq!(all.len(), 257);
        assert_eq!(picker_rows(&[Color::Indexed(1)]).len(), rows.len() + 1);
        assert!(is_light(Color::Indexed(15)) && !is_light(Color::Indexed(0)));
        assert!(is_light(Color::Indexed(231)) && !is_light(Color::Indexed(232)));
    }

    #[test]
    fn brush_stroke_is_one_undo_step_and_connects_gaps() {
        let mut app = app(10, 3);
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(Action::ToolPress(Position::new(0, 1)));
        app.dispatch(Action::ToolDrag(Position::new(4, 1)));
        app.dispatch(Action::ToolDrag(Position::new(6, 1)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(row(&app, 1), "███████...");
        assert_eq!(app.session.history().undo_len(), 1);
        app.dispatch(Action::Undo);
        assert_eq!(row(&app, 1), "..........");
    }

    #[test]
    fn brush_uses_glyph_and_colours() {
        let mut app = app(6, 2);
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(Action::SetBrushGlyph("#".into()));
        app.dispatch(Action::SetColor(ColorTarget::Foreground, Color::Indexed(2)));
        app.dispatch(Action::SetColor(
            ColorTarget::Background,
            Color::Rgb(1, 2, 3),
        ));
        app.dispatch(Action::ApplyTool);
        let cell = app.cell_at(Position::ORIGIN).unwrap();
        assert_eq!(cell.as_glyph().unwrap().as_str(), "#");
        assert_eq!(cell.style.fg, Color::Indexed(2));
        assert_eq!(cell.style.bg, Color::Rgb(1, 2, 3));
        assert_eq!(
            app.paint.recent,
            vec![Color::Rgb(1, 2, 3), Color::Indexed(2)]
        );
        app.dispatch(Action::SwapColors);
        assert_eq!(app.editor.style.fg, Color::Rgb(1, 2, 3));
        app.dispatch(Action::ResetColors);
        assert_eq!(app.editor.style.fg, Color::Default);
    }

    #[test]
    fn eraser_clears() {
        let mut app = app(6, 1);
        app.dispatch(Action::InsertText("abcdef".into()));
        app.dispatch(Action::SelectTool(Tool::Eraser));
        app.dispatch(Action::ToolPress(Position::new(1, 0)));
        app.dispatch(Action::ToolDrag(Position::new(3, 0)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(row(&app, 0), "a...ef");
    }

    #[test]
    fn shapes_by_mouse_and_by_keyboard() {
        let mut app = app(8, 5);
        app.dispatch(Action::SetBrushGlyph("*".into()));
        app.dispatch(Action::SelectTool(Tool::Rect));
        app.dispatch(Action::ToolPress(Position::new(0, 0)));
        app.dispatch(Action::ToolDrag(Position::new(3, 2)));
        assert!(!app.dirty(), "the shape is only a preview while dragging");
        assert_eq!(
            app.shape_writes(app.paint.anchor.unwrap(), app.editor.cursor)
                .len(),
            10
        );
        app.dispatch(Action::ToolRelease);
        assert_eq!(row(&app, 0), "****....");
        assert_eq!(row(&app, 1), "*..*....");

        app.dispatch(Action::SelectTool(Tool::Line));
        app.dispatch(Action::CursorTo(Position::new(0, 4)));
        app.dispatch(Action::ApplyTool);
        assert!(app.paint.anchor.is_some());
        app.dispatch(Action::CursorMove(Direction::Right));
        app.dispatch(Action::CursorMove(Direction::Right));
        app.dispatch(Action::ApplyTool);
        assert_eq!(row(&app, 4), "***.....");

        app.dispatch(Action::SelectTool(Tool::Box));
        app.dispatch(Action::ToolPress(Position::new(5, 0)));
        app.dispatch(Action::ToolDrag(Position::new(7, 2)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(glyph_at(&app, 5, 0), "┌", "single lines by default");
        assert_eq!(glyph_at(&app, 7, 2), "┘");
        app.dispatch(Action::SetBoxStyle(BorderFamily::Double));
        app.dispatch(Action::ToolPress(Position::new(5, 2)));
        app.dispatch(Action::ToolDrag(Position::new(7, 4)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(
            glyph_at(&app, 5, 2),
            "╠",
            "the double box joins the single one"
        );
        assert_eq!(glyph_at(&app, 6, 4), "═");
        app.dispatch(Action::CycleBoxStyle);
        assert_eq!(app.paint.box_style, BorderFamily::Heavy);

        app.dispatch(Action::ApplyTool);
        app.dispatch(Action::Cancel);
        assert!(app.paint.anchor.is_none(), "Esc cancels a started shape");
    }

    #[test]
    fn fill_stops_at_different_cells() {
        let mut app = app(5, 3);
        app.dispatch(Action::SelectTool(Tool::Line));
        app.dispatch(Action::SetBrushGlyph("|".into()));
        app.dispatch(Action::ToolPress(Position::new(2, 0)));
        app.dispatch(Action::ToolDrag(Position::new(2, 2)));
        app.dispatch(Action::ToolRelease);
        app.dispatch(Action::SelectTool(Tool::Fill));
        app.dispatch(Action::SetBrushGlyph("░".into()));
        assert_eq!(
            app.paint.tool,
            Tool::Fill,
            "choosing a glyph keeps a painting tool"
        );
        app.dispatch(Action::ToolPress(Position::new(0, 0)));
        assert_eq!(row(&app, 0), "░░|..");
        assert_eq!(row(&app, 2), "░░|..");
    }

    #[test]
    fn picker_takes_glyph_and_colours_then_returns() {
        let mut app = app(5, 1);
        app.dispatch(Action::SetColor(ColorTarget::Foreground, Color::Indexed(5)));
        app.dispatch(Action::InsertText("Q".into()));
        app.dispatch(Action::ResetColors);
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(Action::SelectTool(Tool::Picker));
        app.dispatch(Action::ToolPress(Position::ORIGIN));
        assert_eq!(app.paint.brush.as_str(), "Q");
        assert_eq!(app.editor.style.fg, Color::Indexed(5));
        assert_eq!(app.paint.tool, Tool::Brush);
        app.dispatch(Action::PickAt(Position::new(4, 0)));
        assert!(app.status.as_ref().unwrap().text.contains("transparent"));
    }

    #[test]
    fn colour_picker_navigates_and_submits() {
        let mut app = app(5, 1);
        app.dispatch(Action::OpenColorPicker(ColorTarget::Background));
        assert_eq!(app.picker.as_ref().map(|p| (p.row, p.col)), Some((0, 0)));
        app.dispatch(Action::PickerMove(Direction::Right));
        app.dispatch(Action::PickerMove(Direction::Right));
        app.dispatch(Action::PickerSubmit);
        assert!(app.picker.is_none());
        assert_eq!(app.editor.style.bg, Color::Indexed(1));
        app.dispatch(Action::OpenColorPicker(ColorTarget::Background));
        assert_eq!(
            app.picker.as_ref().map(|p| (p.row, p.col)),
            Some((0, 2)),
            "opens on the current colour"
        );
        app.dispatch(Action::PickerMove(Direction::Down));
        assert_eq!(app.picker.as_ref().map(|p| (p.row, p.col)), Some((1, 2)));
        app.dispatch(Action::PickerSwitchTarget);
        app.dispatch(Action::PickerSubmit);
        assert_eq!(app.editor.style.fg, Color::Indexed(18));
        app.dispatch(Action::OpenColorPicker(ColorTarget::Foreground));
        app.dispatch(Action::Cancel);
        assert!(app.picker.is_none());
    }

    #[test]
    fn colour_and_glyph_prompts() {
        let mut app = app(5, 1);
        app.dispatch(Action::ColorPrompt(ColorTarget::Foreground));
        app.prompt.as_mut().unwrap().input = "#102030".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.editor.style.fg, Color::Rgb(0x10, 0x20, 0x30));
        app.dispatch(Action::ColorPrompt(ColorTarget::Foreground));
        app.prompt.as_mut().unwrap().input = "nope".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(
            app.status.as_ref().map(|s| s.kind),
            Some(StatusKind::Warning)
        );

        app.dispatch(Action::EditBrushGlyph);
        app.prompt.as_mut().unwrap().input = "double".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.brush_name(), "double line", "first matching candidate");
        assert_eq!(
            BRUSH_PAGES[app.paint.page].name, "Lines ┼",
            "palette follows"
        );
        app.dispatch(Action::EditBrushGlyph);
        app.prompt.as_mut().unwrap().input = "@".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.paint.brush.as_str(), "@");
    }

    #[test]
    fn choosing_a_tool_on_an_interface_layer_switches_to_artwork() {
        let mut app = app(5, 1);
        app.dispatch(Action::LayerNext);
        assert!(!app.on_artwork_layer());
        app.dispatch(Action::SelectTool(Tool::Brush));
        assert!(app.on_artwork_layer());
        assert_eq!(app.screen().layers().len(), 2, "existing layer reused");
    }

    #[test]
    fn a_paint_layer_is_added_when_no_artwork_layer_is_usable() {
        let mut app = app(5, 1);
        let art = app.editor.layer.clone();
        app.session
            .apply_patch(
                Patch::single(Operation::UpdateLayer {
                    target: art,
                    name: None,
                    visible: Some(false),
                    locked: None,
                }),
                "hide",
                glyphforge_core::history::Origin::User,
            )
            .unwrap();
        app.dispatch(Action::LayerNext);
        app.dispatch(Action::SelectTool(Tool::Brush));
        assert!(app.on_artwork_layer());
        assert_eq!(app.editor.layer.as_str(), "paint");
        assert_eq!(app.screen().layers().len(), 3);
        app.dispatch(Action::ApplyTool);
        assert!(app.cell_at(Position::ORIGIN).is_some_and(|c| !c.is_empty()));
    }

    #[test]
    fn brush_pages_wrap() {
        let mut app = app(5, 1);
        app.dispatch(Action::BrushPagePrev);
        assert_eq!(app.paint.page, BRUSH_PAGES.len() - 1);
        app.dispatch(Action::BrushPageNext);
        assert_eq!(app.paint.page, 0);
    }

    fn line(weight: glyphforge_core::boxdraw::Weight) -> BrushKind {
        BrushKind::Line(paint::LineStyle {
            weight,
            rounded: false,
            dashed: false,
        })
    }

    fn drag(app: &mut App, points: &[(u16, u16)]) {
        app.dispatch(Action::ToolPress(Position::new(points[0].0, points[0].1)));
        for &(x, y) in &points[1..] {
            app.dispatch(Action::ToolDrag(Position::new(x, y)));
        }
        app.dispatch(Action::ToolRelease);
    }

    #[test]
    fn line_brush_joins_strokes_and_existing_lines() {
        use glyphforge_core::boxdraw::Weight;
        let mut app = app(7, 5);
        app.dispatch(Action::SetBrushKind(line(Weight::Light)));
        assert_eq!(app.paint.tool, Tool::Brush);
        // An L-shaped stroke and a crossing stroke.
        drag(&mut app, &[(1, 1), (5, 1), (5, 4)]);
        assert_eq!(row(&app, 1), ".────┐.");
        drag(&mut app, &[(3, 0), (3, 3)]);
        assert_eq!(row(&app, 0), "...│...");
        assert_eq!(row(&app, 1), ".──┼─┐.");
        assert_eq!(row(&app, 3), "...│.│.", "a stroke's end stays straight");
        assert_eq!(
            app.session.history().undo_len(),
            2,
            "one undo step per stroke"
        );
        // Diagonal drags step through cell sides.
        drag(&mut app, &[(0, 4), (2, 3)]);
        assert!(row(&app, 4).starts_with('─') || row(&app, 4).starts_with('┘'));
        // A click draws a dash; a double stroke into a light line mixes.
        app.dispatch(Action::SetBrushKind(line(Weight::Double)));
        drag(&mut app, &[(6, 2), (5, 2)]);
        assert_eq!(glyph_at(&app, 5, 2), "╞");
        app.dispatch(Action::Undo);
        app.dispatch(Action::ToolPress(Position::new(0, 0)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(glyph_at(&app, 0, 0), "═");
    }

    #[test]
    fn line_eraser_unjoins_and_shapes_join() {
        use glyphforge_core::boxdraw::Weight;
        let mut app = app(7, 5);
        app.dispatch(Action::SetBrushKind(line(Weight::Light)));
        app.dispatch(Action::SelectTool(Tool::Rect));
        drag(&mut app, &[(0, 0), (4, 2)]);
        app.dispatch(Action::SelectTool(Tool::Line));
        drag(&mut app, &[(2, 0), (2, 4)]);
        assert_eq!(row(&app, 0), "┌─┬─┐..");
        assert_eq!(row(&app, 2), "└─┼─┘..");
        app.dispatch(Action::SelectTool(Tool::Eraser));
        app.dispatch(Action::ToolPress(Position::new(2, 3)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(
            row(&app, 2),
            "└─┴─┘..",
            "erasing below the crossing unjoins it"
        );
        // The Box tool joins in its own style.
        app.dispatch(Action::SetBoxStyle(
            glyphforge_core::boxdraw::BorderFamily::Double,
        ));
        drag(&mut app, &[(4, 1), (6, 4)]);
        assert_eq!(
            glyph_at(&app, 4, 1),
            "╠",
            "double box joins the light frame"
        );
    }

    #[test]
    fn stamps_place_size_and_use_the_label() {
        use glyphforge_core::stamp::{ButtonStyle, StampKind};
        let mut app = app(16, 5);
        app.dispatch(Action::SetBrushKind(BrushKind::Stamp(StampKind::Button(
            ButtonStyle::Bracket,
        ))));
        app.dispatch(Action::ToolPress(Position::new(1, 0)));
        app.dispatch(Action::ToolRelease);
        assert_eq!(row(&app, 0), ".[ OK ].........");
        app.dispatch(Action::EditStampLabel);
        app.prompt.as_mut().unwrap().input = "Save".into();
        app.dispatch(Action::PromptSubmit);
        app.dispatch(Action::SelectTool(Tool::Rect));
        app.dispatch(Action::SetBrushKind(BrushKind::Stamp(StampKind::Button(
            ButtonStyle::Rounded,
        ))));
        assert_eq!(app.paint.tool, Tool::Rect, "shape tools keep sizing stamps");
        drag(&mut app, &[(0, 1), (11, 3)]);
        assert_eq!(row(&app, 1), "╭──────────╮....");
        assert_eq!(row(&app, 2), "│   Save   │....");
        assert_eq!(app.session.history().undo_len(), 2);
        app.dispatch(Action::SetBrushKind(BrushKind::Stamp(StampKind::Checkbox(
            true,
        ))));
        app.dispatch(Action::CursorTo(Position::new(12, 4)));
        app.dispatch(Action::SelectTool(Tool::Brush));
        app.dispatch(Action::ApplyTool);
        assert_eq!(row(&app, 4), "............☑ Sa", "clipped at the edge");
    }

    #[test]
    fn pattern_brushes_paint_fill_and_shade() {
        use glyphforge_core::paint::Pattern;
        let mut app = app(6, 3);
        app.dispatch(Action::SetBrushGlyph("#".into()));
        app.dispatch(Action::SetBrushKind(BrushKind::Pattern(Pattern::Checker)));
        app.dispatch(Action::SelectTool(Tool::Fill));
        app.dispatch(Action::ToolPress(Position::ORIGIN));
        assert_eq!(row(&app, 0), "#.#.#.");
        assert_eq!(row(&app, 1), ".#.#.#");
        app.dispatch(Action::Undo);
        app.dispatch(Action::SetBrushKind(BrushKind::Pattern(Pattern::Shade)));
        app.dispatch(Action::SelectTool(Tool::Brush));
        for _ in 0..3 {
            drag(&mut app, &[(0, 2), (3, 2)]);
        }
        assert_eq!(row(&app, 2), "▓▓▓▓..", "each pass darkens once per cell");
    }

    #[test]
    fn brush_search_selects_presets_and_glyphs() {
        let mut app = app(5, 1);
        app.dispatch(Action::EditBrushGlyph);
        app.prompt.as_mut().unwrap().input = "pill".into();
        app.dispatch(Action::PromptSubmit);
        assert_eq!(app.brush_name(), "pill button");
        assert_eq!(BRUSH_PAGES[app.paint.page].name, "Buttons");
        app.dispatch(Action::SelectTool(Tool::Picker));
        app.dispatch(Action::SetBrushGlyph("x".into()));
        assert_eq!(app.paint.kind, BrushKind::Glyph);
        assert_eq!(app.paint.tool, Tool::Brush);
    }

    #[test]
    fn spectrum_grid_and_hsl() {
        assert_eq!(hsl(0, 100, 50), Color::Rgb(255, 0, 0));
        assert_eq!(hsl(120, 100, 50), Color::Rgb(0, 255, 0));
        assert_eq!(hsl(240, 100, 50), Color::Rgb(0, 0, 255));
        assert_eq!(hsl(0, 0, 100), Color::Rgb(255, 255, 255));
        let rows = spectrum_rows(&[]);
        assert_eq!(rows.len(), 12);
        assert!(rows[1..=10].iter().all(|r| r.len() == 36));
        let mut app = app(4, 2);
        app.dispatch(Action::OpenColorPicker(ColorTarget::Foreground));
        app.dispatch(Action::PickerToggleSpectrum);
        app.dispatch(Action::PickerMove(Direction::Down));
        app.dispatch(Action::PickerSubmit);
        assert!(matches!(app.editor.style.fg, Color::Rgb(..)));
        // Reopening on a truecolour starts in the spectrum.
        app.dispatch(Action::OpenColorPicker(ColorTarget::Foreground));
        assert!(app.picker.as_ref().unwrap().spectrum);
    }

    #[test]
    fn preview_depth_steps_down_and_back() {
        use crate::terminal::ColorDepth;
        let mut app = app(4, 2);
        app.color_depth = ColorDepth::TrueColor;
        app.dispatch(Action::CyclePreviewDepth);
        assert_eq!(app.canvas_depth(), ColorDepth::Ansi256);
        app.dispatch(Action::CyclePreviewDepth);
        assert_eq!(app.canvas_depth(), ColorDepth::Ansi16);
        app.dispatch(Action::CyclePreviewDepth);
        assert_eq!(app.canvas_depth(), ColorDepth::TrueColor);
        app.color_depth = ColorDepth::Ansi16;
        app.dispatch(Action::CyclePreviewDepth);
        assert_eq!(app.preview_depth, None, "nothing lower than 16 colours");
    }
}
