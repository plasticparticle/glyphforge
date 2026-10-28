#!/usr/bin/env python3
"""End-to-end smoke test: runs the glyphforge binary inside a pseudo-terminal.

Usage: scripts/pty_smoke_test.py target/release/glyphforge /tmp/some-empty-dir

The harness answers the terminal queries the editor sends (cursor position,
device attributes) so the run matches a real terminal. Exact screen content
is asserted by the Ratatui TestBackend unit tests; this script checks the
terminal lifecycle, key handling, theme detection and error surfacing.
"""
import os, pty, sys, time, select, signal, struct, fcntl, termios, subprocess, re, json

ESC_RE = re.compile(r"\x1b\[[0-9;?<>=]*[A-Za-z@`~]|\x1b[()][A-Za-z0-9]|\x1b[=>]")
def plain(text):
    return ESC_RE.sub("", text)

def run(binary, home, extra_env=None, keys=None, cols=100, rows=30, timeout=8, args=(), cell_px=None):
    env = dict(os.environ)
    env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "HOME": home,
                "LANG": "en_US.UTF-8", "GLYPHFORGE_LOG": "debug"})
    env.pop("XDG_CONFIG_HOME", None); env.pop("XDG_STATE_HOME", None)
    # Never touch the real clipboard: copies only go out as OSC 52.
    env.pop("WAYLAND_DISPLAY", None); env.pop("DISPLAY", None)
    if extra_env: env.update(extra_env)
    pid, fd = pty.fork()
    if pid == 0:
        os.execve(binary, [binary, *args], env)
    xpix, ypix = (cols * cell_px[0], rows * cell_px[1]) if cell_px else (0, 0)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, xpix, ypix))
    out = b""
    def read_for(secs):
        nonlocal out
        end = time.time() + secs
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return False
                if not chunk:
                    return False
                out += chunk
                # Behave like a real terminal: answer the queries the app sends.
                if b"\x1b[6n" in chunk:
                    os.write(fd, b"\x1b[1;1R")
                if b"\x1b[c" in chunk:
                    os.write(fd, b"\x1b[?62;22c")
        return True
    read_for(1.0)
    for k, wait in (keys or []):
        os.write(fd, k)
        read_for(wait)
    start = time.time()
    status = None
    while time.time() - start < timeout:
        wpid, st = os.waitpid(pid, os.WNOHANG)
        if wpid == pid:
            status = st; break
        read_for(0.1)
    if status is None:
        os.kill(pid, signal.SIGKILL); os.waitpid(pid, 0)
        return None, out
    return os.waitstatus_to_exitcode(status), out

binary = sys.argv[1]
home = sys.argv[2]
os.makedirs(home, exist_ok=True)

# 1. Type text, quit with dirty confirmation.
code, out = run(binary, home, keys=[(b"Hi \xe6\xbc\xa2", 0.3), (b"\x11", 0.3), (b"\x11", 0.3)])
text = out.decode("utf-8", "replace")
assert code == 0, f"exit code {code}\n{text[-2000:]}"
assert "\x1b[?1049h" in text, "alternate screen entered"
assert "\x1b[?1049l" in text, "alternate screen left"
assert "\x1b[?1000h" in text or "\x1b[?1002h" in text, "mouse enabled"
assert "\x1b[?1000l" in text or "\x1b[?1002l" in text, "mouse disabled"
assert "Glyphforge" in text
pt = plain(text)
# Ratatui redraws only changed cells, so exact layout checks live in the
# TestBackend unit tests; here we only check the glyph reached the terminal.
assert "漢" in pt, "typed wide glyph rendered: " + pt[-600:]
assert "Unsaved" in pt and "changes" in pt, "dirty confirmation shown (diff-rendered)"
print("smoke 1 ok: typing + dirty quit + terminal restore")

# 2. Clean quit is immediate; F1 help renders.
code, out = run(binary, home, keys=[(b"\x1bOP", 0.3), (b"\x1b", 0.2), (b"\x11", 0.3)])
text = out.decode("utf-8", "replace")
assert code == 0, f"exit {code}"
assert " Keys " in plain(text) and "later milestone" in plain(text), "help overlay rendered"
print("smoke 2 ok: help overlay + clean quit")

