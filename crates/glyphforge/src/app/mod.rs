//! Application state and the single dispatch entry point.

mod arrange;
mod brushes;
mod commands;
mod dispatch;
mod inspector;
mod interface;
mod layers;
mod menu;
mod paint;
mod pixels;
mod prompt;
mod run;
mod selection;
mod viewport;

use glyphforge_core::api::Session;
use glyphforge_core::layout::LayoutResult;
use glyphforge_core::{Canvas, CellStyle, Document, Layer, ObjectId, Position, Size, Theme};
use ratatui::layout::Rect as RatRect;

use crate::config::{Config, ThemeSource};
use crate::input::Keymap;
use crate::omarchy::{self, OmarchyPaths};
use crate::terminal::{ColorDepth, TerminalCapabilities};

#[cfg(test)]
pub use brushes::BRUSH_PAGES;
pub use brushes::{BOX_STYLES, BrushKind, PaletteEntry};
pub use interface::Drag;
pub use layers::{LayerAction, ScreenAction};
pub use menu::{Menu, MenuEntry};
#[cfg(test)]
pub use paint::picker_rows;
pub use paint::{ColorPicker, ColorTarget, PaintState, Tool, color_label, is_light};
pub use prompt::Prompt;
pub use run::{RunError, run};
pub use viewport::Viewport;

/// Which region receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Canvas,
    LeftPanel,
    /// The layer list at the top of the right panel.
    Layers,
    RightPanel,
}

impl Focus {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::LeftPanel => "left panel",
            Self::Layers => "layers",
            Self::RightPanel => "right panel",
        }
    }
}

/// Which editing model the active layer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignMode {
    Interface,
    Subcell,
    Pixel,
}

impl DesignMode {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Interface => "INTERFACE",
            Self::Subcell => "SUBCELL",
            Self::Pixel => "PIXEL",
        }
    }
}

/// Editing mode. Without Vim navigation the editor is always inserting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
}

impl EditorMode {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    pub kind: StatusKind,
    pub text: String,
}

/// UI-only state: what is shown, what is focused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiState {
    pub show_left_panel: bool,
    pub show_right_panel: bool,
    pub help_open: bool,
    pub focus: Focus,
    pub vim_navigation: bool,
    /// Overview of the whole screen in the canvas corner.
    pub minimap: bool,
    /// Highlighted inspector row (Interface Mode, right panel).
    pub inspector_row: usize,
}

/// Editing state that is not part of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorState {
    pub screen: ObjectId,
    pub layer: ObjectId,
    pub cursor: Position,
    pub viewport: Viewport,
    /// Column to return to on `NewLine`; reset when the cursor is moved
    /// explicitly.
    pub line_start_x: u16,
    pub style: CellStyle,
    pub mode: EditorMode,
    /// Selected component (Interface Mode): the primary selection that
    /// the inspector shows.
    pub selection: Option<ObjectId>,
    /// Further selected components (Shift+click); edits apply to all.
    pub also_selected: Vec<ObjectId>,
    /// Size the screen is previewed at (responsive check); the document
    /// keeps its own size.
    pub preview_size: Option<Size>,
    /// Layer visibility saved by Isolate, restored by the next Isolate.
    pub isolation: Option<Vec<(ObjectId, bool)>>,
}

/// Where the UI chrome theme came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiThemeOrigin {
    Builtin,
    Omarchy,
    /// One of the built-in themes, chosen in the menu or the config.
    Named,
}

impl UiThemeOrigin {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Omarchy => "omarchy",
            Self::Named => "theme",
        }
    }
}

