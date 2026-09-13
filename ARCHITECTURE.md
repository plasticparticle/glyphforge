# TUIForge Architecture

TUIForge is a full-screen terminal application (TUI) for visually designing
terminal user interfaces and ANSI/Unicode artwork. This document describes the
architecture that every milestone builds on. It is a living document: when a
milestone changes a boundary, the corresponding section is updated in the same
change.

Target: Linux, primarily Omarchy (Arch + Hyprland), usable on any modern
terminal. Implementation language: Rust (edition 2024).

---

## 1. Crate layout and the core/presentation boundary

The repository is a Cargo workspace with two crates:

| Crate | Role | Allowed dependencies |
|-------|------|----------------------|
| `crates/tuiforge-core` | Domain model: cells, canvas, layers, selection, compositing, history commands, import/export codecs, project format. | `serde`, `uuid`, `unicode-*`, `thiserror`. **Never** `ratatui` or `crossterm`. |
| `crates/tuiforge` | The application: terminal lifecycle, input, actions, tools, UI widgets, rendering, config, theme, Omarchy integration, storage. | Everything in core plus `ratatui`, `crossterm`, `clap`, `toml`, `tracing`. |

The split is enforced by the manifests, not by convention: `tuiforge-core`
cannot compile against Ratatui, so the document model cannot leak
presentation types. Ratatui is the presentation layer; the domain model owns
its own `Color`, `Cell`, `Grapheme` and buffer types, and the `render` module
in the application crate converts them.

Module map of the application crate (`crates/tuiforge/src`):

```
main.rs        CLI parsing, logging setup, error reporting, process exit code
cli.rs         clap definition
app/           App state, the run loop, action dispatch, error surfacing
actions/       Action enum, action descriptors (names, keywords, aliases)
input/         Key model, key-chord parsing, keymap (config -> Action), crossterm conversion
terminal/      RAII terminal guard, panic hook, capability detection
render/        core Color/Cell -> ratatui Style/Cell, CellBuffer -> ratatui Buffer
ui/            Layout and widgets: canvas view, side panels, status bar, later menus/modals
theme/         UI theme roles (background, accent, ...) and the built-in theme
omarchy/       Omarchy detection and colors.toml -> UiTheme mapping
config/        TOML config model with defaults, XDG path resolution
tools/         (M2+) Tool trait and implementations
commands/      (M9) command palette and fuzzy matching
storage/       (M10) load/save/autosave/recovery
```

Module map of the core crate (`crates/tuiforge-core/src`):

```
document/grapheme.rs   Grapheme: validated single grapheme cluster with terminal width
document/color.rs      Color: Default | Indexed(u8) | Rgb; quantisation helpers
document/cell.rs       Cell, CellContent, CellStyle, Attributes
document/canvas.rs     Canvas: dense cell grid with wide-glyph invariants
document/layer.rs      Layer: id, name, visibility, lock, opacity metadata, canvas
document/document.rs   Document: size, layers (bottom to top), active layer, metadata
document/position.rs   Position, Size, Rect
compose.rs             Compositor: visible layers -> CellBuffer, wide-glyph repair
history/               (M5) reversible commands and transactions
selection/             (M4) rectangular selections
boxdraw/               (M6) box-drawing topology and glyph selection
export/, import/       (M11) codecs independent of the document model
project/               (M10) versioned file envelope and migrations
art/                   (M12) sub-cell raster -> block/Braille conversion
```

Guidance, not law: modules are split by domain responsibility, never by file
size.

---

## 2. Domain model

### 2.1 Grapheme

