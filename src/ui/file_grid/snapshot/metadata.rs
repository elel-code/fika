use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fika_core::{
    Generation, ItemId, MetadataRoleCandidate, MetadataRoleRequest, MetadataRoleResult, PaneId,
    metadata_role_result_for_request, mime_magic_resolution_required,
};

use super::RawFileGridSnapshot;

impl RawFileGridSnapshot {
    pub(crate) fn visible_metadata_role_candidates(&self) -> Vec<MetadataRoleCandidate> {
        visible_metadata_role_candidates(self)
    }
}

pub(crate) fn visible_metadata_role_results_for_raw_grid(
    pane_id: PaneId,
    generation: Generation,
    raw_file_grid: &RawFileGridSnapshot,
    budget: Duration,
) -> Vec<MetadataRoleResult> {
    let started = Instant::now();
    let mut results = Vec::new();
    for candidate in raw_file_grid.visible_metadata_role_candidates() {
        if started.elapsed() >= budget {
            break;
        }
        let Some(request) = MetadataRoleRequest::from_candidate(pane_id, generation, candidate)
        else {
            continue;
        };
        results.push(metadata_role_result_for_request(request));
    }
    results
}

trait MetadataRoleCandidateSource {
    fn item_id(&self) -> ItemId;
    fn path(&self) -> &PathBuf;
    fn is_dir(&self) -> bool;
    fn size_bytes(&self) -> u64;
    fn modified_secs(&self) -> Option<u64>;
    fn mime_type(&self) -> Option<&Arc<str>>;
    fn mime_magic_checked(&self) -> bool;
}

impl MetadataRoleCandidateSource for super::RawVisibleItemSnapshot {
    fn item_id(&self) -> ItemId {
        self.item_id
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    fn modified_secs(&self) -> Option<u64> {
        self.modified_secs
    }

    fn mime_type(&self) -> Option<&Arc<str>> {
        self.mime_type.as_ref()
    }

    fn mime_magic_checked(&self) -> bool {
        self.mime_magic_checked
    }
}

impl MetadataRoleCandidateSource for super::RawDetailsItemSnapshot {
    fn item_id(&self) -> ItemId {
        self.item_id
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    fn modified_secs(&self) -> Option<u64> {
        self.modified_secs
    }

    fn mime_type(&self) -> Option<&Arc<str>> {
        self.mime_type.as_ref()
    }

    fn mime_magic_checked(&self) -> bool {
        self.mime_magic_checked
    }
}

fn visible_metadata_role_candidates(
    raw_file_grid: &RawFileGridSnapshot,
) -> Vec<MetadataRoleCandidate> {
    match raw_file_grid {
        RawFileGridSnapshot::Compact { items, .. } | RawFileGridSnapshot::Icons { items, .. } => {
            metadata_role_candidates_for_items(items.iter().filter(|item| item.visible))
        }
        RawFileGridSnapshot::Details { items, .. } => metadata_role_candidates_for_items(items),
    }
}

fn metadata_role_candidates_for_items<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
) -> Vec<MetadataRoleCandidate>
where
    T: MetadataRoleCandidateSource + 'a,
{
    items
        .into_iter()
        .filter(|item| {
            metadata_role_update_needed(
                item.is_dir(),
                item.size_bytes(),
                item.mime_type().map(Arc::as_ref),
                item.mime_magic_checked(),
            )
        })
        .map(|item| MetadataRoleCandidate {
            item_id: item.item_id(),
            path: item.path().clone(),
            size_bytes: item.size_bytes(),
            modified_secs: item.modified_secs(),
            mime_type: item.mime_type().map(|mime| mime.to_string()),
            mime_magic_checked: item.mime_magic_checked(),
        })
        .collect()
}

fn metadata_role_update_needed(
    is_dir: bool,
    size_bytes: u64,
    mime_type: Option<&str>,
    mime_magic_checked: bool,
) -> bool {
    mime_magic_resolution_required(is_dir, size_bytes, mime_type, mime_magic_checked)
}

#[cfg(test)]
mod tests {
    use super::*;

    use fika_core::{IconsLayout, ItemLayout, ViewRect};

    #[test]
    fn visible_metadata_role_results_respects_zero_budget() {
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, "payload", 0)],
        };

        let results = visible_metadata_role_results_for_raw_grid(
            PaneId(1),
            Generation(1),
            &raw_file_grid,
            Duration::ZERO,
        );

        assert!(results.is_empty());
    }

    #[test]
    fn visible_metadata_role_results_convert_visible_candidates() {
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(2, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(7, "visible.bin", 0), {
                let mut item = test_raw_visible_item(8, "read-ahead.bin", 1);
                item.visible = false;
                item
            }],
        };

        let results = visible_metadata_role_results_for_raw_grid(
            PaneId(3),
            Generation(4),
            &raw_file_grid,
            Duration::from_secs(1),
        );

        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result.pane_id, PaneId(3));
        assert_eq!(result.generation, Generation(4));
        assert_eq!(result.item_id, ItemId(7));
        assert_eq!(result.path, PathBuf::from("/tmp/visible.bin"));
        let role = result.role.as_ref().expect("metadata role should resolve");
        assert_eq!(role.size_bytes, 12);
        assert_eq!(role.modified_secs, Some(42));
        assert_eq!(
            role.mime_type.as_deref(),
            Some(fika_core::GENERIC_BINARY_MIME)
        );
        assert!(role.mime_magic_checked);
    }

    fn test_raw_visible_item(
        id: u64,
        name: &str,
        model_index: usize,
    ) -> super::super::RawVisibleItemSnapshot {
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        super::super::RawVisibleItemSnapshot {
            slot_id: 0,
            visible: true,
            layout: ItemLayout {
                model_index,
                column: 0,
                row: model_index,
                item_rect: rect,
                visual_rect: rect,
                icon_rect: rect,
                text_rect: rect,
            },
            item_id: ItemId(id),
            path: PathBuf::from("/tmp").join(name),
            is_dir: false,
            name: Arc::from(name),
            thumbnail_path: None,
            thumbnail_failed: false,
            modified_secs: Some(42),
            size_bytes: 12,
            metadata_complete: true,
            metadata_refresh_pending: false,
            mime_type: Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            mime_magic_checked: false,
            selected: false,
            drop_target: false,
            draft_name: None,
            draft_caret: None,
            draft_selection: None,
            draft_error: None,
            draft_warning: None,
        }
    }
}
