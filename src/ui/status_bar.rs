mod progress;
mod space;
mod state;
mod summary;
mod zoom;

pub(crate) use state::{
    OperationProgressHandle, OperationProgressSnapshot, SpaceInfoCache, SpaceInfoSnapshot,
    StatusBarSnapshot, StatusSummaryCacheEntry, StatusSummaryCacheKey, filesystem_space_info,
    progress_delay_elapsed,
};
#[cfg(test)]
pub(crate) use state::{
    PROGRESS_DISPLAY_DELAY, parse_df_space_output, progress_percent, space_info_snapshot,
};
pub(crate) use summary::{status_summary_for_model, status_summary_for_model_indexes};
#[cfg(test)]
pub(crate) use zoom::zoom_level_for_track_x;

use crate::FikaApp;
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

use progress::{operation_busy_view, operation_progress_view};
use space::space_info;
use zoom::zoom_control;

const STATUS_PROGRESS_MIN_WIDTH: f32 = 320.0;
const STATUS_ZOOM_MIN_WIDTH: f32 = 520.0;
const STATUS_SPACE_MIN_WIDTH: f32 = 720.0;

pub(crate) fn status_bar(
    pane_id: PaneId,
    visible_width: f32,
    snapshot: StatusBarSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let StatusBarSnapshot {
        message,
        item_summary,
        free_space,
        zoom_level,
        zoom_icon_size,
        zoom_min,
        zoom_max,
        loading_pending,
        operation_progress,
    } = snapshot;
    let status_text = if message == "Ready" {
        item_summary
    } else {
        format!("{item_summary} - {message}")
    };
    let has_progress = operation_progress.is_some();
    let visible_width = visible_width.max(0.0).floor();
    let show_progress = visible_width >= STATUS_PROGRESS_MIN_WIDTH;
    let show_zoom = visible_width >= STATUS_ZOOM_MIN_WIDTH;
    let show_space = visible_width >= STATUS_SPACE_MIN_WIDTH;

    div()
        .id(format!("status-bar-{}", pane_id.0))
        .h(px(28.0))
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex_none()
        .overflow_hidden()
        .border_t_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .text_color(rgb(0x59636e))
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .max_w_full()
                .min_w_0()
                .flex_shrink_1()
                .overflow_hidden()
                .px_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .child(status_text),
                )
                .when(show_progress, |bar| {
                    bar.when_some(operation_progress, |bar, progress| {
                        bar.child(operation_progress_view(pane_id, progress, cx))
                    })
                })
                .when(show_progress && loading_pending && !has_progress, |bar| {
                    bar.child(operation_busy_view(pane_id))
                })
                .when(show_zoom, |bar| {
                    bar.child(zoom_control(
                        pane_id,
                        zoom_level,
                        zoom_icon_size,
                        zoom_min,
                        zoom_max,
                        cx,
                    ))
                })
                .when(show_space, |bar| bar.child(space_info(pane_id, free_space))),
        )
}

pub(super) fn status_section() -> Div {
    div()
        .flex()
        .items_center()
        .min_w_0()
        .flex_shrink_1()
        .overflow_hidden()
}

pub(super) fn fixed_status_text(width: f32, text: impl Into<String>) -> Div {
    div()
        .w(px(width))
        .min_w_0()
        .flex_shrink_1()
        .truncate()
        .child(text.into())
}
