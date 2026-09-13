//! Application state and the single dispatch entry point.

mod dispatch;
mod interface;
mod prompt;
mod run;
mod viewport;

use glyphforge_core::api::Session;
use glyphforge_core::layout::LayoutResult;
use glyphforge_core::{Canvas, CellStyle, Document, Layer, ObjectId, Position, Size, Theme};
use ratatui::layout::Rect as RatRect;

use crate::config::{Config, ThemeSource};
use crate::input::Keymap;
use crate::omarchy::{self, OmarchyPaths};
use crate::terminal::{ColorDepth, TerminalCapabilities};

pub use interface::Drag;
pub use prompt::Prompt;
pub use run::{RunError, run};
pub use viewport::Viewport;

/// Which region receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Canvas,
    LeftPanel,
    RightPanel,
}

impl Focus {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::LeftPanel => "left panel",
            Self::RightPanel => "right panel",
        }
    }
}

/// Which editing model the active layer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignMode {
    Interface,
    Subcell,
}

impl DesignMode {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Interface => "INTERFACE",
            Self::Subcell => "SUBCELL",
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
    /// Selected components (Interface Mode), in selection order.
    pub selection: Vec<ObjectId>,
}

/// Where the UI chrome theme came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiThemeOrigin {
    Builtin,
    Omarchy,
}

impl UiThemeOrigin {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Omarchy => "omarchy",
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
    pub keymap: Keymap,
    pub status: Option<StatusMessage>,
    pub omarchy_paths: OmarchyPaths,
    pub is_omarchy: bool,
    pub prompt: Option<Prompt>,
    pub drag: Option<Drag>,
    /// Whether dragging snaps to neighbouring components.
    pub snap_enabled: bool,
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
}

impl App {
    pub fn new(init: AppInit) -> Self {
        let is_omarchy = init.omarchy_paths.is_omarchy();
        let color_depth = init.caps.effective_color_depth(init.config.ui.color_mode);
        let vim = init.config.ui.vim_navigation;
        let snap = init.config.ui.snap;
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
                selection: Vec::new(),
            },
            session: init.session,
            ui: UiState {
                show_left_panel: init.config.ui.show_left_panel,
                show_right_panel: init.config.ui.show_right_panel,
                help_open: false,
                focus: Focus::Canvas,
                vim_navigation: vim,
            },
            ui_theme: Theme::terminal(),
            ui_theme_origin: UiThemeOrigin::Builtin,
            config: init.config,
            caps: init.caps,
            color_depth,
            keymap: init.keymap,
            status: None,
            omarchy_paths: init.omarchy_paths,
            is_omarchy,
            prompt: None,
            drag: None,
            snap_enabled: snap,
            composite_cache: None,
            layout_cache: None,
            frame_area: RatRect::default(),
            quit_armed: false,
            new_armed: false,
            should_quit: false,
        };
        app.load_ui_theme();
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

    pub fn doc_size(&self) -> Size {
        self.screen().size()
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
            let rendered = match self.session.render(&self.editor.screen, None) {
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
        self.prune_selection();
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
        self.editor.selection.clear();
        self.mark_edited();
    }
}

/// Cell delta of a direction, shared by cursor movement and nudging.
pub(super) const fn delta(dir: crate::actions::Direction) -> (i32, i32) {
    use crate::actions::Direction;
    match dir {
        Direction::Up => (0, -1),
        Direction::Down => (0, 1),
        Direction::Left => (-1, 0),
        Direction::Right => (1, 0),
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
    })
}
