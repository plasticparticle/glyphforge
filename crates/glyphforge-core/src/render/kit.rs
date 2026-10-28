//! Shared helpers for the component library: preview states, data
//! bindings, reading lists, rows and numbers from properties, and theme
//! styles. Renderers stay short and consistent by going through these.

use std::borrow::Cow;

use crate::component::Component;
use crate::document::{Attributes, CellStyle, Color, Rect};
use crate::render::RenderContext;
use crate::render::descriptor::{PropKind, PropSpec};
use crate::render::painter;
use crate::value::Value;

/// The interaction state a component is previewed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum State {
    #[default]
    Normal,
    Focused,
    Hovered,
    Selected,
    Disabled,
    Loading,
    Error,
    Empty,
}

impl State {
    pub const NAMES: &'static [&'static str] = &[
        "normal", "focused", "hovered", "selected", "disabled", "loading", "error", "empty",
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "normal" => Self::Normal,
            "focused" => Self::Focused,
            "hovered" => Self::Hovered,
            "selected" => Self::Selected,
            "disabled" => Self::Disabled,
            "loading" => Self::Loading,
            "error" => Self::Error,
            "empty" => Self::Empty,
            _ => return None,
        })
    }
}

/// The `state` property, or the older boolean ones (`focused`, ...).
pub fn state(c: &Component) -> State {
    if let Some(s) = c.prop_str("state").and_then(State::parse) {
        return s;
    }
    for (key, state) in [
        ("disabled", State::Disabled),
        ("focused", State::Focused),
        ("hovered", State::Hovered),
        ("selected", State::Selected),
    ] {
        if c.prop_bool(key) == Some(true) {
            return state;
        }
    }
    State::Normal
}

/// The `state` property spec shared by every interactive kind.
pub fn state_prop() -> PropSpec {
    PropSpec::new(
        "state",
        PropKind::choice(State::NAMES),
        "preview state (focused, disabled, loading, error, empty, ...)",
    )
    .default("normal")
}

/// The `bind` property spec: read data from the document's fixtures.
pub fn bind_prop() -> PropSpec {
    PropSpec::new(
        "bind",
        PropKind::Text,
        "key in the document's data fixtures to read rows/items/values from",
    )
}

/// A property, or the bound fixture when the component has `bind`.
pub fn value<'a>(c: &'a Component, ctx: &RenderContext<'a>, key: &str) -> Option<Cow<'a, Value>> {
    if let Some(bound) = c
        .prop_str("bind")
        .and_then(|k| ctx.data.and_then(|d| d.get(k)))
    {
        // A bound map can hold several fields (`rows`, `columns`, ...).
        if let Value::Map(m) = bound {
            if let Some(v) = m.get(key) {
                return Some(Cow::Borrowed(v));
            }
        } else {
            return Some(Cow::Borrowed(bound));
        }
    }
    c.prop(key).map(Cow::Borrowed)
}

/// Text of a value as shown in a cell.
pub fn text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Items of a list property: a list, or a comma-separated string.
pub fn strings(c: &Component, ctx: &RenderContext<'_>, key: &str) -> Vec<String> {
    match value(c, ctx, key).as_deref() {
        Some(Value::List(items)) => items.iter().map(text).collect(),
        Some(Value::Str(s)) => s
            .split(',')
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Rows of a table: a list of lists, a list of maps (in `columns` order),
/// or text with `;` between rows and `,` between cells.
pub fn rows(c: &Component, ctx: &RenderContext<'_>, columns: &[String]) -> Vec<Vec<String>> {
    match value(c, ctx, "rows").as_deref() {
        Some(Value::List(rows)) => rows
            .iter()
            .map(|r| match r {
                Value::List(cells) => cells.iter().map(text).collect(),
                Value::Map(m) => columns
                    .iter()
                    .map(|k| m.get(k).map(text).unwrap_or_default())
                    .collect(),
                other => vec![text(other)],
            })
            .collect(),
        Some(Value::Str(s)) => s
            .split(';')
            .filter(|r| !r.trim().is_empty())
            .map(|r| r.split(',').map(|c| c.trim().to_owned()).collect())
            .collect(),
        _ => Vec::new(),
    }
}

/// Whole numbers of a series: a list, or whitespace/comma separated text.
pub fn numbers(c: &Component, ctx: &RenderContext<'_>, key: &str) -> Vec<i64> {
    match value(c, ctx, key).as_deref() {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Int(n) => Some(*n),
                Value::Str(s) => s.trim().parse().ok(),
                _ => None,
            })
            .collect(),
        Some(Value::Str(s)) => s
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .filter_map(|p| p.parse().ok())
            .collect(),
        Some(Value::Int(n)) => vec![*n],
        _ => Vec::new(),
    }
}

