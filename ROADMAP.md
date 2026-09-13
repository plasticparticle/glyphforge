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
- [ ] `hug`-style content sizing for containers (size to children)
- [ ] `fill` in cross axis for absolute children

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
- [ ] Review UI for agent transactions (list, inspect diff, accept, reject)

### F8 — Component abstraction  `[~]`
- [x] Renderer registry, placeholder for unknown kinds
- [x] `group`, `panel`, `label`, `heading`, `button`, `divider`, `artwork`
- [ ] Component descriptors (property schema, defaults, palette metadata)
      so the inspector and the command palette can offer them

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

### E1 — Interface Mode editing  `[~]`
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
- [ ] Reparent (move a component into another container)
- [ ] Multi-selection
- [ ] Editing layout fields (direction, gap, padding) from the inspector
      without typing `set_layout` JSON

### E2 — Alignment and geometry  `[ ]`
Snap to grid and neighbours, alignment guides while dragging, align and
distribute operations on multi-selection, equal size.

### E3 — Subcell Mode tools  `[ ]`
Tool trait and registry; pencil, eraser, text, rectangle selection with
move/copy/cut/paste/fill; line/box tools with topology-aware junctions
(`boxdraw` gains the merge tables); sub-cell raster (half, quarter,
Braille, shading) encoded to glyphs.

### E4 — Colours and palettes  `[ ]`
Foreground/background pickers (16/256/RGB), recent and palette colours,
token picker for components, colour-depth preview modes.

### E5 — Command palette and context actions  `[ ]`
Ctrl+Space (and the Ctrl+P fallback), fuzzy matching over titles,
keywords and aliases with recency weighting, contextual actions from the
selection (table selected -> column actions, artwork selected -> mirror/
invert/convert).

### E6 — Layers and screens UI  `[ ]`
Interactive layer list (create, rename, duplicate, reorder, lock, hide,
isolate, merge), screen switcher, responsive size presets with instant
preview, minimap for large canvases, find component/screen/text.

### E7 — Mouse workflow  `[ ]`
Drag, resize handles, multi-select, double click, context menus where the
terminal permits, wheel, palette and layer interaction.

---

## Product milestones

### P1 — Component library  `[ ]`
The brief's list, in slices ordered by usefulness: inputs and controls
(input, password, search, textarea, checkbox, radio, toggle, select),
navigation (tabs, breadcrumbs, menu, toolbar, status bar), data (list,
tree, table, data grid, property grid, key-value list), viewers (log,
code, markdown, diff), feedback (modal, dialog, notification, toast,
tooltip, empty state, badge, tag, key hint, spinner), charts (sparkline,
bar, histogram, line, gauge, heatmap, progress) with block, half-block,
Braille and ASCII strategies. Each component gets a descriptor, renderer,
intrinsic size and tests.

### P2 — Preview states and data fixtures  `[ ]`
Component state props (focused, hovered, selected, disabled, loading,
error, empty), document-level `data` fixtures and `data` bindings on
components, sample rows and mock series.

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