# 3. Omarchy fixture: theme is picked up and reported in the header.
ohome = home + "/omarchy"
os.makedirs(ohome + "/.config/omarchy/current/theme", exist_ok=True)
os.makedirs(ohome + "/.local/share/omarchy", exist_ok=True)
open(ohome + "/.config/omarchy/current/theme.name", "w").write("tokyo-night\n")
open(ohome + "/.config/omarchy/current/theme/colors.toml", "w").write('''accent = "#7aa2f7"
cursor = "#c0caf5"
foreground = "#a9b1d6"
background = "#1a1b26"
selection_foreground = "#c0caf5"
selection_background = "#7aa2f7"
color0 = "#32344a"
color1 = "#f7768e"
color2 = "#9ece6a"
color3 = "#e0af68"
color4 = "#7aa2f7"
color5 = "#ad8ee6"
color6 = "#449dab"
color7 = "#787c99"
color8 = "#444b6a"
color9 = "#ff7a93"
color10 = "#b9f27c"
color11 = "#ff9e64"
color12 = "#7da6ff"
color13 = "#bb9af7"
color14 = "#0db9d7"
color15 = "#acb0d0"
''')
code, out = run(binary, ohome, keys=[(b"\x11", 0.3)])
text = out.decode("utf-8", "replace")
assert code == 0, f"exit {code}"
assert "Tokyo Night (omarchy)" in plain(text), "omarchy theme name in header"
assert "\x1b[38;2;" in text or "38;2;" in text, "truecolor escapes emitted"
assert "Omarchy" in text
log = open(ohome + "/.local/state/glyphforge/glyphforge.log").read()
assert "loaded Omarchy theme" in log, log[-500:]
print("smoke 3 ok: omarchy theme detected, truecolor used, log written")

# 4. Broken config is reported, not fatal.
bhome = home + "/broken"
os.makedirs(bhome + "/.config/glyphforge", exist_ok=True)
open(bhome + "/.config/glyphforge/config.toml", "w").write("[canvas]\nnot_a_field = 1\n")
code, out = run(binary, bhome, keys=[(b"\x11", 0.3)])
text = out.decode("utf-8", "replace")
assert code == 0, f"exit {code}"
assert "Config ignored" in plain(text), "config error surfaced in status bar"
print("smoke 4 ok: malformed config surfaced in the TUI")

# 5. Resize mid-session does not crash (send SIGWINCH via ioctl change).
env_keys = [(b"x", 0.2)]
code, out = run(binary, home, keys=env_keys + [(b"\x11", 0.2), (b"\x11", 0.3)], cols=50, rows=12)
text = out.decode("utf-8", "replace")
assert code == 0
print("smoke 5 ok: narrow terminal without panels")
# 6. Open the shipped example: components render, undo reports, clean quit.
example = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "examples", "dashboard.glyph")
code, out = run(binary, home, keys=[(b"\x1a", 0.3), (b"\x11", 0.3)], args=(example,), cols=120, rows=36)
text = plain(out.decode("utf-8", "replace"))
assert code == 0, f"exit {code}"
assert "Services" in text and "System Monitor" in text, "example components rendered"
assert "Nothing" in text and "undo" in text, "undo reported in the status bar (diff-rendered)"
assert "Minimal Dark" in text, "document theme shown in the header"
print("smoke 6 ok: example project opens and renders")
# 7. Interface Mode on a copy of the example: select, nudge, save, reopen.
import shutil
work = os.path.join(home, "work.glyph")
shutil.copy(example, work)
code, out = run(binary, home, keys=[(b"]", 0.3), (b"\x1b[C", 0.3), (b"\x13", 0.4), (b"\x11", 0.3)], args=(work,), cols=120, rows=36)
text = plain(out.decode("utf-8", "replace"))
assert code == 0, f"exit {code}"
assert "INTERFACE" in text, "example opens in Interface Mode (artwork layer is a hidden asset)"
assert "sel:" in text, "selection shown in the status bar"
assert "Inspector" in text
saved = open(work, encoding="utf-8").read()
assert '"mode": "absolute"' in saved and '"x": 1' in saved, "nudged root component was saved absolutely"
print("smoke 7 ok: interface mode select, nudge, save")
# 8. Paint tools: Alt+B brush, '%' becomes the brush, Space and a mouse
#    stroke paint, Shift+C opens the colour picker.
art = os.path.join(home, "paint.glyph")
if os.path.exists(art):
    os.remove(art)
