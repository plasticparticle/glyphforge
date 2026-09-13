# Glyphforge Architecture

Glyphforge is a terminal-native visual design environment for terminal user
interfaces. It runs as a full-screen TUI and, through the same core, as a
headless CLI for scripts and coding agents. This document describes the
architecture every milestone builds on; when a milestone changes a boundary,
this file changes in the same commit.

The governing principle:

> **Semantic design first. Terminal cells are a rendering target, not the
> document model.** Interface Mode keeps panels, tables and buttons as
> objects with identity, properties, layout and relationships. Subcell Mode
> works on raw cells (and sub-cell pixels) for artwork. Both live in one
> document and reference each other.

---

## 1. Crates and layering

```
glyphforge-core          domain: ids, components, layout, themes, cells,
                         patches, history, rendering, validation, project
                         format, application API          (no Ratatui)
        │
        │  api::Session  (the application API)
        │
   ┌────┼───────────────┐
  CLI  TUI            MCP (later)
   glyphforge          glyphforge          glyphforge-mcp
   headless.rs         app/, ui/, ...      not before the API is stable
```

| Crate | Responsibility | Allowed dependencies |
|-------|----------------|----------------------|
| `crates/glyphforge-core` | Everything about designs. | `serde`, `serde_json`, `unicode-*`, `thiserror`. **Never** `ratatui`/`crossterm`. |
| `crates/glyphforge` | The binary: terminal lifecycle, input, actions, editor UI, config, Omarchy detection, CLI subcommands. | core + `ratatui`, `crossterm`, `clap`, `toml`, `tracing`. |

The manifest enforces the boundary: the core cannot name a Ratatui type.
The interactive editor, the CLI and a future MCP server call the same
`Session` methods; none of them has private access to the document.

Core module map (`crates/glyphforge-core/src`):

```
id.rs            ObjectId: validated slug, unique per document
value.rs         Value/Properties: JSON-like, totally ordered (no floats)
component.rs     Component tree, responsive rules, tree helpers
layout.rs        Dimension/Placement/Container + deterministic solver
theme.rs         Theme, style tokens, fallback chain, built-in themes
boxdraw.rs       Border families and glyph tables (topology later)
document/        Grapheme, Color, Cell, Canvas (+ diff-friendly repr),
                 Layer (artwork | interface), Screen, Document
patch.rs         Operation, Patch: validated, transactional, invertible
history.rs       Transaction, Origin, History (undo/redo, open strokes)
render/          RenderContext, Registry, ComponentRenderer, painter,
                 render_components, render_screen, to_text_lines
validate.rs      deterministic diagnostics with stable codes
project.rs       .glyph envelope, schema_version, migration chain, atomic save
api.rs           Session: the application API
```

Application module map (`crates/glyphforge/src`):

```
main.rs, cli.rs  argument parsing; TUI by default, subcommands headless
headless.rs      inspect / render / validate / apply / export / new
app/             App state, dispatch (behaviour), run loop, viewport
actions/         Action enum + descriptors (names, keywords, default keys)
input/           key model, key-chord parsing, keymap with config overrides
terminal/        RAII guard, panic hook, capability detection
render/          core Canvas -> Ratatui buffer, colour degradation
ui/              layout, header, canvas view, panels, status bar, help
omarchy/         detection, colors.toml -> Theme
config/          TOML config, XDG paths
```

---

## 2. Identity: `ObjectId`

Every screen, layer, component, theme and (later) symbol has an
`ObjectId`: a slug `[a-z0-9][a-z0-9._-]*`, at most 64 characters, unique
across the whole document. Ids are how humans and agents refer to things
(`sidebar`, `server-table`), so they are readable and stable, never UUIDs.
`ObjectId::slugify` derives ids from names; `unique_among` disambiguates.
Duplicate ids are rejected on load and by every create operation.

---

## 3. Document model

```
Document
 ├─ meta            title, author, description
 ├─ theme           Theme (tokens -> colours, border family, spacing)
 └─ screens[]       Screen { id, name, size, layers[] }
      └─ layers[]   Layer { id, name, visible, locked, content }
            ├─ Interface { components[] }   semantic tree (Interface Mode)
            └─ Artwork   { cells: Canvas }   raw cells      (Subcell Mode)
```

