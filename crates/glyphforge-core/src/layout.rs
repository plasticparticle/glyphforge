//! The terminal layout model and its deterministic solver.
//!
//! Layout is intentionally simple: a component is either placed absolutely
//! inside its parent or flows along the parent's direction (horizontal,
//! vertical, stack or grid). Sizes are fixed, percentages, flex weights,
//! content-sized or fill. No CSS-like cascade, no floats, no fractional
//! cells: every result is an integer rectangle and the same input always
//! yields the same output.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::component::Component;
use crate::document::{Position, Rect, Size};
use crate::id::ObjectId;

/// How much of an axis a component wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dimension {
    /// Size to the content's intrinsic size (or fill when unknown).
    Content,
    /// Take the remaining space, sharing equally with other `Fill`/`Flex`.
    #[default]
    Fill,
    Fixed(u16),
    /// Percentage of the available space (0..=100).
    Percent(u8),
    /// Share of the remaining space, weighted.
    Flex(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid dimension {0:?}; expected content, fill, fixed(N), percent(N) or flex(N)")]
pub struct DimensionParseError(String);

impl FromStr for Dimension {
    type Err = DimensionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let err = || DimensionParseError(s.to_owned());
        match s {
            "content" => return Ok(Self::Content),
            "fill" => return Ok(Self::Fill),
            _ => {}
        }
        let (name, arg) = s.split_once('(').ok_or_else(err)?;
        let arg = arg.strip_suffix(')').ok_or_else(err)?.trim();
        match name.trim() {
            "fixed" => arg.parse().map(Self::Fixed).map_err(|_| err()),
            "percent" => arg
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= 100)
                .map(Self::Percent)
                .ok_or_else(err),
            "flex" => arg.parse().map(Self::Flex).map_err(|_| err()),
            _ => Err(err()),
        }
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content => f.write_str("content"),
            Self::Fill => f.write_str("fill"),
            Self::Fixed(n) => write!(f, "fixed({n})"),
            Self::Percent(n) => write!(f, "percent({n})"),
            Self::Flex(n) => write!(f, "flex({n})"),
        }
    }
}

impl Serialize for Dimension {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Dimension {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Flow direction of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Horizontal,
    #[default]
    Vertical,
    /// Children overlap, each getting the full inner area.
    Stack,
    /// Equal columns, rows added as needed.
    Grid,
}

/// Cross-axis alignment of a flow child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

/// Padding or margin per edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Edges {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Edges {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn all(n: u16) -> Self {
        Self {
            top: n,
            right: n,
            bottom: n,
            left: n,
        }
    }

    pub const fn symmetric(vertical: u16, horizontal: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub const fn horizontal(self) -> u16 {
        self.left.saturating_add(self.right)
    }

    pub const fn vertical(self) -> u16 {
        self.top.saturating_add(self.bottom)
    }

    /// Shrinks `r` by these edges, never below zero size.
    pub fn inset(self, r: Rect) -> Rect {
        Rect::new(
            r.x.saturating_add(self.left),
            r.y.saturating_add(self.top),
            r.width.saturating_sub(self.horizontal()),
            r.height.saturating_sub(self.vertical()),
        )
    }
}

/// Where a component sits inside its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Placement {
    /// Laid out by the parent's direction.
    #[default]
    Flow,
    /// Fixed offset from the parent's inner top-left corner.
    Absolute { x: u16, y: u16 },
}

/// Container behaviour for a component's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Container {
    pub direction: Direction,
    pub gap: u16,
    /// Number of columns for `Direction::Grid` (at least 1).
    pub columns: u16,
    pub align: Align,
}

/// Complete layout specification of one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub placement: Placement,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Option<u16>,
    pub max_width: Option<u16>,
    pub min_height: Option<u16>,
    pub max_height: Option<u16>,
    pub padding: Edges,
    pub margin: Edges,
    pub container: Container,
}

impl Layout {
    pub fn absolute(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            placement: Placement::Absolute { x, y },
            width: Dimension::Fixed(width),
            height: Dimension::Fixed(height),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn with_direction(mut self, direction: Direction) -> Self {
        self.container.direction = direction;
        self
    }

    #[must_use]
    pub const fn with_padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }

    fn clamp_width(self, w: u16) -> u16 {
        let w = self.min_width.map_or(w, |m| w.max(m));
        self.max_width.map_or(w, |m| w.min(m))
    }

