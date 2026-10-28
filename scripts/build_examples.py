#!/usr/bin/env python3
"""Builds the showcase examples in examples/ through the Glyphforge CLI.

Every example is a fresh `glyphforge new --theme ...` plus one patch, so
the files are canonical and reproducible. Run after changing the component
library: `scripts/build_examples.py target/debug/glyphforge`.

The system monitor (examples/dashboard.glyph) is hand-made and not rebuilt.
"""
import json, os, subprocess, sys, tempfile

BIN = sys.argv[1] if len(sys.argv) > 1 else "target/debug/glyphforge"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXAMPLES = os.path.join(ROOT, "examples")


def L(width=None, height=None, x=None, y=None, direction=None, gap=None,
      padding=None, margin=None, align=None, columns=None):
    layout = {}
    if x is not None:
        layout["placement"] = {"mode": "absolute", "x": x, "y": y or 0}
    if width is not None:
        layout["width"] = width if isinstance(width, str) else f"fixed({width})"
    if height is not None:
        layout["height"] = height if isinstance(height, str) else f"fixed({height})"
    container = {}
    if direction:
        container["direction"] = direction
    if gap:
        container["gap"] = gap
    if align:
        container["align"] = align
    if columns:
        container["columns"] = columns
    if container:
        layout["container"] = container

    def edges(v):
        if isinstance(v, int):
            return {"top": v, "right": v, "bottom": v, "left": v}
        t, r, b, l = v if len(v) == 4 else (v[0], v[1], v[0], v[1])
        return {"top": t, "right": r, "bottom": b, "left": l}
    if padding is not None:
        layout["padding"] = edges(padding)
    if margin is not None:
        layout["margin"] = edges(margin)
    return layout


def C(id, kind, layout=None, children=None, **props):
    if kind == "panel":
        # A column of air inside every frame.
        layout = dict(layout or {})
        layout.setdefault("padding", {"top": 0, "right": 1, "bottom": 0, "left": 1})
    c = {"id": id, "kind": kind}
    if props:
        c["props"] = props
    if layout:
        c["layout"] = layout
    if children:
        c["children"] = children
    return c


ROW = lambda **k: L(direction="horizontal", **k)
COL = lambda **k: L(direction="vertical", **k)
ONE = "fixed(1)"


def build(name, theme, size, title, roots, data=None):
    path = os.path.join(EXAMPLES, f"{name}.glyph")
    if os.path.exists(path):
        os.remove(path)
    subprocess.run([BIN, "new", path, "--width", str(size[0]), "--height",
                    str(size[1]), "--theme", theme], check=True,
                   stdout=subprocess.DEVNULL)
    ops = [{"op": "set_data", "key": k, "value": v} for k, v in (data or {}).items()]
    ops += [{"op": "create_component", "layer": "main-ui", "component": r} for r in roots]
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump({"operations": ops}, f)
        patch = f.name
    try:
        subprocess.run([BIN, "apply", path, patch], check=True, stdout=subprocess.DEVNULL)
    finally:
        os.remove(patch)
    print(f"built {path}: {title}")


