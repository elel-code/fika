use gpui::prelude::*;
use gpui::{Div, Stateful, Styled, div, rgb};

pub(crate) fn toolbar_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(format!("toolbar-{id}"))
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xb6bcc6))
        .bg(rgb(0xffffff))
        .hover(|button| button.bg(rgb(0xeaf1ff)))
        .cursor_pointer()
        .text_xs()
        .child(label)
}
