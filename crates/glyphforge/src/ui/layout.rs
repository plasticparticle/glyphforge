//! Screen layout: header, side panels, canvas and status bar.

use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::UiState;

pub const LEFT_PANEL_WIDTH: u16 = 22;
pub const RIGHT_PANEL_WIDTH: u16 = 28;
/// Below this width the side panels are dropped automatically.
pub const MIN_WIDTH_FOR_PANELS: u16 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenLayout {
    pub header: Rect,
    pub left: Option<Rect>,
    /// The canvas block including its border.
    pub canvas_block: Rect,
    /// The drawable canvas area inside the border.
    pub canvas: Rect,
    pub right: Option<Rect>,
    pub status: Rect,
}

pub fn compute(area: Rect, ui: &UiState) -> ScreenLayout {
    let [header, middle, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    let panels_fit = area.width >= MIN_WIDTH_FOR_PANELS;
    let show_left = ui.show_left_panel && panels_fit;
    let show_right = ui.show_right_panel && panels_fit;

    let left_w = if show_left { LEFT_PANEL_WIDTH } else { 0 };
    let right_w = if show_right { RIGHT_PANEL_WIDTH } else { 0 };
    let [left, canvas_block, right] = Layout::horizontal([
        Constraint::Length(left_w),
        Constraint::Min(3),
        Constraint::Length(right_w),
    ])
    .areas(middle);

    let canvas = inner(canvas_block);
    ScreenLayout {
        header,
        left: show_left.then_some(left),
        canvas_block,
        canvas,
        right: show_right.then_some(right),
        status,
    }
}

/// The area inside a one-cell border.
fn inner(r: Rect) -> Rect {
    Rect {
        x: r.x.saturating_add(1),
        y: r.y.saturating_add(1),
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Focus;

    fn ui(left: bool, right: bool) -> UiState {
        UiState {
            show_left_panel: left,
            show_right_panel: right,
            help_open: false,
            focus: Focus::Canvas,
            vim_navigation: false,
        }
    }

    #[test]
    fn full_layout_gives_canvas_the_rest() {
        let l = compute(Rect::new(0, 0, 120, 40), &ui(true, true));
        assert_eq!(l.header, Rect::new(0, 0, 120, 1));
        assert_eq!(l.status, Rect::new(0, 39, 120, 1));
        assert_eq!(l.left, Some(Rect::new(0, 1, LEFT_PANEL_WIDTH, 38)));
        assert_eq!(
            l.right,
            Some(Rect::new(120 - RIGHT_PANEL_WIDTH, 1, RIGHT_PANEL_WIDTH, 38))
        );
        assert_eq!(
            l.canvas_block,
            Rect::new(
                LEFT_PANEL_WIDTH,
                1,
                120 - LEFT_PANEL_WIDTH - RIGHT_PANEL_WIDTH,
                38
            )
        );
        assert_eq!(
            l.canvas,
            Rect::new(
                LEFT_PANEL_WIDTH + 1,
                2,
                120 - LEFT_PANEL_WIDTH - RIGHT_PANEL_WIDTH - 2,
                36
            )
        );
    }

    #[test]
    fn hidden_panels_widen_the_canvas() {
        let l = compute(Rect::new(0, 0, 120, 40), &ui(false, false));
        assert_eq!(l.left, None);
        assert_eq!(l.right, None);
        assert_eq!(l.canvas_block, Rect::new(0, 1, 120, 38));
    }

    #[test]
    fn narrow_terminals_drop_panels() {
        let l = compute(Rect::new(0, 0, 60, 20), &ui(true, true));
        assert_eq!(l.left, None);
        assert_eq!(l.right, None);
        assert_eq!(l.canvas.width, 58);
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let l = compute(Rect::new(0, 0, 2, 2), &ui(true, true));
        assert_eq!(l.canvas.width, 0);
        assert_eq!(l.canvas.height, 0);
    }
}
