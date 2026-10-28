# Installing Glyphforge

Glyphforge is two programs: `glyphforge` (the editor and the command line)
and `glyphforge-mcp` (the MCP server for coding agents). Both are plain
binaries without runtime dependencies besides libc.

## From source (any Linux, macOS)

Requires a stable Rust toolchain, 1.85 or newer (`rustup` or your
distribution's `rust` package).

```sh
git clone https://github.com/plasticparticle/glyphforge
cd glyphforge
scripts/install.sh
```

The script builds a release, installs both binaries into `~/.local/bin`
(override with `PREFIX=/usr/local`), a desktop entry and AppStream
metadata into `~/.local/share`, and on Omarchy the theme hook (below).
`scripts/install.sh --uninstall` removes exactly those files; designs,
configuration and theme files stay.

Only the binaries: `cargo install --path crates/glyphforge` and
`cargo install --path crates/glyphforge-mcp`.

## Arch Linux

`packaging/arch/PKGBUILD` builds, tests and installs both binaries, the
desktop entry, the AppStream metadata, the examples and the Omarchy hook
(into `/usr/share/glyphforge/omarchy/`):

```sh
cd packaging/arch
makepkg -si
```

Optional: `wl-clipboard` (Wayland) or `xclip` (X11) so copies reach the
desktop clipboard also in terminals that ignore OSC 52.

## Omarchy

Glyphforge detects Omarchy and uses its current theme for the editor
(the `system` theme in the Esc menu). It notices theme changes by itself
within a second. For an instant switch, install the hook:

```sh
install -Dm755 packaging/omarchy/theme-set ~/.config/omarchy/hooks/theme-set
```

If you already have a `theme-set` hook, add this line to it instead:

```sh
sh /path/to/glyphforge/packaging/omarchy/theme-set
```

The hook only touches `~/.local/state/glyphforge/reload-theme`; nothing in
Omarchy's own files is written. All four terminals Omarchy ships with
(Alacritty, Ghostty, Kitty, foot) are detected; Ghostty, Kitty and foot
also give pixel-precise mouse input for pixel layers.

## Terminals

Glyphforge needs a UTF-8 locale and works best with truecolor. It
detects colour depth, the kitty keyboard protocol and pixel mouse
reports, and degrades otherwise (`glyphforge --print-caps` shows what it
found). Octant pixel mode needs a font with Unicode 16 block glyphs.

## Coding agents

Register the MCP server with your agent, for example Claude Code:

```sh
claude mcp add glyphforge -- glyphforge-mcp /path/to/design.glyph --agent claude
```

Or let agents use the command line (`glyphforge inspect`, `apply`,
`propose`, `render`, `validate`, `export`); see the README.
