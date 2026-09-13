//! Geometry intelligence: alignment, distribution, equal sizing and
//! snapping.
//!
//! These are pure functions over rectangles that emit patch operations,
//! so the editor, the CLI and agents get identical results and the
//! behaviour is unit tested without a terminal.
//!
//! Alignment and distribution only act on absolutely placed components:
//! moving a component that flows inside a container would rip it out of
//! that container, which is never what the user meant. Such items are
//! reported as skipped instead.

use crate::document::Rect;
use crate::id::ObjectId;
use crate::layout::Dimension;
use crate::patch::Operation;

/// One component taking part in a geometry operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ObjectId,
    /// Current rectangle in screen coordinates.
    pub rect: Rect,
    /// Content rectangle of the parent. [`Operation::Move`] coordinates
    /// are relative to its top-left corner.
    pub parent_inner: Rect,
    /// Whether the component is placed absolutely and may be moved.
    pub movable: bool,
}

impl Item {
    fn move_to(&self, x: u16, y: u16) -> Option<Operation> {
        if (x, y) == (self.rect.x, self.rect.y) {
            return None;
        }
        Some(Operation::Move {
            target: self.id.clone(),
            x: x.saturating_sub(self.parent_inner.x),
            y: y.saturating_sub(self.parent_inner.y),
        })
    }
}

/// Which edge or centre line the selection lines up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Right,
    Top,
    Bottom,
    /// Line up the horizontal centres (move along x).
    CenterX,
    /// Line up the vertical centres (move along y).
    CenterY,
}

impl Align {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "left edges",
            Self::Right => "right edges",
            Self::Top => "top edges",
            Self::Bottom => "bottom edges",
            Self::CenterX => "horizontal centres",
            Self::CenterY => "vertical centres",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontally",
            Self::Vertical => "vertically",
        }
    }
}

/// What a geometry operation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub operations: Vec<Operation>,
    /// Components left alone because they are laid out by their parent.
    pub skipped: Vec<ObjectId>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

/// The smallest rectangle containing every non-empty input rectangle, or
/// an empty rectangle when there is none.
pub fn bounds(rects: impl IntoIterator<Item = Rect>) -> Rect {
    let mut it = rects.into_iter().filter(|r| !r.is_empty());
    let Some(first) = it.next() else {
        return Rect::new(0, 0, 0, 0);
    };
    let (mut x1, mut y1) = (u32::from(first.x), u32::from(first.y));
    let (mut x2, mut y2) = (first.right(), first.bottom());
    for r in it {
        x1 = x1.min(u32::from(r.x));
        y1 = y1.min(u32::from(r.y));
        x2 = x2.max(r.right());
        y2 = y2.max(r.bottom());
    }
    Rect::new(x1 as u16, y1 as u16, (x2 - x1) as u16, (y2 - y1) as u16)
}

/// The smallest rectangle containing every item.
pub fn bounding_box(items: &[Item]) -> Rect {
    bounds(items.iter().map(|i| i.rect))
}

/// Lines the items up inside their common bounding box.
pub fn align(items: &[Item], mode: Align) -> Plan {
    let bbox = bounding_box(items);
    let mut plan = Plan::default();
    if items.len() < 2 {
        return plan;
    }
    for item in items {
        if !item.movable {
            plan.skipped.push(item.id.clone());
            continue;
        }
        let r = item.rect;
        let (x, y) = match mode {
            Align::Left => (bbox.x, r.y),
            Align::Right => (
                (bbox.right().saturating_sub(u32::from(r.width))) as u16,
                r.y,
            ),
            Align::Top => (r.x, bbox.y),
            Align::Bottom => (
                r.x,
                (bbox.bottom().saturating_sub(u32::from(r.height))) as u16,
            ),
            Align::CenterX => (bbox.x + (bbox.width.saturating_sub(r.width)) / 2, r.y),
            Align::CenterY => (r.x, bbox.y + (bbox.height.saturating_sub(r.height)) / 2),
        };
        plan.operations.extend(item.move_to(x, y));
    }
    plan
}

