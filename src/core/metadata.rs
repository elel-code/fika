use super::entries::{EntryMetadataRole, ItemId};
use super::mime::MimeDatabase;
use super::model::DirectoryModel;
use super::pane::{Generation, PaneId};
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetadataWorkKey {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path_hash: u64,
}

impl MetadataWorkKey {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: &MetadataProbeCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path_hash: stable_hash(&candidate.path),
        }
    }

    pub fn from_request(request: &MetadataProbeRequest) -> Self {
        Self {
            pane_id: request.pane_id,
            generation: request.generation,
            item_id: request.item_id,
            path_hash: stable_hash(&request.path),
        }
    }
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProbeCandidate {
    pub item_id: ItemId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProbeRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
}

impl MetadataProbeRequest {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: MetadataProbeCandidate,
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
pub struct MetadataProbeResult {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path: PathBuf,
    pub role: Option<EntryMetadataRole>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProbeBatch {
    pub requests: Vec<MetadataProbeRequest>,
}

#[derive(Debug, Default)]
pub struct MetadataProbeScheduler {
    queued: VecDeque<MetadataProbeRequest>,
    seen: HashSet<MetadataWorkKey>,
    probe_pending: bool,
}

impl MetadataProbeScheduler {
    pub fn queue_candidates(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = MetadataProbeCandidate>,
    ) -> bool {
        let mut queued = false;
        for candidate in candidates {
            let key = MetadataWorkKey::from_candidate(pane_id, generation, &candidate);
            if self.seen.contains(&key) {
                continue;
            }
            let request = MetadataProbeRequest::from_candidate(pane_id, generation, candidate);
            self.seen.insert(key);
            self.queued.push_back(request);
            queued = true;
        }
        queued
    }

    pub fn start_probe_batch(&mut self, batch_size: usize) -> Option<MetadataProbeBatch> {
        if self.probe_pending || self.queued.is_empty() {
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
            self.probe_pending = true;
            Some(MetadataProbeBatch { requests })
        }
    }

    pub fn finish_probe_batch(&mut self) {
        self.probe_pending = false;
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) {
        self.queued.retain(|request| request.pane_id != pane_id);
        self.seen.retain(|key| key.pane_id != pane_id);
    }

    pub fn cancel_stale_pane_generations(&mut self, pane_id: PaneId, generation: Generation) {
        self.queued
            .retain(|request| request.pane_id != pane_id || request.generation == generation);
        self.seen
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty() && self.seen.is_empty() && !self.probe_pending
    }

    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.queued.len()
    }

    #[cfg(test)]
    pub(crate) fn seen_len(&self) -> usize {
        self.seen.len()
    }
}

pub fn metadata_probe_results_for_requests(
    requests: Vec<MetadataProbeRequest>,
) -> Vec<MetadataProbeResult> {
    requests
        .into_iter()
        .map(metadata_probe_result_for_request)
        .collect()
}

pub fn metadata_probe_result_for_request(request: MetadataProbeRequest) -> MetadataProbeResult {
    let role = std::fs::metadata(&request.path).ok().and_then(|metadata| {
        let name = request.path.file_name()?.to_string_lossy();
        let is_dir = metadata.is_dir();
        Some(EntryMetadataRole::from_metadata(
            name.as_ref(),
            is_dir,
            &metadata,
            MimeDatabase::shared(),
        ))
    });
    MetadataProbeResult {
        pane_id: request.pane_id,
        generation: request.generation,
        item_id: request.item_id,
        path: request.path,
        role,
    }
}

pub fn apply_metadata_probe_result_to_model(
    model: &mut DirectoryModel,
    result: MetadataProbeResult,
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
    fn metadata_probe_scheduler_queues_once_and_clears_pane_work() {
        let pane_id = PaneId(1);
        let generation = Generation(1);
        let mut scheduler = MetadataProbeScheduler::default();
        let candidate = MetadataProbeCandidate {
            item_id: ItemId(1),
            path: PathBuf::from("/tmp/fika-metadata/payload"),
        };

        assert!(scheduler.queue_candidates(pane_id, generation, vec![candidate.clone()]));
        assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        let batch = scheduler.start_probe_batch(8).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(scheduler.start_probe_batch(8).is_none());
        scheduler.finish_probe_batch();
        scheduler.cancel_pane(pane_id);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn metadata_probe_result_applies_only_to_matching_model_item_and_path() {
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

        assert!(!apply_metadata_probe_result_to_model(
            &mut model,
            MetadataProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("other"),
                role: Some(role.clone()),
            },
        ));
        assert!(!model.entries()[0].metadata_complete);

        assert!(apply_metadata_probe_result_to_model(
            &mut model,
            MetadataProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                role: Some(role),
            },
        ));
        assert!(model.entries()[0].metadata_complete);
        assert_eq!(model.entries()[0].size_bytes, 12);
    }

    #[test]
    fn metadata_probe_result_reads_filesystem_metadata() {
        let root = std::env::temp_dir().join(format!(
            "fika-metadata-probe-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("payload.txt");
        std::fs::write(&path, b"hello").unwrap();

        let result = metadata_probe_result_for_request(MetadataProbeRequest {
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