# --- Developer dashboard (cobalt) ---------------------------------------------------
build("developer-dashboard", "cobalt", (100, 30), "Developer dashboard", [
    C("app", "group", COL(), [
        C("topbar", "group", ROW(height=1, gap=2), [
            C("crumbs", "breadcrumbs", L(width="content"),
              items=["acme", "platform", "checkout-service"]),
            C("env", "badge", L(width="content"), text="PROD", level="error"),
            C("spacer", "group"),
            C("who", "label", L(width="content"), text="ana@acme.dev", color="foreground-muted"),
        ]),
        C("tabs", "tabs", L(height=2), items=["Overview", "Builds", "Deploys", "Incidents"],
          selected=0, on_activate="select:tabs=2"),
        C("body", "group", ROW(gap=1), [
            C("left", "group", COL(gap=1), [
                C("meters", "group", ROW(height=3, gap=1), [
                    C("m-cpu", "panel", L(), [C("g-cpu", "gauge", L(height=1), label="CPU", value=46)], title="Compute"),
                    C("m-err", "panel", L(), [C("g-err", "gauge", L(height=1), label="Errors", value=93, warn=50, critical=80)], title="Error budget"),
                    C("m-lat", "panel", L(), [C("g-lat", "gauge", L(height=1), label="p99", value=71)], title="Latency"),
                ]),
                C("traffic", "panel", L(height=9), [
                    C("rps", "linechart", L(), bind="requests", axis=True, color="info"),
                ], title="Requests / min"),
                C("deploys", "panel", L(), [
                    C("deploy-table", "table", L(), bind="deploys", selected=0, striped=True),
                ], title="Recent deploys"),
            ]),
            C("right", "group", COL(width=38, gap=1), [
                C("builds", "panel", L(height=8), [
                    C("build-list", "list", L(), items=[
                        "✓ #4821 main · 3m 12s", "✓ #4820 main · 3m 40s",
                        "✗ #4819 feat/cart · 1m 02s", "✓ #4818 main · 3m 09s",
                        "⠋ #4822 feat/tax · running"], selected=2),
                ], title="Pipelines"),
                C("logs", "panel", L(), [
                    C("log", "log", L(), text="\n".join([
                        "09:41:02 INFO  deploy v2.31.0 started",
                        "09:41:19 INFO  canary 5% healthy",
                        "09:42:03 WARN  p99 above 400 ms (eu-west)",
                        "09:42:31 ERROR payment-gw timeout x3",
                        "09:42:32 INFO  rollback armed",
                    ])),
                ], title="Live log"),
            ]),
        ]),
        C("hints", "keyhint", L(height=1), items=["d|deploy", "r|rollback", "l|logs", "/|filter", "q|quit"]),
    ]),
], data={
    "requests": [120, 132, 128, 150, 171, 160, 158, 190, 240, 230, 210, 260, 300, 280, 250, 270, 310, 330, 300, 290],
    "deploys": {
        "columns": ["Version", "Author", "When", "Status"],
        "rows": [
            ["v2.31.0", "ana", "2 min ago", "rolling out"],
            ["v2.30.2", "jun", "3 h ago", "live"],
            ["v2.30.1", "sam", "yesterday", "rolled back"],
            ["v2.30.0", "ana", "2 days ago", "superseded"],
        ],
    },
})

# --- Git client (verdant) --------------------------------------------------------
build("git-client", "verdant", (100, 30), "Git client", [
    C("app", "group", COL(), [
        C("toolbar", "toolbar", L(height=1),
          items=["Fetch|f", "Pull|p", "Push|P", "|", "Commit|c", "Stash|s"]),
        C("workspace", "group", ROW(gap=1), [
            C("refs", "panel", L(width=26), [
                C("ref-tree", "tree", L(), items=[
                    "Branches", "  ● main", "  feat/sextants", "  fix/wide-glyphs",
                    "Remotes", "  origin", "    main", "    release/0.2",
                    "+Tags", "Stashes", "  WIP on main: palette"], selected=1),
            ], title="glyphforge"),
            C("history", "group", COL(gap=1), [
                C("commits", "panel", L(height=11), [
                    C("commit-table", "table", L(), bind="commits", selected=1),
                ], title="History · main"),
                C("changes", "panel", L(), [
                    C("diff", "diff", L(), text="\n".join([
                        "@@ -41,7 +41,9 @@ fn encode_block(",
                        "     let mask = inks.iter().enumerate()",
                        "-        .fold(0u8, |m, (i, p)| m | bit(i, p));",
                        "+        .fold(0u8, |m, (i, p)| {",
                        "+            if *p == a { m | (1 << i) } else { m }",
                        "+        });",
                        "     let glyph = block_glyph(mode, mask)?;",
                    ])),
                ], title="a3f9c21 · Octant glyph table"),
            ]),
        ]),
        C("status", "statusbar", L(height=1), items=["main", "↑2 ↓0", "3 changed"], right="clean · signed"),
    ]),
], data={
    "commits": {
        "columns": ["Hash", "Message", "Author", "Age"],
        "rows": [
            ["e71b0d4", "Prototype mode and screen graph", "lars", "1h"],
            ["a3f9c21", "Octant glyph table from UnicodeData", "lars", "3h"],
            ["9c04e7a", "OKLab gradients", "mia", "5h"],
            ["51de2b8", "Command palette fuzzy scoring", "lars", "1d"],
            ["0bb93f1", "Snap guides while dragging", "jun", "2d"],
            ["7d1aa60", "Initial semantic model", "lars", "1w"],
        ],
    },
})

