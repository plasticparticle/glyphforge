# Glyphforge

A terminal-native visual design environment for terminal user interfaces.
Glyphforge is a full-screen TUI in which humans and coding agents design
TUIs semantically: a panel stays a panel, a table stays a table, and the
terminal cells are rendered from that model. Raw ANSI/Unicode artwork lives
alongside in Subcell Mode.

Think: Figma + terminal-native editor + semantic UI model + ANSI/Unicode
art tools. Built in Rust on Ratatui and Crossterm, with a core crate that
knows nothing about either.

> Status: the foundations are in place (semantic model with stable ids,
> layout solver, style tokens and themes, transactional patches,
> undo/redo, rendering pipeline, versioned diff-friendly project format,
> validator, application API, headless CLI). The editor edits artwork
> layers with a tool palette (text, brush, eraser, line, rectangle, box
> in single/rounded/double/heavy/ASCII lines, fill, picker), semantic
> brushes (auto-joining lines, patterns, button and UI stamps), a paged
> glyph palette (blocks, sextants, every box-drawing glyph, shapes, arrows,
> Braille, ...) and a 256-colour picker, and interface
> layers by selecting, nudging, resizing, adding and editing components.
> See [ROADMAP.md](ROADMAP.md).

## Components

47 component kinds, each with a typed property schema (`glyphforge kinds`
lists them, the inspector edits them):

- **Layout and text**: group, panel, divider, label, heading, artwork
- **Inputs**: button, input, password, search, textarea, checkbox, radio,
  toggle, select (open or closed)
- **Navigation**: tabs (line, boxed, pills), breadcrumbs, menu with key
  hints, toolbar, status bar
- **Data**: list, tree, table, data grid, property grid, key/value list
- **Viewers**: log (levels coloured), code (highlighted, line numbers),
  Markdown, diff
- **Feedback**: modal, dialog, notification, toast, tooltip, empty state,
  badge, tag, key hints, spinner
- **Charts**: sparkline, bar chart, histogram, line chart, gauge,
  heatmap, progress; each drawn with eighth blocks, half blocks, Braille
  or ASCII (`style`), so you see what each terminal will show

Every interactive kind has a `state` (normal, focused, hovered,
selected, disabled, loading, error, empty) to preview its states side by
side. Lists, tables and charts read their content from properties or
from **data fixtures** stored in the document: "Edit Data Fixture" in the
command palette (or the `set_data` operation) stores sample rows, items
or series under a key, and a component's `bind` property points at it.
A fixture can be a list, or a map with several fields (`columns` and
`rows` for a table).

## Prototypes

Give components an `on_activate` action and press **F12** to click
through the design: `goto:settings` shows another screen,
`open:about-dialog`, `close:…` and `toggle:…` show and hide components
(start a modal with `hidden: true`), `select:tabs=2` selects a tab or list
item, `back` returns; chain with `;` (`close:confirm;goto:main`). Tab and
Enter work without a mouse, Backspace goes back, F12 or Esc stops. What
the prototype opens is never written to the document. **Screen Graph**
(command palette) lists every screen with its incoming and outgoing
links, and validation reports links that point at nothing.

## Symbols and libraries

Select a component and run **Make Symbol** (Ctrl+K): it becomes a symbol
and an instance of it takes its place. **Insert Symbol** (Ctrl+Shift+K)
places more instances. Instances follow their master and override its
properties: set `title` on an instance to change the master root's
title, or `overrides` (`{"body.text": "Hello"}`) for nested components.
To change the master, **Detach Instance**, edit, then **Push To Symbol**:
every instance updates.

A library is an ordinary `.glyph` file. **Export Library** writes the
document's symbols, gradients, glyph palettes and data fixtures; **Import
Library** merges one into the open document (same ids are replaced, so
re-importing updates). On the command line:

```sh
glyphforge library export design.glyph kit.glyph
glyphforge library import other.glyph kit.glyph
```

**Edit Glyph Palette** stores a named set of glyphs (`Stars: ★ ☆ ✦`) in the
document; it appears as an extra brush page.

## Two editing models, one document

- **Interface Mode** — components with identity, properties, layout and
  responsive rules. Rendered on demand, validated, patchable, diffable.
- **Subcell Mode** — cells for logos, banners, separators and
  illustrations. Cells are edited directly.
- **Pixel Mode** — raster layers of real sub-cell pixels (half blocks,
  quadrants, sextants or Braille), encoded to glyphs when rendered, with
  dithering, drop shadows and gradient fills that stay adjustable.

