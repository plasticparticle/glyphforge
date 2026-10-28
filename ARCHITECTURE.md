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
align.rs         align, distribute, equal size, snapping with guides
theme.rs         Theme, style tokens, fallback chain, built-in themes
boxdraw.rs       Border families, glyph tables, line topology (Arms per glyph)
paint.rs         Cell drawing: line, outline, box, flood fill, stamping,
                 auto-joining lines, detach, pattern brushes
stamp.rs         UI element stamps (buttons, checkbox, progress, tab, ...)
raster/          Pixel Mode: Pixel, RasterLayer, encoder, dithering,
                 drop shadow, gradients, pixel drawing, row-coded storage
document/        Grapheme, Color, Cell, Canvas (+ diff-friendly repr),
                 Layer (artwork | interface), Screen, Document
patch.rs         Operation, Patch: validated, transactional, invertible
history.rs       Transaction, Origin, History (undo/redo, open strokes)
render/          RenderContext, Registry, ComponentRenderer, descriptors
                 (typed property specs), painter, render_components,
                 render_screen, to_text_lines
validate.rs      deterministic diagnostics with stable codes
project.rs       .glyph envelope, schema_version, migration chain, atomic save
proposal.rs      agent proposals stored beside a file for review
api.rs           Session: the application API
```

Application module map (`crates/glyphforge/src`):

```
main.rs, cli.rs  argument parsing; TUI by default, subcommands headless
headless.rs      inspect / render / validate / apply / propose / import /
                 export / kinds / new
image.rs         PNG decoding and the import patch
app/             App state, dispatch (behaviour), run loop, viewport,
                 interface (selection), inspector (editable rows),
                 arrange (align, distribute, snapping),
                 selection (Select tool, clipboard),
                 commands (command palette, context actions),
                 layers (layer list, screens, preview size, find),
                 mouse (handles, rubber band, context menus, z-order),
                 paint (tools, brush, colours),
                 pixels (raster tools), menu (Esc menu, theme choice)
actions/         Action enum + descriptors (names, keywords, default keys)
input/           key model, key-chord parsing, keymap with config overrides
terminal/        RAII guard, panic hook, capability detection,
                 clipboard (OSC 52, wl-copy, xclip)
render/          core Canvas -> Ratatui buffer, colour degradation
ui/              layout, header, canvas view, panels, palette, colour
                 picker, menu, prompt, status bar, help
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
            ├─ Artwork   { cells: Canvas }   raw cells      (Subcell Mode)
            └─ Raster    { raster }          pixels         (Pixel Mode)
 gradients[]        Gradient { id, name, stops[] } for gradient fills
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
pass for half-covered wide glyphs. Sub-cell pixels are not part of the
cell model: they live in raster layers (3.3) and *produce* cells.

### 3.3 Pixels (Pixel Mode)

A `RasterLayer` is `{ mode, dither, palette, shadow, pixels }`. The pixel
grid has `mode.cell_px()` pixels per cell (half 1×2, quadrant 2×2, sextant
2×3, braille and octant 2×4). A `Pixel` is `Clear`, `Solid(Color)` or
`Shade { fg, bg, level }` (a `░▒▓` mix; `bg: None` mixes over whatever is
beneath, used by soft shadows). The pixels are the document; everything
else is derived at render time, so switching mode, dithering, palette
limit or shadow never repaints. A mode change resamples (nearest pixel
centre) and its inverse patch restores the previous raster exactly.

`raster::encode(layer, backdrop, size)` turns a layer into cells:

1. per cell, gather its pixels; transparent pixels take the drop shadow's
   pixels (`DropShadow::render`: the mask shifted by the offset,
   box-blurred through a summed-area table, scaled by opacity, then hard,
   Bayer-ordered, or quantised to shade levels per cell);
2. a fully transparent cell stays transparent; a cell of one shade pixel
   becomes that shade glyph;
3. transparent pixels resolve to the backdrop's background (the composite
   of the layers beneath), shade pixels in mixed cells blend;
4. with a palette limit, colours are reduced to ANSI 16/256: nearest, with
   Bayer offsets (`ordered`), and uniform cells may become the best
   two-colour shade mix (`shade`);
