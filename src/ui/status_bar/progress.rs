use crate::FikaApp;
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

use super::state::OperationProgressSnapshot;
use super::{fixed_status_text, status_section};

pub(super) fn operation_progress_view(
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

pub(super) fn operation_busy_view(pane_id: PaneId) -> Stateful<Div> {
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