- A **screen** is one terminal-sized design surface; a project holds many
  (Dashboard, Settings, ...). Navigation relationships come with the
  prototype milestone.
- A **layer** holds either components or cells, never both. Layers are
  ordered bottom to top and composited in that order, so artwork can sit
  under or over interface layers.
- A **component** is `{ id, kind, props, layout, responsive[], children[] }`.
  `kind` is a registry key (`panel`, `label`, `table`); unknown kinds are
  preserved and rendered as a labelled placeholder rather than dropped.
  `props` is an ordered map of `Value`s. Components never store cells.
- **Artwork inside interfaces:** the `artwork` component kind references a
  region of an artwork layer (`layer`, `x`, `y`, `width`, `height`) and is
  placed by layout like any other component. The source layer is usually
  hidden so it acts as an asset. This is the bridge between the two modes;
  the reverse direction (render a component into cells) is a planned
  operation.

### 3.1 Cells (Subcell Mode)

`Grapheme` is one extended grapheme cluster of width 1 or 2. `Cell` is
`Empty` (transparent), `Glyph(Grapheme)` or `WideTail`. `Canvas` is a dense
grid that maintains the wide-glyph invariant in `put` and returns the cells
it changed, and `composite_over` merges another canvas on top with a repair
pass for half-covered wide glyphs. Sub-cell raster drawing (Braille,
quarter blocks) is a separate module that *produces* cells; it is not part
of the cell model.

### 3.2 Values and properties

`Value` is `Null | Bool | Int | Str | List | Map` with a total order, so
patches and diffs are deterministic. Colour properties hold either a token
name (`primary`) or a literal (`#7aa2f7`, `ansi:4`); the theme resolves
both. Floats are deliberately absent.

---

## 4. Layout model

`Layout` per component:

| Field | Meaning |
|-------|---------|
| `placement` | `flow` (laid out by the parent) or `absolute {x, y}` inside the parent's inner rect |
| `width`, `height` | `content`, `fill`, `fixed(N)`, `percent(N)`, `flex(N)` |
| `min_*`, `max_*` | clamps |
| `padding`, `margin` | `Edges` |
| `container` | `direction` (`horizontal`, `vertical`, `stack`, `grid`), `gap`, `columns`, `align` |

`layout::solve(roots, area, measure)` returns integer rectangles for every
component (outer and inner). Rules, in order: absolute children are placed
first and never affect flow; fixed, percent and content sizes are resolved;
`fill`/`flex` share the remainder by weight with largest-remainder rounding
in document order; clamps apply; cross-axis alignment stretches by default.
Grid uses equal columns and per-row heights. The solver is a pure function
and has no notion of glyphs: renderers implement `Measure` to provide
intrinsic sizes (a label's text width) and chrome insets (a panel's
border). Same input, same output, always.

Responsive rules live on the component: `{ max_width, min_width, hide,
set, layout }`. `Component::resolve_responsive(width)` produces the
effective tree for a screen width before layout and rendering; previews at
other sizes only change the width passed in.

---

## 5. Style tokens and themes

Components reference tokens, never literal colours (literals are allowed
but discouraged). The token set:

```
background surface surface-alt border border-muted foreground
foreground-muted primary secondary success warning error info
selection selection-foreground focus
```

`Theme { id, name, colors, border, spacing }` maps tokens to `Color` and
provides a fallback chain (`surface` -> `background`, status colours ->
`primary` -> `foreground` -> terminal default) so partial themes still
render. Built-in themes: `terminal` (ANSI only, adapts to any terminal),
`minimal-dark`, `minimal-light`. Omarchy themes are converted at runtime
from `colors.toml` (section 12). More preview themes are original designs,
not copies of vendor palettes.

The **document theme** (in the file) and the **editor chrome theme** (what
the Glyphforge UI itself uses) are separate `Theme` values; "Use current
Omarchy theme" copies the chrome theme into the document as an undoable
operation.

---

## 6. Rendering pipeline

```
Screen ──► for each visible layer, bottom to top:
             Artwork   → composite cells
             Interface → resolve_responsive(width)
                         layout::solve(..., RegistryMeasure)
                         for each component (parents first):
                             registry.renderer(kind).render(c, rect, inner, ctx, canvas)
                         composite result
        ──► Canvas (invariant-safe) ──► TUI viewport / text / exporters
```

