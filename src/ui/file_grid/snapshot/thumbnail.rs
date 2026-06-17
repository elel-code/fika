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
