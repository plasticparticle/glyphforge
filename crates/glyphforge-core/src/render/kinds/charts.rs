//! Charts: sparklines, bar charts, histograms, line charts, gauges,
//! heatmaps and progress bars. Every chart takes a `style`: `block`
//! (eighth blocks), `half` (half blocks), `braille` (2×4 dots per cell) or
//! `ascii`, so a design can show what each terminal will draw.

use crate::component::Component;
use crate::document::{Canvas, CellStyle, Color, Rect, Size};
use crate::render::descriptor::{Category, Descriptor, PropKind, PropSpec};
use crate::render::kit;
use crate::render::{ComponentRenderer, RenderContext, painter};
use crate::value::Value;

const EIGHTHS_UP: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
const EIGHTHS_RIGHT: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
const ASCII_UP: [&str; 5] = [" ", ".", "-", "=", "#"];

fn style_prop() -> PropSpec {
    PropSpec::new(
        "style",
        PropKind::choice(&["block", "half", "braille", "ascii"]),
        "drawing strategy",
    )
    .default("block")
}

fn values_prop(initial: &[i64]) -> PropSpec {
    PropSpec::new("values", PropKind::List, "whole numbers").initial(Value::List(
        initial.iter().map(|v| Value::Int(*v)).collect(),
    ))
}

const SAMPLE: [i64; 16] = [3, 5, 4, 8, 12, 9, 7, 11, 15, 13, 10, 14, 18, 16, 12, 17];

fn chart_style(c: &Component, ctx: &RenderContext<'_>) -> CellStyle {
    CellStyle::new(
        kit::color(c, ctx, "color", "primary"),
        ctx.theme.color("surface"),
    )
}

/// Braille dot bit of the dot at column `dx` (0..2), row `dy` (0..4).
fn braille_bit(dx: u16, dy: u16) -> u32 {
    const DOTS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
    DOTS[usize::from(dy)][usize::from(dx)]
}

/// Draws one column of a vertical bar of `level` (0..=rows*8) eighths
/// from the bottom of `area`, in the chosen style.
fn vertical_bar(canvas: &mut Canvas, x: u16, area: Rect, level: u32, look: &str, style: CellStyle) {
    let rows = u32::from(area.height);
    for r in 0..rows {
        let y = (area.bottom() - 1 - r) as u16;
        let fill = level.saturating_sub(r * 8).min(8);
        let glyph = match look {
            "ascii" => ASCII_UP[(fill as usize).div_ceil(2)],
            "half" => match fill {
                0..=1 => " ",
                2..=5 => "▄",
                _ => "█",
            },
            "braille" => match fill {
                0 => " ",
                1..=2 => "⣀",
                3..=4 => "⣤",
                5..=6 => "⣶",
                _ => "⣿",
            },
            _ => EIGHTHS_UP[fill as usize],
        };
        if glyph != " " {
            kit::put(canvas, x, y, glyph, style, area);
        }
    }
}

/// One-row trend line.
#[derive(Debug)]
pub struct Sparkline;

impl ComponentRenderer for Sparkline {
    fn kind(&self) -> &'static str {
        "sparkline"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("sparkline", "Sparkline", Category::Chart)
            .summary("Tiny trend line, one value per column")
            .keywords(&["trend", "mini chart", "history", "graph"])
            .size(None)
            .prop(values_prop(&SAMPLE))
            .prop(style_prop())
            .prop(PropSpec::new("color", PropKind::Color, "line colour").default("primary"))
            .prop(kit::bind_prop())
    }

    fn intrinsic_size(&self, c: &Component, ctx: &RenderContext<'_>) -> Option<Size> {
        let n = kit::numbers(c, ctx, "values").len() as u16;
        let w = if kit::str_or(c, "style", "block") == "braille" {
            n.div_ceil(2)
        } else {
            n
        };
        Some(Size::new(w.max(1), 1))
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let values = kit::numbers(c, ctx, "values");
        let max = values.iter().copied().max().unwrap_or(1).max(1);
        let style = chart_style(c, ctx);
        let look = kit::str_or(c, "style", "block");
        let row = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        if look == "braille" {
            for (col, pair) in values.chunks(2).enumerate().take(usize::from(inner.width)) {
                let mut bits = 0;
                for (dx, v) in pair.iter().enumerate() {
                    let level = kit::scale(*v, max, 3);
                    bits |= braille_bit(dx as u16, (3 - level) as u16);
                }
                let glyph = char::from_u32(0x2800 + bits)
                    .map(String::from)
                    .unwrap_or_default();
                kit::put(canvas, row.x + col as u16, row.y, &glyph, style, row);
            }
            return;
        }
        for (i, v) in values.iter().enumerate().take(usize::from(inner.width)) {
            vertical_bar(
                canvas,
                row.x + i as u16,
                row,
                kit::scale(*v, max, 8),
                look,
                style,
            );
        }
    }
}