    fn clamp_height(self, h: u16) -> u16 {
        let h = self.min_height.map_or(h, |m| h.max(m));
        self.max_height.map_or(h, |m| h.min(m))
    }
}

/// Provides intrinsic sizes and content insets for components; implemented
/// by the renderer registry so the solver stays independent of glyphs.
pub trait Measure {
    /// Intrinsic (content) size of a component without its children, or
    /// `None` when the component has no natural size.
    fn intrinsic_size(&self, component: &Component) -> Option<Size>;
    /// Extra inset the component's chrome (e.g. a border) consumes.
    fn chrome_inset(&self, component: &Component) -> Edges;
}

/// A measure that knows nothing: everything fills and nothing has chrome.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMeasure;

impl Measure for NoMeasure {
    fn intrinsic_size(&self, _: &Component) -> Option<Size> {
        None
    }

    fn chrome_inset(&self, _: &Component) -> Edges {
        Edges::ZERO
    }
}

/// Solved rectangles, keyed by component id, in absolute screen coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutResult {
    rects: BTreeMap<ObjectId, Rect>,
    /// The inner (content) rect of each component: outer rect minus chrome
    /// and padding.
    inner: BTreeMap<ObjectId, Rect>,
}

impl LayoutResult {
    pub fn rect(&self, id: &ObjectId) -> Option<Rect> {
        self.rects.get(id).copied()
    }

    pub fn inner(&self, id: &ObjectId) -> Option<Rect> {
        self.inner.get(id).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ObjectId, &Rect)> {
        self.rects.iter()
    }

    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }
}

/// Lays out `roots` (the top-level components of a layer) inside `area`.
/// Roots behave as children of an implicit stack container: each flow root
/// gets the whole area, absolute roots are offset from `area`'s corner.
pub fn solve(roots: &[Component], area: Rect, measure: &dyn Measure) -> LayoutResult {
    let mut result = LayoutResult::default();
    let implicit = Container {
        direction: Direction::Stack,
        ..Container::default()
    };
    place_children(roots, area, implicit, measure, &mut result);
    result
}

fn place_children(
    children: &[Component],
    inner: Rect,
    container: Container,
    measure: &dyn Measure,
    out: &mut LayoutResult,
) {
    // Absolute children first: they never affect the flow.
    let flow: Vec<&Component> = children
        .iter()
        .filter(|c| {
            if let Placement::Absolute { x, y } = c.layout.placement {
                let rect = absolute_rect(c, inner, x, y, measure);
                place(c, rect, measure, out);
                false
            } else {
                true
            }
        })
        .collect();
    if flow.is_empty() {
        return;
    }
    match container.direction {
        Direction::Stack => {
            for c in flow {
                let rect = stack_rect(c, inner, measure);
                place(c, rect, measure, out);
            }
        }
        Direction::Horizontal | Direction::Vertical => {
            let horizontal = container.direction == Direction::Horizontal;
            let rects = flow_rects(&flow, inner, container, horizontal, measure);
            for (c, rect) in flow.iter().zip(rects) {
                place(c, rect, measure, out);
            }
        }
        Direction::Grid => {
            let rects = grid_rects(&flow, inner, container, measure);
            for (c, rect) in flow.iter().zip(rects) {
                place(c, rect, measure, out);
            }
        }
    }
}

/// Records the component's rect and recurses into its children.
fn place(c: &Component, rect: Rect, measure: &dyn Measure, out: &mut LayoutResult) {
    let inner = c.layout.padding.inset(measure.chrome_inset(c).inset(rect));
    out.rects.insert(c.id.clone(), rect);
    out.inner.insert(c.id.clone(), inner);
    place_children(&c.children, inner, c.layout.container, measure, out);
}

fn resolve_axis(dim: Dimension, available: u16, intrinsic: Option<u16>) -> u16 {
    match dim {
        Dimension::Fixed(n) => n,
        Dimension::Percent(p) => (u32::from(available) * u32::from(p) / 100) as u16,
        Dimension::Content => intrinsic.unwrap_or(available),
        Dimension::Fill | Dimension::Flex(_) => available,
    }
    .min(available)
}

/// Content size including chrome and padding, if the component has one.
fn outer_intrinsic(c: &Component, measure: &dyn Measure) -> Option<Size> {
    let s = measure.intrinsic_size(c)?;
    let chrome = measure.chrome_inset(c);
    Some(Size::new(
        s.width
            .saturating_add(chrome.horizontal())
            .saturating_add(c.layout.padding.horizontal()),
        s.height
            .saturating_add(chrome.vertical())
            .saturating_add(c.layout.padding.vertical()),
    ))
}

