use std::path::PathBuf;
use std::sync::Arc;

use fika_core::{ItemId, MetadataRoleCandidate, mime_magic_resolution_required};

use super::RawFileGridSnapshot;

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

pub(super) fn visible_metadata_role_candidates(
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