An `artwork` component places a region of an artwork layer by layout, so
a logo can sit inside a sidebar and disappear with it on narrow terminals.

## Try it

```sh
cargo build --release
./target/release/glyphforge render examples/dashboard.glyph
./target/release/glyphforge render examples/dashboard.glyph --width 80 --height 24
./target/release/glyphforge validate examples/dashboard.glyph --width 80 --height 24
./target/release/glyphforge inspect examples/dashboard.glyph
./target/release/glyphforge examples/dashboard.glyph     # open in the editor
```

The example was generated by the CLI itself from `examples/dashboard.patch.json`:

```sh
glyphforge new dashboard.glyph --width 100 --height 28
glyphforge apply dashboard.glyph examples/dashboard.patch.json
```

## For coding agents

Every operation the editor performs is available headlessly and works on
semantic ids, not coordinates:

```sh
glyphforge inspect  design.glyph            # outline: screens, layers, components
glyphforge inspect  design.glyph --json     # the full document
glyphforge apply    design.glyph patch.json # validated, transactional, undoable
glyphforge render   design.glyph            # plain-text preview
glyphforge validate design.glyph --strict   # exit 1 on problems
glyphforge validate design.glyph --advice   # plus design suggestions (opinions)
glyphforge kinds    [KIND] [--json]         # component kinds, their properties and types
glyphforge export   design.glyph --format handoff   # YAML: tree, solved rects, tokens, fixtures
glyphforge export   design.glyph --format ratatui   # also textual, bubbletea, ink
glyphforge export   design.glyph --format ansi256   # ansi (truecolor), ansi256, ansi16, text
glyphforge import   design.glyph logo.ans       # text/ANSI art as an artwork layer (PNG: pixels)
```

The code generators turn a screen into starter code: containers become
the framework's layouts (Ratatui `Layout` constraints, Textual containers
and CSS, Lip Gloss joins, Ink `Box`es), components the closest widget;
the Ratatui output compiles as generated. In the editor, Ctrl+E exports
the screen in any of these formats.

### MCP server

`glyphforge-mcp design.glyph` speaks the Model Context Protocol over
stdio, so MCP clients can work on a design directly. For Claude Code:

```sh
claude mcp add glyphforge -- glyphforge-mcp /path/to/design.glyph --agent claude
```

Tools: `get_document` (outline, JSON or hand-off), `query_components`,
`list_kinds`, `create_component`, `apply_patch`, `render_preview` (text or
ANSI, any size), `validate_layout` (with optional suggestions) and
`export_document`. Edits are agent transactions saved to the file; with
`"propose": true` they are stored for review in the editor instead (see
below). There are no cell-by-cell tools: agents work with components and
patches.

Agents can also propose instead of apply: `glyphforge propose design.glyph
patch.json --agent "Claude Code" -m "tighten the sidebar"` validates the
patch and stores it in `design.glyph.proposals/`. The open editor notices
it ("1 agent proposal waiting"); F9 opens the review: each proposal is
previewed on the canvas with its operations listed, Enter accepts it as an
agent transaction (undoable like any edit), `x` rejects it, and applied
agent changes can be reverted later while other edits stay.

A patch:

```json
{ "operations": [
  { "op": "move",         "target": "sidebar", "x": 2, "y": 0 },
  { "op": "resize",       "target": "metrics", "width": 48, "height": 12 },
  { "op": "set_property", "target": "metrics", "property": "border", "value": "rounded" }
] }
```

Operations: `move`, `resize`, `set_property`, `set_layout`,
`create_component`, `delete_component`, `reparent`, `set_cells`,
`create_screen`, `delete_screen`, `update_screen`, `replace_screen`,
`set_data`, `set_symbol`, `delete_symbol`, `set_palette`, `set_pixels`,
`update_raster`, `replace_raster`, `set_gradient`, `delete_gradient`,
`create_layer`, `delete_layer`, `update_layer`, `reorder_layer`,
`set_theme`. The same
`Session` API drives the editor, the CLI and (later) an MCP server.

## Install

Requires a stable Rust toolchain (1.85 or newer).

```sh
cargo install --path crates/glyphforge
```

An Arch Linux `PKGBUILD`, a `.desktop` entry and the Omarchy installation
guide arrive with the packaging milestone.

## The menu and themes

**Esc** opens a menu, the way btop does: Resume, Theme, Help, Save, Save as,
New document and Quit. Arrows or `j`/`k` move, Enter chooses, Esc closes,
and the mouse works too. While something is in progress — a prompt, the
colour picker, a started shape, a selection — Esc cancels that first.

