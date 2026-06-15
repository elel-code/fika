mod state;

pub(crate) use fika_core::FileTransferMode;
#[cfg(test)]
pub(crate) use state::place_drop_target_matches_insert;
pub(crate) use state::{
    ActiveItemDrag, DragExportPayload, DropTargetState, ItemDragPayload, ItemDropTarget,
    PathListDropTarget, PathListDropTargetKind, PathListDropTargetUpdate, PlaceDropTarget,
    drag_cursor_style_for_transfer_mode, item_drag_export_payload, item_drag_paths,
    item_drop_reject_reason, item_drop_target_matches_directory, item_drop_target_matches_pane,
    normalized_drag_paths, place_drag_export_payload, place_drop_target_matches_place,
};

use crate::FikaApp;
use gpui::{Context, Window};

pub(crate) fn refresh_active_drag_cursor_for_transfer_mode(
    mode: FileTransferMode,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let new_cursor = drag_cursor_style_for_transfer_mode(mode);
    if cx.active_drag_cursor_style() != Some(new_cursor) {
        cx.set_active_drag_cursor_style(new_cursor, window);
    }
}

pub(crate) fn refresh_active_drag_cursor_for_drop_menu(
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let new_cursor = gpui::CursorStyle::ContextualMenu;
    if cx.active_drag_cursor_style() != Some(new_cursor) {
        cx.set_active_drag_cursor_style(new_cursor, window);
    }
}

pub(crate) fn refresh_active_drag_cursor_not_allowed(
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let new_cursor = gpui::CursorStyle::OperationNotAllowed;
    if cx.active_drag_cursor_style() != Some(new_cursor) {
        cx.set_active_drag_cursor_style(new_cursor, window);
    }
}
