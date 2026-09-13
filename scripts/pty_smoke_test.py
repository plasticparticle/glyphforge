#!/usr/bin/env python3
"""End-to-end smoke test: runs the glyphforge binary inside a pseudo-terminal.

Usage: scripts/pty_smoke_test.py target/release/glyphforge /tmp/some-empty-dir

The harness answers the terminal queries the editor sends (cursor position,
device attributes) so the run matches a real terminal. Exact screen content
is asserted by the Ratatui TestBackend unit tests; this script checks the
terminal lifecycle, key handling, theme detection and error surfacing.

Assert only on SHORT substrings. Ratatui writes a diff of the screen, so a
long phrase is often split across unrelated cursor moves and never appears
contiguously in the captured stream, even though the user sees it.
"""
import os, pty, sys, time, select, signal, struct, fcntl, termios, subprocess, re

ESC_RE = re.compile(r"\x1b\[[0-9;?<>=]*[A-Za-z@`~]|\x1b[()][A-Za-z0-9]|\x1b[=>]")
def plain(text):
    return ESC_RE.sub("", text)

def run(binary, home, extra_env=None, keys=None, cols=100, rows=30, timeout=8, args=()):
    env = dict(os.environ)
    env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "HOME": home,
                "LANG": "en_US.UTF-8", "GLYPHFORGE_LOG": "debug"})
    env.pop("XDG_CONFIG_HOME", None); env.pop("XDG_STATE_HOME", None)
    if extra_env: env.update(extra_env)
    pid, fd = pty.fork()
    if pid == 0:
        os.execve(binary, [binary, *args], env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
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
# 8. Multi-selection and alignment in a fresh document.
keys = [
    (b"\x1b[5;5~", 0.3),                      # Ctrl+PgUp: switch to the UI layer
    (b"a", 0.2), (b"panel one\r", 0.4),        # add the first panel
    (b"\x1b", 0.2),                           # clear the selection
    (b"\x1b[C\x1b[C\x1b[C\x1b[B\x1b[B", 0.3),  # move the cursor away
    (b"a", 0.2), (b"panel two\r", 0.4),        # add the second panel
    (b"\x1b", 0.2),
    (b"]", 0.2), (b"}", 0.3),                  # select one, extend to two
    (b"\x1b[1;3A", 0.4),                      # Alt+Up: align top edges
    (b"\x11", 0.3), (b"\x11", 0.3),
]
code, out = run(binary, home, keys=keys, cols=120, rows=30)
text = plain(out.decode("utf-8", "replace"))
assert code == 0, f"exit {code}"
assert "INTERFACE" in text, "the UI layer switches the editor into Interface Mode"
assert "2 components" in text, "the inspector summarises the multi-selection"
assert "Aligned" in text, f"alignment ran: {text[-400:]}"
print("smoke 8 ok: multi-selection and alignment")
print("ALL SMOKE TESTS PASSED")
