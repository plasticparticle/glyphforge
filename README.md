# GlyphForge

A full-screen terminal application for visually designing terminal user
interfaces and ANSI/Unicode artwork. Think ACiDDraw meets a lightweight
Figma, built for the terminal, with first-class Omarchy Linux integration.

GlyphForge is written in Rust on top of Ratatui and Crossterm. The document
model (`tuiforge-core`) is independent of the rendering library.

> Status: early development. Milestone 0 (skeleton, terminal lifecycle,
> architecture) is complete and Milestone 1 (canvas, cells, Unicode,
> rendering) is functional: you can type into an editable canvas with
> correct double-width handling. Saving, tools, layers UI, selection and
> everything else follow the plan in [ROADMAP.md](ROADMAP.md).

## Build

Requires a stable Rust toolchain (1.85 or newer).

```sh
cargo build --release
./target/release/tuiforge
```

Install into `~/.cargo/bin`:

```sh
cargo install --path crates/tuiforge
```

An Arch Linux `PKGBUILD`, a `.desktop` entry and the Omarchy installation
guide arrive with Milestone 14.

## Usage

```sh
tuiforge                       # new 80x24 document
tuiforge --width 120 --height 40
tuiforge dashboard.tuiforge    # a path that does not exist yet becomes the save target
tuiforge --print-caps          # detected terminal capabilities, paths, Omarchy status
tuiforge --print-default-config
```

Default keys (all configurable):

| Key | Action |
|-----|--------|
| Arrows, Home, End, PgUp, PgDn | Move the cursor |
| Printable characters | Insert at the cursor (double-width glyphs advance two columns) |
| Backspace / Delete | Erase left / erase at cursor |
| Enter | Next row, back to the column where typing started |
| Ctrl+Arrows, mouse wheel | Scroll the viewport |
| Left click | Place the cursor |
| F1 | Help overlay with every binding |
| F2 / F3 / F4 | Toggle left / right / all panels |
| Tab / Shift+Tab | Cycle focus between panels and canvas |
| Ctrl+Shift+T | Reload the theme |
| Ctrl+Q | Quit (asks again when there are unsaved changes) |
| Esc | Cancel / close help / leave insert mode (Vim navigation) |

Keys for not-yet-implemented actions (save, undo, palette, ...) are
already bound and report "not implemented yet" in the status bar.

## Configuration

`$XDG_CONFIG_HOME/tuiforge/config.toml` (default `~/.config/tuiforge/config.toml`).
Every field is optional; `tuiforge --print-default-config` prints the full
default file.

```toml
[canvas]
default_width = 80
default_height = 24

[ui]
show_left_panel = true
show_right_panel = true
vim_navigation = false      # h/j/k/l/0/$ in normal mode, i enters insert mode
color_mode = "auto"         # auto | truecolor | ansi256 | ansi16

[theme]
source = "auto"             # auto | omarchy | builtin

[mouse]
enabled = true

[keys]
"ctrl+q" = "quit"
"f1" = "none"               # unbind
```

Action names are the kebab-case identifiers shown by `--print-default-config`
and in `crates/tuiforge/src/actions/mod.rs`.

Logs go to `$XDG_STATE_HOME/tuiforge/tuiforge.log`; set `TUIFORGE_LOG=debug`
for more detail.

## Omarchy

When running on Omarchy Linux, TUIForge reads the active theme from
`~/.config/omarchy/current/theme/colors.toml` and `theme.name`, the public
files that `omarchy-theme-set` maintains, and maps the semantic colours
onto its UI. The theme is reloaded when the terminal window regains focus
and via Ctrl+Shift+T. Nothing under `~/.config/omarchy` or
`/usr/share/omarchy` is ever written. Outside Omarchy, or with
`theme.source = "builtin"`, a theme built from the terminal's own ANSI
palette is used.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
python3 scripts/pty_smoke_test.py target/release/tuiforge /tmp/tuiforge-smoke
```

Read [ARCHITECTURE.md](ARCHITECTURE.md) before changing module boundaries
and [ROADMAP.md](ROADMAP.md) for the milestone plan and its status.

Repository layout:

```
crates/tuiforge-core   document model, compositing (no Ratatui dependency)
crates/tuiforge        the application binary
scripts/               pty smoke test
```

## License

MIT, see [LICENSE](LICENSE).
