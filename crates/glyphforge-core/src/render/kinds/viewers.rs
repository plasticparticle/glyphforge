//! Viewers: logs, source code, Markdown and diffs. Highlighting is
//! deliberately simple and deterministic: it shows what the real view will
//! look like, it is not a parser.

use crate::component::Component;
use crate::document::{Canvas, CellStyle, Rect, Size};
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit;
use crate::render::{ComponentRenderer, RenderContext, painter};

fn text_lines(c: &Component, ctx: &RenderContext<'_>) -> Vec<String> {
    let listed = kit::strings(c, ctx, "lines");
    if !listed.is_empty() && c.prop_str("lines").is_none() {
        return listed;
    }
    match kit::value(c, ctx, "text").as_deref() {
        Some(crate::value::Value::Str(s)) => s.lines().map(str::to_owned).collect(),
        Some(crate::value::Value::List(items)) => items.iter().map(kit::text).collect(),
        _ => Vec::new(),
    }
}

fn lines_size(lines: &[String], extra: u16) -> Size {
    let w = lines
        .iter()
        .map(|l| painter::text_width(l))
        .max()
        .unwrap_or(10)
        + extra;
    Size::new(w, lines.len().max(1) as u16)
}

/// Shows the last lines that fit when `follow` is set (a tail).
fn visible(lines: &[String], rows: u16, follow: bool) -> &[String] {
    let rows = usize::from(rows);
    if follow && lines.len() > rows {
        &lines[lines.len() - rows..]
    } else {
        &lines[..lines.len().min(rows)]
    }
}

/// Log output: level words coloured, timestamps muted.
#[derive(Debug)]
pub struct Log;

impl ComponentRenderer for Log {
    fn kind(&self) -> &'static str {
        "log"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("log", "Log", Category::Viewer)
            .summary("Log lines with levels and timestamps")
            .keywords(&["logs", "output", "console", "journal", "tail"])
            .size(Some((50, 6)))
            .prop(PropSpec::new("text", PropKind::Text, "log text, one entry per line").initial(
                "12:01:07 INFO  server listening on :8080\n12:01:09 WARN  slow query (412 ms)\n12:01:12 ERROR upstream timed out\n12:01:13 DEBUG retrying (1/3)",
            ))
            .prop(PropSpec::new("follow", PropKind::Bool, "show the newest lines").default(true))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        Some(lines_size(&text_lines(c, ctx), 0))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let lines = text_lines(c, ctx);
        let follow = c.prop_bool("follow") != Some(false);
        for (i, line) in visible(&lines, inner.height, follow).iter().enumerate() {
            let y = inner.y + i as u16;
            let mut x = inner.x;
            for (n, word) in line.split_inclusive(' ').enumerate() {
                let w = word.trim();
                let style = match w {
                    "ERROR" | "ERR" | "FATAL" => kit::bold(kit::style(ctx, "error", "surface")),
                    "WARN" | "WARNING" => kit::style(ctx, "warning", "surface"),
                    "INFO" => kit::style(ctx, "info", "surface"),
                    "DEBUG" | "TRACE" => kit::dim(kit::style(ctx, "foreground-muted", "surface")),
                    _ if n == 0 && w.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                        kit::style(ctx, "foreground-muted", "surface")
                    }
                    _ => kit::style(ctx, "foreground", "surface"),
                };
                x += kit::put(canvas, x, y, word, style, inner);
            }
        }
    }
}

const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "mut",
    "pub",
    "use",
    "mod",
    "struct",
    "enum",
    "impl",
    "trait",
    "match",
    "if",
    "else",
    "for",
    "while",
    "loop",
    "return",
    "in",
    "def",
    "class",
    "import",
    "from",
    "as",
    "const",
    "var",
    "function",
    "async",
    "await",
    "export",
    "type",
    "interface",
    "true",
    "false",
    "None",
    "null",
    "self",
    "Self",
    "package",
    "func",
    "go",
    "defer",
    "echo",
    "then",
    "fi",
    "do",
    "done",
];