The Theme page switches the editor's own colours. Moving the selection
applies the theme straight away, so you see it on the whole UI while you
scroll; Enter keeps it, Esc puts back the one you had. Each row previews
the theme's accent, text and surface colours beside its name:

| Theme | Look |
|-------|------|
| System | Follows your Omarchy theme, or your terminal's own palette |
| Forge Dark | Warm greys with an amber accent (the designed default) |
| Paper | Light, warm paper and ink |
| Cobalt | Deep blue with a bright azure accent |
| Verdant | Forest greens |
| Lagoon | Teal and cyan |
| Amber CRT | Retro amber monochrome, in double-line DOS frames |
| Phosphor | Retro green phosphor, in plain single-line frames |
| Nebula | Violet night with neon magenta, gradient frames |
| Aurora | Northern lights: teal fading to violet frames |
| Ember | Hot metal: amber fading to crimson, heavy frames |

Nebula, Aurora and Ember draw **gradient frames**: every box border fades
from one colour to the other across the screen, like btop's gradients, with
the panel you are working in drawn brighter than the rest. A theme gets
them by defining the optional `frame-from` and `frame-to` colours.

The choice is remembered in `$XDG_STATE_HOME/glyphforge/ui-theme`;
`[theme] ui = "cobalt"` in the config pins one instead. **Ctrl+Shift+O**
(or "Use in document" in the menu) copies the editor's theme into the
document, as an undoable change, so the design uses the colours you see.

**Design theme** in the menu sets the document's own theme from the same
list (one undoable step). **Edit this theme…** on either page opens the
theme editor: every token with its swatch, Enter types a colour, `c`
opens the colour picker, `n` names the theme, `s` saves. Editing the
editor theme previews on the chrome and saves `~/.config/glyphforge/themes/<id>.toml`
(theme files there are listed with the built-in themes); editing the
design theme previews on the canvas and saves into the document.

## Editor keys

**F5** (Check Design) lists what validation finds on the screen (clipped,
truncated, overlapping, invisible or low-contrast text, cut wide glyphs,
components too small for their content, controls covered by others) and,
separately labelled, design suggestions (uneven spacing, near-misses in
alignment, missing or competing headings, crowded text); Enter jumps to
the component.

**Ctrl+Space** (or Ctrl+P) opens the command palette: every action by
name, fuzzy matched over titles, keywords and aliases ("togall" finds
Toggle All Panels), recently used commands first. It also lists what the
moment offers: component kinds to add on interface layers, layers to
switch to, editor themes, and for a cell selection Flip Horizontally /
Vertically, Swap Colours and Make Component From Selection (an `artwork`
component showing those cells, placed where they are).

All bindings are configurable. The mode follows the active layer: an
interface layer puts the editor in INTERFACE mode, an artwork layer in
SUBCELL mode (Ctrl+PgUp/PgDn switch layers).

Interface mode:

| Key | Action |
|-----|--------|
| `]` / `[`, click | Select the next / previous component, or the one under the mouse |
| Shift+click, Ctrl+A | Add to the selection / select everything on the layer; edits apply to all |
| `u` / `d` (Alt+Up/Down) | Select the parent / first child |
| `p` | Move the selection into another container (or to the top level) |
| Ctrl+F | Find a component, screen, layer or text (jumps there) |
| Arrows | Nudge the selection (or move the cursor when nothing is selected) |
| Shift+Arrows | Resize the selection |
| Mouse drag | Move; the `◆` corners and `◇` edge handles resize from that side; edges and centres snap to other components (dotted guides) |
| Drag on empty space | Rubber band: selects the components fully inside |
| Double click | Edit the component's text (`text`, `label` or `title`) |
| Right click | Context menu: edit, duplicate, copy, cut, move into, z-order, align, delete |
| Shift+Home / End, Shift+PgUp / PgDn | Bring to front / send to back, forward / backward |
| `a` | Add a component at the cursor (inside the selection if any): `kind [name]` |
| Enter | Edit a property of the selection: `key=value` (empty value removes) |
| `i`, Tab, click | Focus the inspector |
| Shift+H / L / K / J | Align left / right / top / bottom (one component: inside its parent) |
| Shift+C / M | Align centres horizontally / middles vertically |
| Shift+D / V | Distribute horizontally / vertically (three or more) |
| Shift+W / E / Z | Equal width / height / size (the last selected sets it) |
| Shift+S, Shift+G | Toggle snapping to components; cycle the snap grid (off, 2, 4, 8) |
| Delete | Delete the selection |
| Esc | Clear the selection |

