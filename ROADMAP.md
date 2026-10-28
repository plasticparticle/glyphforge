# Glyphforge Roadmap

Development proceeds in small, independently testable milestones. Every
milestone runs the same pipeline before it counts as done:

1. implement, 2. write tests, 3. `cargo test --workspace`,
4. `cargo clippy --workspace --all-targets -- -D warnings`,
5. `cargo fmt --all --check`, 6. fix everything, 7. update
`ARCHITECTURE.md`, `README.md` and this file.

The repository must never be left with a failing pipeline.

Status legend: `[x]` done, `[~]` partial (details listed), `[ ]` open.

The order follows the foundational priorities of the product brief:
semantic object model, stable ids, layout, tokens, action system, patches,
rendering, undo transactions, component abstraction, agent-readable
serialisation. The component library, advanced tooling, MCP and framework
exporters come only after those are stable.

---

## Foundation milestones

### F0 — Skeleton and terminal lifecycle  `[x]`
Workspace, terminal guard with panic-safe restore, capability detection,
XDG config, keymap with overrides, action dispatch, event loop, hideable
panels, status bar, help overlay, pty smoke test.

### F1 — Cells and Unicode  `[x]`
Grapheme validation and width, cell model with transparency and wide
tails, canvas invariants and deltas, compositing with repair, viewport
rendering.

### F2 — Semantic object model and ids  `[x]`
`ObjectId`, `Value`/`Properties`, `Component` tree with responsive rules,
tree helpers (find, locate, insert, remove), documents with screens and
interface/artwork layers, document-wide id uniqueness.

### F3 — Layout model  `[x]`
`Dimension` (content/fill/fixed/percent/flex), placement, containers
(horizontal, vertical, stack, grid), padding, margin, gap, alignment,
min/max clamps, deterministic solver with `Measure` for intrinsic sizes.
- [x] Hug sizing: `content` on a container without an intrinsic size fits
      its children (per direction, grid and absolute children included)
- [x] Absolute children: `fill` stops at the right/bottom margin,
      `percent` is a share of the whole parent

### F4 — Style tokens and themes  `[x]`
Token set, `Theme` with fallback chain, built-in `terminal`,
`minimal-dark`, `minimal-light`, Omarchy conversion, document theme vs
chrome theme, "Use current Omarchy theme" as an undoable operation.

### F5 — Patches  `[x]`
Operations for components, layers, cells and theme; validated,
transactional, invertible, JSON shape as in the brief.

### F6 — Rendering pipeline  `[x]`
`Registry`/`ComponentRenderer`, `RenderContext`, painter, screen
rendering with responsive resolution and size override, `artwork`
component bridging the modes, plain-text preview.

### F7 — History  `[x]`
Transactions with origin, stroke grouping, undo/redo, limit, saved
marker; editor typing and agent patches both go through it.
- [x] Review of agent changes: `glyphforge propose` stores validated
      patches beside the file, the editor polls them, F9 lists proposals
      and applied agent transactions with one-line operation summaries,
      previews proposals on the canvas, accepts (agent transaction),
      rejects, or reverts applied ones (`Session::revert`)

### F8 — Component abstraction  `[x]`
- [x] Renderer registry, placeholder for unknown kinds
- [x] `group`, `panel`, `label`, `heading`, `button`, `divider`, `artwork`
- [x] Component descriptors: title, category, keywords, container flag,
      default size, typed property specs (text, bool, int range, colour,
      choice, border, reference, list, map) with defaults and initial
      values; `prop-type` validation; `glyphforge kinds [--json]`

### F9 — Agent-readable serialisation and API  `[x]`
`.glyph` envelope with `schema_version`, migration chain, one-line diffs
for layout changes, run-based artwork storage, atomic save, `Session`
API, CLI `inspect`/`render`/`validate`/`apply`/`export`/`new`, validator
with stable codes.
- [ ] YAML-style outline export for hand-off (`inspect` prints a compact
      outline today; a dedicated `handoff` format follows the exporter
      interfaces)

---

