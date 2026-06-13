use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::drag_drop::FileTransferMode;
use crate::ui::icons::FileIconSnapshot;

pub(crate) fn format_entry_kind_label(entry: &fika_core::EntryData) -> String {
    if let Some(deletion_time) = &entry.trash_deletion_time {
        return fika_core::format_trash_deletion_time(deletion_time);
    }
    if entry.is_dir {
        "Folder".to_string()
    } else {
        fika_core::format_size(entry.size_bytes)
    }
}

pub(crate) fn visible_item_thumbnail_path(entry: &fika_core::EntryData) -> Option<PathBuf> {
    if entry.is_dir {
        None
    } else {
        entry.thumbnail_path.clone()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: fika_core::ItemLayout,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) kind_label: String,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_item_thumbnail_path_uses_file_cache_hit_only() {
        let thumbnail = PathBuf::from("/tmp/fika-thumbnail-cache/normal/hash.png");
        let file = fika_core::EntryData {
            name: Arc::from("photo.jpg"),
            name_width_units: 9,
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("image/jpeg")),
            thumbnail_path: Some(thumbnail.clone()),
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        };
        let dir = fika_core::EntryData {
            name: Arc::from("Pictures"),
            name_width_units: 8,
            size_bytes: 0,
            modified_secs: Some(42),
            mime_type: None,
            thumbnail_path: Some(thumbnail.clone()),
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: true,
        };

        assert_eq!(visible_item_thumbnail_path(&file), Some(thumbnail));
        assert_eq!(visible_item_thumbnail_path(&dir), None);
    }
}
