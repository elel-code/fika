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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DropTargetState {
    item: Option<ItemDropTarget>,
    place: Option<PlaceDropTarget>,
    stale_generation: u64,
}

impl DropTargetState {
    pub(crate) fn item(&self) -> Option<&ItemDropTarget> {
        self.item.as_ref()
    }

    pub(crate) fn place(&self) -> Option<&PlaceDropTarget> {
        self.place.as_ref()
    }

    pub(crate) fn stale_generation(&self) -> u64 {
        self.stale_generation
    }

    pub(crate) fn has_target(&self) -> bool {
        self.item.is_some() || self.place.is_some()
    }

    pub(crate) fn clear_without_touch(&mut self) {
        self.item = None;
        self.place = None;
    }

    pub(crate) fn set_item(&mut self, target: ItemDropTarget) -> bool {
        let target = Some(target);
        if self.item == target && self.place.is_none() {
            self.touch_stale_generation();
            return false;
        }
        self.item = target;
        self.place = None;
        self.touch_stale_generation();
        true
    }

    pub(crate) fn set_place(&mut self, target: PlaceDropTarget) -> bool {
        let target = Some(target);
        if self.place == target && self.item.is_none() {
            self.touch_stale_generation();
            return false;
        }
        self.place = target;
        self.item = None;
        self.touch_stale_generation();
        true
    }

    pub(crate) fn clear_item(&mut self) -> bool {
        let had_target = self.item.is_some();
        self.item = None;
        if had_target {
            self.touch_stale_generation();
        }
        had_target
    }

    pub(crate) fn clear_item_for_pane(&mut self, pane_id: PaneId) -> bool {
        if matches!(
            self.item,
            Some(ItemDropTarget::Pane {
                pane_id: target_pane,
                ..
            }) if target_pane == pane_id
        ) {
            return self.clear_item();
        }
        false
    }

    pub(crate) fn clear_item_for_directory(&mut self, pane_id: PaneId, path: &Path) -> bool {
        if self.item.as_ref().is_some_and(|target| {
            matches!(
                target,
                ItemDropTarget::Directory {
                    pane_id: target_pane,
                    path: target_path,
                    ..
                } if *target_pane == pane_id && target_path == path
            )
        }) {
            return self.clear_item();
        }
        false
    }

    pub(crate) fn clear_place(&mut self) -> bool {
        let had_target = self.place.is_some();
        self.place = None;
        if had_target {
            self.touch_stale_generation();
        }
        had_target
    }

    pub(crate) fn clear_place_for_insert(&mut self, index: usize) -> bool {
        if matches!(
            self.place,
            Some(PlaceDropTarget::Insert { index: target_index }) if target_index == index
        ) {
            return self.clear_place();
        }
        false
    }

    pub(crate) fn clear_place_for_row(
        &mut self,
        path: &Path,
        insert_before_index: usize,
        insert_after_index: usize,
    ) -> bool {
        let matches_row = self.place.as_ref().is_some_and(|target| match target {
            PlaceDropTarget::Place {
                path: target_path, ..
            } => target_path == path,
            PlaceDropTarget::Insert { index } => {
                *index == insert_before_index || *index == insert_after_index
            }
        });
        if matches_row {
            return self.clear_place();
        }
        false
    }

    pub(crate) fn clear_all(&mut self) -> bool {
        let had_target = self.has_target();
        self.item = None;
        self.place = None;
        if had_target {
            self.touch_stale_generation();
        }
        had_target
    }

    pub(crate) fn clear_stale_for_generation(&mut self, generation: u64) -> bool {
        if self.stale_generation != generation {
            return false;
        }
        self.clear_all()
    }

    fn touch_stale_generation(&mut self) {
        self.stale_generation = self.stale_generation.wrapping_add(1);
    }
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
    use super::{
        DropTargetState, ItemDropTarget, PlaceDropTarget, drag_export_payload_for_paths,
        item_drop_target_mode_for_directory, item_drop_target_mode_for_pane,
        place_drag_export_payload, place_drop_target_matches_insert,
        place_drop_target_mode_for_place,
    };
    use fika_core::{FileTransferMode, PaneId};
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