`RenderContext` carries the theme and the screen (for artwork references).
`Registry` maps kinds to `ComponentRenderer`s; adding a kind is one struct
and one registration line. Renderers only paint through `painter`
(`draw_text` with clipping and wide-glyph safety, `fill`, `draw_border`,
`truncate` with ellipsis). Current kinds: `group`, `panel`, `label`,
`heading`, `button`, `divider`, `artwork`, plus the placeholder.

The editor shows the rendered canvas through a scrolling viewport and
degrades colours to the detected depth. Previews at other terminal sizes
render the same screen with a different `size` argument; nothing in the
document changes.

---

## 7. Patches: the mutation mechanism

Every change to a document is a `Patch { operations[] }`. Operations
address objects by id:

```
move, resize, set_property, set_layout,
create_component, delete_component,
set_cells,
create_layer, delete_layer, update_layer, reorder_layer,
set_theme
```

`Patch::apply(&mut Document) -> Result<Patch>`:

- **validated**: unknown targets, duplicate ids, locked layers, wrong layer
  kinds and out-of-range cells are errors;
- **transactional**: on the first failing operation, the already applied
  ones are rolled back through their inverses and the document is
  unchanged;
- **invertible**: the return value is the inverse patch (`move` ->
  `set_layout` with the previous layout, `create` -> `delete`, `set_cells`
  -> `set_cells` with the previous cells, ...);
- **deterministic and diffable**: operations are plain JSON with an `op`
  tag, exactly the shape agents send.

Humans and agents use the same operations. The editor's typing produces
`set_cells`; an agent's "make it more compact" produces `resize` and
`set_layout`. There is no second mutation path.

---

## 8. History

`History` is a stack of `Transaction { label, origin, forward, inverse }`.
`apply` runs a whole patch as one transaction; `begin`/`record`/`end`
collect continuous edits (a stroke, a typed word) into one. Undo replays
the inverse, redo the forward patch. The saved position is tracked for
dirty state; trimming the stack past the limit makes the saved state
unreachable and therefore dirty. `Origin::Agent { name, description }`
marks agent transactions so the UI can list, inspect, accept or reject
them (review UI is a later milestone; the data is already there).

---

## 9. Application API

`api::Session` owns a document, its history, the renderer registry and the
file path, and exposes: `document`, `component`, `query_components`,
`create_component`, `update_component`, `move_component`,
`resize_component`, `delete_component`, `apply_patch`, `begin`/`record`/
`end`, `undo`/`redo`, `render` (canvas), `render_text`, `validate`,
`validate_at(screen, size)`, `save`/`save_as`/`open`, `to_json`.

The TUI's `App` holds a `Session` and dispatches actions into it; the CLI
subcommands call the same methods; an MCP server will wrap them once the
surface has settled. Nothing is added for agents that humans do not use.

### 9.1 CLI

```
glyphforge inspect  FILE [--json]
glyphforge render   FILE [--screen ID] [--width W --height H]
glyphforge validate FILE [--width W --height H] [--strict]
glyphforge apply    FILE PATCH.json [--out FILE] [--dry-run]
glyphforge export   FILE --format text [--out FILE]
glyphforge new      FILE [--width W --height H]
```

The intended agent loop is `inspect -> apply -> render -> validate`, each a
cheap process.

---

## 10. Project format

`.glyph` files are JSON with a versioned envelope:

```json
{ "format": "glyphforge", "schema_version": 1, "meta": {...}, "theme": {...}, "screens": [...] }
```

Design rules for diffability:

- pretty-printed, one key per line, keys sorted (all maps are `BTreeMap`);
- defaults are omitted (`serde(default, skip_serializing_if)`), so a
  component with no children has no `children` key;
- layout dimensions are short strings (`"fixed(24)"`), so a width change
  is a one-line diff (asserted by a test);
- artwork is stored as text runs and style runs per row, not one object
  per cell; wide-glyph tails are not stored and are rebuilt on load;
- there is no binary blob; heavy assets would be separate files.

Loading reads `schema_version`, runs the migration chain up to the current
version (each future bump adds one step and a fixture test), rejects
unversioned or newer files, then validates ids. Saving writes to a
temporary sibling and renames, so a crash never leaves a half-written file.