fn absolute_rect(
    comp: &Component,
    inner: Rect,
    off_x: u16,
    off_y: u16,
    measure: &dyn Measure,
) -> Rect {
    let intrinsic = outer_intrinsic(comp, measure);
    let avail_w = inner.width.saturating_sub(off_x);
    let avail_h = inner.height.saturating_sub(off_y);
    let width = comp.layout.clamp_width(resolve_axis(
        comp.layout.width,
        avail_w,
        intrinsic.map(|s| s.width),
    ));
    let height = comp.layout.clamp_height(resolve_axis(
        comp.layout.height,
        avail_h,
        intrinsic.map(|s| s.height),
    ));
    Rect::new(
        inner.x.saturating_add(off_x),
        inner.y.saturating_add(off_y),
        width.min(avail_w),
        height.min(avail_h),
    )
}

fn stack_rect(c: &Component, inner: Rect, measure: &dyn Measure) -> Rect {
    let area = c.layout.margin.inset(inner);
    let intrinsic = outer_intrinsic(c, measure);
    let w = c.layout.clamp_width(resolve_axis(
        c.layout.width,
        area.width,
        intrinsic.map(|s| s.width),
    ));
    let h = c.layout.clamp_height(resolve_axis(
        c.layout.height,
        area.height,
        intrinsic.map(|s| s.height),
    ));
    Rect::new(area.x, area.y, w.min(area.width), h.min(area.height))
}

/// Main-axis allocation for a horizontal or vertical container.
fn flow_rects(
    children: &[&Component],
    inner: Rect,
    container: Container,
    horizontal: bool,
    measure: &dyn Measure,
) -> Vec<Rect> {
    let main_total = if horizontal {
        inner.width
    } else {
        inner.height
    };
    let gaps = container
        .gap
        .saturating_mul(children.len().saturating_sub(1) as u16);
    let available = main_total.saturating_sub(gaps);

    // Pass 1: everything that is not flexible.
    let mut sizes = vec![0u16; children.len()];
    let mut weights = vec![0u32; children.len()];
    let mut used: u32 = 0;
    for (i, c) in children.iter().enumerate() {
        let margin = if horizontal {
            c.layout.margin.horizontal()
        } else {
            c.layout.margin.vertical()
        };
        let dim = if horizontal {
            c.layout.width
        } else {
            c.layout.height
        };
        let intrinsic =
            outer_intrinsic(c, measure).map(|s| if horizontal { s.width } else { s.height });
        let clamp = |v: u16| {
            if horizontal {
                c.layout.clamp_width(v)
            } else {
                c.layout.clamp_height(v)
            }
        };
        match dim {
            Dimension::Fill => weights[i] = 1,
            Dimension::Flex(w) => weights[i] = u32::from(w.max(1)),
            other => {
                let v = clamp(resolve_axis(other, available, intrinsic));
                sizes[i] = v.saturating_add(margin);
                used += u32::from(sizes[i]);
            }
        }
    }

    // Pass 2: share the remainder among flexible children, largest remainder
    // rounding in order so the total is exact and deterministic.
    let remaining = u32::from(available).saturating_sub(used);
    let total_weight: u32 = weights.iter().sum();
    if total_weight > 0 {
        let mut assigned = 0u32;
        let mut fractions: Vec<(usize, u32)> = Vec::new();
        for (i, &w) in weights.iter().enumerate() {
            if w == 0 {
                continue;
            }
            let exact = remaining * w;
            let base = exact / total_weight;
            sizes[i] = base as u16;
            assigned += base;
            fractions.push((i, exact % total_weight));
        }
        let mut leftover = remaining - assigned;
        fractions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (i, _) in fractions {
            if leftover == 0 {
                break;
            }
            sizes[i] = sizes[i].saturating_add(1);
            leftover -= 1;
        }
        // Apply min/max clamps to flexible children after distribution; any
        // overflow is clipped by the parent, which keeps the pass simple.
        for (i, c) in children.iter().enumerate() {
            if weights[i] > 0 {
                let margin = if horizontal {
                    c.layout.margin.horizontal()
                } else {
                    c.layout.margin.vertical()
                };
                let content = sizes[i].saturating_sub(margin);
                let clamped = if horizontal {
                    c.layout.clamp_width(content)
                } else {
                    c.layout.clamp_height(content)
                };
                sizes[i] = clamped.saturating_add(margin);
            }
        }
    }

    // Pass 3: positions along the main axis, cross-axis alignment.
    let mut rects = Vec::with_capacity(children.len());
    let mut cursor: u32 = 0;
    let main_limit = u32::from(main_total);
    for (i, c) in children.iter().enumerate() {
        let start = cursor.min(main_limit);
        let size = u32::from(sizes[i]).min(main_limit - start) as u16;
        cursor += u32::from(sizes[i]) + u32::from(container.gap);

        let m = c.layout.margin;
        let cross_total = if horizontal {
            inner.height
        } else {
            inner.width
        };
        let cross_margin = if horizontal {
            m.vertical()
        } else {
            m.horizontal()
        };
        let cross_avail = cross_total.saturating_sub(cross_margin);
        let cross_dim = if horizontal {
            c.layout.height
        } else {
            c.layout.width
        };
        let intrinsic_cross =
            outer_intrinsic(c, measure).map(|s| if horizontal { s.height } else { s.width });
        let cross_size = match (container.align, cross_dim) {
            (Align::Stretch, Dimension::Fill | Dimension::Flex(_)) => cross_avail,
            (_, dim) => {
                let v = resolve_axis(dim, cross_avail, intrinsic_cross);
                if horizontal {
                    c.layout.clamp_height(v)
                } else {
                    c.layout.clamp_width(v)
                }
            }
        }
        .min(cross_avail);
        let cross_offset = match container.align {
            Align::Start | Align::Stretch => 0,
            Align::Center => (cross_avail - cross_size) / 2,
            Align::End => cross_avail - cross_size,
        };

        let rect = if horizontal {
            Rect::new(
                inner.x.saturating_add(start as u16).saturating_add(m.left),
                inner.y.saturating_add(m.top).saturating_add(cross_offset),
                size.saturating_sub(m.horizontal()),
                cross_size,
            )
        } else {
            Rect::new(
                inner.x.saturating_add(m.left).saturating_add(cross_offset),
                inner.y.saturating_add(start as u16).saturating_add(m.top),
                cross_size,
                size.saturating_sub(m.vertical()),
            )
        };
        rects.push(rect);
    }
    rects
}

