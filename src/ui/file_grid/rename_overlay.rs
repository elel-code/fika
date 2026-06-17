use fika_core::{ItemLayout, PaneId};
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, Rgba, SharedString, div, px, rgb};

use crate::FikaApp;
use crate::ui::rename::RENAME_TEXT_INSET_X;

#[cfg(test)]
use super::layout;
use super::{ITEM_NAME_LINE_HEIGHT, ItemTileTextAlignment};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RenameTextLayout {
    pub(super) name_height: f32,
    pub(super) helper_height: f32,
}

const RENAME_NAME_HEIGHT: f32 = 20.0;

pub(super) fn rename_text_view(
    pane_id: PaneId,
    display_name: SharedString,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
    rename_error: Option<&str>,
    rename_warning: Option<&str>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let display_name_ref = display_name.as_ref();
    let visual = layout.visual_rect;
    let text = layout.text_rect;
    let show_helper = rename_error.is_some() || rename_warning.is_some();
    let rename_layout = rename_text_layout(text.height, show_helper);
    let helper_text = rename_error.or(rename_warning).unwrap_or_default();
    let helper_color = if rename_error.is_some() {
        rgb(0xdc2626)
    } else if rename_warning.is_some() {
        rgb(0xb45309)
    } else {
        rgb(0x6b7280)
    };
    let border_color = if rename_error.is_some() {
        rgb(0xdc2626)
    } else if rename_warning.is_some() {
        rgb(0xd97706)
    } else {
        rgb(0x2f6fed)
    };
    div()
        .absolute()
        .left(px(text.x - visual.x))
        .top(px(text.y - visual.y))
        .w(px(text.width))
        .h(px(text.height))
        .flex()
        .flex_col()
        .when(
            matches!(text_alignment, ItemTileTextAlignment::Start) && !show_helper,
            |view| view.justify_center(),
        )
        .child(
            rename_editor_view(
                pane_id,
                display_name_ref,
                selected,
                rename_caret,
                rename_selection,
                border_color,
                rename_layout.name_height,
                cx,
            )
            .when(
                matches!(text_alignment, ItemTileTextAlignment::Start),
                |editor| editor.relative().left(px(-1.0)).top(px(1.0)),
            ),
        )
        .when(show_helper, |view| {
            view.child(item_helper_label_view(
                helper_text,
                helper_color,
                rename_layout.helper_height,
                text_alignment,
            ))
        })
}

fn item_helper_label_view(
    helper_text: &str,
    helper_color: Rgba,
    height: f32,
    text_alignment: ItemTileTextAlignment,
) -> Div {
    match text_alignment {
        ItemTileTextAlignment::Start => div()
            .h(px(height))
            .min_h_0()
            .text_xs()
            .text_color(helper_color)
            .truncate()
            .child(helper_text.to_string()),
        ItemTileTextAlignment::Center => div()
            .h(px(height))
            .w_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .max_w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(helper_color)
                    .truncate()
                    .child(helper_text.to_string()),
            ),
    }
}

fn rename_editor_view(
    pane_id: PaneId,
    display_name: &str,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
    border_color: Rgba,
    height: f32,
    cx: &mut Context<FikaApp>,
) -> Div {
    div()
        .h(px(height))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .border_1()
        .rounded_sm()
        .border_color(border_color)
        .bg(rgb(0xffffff))
        .px(px(RENAME_TEXT_INSET_X))
        .cursor_text()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if this.set_rename_caret_from_window_position(pane_id, event.position) {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .child(rename_name_view(
            display_name,
            SharedString::from(display_name),
            true,
            selected,
            rename_caret,
            rename_selection,
        ))
}

fn rename_name_view(
    display_name: &str,
    display_name_text: SharedString,
    renaming: bool,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
) -> Div {
    let text_color = if selected {
        rgb(0x0f172a)
    } else {
        rgb(0x24292f)
    };
    let base = div()
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .text_sm()
        .line_height(px(ITEM_NAME_LINE_HEIGHT))
        .text_color(text_color)
        .when(renaming, |name| name.cursor_text());
    if !renaming {
        return base.whitespace_normal().child(display_name_text);
    }

    let base = base.whitespace_nowrap();
    if let Some((start, end)) = normalized_text_range(display_name, rename_selection) {
        return base
            .flex()
            .items_center()
            .child(display_name[..start].to_string())
            .child(
                div()
                    .bg(rgb(0xbfdbfe))
                    .text_color(rgb(0x0f172a))
                    .child(display_name[start..end].to_string()),
            )
            .child(display_name[end..].to_string());
    }

    let caret = clamp_text_boundary(display_name, rename_caret.unwrap_or(display_name.len()));
    base.flex()
        .items_center()
        .child(display_name[..caret].to_string())
        .child(rename_caret_view())
        .child(display_name[caret..].to_string())
}

fn rename_caret_view() -> Div {
    div().w(px(1.0)).h(px(16.0)).flex_none().bg(rgb(0x2f6fed))
}

pub(super) fn rename_text_layout(text_height: f32, show_helper: bool) -> RenameTextLayout {
    let text_height = text_height.max(0.0);
    let name_height = text_height.min(RENAME_NAME_HEIGHT);
    RenameTextLayout {
        name_height,
        helper_height: if show_helper {
            (text_height - name_height).max(0.0)
        } else {
            0.0
        },
    }
}

#[cfg(test)]
pub(super) fn display_text_layout(
    display_name: &str,
    text_width: f32,
    text_height: f32,
    text_alignment: ItemTileTextAlignment,
) -> RenameTextLayout {
    let text_height = text_height.max(0.0);
    if matches!(text_alignment, ItemTileTextAlignment::Center) {
        return RenameTextLayout {
            name_height: text_height,
            helper_height: 0.0,
        };
    }

    let required_name_height =
        layout::item_name_text_height_for_name(display_name, text_width).min(text_height);
    RenameTextLayout {
        name_height: required_name_height,
        helper_height: 0.0,
    }
}

pub(super) fn normalized_text_range(
    text: &str,
    range: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let (raw_start, raw_end) = range?;
    let start = clamp_text_boundary(text, raw_start.min(raw_end));
    let end = clamp_text_boundary(text, raw_start.max(raw_end));
    (start < end).then_some((start, end))
}

fn clamp_text_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
