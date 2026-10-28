//! The central action model. Every user-visible behaviour is an `Action`;
//! keyboard, mouse, menus and the command palette all dispatch through it.

use std::sync::OnceLock;

use glyphforge_core::align::{Axis, Edge};
use glyphforge_core::boxdraw::BorderFamily;
use glyphforge_core::raster::{Dither, GradientShape, GradientStyle, PaletteLimit, RasterMode};
use glyphforge_core::{Color, ObjectId, Position};

use crate::app::{BrushKind, ColorTarget, Tool};

/// Direction for cursor and viewport movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Which page of the Esc menu is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuPage {
    Main,
    Theme,
}

/// Esc menu actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuAction {
    Open,
    Close,
    Page(MenuPage),
    Move(Direction),
    Select,
    /// Chooses the entry at an index (a mouse click).
    Click(usize),
    /// A built-in theme id, or `system` to follow Omarchy or the terminal.
    SetTheme(String),
}

/// Cell attributes the brush can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attr {
    Bold,
    Italic,
    Underline,
    Strike,
    Dim,
    Reverse,
}

impl Attr {
    /// The ones shown in the palette, with their marks.
    pub const PALETTE: [(Self, &'static str); 6] = [
        (Self::Bold, "B"),
        (Self::Italic, "I"),
        (Self::Underline, "U"),
        (Self::Strike, "S"),
        (Self::Dim, "D"),
        (Self::Reverse, "R"),
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::Strike => "strikethrough",
            Self::Dim => "dim",
            Self::Reverse => "reverse",
        }
    }

    /// The flag in `attrs` this attribute controls.
    pub fn flag(self, attrs: &mut glyphforge_core::Attributes) -> &mut bool {
        match self {
            Self::Bold => &mut attrs.bold,
            Self::Italic => &mut attrs.italic,
            Self::Underline => &mut attrs.underline,
            Self::Strike => &mut attrs.strikethrough,
            Self::Dim => &mut attrs.dim,
            Self::Reverse => &mut attrs.reverse,
        }
    }
}

/// Inspector actions (right panel in Interface Mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorAction {
    /// Highlights the next (`true`) or previous row.
    Move(bool),
    /// Toggles or advances the highlighted value (`false`: backwards).
    Cycle(bool),
    Edit,
    /// Resets the highlighted value to its default.
    Clear,
    /// Highlights a row (mouse); a click on the highlighted row edits it.
    Click(usize),
}

/// Pixel Mode actions (raster layers). Positions are in pixels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PixelAction {
    NewLayer,
    Settings,
    CycleMode,
    CycleDither,
    ToggleShadow,
    SizeUp,
    SizeDown,
    Cursor(Direction),
    EditGradient,
    CycleGradientStyle,
    /// Mouse press at a pixel; `true` when the mouse is only cell-precise.
    Press(Position, bool),
    Drag(Position),
    Release,
    Pick(Position),
    SetMode(RasterMode),
    SetDither(Dither),
    SetPalette(PaletteLimit),
    /// Moves the shadow further out (positive) or in.
    ShadowOffset(i16),
    ShadowSoftness(u8),
    ShadowOpacity(u8),
    ShadowColorFromBrush,
    SelectGradient(ObjectId),
    GradientStyle(GradientStyle),
    GradientShape(GradientShape),
}

/// Where a key binding applies. Interface bindings win over global ones
/// while the active layer holds components; paint bindings win while a
/// painting tool (anything but Text) is active on an artwork layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    Global,
    Interface,
    Paint,
}

impl Context {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Interface => "interface",
            Self::Paint => "paint",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Global, Self::Interface, Self::Paint]
            .into_iter()
            .find(|c| c.name() == s)
    }
}

