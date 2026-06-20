use crate::FikaApp;
use fika_core::{OperationId, PaneId};
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

pub(crate) type BackgroundTaskId = OperationId;
type BackgroundTaskSummary = (
    String,
    String,
    Option<u8>,
    Option<BackgroundTaskId>,
    Option<BackgroundTaskState>,
);

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundTaskDetailDialog {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) state: Option<BackgroundTaskState>,
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

    let Some((title, detail, percent, task_id, state)) = summary else {
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
        .child(summary_header(
            title.clone(),
            detail.clone(),
            state,
            task_id,
            cx,
        ))
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
                .children(history.into_iter().map(|task| history_row(task, cx)))
        })
}

pub(crate) fn background_task_detail_dialog_overlay(
    dialog: BackgroundTaskDetailDialog,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let state = dialog.state;
    let lines = dialog
        .detail
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    div()
        .id("background-task-detail-layer")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .bg(rgba(0x00000066))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_background_task_detail_dialog();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .id("background-task-detail-dialog")
                .w(px(560.0))
                .max_w_full()
                .max_h(px(520.0))
                .flex()
                .flex_col()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1f2328))
                                .child(dialog.title),
                        )
                        .when_some(state, |row, state| {
                            row.child(
                                div()
                                    .text_xs()
                                    .text_color(match state {
                                        BackgroundTaskState::Complete => rgb(0x276749),
                                        BackgroundTaskState::Failed => rgb(0x9b1c1c),
                                    })
                                    .child(match state {
                                        BackgroundTaskState::Complete => "Complete",
                                        BackgroundTaskState::Failed => "Failed",
                                    }),
                            )
                        }),
                )
                .child(
                    div()
                        .id("background-task-detail-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px_4()
                        .py_3()
                        .text_sm()
                        .text_color(rgb(0x374151))
                        .children(lines.into_iter().map(|line| {
                            div().min_w_0().pb_1().child(if line.is_empty() {
                                " ".to_string()
                            } else {
                                line
                            })
                        })),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(rgb(0xd5d9df))
                        .child(
                            div()
                                .id("background-task-detail-close")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(0xc8ced6))
                                .text_sm()
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(0xeaf1ff)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                            this.dismiss_background_task_detail_dialog();
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .child("Close"),
                        ),
                ),
        )
}

fn background_tasks_summary(active: &[BackgroundTaskSnapshot]) -> Option<BackgroundTaskSummary> {
    let first = active.first()?;
    if active.len() == 1 {
        return Some((
            first.title.clone(),
            first.detail.clone(),
            first.percent,
            first.cancellable.then_some(first.id),
            None,
        ));
    }
    Some((
        format!("{} active tasks", active.len()),
        first.title.clone(),
        None,
        None,
        None,
    ))
}

fn background_task_history_summary_labels(
    snapshot: &BackgroundTaskHistorySnapshot,
) -> BackgroundTaskSummary {
    (
        snapshot.title.clone(),
        snapshot.detail.clone(),
        Some(match snapshot.state {
            BackgroundTaskState::Complete => 100,
            BackgroundTaskState::Failed => 100,
        }),
        None,
        Some(snapshot.state),
    )
}

fn summary_header(
    title: String,
    detail: String,
    state: Option<BackgroundTaskState>,
    task_id: Option<BackgroundTaskId>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let detail_title = title.clone();
    let detail_body = detail.clone();
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
        .child(
            div()
                .id("background-task-view")
                .flex_none()
                .px_2()
                .py_1()
                .rounded_md()
                .text_xs()
                .text_color(rgb(0x2f6fed))
                .hover(|button| button.bg(rgb(0xeaf1ff)))
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.standard_click() {
                            this.show_background_task_detail_dialog(
                                detail_title.clone(),
                                detail_body.clone(),
                                state,
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                )
                .child("View"),
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

fn history_row(snapshot: BackgroundTaskHistorySnapshot, cx: &mut Context<FikaApp>) -> Div {
    let color = match snapshot.state {
        BackgroundTaskState::Complete => rgb(0x276749),
        BackgroundTaskState::Failed => rgb(0x9b1c1c),
    };
    let state = snapshot.state;
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .pt_2()
        .border_t_1()
        .border_color(rgb(0xe2e6ec))
        .child(
            summary_header(
                snapshot.title,
                snapshot.detail.clone(),
                Some(state),
                None,
                cx,
            )
            .text_color(color),
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
                        .child(snapshot.title.clone()),
                )
                .child(
                    div()
                        .id(format!("background-task-view-{}", snapshot.id.0))
                        .flex_none()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .text_color(rgb(0x2f6fed))
                        .hover(|button| button.bg(rgb(0xeaf1ff)))
                        .cursor_pointer()
                        .on_click({
                            let title = snapshot.title.clone();
                            let detail = snapshot.detail.clone();
                            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                                if event.standard_click() {
                                    this.show_background_task_detail_dialog(
                                        title.clone(),
                                        detail.clone(),
                                        None,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            })
                        })
                        .child("View"),
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