/// The colour of each character of a source line: strings, comments,
/// numbers and keywords.
fn highlight(line: &str, ctx: &RenderContext<'_>) -> Vec<(String, CellStyle)> {
    let plain = kit::style(ctx, "foreground", "surface");
    let comment = kit::dim(kit::style(ctx, "foreground-muted", "surface"));
    let string = kit::style(ctx, "success", "surface");
    let number = kit::style(ctx, "warning", "surface");
    let keyword = kit::bold(kit::style(ctx, "primary", "surface"));
    let mut out: Vec<(String, CellStyle)> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("//") || c == '#' {
            out.push((rest, comment));
            break;
        }
        if c == '"' || c == '\'' {
            let end = chars[i + 1..]
                .iter()
                .position(|&q| q == c)
                .map_or(chars.len(), |p| i + p + 2);
            out.push((chars[i..end].iter().collect(), string));
            i = end;
            continue;
        }
        if c.is_ascii_digit() {
            let end = chars[i..]
                .iter()
                .position(|d| !d.is_ascii_alphanumeric() && *d != '.' && *d != '_')
                .map_or(chars.len(), |p| i + p);
            out.push((chars[i..end].iter().collect(), number));
            i = end;
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let end = chars[i..]
                .iter()
                .position(|d| !d.is_alphanumeric() && *d != '_')
                .map_or(chars.len(), |p| i + p);
            let word: String = chars[i..end].iter().collect();
            let style = if KEYWORDS.contains(&word.as_str()) {
                keyword
            } else {
                plain
            };
            out.push((word, style));
            i = end;
            continue;
        }
        out.push((c.to_string(), plain));
        i += 1;
    }
    out
}

/// Source code with simple highlighting and optional line numbers.
#[derive(Debug)]
pub struct Code;

impl ComponentRenderer for Code {
    fn kind(&self) -> &'static str {
        "code"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("code", "Code", Category::Viewer)
            .summary("Source code with highlighting")
            .keywords(&["source", "editor", "snippet", "syntax"])
            .size(Some((40, 6)))
            .prop(PropSpec::new("text", PropKind::Text, "the code").initial(
                "fn main() {\n    // greet\n    let n = 42;\n    println!(\"hello {n}\");\n}",
            ))
            .prop(PropSpec::new("line_numbers", PropKind::Bool, "number the lines").initial(true))
            .prop(
                PropSpec::new(
                    "start",
                    PropKind::int(1, 1_000_000),
                    "number of the first line",
                )
                .default(1i64),
            )
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        Some(lines_size(&text_lines(c, ctx), 5))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let lines = text_lines(c, ctx);
        let numbers = c.prop_bool("line_numbers") == Some(true);
        let start = kit::int(c, "start", 1);
        let gutter = (start + i64::try_from(lines.len()).unwrap_or(0))
            .to_string()
            .len();
        for (i, line) in lines.iter().enumerate().take(usize::from(inner.height)) {
            let y = inner.y + i as u16;
            let mut x = inner.x;
            if numbers {
                let n = format!("{:>gutter$} ", start + i64::try_from(i).unwrap_or(0));
                x += kit::put(
                    canvas,
                    x,
                    y,
                    &n,
                    kit::dim(kit::style(ctx, "foreground-muted", "surface")),
                    inner,
                );
            }
            for (part, style) in highlight(line, ctx) {
                x += kit::put(canvas, x, y, &part, style, inner);
            }
        }
    }
}

/// Markdown: headings, bullets, quotes, rules, `code` and **bold**.
#[derive(Debug)]
pub struct Markdown;

impl Markdown {
    /// Splits inline `code` and **bold** spans.
    fn inline(line: &str, ctx: &RenderContext<'_>, base: CellStyle) -> Vec<(String, CellStyle)> {
        let mut out = Vec::new();
        let mut rest = line;
        while !rest.is_empty() {
            let code = rest.find('`');
            let strong = rest.find("**");
            match (code, strong) {
                (Some(c), s) if s.is_none_or(|s| c < s) => {
                    out.push((rest[..c].to_owned(), base));
                    let after = &rest[c + 1..];
                    let end = after.find('`').unwrap_or(after.len());
                    out.push((
                        after[..end].to_owned(),
                        kit::style(ctx, "secondary", "surface-alt"),
                    ));
                    rest = after.get(end + 1..).unwrap_or("");
                }
                (_, Some(s)) => {
                    out.push((rest[..s].to_owned(), base));
                    let after = &rest[s + 2..];
                    let end = after.find("**").unwrap_or(after.len());
                    out.push((after[..end].to_owned(), kit::bold(base)));
                    rest = after.get(end + 2..).unwrap_or("");
                }
                _ => {
                    out.push((rest.to_owned(), base));
                    break;
                }
            }
        }
        out
    }
}

