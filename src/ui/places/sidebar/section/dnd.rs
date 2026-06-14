use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::{
    FileTransferMode, refresh_active_drag_cursor_for_transfer_mode,
    refresh_active_drag_cursor_not_allowed,
};
use crate::ui::file_grid::ItemDrag;

use super::super::super::drag::PlaceDrag;

pub(super) fn install_section_dnd(
    heading: Stateful<Div>,
    insert_index: usize,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    heading
        .on_drag_move::<ItemDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                if !contains {
                    return;
                }
                let changed = this.set_place_drag_drop_target_for_insert(insert_index);
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
                this.schedule_drop_target_stale_clear(cx);
                if changed {
                    cx.notify();
                }
                cx.stop_propagation();
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                if !contains {
                    return;
                }
                let changed = this.set_place_drag_drop_target_for_insert(insert_index);
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
                this.schedule_drop_target_stale_clear(cx);
                if changed {
                    cx.notify();
                }
                cx.stop_propagation();
            },
        ))
        .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, _window, cx| {
            this.drop_item_drag_to_place_insert(drag.payload(), insert_index);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_drop::<ExternalPaths>(cx.listener(
            move |this, external_paths: &ExternalPaths, _window, cx| {
                this.drop_external_paths_to_place_insert(
                    external_paths.paths().to_vec(),
                    insert_index,
                );
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                if !contains {
                    return;
                }
                let drag = event.drag(cx);
                let changed = if drag.movable() {
                    this.set_place_drag_drop_target_for_insert(insert_index)
                } else {
                    false
                };
                if drag.movable() {
                    refresh_active_drag_cursor_for_transfer_mode(
                        FileTransferMode::Move,
                        window,
                        cx,
                    );
                    this.schedule_drop_target_stale_clear(cx);
                } else if contains {
                    let cleared = this.clear_drag_drop_targets();
                    refresh_active_drag_cursor_not_allowed(window, cx);
                    if cleared {
                        cx.notify();
                    }
                }
                if changed {
                    cx.notify();
                }
                cx.stop_propagation();
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
            this.drop_place_drag_to_place_insert(drag.source_index(), insert_index);
            cx.stop_propagation();
            cx.notify();
        }))
}