/// Equal-width columns; row heights come from the tallest fixed/content
/// child in the row, otherwise rows share the height equally.
fn grid_rects(
    children: &[&Component],
    inner: Rect,
    container: Container,
    measure: &dyn Measure,
) -> Vec<Rect> {
    let columns = usize::from(container.columns.max(1));
    let rows = children.len().div_ceil(columns).max(1);
    let gap = u32::from(container.gap);
    let col_space = u32::from(inner.width).saturating_sub(gap * (columns as u32 - 1));
    let col_w = col_space / columns as u32;
    let col_extra = col_space % columns as u32;

    // Row heights.
    let mut row_heights = vec![0u32; rows];
    let mut flexible_rows = vec![true; rows];
    for (i, c) in children.iter().enumerate() {
        let row = i / columns;
        let intrinsic = outer_intrinsic(c, measure).map(|s| s.height);
        match c.layout.height {
            Dimension::Fill | Dimension::Flex(_) => {}
            other => {
                let h = c
                    .layout
                    .clamp_height(resolve_axis(other, inner.height, intrinsic));
                row_heights[row] = row_heights[row].max(u32::from(h));
                flexible_rows[row] = false;
            }
        }
    }
    let fixed_total: u32 = row_heights.iter().sum();
    let flex_rows = flexible_rows.iter().filter(|f| **f).count() as u32;
    let row_space = u32::from(inner.height)
        .saturating_sub(gap * (rows as u32 - 1))
        .saturating_sub(fixed_total);
    if flex_rows > 0 {
        let each = row_space / flex_rows;
        let mut extra = row_space % flex_rows;
        for (h, flexible) in row_heights.iter_mut().zip(&flexible_rows) {
            if *flexible {
                *h = each + u32::from(extra > 0);
                extra = extra.saturating_sub(1);
            }
        }
    }

    let mut rects = Vec::with_capacity(children.len());
    let mut y = u32::from(inner.y);
    let mut row_start_y = Vec::with_capacity(rows);
    for h in &row_heights {
        row_start_y.push(y);
        y += h + gap;
    }
    for (i, c) in children.iter().enumerate() {
        let (row, col) = (i / columns, i % columns);
        let x = u32::from(inner.x) + col as u32 * (col_w + gap) + col_extra.min(col as u32);
        let w = col_w + u32::from((col as u32) < col_extra);
        let h = row_heights[row];
        let rect = Rect::new(x as u16, row_start_y[row] as u16, w as u16, h as u16);
        rects.push(c.layout.margin.inset(rect));
    }
    rects
}