/// Bar chart: vertical bars with labels under them, or horizontal bars
/// with labels on the left.
#[derive(Debug)]
pub struct BarChart;

impl ComponentRenderer for BarChart {
    fn kind(&self) -> &'static str {
        "barchart"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("barchart", "Bar Chart", Category::Chart)
            .summary("Bars for a few labelled values")
            .keywords(&["bars", "columns", "compare", "graph"])
            .size(Some((30, 8)))
            .prop(values_prop(&[42, 77, 18, 60]))
            .prop(
                PropSpec::new("labels", PropKind::List, "one per value").initial(Value::List(
                    vec!["api".into(), "web".into(), "db".into(), "cache".into()],
                )),
            )
            .prop(
                PropSpec::new(
                    "orientation",
                    PropKind::choice(&["vertical", "horizontal"]),
                    "bar direction",
                )
                .default("vertical"),
            )
            .prop(style_prop())
            .prop(PropSpec::new("color", PropKind::Color, "bar colour").default("primary"))
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let values = kit::numbers(c, ctx, "values");
        let labels = kit::strings(c, ctx, "labels");
        let max = values.iter().copied().max().unwrap_or(1).max(1);
        let style = chart_style(c, ctx);
        let muted = kit::style(ctx, "foreground-muted", "surface");
        let look = kit::str_or(c, "style", "block");
        if kit::str_or(c, "orientation", "vertical") == "horizontal" {
            let lw = labels
                .iter()
                .map(|l| painter::text_width(l))
                .max()
                .unwrap_or(0);
            for (i, v) in values.iter().enumerate().take(usize::from(inner.height)) {
                let y = inner.y + i as u16;
                let label = labels.get(i).map_or("", String::as_str);
                kit::put(canvas, inner.x, y, &kit::fit(label, lw), muted, inner);
                let room = inner.width.saturating_sub(lw + 1 + 5);
                let eighths = kit::scale(*v, max, u32::from(room) * 8);
                let bar = match look {
                    "ascii" => "#".repeat((eighths / 8) as usize),
                    _ => format!(
                        "{}{}",
                        "█".repeat((eighths / 8) as usize),
                        EIGHTHS_RIGHT[(eighths % 8) as usize].trim()
                    ),
                };
                let x = inner.x + lw + 1;
                let w = kit::put(canvas, x, y, &bar, style, inner);
                kit::put(canvas, x + w + 1, y, &v.to_string(), muted, inner);
            }
            return;
        }
        let chart = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let n = values.len().max(1) as u16;
        let slot = (inner.width / n).max(1);
        let bar_w = slot.saturating_sub(1).max(1);
        for (i, v) in values.iter().enumerate() {
            let x0 = inner.x + i as u16 * slot;
            let level = kit::scale(*v, max, u32::from(chart.height) * 8);
            for dx in 0..bar_w {
                vertical_bar(canvas, x0 + dx, chart, level, look, style);
            }
            if let Some(label) = labels.get(i) {
                kit::put(
                    canvas,
                    x0,
                    inner.bottom() as u16 - 1,
                    &painter::truncate(label, slot),
                    muted,
                    inner,
                );
            }
        }
    }
}

/// Histogram: the values are samples, counted into `bins` buckets.
#[derive(Debug)]
pub struct Histogram;

