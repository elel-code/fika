use std::path::{Path, PathBuf};

use fika_core::{FileTransferMode, PaneController, PaneId};

pub(crate) fn file_transfer_mode_for_modifiers(modifiers: gpui::Modifiers) -> FileTransferMode {
    if modifiers.alt || (modifiers.shift && modifiers.secondary()) {
        FileTransferMode::Link
    } else if modifiers.shift {
        FileTransferMode::Move
    } else {
        FileTransferMode::Copy
    }
}

pub(crate) fn drag_cursor_style_for_transfer_mode(mode: FileTransferMode) -> gpui::CursorStyle {
    match mode {
        FileTransferMode::Copy => gpui::CursorStyle::DragCopy,
        FileTransferMode::Move => gpui::CursorStyle::Arrow,
        FileTransferMode::Link => gpui::CursorStyle::DragLink,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemDragPayload {
    pub(crate) source_pane: PaneId,
    pub(crate) source_path: PathBuf,
    pub(crate) source_selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActiveItemDrag {
    pub(crate) payload: ItemDragPayload,
    pub(crate) paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ItemDropTarget {
    Pane {
        pane_id: PaneId,
        mode: FileTransferMode,
    },
    Directory {
        pane_id: PaneId,
        path: PathBuf,
        mode: FileTransferMode,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaceDropTarget {
    Place {
        path: PathBuf,
        mode: FileTransferMode,
    },
    Insert {
        index: usize,
    },
}

pub(crate) fn item_drag_paths(
    controller: &PaneController,
    payload: &ItemDragPayload,
) -> Vec<PathBuf> {
    if payload.source_selected && controller.is_selected(payload.source_pane, &payload.source_path)
    {
        let selected_paths = controller
            .selected_paths(payload.source_pane)
            .unwrap_or_default();
        if !selected_paths.is_empty() {
            return selected_paths;
        }
    }
    vec![payload.source_path.clone()]
}

pub(crate) fn item_drop_reject_reason(paths: &[PathBuf], target_dir: &Path) -> Option<String> {
    if paths.is_empty() {
        return Some("No dragged items".to_string());
    }
    if !target_dir.is_dir() {
        return Some(format!("Cannot drop into {}", target_dir.display()));
    }
    if paths.iter().any(|path| same_drop_url(path, target_dir)) {
        return Some("Cannot drop an item onto itself".to_string());
    }
    None
}

pub(crate) fn item_drop_target_mode_for_pane(
    target: Option<&ItemDropTarget>,
    pane_id: PaneId,
) -> Option<FileTransferMode> {
    match target {
        Some(ItemDropTarget::Pane {
            pane_id: target_pane,
            mode,
        }) if *target_pane == pane_id => Some(*mode),
        _ => None,
    }
}

pub(crate) fn item_drop_target_mode_for_directory(
    target: Option<&ItemDropTarget>,
    pane_id: PaneId,
    path: &Path,
) -> Option<FileTransferMode> {
    match target {
        Some(ItemDropTarget::Directory {
            pane_id: target_pane,
            path: target_path,
            mode,
        }) if *target_pane == pane_id && target_path == path => Some(*mode),
        _ => None,
    }
}

pub(crate) fn place_drop_target_mode_for_place(
    target: Option<&PlaceDropTarget>,
    path: &Path,
) -> Option<FileTransferMode> {
    match target {
        Some(PlaceDropTarget::Place {
            path: target_path,
            mode,
        }) if target_path == path => Some(*mode),
        _ => None,
    }
}

pub(crate) fn place_drop_target_matches_insert(
    target: Option<&PlaceDropTarget>,
    index: usize,
) -> bool {
    matches!(target, Some(PlaceDropTarget::Insert { index: target_index }) if *target_index == index)
}

fn same_drop_url(path: &Path, target_dir: &Path) -> bool {
    if path == target_dir {
        return true;
    }
    match (path.canonicalize(), target_dir.canonicalize()) {
        (Ok(path), Ok(target_dir)) => path == target_dir,
        _ => false,
    }
}
