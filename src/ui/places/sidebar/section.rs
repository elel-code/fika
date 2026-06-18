mod dnd;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb};

use dnd::install_section_dnd;

use super::super::visual::PLACE_SECTION_HEADING_HEIGHT;

pub(super) fn group_heading(
    label: &'static str,
    insert_index: usize,
    custom_visual: bool,
    shell_targeting_enabled: bool,
    shell_dnd_enabled: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let mut heading = div()
        .id(format!("place-group-{label}"))
        .px_2()
        .pt_2()
        .pb_1()
        .text_xs()
        .text_color(rgb(0x6b7280));
    if shell_targeting_enabled {
        heading = heading.on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_place_section_context_menu(label, event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        );
    }
    let heading = if custom_visual {
        heading.h(px(PLACE_SECTION_HEADING_HEIGHT))
    } else {
        heading
    };
    let heading = if shell_dnd_enabled {
        install_section_dnd(heading, insert_index, cx)
    } else {
        heading
    }
    .child(label);

    div()
        .id(format!("place-group-wrap-{label}"))
        .relative()
        .flex()
        .flex_col()
        .child(heading)
}