/// Spreads the items evenly along `axis`, keeping the outermost two in
/// place. Needs at least three items, and does nothing when they
/// already overlap: spreading them would push the outermost item beyond
/// the bounding box the user can see.
pub fn distribute(items: &[Item], axis: Axis) -> Plan {
    let mut plan = Plan::default();
    let movable: Vec<&Item> = items
        .iter()
        .filter(|i| {
            if i.movable {
                true
            } else {
                plan.skipped.push(i.id.clone());
                false
            }
        })
        .collect();
    if movable.len() < 3 {
        return plan;
    }
    let bbox = bounding_box(items);
    let size = |r: Rect| {
        if axis == Axis::Horizontal {
            r.width
        } else {
            r.height
        }
    };
    let start = |r: Rect| if axis == Axis::Horizontal { r.x } else { r.y };

    let mut order: Vec<&Item> = movable;
    order.sort_by_key(|i| (start(i.rect), i.id.clone()));
    let total: u32 = order.iter().map(|i| u32::from(size(i.rect))).sum();
    let span = if axis == Axis::Horizontal {
        u32::from(bbox.width)
    } else {
        u32::from(bbox.height)
    };
    if total > span {
        return plan;
    }
    let free = span - total;
    let gaps = (order.len() - 1) as u32;
    let base = free / gaps;
    let extra = free % gaps;

    let mut cursor = u32::from(if axis == Axis::Horizontal {
        bbox.x
    } else {
        bbox.y
    });
    for (i, item) in order.iter().enumerate() {
        let pos = cursor as u16;
        let (x, y) = if axis == Axis::Horizontal {
            (pos, item.rect.y)
        } else {
            (item.rect.x, pos)
        };
        plan.operations.extend(item.move_to(x, y));
        cursor += u32::from(size(item.rect)) + base + u32::from((i as u32) < extra);
    }
    plan
}

/// Gives every item the largest size found along `axis`, leaving the
/// other axis as it is.
pub fn equalize(items: &[Item], axis: Axis) -> Plan {
    let mut plan = Plan::default();
    if items.len() < 2 {
        return plan;
    }
    let size = |r: Rect| {
        if axis == Axis::Horizontal {
            r.width
        } else {
            r.height
        }
    };
    let Some(target) = items.iter().map(|i| size(i.rect)).max() else {
        return plan;
    };
    for item in items {
        if size(item.rect) == target {
            continue;
        }
        let dim = Some(Dimension::Fixed(target));
        plan.operations.push(Operation::SetSize {
            target: item.id.clone(),
            width: if axis == Axis::Horizontal { dim } else { None },
            height: if axis == Axis::Vertical { dim } else { None },
        });
    }
    plan
}

// --- snapping -------------------------------------------------------------

/// Lines a dragged rectangle can snap to, in screen coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapTargets {
    pub vertical: Vec<u16>,
    pub horizontal: Vec<u16>,
}

impl SnapTargets {
    /// Adds the left, centre and right lines of `rect` as vertical
    /// targets and its top, centre and bottom lines as horizontal ones.
    /// The right and bottom lines are the exclusive edges, so a
    /// component snaps flush against its neighbour.
    pub fn add_rect(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.vertical
            .extend([rect.x, rect.x + rect.width / 2, rect.right() as u16]);
        self.horizontal
            .extend([rect.y, rect.y + rect.height / 2, rect.bottom() as u16]);
    }

    /// Adds every `step`-th column and row inside `area`.
    pub fn add_grid(&mut self, area: Rect, step: u16) {
        if step == 0 || area.is_empty() {
            return;
        }
        self.vertical
            .extend((area.x..area.right() as u16).step_by(usize::from(step)));
        self.horizontal
            .extend((area.y..area.bottom() as u16).step_by(usize::from(step)));
    }

    /// Sorts and removes duplicates so results are deterministic.
    pub fn normalize(&mut self) {
        self.vertical.sort_unstable();
        self.vertical.dedup();
        self.horizontal.sort_unstable();
        self.horizontal.dedup();
    }

