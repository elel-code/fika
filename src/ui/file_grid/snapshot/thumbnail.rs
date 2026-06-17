use std::path::{Path, PathBuf};
use std::sync::Arc;

use fika_core::{
    DirectoryModel, FilteredModel, ItemId, ThumbnailCandidate, ThumbnailRequestPriority,
    is_network_path, mime_magic_resolution_required, thumbnail_read_ahead_indexes,
    thumbnail_request_may_have_preview,
};

use super::super::layout::model_index_for_layout_index;
use super::RawFileGridSnapshot;

pub(crate) fn visible_item_thumbnail_path(entry: &fika_core::ModelEntry) -> Option<PathBuf> {
    if entry.is_dir {
        None
    } else {
        entry.thumbnail_path.clone()
    }
}

pub(crate) fn deferred_thumbnail_candidates_for_model<'a>(
    raw_file_grid: &RawFileGridSnapshot,
    model: &'a DirectoryModel,
    filtered: Option<&'a FilteredModel>,
    item_count: usize,
) -> impl Iterator<Item = ThumbnailCandidate> + 'a {
    raw_file_grid
        .visible_layout_range_and_count()
        .into_iter()
        .flat_map(move |(visible_range, visible_count)| {
            thumbnail_read_ahead_indexes(visible_range, item_count, visible_count)
        })
        .filter_map(move |layout_index| {
            let model_index = model_index_for_layout_index(filtered, layout_index)?;
            let entry = model.get(model_index)?;
            let path = model.path_for_index(model_index)?;
            if entry.is_dir
                || is_network_path(&path)
                || !entry.effective_metadata_complete()
                || entry.metadata_refresh_pending
                || visible_item_thumbnail_path(entry).is_some()
                || entry.thumbnail_failed
                || !thumbnail_request_may_have_preview(
                    &path,
                    entry.effective_mime_type().map(Arc::as_ref),
                )
            {
                return None;
            }
            if mime_magic_resolution_required(
                entry.is_dir,
                entry.effective_size_bytes(),
                entry.effective_mime_type().map(Arc::as_ref),
                entry.effective_mime_magic_checked(),
            ) {
                return None;
            }
            Some(ThumbnailCandidate {
                item_id: entry.id,
                path,
                modified_secs: entry.effective_modified_secs()?,
                metadata_complete: entry.effective_metadata_complete(),
                mime_type: entry
                    .effective_mime_type()
                    .map(|mime| mime.as_ref().to_string()),
                priority: ThumbnailRequestPriority::Deferred,
            })
        })
}