A `Grapheme` is an immutable, validated string that holds **exactly one
extended grapheme cluster** (per Unicode UAX #29, via `unicode-segmentation`)
whose display width is 1 or 2 terminal columns (per `unicode-width`).

Rejected at construction: empty strings, more than one cluster, control
characters, clusters with width 0 (a lone combining mark cannot occupy a
cell by itself; a base character with combining marks attached is a single
width-1 cluster and is accepted), and clusters wider than 2.

Width uses the non-CJK ("narrow") interpretation of East Asian Ambiguous
characters. Box-drawing glyphs are Ambiguous; in CJK-locale terminals that
render them double width the editor will be off. This is a documented
limitation; a `cjk_ambiguous_wide` setting is planned (see ROADMAP).

### 2.2 Color and style

```rust
enum Color { Default, Indexed(u8), Rgb(u8, u8, u8) }
```

`Indexed(0..=15)` are the ANSI 16 colours, `Indexed(16..=255)` the 256-colour
extension. `Default` means "the terminal's default", which is distinct from
any concrete colour and survives export (it becomes `SGR 39`/`49`).

`Attributes` is a set of booleans: bold, dim, italic, underline, blink,
reverse, strikethrough. `CellStyle` bundles foreground, background and
attributes.

Colours serialise as short strings (`"default"`, `"ansi:12"`, `"#7aa2f7"`) so
project files stay readable.

### 2.3 Cell and canvas

```rust
enum CellContent { Empty, Glyph(Grapheme), WideTail }
struct Cell { content: CellContent, style: CellStyle }
```

- `Empty` is **transparent**: the compositor looks through it to lower
  layers. A painted background with no character is `Glyph(" ")` with a
  background colour, which is opaque.
- `WideTail` is the right half of a double-width glyph. It is never written
  by callers; the canvas maintains it.

`Canvas` is a dense `Vec<Cell>` in row-major order with a fixed width and
height. Every mutation goes through `Canvas::put`, which maintains the wide
glyph invariant:

1. A width-2 glyph at `(x, y)` implies `WideTail` at `(x+1, y)`.
2. A `WideTail` at `(x, y)` implies a width-2 glyph at `(x-1, y)`.
3. A width-2 glyph is never placed in the last column; the write is rejected
   with `CanvasError::WideGlyphAtEdge`.

Overwriting either half of a wide glyph blanks the other half. `put` returns
the list of cells it changed (position and previous value), which is exactly
the delta the history system stores.

`Vec<String>` is not used anywhere for canvas data.

### 2.4 Layers and document

A `Layer` has a UUID, name, `visible`, `locked`, `opacity` (metadata only for
now, 1.0 by default, kept for future formats) and its own `Canvas`. All
layers of a document share the document's size.

`Document` holds `layers` ordered bottom to top, the index of the active
layer, the size, a palette, and metadata (title, timestamps). Layer
operations (M3) are methods on `Document`, each returning the information
needed to reverse it.

### 2.5 Compositing

`compose::composite(&Document) -> CellBuffer` walks layers top-down per cell
and takes the first non-`Empty` content. Afterwards a repair pass fixes
half-covered wide glyphs: a `WideTail` whose head was covered by a higher
layer, or a head whose tail was covered, is replaced by a blank glyph that
keeps its style. The output buffer therefore always satisfies the same
invariants as a canvas and can be rendered or exported without further
checks. The pass is a pure function and is unit tested.

Colour-mode preview (ANSI 16/256/TrueColor) is applied after compositing by
mapping each `Color` through a quantiser; the document is never mutated for
previews.

---

## 3. Application state and event loop

The application is a single-threaded, explicit state machine:

```
loop {
    if needs_redraw { terminal.draw(|frame| ui::render(&app, frame)) }
    match poll(tick) {
        Event -> input::translate(event) -> Vec<Action> -> app.dispatch(action)
        Tick  -> app.tick()   // autosave timer, theme mtime poll, blink
    }
    if app.should_quit { break }
}
```

`App` owns the `Document`, the editor state (cursor, viewport, active tool,
selection, clipboard), the UI state (panel visibility, focus, open modal), the
`UiTheme`, the `Config`, the detected `TerminalCapabilities`, and a status
line with an optional error.

Rendering is pull-based: `ui::render` reads `&App` and never mutates it.
Redraws happen only after an event or tick that changed state; the composite
buffer is cached and invalidated by document edits.

### 3.1 Errors

Domain errors are `thiserror` enums (`CanvasError`, `GraphemeError`, later
`ProjectError`, `CodecError`). The application converts recoverable errors
into `StatusMessage::Error` shown in the status bar and logged via `tracing`;
the terminal session is never torn down for a recoverable error. Unrecoverable
errors (e.g. the terminal cannot enter raw mode) propagate to `main`, which
restores the terminal, prints the error chain to stderr and exits non-zero.

`unwrap`/`expect` are clippy-denied in production code (allowed in tests).

### 3.2 Terminal lifecycle

`terminal::TerminalGuard` enables raw mode, enters the alternate screen,
enables mouse capture, bracketed paste and focus-change events, and pushes
kitty keyboard enhancement flags when the terminal reports support. `Drop`
undoes all of it in reverse order. A panic hook restores the terminal before
the default hook prints the panic, so a bug never leaves the user's shell in
raw mode.

### 3.3 Capability detection

`terminal::caps` determines at startup:

- colour depth (`COLORTERM=truecolor|24bit` -> TrueColor; `TERM` containing
  `256color` -> 256; else 16; `NO_COLOR` respected),
- keyboard enhancement support (crossterm's kitty-protocol query),
- Unicode support (`LANG`/`LC_ALL`/`LC_CTYPE` mention UTF-8),
- mouse support (assumed unless `TERM=linux`/`dumb`).

The result is data, not behaviour: renderers and exporters read it and
degrade (e.g. quantise theme colours to 256).

---

## 4. Actions

Every user-visible behaviour is an `Action` variant. Keyboard shortcuts,
mouse gestures, menus and the command palette all produce `Action`s and feed
the single `App::dispatch`. UI handlers never implement behaviour.

`actions::ActionDescriptor` attaches metadata to bindable actions: stable
kebab-case `name` (used in config), human `title`, `keywords` and `aliases`
(used by the command palette's fuzzy matcher), and a `category`. Actions with
payloads that come from UI geometry (e.g. `CursorTo(Position)`) are not
bindable and have no descriptor.

Keymaps map `KeyChord` (key + modifiers, parsed from strings like
`"ctrl+shift+s"`) to action names. The built-in defaults are a keymap like
any other; the user's `[keys]` config table overrides entries by name.

Ctrl+Space: with the kitty keyboard protocol it arrives unambiguously; on
legacy terminals it arrives as NUL, which crossterm reports as
`Ctrl+Space` too. Where a terminal swallows it, the configurable fallback
(default `Ctrl+P`) opens the palette. The action itself is the same.

---

## 5. Input

`input::Key` is TUIForge's own key model (code + modifiers), converted from
crossterm events in one place. Key *release* and *repeat* events are ignored
unless a tool opts in. Paste events become `Action::PasteText`. Mouse events
are converted to `input::MouseEvent` with canvas-relative coordinates by the
UI layer, which knows the layout; the canvas view then asks the active tool
what to do.

Focus is explicit: `UiState::focus` names the focused region (canvas, left
panel, right panel, modal). Tab/Shift+Tab cycle it; keys are routed to the
focused region first, then to the global keymap.

---

## 6. Tools (M2+)

```rust
trait Tool {
    fn id(&self) -> ToolId;
    fn on_key(&mut self, ctx: &mut ToolContext, key: Key) -> ToolResponse;
    fn on_mouse(&mut self, ctx: &mut ToolContext, ev: MouseEvent) -> ToolResponse;
    fn overlay(&self, ctx: &ToolContext) -> Vec<OverlayCell>;  // preview, not committed
}
```

Tools are registered in a `ToolRegistry` (id -> boxed tool + descriptor);
adding a tool means adding a file and one registration line, not editing a
central match. Tools mutate the document only through `ToolContext`, which
records changes into the open history transaction, so every tool gets
undo/redo for free. A drag from press to release is one transaction.

Line and box tools do not write glyphs directly; they write *topology*
(which edges of a cell are connected, and in which style) and the
`boxdraw` module resolves topology to glyphs, merging with existing
box-drawing cells when smart connectivity is on.

---

## 7. History (M5)

`history::Command` is a reversible unit: `apply(&mut Document)` and
`revert(&mut Document)`. Commands are compact deltas (`CellsChanged { layer,
cells: Vec<(Position, before, after)> }`, `LayerAdded`, `LayerRemoved`,
`LayersReordered`, ...). A `Transaction` groups commands under one label and
is what undo/redo operates on. The document is never cloned or serialised
per edit. The stack has a configurable depth (default 1000 transactions) and
tracks the saved position for dirty state.

---

## 8. Rendering

`render` converts `tuiforge_core::CellBuffer` into the Ratatui frame buffer
for a rectangular viewport:

- visible region = viewport offset + area size, clipped to the document;
- each core cell maps to one Ratatui cell; a width-2 glyph writes its symbol
  into the head cell and resets the tail cell, matching Ratatui's own
  wide-character convention so its diff algorithm skips the tail;
- `Color` -> `ratatui::style::Color` is one function, degraded by the
  detected colour depth;
- overlays (cursor, selection, tool previews, guides) are drawn after the
  composite, in the UI layer.

The canvas view owns scrolling: the cursor is always kept inside the visible
region; the viewport moves in whole cells.

---

## 9. Serialization (M10)

Project files (`.tuiforge`) are JSON with a top-level envelope:

```json
{ "format": "tuiforge", "schema_version": 1, "document": { ... } }
```

Loading reads `schema_version` first and runs a chain of migrations
(`v1 -> v2 -> ...`) before deserialising into the current model. Writing
always emits the current version. An unversioned file is rejected. Autosave
writes to a sibling `*.autosave` and recovery never overwrites the original
without the user's confirmation.

---

## 10. Import and export (M11)

Codecs live in `tuiforge_core::export` / `import` and operate on
`CellBuffer` (export) or produce a `Canvas` (import). They never touch the
application. ANSI export tracks the current SGR state and emits only the
deltas, ending with `ESC[0m` and a newline so `cat` leaves the terminal
clean. ANSI import is an SGR interpreter plus a cursor that handles `\n`,
`\r` and the common cursor-forward sequence; it is not a terminal emulator.

---

## 11. Theme and Omarchy integration

`theme::UiTheme` is a set of semantic roles: `background`, `panel_background`,
`foreground`, `muted`, `accent`, `selection`, `error`, `warning`, `success`,
`info`, `border`, `cursor`. All UI widgets use roles, never literal colours.

The built-in theme uses only `Color::Default` and ANSI 16 indices, so it
adapts to any terminal palette without configuration.

`omarchy` uses only public, stable Omarchy interfaces observed in the Omarchy
repository (`bin/omarchy-theme-set`, `bin/omarchy-theme-current`,
`bin/omarchy-hook`):

- detection: `$OMARCHY_PATH` is set, or `$XDG_DATA_HOME/omarchy` (default
  `~/.local/share/omarchy`) exists, or `~/.config/omarchy/current/theme.name`
  exists;
- current theme name: `~/.config/omarchy/current/theme.name`;
- colours: `~/.config/omarchy/current/theme/colors.toml` with keys `accent`,
  `cursor`, `foreground`, `background`, `selection_foreground`,
  `selection_background`, `color0..color15` (Omarchy generates this file
  from `alacritty.toml` for themes that do not ship one);
- role mapping: background/foreground/accent/cursor/selection direct;
  error = color1, success = color2, warning = color3, info = color4,
  muted = color8, panel background = background mixed 10 % toward
  foreground, border = muted. No theme is hard-coded.
- change notification: the theme is reloaded on terminal focus gain and via
  the `reload-theme` action; M14 adds an mtime poll of `theme.name` on the
  tick and documents an optional `~/.config/omarchy/hooks/theme-set.d/`
  hook that signals running instances. Omarchy system files are never
  modified.

Outside Omarchy, or when the config sets `theme.source = "builtin"`, the
built-in theme is used. Nothing else depends on Omarchy.

---

## 12. Configuration and XDG

`config::paths` resolves `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`
and `XDG_CACHE_HOME` with the specification's defaults relative to `$HOME`.
TUIForge uses:

- `$XDG_CONFIG_HOME/tuiforge/config.toml` — configuration,
- `$XDG_DATA_HOME/tuiforge/` — user palettes, favourites, examples,
- `$XDG_STATE_HOME/tuiforge/` — log file, recovery files, recent documents,
- `$XDG_CACHE_HOME/tuiforge/` — glyph availability caches.

The config file is TOML; every field has a default and a missing file is
not an error. A malformed file is reported in the status bar and the
defaults are used, so a typo never locks the user out of the editor.

---

## 13. Testing strategy

- Core: unit tests next to each module (grapheme width, wide-glyph
  invariants, compositing and repair, clipping, later box topology, flood
  fill, history, codecs, migrations).
- Application: pure functions (key-chord parsing, keymap resolution,
  capability inference, colors.toml mapping, viewport math) are unit
  tested; UI widgets are rendered into Ratatui's `TestBackend` and compared
  as text.
- A pty smoke test (scripted outside cargo) starts the binary, sends keys
  and checks that it exits and restores the terminal.

---

## 14. Performance notes

The composite `CellBuffer` is cached in `App` and recomputed only when a
document edit invalidates it (later: per-row dirty flags). Rendering touches
only the viewport. Canvases are dense vectors because terminal documents are
small (a 400x200 canvas is 80k cells, ~4 MB) and dense storage gives
predictable O(1) access; sparse structures are not worth their complexity
here. Measure before optimising further.
