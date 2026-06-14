mod dnd;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, rgb};

use dnd::install_section_dnd;

pub(super) fn group_heading(
    label: &'static str,
    insert_index: usize,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let heading = div()
        .id(format!("place-group-{label}"))
        .px_2()
        .pt_2()
        .pb_1()
        .text_xs()
        .text_color(rgb(0x6b7280))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_place_section_context_menu(label, event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        );
    let heading = install_section_dnd(heading, insert_index, cx).child(label);

    div()
        .id(format!("place-group-wrap-{label}"))
        .relative()
        .flex()
        .flex_col()
        .child(heading)
}
