use fika_core::{
    Generation, MetadataRoleScheduler, PaneId, ThumbnailCandidate, ThumbnailScheduler,
};

use super::{RawFileGridSnapshot, RawVisibleItemSnapshot, thumbnail};

impl RawFileGridSnapshot {
    pub(crate) fn queue_metadata_role_candidates(
        &self,
        scheduler: &mut MetadataRoleScheduler,
        pane_id: PaneId,
        generation: Generation,
    ) -> bool {
        scheduler.queue_candidates(pane_id, generation, self.visible_metadata_role_candidates())
    }

    pub(crate) fn queue_thumbnail_candidates(
        &self,
        scheduler: &mut ThumbnailScheduler,
        pane_id: PaneId,
        generation: Generation,
        deferred_candidates: impl IntoIterator<Item = ThumbnailCandidate>,
    ) -> bool {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => scheduler.queue_candidates(
                pane_id,
                generation,
                items
                    .iter()
                    .filter_map(raw_visible_thumbnail_candidate)
                    .chain(deferred_candidates),
            ),
            Self::Details { .. } => {
                scheduler.queue_candidates(pane_id, generation, deferred_candidates)
            }
        }
    }
}

fn raw_visible_thumbnail_candidate(item: &RawVisibleItemSnapshot) -> Option<ThumbnailCandidate> {
    if !item.visible {
        return None;
    }
    thumbnail::visible_thumbnail_candidate(
        item.item_id,
        &item.path,
        item.is_dir,
        item.thumbnail_path.as_ref(),
        item.thumbnail_failed,
        item.modified_secs,
        item.size_bytes,
        item.metadata_complete,
        item.metadata_refresh_pending,
        item.mime_type.as_ref(),
        item.mime_magic_checked,
    )
}