impl ComponentRenderer for Histogram {
    fn kind(&self) -> &'static str {
        "histogram"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("histogram", "Histogram", Category::Chart)
            .summary("Distribution of samples in buckets")
            .keywords(&["distribution", "frequency", "buckets", "latency"])
            .size(Some((30, 6)))
            .prop(values_prop(&[
                12, 15, 15, 18, 21, 22, 22, 23, 25, 25, 26, 30, 31, 38, 45, 60,
            ]))
            .prop(
                PropSpec::new("bins", PropKind::int(1, 200), "number of buckets")
                    .default(Value::Int(10)),
            )
            .prop(style_prop())
            .prop(PropSpec::new("color", PropKind::Color, "bar colour").default("secondary"))
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let samples = kit::numbers(c, ctx, "values");
        let bins = usize::try_from(kit::int(c, "bins", 10))
            .unwrap_or(10)
            .max(1);
        let (lo, hi) = (
            samples.iter().copied().min().unwrap_or(0),
            samples.iter().copied().max().unwrap_or(1),
        );
        let span = (hi - lo).max(1);
        let mut counts = vec![0i64; bins];
        for s in &samples {
            let i =
                usize::try_from((s - lo) * i64::try_from(bins).unwrap_or(i64::MAX) / (span + 1))
                    .unwrap_or(0);
            counts[i.min(bins - 1)] += 1;
        }
        let max = counts.iter().copied().max().unwrap_or(1).max(1);
        let style = CellStyle::new(
            kit::color(c, ctx, "color", "secondary"),
            ctx.theme.color("surface"),
        );
        let look = kit::str_or(c, "style", "block");
        let slot = (inner.width / bins as u16).max(1);
        for (i, n) in counts.iter().enumerate() {
            let level = kit::scale(*n, max, u32::from(inner.height) * 8);
            for dx in 0..slot {
                vertical_bar(
                    canvas,
                    inner.x + i as u16 * slot + dx,
                    inner,
                    level,
                    look,
                    style,
                );
            }
        }
    }
}

/// Line chart drawn with Braille dots (or half blocks / ASCII), with an
/// optional axis.
#[derive(Debug)]
pub struct LineChart;

impl ComponentRenderer for LineChart {
    fn kind(&self) -> &'static str {
        "linechart"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("linechart", "Line Chart", Category::Chart)
            .summary("Series plotted as a line")
            .keywords(&["plot", "graph", "series", "timeline", "trend"])
            .size(Some((40, 8)))
            .prop(values_prop(&SAMPLE))
            .prop(style_prop().default("braille"))
            .prop(
                PropSpec::new("axis", PropKind::Bool, "draw the axis with the maximum")
                    .initial(true),
            )
            .prop(PropSpec::new("color", PropKind::Color, "line colour").default("primary"))
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let values = kit::numbers(c, ctx, "values");
        if values.is_empty() {
            return;
        }
        let max = values.iter().copied().max().unwrap_or(1).max(1);
        let style = chart_style(c, ctx);
        let muted = kit::style(ctx, "foreground-muted", "surface");
        let mut plot = inner;
        if c.prop_bool("axis") == Some(true) && inner.width > 6 && inner.height > 1 {
            let label = max.to_string();
            let lw = painter::text_width(&label) + 1;
            kit::put(canvas, inner.x, inner.y, &label, muted, inner);
            kit::put(
                canvas,
                inner.x,
                inner.bottom() as u16 - 1,
                "0",
                muted,
                inner,
            );
            for y in inner.y..inner.bottom() as u16 {
                kit::put(canvas, inner.x + lw, y, "│", muted, inner);
            }
            plot = Rect::new(
                inner.x + lw + 1,
                inner.y,
                inner.width.saturating_sub(lw + 1),
                inner.height,
            );
        }
        let look = kit::str_or(c, "style", "braille");
        let (cw, ch): (u16, u16) = match look {
            "braille" => (2, 4),
            "half" => (1, 2),
            _ => (1, 1),
        };
        let (pw, ph) = (plot.width * cw, plot.height * ch);
        if pw == 0 || ph == 0 {
            return;
        }
        // Sample the series across the plot's pixel columns.
        let ys: Vec<u16> = (0..pw)
            .map(|px| {
                let i =
                    usize::from(px) * (values.len() - 1).max(1) / usize::from(pw.max(2) - 1).max(1);
                let v = values[i.min(values.len() - 1)];
                (ph - 1) - kit::scale(v, max, u32::from(ph - 1)) as u16
            })
            .collect();
        let mut dots = vec![0u32; usize::from(plot.width) * usize::from(plot.height)];
        let mut mark = |px: u16, py: u16| {
            let (cx, cy) = (px / cw, py / ch);
            let i = usize::from(cy) * usize::from(plot.width) + usize::from(cx);
            dots[i] |= match look {
                "braille" => braille_bit(px % 2, py % 4),
                "half" => 1 << (py % 2),
                _ => 1,
            };
        };
        for px in 0..pw {
            let y = ys[usize::from(px)];
            // Join to the previous column so steep lines stay connected.
            let prev = if px == 0 { y } else { ys[usize::from(px - 1)] };
            for py in prev.min(y)..=prev.max(y) {
                mark(px, py);
            }
        }
        for cy in 0..plot.height {
            for cx in 0..plot.width {
                let bits = dots[usize::from(cy) * usize::from(plot.width) + usize::from(cx)];
                if bits == 0 {
                    continue;
                }
                let glyph = match look {
                    "braille" => char::from_u32(0x2800 + bits)
                        .map(String::from)
                        .unwrap_or_default(),
                    "half" => ["", "▀", "▄", "█"][bits as usize & 3].to_owned(),
                    "ascii" => "*".to_owned(),
                    _ => "•".to_owned(),
                };
                kit::put(canvas, plot.x + cx, plot.y + cy, &glyph, style, plot);
            }
        }
    }
}