## Editor milestones

### E1 — Interface Mode editing  `[x]`
- [x] Mode follows the active layer (interface -> INTERFACE, artwork -> SUBCELL)
- [x] Selection by click, by `]`/`[` cycling and by id prompt (Ctrl+F, jumps
      across layers and screens); selection outline with resize handle
- [x] Nudge with arrows, resize with Shift+arrows, mouse drag to move and
      corner drag to resize; each an undoable `move`/`resize` operation,
      a drag is one transaction
- [x] Add components from a prompt with kind completion (`a`), as roots or
      inside the selection, with per-kind default size and properties
- [x] Edit properties by prompt (`Enter`, `key=value`), delete (`Delete`)
- [x] Inspector panel (kind, id, geometry, layout dimensions, properties)
- [x] Save As prompt (Ctrl+Shift+S)
- [x] Reparent: `reparent` operation (subtree, cross-layer, cycle check),
      `p` prompt that keeps absolutely placed components in place
- [x] Multi-selection: Shift+click, Ctrl+A; nudge, resize, delete and
      inspector edits apply to all as one undo step; parent/child stepping
- [x] Interactive inspector: every layout field and descriptor property
      as a row; cycle, edit with suggestions, reset; keyboard and mouse

### E2 — Alignment and geometry  `[x]`
- [x] Core `align`: bounds, align on six edges/centres, distribute with
      exact remainder handling, equal size, snapping with guides
- [x] Drag snapping to the edges and centres of other components and the
      screen, or to a grid; dotted guides while dragging; corner snapping
      for resizes; `[ui] snap`, `snap_grid`
- [x] Align, distribute and equal size on the multi-selection as one
      undo step each; a single component aligns inside its parent

### E3 — Subcell Mode tools  `[x]`
- [x] Tool palette in the left panel (mouse and keys): text, brush, eraser,
      line, rectangle, box, fill, picker; `paint` key context so bare
      letters are tool keys while painting
- [x] Core `paint` module: Bresenham lines, outlines, box frames in a
      chosen border family, flood fill, wide-glyph-safe stamping
- [x] Strokes are one transaction, shapes preview until release (or the
      second Space), fills and shapes are one `set_cells` patch each
- [x] Paged glyph palette in semantic groups (blocks, quadrants, sextants,
      light/heavy/double box lines and junctions covering U+2500–U+259F,
      triangles, shapes, symbols, UI, arrows, Braille, ASCII, math); page
      keys, wheel and `‹ ›`; searchable glyph prompt; any unbound character
      sets the brush
- [x] Box tool style: single, rounded, double, heavy, ASCII
- [x] Semantic brushes: auto-joining line brushes (topology table for
      every box-drawing glyph in `boxdraw`, merging with loose-end
      detection, fallbacks for missing Unicode mixtures, unjoining eraser);
      pattern brushes (shade ramp, checker, rows, columns, dots, gradient);
      button stamps in twelve styles and UI stamps (checkbox, radio, toggle,
      progress, scrollbar, key hint, tab, rule, spinner) in core `stamp`,
      placed by click or sized by the Rect/Line tools; stamp label
- [x] Choosing a tool on an interface layer activates an artwork layer or
      adds a "Paint" layer (undoable)
- [x] Select tool: marquee, drag to move (one undo step, transparent
      cells keep what is beneath), Delete clears, Ctrl+A selects all
- [x] Clipboard: copy/cut/paste of cells and of component subtrees (fresh
      ids), Ctrl+D duplicate; system clipboard through OSC 52 plus
      `wl-copy`/`xclip`
- [x] Dashed line brushes (light and heavy; corners stay solid)
- [x] Brush sizes on artwork layers (square footprint up to 9×9)
- [x] Attribute toggles (bold, italic, underline, strikethrough, dim,
      reverse) in the palette and as keys

### E4 — Colours and palettes  `[x]`
- [x] Foreground/background from the ANSI 16 swatches (left/right click)
- [x] 256-colour picker popup (default, base, cube, greys, recent), mouse
      and keyboard; prompt for names, indices and `#rrggbb`