/// Application actions. Variants without UI-geometry payloads are
/// "bindable" and have an [`ActionDescriptor`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    // Interface Mode
    SelectNext,
    SelectPrev,
    SelectById,
    SelectAt(Position),
    MoveSelection(Direction),
    ResizeSelection(Direction),
    AddComponent,
    EditProperty,
    /// Adds the component at a position to the selection or removes it.
    ToggleSelectAt(Position),
    SelectAll,
    SelectParent,
    SelectFirstChild,
    /// Prompt for a new parent of the selection.
    Reparent,
    Inspector(InspectorAction),
    /// Aligns the selection on an edge of its bounds (one component: of
    /// its parent).
    Align(Edge),
    Distribute(Axis),
    /// Gives the selection the primary's width and/or height.
    EqualSize {
        width: bool,
        height: bool,
    },
    ToggleSnap,
    CycleSnapGrid,
    /// Toggles a text attribute of the brush style.
    ToggleAttr(Attr),
    // Paint and colour (Subcell Mode)
    SelectTool(Tool),
    ApplyTool,
    EditBrushGlyph,
    OpenColorPicker(ColorTarget),
    SwapColors,
    ResetColors,
    BrushPageNext,
    BrushPagePrev,
    CycleBoxStyle,
    EditStampLabel,
    // Paint input from the mouse, palette and picker (internal, not bindable)
    ToolPress(Position),
    ToolDrag(Position),
    ToolRelease,
    PickAt(Position),
    SetBrushGlyph(String),
    SetColor(ColorTarget, Color),
    SetBoxStyle(BorderFamily),
    SetBrushKind(BrushKind),
    Pixel(PixelAction),
    Menu(MenuAction),
    ColorPrompt(ColorTarget),
    PickerMove(Direction),
    PickerSubmit,
    PickerSwitchTarget,
    PickerToggleSpectrum,
    // Prompt (internal, not bindable)
    PromptInput(char),
    PromptBackspace,
    PromptNext,
    PromptPrev,
    PromptComplete,
    PromptSubmit,
    PromptCancel,
    // Application
    Quit,
    ToggleHelp,
    Cancel,
    ReloadTheme,
    UseUiThemeInDocument,
    // View
    ToggleLeftPanel,
    ToggleRightPanel,
    TogglePanels,
    /// Steps the canvas through lower colour depths.
    CyclePreviewDepth,
    FocusNext,
    FocusPrev,
    ScrollView(Direction),
    LayerNext,
    LayerPrev,
    // Cursor
    CursorMove(Direction),
    CursorLineStart,
    CursorLineEnd,
    CursorPageUp,
    CursorPageDown,
    CursorTo(Position),
    // Editing (Milestone 1 minimum; routed through history from M5)
    InsertText(String),
    NewLine,
    Backspace,
    DeleteForward,
    // File and history
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    Undo,
    Redo,
    // Not implemented yet: dispatching reports so in the status bar. They
    // exist so default bindings and the help overlay stay stable.
    Copy,
    Cut,
    Paste,
    Duplicate,
    DeleteSelection,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Application,
    File,
    Edit,
    View,
    Cursor,
    Interface,
    Paint,
}

impl Category {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Cursor => "Cursor",
            Self::Interface => "Interface Mode",
            Self::Paint => "Paint and Colour",
        }
    }
}

/// Metadata for a bindable action: its stable config name, title, search
/// keywords and aliases (command palette), category and default keys.
#[derive(Debug, Clone)]
pub struct ActionDescriptor {
    pub name: &'static str,
    pub title: &'static str,
    pub category: Category,
    pub context: Context,
    #[allow(
        dead_code,
        reason = "consumed by the command palette fuzzy matcher (Milestone 9)"
    )]
    pub keywords: &'static [&'static str],
    #[allow(
        dead_code,
        reason = "consumed by the command palette fuzzy matcher (Milestone 9)"
    )]
    pub aliases: &'static [&'static str],
    /// Key chords; a `paint:`/`interface:`/`global:` prefix binds the chord
    /// in that context instead of the descriptor's own.
    pub default_keys: &'static [&'static str],
    pub action: Action,
    /// Whether dispatching does something in the current milestone.
    pub implemented: bool,
}

macro_rules! desc {
    ($name:literal, $title:literal, $cat:ident, $action:expr, keys: [$($k:literal),*], kw: [$($kw:literal),*], aliases: [$($al:literal),*], implemented: $impl:literal) => {
        desc!($name, $title, $cat, Global, $action, keys: [$($k),*], kw: [$($kw),*], aliases: [$($al),*], implemented: $impl)
    };
    ($name:literal, $title:literal, $cat:ident, $ctx:ident, $action:expr, keys: [$($k:literal),*], kw: [$($kw:literal),*], aliases: [$($al:literal),*], implemented: $impl:literal) => {
        ActionDescriptor {
            name: $name,
            title: $title,
            category: Category::$cat,
            context: Context::$ctx,
            keywords: &[$($kw),*],
            aliases: &[$($al),*],
            default_keys: &[$($k),*],
            action: $action,
            implemented: $impl,
        }
    };
}

