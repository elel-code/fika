use gpui::{InteractiveElement, IntoElement, Styled, div, px, rgb, rgba};

pub(super) fn place_row_background(active: bool, drop_target: bool) -> gpui::Rgba {
    if drop_target {
        place_drop_target_background()
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xf8f9fb)
    }
}

pub(super) fn place_row_border_color(active: bool, drop_target: bool) -> gpui::Rgba {
    if drop_target {
        place_drop_target_border_color()
    } else if active {
        rgb(0xbfdbfe)
    } else {
        rgba(0x00000000)
    }
}

pub(super) fn place_row_hover_background(active: bool, drop_target: bool) -> gpui::Rgba {
    if drop_target {
        place_drop_target_hover_background()
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xeef3f8)
    }
}

fn place_drop_target_background() -> gpui::Rgba {
    rgba(0xf59e0b34)
}

fn place_drop_target_hover_background() -> gpui::Rgba {
    rgba(0xf59e0b4a)
}

fn place_drop_target_border_color() -> gpui::Rgba {
    rgb(0xd97706)
}

pub(super) fn place_insert_indicator(id: String) -> impl IntoElement {
    div()
        .id(id)
        .mx_2()
        .h(px(2.0))
        .rounded_full()
        .bg(rgb(0x2f6fed))
}
