use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PropertiesDialogState {
    pub(crate) title: String,
    pub(crate) rows: Vec<PropertyRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PropertyRow {
    pub(crate) label: &'static str,
    pub(crate) value: String,
}

pub(crate) fn properties_dialog_overlay(
    dialog: PropertiesDialogState,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let title = dialog.title;
    div()
        .id("properties-dialog-layer")
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
                this.dismiss_properties_dialog();
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
                .id("properties-dialog")
                .w(px(440.0))
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
                                .child(title),
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
                                            this.dismiss_properties_dialog();
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
                        .gap_1()
                        .px_4()
                        .py_3()
                        .children(dialog.rows.into_iter().map(property_dialog_row)),
                ),
        )
}

fn property_dialog_row(row: PropertyRow) -> Stateful<Div> {
    div()
        .id(format!("property-row-{}", row.label))
        .flex()
        .items_center()
        .gap_3()
        .py_1()
        .child(
            div()
                .w(px(92.0))
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(row.label),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_sm()
                .text_color(rgb(0x24292f))
                .child(row.value),
        )
}