- [x] Recent colours, swap, reset, picker tool / right-click pick
- [x] Truecolour spectrum in the picker (hue × lightness, muted row,
      greys); opens there for truecolour brushes
- [x] Token picking for components: colour properties cycle through the
      theme tokens, suggest them when edited and show a resolved swatch
- [x] Colour-depth preview of the canvas (F6: 256, 16, terminal)

### E8 — Pixel Mode  `[x]`
- [x] Raster layers (`LayerContent::Raster`) with half, quadrant, sextant
      and Braille modes; schema v2 with migration; row-coded storage
- [x] Encoder: two best colours per cell, glyph tables, transparency over
      the layers beneath, Braille majority colour
- [x] Palette limits (truecolor, 256, 16) with ordered (Bayer) and shade
      glyph dithering, and both combined
- [x] Dynamic drop shadow: offset, blur, opacity, colour; rendered hard,
      ordered, or as per-cell shade glyphs
- [x] Gradient library in the document (stops), six presets; linear and
      radial fills as smooth, hard bands, dithered sub-cells or shade glyphs
- [x] Pixel tools: brush with size, eraser, line, rectangle, filled box,
      flood fill, picker, gradient; previews; one undo step each
- [x] Pixel-precise mouse (SGR-Pixels 1016) in Ghostty, kitty, WezTerm,
      foot and herdr; cell-precise fallback and a keyboard pixel cursor
- [x] Patch operations `set_pixels`, `update_raster`, `replace_raster`,
      `set_gradient`, `delete_gradient`; `inspect` lists rasters and
      gradients
- [x] Octant mode (2×4 blocks, Unicode 16), table generated from
      `UnicodeData.txt` including the 26 patterns older blocks cover
- [x] Error-diffusion dithering (Floyd–Steinberg, Atkinson)
- [x] Gradient editor with draggable stops; OKLab interpolation
- [x] Pixel selection, move, copy, cut, paste; PNG import (CLI and
      editor) with aspect-correct fitting per mode
- [x] `artwork` component showing a raster layer region

### E9 — Menu and themes  `[x]`
- [x] Esc menu (btop style): resume, theme, help, save, save as, new
      document, quit; keyboard and mouse; Esc cancels first, then opens it
- [x] Ten designed themes (forge-dark, paper, cobalt, verdant, lagoon,
      amber-crt, phosphor, nebula, aurora, ember) plus the adaptive system
      theme, with preview swatches, a contrast test and persistence
- [x] Live preview: scrolling the theme list applies themes at once, Enter
      keeps one, Esc restores what was there
- [x] Gradient frames (`frame-from`/`frame-to`): panel and popup borders
      fade across the screen, dimmed when unfocused
- [x] Muted chrome text falls back to a dimmed foreground when the theme's
      muted colour cannot be shown to contrast
- [x] Theme editor: every token with swatch and value, typed or picked
      colours, live preview (chrome or canvas), rename, save as
      `<config>/themes/<id>.toml` or into the document; theme files are
      loaded at startup and listed with the built-in themes
- [x] Design theme page in the menu (undoable `set_theme`)

### E5 — Command palette and context actions  `[x]`
- [x] Ctrl+Space / Ctrl+P, fuzzy subsequence scoring (word starts, runs,
      gaps) over titles, keywords and aliases, recency weighting
- [x] Context entries: add any component kind, go to layer, editor themes
- [x] Cell selection: flip horizontally/vertically (box glyphs mirrored
      through their arms, arrows and blocks through pair tables), swap
      colours, make an `artwork` component from the selection
- [ ] Component-specific actions (table columns, tab pages) arrive with
      those kinds in P1

### E6 — Layers and screens UI  `[x]`
- [x] Interactive layer list with its own focus: activate, show/hide,
      lock, isolate/restore, reorder, new (artwork, interface, pixel),
      rename, duplicate (fresh component ids), merge down (cells or
      components), delete; mouse on eye, lock and name