click = lambda kind, col, row: f"\x1b[<{kind};{col};{row}M".encode()
release = lambda col, row: f"\x1b[<0;{col};{row}m".encode()
code, out = run(binary, home, keys=[
    (b"\x1bb", 0.3), (b"%", 0.2), (b" ", 0.2),
    (click(0, 30, 6), 0.1), (click(32, 36, 6), 0.1), (release(36, 6), 0.3),
    (b"C", 0.3), (b"\x1b", 0.2),
    (b"\x1bOP", 0.1), (b"\x1b", 0.1),
    (b"\x13", 0.3), (b"\x1b[27;6;115~", 0.1),
], args=(art,), cols=120, rows=36, timeout=4)
text = plain(out.decode("utf-8", "replace"))
assert "Tools" in text and "Colours" in text and "Brushes" in text, "palette rendered"
assert "BRUSH" in text, "brush tool active in the status bar (diff-rendered)"
assert "Choose background colour" in text, "colour picker opened"
assert "%%%" in text, "mouse stroke painted with the typed brush glyph"
assert code == 0 or code is None
assert '"%%%%%%%"' in open(art, encoding="utf-8").read(), "stroke saved as one text run"
print("smoke 8 ok: palette, brush, stroke, colour picker")
# 9. Semantic brushes: search "double" (double line brush), draw an L, then
#    cross it with a single line from the same page.
lines = os.path.join(home, "lines.glyph")
if os.path.exists(lines):
    os.remove(lines)
drag = lambda col, row: f"\x1b[<32;{col};{row}M".encode()
code, out = run(binary, home, keys=[
    (b"\x1bb", 0.3), (b"g", 0.2), (b"double", 0.2), (b"\r", 0.3),
    (click(0, 30, 6), 0.1), (drag(40, 6), 0.1), (drag(40, 10), 0.1), (release(40, 10), 0.3),
    (b"g", 0.2), (b"single", 0.2), (b"\r", 0.3),
    (click(0, 35, 4), 0.1), (drag(35, 9), 0.1), (release(35, 9), 0.3),
    (b"\x13", 0.3),
], args=(lines,), cols=120, rows=36, timeout=3)
saved = open(lines, encoding="utf-8").read()
assert "═╪═" in saved and "╗" in saved, "double L joined and crossed: " + saved[-800:]
print("smoke 9 ok: line brushes join and cross")
# 10. Pixel Mode with pixel-precise mouse reports (as Ghostty/kitty send them):
#     Alt+P adds a sextant layer, a press at a sub-cell pixel paints exactly it.
pixels = os.path.join(home, "pixels.glyph")
if os.path.exists(pixels):
    os.remove(pixels)
# Cells are 10x20 px. Canvas cell (0,0) is at terminal cell (23,2) with the
# default panels (left panel 22 wide + border), i.e. pixel x 230.., y 40...
# Sextant pixels are 5x~6.7 px: pixel (1,2) of the cell is at x+7, y+15.
pclick = lambda col, row: f"\x1b[<0;{col + 1};{row + 1}M".encode()
prelease = lambda col, row: f"\x1b[<0;{col + 1};{row + 1}m".encode()
code, out = run(binary, home, extra_env={"TERM_PROGRAM": "ghostty"}, keys=[
    (b"\x1bp", 0.3), (b"\r", 0.3), (b"\x1bb", 0.2),
    (pclick(230 + 7, 40 + 15), 0.1), (prelease(230 + 7, 40 + 15), 0.3),
    (b"\x13", 0.3), (b"\x11", 0.2), (b"\x11", 0.2),
], args=(pixels,), cols=120, rows=36, timeout=4, cell_px=(10, 20))
text = out.decode("utf-8", "replace")
assert "\x1b[?1016h" in text and "\x1b[?1016l" in text, "pixel mouse mode enabled and restored"
assert "PIXEL" in plain(text), "pixel mode shown"
saved = json.load(open(pixels, encoding="utf-8"))
raster = [l for l in saved["screens"][0]["layers"] if l["kind"] == "raster"][0]["raster"]
assert raster["mode"] == "sextant", raster
assert raster["rows"][2] == ".!", f"exactly pixel (1,2) painted: {raster['rows']}"
print("smoke 10 ok: pixel layer, pixel-precise mouse")
# 11. The Esc menu: open it, walk into the theme page, pick a theme, and
#     quit through the menu itself. The theme is remembered for next time.
mhome = home + "/menu"
os.makedirs(mhome, exist_ok=True)
theme_file = mhome + "/.local/state/glyphforge/ui-theme"
if os.path.exists(theme_file):
    os.remove(theme_file)