The inspector (right panel) lists every layout field (placement, size,
limits, margin; direction, gap, columns, alignment and padding for
containers) and every property the component's kind reads, with its
default. Up/Down pick a row, Space and Left/Right cycle toggles, choices
and sizes (`content` → `fill` → `fixed` → `percent` → `flex`), Enter
edits with suggestions (border families, colour tokens, layer ids),
Delete resets to the default, Esc returns to the canvas. A click picks a
row, a second click edits or cycles it.

Layers and screens: the top of the right panel switches screens (`◂ ▸`)
and lists the layers, top-most first. Click a layer to edit it, the `●`
to show or hide it, the lock column to lock it. With the list focused
(Alt+Y, Tab, or a click):

| Key | Action |
|-----|--------|
| Up / Down | Activate the layer above / below |
| Shift+Up / Down | Move the layer up / down the stack |
| Space or `v`, `l`, `s` | Show/hide, lock/unlock, isolate (again restores) |
| `n`, `r`, `d`, `m`, Delete | New, rename, duplicate, merge down, delete |
| Left / Right, Shift+N / R / S | Previous / next screen; new, rename, resize screen |

A right click on a layer opens its menu. Shift+wheel scrolls sideways.
Alt+PgUp/PgDn switch screens anywhere; screen and layer operations are
also in the command palette. F7 previews the screen at 60×20, 80×24,
100×30, 120×40 and 160×50 (responsive rules and layout follow; the screen
keeps its size), F8 shows a minimap (click it to jump), Ctrl+F finds
components, screens, layers and text on artwork layers across the
document.

Subcell mode: the left panel is the palette. Click a tool, a box style
(`┌ ╭ ╔ ┏ +` under the tools), a glyph or a colour swatch (left click =
foreground, right click = background, `+` opens the full 256-colour
picker). `‹` / `›` or the mouse wheel over the Brushes block turn pages;
clicking the page name searches every brush and glyph.

The first pages hold semantic brushes; every tool uses the selected one:

- **Lines** (single, rounded, heavy, double, dashed, heavy dashed): lines
  join by themselves; dashed lines use `┄ ┆` on straight runs and solid
  glyphs at corners and junctions (Unicode has no dashed corners).
  Strokes turn corners, crossing a line makes `┼`, ending on one makes `├`,
  a double line across a single one makes `╪`. The Rect tool draws joined
  frames, the Line tool draws an elbow, and the Eraser unjoins neighbours
  while a line brush is selected. The Box tool joins in its box style.
- **Patterns**: shade (each pass darkens `░ ▒ ▓ █`), checker, rows,
  columns, dots, and a gradient from the foreground to the background
  colour. They work with strokes, the Line and Rect tools and Fill, and use
  the brush glyph.
- **Buttons**: `[OK]`, `<OK>`, `«OK»`, `▕OK▏`, pill `▐OK▌`, solid, drop
  shadow, link, and boxed buttons in single, rounded, double and heavy
  lines. Filled buttons use the background colour as the face.
- **UI stamps**: checkbox, radio, toggle, progress bar and scrollbar (a
  numeric label sets the fill), key hint (`q quit`), tab, titled rule,
  spinner.

Stamps use the stamp label (`N`, default "OK"): click places one at its
natural size, dragging with the Rect or Line tool sizes it. Stamps are
cells, not components; they are for mock-ups on artwork layers.

The remaining pages hold glyphs: blocks, quadrants, sextants, light, heavy
and double box lines and their junctions, triangles, shapes, symbols, UI
symbols, arrows, Braille, ASCII and math. Choosing a tool on an interface layer switches
to an artwork layer, adding a "Paint" layer if the screen has none visible.