# --- AI coding assistant (nebula) -----------------------------------------------
build("ai-assistant", "nebula", (100, 30), "AI coding assistant", [
    C("app", "group", ROW(gap=1), [
        C("threads", "panel", L(width=24), [
            C("thread-list", "list", L(), items=[
                "Fix flaky test", "Add octant mode", "Refactor dispatch",
                "Explain OKLab", "Write README"], selected=0),
            C("new-chat", "button", L(height=1), label="New chat", primary=True),
        ], title="Conversations"),
        C("chat", "group", COL(gap=1), [
            C("transcript", "panel", L(), [
                C("conversation", "markdown", L(), text="\n".join([
                    "## Fix flaky test",
                    "**You** the snap test fails on CI about one run in ten",
                    "",
                    "**Assistant** Two tests share `glyphforge-omarchy-<pid>`",
                    "as a temp directory and run in parallel; one deletes it",
                    "while the other writes. Give each test its own name:",
                ])),
                C("snippet", "code", L(height=4), text="let dir = std::env::temp_dir()\n    .join(format!(\"glyphforge-theme-poll-{}\", pid));", line_numbers=True, start=239),
                C("thinking", "spinner", L(height=1), label="Running the test suite…", frame=3),
            ], title="Chat"),
            C("composer", "group", ROW(height=3, gap=1), [
                C("prompt", "textarea", L(), placeholder="Ask about the code…  (Ctrl+Enter sends)", state="focused"),
                C("send", "button", L(width=10), label="Send", primary=True, on_activate="open:sent"),
            ]),
        ]),
        C("context", "panel", L(width=26), [
            C("ctx-files", "list", L(height=5), items=["app/autosave.rs", "omarchy/mod.rs", "Cargo.toml"], marker="◆"),
            C("ctx-div", "divider", L(height=1)),
            C("tokens", "gauge", L(height=1), label="Context", value=38, warn=70, critical=90),
            C("model", "keyvalue", L(height=3), items=["Model=opus", "Mode=plan", "Cost=$0.42"]),
            C("ctx-space", "group"),
            C("hint", "keyhint", L(height=1), items=["⏎|send", "↑|edit", "esc|stop"]),
        ], title="Context"),
    ]),
    C("sent", "toast", L(x=60, y=26, width=24, height=1), text="Message sent", level="success", hidden=True),
])