#[derive(Debug)]
pub struct App {
    pub session: Session,
    pub editor: EditorState,
    pub ui: UiState,
    /// Theme of the editor chrome (panels, status bar), not of the design.
    pub ui_theme: Theme,
    pub ui_theme_origin: UiThemeOrigin,
    pub config: Config,
    pub caps: TerminalCapabilities,
    pub color_depth: ColorDepth,
    /// Preview the design at a lower colour depth than the terminal's.
    pub preview_depth: Option<ColorDepth>,
    pub keymap: Keymap,
    pub status: Option<StatusMessage>,
    pub omarchy_paths: OmarchyPaths,
    pub is_omarchy: bool,
    pub prompt: Option<Prompt>,
    pub menu: Option<Menu>,
    /// The open command palette.
    pub palette: Option<commands::CommandPalette>,
    /// Titles of recently run palette commands, newest first.
    pub recent_commands: Vec<String>,
    /// The chosen built-in theme id; `None` follows Omarchy or the terminal.
    pub ui_theme_choice: Option<String>,
    /// Where the chosen theme is remembered.
    pub state_dir: Option<std::path::PathBuf>,
    pub picker: Option<ColorPicker>,
    pub drag: Option<Drag>,
    /// What Copy and Cut put aside (cells or components).
    pub clipboard: Option<selection::Clipboard>,
    /// Text waiting to be sent to the system clipboard.
    system_clipboard: Option<String>,
    /// What the entries of an open Find prompt point at.
    find_targets: Vec<(String, layers::FindTarget)>,
    /// Snapping while dragging components.
    pub snap: glyphforge_core::align::Snap,
    /// Guides that explain the current snap (shown while dragging).
    pub guides: Vec<glyphforge_core::align::Guide>,
    /// Subcell Mode tool, brush and colour state.
    pub paint: PaintState,
    /// Cell size in pixels when the terminal reports pixel-precise mouse
    /// positions (mode 1016 is active).
    pub pixel_mouse: Option<(u16, u16)>,
    composite_cache: Option<Canvas>,
    layout_cache: Option<LayoutResult>,
    frame_area: RatRect,
    quit_armed: bool,
    new_armed: bool,
    should_quit: bool,
}

/// Everything `main` needs to hand over to build the application.
#[derive(Debug)]
pub struct AppInit {
    pub session: Session,
    pub config: Config,
    pub caps: TerminalCapabilities,
    pub keymap: Keymap,
    pub omarchy_paths: OmarchyPaths,
    pub startup_warnings: Vec<String>,
    /// XDG state directory, for remembering the chosen theme.
    pub state_dir: Option<std::path::PathBuf>,
}

impl App {
    pub fn new(init: AppInit) -> Self {
        let is_omarchy = init.omarchy_paths.is_omarchy();
        let color_depth = init.caps.effective_color_depth(init.config.ui.color_mode);
        let vim = init.config.ui.vim_navigation;
        let doc = init.session.document();
        let screen = doc.first_screen();
        let layer = Self::default_edit_layer(screen.layers())
            .unwrap_or_else(|| screen.layers()[0].id.clone());
        let mut app = Self {
            editor: EditorState {
                screen: screen.id.clone(),
                layer,
                cursor: Position::ORIGIN,
                viewport: Viewport::default(),
                line_start_x: 0,
                style: CellStyle::DEFAULT,
                mode: if vim {
                    EditorMode::Normal
                } else {
                    EditorMode::Insert
                },
                selection: None,
                also_selected: Vec::new(),
                preview_size: None,
                isolation: None,
            },
            session: init.session,
            ui: UiState {
                show_left_panel: init.config.ui.show_left_panel,
                show_right_panel: init.config.ui.show_right_panel,
                help_open: false,
                focus: Focus::Canvas,
                vim_navigation: vim,
                minimap: false,
                inspector_row: 0,
            },
            ui_theme: Theme::terminal(),
            ui_theme_origin: UiThemeOrigin::Builtin,
            config: init.config,
            caps: init.caps,
            color_depth,
            preview_depth: None,
            keymap: init.keymap,
            status: None,
            omarchy_paths: init.omarchy_paths,
            is_omarchy,
            prompt: None,
            menu: None,
            palette: None,
            recent_commands: Vec::new(),
            ui_theme_choice: None,
            state_dir: init.state_dir,
            picker: None,
            drag: None,
            clipboard: None,
            system_clipboard: None,
            find_targets: Vec::new(),
            snap: glyphforge_core::align::Snap::default(),
            guides: Vec::new(),
            paint: PaintState::default(),
            pixel_mouse: None,
            composite_cache: None,
            layout_cache: None,
            frame_area: RatRect::default(),
            quit_armed: false,
            new_armed: false,
            should_quit: false,
        };
        app.snap.neighbours = app.config.ui.snap;
        app.snap.grid = app.config.ui.snap_grid;
        app.load_ui_theme();
        app.restore_ui_theme();
        if !app.caps.unicode {
            app.set_status(
                StatusKind::Warning,
                "Locale is not UTF-8; Unicode glyphs may render incorrectly (set LANG to a UTF-8 locale)",
            );
        }
        if !app.caps.mouse && app.config.mouse.enabled {
            tracing::info!(
                "terminal {:?} is not expected to report mouse events",
                app.caps.term
            );
        }
        for w in init.startup_warnings {
            tracing::warn!("{w}");
            app.set_status(StatusKind::Warning, w);
        }
        app
    }

