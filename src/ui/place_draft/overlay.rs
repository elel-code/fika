use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

use super::state::{PlaceDraft, PlaceDraftField};

pub(crate) fn place_draft_overlay(draft: PlaceDraft, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let title = if draft.editing_path.is_some() {
        "Edit Place"
    } else {
        "Add Place"
    };
    div()
        .id("place-draft-layer")
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
                this.dismiss_place_draft();
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
                .id("place-draft-dialog")
                .w(px(460.0))
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
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1f2328))
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .child(place_draft_field(
                            PlaceDraftField::Label,
                            "Label",
                            draft.label,
                            draft.focus == PlaceDraftField::Label,
                            cx,
                        ))
                        .child(place_draft_field(
                            PlaceDraftField::Path,
                            "Path",
                            draft.path,
                            draft.focus == PlaceDraftField::Path,
                            cx,
                        ))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .pt_1()
                                .child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .text_sm()
                                        .text_color(rgb(0x59636e))
                                        .hover(|button| button.bg(rgb(0xeaf1ff)))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this,
                                                 _event: &gpui::MouseDownEvent,
                                                 _window,
                                                 cx| {
                                                    this.dismiss_place_draft();
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                },
                                            ),
                                        )
                                        .child("Cancel"),
                                )
                                .child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(0x2f6fed))
                                        .text_sm()
                                        .text_color(rgb(0xffffff))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this,
                                                 _event: &gpui::MouseDownEvent,
                                                 _window,
                                                 cx| {
                                                    this.commit_place_draft();
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                },
                                            ),
                                        )
                                        .child("Save"),
                                ),
                        ),
                ),
        )
}

fn place_draft_field(
    field: PlaceDraftField,
    label: &'static str,
    value: String,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("place-draft-field-{field:?}"))
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(rgb(0x6b7280)).child(label))
        .child(
            div()
                .min_h(px(30.0))
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if focused {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xc8ced6)
                })
                .bg(if focused {
                    rgb(0xf3f7ff)
                } else {
                    rgb(0xffffff)
                })
                .text_sm()
                .text_color(rgb(0x24292f))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.set_place_draft_focus(field);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(if focused { format!("{value}|") } else { value }),
        )
}