/// An integer property with a default.
pub fn int(c: &Component, key: &str, default: i64) -> i64 {
    c.prop_int(key).unwrap_or(default)
}

/// A text property with a default.
pub fn str_or<'a>(c: &'a Component, key: &str, default: &'a str) -> &'a str {
    c.prop_str(key).unwrap_or(default)
}

/// The index `selected` names: a number, or an item's text.
pub fn selected_index(c: &Component, items: &[String]) -> Option<usize> {
    match c.prop("selected")? {
        Value::Int(n) => usize::try_from(*n).ok(),
        Value::Str(s) => s.parse::<usize>().ok().or_else(|| {
            items
                .iter()
                .position(|i| i == s || i.split('|').next() == Some(s))
        }),
        _ => None,
    }
}

/// Resolves a colour property (token or literal) with a default token.
pub fn color(c: &Component, ctx: &RenderContext<'_>, key: &str, default: &str) -> Color {
    c.prop_str(key)
        .map_or_else(|| ctx.theme.color(default), |t| ctx.theme.resolve(t))
}

pub fn style(ctx: &RenderContext<'_>, fg: &str, bg: &str) -> CellStyle {
    CellStyle::new(ctx.theme.color(fg), ctx.theme.color(bg))
}

#[must_use]
pub fn bold(mut s: CellStyle) -> CellStyle {
    s.attrs.bold = true;
    s
}

#[must_use]
pub fn dim(mut s: CellStyle) -> CellStyle {
    s.attrs.dim = true;
    s
}

#[must_use]
pub fn underline(mut s: CellStyle) -> CellStyle {
    s.attrs.underline = true;
    s
}

/// Text in a component's own colours (`color`, `background`, attributes).
pub fn own_style(c: &Component, ctx: &RenderContext<'_>, fg: &str, bg: &str) -> CellStyle {
    let mut attrs = Attributes::NONE;
    attrs.bold = c.prop_bool("bold") == Some(true);
    attrs.dim = c.prop_bool("dim") == Some(true);
    attrs.italic = c.prop_bool("italic") == Some(true);
    attrs.underline = c.prop_bool("underline") == Some(true);
    CellStyle {
        fg: color(c, ctx, "color", fg),
        bg: color(c, ctx, "background", bg),
        attrs,
    }
}

/// The colour token of a status level (`info`, `success`, ...).
pub fn level_token(level: &str) -> &'static str {
    match level {
        "success" | "ok" => "success",
        "warning" | "warn" => "warning",
        "error" | "danger" => "error",
        _ => "info",
    }
}

/// The glyph of a status level.
pub fn level_icon(level: &str) -> &'static str {
    match level {
        "success" | "ok" => "✓",
        "warning" | "warn" => "!",
        "error" | "danger" => "✗",
        _ => "i",
    }
}

/// The `level` property spec.
pub fn level_prop() -> PropSpec {
    PropSpec::new(
        "level",
        PropKind::choice(&["info", "success", "warning", "error"]),
        "status colour and icon",
    )
    .default("info")
}

/// Writes `text` clipped to the row `y` of `rect`, from `x`.
pub fn put(
    canvas: &mut crate::document::Canvas,
    x: u16,
    y: u16,
    text: &str,
    style: CellStyle,
    rect: Rect,
) -> u16 {
    if y < rect.y || u32::from(y) >= rect.bottom() {
        return 0;
    }
    painter::draw_text(canvas, x, y, text, style, rect)
}

/// `text` padded or cut to exactly `width` columns.
pub fn fit(text: &str, width: u16) -> String {
    let t = painter::truncate(text, width);
    let pad = usize::from(width.saturating_sub(painter::text_width(&t)));
    format!("{t}{}", " ".repeat(pad))
}

/// Scales `v` from `0..=max` to `0..=steps`, rounding.
pub fn scale(v: i64, max: i64, steps: u32) -> u32 {
    if max <= 0 {
        return 0;
    }
    let v = v.clamp(0, max);
    ((v as f64 / max as f64) * f64::from(steps)).round() as u32
}