    /// The layer to start editing on: the top-most visible artwork layer,
    /// otherwise the top-most visible layer of any kind (a design whose
    /// artwork is only a hidden asset opens in Interface Mode).
    fn default_edit_layer(layers: &[Layer]) -> Option<ObjectId> {
        layers
            .iter()
            .rev()
            .find(|l| l.visible && l.cells().is_some())
            .or_else(|| layers.iter().rev().find(|l| l.visible))
            .map(|l| l.id.clone())
    }

    /// The colour depth the canvas is drawn at: the preview depth when one
    /// is set, never more than the terminal shows.
    pub fn canvas_depth(&self) -> ColorDepth {
        self.preview_depth
            .map_or(self.color_depth, |d| d.min(self.color_depth))
    }

    /// Steps the canvas preview: terminal depth, 256, 16, back.
    pub fn cycle_preview_depth(&mut self) {
        self.preview_depth = match self.preview_depth {
            None if self.color_depth > ColorDepth::Ansi256 => Some(ColorDepth::Ansi256),
            None | Some(ColorDepth::Ansi256) if self.color_depth > ColorDepth::Ansi16 => {
                Some(ColorDepth::Ansi16)
            }
            _ => None,
        };
        let text = match self.preview_depth {
            Some(d) => format!("Previewing the design in {d} (F6 steps on)"),
            None => format!("Design shown in {} (terminal)", self.color_depth),
        };
        self.set_status(StatusKind::Info, text);
    }

    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn doc(&self) -> &Document {
        self.session.document()
    }

    pub fn dirty(&self) -> bool {
        self.session.is_dirty()
    }

    /// The document title shown in the UI.
    pub fn title(&self) -> String {
        self.session.path().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        let text = text.into();
        match kind {
            StatusKind::Info => tracing::info!("{text}"),
            StatusKind::Warning => tracing::warn!("{text}"),
            StatusKind::Error => tracing::error!("{text}"),
        }
        self.status = Some(StatusMessage { kind, text });
    }

    /// Reports a recoverable error in the status bar.
    pub fn report_error(&mut self, err: &dyn std::error::Error) {
        let mut text = err.to_string();
        let mut source = err.source();
        while let Some(s) = source {
            text.push_str(": ");
            text.push_str(&s.to_string());
            source = s.source();
        }
        self.set_status(StatusKind::Error, text);
    }

    /// The active screen.
    pub fn screen(&self) -> &glyphforge_core::Screen {
        // The editor's screen id always refers to an existing screen: it is
        // only ever set from the document.
        self.doc()
            .screen(&self.editor.screen)
            .unwrap_or_else(|| self.doc().first_screen())
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.screen().layer(&self.editor.layer)
    }

    /// The size the screen is shown and laid out at: the preview size
    /// when one is chosen, otherwise the screen's own.
    pub fn doc_size(&self) -> Size {
        self.editor
            .preview_size
            .unwrap_or_else(|| self.screen().size())
    }

    /// Cell under a position on the active layer, if it is an artwork layer.
    pub fn cell_at(&self, pos: Position) -> Option<&glyphforge_core::Cell> {
        self.active_layer()
            .and_then(Layer::cells)
            .and_then(|c| c.get(pos))
    }

    /// The rendered active screen, recomputed only after edits.
    pub fn composite(&mut self) -> &Canvas {
        if self.composite_cache.is_none() {
            let rendered = match self
                .session
                .render(&self.editor.screen, self.editor.preview_size)
            {
                Ok(c) => c,
                Err(e) => {
                    self.report_error(&e);
                    Canvas::new(self.doc_size())
                        .unwrap_or_else(|_| unreachable!("screen sizes are non-zero"))
                }
            };
            self.composite_cache = Some(rendered);
        }
        self.composite_cache
            .as_ref()
            .unwrap_or_else(|| unreachable!("composite cache populated"))
    }