| Key | Action |
|-----|--------|
| Alt+T / B / E / L / R / X / F / I / V | Text, Brush, Eraser, Line, Rect, Box, Fill, Picker, Select tool |
| T B E L R X F I V | The same, without Alt, while a painting tool is active |
| = / - (Alt+= / Alt+-) | Brush size (a square of up to 9×9 cells for glyph, pattern and eraser strokes) |
| Alt+Shift+B / I / U / K | Toggle bold, italic, underline, strikethrough on the brush (all six attributes are clickable under the box styles) |
| Click, drag | Apply the tool: strokes, shapes from press to release, fill, pick |
| Space | Apply at the cursor (shapes: first Space sets the corner, second draws) |
| Right click on the canvas | Pick glyph and colours from a cell (inside a selection: the selection menu) |
| Any other character | Becomes the brush glyph (painting tools) |
| C / Shift+C (Alt+C / Alt+Shift+C) | Colour picker for foreground / background |
| S / D (Alt+S / Alt+D) | Swap colours / reset to the terminal default |
| G (Alt+G) | Search brushes and glyphs (e.g. "double", "button", "sextant") |
| N (Alt+N) | Edit the stamp label |
| [ / ] (Alt+, / Alt+.) | Previous / next brush page |
| Shift+X (Alt+Shift+X) | Cycle the box style: single, rounded, double, heavy, ASCII |
| Ctrl+A | Select the whole layer (Select tool) |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy, cut, paste the selected cells (paste goes to the cursor) |
| Delete | Clear the selected cells |
| Esc | Cancel a started shape, then clear the selection |

The Select tool draws a marquee; dragging inside it moves the cells (one
undo step), transparent cells of the moved block leave what is beneath.
On interface layers Ctrl+C / X / V copy, cut and paste components with
fresh ids, and Ctrl+D duplicates the selection in place. Copies also reach
the system clipboard: cells as text, components as JSON, through OSC 52
(and `wl-copy` or `xclip` when available).

In the colour picker: arrows move, Enter picks, Tab switches foreground and
background, `r` switches between the 256-colour grid and a truecolour
spectrum (36 hues at nine lightness levels, a muted row, 24 greys), `#`
types a name, index or `#rrggbb`, `d` picks the terminal default. F6
previews the design at 256 and 16 colours (the editor chrome is
unaffected); colour properties in the inspector show a swatch and cycle
through the theme tokens with Space. Brush strokes, shapes and fills are single undo steps.

Pixel mode: a pixel layer (Alt+P, or choosing the Gradient tool) stores
real sub-cell pixels. You paint pixels; for every cell Glyphforge picks the
glyph and the two colours that show them best, over whatever lies beneath.

| Mode | Pixels per cell | Glyphs |
|------|-----------------|--------|
| half | 1×2 | `▀ ▄ █`, two full colours per cell |
| quadrant | 2×2 | `▘ ▚ ▟ …` |
| sextant (default) | 2×3 | `🬀`–`🬻` |
| braille | 2×4 | `⣿`, one colour per cell |
| octant | 2×4 | Unicode 16 octants, two colours per cell (needs a recent font) |

- **Tools**: Brush and Eraser (brush size Alt+= / Alt+-), Line, Rect, Box
  (filled rectangle), Fill, Pick and Gradient all work on pixels.
- **Mouse**: in Ghostty, kitty, WezTerm, foot and inside herdr the
  terminal reports pixel positions (mode 1016), so a click hits the exact
  sub-cell pixel. Elsewhere a click paints the whole cell;
  Shift+arrows move a pixel cursor and Space paints at it.
- **Settings** (palette page "Dither", `O` for all of them as text): mode,
  dithering (`none`, `ordered` Bayer patterns across sub-cell pixels,
  `shade` glyphs `░▒▓`, both, or error diffusion with `floyd-steinberg`
  or `atkinson`), palette limit (truecolor, 256, 16 colours).
  Changing the mode resamples the pixels and is undoable losslessly.
- **Drop shadow** (page "Shadow", `Shift+S`): a live effect computed from
  the painted pixels: offset, blur, opacity and colour. Its soft edges use
  the layer's dithering: hard, ordered patterns, shade glyphs, or both.
- **Gradients** (Gradient tool, pages "Gradient" and "Gradients"): drag
  from start to end; the fill covers the connected area you pressed in.
  Linear or radial; smooth, hard bands, dithered sub-cells or shade glyphs.
  Gradients are saved in the document with their stops. `Shift+G` opens
  the gradient editor: the bar with its stops, drag a stop or move it with
  Shift+arrows, Enter sets its colour, `a`/`x` add and remove stops, `o`
  blends in OKLab instead of RGB (even lightness, no muddy middles), `r`
  reverses, `t` edits it as text (`Sunset: #ff5f6d 0, #ffc371 100`), `s`
  saves. Six presets are offered and copied into the document when used.
- **Selection** (Select tool `V`): a pixel-precise marquee; drag inside to
  move, Ctrl+C / X / V copy, cut and paste pixels, Delete clears.
- **Images**: "Import Image" in the command palette (or
  `glyphforge import design.glyph logo.png --mode braille --cols 30 --rows 10`)
  fits a PNG at the cursor as a new pixel layer, keeping its proportions in
  every mode. An `artwork` component can show a pixel layer too.

