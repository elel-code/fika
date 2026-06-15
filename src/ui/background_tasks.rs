use crate::FikaApp;
use fika_core::{OperationId, PaneId};
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

pub(crate) type BackgroundTaskId = OperationId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundTasksSnapshot {
    pub(crate) active: Vec<BackgroundTaskSnapshot>,
    pub(crate) history: Vec<BackgroundTaskHistorySnapshot>,
    pub(crate) expanded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundTaskSnapshot {
    pub(crate) id: BackgroundTaskId,
    pub(crate) pane_id: PaneId,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) percent: Option<u8>,
    pub(crate) cancellable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundTaskHistorySnapshot {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) state: BackgroundTaskState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackgroundTaskState {
    Complete,
    Failed,
}

pub(crate) fn background_tasks_panel(
    snapshot: BackgroundTasksSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let BackgroundTasksSnapshot {
        active,
        history,
        expanded,
    } = snapshot;
    let has_history = !history.is_empty();
    let has_active = !active.is_empty();
    let summary = background_tasks_summary(&active)
        .or_else(|| history.first().map(background_task_history_summary_labels));

    let Some((title, detail, percent, task_id)) = summary else {
        return div().id("background-tasks-empty");
    };

    div()
        .id("background-tasks-panel")
        .flex()
        .flex_col()
        .gap_2()
        .flex_none()
        .mt_2()
        .mx_1()
        .p_2()
        .border_t_1()
        .border_color(rgb(0xd5d9df))
        .child(summary_header(title, task_id, cx))
        .when(has_active, |panel| panel.child(progress_bar(percent)))
        .child(task_detail_text(detail))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("background-tasks-details")
                        .px_0()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(0x2f6fed))
                        .hover(|button| button.text_color(rgb(0x174ea6)))
                        .cursor_pointer()
                        .on_click(cx.listener(|_this, event: &gpui::ClickEvent, _window, cx| {
                            if event.standard_click() {
                                _this.toggle_background_tasks_details();
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }))
                        .child(if expanded { "Hide" } else { "Details" }),
                )
                .child(div().flex_1())
                .when(has_history, |row| {
                    row.child(
                        div()
                            .id("background-tasks-clear")
                            .px_0()
                            .py_1()
                            .text_xs()
                            .text_color(rgb(0x59636e))
                            .hover(|button| button.text_color(rgb(0x24292f)))
                            .cursor_pointer()
                            .on_click(cx.listener(
                                |_this, event: &gpui::ClickEvent, _window, cx| {
                                    if event.standard_click() {
                                        _this.clear_background_task_history();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .child("Clear"),
                    )
                }),
        )
        .when(expanded, |panel| {
            panel
                .children(active.into_iter().map(|task| active_row(task, cx)))
                .children(history.into_iter().map(history_row))
        })
}

fn background_tasks_summary(
    active: &[BackgroundTaskSnapshot],
) -> Option<(String, String, Option<u8>, Option<BackgroundTaskId>)> {
    let first = active.first()?;
    if active.len() == 1 {
        return Some((
            first.title.clone(),
            first.detail.clone(),
            first.percent,
            first.cancellable.then_some(first.id),
        ));
    }
    Some((
        format!("{} active tasks", active.len()),
        first.title.clone(),
        None,
        None,
    ))
}

fn background_task_history_summary_labels(
    snapshot: &BackgroundTaskHistorySnapshot,
) -> (String, String, Option<u8>, Option<BackgroundTaskId>) {
    (
        snapshot.title.clone(),
        snapshot.detail.clone(),
        Some(match snapshot.state {
            BackgroundTaskState::Complete => 100,
            BackgroundTaskState::Failed => 100,
        }),
        None,
    )
}

fn summary_header(
    title: String,
    task_id: Option<BackgroundTaskId>,
    cx: &mut Context<FikaApp>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0x24292f))
                .child(title),
        )
        .when_some(task_id, |row, task_id| {
            row.child(
                div()
                    .id("background-task-cancel")
                    .flex_none()
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
                                this.cancel_background_operation(task_id);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )
                    .child("Stop"),
            )
        })
}

fn progress_bar(percent: Option<u8>) -> Div {
    let progress_width = percent.map_or(28.0, |percent| {
        144.0 * f32::from(percent).clamp(0.0, 100.0) / 100.0
    });
    div()
        .relative()
        .w(px(144.0))
        .max_w_full()
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
        )
}

fn task_detail_text(text: String) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_xs()
        .text_color(rgb(0x59636e))
        .child(text)
}

fn history_row(snapshot: BackgroundTaskHistorySnapshot) -> Div {
    let color = match snapshot.state {
        BackgroundTaskState::Complete => rgb(0x276749),
        BackgroundTaskState::Failed => rgb(0x9b1c1c),
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .pt_2()
        .border_t_1()
        .border_color(rgb(0xe2e6ec))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(color)
                .child(snapshot.title),
        )
        .child(task_detail_text(snapshot.detail))
}

fn active_row(snapshot: BackgroundTaskSnapshot, cx: &mut Context<FikaApp>) -> Div {
    let stop_id = snapshot.cancellable.then_some(snapshot.id);
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .pt_2()
        .border_t_1()
        .border_color(rgb(0xe2e6ec))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(rgb(0x2f6fed))
                        .child(snapshot.title),
                )
                .when_some(stop_id, |row, task_id| {
                    row.child(
                        div()
                            .id(format!("background-task-cancel-{}", task_id.0))
                            .flex_none()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_xs()
                            .text_color(rgb(0x5f2a11))
                            .hover(|button| button.bg(rgb(0xffedd5)))
                            .cursor_pointer()
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, _window, cx| {
                                    if event.standard_click() {
                                        this.cancel_background_operation(task_id);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .child("Stop"),
                    )
                }),
        )
        .child(progress_bar(snapshot.percent))
        .child(task_detail_text(snapshot.detail))
}