    pub fn is_empty(&self) -> bool {
        self.vertical.is_empty() && self.horizontal.is_empty()
    }

    /// Explicit lines, mainly for tests and for callers that already know
    /// the geometry they want to snap to.
    pub fn from_lines(vertical: Vec<u16>, horizontal: Vec<u16>) -> Self {
        let mut t = Self {
            vertical,
            horizontal,
        };
        t.normalize();
        t
    }
}

/// A line to draw while a snap is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Guide {
    /// A column.
    Vertical(u16),
    /// A row.
    Horizontal(u16),
}

/// The snapped position and the lines that matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapped {
    pub x: u16,
    pub y: u16,
    pub guides: Vec<Guide>,
}

/// Pulls `rect` onto the nearest target lines within `threshold` cells.
///
/// Each axis is considered separately. The candidate lines of the
/// rectangle are its start edge, its exclusive end edge and its centre,
/// and the smallest movement wins; ties are resolved in that order, so
/// flush edges beat centre alignment and behaviour is stable.
pub fn snap(rect: Rect, targets: &SnapTargets, threshold: u16) -> Snapped {
    let (dx, gx) = best(rect.x, rect.width, &targets.vertical, threshold);
    let (dy, gy) = best(rect.y, rect.height, &targets.horizontal, threshold);
    let mut guides = Vec::new();
    if let Some(line) = gx {
        guides.push(Guide::Vertical(line));
    }
    if let Some(line) = gy {
        guides.push(Guide::Horizontal(line));
    }
    Snapped {
        x: (i32::from(rect.x) + dx).max(0) as u16,
        y: (i32::from(rect.y) + dy).max(0) as u16,
        guides,
    }
}