| Key (pixel layers) | Action |
|-----|--------|
| Alt+P | New pixel layer (choose the mode) |
| A (Alt+A) | Gradient tool |
| Shift+Arrows | Move the pixel cursor |
| M (Alt+M) | Cycle the mode: half, quadrant, sextant, braille |
| Shift+D (Alt+Shift+D) | Cycle dithering |
| Shift+S (Alt+Shift+S) | Toggle the drop shadow |
| O (Alt+O) | All layer settings as `key=value` |
| Shift+G (Alt+Shift+G) | Edit or create a gradient |
| Shift+A (Alt+Shift+A) | Cycle the gradient style |
| Right click | Pick the pixel colour |

Subcell mode (Text tool) and global:

| Key | Action |
|-----|--------|
| Arrows, Home, End, PgUp, PgDn | Move the cursor |
| Printable characters | Insert on the active artwork layer (wide glyphs advance two columns) |
| Backspace / Delete | Erase left / erase at cursor |
| Enter | Next row, back to the column where typing started |
| Ctrl+Z / Ctrl+Y | Undo / redo (a typed word is one step) |
| Ctrl+S / Ctrl+Shift+S | Save / save as |
| Ctrl+N | New document |
| Ctrl+PgUp / Ctrl+PgDn | Switch the active layer |
| Ctrl+Shift+O | Copy the editor theme into the document |
| Ctrl+Shift+T | Reload the editor theme |
| Ctrl+Arrows, mouse wheel | Scroll the viewport |
| Esc | Cancel what is in progress, or open the menu |
| F10 | Open the menu |
| F1 | Help overlay with every binding |
| F2 / F3 / F4 | Toggle left / right / all panels |
| Tab / Shift+Tab | Cycle focus |
| Ctrl+Q | Quit (asks again with unsaved changes) |

## Configuration

`$XDG_CONFIG_HOME/glyphforge/config.toml`; every field is optional and
`glyphforge --print-default-config` prints the full default.

```toml
[canvas]
default_width = 80
default_height = 24

[ui]
show_left_panel = true
show_right_panel = true
vim_navigation = false      # h/j/k/l/0/$ in normal mode, i enters insert mode
snap = true                 # snap dragged components to neighbours (Shift+S toggles)
snap_grid = 0               # snap grid in cells while dragging (Shift+G cycles 0 2 4 8)
color_mode = "auto"         # auto | truecolor | ansi256 | ansi16

[theme]
source = "auto"             # auto | omarchy | builtin
# ui = "cobalt"             # pin a theme: forge-dark paper cobalt verdant
                            # lagoon amber-crt phosphor nebula aurora ember
                            # minimal-dark minimal-light terminal, "system"

[mouse]
enabled = true
pixel_precision = "auto"    # auto | on | off: pixel-precise reports for Pixel Mode

[keys]
"ctrl+q" = "quit"
"f1" = "none"               # unbind
"paint:p" = "tool-brush"    # paint: binds only while a painting tool is active
```

Logs go to `$XDG_STATE_HOME/glyphforge/glyphforge.log`; set
`GLYPHFORGE_LOG=debug` for detail.

Autosave (`[autosave] enabled = true`, `interval_seconds = 60`) writes
unsaved work to a hidden sibling (`.design.glyph.autosave`; unnamed
documents go to the state directory). The design file itself is only
written when you save, and saving removes the autosave. When Glyphforge
starts on a file whose autosave is newer, it offers to restore or discard
the autosaved changes.

## Omarchy

Under Omarchy Linux, Glyphforge reads the active theme from
`~/.config/omarchy/current/theme/colors.toml` and `theme.name`, the files
that `omarchy-theme-set` maintains, and maps them onto its style tokens for
the editor chrome. Ctrl+Shift+O copies that theme into the document.
Nothing under `~/.config/omarchy` or `/usr/share/omarchy` is written.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
python3 scripts/pty_smoke_test.py target/release/glyphforge /tmp/glyphforge-smoke
```

Read [ARCHITECTURE.md](ARCHITECTURE.md) before changing module boundaries,
and [ROADMAP.md](ROADMAP.md) for the plan and its status.

```
crates/glyphforge-core   document model, layout, patches, history, rendering,
                         validation, project format, application API
crates/glyphforge        the editor and CLI binary
examples/                example projects and the patch that builds them
scripts/                 pty smoke test
```

## License

MIT, see [LICENSE](LICENSE).