# --- Database explorer (amber CRT) -------------------------------------------------
build("database-explorer", "amber-crt", (100, 30), "Database explorer", [
    C("app", "group", COL(), [
        C("header", "group", ROW(height=1, gap=2), [
            C("db-title", "heading", L(width="content"), text="pgx://analytics@db-01"),
            C("conn", "tag", L(width="content"), text="read-only"),
            C("hdr-space", "group"),
            C("latency", "label", L(width="content"), text="12 ms", color="foreground-muted"),
        ]),
        C("workspace", "group", ROW(gap=1), [
            C("schema", "panel", L(width=26), [
                C("schema-tree", "tree", L(), items=[
                    "public", "  orders", "  order_items", "  customers", "  products",
                    "analytics", "  daily_revenue", "  cohorts", "+pg_catalog"], selected=1),
            ], title="Schemas"),
            C("work", "group", COL(gap=1), [
                C("editor", "panel", L(height=7), [
                    C("sql", "code", L(), text="\n".join([
                        "select o.id, c.name, o.total, o.status",
                        "  from orders o",
                        "  join customers c on c.id = o.customer_id",
                        " where o.created_at > now() - interval '1 day'",
                        " order by o.total desc limit 50;",
                    ]), line_numbers=True),
                ], title="Query 1"),
                C("results", "panel", L(), [
                    C("grid", "datagrid", L(), bind="orders"),
                ], title="Results · 5 of 142 rows"),
            ]),
            C("details", "panel", L(width=28), [
                C("table-props", "propertygrid", L(height=6), items=[
                    "Table=orders", "Rows=1 284 033", "Size=412 MB", "Indexes=4", "Vacuumed=2 h ago"]),
                C("rev-title", "label", L(height=1), text="Revenue, 14 days", color="foreground-muted"),
                C("revenue", "barchart", L(height=6), values=[4, 6, 5, 7, 9, 8, 10, 12, 9, 11, 13, 12, 14, 16], labels=[], style="block"),
            ], title="orders"),
        ]),
        C("status", "statusbar", L(height=1), items=["NORMAL", "analytics", "UTF8"], right="F5 run · F6 explain"),
    ]),
], data={
    "orders": {
        "columns": ["id", "name", "total", "status"],
        "rows": [
            ["98121", "Kowalski & Co", "4,980.00", "paid"],
            ["98117", "Ostrander GmbH", "3,214.50", "paid"],
            ["98130", "Hallström AB", "2,870.00", "pending"],
            ["98109", "Nakamura KK", "2,402.75", "refunded"],
            ["98126", "Duarte Lda", "1,990.10", "paid"],
        ],
    },
})

# --- Settings application (paper, light) ------------------------------------------
build("settings", "paper", (100, 30), "Settings application", [
    C("app", "group", ROW(gap=2, padding=(1, 2)), [
        C("sections", "menu", L(width=24), items=[
            "General", "Appearance", "Keyboard", "Notifications", "-", "Accounts", "About"], selected=1),
        C("form", "group", COL(gap=1), [
            C("form-title", "heading", L(height=1), text="Appearance"),
            C("form-sub", "label", L(height=1), text="How Glyphforge looks on this machine.", color="foreground-muted"),
            C("theme-row", "group", ROW(height=1, gap=2), [
                C("theme-label", "label", L(width=16), text="Theme"),
                C("theme", "select", L(width=24), options=["System", "Paper", "Cobalt", "Ember"], value="Paper"),
            ]),
            C("font-row", "group", ROW(height=1, gap=2), [
                C("font-label", "label", L(width=16), text="Font"),
                C("font", "input", L(width=30), value="JetBrains Mono 11"),
            ]),
            C("density-row", "group", ROW(height=1, gap=2), [
                C("density-label", "label", L(width=16), text="Density"),
                C("compact", "radio", L(width="content"), label="Compact", checked=False),
                C("comfortable", "radio", L(width="content"), label="Comfortable", checked=True),
            ]),
            C("toggles", "group", COL(height=3), [
                C("t-anim", "toggle", L(height=1), label="Animations", checked=True),
                C("t-frames", "toggle", L(height=1), label="Gradient frames", checked=True),
                C("t-mouse", "toggle", L(height=1), label="Pixel-precise mouse", checked=False, state="disabled"),
            ]),
            C("checks", "group", COL(height=2), [
                C("c-tips", "checkbox", L(height=1), label="Show tips on start", checked=True),
                C("c-telemetry", "checkbox", L(height=1), label="Send anonymous usage data", checked=False),
            ]),
            C("form-space", "group"),
            C("buttons", "group", ROW(height=1, gap=2), [
                C("btn-space", "group"),
                C("cancel", "button", L(width="content"), label="Cancel"),
                C("save", "button", L(width="content"), label="Save changes", primary=True, on_activate="open:saved"),
            ]),
        ]),
    ]),
    C("saved", "notification", L(x=62, y=1, width=34, height=4), title="Settings saved",
      message="Restart the editor to apply the font.", level="success", hidden=True),
])