/// `progress` and `gauge`: a bar filled to `value` of `max`, with the
/// percentage; the gauge adds a label and colours by thresholds.
#[derive(Debug)]
pub struct Meter {
    gauge: bool,
}

impl Meter {
    pub const PROGRESS: Self = Self { gauge: false };
    pub const GAUGE: Self = Self { gauge: true };
}

impl ComponentRenderer for Meter {
    fn kind(&self) -> &'static str {
        if self.gauge { "gauge" } else { "progress" }
    }

    fn descriptor(&self) -> Descriptor {
        let d = if self.gauge {
            Descriptor::new("gauge", "Gauge", Category::Chart)
                .summary("Labelled meter coloured by thresholds")
                .keywords(&["meter", "usage", "level", "cpu", "disk"])
                .prop(PropSpec::new("label", PropKind::Text, "what is measured").initial("CPU"))
                .prop(
                    PropSpec::new(
                        "warn",
                        PropKind::int(0, 1_000_000),
                        "warning from this value",
                    )
                    .default(Value::Int(70)),
                )
                .prop(
                    PropSpec::new(
                        "critical",
                        PropKind::int(0, 1_000_000),
                        "error from this value",
                    )
                    .default(Value::Int(90)),
                )
        } else {
            Descriptor::new("progress", "Progress Bar", Category::Chart)
                .summary("Bar filled to a fraction")
                .keywords(&["loading", "percent", "completion", "download"])
        };
        d.size(Some((24, 1)))
            .prop(
                PropSpec::new("value", PropKind::int(0, 1_000_000), "current value")
                    .initial(Value::Int(62)),
            )
            .prop(
                PropSpec::new("max", PropKind::int(1, 1_000_000), "full value")
                    .default(Value::Int(100)),
            )
            .prop(style_prop())
            .prop(PropSpec::new("color", PropKind::Color, "fill colour").default("primary"))
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let value = kit::numbers(c, ctx, "value").first().copied().unwrap_or(0);
        let max = kit::int(c, "max", 100).max(1);
        let pct = kit::scale(value, max, 100);
        let fg = if self.gauge {
            let token = if value >= kit::int(c, "critical", 90) {
                "error"
            } else if value >= kit::int(c, "warn", 70) {
                "warning"
            } else {
                "success"
            };
            ctx.theme.color(token)
        } else {
            kit::color(c, ctx, "color", "primary")
        };
        let muted = kit::style(ctx, "foreground-muted", "surface");
        let mut x = inner.x;
        if self.gauge {
            if let Some(label) = c.prop_str("label") {
                x += kit::put(
                    canvas,
                    x,
                    inner.y,
                    &format!("{label} "),
                    kit::bold(kit::style(ctx, "foreground", "surface")),
                    inner,
                );
            }
        }
        let suffix = format!(" {pct:>3}%");
        let room = inner
            .right()
            .saturating_sub(u32::from(x))
            .saturating_sub(painter::text_width(&suffix).into()) as u16;
        let look = kit::str_or(c, "style", "block");
        let eighths = kit::scale(value, max, u32::from(room) * 8);
        let full = (eighths / 8) as usize;
        let (filled, empty_glyph) = match look {
            "ascii" => (
                format!(
                    "{}{}",
                    "=".repeat(full),
                    if full < usize::from(room) && eighths % 8 >= 4 {
                        ">"
                    } else {
                        ""
                    }
                ),
                "-",
            ),
            "half" => ("▬".repeat(full), "▭"),
            "braille" => ("⣿".repeat(full), "⣀"),
            _ => (
                format!(
                    "{}{}",
                    "█".repeat(full),
                    EIGHTHS_RIGHT[(eighths % 8) as usize].trim()
                ),
                "░",
            ),
        };
        let w = kit::put(
            canvas,
            x,
            inner.y,
            &filled,
            CellStyle::new(fg, ctx.theme.color("surface")),
            inner,
        );
        let rest = usize::from(room.saturating_sub(w));
        let ew = kit::put(
            canvas,
            x + w,
            inner.y,
            &empty_glyph.repeat(rest),
            muted,
            inner,
        );
        kit::put(canvas, x + w + ew, inner.y, &suffix, muted, inner);
    }
}

