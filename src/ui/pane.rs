use crate::{ClipboardMode, FikaApp, PaneSnapshot};
use fika_core::CreatedItemKind;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Styled, div, px, rgb};

use super::file_grid::{FileGridMode, FileGridProps, file_grid};

pub(crate) struct PaneProps {
    pub snapshot: PaneSnapshot,
    pub manager_mode: bool,
    pub file_grid_mode: FileGridMode,
}

pub(crate) fn pane_view(props: PaneProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let PaneProps {
        snapshot,
        manager_mode,
        file_grid_mode,
    } = props;
    let PaneSnapshot {
        id: pane_id,
        path,
        item_count,
        visible_items,
        view,
        rubber_band,
        focused,
        can_close,
        can_go_back,
        can_go_forward,
        can_paste,
        can_rename,
        can_undo,
        operation_pending,
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
                    toolbar_button("back", "Back")
                        .when(!can_go_back, |button| button.opacity(0.35))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.go_back(pane_id);
                            cx.notify();
                        })),
                )
                .child(
                    toolbar_button("forward", "Forward")
                        .when(!can_go_forward, |button| button.opacity(0.35))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.go_forward(pane_id);
                            cx.notify();
                        })),
                )
                .child(toolbar_button("up", "Up").on_click(cx.listener(
                    move |this, _event, _window, cx| {
                        this.go_parent(pane_id);
                        cx.notify();
                    },
                )))
                .child(toolbar_button("refresh", "Refresh").on_click(cx.listener(
                    move |this, _event, _window, cx| {
                        this.reload_pane(pane_id);
                        cx.notify();
                    },
                )))
                .child(toolbar_button("split", "Split").on_click(cx.listener(
                    move |this, _event, _window, cx| {
                        this.split_pane(pane_id);
                        cx.notify();
                    },
                )))
                .child(
                    toolbar_button("close", "Close")
                        .when(!can_close, |button| button.opacity(0.35))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.close_pane(pane_id);
                            cx.notify();
                        })),
                )
                .child(toolbar_button("all", "All").on_click(cx.listener(
                    move |this, _event, _window, cx| {
                        this.select_all(pane_id);
                        cx.notify();
                    },
                )))
                .child(toolbar_button("clear", "Clear").on_click(cx.listener(
                    move |this, _event, _window, cx| {
                        this.clear_selection(pane_id);
                        cx.notify();
                    },
                )))
                .when(manager_mode, |toolbar| {
                    toolbar
                        .child(
                            toolbar_button("new-folder", "New Folder")
                                .when(operation_pending, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.create_item_in_pane(pane_id, CreatedItemKind::Folder, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            toolbar_button("new-file", "New File")
                                .when(operation_pending, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.create_item_in_pane(pane_id, CreatedItemKind::File, cx);
                                    cx.notify();
                                })),
                        )
                        .child(toolbar_button("copy", "Copy").on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.store_selection_for_transfer(pane_id, ClipboardMode::Copy);
                                cx.notify();
                            },
                        )))
                        .child(toolbar_button("cut", "Cut").on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.store_selection_for_transfer(pane_id, ClipboardMode::Cut);
                                cx.notify();
                            },
                        )))
                        .child(
                            toolbar_button("paste", "Paste")
                                .when(!can_paste, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.paste_into_pane(pane_id, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            toolbar_button("rename", "Rename")
                                .when(!can_rename, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.start_rename_in_pane(pane_id);
                                    cx.notify();
                                })),
                        )
                        .child(
                            toolbar_button("trash", "Trash")
                                .when(operation_pending, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.trash_selection(pane_id, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            toolbar_button("undo", "Undo")
                                .when(!can_undo, |button| button.opacity(0.35))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.undo_latest(cx);
                                    cx.notify();
                                })),
                        )
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
                item_count,
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