---

## 11. Validation

`validate::validate(doc)` returns `Diagnostic { severity, code, screen,
target, message }` with stable codes (`duplicate-id`, `no-space`,
`clipped`, `truncated`, `overlap`, `unknown-kind`, `theme-tokens`).
Messages name components, not coordinates ("`metrics-table`: "Request
latency percentile" will be truncated at 80×24"). `validate_at(screen,
size)` checks responsiveness. Deterministic layout validation is kept
separate from heuristic design suggestions (a later, clearly labelled
module) because the latter are opinions.

---

## 12. Omarchy integration

Detection and theme loading use only public Omarchy files: the active
theme name in `~/.config/omarchy/current/theme.name` and its colours in
`~/.config/omarchy/current/theme/colors.toml` (Omarchy generates that file
from `alacritty.toml` when a theme lacks it). `OmarchyColors::to_theme`
derives every token from the theme's own colours (`primary` = accent,
status colours = ANSI 1..4, muted = colour 8, surfaces mixed from
background toward foreground). No Omarchy theme is hard-coded; no Omarchy
file is written. The chrome theme reloads on focus gain and on demand;
Milestone 14 adds a modification-time poll and documents the
`~/.config/omarchy/hooks/theme-set.d/` hook for instant refresh.

---

## 13. The editor application

- **State machine, single dispatch.** `App::dispatch(Action)` is the only
  place with behaviour; keys, mouse, palette and menus produce `Action`s.
  Document mutations go through `Session` as patches, so the editor gets
  undo/redo, dirty tracking and agent transactions from the core.
- **Event loop.** Draw when something changed, poll events with a tick,
  route keys through the help overlay, the keymap, Vim navigation, then
  text insertion. Typing on an artwork layer records `set_cells` into an
  open transaction that closes on the next non-edit action.
- **Terminal lifecycle.** An RAII guard enables raw mode, the alternate
  screen, mouse, bracketed paste, focus events and kitty keyboard flags
  when supported; a panic hook restores everything first.
- **Capabilities.** Colour depth, Unicode locale, mouse and keyboard
  protocol are detected once and only consulted, never assumed.

---

## 14. Assumptions from the first pass that were replaced

The first skeleton modelled a document as layers of cells only. The
following assumptions would have blocked the semantic direction and were
removed before building on them:

| Old assumption | Why it blocked the goals | Replacement |
|----------------|--------------------------|-------------|
| `Document = Vec<Layer>` of cells | No identity, hierarchy or properties to patch, query, validate or export | Screens with interface (component tree) and artwork (cells) layers |
| Layer ids were UUIDs | Agents and diffs need readable, stable names | `ObjectId` slugs, unique per document |
| Edits mutated `Canvas` directly from the app | Undo, agent patches and the CLI would each need their own mutation path | Every change is an `Operation` in a `Patch`; history stores inverse patches |
| `Canvas` serialised as one object per cell | Thousands of unrelated diff lines per edit | Text runs and style runs per row |
| UI theme was a fixed struct of roles | Documents need portable, user-definable tokens | `Theme` with token map and fallback chain, shared by chrome and documents |
| Rendering knew only cells | No place to lay out or draw components | `Registry` of renderers, `RenderContext`, layout solver |
| Active layer lived in the document | Editor state leaked into the file | Active screen/layer are editor state |
| Colours as an ad-hoc `UiTheme` per app | Omarchy mapping could not be reused by documents | Omarchy colours become a `Theme` like any other |

Unchanged and still correct: grapheme-based cells with wide-glyph
invariants, the terminal guard, capability detection, XDG configuration,
the action/keymap design.

---

## 15. Testing strategy

- Core: unit tests beside each module (ids, values, layout solver
  including rounding and clamps, theme fallbacks, patch inverses and
  rollback, history grouping and limits, renderers, painter clipping,
  validation codes, project round trip and one-line-diff property) plus an
  integration test that loads, validates and renders the shipped example
  at two sizes and checks it is stored canonically.
- Application: pure functions (key chords, keymaps, capabilities, Omarchy
  parsing, viewport math) are unit tested; the editor UI renders into
  Ratatui's `TestBackend`; CLI subcommands run against temporary files.
- `scripts/pty_smoke_test.py` runs the real binary in a pseudo-terminal.
