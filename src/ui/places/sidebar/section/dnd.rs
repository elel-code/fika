use std::path::PathBuf;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::{
    FileTransferMode, refresh_active_drag_cursor_for_transfer_mode,
    refresh_active_drag_cursor_not_allowed,
};
use crate::ui::file_grid::ItemDrag;

use super::super::super::drag::{PlaceDrag, place_drag_insert_index};

fn handle_section_path_list_drag_move(
    app: &mut FikaApp,
    source_paths: &[PathBuf],
    insert_index: usize,
    window: &mut gpui::Window,
    cx: &mut Context<FikaApp>,
) {
    let changed = if app.dragged_paths_can_add_place(source_paths) {
        let changed = app.set_place_drag_drop_target_for_insert(insert_index);
        refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
        app.refresh_drop_target_lease(cx);
        changed
    } else {
        let changed = app.clear_drag_drop_targets();
        refresh_active_drag_cursor_not_allowed(window, cx);
        changed
    };
    if changed {
        cx.notify();
    }
    cx.stop_propagation();
}

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
                let source_paths = this.item_drag_source_paths(&event.drag(cx).payload());
                handle_section_path_list_drag_move(this, &source_paths, insert_index, window, cx);
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                let contains = event.bounds.contains(&event.event.position);
                if !contains {
                    return;
                }
                let source_paths = this.external_drag_source_paths(event.drag(cx).paths());
                handle_section_path_list_drag_move(this, &source_paths, insert_index, window, cx);
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
                let target_index = drag
                    .movable()
                    .then(|| place_drag_insert_index(drag.source_index(), insert_index))
                    .flatten();
                let changed = match target_index {
                    Some(index) => this.set_place_drag_drop_target_for_insert(index),
                    None => this.clear_drag_drop_targets(),
                };
                if target_index.is_some() {
                    refresh_active_drag_cursor_for_transfer_mode(
                        FileTransferMode::Move,
                        window,
                        cx,
                    );
                    this.refresh_drop_target_lease(cx);
                } else {
                    refresh_active_drag_cursor_not_allowed(window, cx);
                }
                if changed {
                    cx.notify();
                }
                cx.stop_propagation();
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
            if drag.movable()
                && place_drag_insert_index(drag.source_index(), insert_index).is_some()
            {
                this.drop_place_drag_to_place_insert(drag.source_index(), insert_index);
            }
            cx.stop_propagation();
            cx.notify();
        }))
}
