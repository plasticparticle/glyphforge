#!/bin/sh
# Builds and installs Glyphforge for the current user (~/.local), with the
# desktop entry, AppStream metadata and, on Omarchy, the theme hook.
#   scripts/install.sh            install
#   scripts/install.sh --uninstall
set -eu
cd "$(dirname "$0")/.."
prefix="${PREFIX:-$HOME/.local}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
hook="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/hooks/theme-set"

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$prefix/bin/glyphforge" "$prefix/bin/glyphforge-mcp" \
        "$data/applications/glyphforge.desktop" \
        "$data/metainfo/io.github.plasticparticle.glyphforge.metainfo.xml"
  if [ -f "$hook" ] && grep -q glyphforge "$hook"; then rm -f "$hook"; fi
  echo "Glyphforge removed (your designs, config and themes are kept)."
  exit 0
fi

command -v cargo >/dev/null || { echo "cargo is required: https://rustup.rs" >&2; exit 1; }
cargo build --release --workspace
install -Dm755 target/release/glyphforge "$prefix/bin/glyphforge"
install -Dm755 target/release/glyphforge-mcp "$prefix/bin/glyphforge-mcp"
install -Dm644 packaging/glyphforge.desktop "$data/applications/glyphforge.desktop"
install -Dm644 packaging/io.github.plasticparticle.glyphforge.metainfo.xml \
  "$data/metainfo/io.github.plasticparticle.glyphforge.metainfo.xml"

if [ -d "${XDG_CONFIG_HOME:-$HOME/.config}/omarchy" ]; then
  if [ -e "$hook" ] && ! grep -q glyphforge "$hook"; then
    echo "An Omarchy theme-set hook exists already; add this line to it:"
    echo "  sh $(pwd)/packaging/omarchy/theme-set"
  else
    install -Dm755 packaging/omarchy/theme-set "$hook"
    echo "Installed the Omarchy theme hook: $hook"
  fi
fi
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "Add $prefix/bin to your PATH." ;;
esac
echo "Installed: $prefix/bin/glyphforge and glyphforge-mcp"