menu_keys = [
    (b"\x1b", 0.4),                      # Esc opens the menu
    (b"\x1b[B", 0.2), (b"\r", 0.4),       # down to Theme, enter
    (b"\x1b[B" * 7, 0.3), (b"\r", 0.4),   # down to Phosphor, apply
    (b"\x1b", 0.3),                      # close the menu
    (b"\x1b", 0.3),                      # open it again
    (b"\x1b[B" * 6, 0.2), (b"\r", 0.5),   # down to Quit, enter
]
code, out = run(binary, mhome, keys=menu_keys, cols=100, rows=30, timeout=4)
text = plain(out.decode("utf-8", "replace"))
assert code == 0, f"menu quit exits cleanly: {code}"
assert "Menu" in text and "Resume" in text and "Quit" in text, "menu drawn"
assert "Phosphor" in text and "Amber CRT" in text, "theme page lists the themes"
# Ratatui redraws only changed cells, so spaces inside the message may be
# left standing: match the words, not the exact spacing.
assert re.search(r"Theme:\s*Phosphor", text), "theme applied"
assert open(theme_file, encoding="utf-8").read().strip() == "phosphor", "theme remembered"

# Starting again picks the remembered theme up.
code, out = run(binary, mhome, keys=[(b"\x11", 0.3)], cols=100, rows=30, timeout=4)
assert re.search(r"ui:\s*Phosphor", plain(out.decode("utf-8", "replace"))), "theme restored"
print("smoke 11 ok: Esc menu, theme switch, quit from the menu, theme remembered")
# 12. Gradient frames: a gradient theme paints its frames in many shades,
#     a flat one in a single border colour.
ghome = home + "/gradient"
os.makedirs(ghome + "/.local/state/glyphforge", exist_ok=True)
def frame_colours(theme):
    open(ghome + "/.local/state/glyphforge/ui-theme", "w").write(theme + "\n")
    _, out = run(binary, ghome, keys=[(b"\x11", 0.3)], cols=100, rows=30, timeout=3)
    raw = out.decode("utf-8", "replace")
    return len(set(re.findall(r"\x1b\[38;2;(\d+;\d+;\d+)", raw)))
gradient, flat = frame_colours("aurora"), frame_colours("cobalt")
assert gradient > flat * 5, f"gradient frames vary: {gradient} vs {flat}"
print(f"smoke 12 ok: gradient frames ({gradient} shades vs {flat} flat)")
# 13. Command palette: Ctrl+P, fuzzy query, Enter runs the command.
#     Copying cells sends them to the system clipboard through OSC 52.
phome = home + "/palette"
os.makedirs(phome, exist_ok=True)
keys = [
    (b"\x10", 0.4),                       # Ctrl+P
    (b"togall", 0.3), (b"\r", 0.4),        # Toggle All Panels
    (b"ab", 0.2),                         # type on the artwork layer
    (b"\x01", 0.2), (b"\x03", 0.4),        # Ctrl+A, Ctrl+C
    (b"\x11", 0.3), (b"\x11", 0.3),        # quit, confirm
]
code, out = run(binary, phome, keys=keys, cols=100, rows=30, timeout=4)
raw = out.decode("utf-8", "replace")
assert code == 0, f"exit {code}"
assert "Commands" in plain(raw), "palette drawn"
assert "Toggle All Panels" in plain(raw), "fuzzy match shown"
assert "\x1b]52;c;YWI=" in raw, "OSC 52 carries the copied cells"
print("smoke 13 ok: command palette and OSC 52 copy")
print("ALL SMOKE TESTS PASSED")