5. the cell glyph: Braille takes the majority colour and one dot per
   painted pixel; block modes choose the two colours with the least
   weighted RGB error (exhaustive over the distinct colours) and look the
   pixel mask up in the half, quadrant or sextant table (sextant code
   points are computed from the mask, skipping the masks that are `▌▐`).

Before the per-cell pass, error-diffusion dithering (Floyd–Steinberg,
Atkinson) reduces every solid pixel to the palette in reading order and
spreads the error to solid neighbours; shadows threshold their alpha with
the ordered pattern instead. Octant glyphs come from a 256-entry table
generated from `UnicodeData.txt` (230 octants plus the 26 patterns that
older blocks, quadrants and quarter blocks already cover).

Gradients (`raster::gradient`) have sorted stops and sample in RGB or, with
`space: "oklab"`, in OKLab. `raster::import` fits RGBA images into a
screen area keeping their physical proportions (each mode's pixels have
their own shape on 1:2 cells) and box-filters them, alpha-weighted, with
transparent pixels below half coverage; PNG decoding lives in the
application (`image.rs`, the `png` crate). `RenderContext::artwork`
encodes a pixel layer on its own when an `artwork` component references
one. A fill
takes a region (the flood region at the start point), a shape (linear or
radial, measured in cell-proportional units so circles stay round) and a
style: smooth, hard bands, ordered dither between the two stops around
each pixel, or shade glyphs quantised per cell. Raster storage is a pixel
palette plus one code string per row (91-symbol alphabet, wider codes for
large palettes), trailing transparency trimmed.

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
A `content`-sized component without an intrinsic size of its own hugs its
children: the sum along its direction (plus gaps), the maximum across,
grid cells times columns and rows, and absolute children's far corners;
children that only follow their container (`fill`, `flex`, `percent`)
contribute nothing, and if nothing contributes the component fills. For
absolute children `fill` stops at the right/bottom margin and `percent`
is a share of the whole parent.
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

Thirteen themes ship with the editor (`Theme::builtin`): the adaptive
`terminal` theme, `minimal-dark`/`minimal-light`, and ten designed ones
(`forge-dark`, `paper`, `cobalt`, `verdant`, `lagoon`, `amber-crt`,
`phosphor`, `nebula`, `aurora`, `ember`). The optional `frame-from` and
`frame-to` colours give a theme gradient frames: `ui::paint_frame`
recolours the box-drawing cells of each panel and popup along the diagonal
of the whole screen after the block is drawn, so the fade is continuous
across panels and unfocused frames are dimmed. A test holds them to their contract: every token
defined, unique ids, and foreground, muted, accent and selection colours
that stand out from their background by at least 70 of 255 in perceived
brightness.

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
             Raster    → raster::encode(layer, composite so far) → composite
             Interface → resolve_responsive(width)
                         layout::solve(..., RegistryMeasure)
                         for each component (parents first):
                             registry.renderer(kind).render(c, rect, inner, ctx, canvas)
                         composite result
        ──► Canvas (invariant-safe) ──► TUI viewport / text / exporters
```

`RenderContext` carries the theme and the screen (for artwork references).
`Registry` maps kinds to `ComponentRenderer`s; adding a kind is one struct
and one registration line. Each renderer returns a `Descriptor`: title,
category, summary, keywords, whether it is a container, default size, and
one `PropSpec` per property it reads (`PropKind`: text, bool, int range,
colour, choice, border, reference, list, map; the default the renderer
assumes and the initial value a new instance gets). Descriptors are data:
the inspector picks an editor per `PropKind` (cycle, parse, suggestions),
the add prompt and command palette match titles and keywords, the
validator reports `prop-type` for values that do not fit, and `glyphforge
kinds --json` gives agents the same table. Renderers only paint through `painter`
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
create_component, delete_component, reparent,
set_cells,
set_pixels, update_raster, replace_raster,
set_gradient, delete_gradient,
create_layer, delete_layer, update_layer, reorder_layer,
create_screen, delete_screen, update_screen, replace_screen,
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
glyphforge propose  FILE PATCH.json [--agent NAME] [-m MESSAGE]
glyphforge kinds    [KIND] [--json]
```

`propose` validates the patch against the file, then writes it to
`FILE.proposals/NNNN-agent.json` (atomic rename) instead of applying it.
The editor polls that directory once a second; its review panel previews
a proposal by applying it to a clone of the document, accepting applies
it through `Session::apply_proposal` (an `Origin::Agent` transaction) and
deletes the file. `Session::revert(index)` applies an older transaction's
inverse as a new transaction, so later edits stay; it fails cleanly when
they conflict. `Operation::summary` gives every operation a one-line
description for the review and logs.