- [x] Screen operations `create_screen`, `delete_screen`, `update_screen`
      (rename, resize with a lossless inverse), `replace_screen`; switcher
      in the panel, Alt+PgUp/PgDn, new/rename/resize/duplicate/delete
- [x] Responsive preview sizes (F7) without changing the document
- [x] Minimap (F8) with click-to-jump
- [x] Find (Ctrl+F) over components, screens, layers and artwork text

### E7 — Mouse workflow  `[x]`
- [x] Eight resize handles (corners and edge middles) with snapping; a
      left or top handle moves and resizes in one transaction
- [x] Rubber-band selection, Shift+click toggling
- [x] Double click edits a component's text; layer list clicks
- [x] Context menus for components, empty interface space, cell
      selections and layers (keyboard and mouse)
- [x] Z-order (front, back, forward, backward) through `reparent`
- [x] Shift+wheel scrolls horizontally; minimap, palette and inspector
      clicks

---

## Product milestones

### P1 — Component library  `[x]`
- [x] Inputs: input, password, search, textarea, checkbox, radio,
      toggle, select
- [x] Navigation: tabs, breadcrumbs, menu, toolbar, status bar
- [x] Data: list, tree, table, data grid, property grid, key/value list
- [x] Viewers: log, code, markdown, diff
- [x] Feedback: modal, dialog, notification, toast, tooltip, empty
      state, badge, tag, key hint, spinner
- [x] Charts: sparkline, bar, histogram, line, gauge, heatmap, progress
      with block, half-block, Braille and ASCII strategies
- [x] Each with a descriptor, renderer, intrinsic size; a test renders
      every kind at four sizes and checks its descriptor

### P2 — Preview states and data fixtures  `[x]`
- [x] `state` property (normal, focused, hovered, selected, disabled,
      loading, error, empty) honoured by every interactive kind
- [x] Document `data` fixtures (schema v3), `bind` on components (a list,
      or a map with fields such as `columns`/`rows`), `set_data`
      operation, editor prompt, `inspect` lists them, the inspector
      suggests fixture keys

### P3 — Symbols and libraries  `[ ]`
Symbol definitions with instances and property overrides, propagation of
source changes, project libraries (themes, tokens, symbols, templates,
character palettes) as separate `.glyph`-compatible files that can be
shared.

### P4 — Templates and showcase examples  `[ ]`
Starter templates and at least six polished examples (system monitor,
developer dashboard, Git client, AI coding assistant, database explorer,
settings application), each with a distinct aesthetic; the shipped
`examples/dashboard.glyph` is the first.

### P5 — Multi-screen prototypes and overview  `[ ]`
Navigation actions (open dialog, switch tab, go to screen, close modal),
prototype mode, screen graph overview.

### P6 — Import and export  `[ ]`
Plain text and ANSI import; ANSI 16/256/TrueColor export with minimal SGR
deltas and clean reset; clipboard (internal, OSC 52, wl-copy/xclip);
hand-off representation for code generation; exporter interfaces for
Ratatui, Textual, Bubble Tea and Ink (implementations only after the
semantic model has stabilised).

### P7 — Validation and design analysis  `[ ]`
More deterministic checks (invisible fg/bg combinations, wide-character
collisions, minimum sizes, unreachable controls); a separate heuristic
module for spacing, alignment and hierarchy suggestions, clearly labelled
as opinions.

### P8 — MCP server  `[ ]`
Only after the `Session` API is stable: a `glyphforge-mcp` crate exposing
`get_document`, `query_components`, `create_component`, `apply_patch`,
`render_preview`, `validate_layout`, `export_document`. No `set_cell`-sized
tools.

### P9 — Omarchy integration and packaging  `[ ]`
Theme change polling and `theme-set.d` hook, checks in the four Omarchy
terminals, `.desktop` entry, AppStream metadata, PKGBUILD, install script,
installation guide.

### P10 — Autosave and recovery  `[ ]`
Autosave to a sibling file on a timer, recovery prompt on start, never
overwriting the original silently.
