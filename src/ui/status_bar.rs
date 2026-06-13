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
use gpui::{Context, Div, ParentElement, Rgba, Stateful, Styled, div, px, rgb};

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
        operation_pending,
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
                .when(show_progress && operation_pending && !has_progress, |bar| {
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

fn operation_progress_view(
    pane_id: PaneId,
    progress: OperationProgressSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let OperationProgressSnapshot {
        label,
        percent,
        cancellable,
        ..
    } = progress;
    let percent_value = percent.unwrap_or_default();
    let progress_width = 96.0 * f32::from(percent_value) / 100.0;
    let text = match percent {
        Some(percent) => format!("{label} {percent}%"),
        None => label,
    };
    status_section()
        .id(format!("status-operation-progress-{}", pane_id.0))
        .gap_2()
        .child(
            fixed_status_text(132.0, text)
                .text_xs()
                .text_color(rgb(0x59636e)),
        )
        .child(
            div()
                .relative()
                .w(px(96.0))
                .min_w_0()
                .flex_shrink_1()
                .h(px(6.0))
                .rounded_md()
                .bg(rgb(0xdce3ee))
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .w(px(progress_width.max(4.0)))
                        .h(px(6.0))
                        .rounded_md()
                        .bg(rgb(0x2f6fed)),
                ),
        )
        .when(cancellable, |row| {
            row.child(
                div()
                    .id(format!("status-operation-cancel-{}", pane_id.0))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .text_color(rgb(0x5f2a11))
                    .hover(|button| button.bg(rgb(0xffedd5)))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                            if event.standard_click() {
                                this.cancel_operation_or_loading(pane_id);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )
                    .child("Stop"),
            )
        })
}

fn operation_busy_view(pane_id: PaneId) -> Stateful<Div> {
    status_section()
        .id(format!("status-operation-progress-{}", pane_id.0))
        .gap_1()
        .child(div().text_xs().text_color(rgb(0x59636e)).child("Working"))
        .child(
            div()
                .relative()
                .w(px(72.0))
                .min_w_0()
                .flex_shrink_1()
                .h(px(6.0))
                .rounded_md()
                .bg(rgb(0xdce3ee))
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .w(px(28.0))
                        .h(px(6.0))
                        .rounded_md()
                        .bg(rgb(0x2f6fed)),
                ),
        )
}

fn space_info(pane_id: PaneId, space: Option<SpaceInfoSnapshot>) -> Stateful<Div> {
    match space {
        Some(space) => {
            let used_width = 72.0 * f32::from(space.used_percent) / 100.0;
            status_section()
                .id(format!("status-space-info-{}", pane_id.0))
                .gap_2()
                .text_xs()
                .text_color(rgb(0x59636e))
                .child(fixed_status_text(104.0, space.free_label))
                .child(
                    div()
                        .relative()
                        .w(px(72.0))
                        .min_w_0()
                        .flex_shrink_1()
                        .h(px(6.0))
                        .rounded_md()
                        .bg(rgb(0xe6e9ef))
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w(px(used_width))
                                .h(px(6.0))
                                .rounded_md()
                                .bg(space_usage_color(space.used_percent)),
                        ),
                )
                .child(fixed_status_text(152.0, space.detail_label).text_color(rgb(0x7a8494)))
        }
        None => div()
            .id(format!("status-space-info-{}", pane_id.0))
            .w(px(180.0))
            .min_w_0()
            .flex_shrink_1()
            .truncate()
            .text_xs()
            .text_color(rgb(0x7a8494))
            .child("Space unavailable"),
    }
}

fn space_usage_color(percent: u8) -> Rgba {
    if percent >= 90 {
        rgb(0xb42318)
    } else if percent >= 75 {
        rgb(0xb54708)
    } else {
        rgb(0x2f6fed)
    }
}
