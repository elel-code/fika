use crate::{FikaApp, PlaceSnapshot};
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

pub(crate) fn places_sidebar(
    places: Vec<PlaceSnapshot>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let mut rows = Vec::new();
    let mut current_group = None;

    for (index, place) in places.into_iter().enumerate() {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                rows.push(group_heading(place.group));
            }
        }
        rows.push(place_row(index, place, cx));
    }

    div()
        .id("places-sidebar")
        .flex()
        .flex_col()
        .w(px(220.0))
        .min_w(px(200.0))
        .h_full()
        .my_2()
        .ml_2()
        .border_1()
        .rounded_lg()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xf8f9fb))
        .px_2()
        .py_2()
        .child(
            div()
                .px_2()
                .pb_2()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .text_color(rgb(0x24292f))
                .child("Places"),
        )
        .children(rows)
}

fn group_heading(label: &'static str) -> Stateful<Div> {
    div()
        .id(format!("place-group-{label}"))
        .px_2()
        .pt_2()
        .pb_1()
        .text_xs()
        .text_color(rgb(0x6b7280))
        .child(label)
}

fn place_row(index: usize, place: PlaceSnapshot, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let path = place.path.clone();
    div()
        .id(format!("place-{index}"))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if place.active {
            rgb(0xeaf1ff)
        } else {
            rgb(0xf8f9fb)
        })
        .hover(|row| row.bg(rgb(0xeef3f8)))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.open_place(path.clone());
            cx.notify();
        }))
        .child(
            div()
                .w(px(28.0))
                .text_xs()
                .text_color(rgb(0x59636e))
                .child(place.marker),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_sm()
                .text_color(if place.active {
                    rgb(0x1f4fbf)
                } else {
                    rgb(0x24292f)
                })
                .child(place.label),
        )
}
