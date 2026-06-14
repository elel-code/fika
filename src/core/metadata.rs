use super::entries::{EntryMetadataRole, ItemId};
use super::mime::MimeDatabase;
use super::model::DirectoryModel;
use super::pane::{Generation, PaneId};
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetadataRoleWorkKey {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path_hash: u64,
}

impl MetadataRoleWorkKey {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: &MetadataRoleCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path_hash: stable_hash(&candidate.path),
        }
    }

    pub fn from_request(request: &MetadataRoleRequest) -> Self {
        Self {
            pane_id: request.pane_id,
            generation: request.generation,
            item_id: request.item_id,
            path_hash: stable_hash(&request.path),
        }
    }

    pub fn from_result(result: &MetadataRoleResult) -> Self {
        Self {
            pane_id: result.pane_id,
            generation: result.generation,
            item_id: result.item_id,
            path_hash: stable_hash(&result.path),
        }
    }
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleCandidate {
    pub item_id: ItemId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
}

impl MetadataRoleRequest {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: MetadataRoleCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path: candidate.path,
        }
    }

    pub fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn item_id(&self) -> ItemId {
        self.item_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleResult {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path: PathBuf,
    pub role: Option<EntryMetadataRole>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleBatch {
    pub requests: Vec<MetadataRoleRequest>,
}

#[derive(Debug, Default)]
pub struct MetadataRoleScheduler {
    queued: VecDeque<MetadataRoleRequest>,
    seen: HashSet<MetadataRoleWorkKey>,
    active: HashSet<MetadataRoleWorkKey>,
    role_batch_pending: bool,
}

impl MetadataRoleScheduler {
    pub fn queue_candidates(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = MetadataRoleCandidate>,
    ) -> bool {
        let mut keep = HashSet::new();
        let mut pending = Vec::new();
        for candidate in candidates {
            let key = MetadataRoleWorkKey::from_candidate(pane_id, generation, &candidate);
            keep.insert(key.clone());
            let request = MetadataRoleRequest::from_candidate(pane_id, generation, candidate);
            pending.push((key, request));
        }
        self.prune_queued_for_snapshot(pane_id, generation, &keep);

        let mut queued = false;
        for (key, request) in pending {
            if self.seen.contains(&key) {
                continue;
            }
            self.seen.insert(key);
            self.queued.push_back(request);
            queued = true;
        }
        queued
    }

    pub fn start_role_batch(&mut self, batch_size: usize) -> Option<MetadataRoleBatch> {
        if self.role_batch_pending || self.queued.is_empty() {
            return None;
        }
        let mut requests = Vec::new();
        while requests.len() < batch_size {
            let Some(request) = self.queued.pop_front() else {
                break;
            };
            requests.push(request);
        }
        if requests.is_empty() {
            None
        } else {
            self.role_batch_pending = true;
            self.active = requests
                .iter()
                .map(MetadataRoleWorkKey::from_request)
                .collect();
            Some(MetadataRoleBatch { requests })
        }
    }

    pub fn finish_role_batch(&mut self) {
        self.role_batch_pending = false;
        for key in self.active.drain() {
            self.seen.remove(&key);
        }
    }

    pub fn finish_role_batch_with_results(&mut self, results: &[MetadataRoleResult]) {
        let failed = results
            .iter()
            .filter(|result| result.role.is_none())
            .map(MetadataRoleWorkKey::from_result)
            .collect::<HashSet<_>>();
        self.role_batch_pending = false;
        for key in self.active.drain() {
            if !failed.contains(&key) {
                self.seen.remove(&key);
            }
        }
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) {
        self.queued.retain(|request| request.pane_id != pane_id);
        self.seen.retain(|key| key.pane_id != pane_id);
        self.active.retain(|key| key.pane_id != pane_id);
    }

    pub fn cancel_stale_pane_generations(&mut self, pane_id: PaneId, generation: Generation) {
        self.queued
            .retain(|request| request.pane_id != pane_id || request.generation == generation);
        self.seen
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
        self.active
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty() && !self.role_batch_pending
    }

    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.queued.len()
    }

    #[cfg(test)]
    pub(crate) fn seen_len(&self) -> usize {
        self.seen.len()
    }

    fn prune_queued_for_snapshot(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        keep: &HashSet<MetadataRoleWorkKey>,
    ) {
        let mut removed = Vec::new();
        self.queued.retain(|request| {
            if request.pane_id != pane_id || request.generation != generation {
                return true;
            }
            let key = MetadataRoleWorkKey::from_request(request);
            if keep.contains(&key) {
                true
            } else {
                removed.push(key);
                false
            }
        });
        for key in removed {
            self.seen.remove(&key);
        }
    }
}

pub fn metadata_role_results_for_requests(
    requests: Vec<MetadataRoleRequest>,
) -> Vec<MetadataRoleResult> {
    requests
        .into_iter()
        .map(metadata_role_result_for_request)
        .collect()
}

pub fn metadata_role_result_for_request(request: MetadataRoleRequest) -> MetadataRoleResult {
    let role = std::fs::metadata(&request.path).ok().and_then(|metadata| {
        let name = request.path.file_name()?.to_string_lossy();
        let is_dir = metadata.is_dir();
        Some(EntryMetadataRole::resolved_from_path(
            name.as_ref(),
            &request.path,
            is_dir,
            &metadata,
            MimeDatabase::shared(),
        ))
    });
    MetadataRoleResult {
        pane_id: request.pane_id,
        generation: request.generation,
        item_id: request.item_id,
        path: request.path,
        role,
    }
}

pub fn apply_metadata_role_result_to_model(
    model: &mut DirectoryModel,
    result: MetadataRoleResult,
) -> bool {
    let Some(role) = result.role else {
        return false;
    };
    !model
        .set_metadata_role(result.item_id, &result.path, role)
        .is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entries::{Entry, EntryData};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn metadata_role_scheduler_queues_once_and_clears_pane_work() {
        let pane_id = PaneId(1);
        let generation = Generation(1);
        let mut scheduler = MetadataRoleScheduler::default();
        let candidate = MetadataRoleCandidate {
            item_id: ItemId(1),
            path: PathBuf::from("/tmp/fika-metadata/payload"),
        };

        assert!(scheduler.queue_candidates(pane_id, generation, vec![candidate.clone()]));
        assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(scheduler.start_role_batch(8).is_none());
        scheduler.finish_role_batch();
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.seen_len(), 0);
    }

    #[test]
    fn metadata_role_scheduler_prunes_invisible_queued_requests() {
        let root = PathBuf::from("/tmp/fika-metadata-prune");
        let first = MetadataRoleCandidate {
            item_id: ItemId(1),
            path: root.join("first"),
        };
        let second = MetadataRoleCandidate {
            item_id: ItemId(2),
            path: root.join("second"),
        };
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![first.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![second.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);
        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests[0].item_id(), second.item_id);
        scheduler.finish_role_batch();
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.seen_len(), 0);
    }

    #[test]
    fn metadata_role_scheduler_releases_finished_active_keys_but_keeps_queued_keys() {
        let root = PathBuf::from("/tmp/fika-metadata-active");
        let first = MetadataRoleCandidate {
            item_id: ItemId(1),
            path: root.join("first"),
        };
        let second = MetadataRoleCandidate {
            item_id: ItemId(2),
            path: root.join("second"),
        };
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(
            PaneId(1),
            Generation(1),
            vec![first.clone(), second.clone()]
        ));
        assert_eq!(scheduler.seen_len(), 2);

        let batch = scheduler.start_role_batch(1).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(scheduler.queued_len(), 1);
        scheduler.finish_role_batch();

        assert_eq!(scheduler.seen_len(), 1);
        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![second]));
        assert_eq!(scheduler.queued_len(), 1);
    }

    #[test]
    fn metadata_role_scheduler_keeps_failed_active_key_seen() {
        let root = PathBuf::from("/tmp/fika-metadata-failed");
        let candidate = MetadataRoleCandidate {
            item_id: ItemId(1),
            path: root.join("missing"),
        };
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![candidate.clone()]));
        let batch = scheduler.start_role_batch(8).unwrap();
        scheduler.finish_role_batch_with_results(&[MetadataRoleResult {
            pane_id: PaneId(1),
            generation: Generation(1),
            item_id: batch.requests[0].item_id(),
            path: batch.requests[0].path().to_path_buf(),
            role: None,
        }]);

        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![candidate]));
        assert_eq!(scheduler.queued_len(), 0);
        assert_eq!(scheduler.seen_len(), 1);
    }

    #[test]
    fn metadata_role_result_applies_only_to_matching_model_item_and_path() {
        let root = PathBuf::from("/tmp/fika-metadata-result");
        let mut model = DirectoryModel::for_directory(root.clone());
        model.replace_listing(
            root.clone(),
            Arc::new(vec![Entry::new(EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                size_bytes: 0,
                modified_secs: None,
                metadata_complete: false,
                mime_type: None,
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let item_id = model.entries()[0].id;
        let role = EntryMetadataRole {
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        };

        assert!(!apply_metadata_role_result_to_model(
            &mut model,
            MetadataRoleResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("other"),
                role: Some(role.clone()),
            },
        ));
        assert!(!model.entries()[0].effective_metadata_complete());

        assert!(apply_metadata_role_result_to_model(
            &mut model,
            MetadataRoleResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                role: Some(role),
            },
        ));
        assert!(model.entries()[0].effective_metadata_complete());
        assert_eq!(model.entries()[0].effective_size_bytes(), 12);
    }

    #[test]
    fn metadata_role_result_reads_filesystem_metadata() {
        let root = std::env::temp_dir().join(format!(
            "fika-metadata-role-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("payload.txt");
        std::fs::write(&path, b"hello").unwrap();

        let result = metadata_role_result_for_request(MetadataRoleRequest {
            pane_id: PaneId(1),
            generation: Generation(2),
            item_id: ItemId(3),
            path: path.clone(),
        });

        assert_eq!(result.pane_id, PaneId(1));
        assert_eq!(result.generation, Generation(2));
        assert_eq!(result.item_id, ItemId(3));
        assert_eq!(result.path, path);
        let role = result.role.unwrap();
        assert_eq!(role.size_bytes, 5);
        assert_eq!(role.mime_type.as_deref(), Some("text/plain"));
        assert!(role.mime_magic_checked);

        let _ = std::fs::remove_dir_all(root);
    }
}