/// All bindable actions in a stable order.
pub fn descriptors() -> &'static [ActionDescriptor] {
    static TABLE: OnceLock<Vec<ActionDescriptor>> = OnceLock::new();
    TABLE.get_or_init(|| {
        vec![
            desc!("quit", "Quit", Application, Action::Quit, keys: ["ctrl+q"], kw: ["exit", "close"], aliases: ["q"], implemented: true),
            desc!("toggle-help", "Toggle Help", Application, Action::ToggleHelp, keys: ["f1"], kw: ["keys", "shortcuts", "bindings"], aliases: ["help"], implemented: true),
            desc!("cancel", "Cancel", Application, Action::Cancel, keys: ["esc"], kw: ["escape", "abort"], aliases: [], implemented: true),
            desc!("reload-theme", "Reload UI Theme", Application, Action::ReloadTheme, keys: ["ctrl+shift+t"], kw: ["omarchy", "colors", "palette"], aliases: [], implemented: true),
            desc!("use-ui-theme", "Use The Editor Theme In The Document", Application, Action::UseUiThemeInDocument, keys: ["ctrl+shift+o"], kw: ["omarchy", "theme", "colors", "document", "design"], aliases: [], implemented: true),
            desc!("menu", "Menu", Application, Action::Menu(MenuAction::Open), keys: ["f10"], kw: ["esc", "options", "theme", "quit"], aliases: ["options"], implemented: true),
            desc!("command-palette", "Command Palette", Application, Action::CommandPalette, keys: ["ctrl+space", "ctrl+p"], kw: ["search", "actions", "run"], aliases: ["palette"], implemented: false),
            desc!("new-document", "New Document", File, Action::NewDocument, keys: ["ctrl+n"], kw: ["create", "canvas"], aliases: ["new"], implemented: true),
            desc!("open-document", "Open Document", File, Action::OpenDocument, keys: ["ctrl+o"], kw: ["load", "file"], aliases: ["open"], implemented: false),
            desc!("save-document", "Save Document", File, Action::SaveDocument, keys: ["ctrl+s"], kw: ["write", "file"], aliases: ["save"], implemented: true),
            desc!("save-document-as", "Save Document As", File, Action::SaveDocumentAs, keys: ["ctrl+shift+s"], kw: ["write", "file", "rename"], aliases: ["save as"], implemented: true),
            desc!("undo", "Undo", Edit, Action::Undo, keys: ["ctrl+z"], kw: ["history", "revert"], aliases: [], implemented: true),
            desc!("redo", "Redo", Edit, Action::Redo, keys: ["ctrl+y", "ctrl+shift+z"], kw: ["history", "repeat"], aliases: [], implemented: true),
            desc!("copy", "Copy", Edit, Action::Copy, keys: ["ctrl+c"], kw: ["clipboard", "selection"], aliases: [], implemented: true),
            desc!("cut", "Cut", Edit, Action::Cut, keys: ["ctrl+x"], kw: ["clipboard", "selection"], aliases: [], implemented: true),
            desc!("paste", "Paste", Edit, Action::Paste, keys: ["ctrl+v"], kw: ["clipboard", "insert"], aliases: [], implemented: true),
            desc!("duplicate", "Duplicate", Edit, Interface, Action::Duplicate, keys: ["ctrl+d"], kw: ["copy", "clone", "repeat"], aliases: [], implemented: true),
            desc!("delete-selection", "Delete Selection", Edit, Action::DeleteSelection, keys: ["ctrl+delete"], kw: ["clear", "remove"], aliases: [], implemented: true),
            desc!("select-next", "Select Next Component", Interface, Interface, Action::SelectNext, keys: ["]"], kw: ["component", "cycle"], aliases: [], implemented: true),
            desc!("select-prev", "Select Previous Component", Interface, Interface, Action::SelectPrev, keys: ["["], kw: ["component", "cycle"], aliases: [], implemented: true),
            desc!("select-by-id", "Select Component By Id", Interface, Global, Action::SelectById, keys: ["ctrl+f"], kw: ["find", "jump", "goto"], aliases: ["find"], implemented: true),
            desc!("add-component", "Add Component", Interface, Interface, Action::AddComponent, keys: ["a"], kw: ["create", "insert", "panel", "label", "button"], aliases: ["new component"], implemented: true),
            desc!("edit-property", "Edit Property", Interface, Interface, Action::EditProperty, keys: ["enter"], kw: ["set", "title", "text", "inspector"], aliases: [], implemented: true),
            desc!("select-all", "Select All", Interface, Interface, Action::SelectAll, keys: ["ctrl+a", "paint:ctrl+a"], kw: ["multi", "selection", "everything"], aliases: [], implemented: true),
            desc!("select-parent", "Select Parent", Interface, Interface, Action::SelectParent, keys: ["alt+up", "u"], kw: ["container", "up", "tree"], aliases: ["parent"], implemented: true),
            desc!("select-child", "Select First Child", Interface, Interface, Action::SelectFirstChild, keys: ["alt+down", "d"], kw: ["children", "down", "tree"], aliases: ["child"], implemented: true),
            desc!("reparent", "Move Into Container", Interface, Interface, Action::Reparent, keys: ["p"], kw: ["reparent", "parent", "nest", "group", "container", "tree"], aliases: ["move into"], implemented: true),
            desc!("inspector-edit", "Edit Inspector Field", Interface, Interface, Action::Inspector(InspectorAction::Edit), keys: ["i"], kw: ["inspector", "property", "layout", "padding", "gap"], aliases: ["inspect"], implemented: true),
            desc!("align-left", "Align Left", Interface, Interface, Action::Align(Edge::Left), keys: ["shift+h"], kw: ["alignment", "edge", "arrange"], aliases: [], implemented: true),
            desc!("align-right", "Align Right", Interface, Interface, Action::Align(Edge::Right), keys: ["shift+l"], kw: ["alignment", "edge", "arrange"], aliases: [], implemented: true),
            desc!("align-top", "Align Top", Interface, Interface, Action::Align(Edge::Top), keys: ["shift+k"], kw: ["alignment", "edge", "arrange"], aliases: [], implemented: true),
            desc!("align-bottom", "Align Bottom", Interface, Interface, Action::Align(Edge::Bottom), keys: ["shift+j"], kw: ["alignment", "edge", "arrange"], aliases: [], implemented: true),
            desc!("align-center", "Align Centres Horizontally", Interface, Interface, Action::Align(Edge::Center), keys: ["shift+c"], kw: ["alignment", "centre", "center", "arrange"], aliases: [], implemented: true),
            desc!("align-middle", "Align Middles Vertically", Interface, Interface, Action::Align(Edge::Middle), keys: ["shift+m"], kw: ["alignment", "middle", "centre", "arrange"], aliases: [], implemented: true),
            desc!("distribute-horizontal", "Distribute Horizontally", Interface, Interface, Action::Distribute(Axis::Horizontal), keys: ["shift+d"], kw: ["spacing", "even", "gaps", "arrange"], aliases: [], implemented: true),
            desc!("distribute-vertical", "Distribute Vertically", Interface, Interface, Action::Distribute(Axis::Vertical), keys: ["shift+v"], kw: ["spacing", "even", "gaps", "arrange"], aliases: [], implemented: true),
            desc!("equal-width", "Equal Width", Interface, Interface, Action::EqualSize { width: true, height: false }, keys: ["shift+w"], kw: ["size", "match", "same"], aliases: [], implemented: true),
            desc!("equal-height", "Equal Height", Interface, Interface, Action::EqualSize { width: false, height: true }, keys: ["shift+e"], kw: ["size", "match", "same"], aliases: [], implemented: true),
            desc!("equal-size", "Equal Size", Interface, Interface, Action::EqualSize { width: true, height: true }, keys: ["shift+z"], kw: ["size", "match", "same"], aliases: [], implemented: true),
            desc!("toggle-snap", "Toggle Snapping", Interface, Interface, Action::ToggleSnap, keys: ["shift+s"], kw: ["snap", "guides", "magnet"], aliases: ["snap"], implemented: true),
            desc!("cycle-snap-grid", "Cycle Snap Grid", Interface, Interface, Action::CycleSnapGrid, keys: ["shift+g"], kw: ["snap", "grid", "spacing"], aliases: ["grid"], implemented: true),
            desc!("delete-component", "Delete Component", Interface, Interface, Action::DeleteSelection, keys: ["delete", "backspace"], kw: ["remove"], aliases: [], implemented: true),
            desc!("move-selection-up", "Move Selection Up", Interface, Interface, Action::MoveSelection(Direction::Up), keys: ["up"], kw: ["nudge"], aliases: [], implemented: true),
            desc!("move-selection-down", "Move Selection Down", Interface, Interface, Action::MoveSelection(Direction::Down), keys: ["down"], kw: ["nudge"], aliases: [], implemented: true),
            desc!("move-selection-left", "Move Selection Left", Interface, Interface, Action::MoveSelection(Direction::Left), keys: ["left"], kw: ["nudge"], aliases: [], implemented: true),
            desc!("move-selection-right", "Move Selection Right", Interface, Interface, Action::MoveSelection(Direction::Right), keys: ["right"], kw: ["nudge"], aliases: [], implemented: true),
            desc!("grow-right", "Resize Selection Wider", Interface, Interface, Action::ResizeSelection(Direction::Right), keys: ["shift+right"], kw: ["width", "resize"], aliases: [], implemented: true),
            desc!("shrink-left", "Resize Selection Narrower", Interface, Interface, Action::ResizeSelection(Direction::Left), keys: ["shift+left"], kw: ["width", "resize"], aliases: [], implemented: true),
            desc!("grow-down", "Resize Selection Taller", Interface, Interface, Action::ResizeSelection(Direction::Down), keys: ["shift+down"], kw: ["height", "resize"], aliases: [], implemented: true),
            desc!("shrink-up", "Resize Selection Shorter", Interface, Interface, Action::ResizeSelection(Direction::Up), keys: ["shift+up"], kw: ["height", "resize"], aliases: [], implemented: true),
            desc!("tool-text", "Text Tool", Paint, Action::SelectTool(Tool::Text), keys: ["alt+t", "paint:t"], kw: ["type", "insert", "tool", "palette"], aliases: ["text"], implemented: true),
            desc!("tool-brush", "Brush Tool", Paint, Action::SelectTool(Tool::Brush), keys: ["alt+b", "paint:b"], kw: ["paint", "pencil", "draw", "tool", "palette"], aliases: ["brush", "pencil"], implemented: true),
            desc!("tool-eraser", "Eraser Tool", Paint, Action::SelectTool(Tool::Eraser), keys: ["alt+e", "paint:e"], kw: ["erase", "clear", "tool", "palette"], aliases: ["eraser"], implemented: true),
            desc!("tool-line", "Line Tool", Paint, Action::SelectTool(Tool::Line), keys: ["alt+l", "paint:l"], kw: ["draw", "shape", "tool", "palette"], aliases: ["line"], implemented: true),
            desc!("tool-rect", "Rectangle Tool", Paint, Action::SelectTool(Tool::Rect), keys: ["alt+r", "paint:r"], kw: ["draw", "shape", "outline", "tool", "palette"], aliases: ["rect"], implemented: true),
            desc!("tool-box", "Box Tool", Paint, Action::SelectTool(Tool::Box), keys: ["alt+x", "paint:x"], kw: ["draw", "frame", "border", "box drawing", "tool", "palette"], aliases: ["box"], implemented: true),
            desc!("tool-fill", "Fill Tool", Paint, Action::SelectTool(Tool::Fill), keys: ["alt+f", "paint:f"], kw: ["bucket", "flood", "tool", "palette"], aliases: ["fill"], implemented: true),
            desc!("tool-picker", "Picker Tool", Paint, Action::SelectTool(Tool::Picker), keys: ["alt+i", "paint:i"], kw: ["eyedropper", "sample", "colour", "tool", "palette"], aliases: ["pick"], implemented: true),
            desc!("tool-select", "Select Tool", Paint, Action::SelectTool(Tool::Select), keys: ["alt+v", "paint:v"], kw: ["marquee", "rectangle", "selection", "move", "copy", "tool", "palette"], aliases: ["select"], implemented: true),
            desc!("tool-gradient", "Gradient Tool", Paint, Action::SelectTool(Tool::Gradient), keys: ["alt+a", "paint:a"], kw: ["gradient", "fill", "pixel", "tool", "palette"], aliases: ["gradient"], implemented: true),
            desc!("new-pixel-layer", "New Pixel Layer", Paint, Action::Pixel(PixelAction::NewLayer), keys: ["alt+p"], kw: ["raster", "sub-cell", "sextant", "braille", "layer"], aliases: ["pixels"], implemented: true),
            desc!("pixel-settings", "Pixel Layer Settings", Paint, Action::Pixel(PixelAction::Settings), keys: ["alt+o", "paint:o"], kw: ["raster", "mode", "dither", "palette", "shadow"], aliases: [], implemented: true),
            desc!("cycle-pixel-mode", "Cycle Pixel Mode", Paint, Action::Pixel(PixelAction::CycleMode), keys: ["alt+m", "paint:m"], kw: ["half", "quadrant", "sextant", "braille", "resolution"], aliases: [], implemented: true),
            desc!("cycle-dither", "Cycle Dithering", Paint, Action::Pixel(PixelAction::CycleDither), keys: ["alt+shift+d", "paint:shift+d"], kw: ["dither", "ordered", "shade", "bayer"], aliases: [], implemented: true),
            desc!("toggle-shadow", "Toggle Drop Shadow", Paint, Action::Pixel(PixelAction::ToggleShadow), keys: ["alt+shift+s", "paint:shift+s"], kw: ["shadow", "effect", "raster"], aliases: [], implemented: true),
            desc!("pixel-size-up", "Larger Brush", Paint, Action::Pixel(PixelAction::SizeUp), keys: ["alt+=", "paint:="], kw: ["brush", "size", "pixel", "bigger"], aliases: [], implemented: true),
            desc!("pixel-size-down", "Smaller Brush", Paint, Action::Pixel(PixelAction::SizeDown), keys: ["alt+-", "paint:-"], kw: ["brush", "size", "pixel", "smaller"], aliases: [], implemented: true),
            desc!("toggle-bold", "Toggle Bold", Paint, Action::ToggleAttr(Attr::Bold), keys: ["alt+shift+b"], kw: ["attribute", "style", "text", "weight"], aliases: ["bold"], implemented: true),
            desc!("toggle-italic", "Toggle Italic", Paint, Action::ToggleAttr(Attr::Italic), keys: ["alt+shift+i"], kw: ["attribute", "style", "text", "slant"], aliases: ["italic"], implemented: true),
            desc!("toggle-underline", "Toggle Underline", Paint, Action::ToggleAttr(Attr::Underline), keys: ["alt+shift+u"], kw: ["attribute", "style", "text"], aliases: ["underline"], implemented: true),
            desc!("toggle-strikethrough", "Toggle Strikethrough", Paint, Action::ToggleAttr(Attr::Strike), keys: ["alt+shift+k"], kw: ["attribute", "style", "text", "strike"], aliases: ["strike"], implemented: true),
            desc!("toggle-dim", "Toggle Dim", Paint, Action::ToggleAttr(Attr::Dim), keys: [], kw: ["attribute", "style", "faint"], aliases: ["dim"], implemented: true),
            desc!("toggle-reverse", "Toggle Reverse", Paint, Action::ToggleAttr(Attr::Reverse), keys: [], kw: ["attribute", "style", "inverse", "invert"], aliases: ["reverse"], implemented: true),
            desc!("pixel-cursor-up", "Pixel Cursor Up", Paint, Paint, Action::Pixel(PixelAction::Cursor(Direction::Up)), keys: ["shift+up"], kw: ["sub-cell", "move"], aliases: [], implemented: true),
            desc!("pixel-cursor-down", "Pixel Cursor Down", Paint, Paint, Action::Pixel(PixelAction::Cursor(Direction::Down)), keys: ["shift+down"], kw: ["sub-cell", "move"], aliases: [], implemented: true),
            desc!("pixel-cursor-left", "Pixel Cursor Left", Paint, Paint, Action::Pixel(PixelAction::Cursor(Direction::Left)), keys: ["shift+left"], kw: ["sub-cell", "move"], aliases: [], implemented: true),
            desc!("pixel-cursor-right", "Pixel Cursor Right", Paint, Paint, Action::Pixel(PixelAction::Cursor(Direction::Right)), keys: ["shift+right"], kw: ["sub-cell", "move"], aliases: [], implemented: true),
            desc!("edit-gradient", "Edit Gradient", Paint, Action::Pixel(PixelAction::EditGradient), keys: ["alt+shift+g", "paint:shift+g"], kw: ["gradient", "stops", "colours"], aliases: [], implemented: true),
            desc!("cycle-gradient-style", "Cycle Gradient Style", Paint, Action::Pixel(PixelAction::CycleGradientStyle), keys: ["alt+shift+a", "paint:shift+a"], kw: ["gradient", "hard", "dither", "shade", "smooth"], aliases: [], implemented: true),
            desc!("apply-tool", "Apply Tool At Cursor", Paint, Paint, Action::ApplyTool, keys: ["space"], kw: ["paint", "draw", "stamp"], aliases: [], implemented: true),
            desc!("brush-glyph", "Search Brushes", Paint, Action::EditBrushGlyph, keys: ["alt+g", "paint:g"], kw: ["character", "symbol", "glyph", "line", "pattern", "button", "stamp"], aliases: ["glyph", "brush"], implemented: true),
            desc!("stamp-label", "Edit Stamp Label", Paint, Action::EditStampLabel, keys: ["alt+n", "paint:n"], kw: ["button", "text", "caption", "stamp"], aliases: ["label"], implemented: true),
            desc!("brush-page-next", "Next Brush Page", Paint, Action::BrushPageNext, keys: ["alt+.", "paint:]"], kw: ["glyphs", "palette", "page", "characters"], aliases: [], implemented: true),
            desc!("brush-page-prev", "Previous Brush Page", Paint, Action::BrushPagePrev, keys: ["alt+,", "paint:["], kw: ["glyphs", "palette", "page", "characters"], aliases: [], implemented: true),
            desc!("cycle-box-style", "Cycle Box Style", Paint, Action::CycleBoxStyle, keys: ["alt+shift+x", "paint:shift+x"], kw: ["single", "double", "rounded", "heavy", "ascii", "lines", "border"], aliases: ["box style"], implemented: true),
            desc!("foreground-color", "Choose Foreground Colour", Paint, Action::OpenColorPicker(ColorTarget::Foreground), keys: ["alt+c", "paint:c"], kw: ["colour", "color", "fg", "picker", "palette"], aliases: ["fg"], implemented: true),
            desc!("background-color", "Choose Background Colour", Paint, Action::OpenColorPicker(ColorTarget::Background), keys: ["alt+shift+c", "paint:shift+c"], kw: ["colour", "color", "bg", "picker", "palette"], aliases: ["bg"], implemented: true),
            desc!("swap-colors", "Swap Colours", Paint, Action::SwapColors, keys: ["alt+s", "paint:s"], kw: ["colour", "color", "fg", "bg", "exchange"], aliases: [], implemented: true),
            desc!("reset-colors", "Reset Colours", Paint, Action::ResetColors, keys: ["alt+d", "paint:d"], kw: ["colour", "color", "default"], aliases: [], implemented: true),
            desc!("backspace", "Erase Left", Edit, Action::Backspace, keys: ["backspace"], kw: ["delete", "erase"], aliases: [], implemented: true),
            desc!("delete-forward", "Erase At Cursor", Edit, Action::DeleteForward, keys: ["delete"], kw: ["delete", "erase", "clear"], aliases: [], implemented: true),
            desc!("new-line", "New Line", Edit, Action::NewLine, keys: ["enter"], kw: ["return", "next row"], aliases: [], implemented: true),
            desc!("toggle-left-panel", "Toggle Left Panel", View, Action::ToggleLeftPanel, keys: ["f2"], kw: ["tools", "sidebar", "hide", "show"], aliases: [], implemented: true),
            desc!("toggle-right-panel", "Toggle Right Panel", View, Action::ToggleRightPanel, keys: ["f3"], kw: ["layers", "properties", "sidebar", "hide", "show"], aliases: [], implemented: true),
            desc!("toggle-panels", "Toggle All Panels", View, Action::TogglePanels, keys: ["f4"], kw: ["zen", "focus", "fullscreen", "hide", "show"], aliases: ["zen mode"], implemented: true),
            desc!("preview-depth", "Preview Colour Depth", View, Action::CyclePreviewDepth, keys: ["f6"], kw: ["256", "16", "ansi", "colours", "degrade", "truecolor"], aliases: [], implemented: true),
            desc!("focus-next", "Focus Next Region", View, Action::FocusNext, keys: ["tab"], kw: ["panel", "cycle"], aliases: [], implemented: true),
            desc!("focus-prev", "Focus Previous Region", View, Action::FocusPrev, keys: ["shift+tab"], kw: ["panel", "cycle"], aliases: [], implemented: true),
            desc!("layer-next", "Next Layer", View, Action::LayerNext, keys: ["ctrl+pageup"], kw: ["layers", "up", "activate"], aliases: [], implemented: true),
            desc!("layer-prev", "Previous Layer", View, Action::LayerPrev, keys: ["ctrl+pagedown"], kw: ["layers", "down", "activate"], aliases: [], implemented: true),
            desc!("scroll-up", "Scroll View Up", View, Action::ScrollView(Direction::Up), keys: ["ctrl+up"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-down", "Scroll View Down", View, Action::ScrollView(Direction::Down), keys: ["ctrl+down"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-left", "Scroll View Left", View, Action::ScrollView(Direction::Left), keys: ["ctrl+left"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("scroll-right", "Scroll View Right", View, Action::ScrollView(Direction::Right), keys: ["ctrl+right"], kw: ["viewport"], aliases: [], implemented: true),
            desc!("cursor-up", "Cursor Up", Cursor, Action::CursorMove(Direction::Up), keys: ["up"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-down", "Cursor Down", Cursor, Action::CursorMove(Direction::Down), keys: ["down"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-left", "Cursor Left", Cursor, Action::CursorMove(Direction::Left), keys: ["left"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-right", "Cursor Right", Cursor, Action::CursorMove(Direction::Right), keys: ["right"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-line-start", "Cursor To Line Start", Cursor, Action::CursorLineStart, keys: ["home"], kw: ["move", "begin"], aliases: [], implemented: true),
            desc!("cursor-line-end", "Cursor To Line End", Cursor, Action::CursorLineEnd, keys: ["end"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-page-up", "Cursor Page Up", Cursor, Action::CursorPageUp, keys: ["pageup"], kw: ["move"], aliases: [], implemented: true),
            desc!("cursor-page-down", "Cursor Page Down", Cursor, Action::CursorPageDown, keys: ["pagedown"], kw: ["move"], aliases: [], implemented: true),
        ]
    })
}

/// Finds a descriptor by its stable name.
pub fn find_descriptor(name: &str) -> Option<&'static ActionDescriptor> {
    descriptors().iter().find(|d| d.name == name)
}

/// The descriptor of a bindable action, if it has one.
pub fn descriptor_of(action: &Action) -> Option<&'static ActionDescriptor> {
    descriptors().iter().find(|d| d.action == *action)
}

/// Vim-style navigation chords, applied on top of the keymap when enabled.
pub fn vim_navigation(key: &crate::input::Key) -> Option<Action> {
    use crate::input::KeyCode;
    if !key.mods.is_none() {
        return None;
    }
    match key.code {
        KeyCode::Char('h') => Some(Action::CursorMove(Direction::Left)),
        KeyCode::Char('j') => Some(Action::CursorMove(Direction::Down)),
        KeyCode::Char('k') => Some(Action::CursorMove(Direction::Up)),
        KeyCode::Char('l') => Some(Action::CursorMove(Direction::Right)),
        KeyCode::Char('0') => Some(Action::CursorLineStart),
        KeyCode::Char('$') => Some(Action::CursorLineEnd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_kebab_case() {
        let mut seen = HashSet::new();
        for d in descriptors() {
            assert!(seen.insert(d.name), "duplicate action name {}", d.name);
            assert!(
                d.name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{} is not kebab-case",
                d.name
            );
        }
    }

    #[test]
    fn actions_are_unique_per_context() {
        let mut seen = HashSet::new();
        for d in descriptors() {
            assert!(
                seen.insert((d.action.clone(), d.context)),
                "duplicate action {:?}",
                d.action
            );
        }
    }

    #[test]
    fn default_keys_parse() {
        for d in descriptors() {
            for k in d.default_keys {
                assert!(
                    crate::input::keymap::parse_binding(k, d.context).is_ok(),
                    "{} has bad key {k}",
                    d.name
                );
            }
        }
    }

    #[test]
    fn find_and_reverse_lookup_agree() {
        let d = find_descriptor("toggle-panels").unwrap();
        assert_eq!(
            descriptor_of(&d.action).map(|x| x.name),
            Some("toggle-panels")
        );
        assert!(find_descriptor("nope").is_none());
        assert!(descriptor_of(&Action::CursorTo(Position::ORIGIN)).is_none());
    }
}