impl ComponentRenderer for Markdown {
    fn kind(&self) -> &'static str {
        "markdown"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("markdown", "Markdown", Category::Viewer)
            .summary("Rendered Markdown text")
            .keywords(&["readme", "docs", "rich text", "notes"])
            .size(Some((40, 8)))
            .prop(PropSpec::new("text", PropKind::Text, "Markdown source").initial(
                "# Release notes\n\nGlyphforge **0.2** adds:\n- the `table` component\n- data fixtures\n> Try it today.",
            ))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        Some(lines_size(&text_lines(c, ctx), 2))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let lines = text_lines(c, ctx);
        let base = kit::style(ctx, "foreground", "surface");
        for (i, line) in lines.iter().enumerate().take(usize::from(inner.height)) {
            let y = inner.y + i as u16;
            let trimmed = line.trim_start();
            let (lead, lead_style, rest, style) = if let Some(h) = trimmed.strip_prefix("# ") {
                (
                    "",
                    base,
                    h,
                    kit::underline(kit::bold(kit::style(ctx, "primary", "surface"))),
                )
            } else if let Some(h) = trimmed
                .strip_prefix("## ")
                .or_else(|| trimmed.strip_prefix("### "))
            {
                (
                    "",
                    base,
                    h,
                    kit::bold(kit::style(ctx, "primary", "surface")),
                )
            } else if let Some(b) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                (" • ", kit::style(ctx, "primary", "surface"), b, base)
            } else if let Some(q) = trimmed.strip_prefix("> ") {
                (
                    "▎ ",
                    kit::style(ctx, "border", "surface"),
                    q,
                    kit::style(ctx, "foreground-muted", "surface"),
                )
            } else if trimmed == "---" || trimmed == "***" {
                let rule: String = std::iter::repeat_n("─", usize::from(inner.width)).collect();
                kit::put(
                    canvas,
                    inner.x,
                    y,
                    &rule,
                    kit::style(ctx, "border-muted", "surface"),
                    inner,
                );
                continue;
            } else {
                ("", base, trimmed, base)
            };
            let mut x = inner.x + kit::put(canvas, inner.x, y, lead, lead_style, inner);
            for (part, s) in Self::inline(rest, ctx, style) {
                x += kit::put(canvas, x, y, &part, s, inner);
            }
        }
    }
}

/// Unified diff: additions, removals and hunk headers coloured.
#[derive(Debug)]
pub struct Diff;

impl ComponentRenderer for Diff {
    fn kind(&self) -> &'static str {
        "diff"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("diff", "Diff", Category::Viewer)
            .summary("Unified diff with coloured changes")
            .keywords(&["patch", "changes", "git", "review", "compare"])
            .size(Some((44, 6)))
            .prop(
                PropSpec::new("text", PropKind::Text, "unified diff").initial(
                    "@@ -1,3 +1,3 @@\n fn main() {\n-    run(8080);\n+    run(config.port);\n }",
                ),
            )
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        Some(lines_size(&text_lines(c, ctx), 0))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        for (i, line) in text_lines(c, ctx)
            .iter()
            .enumerate()
            .take(usize::from(inner.height))
        {
            let y = inner.y + i as u16;
            let style = if line.starts_with("@@") {
                kit::style(ctx, "info", "surface")
            } else if line.starts_with('+') {
                kit::style(ctx, "success", "surface")
            } else if line.starts_with('-') {
                kit::style(ctx, "error", "surface")
            } else {
                kit::style(ctx, "foreground-muted", "surface")
            };
            kit::put(canvas, inner.x, y, line, style, inner);
        }
    }
}