/// The smallest shift that lands one of the rectangle's three lines on a
/// target, plus the target it landed on.
fn best(start: u16, size: u16, targets: &[u16], threshold: u16) -> (i32, Option<u16>) {
    let lines = [start, start.saturating_add(size), start + size / 2];
    let mut best: Option<(i32, u16)> = None;
    for line in lines {
        for &t in targets {
            let delta = i32::from(t) - i32::from(line);
            if delta.unsigned_abs() > u32::from(threshold) {
                continue;
            }
            let better = match best {
                None => true,
                Some((d, _)) => delta.abs() < d.abs(),
            };
            if better {
                best = Some((delta, t));
            }
        }
    }
    match best {
        Some((delta, line)) => (delta, Some(line)),
        None => (0, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id;

    fn item(name: &str, x: u16, y: u16, w: u16, h: u16) -> Item {
        Item {
            id: ObjectId::new(name).unwrap(),
            rect: Rect::new(x, y, w, h),
            parent_inner: Rect::new(0, 0, 100, 40),
            movable: true,
        }
    }

    fn moves(plan: &Plan) -> Vec<(String, u16, u16)> {
        plan.operations
            .iter()
            .filter_map(|op| match op {
                Operation::Move { target, x, y } => Some((target.to_string(), *x, *y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn bounding_box_covers_every_item() {
        let items = [item("a", 2, 3, 10, 4), item("b", 20, 1, 5, 10)];
        assert_eq!(bounding_box(&items), Rect::new(2, 1, 23, 10));
        assert!(bounding_box(&[]).is_empty());
    }

    #[test]
    fn align_left_right_top_bottom() {
        let items = [item("a", 2, 3, 10, 4), item("b", 20, 8, 6, 2)];
        assert_eq!(moves(&align(&items, Align::Left)), vec![("b".into(), 2, 8)]);
        assert_eq!(
            moves(&align(&items, Align::Right)),
            vec![("a".into(), 16, 3)]
        );
        assert_eq!(moves(&align(&items, Align::Top)), vec![("b".into(), 20, 3)]);
        // bbox is y 3..10, so bottom aligns a to 10 - 4 = 6.
        assert_eq!(
            moves(&align(&items, Align::Bottom)),
            vec![("a".into(), 2, 6)]
        );
    }

    #[test]
    fn align_centres() {
        let items = [item("a", 0, 0, 10, 2), item("b", 0, 4, 4, 2)];
        // bbox x 0..10; centring b gives x = (10 - 4) / 2 = 3.
        assert_eq!(
            moves(&align(&items, Align::CenterX)),
            vec![("b".into(), 3, 4)]
        );
        // bbox y 0..6; a centres to (6 - 2) / 2 = 2, b to 2.
        assert_eq!(
            moves(&align(&items, Align::CenterY)),
            vec![("a".into(), 0, 2), ("b".into(), 0, 2)]
        );
    }

    #[test]
    fn align_is_relative_to_the_parent_content_box() {
        let mut a = item("a", 12, 8, 4, 2);
        let mut b = item("b", 20, 8, 4, 2);
        a.parent_inner = Rect::new(10, 5, 40, 20);
        b.parent_inner = Rect::new(10, 5, 40, 20);
        // Absolute target x is 12, so the relative coordinate is 2.
        assert_eq!(
            moves(&align(&[a, b], Align::Left)),
            vec![("b".into(), 2, 3)]
        );
    }

    #[test]
    fn align_needs_two_items_and_skips_flow_children() {
        assert!(align(&[item("a", 0, 0, 2, 2)], Align::Left).is_empty());
        let mut flow = item("b", 20, 8, 4, 2);
        flow.movable = false;
        let plan = align(&[item("a", 0, 0, 4, 2), flow], Align::Left);
        assert!(plan.operations.is_empty());
        assert_eq!(plan.skipped, vec![id!("b")]);
    }

    #[test]
    fn distribute_spreads_evenly_and_keeps_the_outer_items() {
        // Span 0..30 with three 4-wide items: 18 free over 2 gaps = 9 each.
        let items = [
            item("a", 0, 0, 4, 2),
            item("b", 5, 0, 4, 2),
            item("c", 26, 0, 4, 2),
        ];
        let plan = distribute(&items, Axis::Horizontal);
        assert_eq!(moves(&plan), vec![("b".into(), 13, 0)]);
        // Uneven free space is handed out from the left, deterministically.
        let items = [
            item("a", 0, 0, 4, 2),
            item("b", 5, 0, 4, 2),
            item("c", 25, 0, 4, 2),
        ];
        let plan = distribute(&items, Axis::Horizontal);
        assert_eq!(moves(&plan), vec![("b".into(), 13, 0)]);
    }

    #[test]
    fn distribute_sorts_by_position_not_by_input_order() {
        let items = [
            item("c", 20, 0, 2, 2),
            item("a", 0, 0, 2, 2),
            item("b", 4, 0, 2, 2),
        ];
        let plan = distribute(&items, Axis::Vertical);
        // All share y = 0 and height 2: three 2-cell items cannot be
        // spread across a 2-cell span, so nothing moves.
        assert!(plan.operations.is_empty());
        // Span y 0..22 holds 6 cells of content: 16 free over two gaps of
        // 8, which puts the last item back exactly where it was.
        let items = [
            item("a", 0, 0, 2, 2),
            item("b", 0, 3, 2, 2),
            item("c", 0, 20, 2, 2),
        ];
        assert_eq!(
            moves(&distribute(&items, Axis::Vertical)),
            vec![("b".into(), 0, 10)]
        );
    }

    #[test]
    fn distribute_needs_three_items() {
        let items = [item("a", 0, 0, 2, 2), item("b", 10, 0, 2, 2)];
        assert!(distribute(&items, Axis::Horizontal).is_empty());
        let overlapping = [
            item("a", 0, 0, 9, 2),
            item("b", 1, 0, 9, 2),
            item("c", 2, 0, 9, 2),
        ];
        assert!(
            distribute(&overlapping, Axis::Horizontal).is_empty(),
            "overlapping items cannot be spread without growing the selection"
        );
    }

    #[test]
    fn equalize_uses_the_largest_size_and_one_axis() {
        let items = [item("a", 0, 0, 4, 2), item("b", 10, 0, 9, 5)];
        let plan = equalize(&items, Axis::Horizontal);
        assert_eq!(
            plan.operations,
            vec![Operation::SetSize {
                target: id!("a"),
                width: Some(Dimension::Fixed(9)),
                height: None,
            }]
        );
        let plan = equalize(&items, Axis::Vertical);
        assert_eq!(
            plan.operations,
            vec![Operation::SetSize {
                target: id!("a"),
                width: None,
                height: Some(Dimension::Fixed(5)),
            }]
        );
    }

    #[test]
    fn equalize_works_for_flow_children_too() {
        let mut flow = item("b", 0, 0, 2, 2);
        flow.movable = false;
        let plan = equalize(&[item("a", 0, 0, 6, 2), flow], Axis::Horizontal);
        assert_eq!(plan.operations.len(), 1);
        assert!(plan.skipped.is_empty());
    }

    fn targets(rects: &[Rect]) -> SnapTargets {
        let mut t = SnapTargets::default();
        for r in rects {
            t.add_rect(*r);
        }
        t.normalize();
        t
    }

    #[test]
    fn snap_pulls_the_nearest_edge_and_reports_a_guide() {
        let t = targets(&[Rect::new(10, 5, 20, 6)]);
        // Left edge one cell off the neighbour's left edge.
        let s = snap(Rect::new(11, 20, 4, 2), &t, 1);
        assert_eq!(s.x, 10);
        assert_eq!(s.guides, vec![Guide::Vertical(10)]);
        // Right edge flush against the neighbour's left edge.
        let s = snap(Rect::new(7, 20, 4, 2), &t, 1);
        assert_eq!(s.x, 6, "the end edge at 11 snaps flush onto 10");
        // Both axes at once, against lines chosen so each one has to move.
        let t = SnapTargets::from_lines(vec![10], vec![5]);
        let s = snap(Rect::new(11, 6, 4, 2), &t, 1);
        assert_eq!((s.x, s.y), (10, 5));
        assert_eq!(s.guides, vec![Guide::Vertical(10), Guide::Horizontal(5)]);
    }

    #[test]
    fn an_alignment_that_needs_no_movement_wins() {
        // The rectangle's end edge already sits on a target line, so that
        // guide is reported and the rectangle stays where it is, even
        // though its start edge is one cell off another line.
        let t = SnapTargets::from_lines(Vec::new(), vec![5, 8]);
        let s = snap(Rect::new(0, 6, 4, 2), &t, 1);
        assert_eq!(s.y, 6);
        assert_eq!(s.guides, vec![Guide::Horizontal(8)]);
    }

    #[test]
    fn snap_leaves_distant_rectangles_alone() {
        let t = targets(&[Rect::new(10, 5, 20, 6)]);
        let s = snap(Rect::new(50, 30, 4, 2), &t, 1);
        assert_eq!((s.x, s.y), (50, 30));
        assert!(s.guides.is_empty());
        let empty = SnapTargets::default();
        assert!(empty.is_empty());
        let s = snap(Rect::new(11, 6, 4, 2), &empty, 2);
        assert_eq!((s.x, s.y), (11, 6));
    }

    #[test]
    fn snap_prefers_the_smallest_movement() {
        // Targets at 10 and 14; a 4-wide rect at 12 has left 12, centre 14,
        // right 16. The centre match needs no movement at all.
        let t = SnapTargets::from_lines(vec![10, 14], Vec::new());
        let s = snap(Rect::new(12, 0, 4, 2), &t, 3);
        assert_eq!(s.x, 12);
        assert_eq!(s.guides, vec![Guide::Vertical(14)]);
    }

    #[test]
    fn snap_never_moves_a_rectangle_off_the_canvas() {
        let t = SnapTargets::from_lines(vec![0], Vec::new());
        let s = snap(Rect::new(1, 0, 4, 2), &t, 5);
        assert_eq!(s.x, 0);
    }

    #[test]
    fn grid_targets_step_across_the_area() {
        let mut t = SnapTargets::default();
        t.add_grid(Rect::new(0, 0, 10, 4), 4);
        t.normalize();
        assert_eq!(t.vertical, vec![0, 4, 8]);
        assert_eq!(t.horizontal, vec![0]);
    }
}
