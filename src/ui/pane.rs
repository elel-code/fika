use crate::{BreadcrumbSegment, FikaApp, PaneSnapshot};
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb};

use super::file_grid::{FileGridMode, FileGridProps, file_grid};

pub(crate) struct PaneProps {
    pub snapshot: PaneSnapshot,
    pub file_grid_mode: FileGridMode,
}

pub(crate) fn pane_view(props: PaneProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let PaneProps {
        snapshot,
        file_grid_mode,
    } = props;
    let PaneSnapshot {
        id: pane_id,
        breadcrumbs,
        location_draft,
        item_count,
        layout,
        visible_items,
        view,
        rubber_band,
        focused,
    } = snapshot;
    let border = if focused {
        rgb(0x2f6fed)
    } else {
        rgb(0xb6bcc6)
    };
    div()
        .id(format!("pane-{}", pane_id.0))
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(280.0))
        .border_1()
        .rounded_md()
        .border_color(border)
        .bg(rgb(0xffffff))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.panes.focus(pane_id);
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(rgb(0xd5d9df))
                .bg(if focused {
                    rgb(0xeaf1ff)
                } else {
                    rgb(0xf6f7f9)
                })
                .child(location_bar(
                    pane_id,
                    breadcrumbs,
                    location_draft,
                    focused,
                    cx,
                )),
        )
        .child(file_grid(
            FileGridProps {
                pane_id,
                layout,
                visible_items,
                view,
                rubber_band,
                mode: file_grid_mode,
            },
            cx,
        ))
        .child(
            div()
                .px_2()
                .py_1()
                .border_t_1()
                .border_color(rgb(0xd5d9df))
                .text_xs()
                .text_color(rgb(0x59636e))
                .child(format!("{item_count} item(s)")),
        )
}

fn location_bar(
    pane_id: fika_core::PaneId,
    breadcrumbs: Vec<BreadcrumbSegment>,
    location_draft: Option<String>,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    match location_draft {
        Some(draft) => editable_location_bar(pane_id, draft, focused),
        None => breadcrumb_location_bar(pane_id, breadcrumbs, focused, cx),
    }
}

fn editable_location_bar(
    pane_id: fika_core::PaneId,
    draft: String,
    focused: bool,
) -> Stateful<Div> {
    div()
        .id(format!("location-edit-{}", pane_id.0))
        .flex()
        .items_center()
        .flex_1()
        .h(px(28.0))
        .px_2()
        .border_1()
        .rounded_md()
        .border_color(if focused {
            rgb(0x2f6fed)
        } else {
            rgb(0xb6bcc6)
        })
        .bg(rgb(0xffffff))
        .overflow_hidden()
        .child(
            div()
                .flex_1()
                .truncate()
                .text_sm()
                .text_color(rgb(0x111827))
                .child(draft),
        )
        .child(div().w(px(1.0)).h(px(18.0)).bg(if focused {
            rgb(0x2f6fed)
        } else {
            rgb(0x94a3b8)
        }))
}

fn breadcrumb_location_bar(
    pane_id: fika_core::PaneId,
    breadcrumbs: Vec<BreadcrumbSegment>,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let segment_count = breadcrumbs.len();
    div()
        .id(format!("location-bar-{}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .flex_1()
        .h(px(28.0))
        .px_1()
        .rounded_md()
        .bg(if focused {
            rgb(0xf8fbff)
        } else {
            rgb(0xffffff)
        })
        .overflow_hidden()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.start_location_edit(pane_id);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .children(breadcrumbs.into_iter().enumerate().map(|(index, segment)| {
            breadcrumb_segment(pane_id, index, index + 1 < segment_count, segment, cx)
        }))
}

fn breadcrumb_segment(
    pane_id: fika_core::PaneId,
    index: usize,
    show_separator: bool,
    segment: BreadcrumbSegment,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let path = segment.path.clone();
    div()
        .id(format!("location-segment-{}-{index}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .id(format!("location-segment-button-{}-{index}", pane_id.0))
                .px_2()
                .py_1()
                .rounded_md()
                .text_sm()
                .text_color(rgb(0x1f2937))
                .hover(|button| button.bg(rgb(0xe8eef7)))
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.standard_click() {
                            this.open_location_segment(pane_id, path.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                )
                .child(segment.label),
        )
        .when(show_separator, |row| {
            row.child(div().text_sm().text_color(rgb(0x94a3b8)).child(">"))
        })
}
