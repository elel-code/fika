use std::path::{Path, PathBuf};

use fika_core::{
    FileClipboardRole, FileTransferMode, PaneController, PaneId, encode_file_clipboard_text,
};

pub(crate) const TEXT_URI_LIST_MIME: &str = "text/uri-list";
pub(crate) const TEXT_PLAIN_MIME: &str = "text/plain";

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
    pub(crate) export: Option<DragExportPayload>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DragExportPayload {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) uri_list_mime: &'static str,
    pub(crate) uri_list: String,
    pub(crate) plain_text_mime: &'static str,
    pub(crate) plain_text: String,
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

pub(crate) fn item_drag_export_payload(
    controller: &PaneController,
    payload: &ItemDragPayload,
) -> Option<DragExportPayload> {
    drag_export_payload_for_paths(item_drag_paths(controller, payload))
}

pub(crate) fn place_drag_export_payload(path: &Path) -> Option<DragExportPayload> {
    path.is_dir()
        .then(|| drag_export_payload_for_paths(vec![path.to_path_buf()]))
        .flatten()
}

pub(crate) fn drag_export_payload_for_paths(paths: Vec<PathBuf>) -> Option<DragExportPayload> {
    let paths = drag_export_paths(paths);
    if paths.is_empty() {
        return None;
    }
    let uri_list = encode_file_clipboard_text(FileClipboardRole::Copy, &paths);
    let plain_text = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Some(DragExportPayload {
        paths,
        uri_list_mime: TEXT_URI_LIST_MIME,
        uri_list,
        plain_text_mime: TEXT_PLAIN_MIME,
        plain_text,
    })
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

fn drag_export_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut exported = Vec::<PathBuf>::new();
    for path in paths {
        if exported.iter().any(|existing| path == *existing) {
            continue;
        }
        if exported
            .iter()
            .any(|existing| path_is_child_of(&path, existing))
        {
            continue;
        }
        exported.retain(|existing| !path_is_child_of(existing, &path));
        exported.push(path);
    }
    exported
}

fn path_is_child_of(path: &Path, parent: &Path) -> bool {
    path != parent && path.starts_with(parent)
}

#[cfg(test)]
mod tests {
    use super::{drag_export_payload_for_paths, place_drag_export_payload};
    use std::path::PathBuf;

    #[test]
    fn drag_export_payload_encodes_uri_list_and_plain_text() {
        let payload = drag_export_payload_for_paths(vec![
            PathBuf::from("/tmp/a file.txt"),
            PathBuf::from("/tmp/unicode-文档.txt"),
        ])
        .unwrap();

        assert_eq!(payload.uri_list_mime, "text/uri-list");
        assert_eq!(payload.plain_text_mime, "text/plain");
        assert_eq!(
            payload.uri_list,
            "file:///tmp/a%20file.txt\nfile:///tmp/unicode-%E6%96%87%E6%A1%A3.txt"
        );
        assert_eq!(payload.plain_text, "/tmp/a file.txt\n/tmp/unicode-文档.txt");
    }

    #[test]
    fn drag_export_payload_prunes_children_when_parent_is_exported() {
        let payload = drag_export_payload_for_paths(vec![
            PathBuf::from("/tmp/parent/child.txt"),
            PathBuf::from("/tmp/parent"),
            PathBuf::from("/tmp/parent/other.txt"),
            PathBuf::from("/tmp/sibling"),
            PathBuf::from("/tmp/sibling"),
        ])
        .unwrap();

        assert_eq!(
            payload.paths,
            vec![PathBuf::from("/tmp/parent"), PathBuf::from("/tmp/sibling")]
        );
    }

    #[test]
    fn place_drag_export_payload_requires_existing_directory() {
        let root = std::env::temp_dir().join(format!(
            "fika-place-drag-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir = root.join("dir");
        let file = root.join("file.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "not a place directory").unwrap();

        assert!(place_drag_export_payload(&dir).is_some());
        assert_eq!(place_drag_export_payload(&file), None);
        assert_eq!(place_drag_export_payload(&root.join("missing")), None);

        let _ = std::fs::remove_dir_all(root);
    }
}
