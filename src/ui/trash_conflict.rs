use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrashConflictDialogState {
    pub(crate) pane_id: fika_core::PaneId,
    pub(crate) conflicts: Vec<fika_core::file_ops::TrashRestoreConflict>,
}

pub(crate) fn trash_conflict_dialog_overlay(
    dialog: TrashConflictDialogState,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let pane_id = dialog.pane_id;
    let replace_conflicts = dialog.conflicts.clone();
    let row_conflicts = dialog.conflicts;

    div()
        .id("trash-conflict-dialog-layer")
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
                this.dismiss_trash_conflict_dialog();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .id("trash-conflict-dialog")
                .w(px(520.0))
                .max_w(px(680.0))
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
                                .truncate()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1f2328))
                                .child("Restore Conflict"),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0x59636e))
                                .hover(|button| button.bg(rgb(0xeaf1ff)))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                            this.dismiss_trash_conflict_dialog();
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .child("Close"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .child(div().text_sm().text_color(rgb(0x59636e)).child(format!(
                            "{} item(s) already exist at their original location.",
                            row_conflicts.len()
                        )))
                        .children(row_conflicts.into_iter().map(conflict_row)),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(rgb(0xd5d9df))
                        .child(dialog_button("skip", "Skip").on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                                this.dismiss_trash_conflict_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(
                            dialog_button("replace", "Replace Existing")
                                .bg(rgb(0x2563eb))
                                .text_color(rgb(0xffffff))
                                .hover(|button| button.bg(rgb(0x1d4ed8)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                            this.replace_trash_restore_conflicts(
                                                pane_id,
                                                replace_conflicts.clone(),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                ),
                        ),
                ),
        )
}

fn dialog_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(format!("trash-conflict-{id}"))
        .px_3()
        .py_1()
        .rounded_md()
        .text_sm()
        .text_color(rgb(0x1f2328))
        .border_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .hover(|button| button.bg(rgb(0xeaf1ff)))
        .cursor_pointer()
        .child(label)
}

fn conflict_row(conflict: fika_core::file_ops::TrashRestoreConflict) -> Stateful<Div> {
    div()
        .id(format!(
            "trash-conflict-row-{}",
            conflict.trash_path.display()
        ))
        .flex()
        .flex_col()
        .gap_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(0xe2e6eb))
        .bg(rgb(0xf8fafc))
        .px_2()
        .py_2()
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x1f2328))
                .truncate()
                .child(conflict.original_path.display().to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x6b7280))
                .truncate()
                .child(conflict.trash_path.display().to_string()),
        )
}
