use std::path::{Path, PathBuf};
use std::sync::Arc;

use fika_core::PaneId;
use gpui::{Context, ExternalPaths, Window};

use crate::FikaApp;
use crate::ui::drag_drop::{
    FileTransferMode, ItemDragPayload, PathListDropTargetKind, PathListDropTargetUpdate,
    refresh_active_drag_cursor_for_drop_menu, refresh_active_drag_cursor_for_transfer_mode,
    refresh_active_drag_cursor_not_allowed,
};
use crate::ui::icons::FileIconSnapshot;
use crate::ui::places::PlaceDrag;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemDrag {
    pub(super) pane_id: PaneId,
    pub(super) path: Arc<Path>,
    pub(super) name: Arc<str>,
    pub(super) icon: FileIconSnapshot,
    pub(super) selected: bool,
    pub(super) selection_count: usize,
}

impl ItemDrag {
    pub(crate) fn payload(&self) -> ItemDragPayload {
        ItemDragPayload {
            source_pane: self.pane_id,
            source_path: self.path.as_ref().to_path_buf(),
            source_selected: self.selected,
        }
    }
}

pub(super) fn handle_file_grid_item_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ItemDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let payload = event.drag(cx).payload();
    let source_paths = app.item_drag_source_paths(&payload);
    handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
}

pub(super) fn handle_file_grid_external_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ExternalPaths>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_paths = app.external_drag_source_paths(event.drag(cx).paths());
    handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
}

fn handle_file_grid_path_list_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    contains: bool,
    position: gpui::Point<gpui::Pixels>,
    source_paths: &[PathBuf],
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(pane_id, position, source_paths)
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    if contains {
        if update.accepted() {
            refresh_active_drag_cursor_for_drop_menu(window, cx);
            app.refresh_drop_target_lease(cx);
        } else {
            refresh_active_drag_cursor_not_allowed(window, cx);
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
}

pub(super) fn handle_file_grid_place_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<PlaceDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_path = event.drag(cx).path();
    let source_paths = std::slice::from_ref(&source_path);
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            event.event.position,
            source_paths,
        )
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    if contains {
        match update.kind {
            Some(PathListDropTargetKind::Directory) => {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
                app.refresh_drop_target_lease(cx);
            }
            Some(PathListDropTargetKind::Pane) => {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
                app.refresh_drop_target_lease(cx);
            }
            None => {
                refresh_active_drag_cursor_not_allowed(window, cx);
            }
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
}

pub(super) fn handle_file_grid_item_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &ItemDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    let payload = drag.payload();
    app.drop_item_drag_to_position_in_pane(pane_id, payload, position, cx);
    cx.stop_propagation();
    cx.notify();
}

pub(super) fn handle_file_grid_external_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    external_paths: &ExternalPaths,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_external_paths_to_position_in_pane(
        pane_id,
        external_paths.paths().to_vec(),
        position,
        cx,
    );
    cx.stop_propagation();
    cx.notify();
}

pub(super) fn handle_file_grid_place_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &PlaceDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_place_drag_to_position_in_pane(pane_id, drag.path(), position, cx);
    cx.stop_propagation();
    cx.notify();
}