The intended agent loop is `inspect -> apply -> render -> validate`, each a
cheap process.

---

## 10. Project format

`.glyph` files are JSON with a versioned envelope:

```json
{ "format": "glyphforge", "schema_version": 2, "meta": {...}, "theme": {...}, "gradients": [...], "screens": [...] }
```

Design rules for diffability:

- pretty-printed, one key per line, keys sorted (all maps are `BTreeMap`);
- defaults are omitted (`serde(default, skip_serializing_if)`), so a
  component with no children has no `children` key;
- layout dimensions are short strings (`"fixed(24)"`), so a width change
  is a one-line diff (asserted by a test);
- artwork is stored as text runs and style runs per row, not one object
  per cell; wide-glyph tails are not stored and are rebuilt on load;
- raster pixels are a palette of pixel values plus one code string per
  pixel row, so a stroke changes only the rows it touches;
- there is no binary blob; heavy assets would be separate files.

Loading reads `schema_version`, runs the migration chain up to the current
version (1 → 2 added raster layers and gradients, which v1 files cannot
contain, so the step is the identity), rejects
unversioned or newer files, then validates ids. Saving writes to a
temporary sibling and renames, so a crash never leaves a half-written file.

---

## 11. Validation

`validate::validate(doc)` returns `Diagnostic { severity, code, screen,
target, message }` with stable codes (`duplicate-id`, `no-space`,
`clipped`, `truncated`, `overlap`, `unknown-kind`, `theme-tokens`,
`prop-type`).
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
  route keys through an open prompt, the help overlay, the keymap (the
  `Interface` context shadows `Global` while the active layer holds
  components), Vim navigation, then text insertion. Typing on an artwork
  layer records `set_cells` into an open transaction that closes on the
  next non-edit action.
- **Interface Mode.** Selection is editor state (an `ObjectId`). Hit
  testing and nudging use the solved `LayoutResult` of the active layer,
  cached per frame like the composite. Nudge, resize, add, edit and delete
  are `move`, `resize`, `create_component`, `set_property` and
  `delete_component` operations, `p` is `reparent`; with several components
selected (Shift+click, Ctrl+A) every operation is issued per component in
one patch. The inspector (`app/inspector.rs`) lists the primary
selection's layout fields and descriptor properties as rows (`FieldKey`)
and edits them as `set_layout` and `set_property` patches over the whole
selection; its geometry function serves drawing and mouse hits.
Arranging (`app/arrange.rs`) takes the selection's solved rects, runs
them through `glyphforge_core::align` and issues `move`/`resize` per
changed rect in one patch; positions go back to parent-relative offsets.
A move drag snaps the dragged rect, a resize drag snaps its corner cell,
against every other component on the layer and the screen (not its own
descendants); the guides are editor state drawn over the canvas until the
button is released. A mouse
drag opens one transaction and
  records an operation per motion event, so undo removes the whole drag.
  The `Prompt` (input line with candidates) serves select-by-id, add
  component, edit property, save-as, colours and the brush glyph, and is
  the base of the command palette.
- **Subcell Mode tools.** `PaintState` holds the tool, brush kind and
  glyph, stamp label, palette page, box style, shape anchor, open stroke
  and recent colours; the brush colours are the editor's `CellStyle`.
  `app/brushes.rs` is the catalogue: `BrushKind` (`Glyph`, `Line`,
  `Pattern`, `Stamp`), named presets and `BRUSH_PAGES` (semantic brushes
  first, then glyph pages; tests require box-drawing and block coverage and
  that every page fits the palette). Every tool dispatches on the brush
  kind. Line topology: `boxdraw::Arms` gives each box-drawing glyph four
  weighted arms, generated from the Unicode names; `paint::join_edges`
  adds arms along path or rectangle edges and merges with existing glyphs,
  counting an existing arm only if the neighbour connects back, so a
  stroke from a loose end turns a corner instead of making a tee. Geometry lives in `glyphforge_core::paint` (pure,
  usable headlessly); the editor only turns it into `set_cells`. A mouse
  stroke opens one transaction and records a line segment per motion
  event; shapes are previewed as an overlay from `shape_writes` and
  applied as one patch on release or on the second Space; fill and picker
  act immediately. Key lookup has three contexts: `interface` (active
  layer holds components), `paint` (artwork layer with a tool other than
  Text, so bare letters are commands and unbound characters set the brush)
  and `global`. Bindings may carry a context prefix (`paint:b`), in the
  defaults and in `[keys]`. The palette and the colour picker each expose
  a geometry function used by both rendering and mouse hit-testing.
