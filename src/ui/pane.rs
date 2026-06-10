use crate::{FikaApp, PaneSnapshot};
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

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
        path,
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
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .text_color(rgb(0x24292f))
                        .child(path.display().to_string()),
                ),
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