    pub const fn cached_composite(&self) -> Option<&Canvas> {
        self.composite_cache.as_ref()
    }

    pub const fn cached_layout(&self) -> Option<&LayoutResult> {
        self.layout_cache.as_ref()
    }

    pub fn mark_edited(&mut self) {
        self.composite_cache = None;
        self.layout_cache = None;
        self.quit_armed = false;
        self.new_armed = false;
    }

    /// The terminal area of the last frame, for hit-testing.
    pub const fn last_frame_area(&self) -> RatRect {
        self.frame_area
    }

    /// Measures the layout for `area` before drawing: records the visible
    /// canvas size and keeps the cursor in view. Called once per frame.
    pub fn update_layout(&mut self, area: RatRect) {
        self.frame_area = area;
        let layout = crate::ui::layout::compute(area, &self.ui);
        self.editor.viewport.size = Size::new(layout.canvas.width, layout.canvas.height);
        let size = self.doc_size();
        self.editor
            .viewport
            .ensure_visible(self.editor.cursor, size);
        let _ = self.composite();
        let _ = self.layout();
    }

    /// (Re)loads the UI chrome theme according to the configured source.
    pub fn load_ui_theme(&mut self) {
        let want_omarchy = match self.config.theme.source {
            ThemeSource::Builtin => false,
            ThemeSource::Omarchy => true,
            ThemeSource::Auto => self.is_omarchy,
        };
        if !want_omarchy {
            self.ui_theme = Theme::terminal();
            self.ui_theme_origin = UiThemeOrigin::Builtin;
            return;
        }
        match omarchy::load_theme(&self.omarchy_paths) {
            Ok(theme) => {
                tracing::info!("loaded Omarchy theme {:?}", theme.name);
                self.ui_theme = theme;
                self.ui_theme_origin = UiThemeOrigin::Omarchy;
            }
            Err(e) => {
                self.ui_theme = Theme::terminal();
                self.ui_theme_origin = UiThemeOrigin::Builtin;
                let forced = self.config.theme.source == ThemeSource::Omarchy;
                let msg = format!("Omarchy theme unavailable, using built-in theme: {e}");
                if forced {
                    self.set_status(StatusKind::Warning, msg);
                } else {
                    tracing::warn!("{msg}");
                }
            }
        }
    }

    /// Replaces the session (new or opened document) and resets the editor.
    pub fn replace_session(&mut self, session: Session) {
        self.session = session;
        let screen = self.doc().first_screen();
        let screen_id = screen.id.clone();
        let layer = Self::default_edit_layer(screen.layers())
            .unwrap_or_else(|| screen.layers()[0].id.clone());
        self.editor.screen = screen_id;
        self.editor.layer = layer;
        self.editor.cursor = Position::ORIGIN;
        self.editor.line_start_x = 0;
        self.editor.viewport.offset = Position::ORIGIN;
        self.editor.selection = None;
        self.editor.also_selected.clear();
        self.editor.preview_size = None;
        self.editor.isolation = None;
        self.paint.anchor = None;
        self.paint.stroke = None;
        self.paint.pixel_anchor = None;
        self.paint.pixel_stroke = None;
        self.paint.marquee = None;
        self.paint.moving = None;
        self.mark_edited();
    }

    /// Where key bindings are looked up for the current state.
    pub fn key_context(&self) -> crate::actions::Context {
        use crate::actions::Context;
        if self.on_raster_layer() {
            Context::Paint
        } else if !self.on_artwork_layer() {
            Context::Interface
        } else if self.paint.tool == Tool::Text {
            Context::Global
        } else {
            Context::Paint
        }
    }
}

#[cfg(test)]
pub(crate) fn test_app(size: Size) -> App {
    use std::path::PathBuf;
    App::new(AppInit {
        session: Session::new(Document::new(size).unwrap()),
        config: Config::default(),
        caps: TerminalCapabilities::infer(|_| None, false),
        keymap: Keymap::defaults(),
        omarchy_paths: OmarchyPaths {
            install_dir: PathBuf::from("/nonexistent"),
            config_dir: PathBuf::from("/nonexistent"),
        },
        startup_warnings: Vec::new(),
        state_dir: None,
    })
}
