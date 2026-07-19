use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::context_menu::paint::push_context_menu_shadow;
use crate::shell::menu_geometry::scaled_context_menu_metric;
use crate::shell::metrics::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_TEXT_LINE_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING,
};
use crate::shell::overflow_menu::{ShellOverflowMenu, overflow_menu_items, overflow_menu_rect};
use crate::shell::theme::ShellTheme;
use crate::{LabelAlignment, QuadVertex, TextFrameBuilder, push_clipped_rounded_rect, push_rect};

pub(crate) fn push_overflow_menu_overlay(
    menu: &ShellOverflowMenu,
    show_hidden: bool,
    places_visible: bool,
    dark_mode: bool,
    theme: ShellTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let rect = overflow_menu_rect(menu, size, scale);
    let clip = screen_rect(size);
    let surface = if theme.is_dark() {
        theme.field()
    } else {
        [1.0, 1.0, 1.0, 1.0]
    };
    let border = theme.divider();
    let hover = theme.toolbar_button(true).fill;
    let text_color = theme.primary_text();
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let horizontal_padding = scaled_context_menu_metric(12.0, scale);
    let switch_width = scaled_context_menu_metric(28.0, scale);
    let switch_height = scaled_context_menu_metric(14.0, scale);

    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        surface,
        size,
    );
    for (row, item) in overflow_menu_items(show_hidden, places_visible, dark_mode)
        .iter()
        .enumerate()
    {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        let hovered = menu.hovered_row == Some(row);
        if hovered {
            push_rect(vertices, row_rect, hover, size);
        }
        let switch_rect = ViewRect {
            x: row_rect.right() - horizontal_padding - switch_width,
            y: row_rect.y + (row_rect.height - switch_height) / 2.0,
            width: switch_width,
            height: switch_height,
        };
        text.push_label_aligned(
            item.label,
            ViewRect {
                x: row_rect.x + horizontal_padding,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (switch_rect.x - row_rect.x - horizontal_padding * 2.0).max(1.0),
                height: text_height,
            },
            rect,
            if hovered {
                theme.accent_text()
            } else {
                text_color
            },
            LabelAlignment::Start,
        );
        push_switch(vertices, switch_rect, rect, item.active, theme, scale, size);
    }
    crate::push_clipped_rect_outline(vertices, rect, clip, 1.0, border, size);
}

fn push_switch(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    active: bool,
    theme: ShellTheme,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let track = if active {
        theme.accent()
    } else {
        theme.divider()
    };
    push_clipped_rounded_rect(vertices, rect, clip, rect.height / 2.0, track, size);
    let inset = scaled_context_menu_metric(2.0, scale);
    let knob_size = (rect.height - inset * 2.0).max(1.0);
    let knob = ViewRect {
        x: if active {
            rect.right() - inset - knob_size
        } else {
            rect.x + inset
        },
        y: rect.y + inset,
        width: knob_size,
        height: knob_size,
    };
    let knob_color = if active || theme.is_dark() {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.36, 0.39, 0.44, 1.0]
    };
    push_clipped_rounded_rect(vertices, knob, clip, knob_size / 2.0, knob_color, size);
}

fn screen_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}
