use crate::FikaApp;
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Bounds, Context, Div, Empty, ParentElement, Stateful, Styled, div, px, rgb};

pub(crate) const MIN_PANE_WIDTH: f32 = 1.0;
pub(crate) const PANE_SPLITTER_WIDTH: f32 = 1.0;
const PANE_SPLITTER_HITBOX_WIDTH: f32 = 8.0;
const SPLIT_RATIO_EPSILON: f32 = 0.0005;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PaneSplitterDrag {
    pub(crate) left: PaneId,
    pub(crate) right: PaneId,
}

pub(crate) fn width_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

pub(crate) fn split_ratio_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < SPLIT_RATIO_EPSILON
}

pub(crate) fn pane_width_available(row_width: f32, pane_count: usize) -> f32 {
    if pane_count == 0 {
        return 0.0;
    }
    (row_width - pane_count.saturating_sub(1) as f32 * PANE_SPLITTER_WIDTH).max(0.0)
}

pub(crate) fn normalize_pane_ratios(mut ratios: Vec<f32>) -> Vec<f32> {
    let count = ratios.len();
    if count == 0 {
        return ratios;
    }
    for ratio in &mut ratios {
        if !ratio.is_finite() || *ratio <= 0.0 {
            *ratio = 0.0;
        }
    }
    let total = ratios.iter().sum::<f32>();
    if total <= 0.0 {
        ratios.fill(1.0 / count as f32);
        return ratios;
    }
    for ratio in &mut ratios {
        *ratio /= total;
    }
    ratios
}

pub(crate) fn pane_splitter(
    left: PaneId,
    right: PaneId,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("pane-splitter-{}-{}", left.0, right.0))
        .relative()
        .flex_none()
        .w(px(PANE_SPLITTER_WIDTH))
        .h_full()
        .bg(rgb(0xc8ced6))
        .child(
            div()
                .id(format!("pane-splitter-hitbox-{}-{}", left.0, right.0))
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .left(px((PANE_SPLITTER_WIDTH - PANE_SPLITTER_HITBOX_WIDTH) / 2.0))
                .w(px(PANE_SPLITTER_HITBOX_WIDTH))
                .cursor_col_resize()
                .block_mouse_except_scroll()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.click_count() >= 2 && this.reset_pane_pair_ratio(left, right) {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_drag(PaneSplitterDrag { left, right }, |_, _, _, cx| {
                    cx.new(|_| Empty)
                }),
        )
        .hover(|splitter| splitter.bg(rgb(0x2f6fed)))
}

pub(crate) fn pane_row_width_from_child_bounds(bounds: &[Bounds<gpui::Pixels>]) -> Option<f32> {
    let first = bounds.first()?;
    let mut left = first.left();
    let mut right = first.right();
    for bound in bounds.iter().skip(1) {
        left = left.min(bound.left());
        right = right.max(bound.right());
    }
    Some((right - left).as_f32().max(0.0))
}