/// Heatmap: a grid of numbers as colours from the surface to the primary
/// colour, or shade glyphs in `ascii` style.
#[derive(Debug)]
pub struct Heatmap;

impl ComponentRenderer for Heatmap {
    fn kind(&self) -> &'static str {
        "heatmap"
    }

    fn descriptor(&self) -> Descriptor {
        Descriptor::new("heatmap", "Heatmap", Category::Chart)
            .summary("Grid of values as colour intensity")
            .keywords(&["matrix", "density", "calendar", "activity"])
            .size(Some((24, 4)))
            .prop(
                PropSpec::new("rows", PropKind::List, "rows of whole numbers").initial(
                    Value::List(
                        [[0, 2, 5, 9, 4, 1], [3, 7, 9, 6, 2, 0], [1, 4, 6, 8, 9, 5]]
                            .iter()
                            .map(|r| Value::List(r.iter().map(|v| Value::Int(*v)).collect()))
                            .collect(),
                    ),
                ),
            )
            .prop(
                PropSpec::new("cell", PropKind::int(1, 8), "columns per value")
                    .default(Value::Int(2)),
            )
            .prop(style_prop())
            .prop(PropSpec::new("color", PropKind::Color, "hot colour").default("primary"))
            .prop(kit::bind_prop())
    }

    fn render(
        &self,
        c: &Component,
        _: Rect,
        inner: Rect,
        ctx: &RenderContext<'_>,
        canvas: &mut Canvas,
    ) {
        let grid: Vec<Vec<i64>> = kit::rows(c, ctx, &[])
            .into_iter()
            .map(|r| r.iter().filter_map(|v| v.parse().ok()).collect())
            .collect();
        let max = grid.iter().flatten().copied().max().unwrap_or(1).max(1);
        let cell_w = u16::try_from(kit::int(c, "cell", 2)).unwrap_or(2).max(1);
        let hot = kit::color(c, ctx, "color", "primary");
        let cold = ctx.theme.color("surface-alt");
        let ascii = kit::str_or(c, "style", "block") == "ascii";
        for (y, row) in grid.iter().enumerate().take(usize::from(inner.height)) {
            for (x, v) in row.iter().enumerate() {
                let px = inner.x + x as u16 * cell_w;
                let level = kit::scale(*v, max, 4);
                let (glyph, style) = if ascii {
                    (
                        [" ", "░", "▒", "▓", "█"][level as usize],
                        CellStyle::new(hot, ctx.theme.color("surface")),
                    )
                } else {
                    let mixed = mix(cold, hot, level as f32 / 4.0);
                    (" ", CellStyle::new(mixed, mixed))
                };
                kit::put(
                    canvas,
                    px,
                    inner.y + y as u16,
                    &glyph.repeat(usize::from(cell_w)),
                    style,
                    inner,
                );
            }
        }
    }
}

fn mix(cold: Color, hot: Color, share: f32) -> Color {
    match (cold.to_rgb(), hot.to_rgb()) {
        (Some(from), Some(to)) => {
            Color::Rgb(from.0, from.1, from.2).mix(Color::Rgb(to.0, to.1, to.2), share)
        }
        _ if share < 0.5 => cold,
        _ => hot,
    }
}
