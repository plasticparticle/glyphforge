# TUIForge Roadmap

Development proceeds in small, independently testable milestones. Every
milestone follows the same pipeline before it is considered done:

1. implement
2. write tests
3. `cargo test --workspace`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo fmt --all --check`
6. fix every warning and error
7. update `ARCHITECTURE.md`, `README.md` and this file

The repository must never be left in a state where the pipeline fails.

Status legend: `[x]` done, `[~]` partially done (details listed), `[ ]` open.

---

## Milestone 0 — Skeleton, terminal lifecycle, architecture  `[x]`

- [x] Cargo workspace: `tuiforge-core` (no Ratatui) and `tuiforge` (app)
- [x] `ARCHITECTURE.md`, `ROADMAP.md`, `README.md`, `LICENSE`
- [x] Terminal guard: raw mode, alternate screen, mouse capture, bracketed
      paste, focus events, kitty keyboard flags; restored on drop and panic
- [x] Capability detection: colour depth, keyboard enhancement, Unicode, mouse
- [x] `Action` enum, action descriptors, `App::dispatch` as single entry point
- [x] Key model, key-chord parsing, default keymap, config overrides
- [x] Event loop with tick, resize handling, focus events
- [x] XDG path resolution and TOML config with defaults
- [x] Logging to `$XDG_STATE_HOME/tuiforge/tuiforge.log` (`TUIFORGE_LOG`)
- [x] UI theme roles, built-in ANSI theme
- [x] Omarchy detection and `colors.toml` mapping (loaded at start, on
      focus gain and via `reload-theme`); pulled forward from M14 because
      the UI palette needs it from day one
- [x] Layout: header, hideable left/right panels, canvas, status bar
- [x] Errors surfaced in the status bar, never by tearing down the terminal
- [x] pty smoke test (`scripts/pty_smoke_test.py`): lifecycle, typing,
      help overlay, Omarchy fixture, malformed config, narrow terminal

## Milestone 1 — Canvas, cells, Unicode, rendering  `[~]`

- [x] `Grapheme` with cluster validation and width (1 or 2)
- [x] `Color`, `Attributes`, `CellStyle`, `Cell`, `CellContent`
- [x] `Canvas` with wide-glyph invariants and delta-returning `put`
- [x] `Layer`, `Document` (single default layer, active layer)
- [x] Compositor with wide-glyph repair
- [x] Canvas view: viewport scrolling, clipping, cursor follows viewport
- [x] Cursor movement, typing places graphemes, Backspace/Delete erase
- [x] Tests: width, wide-glyph insert/overwrite/edge, compositing,
      repair, clipping, view rendering via `TestBackend`
- [ ] Colour-depth degradation for document colours in the view
- [ ] Optional `cjk_ambiguous_wide` width mode
- [ ] Per-row dirty tracking for the composite cache

## Milestone 2 — Cursor, pencil, eraser, text  `[ ]`

- `Tool` trait, `ToolRegistry`, `ToolContext`; tools panel shows real tools
- Pencil (current glyph + style), Eraser, Text (typing with wrap-less flow)
- Cursor modes: insert vs overwrite; Vim-style navigation option
- Tests: tool routing, pencil over wide glyphs, eraser tail handling

## Milestone 3 — Layers  `[ ]`

- create / delete / rename / duplicate / reorder / show-hide / lock / merge down
- Layers panel becomes interactive (keyboard first, mouse in M8)
- Locked layers reject edits with a status message
- Tests: every operation, merge-down compositing, z-order

## Milestone 4 — Selection and clipboard  `[ ]`

- Rectangular selection model; select/deselect/move/copy/cut/paste/duplicate/
  delete/fill/replace glyph/replace fg/replace bg
- Internal clipboard; OSC 52 and `wl-copy`/`xclip` best-effort backends
- Tests: move/copy across layer boundaries, wide glyphs at selection edges,
  clipping when pasting near the border

## Milestone 5 — Undo/redo  `[ ]`

- `history::Command`, `Transaction`, `History` with depth limit
- Every tool and layer/selection operation goes through transactions
- Stroke grouping (press..release = one transaction), saved-marker dirty state
- Tests: apply/revert symmetry, grouping, depth limit, dirty tracking

## Milestone 6 — Lines, rectangles, smart box drawing  `[ ]`

- `boxdraw` topology model: per-cell edge set with style (ascii, single,
  double, heavy, rounded); glyph resolution table; smart merge toggle
- Line, H-line, V-line, Rectangle, Filled rectangle, Box, Rounded box tools
- Box-style palette
- Tests: intersection table (`─`+`│` = `┼`, T-junctions, corners), mixed
  styles, merge disabled

## Milestone 7 — Colours and palettes  `[ ]`

- Foreground/background selectors, recent colours, palette colours, colour
  picker (16 / 256 / RGB), swap, reset to default
- Preview modes ANSI 16 / 256 / TrueColor via quantisers
- Tests: quantisation, preview does not mutate document

## Milestone 8 — Mouse interaction  `[ ]`

- Click, double click, drag, selection handles, scrollbars, wheel scrolling,
  panel and palette selection
- Hit-testing derived from the layout, not duplicated
- Tests: hit-test math, drag-to-selection

## Milestone 9 — Command palette and fuzzy actions  `[ ]`

- Ctrl+Space (kitty protocol / NUL) plus configurable fallback (Ctrl+P)
- `nucleo-matcher` based fuzzy matching over titles, keywords, aliases;
  recency weighting; shortcut display; mouse selection
- Tests: "border" matches Draw Box, "layer up" ranks Move Layer Up first

## Milestone 10 — Project persistence  `[ ]`

- `.tuiforge` JSON envelope with `schema_version`, migrations chain
- New/Open/Save/Save As, recent files, `tuiforge file.tuiforge`
- Autosave to `*.autosave`, recovery prompt, never overwriting the original
- Tests: round trip, migration from a fixture v1 file, corrupted file error

## Milestone 11 — ANSI import/export  `[ ]`

- Exporters: plain UTF-8, ASCII-safe, ANSI 16 / 256 / TrueColor, clipboard
- SGR delta minimisation, final reset
- Importers: plain text, ANSI (SGR + basic cursor movement)
- Tests: minimal escape output, `cat`-safe termination, import round trip

## Milestone 12 — Art mode / sub-cell drawing  `[ ]`

- `art` raster (2x2 quadrant, 2x4 Braille, half blocks, shading)
- Raster -> glyph conversion, separate from the cell model
- Tests: every quadrant pattern, Braille bit mapping

## Milestone 13 — Semantic TUI components  `[ ]`

- Component descriptors rendering to cells: Panel, Border, Label, Button,
  Input, Checkbox, Radio, Tabs, Table, List, Scrollbar, Progress, Modal,
  Status bar, Menu
- Stored as metadata in the project so they can become editable later
- Tests: rendering snapshots per component

## Milestone 14 — Omarchy integration and packaging  `[ ]`

- Theme mtime polling on tick; documented `theme-set.d` hook
- Terminal-specific checks for the four Omarchy terminals
- `.desktop` entry, AppStream metadata, PKGBUILD, install script, README
  installation section, example projects
- Tests: desktop file validation in CI, colors.toml fixtures for several
  Omarchy themes