- **Selection and clipboard.** The Select tool's marquee is editor
  state; a drag inside it lifts the cells into a `CellClip` and previews
  them at the pointer, the release applies one `set_cells` patch (clears,
  then writes; empty cells are transparent). The clipboard holds either a
  `CellClip` or component subtrees; pasting components re-ids every node
  against the document. Copy also queues text for the system clipboard,
  which the run loop (the only owner of the terminal) sends as OSC 52 and
  pipes to `wl-copy`/`xclip`.
- **Layers and screens.** The layer list is its own focus region (`Focus::Layers`)
  between the canvas and the inspector; plain keys there manage layers,
  everything else falls through to the keymap. Merging down paints the
  upper artwork's cells onto the lower layer or reparents the upper
  interface layer's roots, then deletes the upper layer, in one patch.
  Isolate remembers the visibility it replaced as editor state. Resizing
  a screen crops or extends every layer, so `update_screen` answers with
  `replace_screen` holding the whole previous screen. The preview size is
  editor state: `doc_size()`, the composite and the layout use it, so
  responsive rules, selection and hit-testing follow while the document
  keeps its size.
- **Mouse.** Resize handles are computed from the solved rect
  (`mouse::handle_at`); a drag moves the grabbed edges, snaps the moving
  corner cell and records `move` + `resize` into the open transaction.
  Empty space starts a rubber band (editor state) that selects fully
  contained components on release. Double clicks are detected in the run
  loop (same cell within 400 ms) and become `Action::DoubleClick`.
  Context menus are lists of `(title, Action)` built for what was
  clicked, so they add no behaviour of their own. Z-order is `reparent`
  to the same parent at another index.
- **Command palette.** `app/commands.rs` builds the entries when the
  palette opens: implemented action descriptors that apply in the current
  key context (cursor steps excluded), then context entries (component
  kinds from the registry's descriptors, layers, themes, and cell
  selection actions first). Matching is a subsequence score over the
  title (keywords and aliases at half weight) plus a bonus for recently
  run commands; running an entry dispatches its `Action`, so the palette
  adds no behaviour of its own. Mirroring lives in `paint::mirror_glyph`
  and `flip_region` (core): box-drawing glyphs swap their arms, other
  glyphs use pair tables.
- **Pixel Mode.** `app/pixels.rs` holds the pixel tools, settings,
  gradient library and preview. Tools that need a layer kind (Text:
  artwork, Gradient: raster) switch to one or add it. Mouse presses map
  to pixels: with SGR-Pixels reports (mode 1016, enabled only for known
  terminals and herdr when the cell size in pixels is known) the fraction
  inside the cell selects the sub-cell pixel, otherwise a press covers the
  whole cell. The keyboard has a pixel cursor (cell cursor + sub-cell
  offset). Strokes are one transaction; shapes and gradient fills preview
  by rendering a cloned screen and apply as one patch (a preset gradient
  is saved by the same patch). The brushes block becomes pixel pages
  (dither, shadow, gradient style, gradients) built at runtime as
  `PaletteEntry`s that carry their `Action`.
- **Menu.** Esc cancels whatever is in progress and otherwise opens the
  menu (`app/menu.rs`). Its entries are built per page as label, hint,
  `Action` and optional preview swatch, so keyboard and mouse share one
  path: choosing an entry dispatches its action and closes the menu unless
  the action belongs to the menu itself (paging, trying themes). On the
  theme page the selection previews live: moving applies the theme without
  remembering it, Enter keeps it (written to `<state>/ui-theme`, restored
  at startup unless `[theme] ui` pins one) and Esc restores the choice
  captured when the page opened.
- **Readable chrome.** `styles::muted` uses the muted token only when it
  provably contrasts with the background; the terminal's own palette is
  unknown, so otherwise the normal foreground is dimmed.
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