/// Convenience for callers that need a position rather than a rect.
pub fn origin(r: Rect) -> Position {
    Position::new(r.x, r.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;

    fn comp(id: &str, layout: Layout) -> Component {
        Component::new(ObjectId::new(id).unwrap(), "group").with_layout(layout)
    }

    fn fixed_w(id: &str, w: u16) -> Component {
        comp(
            id,
            Layout {
                width: Dimension::Fixed(w),
                ..Layout::default()
            },
        )
    }

    #[test]
    fn dimension_parses_and_prints() {
        for (s, d) in [
            ("content", Dimension::Content),
            ("fill", Dimension::Fill),
            ("fixed(24)", Dimension::Fixed(24)),
            ("percent(30)", Dimension::Percent(30)),
            ("flex(2)", Dimension::Flex(2)),
        ] {
            assert_eq!(s.parse::<Dimension>().unwrap(), d);
            assert_eq!(d.to_string(), s);
        }
        assert!("percent(120)".parse::<Dimension>().is_err());
        assert!("auto".parse::<Dimension>().is_err());
    }

    #[test]
    fn absolute_root_is_offset_and_clipped() {
        let roots = vec![
            comp("a", Layout::absolute(25, 3, 48, 12)),
            comp("b", Layout::absolute(70, 20, 30, 30)),
        ];
        let r = solve(&roots, Rect::new(0, 0, 80, 24), &NoMeasure);
        assert_eq!(r.rect(&id!("a")), Some(Rect::new(25, 3, 48, 12)));
        assert_eq!(
            r.rect(&id!("b")),
            Some(Rect::new(70, 20, 10, 4)),
            "clipped to the area"
        );
    }

    #[test]
    fn horizontal_sidebar_and_flex_content() {
        let mut root = comp(
            "root",
            Layout::default().with_direction(Direction::Horizontal),
        );
        root.children = vec![fixed_w("sidebar", 24), comp("content", Layout::default())];
        let r = solve(&[root], Rect::new(0, 0, 100, 30), &NoMeasure);
        assert_eq!(r.rect(&id!("sidebar")), Some(Rect::new(0, 0, 24, 30)));
        assert_eq!(r.rect(&id!("content")), Some(Rect::new(24, 0, 76, 30)));
    }

    #[test]
    fn flex_weights_share_remainder_deterministically() {
        let mut root = comp(
            "root",
            Layout::default().with_direction(Direction::Horizontal),
        );
        root.layout.container.gap = 1;
        root.children = vec![
            comp(
                "a",
                Layout {
                    width: Dimension::Flex(1),
                    ..Layout::default()
                },
            ),
            comp(
                "b",
                Layout {
                    width: Dimension::Flex(2),
                    ..Layout::default()
                },
            ),
            fixed_w("c", 10),
        ];
        // 100 - 2 gaps - 10 fixed = 88 to share 1:2 -> 29.33 / 58.67 -> 29 + 59
        let r = solve(&[root], Rect::new(0, 0, 100, 10), &NoMeasure);
        assert_eq!(r.rect(&id!("a")), Some(Rect::new(0, 0, 29, 10)));
        assert_eq!(r.rect(&id!("b")), Some(Rect::new(30, 0, 59, 10)));
        assert_eq!(r.rect(&id!("c")), Some(Rect::new(90, 0, 10, 10)));
    }

    #[test]
    fn percent_and_min_max_clamps() {
        let mut root = comp(
            "root",
            Layout::default().with_direction(Direction::Vertical),
        );
        root.children = vec![
            comp(
                "top",
                Layout {
                    height: Dimension::Percent(30),
                    ..Layout::default()
                },
            ),
            comp(
                "mid",
                Layout {
                    height: Dimension::Fill,
                    max_height: Some(5),
                    ..Layout::default()
                },
            ),
            comp(
                "bot",
                Layout {
                    height: Dimension::Fixed(2),
                    min_height: Some(4),
                    ..Layout::default()
                },
            ),
        ];
        let r = solve(&[root], Rect::new(0, 0, 40, 40), &NoMeasure);
        assert_eq!(r.rect(&id!("top")).unwrap().height, 12);
        assert_eq!(r.rect(&id!("mid")).unwrap().height, 5);
        assert_eq!(r.rect(&id!("bot")).unwrap().height, 4);
        assert_eq!(r.rect(&id!("bot")).unwrap().y, 17);
    }

    #[test]
    fn padding_and_margin_inset_children() {
        let mut root = comp("root", Layout::default().with_padding(Edges::all(1)));
        root.children = vec![comp(
            "child",
            Layout {
                margin: Edges::symmetric(0, 2),
                ..Layout::default()
            },
        )];
        let r = solve(&[root], Rect::new(0, 0, 20, 10), &NoMeasure);
        assert_eq!(r.inner(&id!("root")), Some(Rect::new(1, 1, 18, 8)));
        assert_eq!(r.rect(&id!("child")), Some(Rect::new(3, 1, 14, 8)));
    }

    #[test]
    fn grid_distributes_columns_and_rows() {
        let mut root = comp("root", Layout::default().with_direction(Direction::Grid));
        root.layout.container.columns = 2;
        root.layout.container.gap = 1;
        root.children = (0..4)
            .map(|i| comp(&format!("c{i}"), Layout::default()))
            .collect();
        let r = solve(&[root], Rect::new(0, 0, 21, 9), &NoMeasure);
        assert_eq!(r.rect(&id!("c0")), Some(Rect::new(0, 0, 10, 4)));
        assert_eq!(r.rect(&id!("c1")), Some(Rect::new(11, 0, 10, 4)));
        assert_eq!(r.rect(&id!("c2")), Some(Rect::new(0, 5, 10, 4)));
        assert_eq!(r.rect(&id!("c3")), Some(Rect::new(11, 5, 10, 4)));
    }

    #[test]
    fn content_sizing_uses_measure_and_chrome() {
        struct M;
        impl Measure for M {
            fn intrinsic_size(&self, c: &Component) -> Option<Size> {
                (c.kind == "label").then_some(Size::new(10, 1))
            }
            fn chrome_inset(&self, c: &Component) -> Edges {
                if c.kind == "panel" {
                    Edges::all(1)
                } else {
                    Edges::ZERO
                }
            }
        }
        let mut panel = Component::new(id!("p"), "panel").with_layout(Layout {
            height: Dimension::Content,
            ..Layout::default()
        });
        let label = Component::new(id!("l"), "label").with_layout(Layout {
            width: Dimension::Content,
            height: Dimension::Content,
            ..Layout::default()
        });
        panel.children = vec![label];
        let r = solve(&[panel], Rect::new(0, 0, 40, 20), &M);
        // Panel has no intrinsic size itself -> content falls back to fill.
        assert_eq!(r.rect(&id!("p")), Some(Rect::new(0, 0, 40, 20)));
        assert_eq!(r.inner(&id!("p")), Some(Rect::new(1, 1, 38, 18)));
        assert_eq!(r.rect(&id!("l")), Some(Rect::new(1, 1, 10, 1)));
    }

    #[test]
    fn stack_children_overlap() {
        let mut root = comp("root", Layout::default().with_direction(Direction::Stack));
        root.children = vec![comp("a", Layout::default()), comp("b", Layout::default())];
        let r = solve(&[root], Rect::new(2, 2, 10, 5), &NoMeasure);
        assert_eq!(r.rect(&id!("a")), r.rect(&id!("b")));
        assert_eq!(r.rect(&id!("a")), Some(Rect::new(2, 2, 10, 5)));
    }

    #[test]
    fn zero_area_does_not_panic() {
        let mut root = comp(
            "root",
            Layout::default().with_direction(Direction::Horizontal),
        );
        root.children = vec![fixed_w("a", 5), comp("b", Layout::default())];
        let r = solve(&[root], Rect::new(0, 0, 0, 0), &NoMeasure);
        assert_eq!(r.rect(&id!("a")).unwrap().width, 0);
    }

    #[test]
    fn layout_serde_round_trip_with_defaults_omitted() {
        let l = Layout::absolute(1, 2, 3, 4);
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"mode\":\"absolute\""));
        assert!(json.contains("\"width\":\"fixed(3)\""));
        let back: Layout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
        let sparse: Layout = serde_json::from_str(r#"{"width":"percent(30)"}"#).unwrap();
        assert_eq!(sparse.width, Dimension::Percent(30));
        assert_eq!(sparse.height, Dimension::Fill);
    }
}