    #[test]
    fn drop_target_state_replaces_item_and_place_targets() {
        let pane = PaneId(1);
        let path = PathBuf::from("/tmp/fika-drop-target-state");
        let mut state = DropTargetState::default();

        assert!(state.set_item(ItemDropTarget::Pane {
            pane_id: pane,
            mode: FileTransferMode::Copy,
        }));
        let first_generation = state.stale_generation();
        assert_eq!(
            item_drop_target_mode_for_pane(state.item(), pane),
            Some(FileTransferMode::Copy)
        );

        assert!(!state.set_item(ItemDropTarget::Pane {
            pane_id: pane,
            mode: FileTransferMode::Copy,
        }));
        assert!(state.stale_generation() > first_generation);
        let refreshed_generation = state.stale_generation();

        assert!(state.set_place(PlaceDropTarget::Place {
            path: path.clone(),
            mode: FileTransferMode::Move,
        }));
        assert!(state.item().is_none());
        assert_eq!(
            place_drop_target_mode_for_place(state.place(), &path),
            Some(FileTransferMode::Move)
        );
        assert!(state.stale_generation() > refreshed_generation);
    }

    #[test]
    fn drop_target_state_clears_only_matching_item_target() {
        let pane = PaneId(1);
        let other_pane = PaneId(2);
        let path = PathBuf::from("/tmp/fika-drop-target-state/target");
        let other_path = PathBuf::from("/tmp/fika-drop-target-state/other");
        let mut state = DropTargetState::default();

        assert!(state.set_item(ItemDropTarget::Directory {
            pane_id: pane,
            path: path.clone(),
            mode: FileTransferMode::Link,
        }));
        let generation = state.stale_generation();

        assert!(!state.clear_item_for_directory(pane, &other_path));
        assert!(!state.clear_item_for_directory(other_pane, &path));
        assert_eq!(
            item_drop_target_mode_for_directory(state.item(), pane, &path),
            Some(FileTransferMode::Link)
        );
        assert_eq!(state.stale_generation(), generation);

        assert!(state.clear_item_for_directory(pane, &path));
        assert!(state.item().is_none());
        assert!(state.stale_generation() > generation);
    }

    #[test]
    fn drop_target_state_clears_only_matching_place_target() {
        let row_path = PathBuf::from("/tmp/fika-drop-target-state/place");
        let other_path = PathBuf::from("/tmp/fika-drop-target-state/other-place");
        let mut state = DropTargetState::default();

        assert!(state.set_place(PlaceDropTarget::Insert { index: 3 }));
        let insert_generation = state.stale_generation();
        assert!(!state.clear_place_for_insert(2));
        assert!(place_drop_target_matches_insert(state.place(), 3));
        assert_eq!(state.stale_generation(), insert_generation);
        assert!(state.clear_place_for_insert(3));
        assert!(state.place().is_none());

        assert!(state.set_place(PlaceDropTarget::Place {
            path: row_path.clone(),
            mode: FileTransferMode::Copy,
        }));
        let row_generation = state.stale_generation();
        assert!(!state.clear_place_for_row(&other_path, 1, 2));
        assert_eq!(
            place_drop_target_mode_for_place(state.place(), &row_path),
            Some(FileTransferMode::Copy)
        );
        assert_eq!(state.stale_generation(), row_generation);
        assert!(state.clear_place_for_row(&row_path, 1, 2));
        assert!(state.place().is_none());
    }

    #[test]
    fn drop_target_state_stale_generation_only_clears_current_target() {
        let pane = PaneId(1);
        let path = PathBuf::from("/tmp/fika-drop-target-state/place");
        let mut state = DropTargetState::default();

        assert!(state.set_item(ItemDropTarget::Pane {
            pane_id: pane,
            mode: FileTransferMode::Copy,
        }));
        let stale_generation = state.stale_generation();

        assert!(state.set_place(PlaceDropTarget::Place {
            path: path.clone(),
            mode: FileTransferMode::Copy,
        }));
        assert!(!state.clear_stale_for_generation(stale_generation));
        assert_eq!(
            place_drop_target_mode_for_place(state.place(), &path),
            Some(FileTransferMode::Copy)
        );

        let current_generation = state.stale_generation();
        assert!(state.clear_stale_for_generation(current_generation));
        assert!(!state.has_target());
        assert!(state.stale_generation() > current_generation);
    }
}
