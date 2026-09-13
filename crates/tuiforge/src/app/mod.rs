//! Application state and the single dispatch entry point.

mod dispatch;
mod run;
mod viewport;

use std::path::PathBuf;

use ratatui::layout::Rect as RatRect;
use tuiforge_core::{CellBuffer, CellStyle, Document, Position, Size, composite};

use crate::config::{Config, ThemeSource};
use crate::input::Keymap;
use crate::omarchy::{self, OmarchyPaths};
use crate::terminal::{ColorDepth, TerminalCapabilities};
use crate::theme::UiTheme;

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
    pub cursor: Position,
    pub viewport: Viewport,
    /// Column to return to on `NewLine`; reset when the cursor is moved
    /// explicitly.
    pub line_start_x: u16,
    pub style: CellStyle,
    pub mode: EditorMode,
}

#[derive(Debug)]
pub struct App {
    pub doc: Document,
    pub doc_path: Option<PathBuf>,
    pub dirty: bool,
    pub editor: EditorState,
    pub ui: UiState,
    pub theme: UiTheme,
    pub config: Config,
    pub caps: TerminalCapabilities,
    pub color_depth: ColorDepth,
    pub keymap: Keymap,
    pub status: Option<StatusMessage>,
    pub omarchy_paths: OmarchyPaths,
    pub is_omarchy: bool,
    composite_cache: Option<CellBuffer>,
    frame_area: RatRect,
    quit_armed: bool,
    should_quit: bool,
}

/// Everything `main` needs to hand over to build the application.
#[derive(Debug)]
pub struct AppInit {
    pub doc: Document,
    pub doc_path: Option<PathBuf>,
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
        let mut app = Self {
            doc: init.doc,
            doc_path: init.doc_path,
            dirty: false,
            editor: EditorState {
                cursor: Position::ORIGIN,
                viewport: Viewport::default(),
                line_start_x: 0,
                style: CellStyle::DEFAULT,
                mode: if vim {
                    EditorMode::Normal
                } else {
                    EditorMode::Insert
                },
            },
            ui: UiState {
                show_left_panel: init.config.ui.show_left_panel,
                show_right_panel: init.config.ui.show_right_panel,
                help_open: false,
                focus: Focus::Canvas,
                vim_navigation: vim,
            },
            theme: UiTheme::builtin(),
            config: init.config,
            caps: init.caps,
            color_depth,
            keymap: init.keymap,
            status: None,
            omarchy_paths: init.omarchy_paths,
            is_omarchy,
            composite_cache: None,
            frame_area: RatRect::default(),
            quit_armed: false,
            should_quit: false,
        };
        app.load_theme();
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

    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// The document title shown in the UI.
    pub fn title(&self) -> String {
        self.doc_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(
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

    /// The composited document, recomputed only after edits.
    pub fn composite(&mut self) -> &CellBuffer {
        if self.composite_cache.is_none() {
            self.composite_cache = Some(composite(&self.doc));
        }
        // Just populated above.
        self.composite_cache
            .as_ref()
            .unwrap_or_else(|| unreachable!("composite cache populated"))
    }

    /// The cached composite, if it is warm. Rendering uses this and falls
    /// back to compositing on the fly when the cache is cold.
    pub const fn cached_composite(&self) -> Option<&CellBuffer> {
        self.composite_cache.as_ref()
    }

    pub fn mark_edited(&mut self) {
        self.composite_cache = None;
        self.dirty = true;
        self.quit_armed = false;
    }

    pub fn doc_size(&self) -> Size {
        self.doc.size()
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
    }

    /// (Re)loads the UI theme according to the configured source.
    pub fn load_theme(&mut self) {
        let want_omarchy = match self.config.theme.source {
            ThemeSource::Builtin => false,
            ThemeSource::Omarchy => true,
            ThemeSource::Auto => self.is_omarchy,
        };
        if !want_omarchy {
            self.theme = UiTheme::builtin();
            return;
        }
        match omarchy::load_theme(&self.omarchy_paths) {
            Ok(theme) => {
                tracing::info!("loaded Omarchy theme {:?}", theme.name);
                self.theme = theme;
            }
            Err(e) => {
                self.theme = UiTheme::builtin();
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
}

#[cfg(test)]
pub(crate) fn test_app(size: Size) -> App {
    use std::path::PathBuf;
    App::new(AppInit {
        doc: Document::new(size).unwrap(),
        doc_path: None,
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