pub(super) fn visible_thumbnail_candidate(
    item_id: ItemId,
    path: &Path,
    is_dir: bool,
    thumbnail_path: Option<&PathBuf>,
    thumbnail_failed: bool,
    modified_secs: Option<u64>,
    size_bytes: u64,
    metadata_complete: bool,
    metadata_refresh_pending: bool,
    mime_type: Option<&Arc<str>>,
    mime_magic_checked: bool,
) -> Option<ThumbnailCandidate> {
    if is_dir
        || is_network_path(path)
        || !metadata_complete
        || metadata_refresh_pending
        || thumbnail_path.is_some()
        || thumbnail_failed
        || !thumbnail_request_may_have_preview(path, mime_type.map(Arc::as_ref))
        || mime_magic_resolution_required(
            is_dir,
            size_bytes,
            mime_type.map(Arc::as_ref),
            mime_magic_checked,
        )
    {
        return None;
    }
    Some(ThumbnailCandidate {
        item_id,
        path: path.to_path_buf(),
        modified_secs: modified_secs?,
        metadata_complete,
        mime_type: mime_type.map(|mime| mime.as_ref().to_string()),
        priority: ThumbnailRequestPriority::Visible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use fika_core::{IconsLayout, ItemLayout};
    use std::sync::Arc;

    use super::super::RawVisibleItemSnapshot;

    #[test]
    fn visible_item_thumbnail_path_uses_file_cache_hit_only() {
        let thumbnail = PathBuf::from("/tmp/fika-thumbnail-cache/normal/hash.png");
        let file = fika_core::ModelEntry {
            id: fika_core::ItemId(1),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: Some(thumbnail.clone()),
            thumbnail_failed: false,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("photo.jpg"),
                name_width_units: 9,
                target_path: None,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from("image/jpeg")),
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            }),
        };
        let dir = fika_core::ModelEntry {
            id: fika_core::ItemId(2),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: Some(thumbnail.clone()),
            thumbnail_failed: false,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("Pictures"),
                name_width_units: 8,
                target_path: None,
                size_bytes: 0,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: None,
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: true,
            }),
        };

        assert_eq!(visible_item_thumbnail_path(&file), Some(thumbnail));
        assert_eq!(visible_item_thumbnail_path(&dir), None);
    }

    #[test]
    fn deferred_thumbnail_candidates_stream_from_model_read_ahead() {
        let directory = PathBuf::from("/tmp/fika-deferred-thumbnail-candidates");
        let entries = Arc::new(vec![
            test_entry("a-visible.jpg", Some("image/jpeg"), true, Some(10)),
            test_entry("b-candidate.png", Some("image/png"), true, Some(20)),
            test_entry(
                "c-needs-magic.bin",
                Some("application/octet-stream"),
                false,
                Some(30),
            ),
            test_entry("d-no-mtime.jpg", Some("image/jpeg"), true, None),
        ]);
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory.clone(), entries);
        let visible_entry = model.get(0).unwrap();
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(4, fika_core::IconsLayoutOptions::default()),
            items: vec![RawVisibleItemSnapshot {
                slot_id: 0,
                visible: true,
                layout: test_layout(0),
                item_id: visible_entry.id,
                path: model.path_for_index(0).unwrap(),
                is_dir: visible_entry.is_dir,
                name: visible_entry.name.clone(),
                thumbnail_path: None,
                thumbnail_failed: false,
                modified_secs: visible_entry.effective_modified_secs(),
                size_bytes: visible_entry.effective_size_bytes(),
                metadata_complete: visible_entry.effective_metadata_complete(),
                metadata_refresh_pending: visible_entry.metadata_refresh_pending,
                mime_type: visible_entry.effective_mime_type_cloned(),
                mime_magic_checked: visible_entry.effective_mime_magic_checked(),
                selected: false,
                drop_target: false,
                draft_name: None,
                draft_caret: None,
                draft_selection: None,
                draft_error: None,
                draft_warning: None,
            }],
        };

        let candidates =
            deferred_thumbnail_candidates_for_model(&raw_file_grid, &model, None, model.len())
                .collect::<Vec<_>>();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, directory.join("b-candidate.png"));
        assert_eq!(candidates[0].modified_secs, 20);
        assert_eq!(candidates[0].priority, ThumbnailRequestPriority::Deferred);
    }

    #[test]
    fn thumbnail_candidates_skip_plain_text_without_preview_support() {
        let mime_type = Arc::from("text/plain");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("/tmp/notes.txt"),
                false,
                None,
                false,
                Some(42),
                12,
                true,
                false,
                Some(&mime_type),
                true,
            ),
            None
        );
    }

    #[test]
    fn thumbnail_candidates_skip_network_paths() {
        let mime_type = Arc::from("image/png");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("smb://server/share/photo.png"),
                false,
                None,
                false,
                Some(42),
                12,
                true,
                false,
                Some(&mime_type),
                true,
            ),
            None
        );
    }

    #[test]
    fn thumbnail_candidates_include_images() {
        let mime_type = Arc::from("image/png");

        let candidate = visible_thumbnail_candidate(
            ItemId(1),
            Path::new("/tmp/photo.png"),
            false,
            None,
            false,
            Some(42),
            12,
            true,
            false,
            Some(&mime_type),
            true,
        )
        .unwrap();

        assert_eq!(candidate.path, PathBuf::from("/tmp/photo.png"));
        assert_eq!(candidate.mime_type.as_deref(), Some("image/png"));
        assert_eq!(candidate.priority, ThumbnailRequestPriority::Visible);
    }

    #[test]
    fn thumbnail_candidates_skip_failed_preview_role() {
        let mime_type = Arc::from("image/png");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("/tmp/photo.png"),
                false,
                None,
                true,
                Some(42),
                12,
                true,
                false,
                Some(&mime_type),
                true,
            ),
            None
        );
    }

    fn test_entry(
        name: &str,
        mime_type: Option<&str>,
        mime_magic_checked: bool,
        modified_secs: Option<u64>,
    ) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.chars().count() as u16,
            target_path: None,
            size_bytes: 12,
            modified_secs,
            metadata_complete: true,
            mime_type: mime_type.map(Arc::from),
            mime_magic_checked,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn test_layout(model_index: usize) -> ItemLayout {
        let rect = fika_core::ViewRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        ItemLayout {
            model_index,
            column: 0,
            row: model_index,
            item_rect: rect,
            visual_rect: rect,
            icon_rect: rect,
            text_rect: rect,
        }
    }
}
