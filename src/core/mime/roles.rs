use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use super::super::{
    entries::ItemId,
    model::DirectoryModel,
    pane::{Generation, PaneId},
};
use super::{GENERIC_BINARY_MIME, MimeDatabase};

const MIME_MAGIC_READ_LIMIT: usize = 4096;
const MIME_PROBE_WORKER_LIMIT: usize = 4;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MimeWorkKey {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub modified_secs: Option<u64>,
    pub path_hash: u64,
    pub mime_hash: Option<u64>,
}

impl MimeWorkKey {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: &MimeProbeCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            modified_secs: candidate.modified_secs,
            path_hash: stable_hash(&candidate.path),
            mime_hash: candidate.mime_type.as_deref().map(stable_hash),
        }
    }

    pub fn from_request(request: &MimeProbeRequest) -> Self {
        Self {
            pane_id: request.pane_id,
            generation: request.generation,
            item_id: request.item_id,
            modified_secs: request.modified_secs,
            path_hash: stable_hash(&request.path),
            mime_hash: request.mime_type.as_deref().map(stable_hash),
        }
    }
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeProbeCandidate {
    pub item_id: ItemId,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_secs: Option<u64>,
    pub metadata_complete: bool,
    pub mime_type: Option<String>,
    pub mime_magic_checked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeProbeRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
    modified_secs: Option<u64>,
    mime_type: Option<String>,
}

impl MimeProbeRequest {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: MimeProbeCandidate,
    ) -> Option<Self> {
        if !candidate.metadata_complete {
            return None;
        }
        if !mime_magic_probe_required(
            false,
            candidate.size_bytes,
            candidate.mime_type.as_deref(),
            candidate.mime_magic_checked,
        ) {
            return None;
        }
        Some(Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path: candidate.path,
            modified_secs: candidate.modified_secs,
            mime_type: candidate.mime_type,
        })
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

    pub fn modified_secs(&self) -> Option<u64> {
        self.modified_secs
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeProbeResult {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path: PathBuf,
    pub modified_secs: Option<u64>,
    pub mime_type: Option<Arc<str>>,
    pub mime_magic_checked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeProbeBatch {
    pub requests: Vec<MimeProbeRequest>,
}

#[derive(Debug, Default)]
pub struct MimeProbeScheduler {
    requests: VecDeque<MimeProbeRequest>,
    seen: HashSet<MimeWorkKey>,
    active: HashSet<MimeWorkKey>,
    probe_pending: bool,
}

impl MimeProbeScheduler {
    pub fn queue_candidates(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = MimeProbeCandidate>,
    ) -> bool {
        let mut keep = HashSet::new();
        let mut pending = Vec::new();
        for candidate in candidates {
            let key = MimeWorkKey::from_candidate(pane_id, generation, &candidate);
            keep.insert(key.clone());
            if let Some(request) = MimeProbeRequest::from_candidate(pane_id, generation, candidate)
            {
                pending.push((key, request));
            }
        }
        self.prune_queued_for_snapshot(pane_id, generation, &keep);

        let mut queued = false;
        for (key, request) in pending {
            if self.seen.contains(&key) {
                continue;
            }
            self.seen.insert(key);
            self.requests.push_back(request);
            queued = true;
        }
        queued
    }

    pub fn start_probe_batch(&mut self, batch_size: usize) -> Option<MimeProbeBatch> {
        if self.probe_pending || self.requests.is_empty() {
            return None;
        }
        let mut requests = Vec::new();
        while requests.len() < batch_size {
            let Some(request) = self.requests.pop_front() else {
                break;
            };
            requests.push(request);
        }
        if requests.is_empty() {
            return None;
        }
        self.probe_pending = true;
        self.active = requests.iter().map(MimeWorkKey::from_request).collect();
        Some(MimeProbeBatch { requests })
    }

    pub fn finish_probe_batch(&mut self) {
        self.probe_pending = false;
        for key in self.active.drain() {
            self.seen.remove(&key);
        }
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) {
        self.requests.retain(|request| request.pane_id != pane_id);
        self.seen.retain(|key| key.pane_id != pane_id);
        self.active.retain(|key| key.pane_id != pane_id);
    }

    pub fn cancel_stale_pane_generations(&mut self, pane_id: PaneId, generation: Generation) {
        self.requests
            .retain(|request| request.pane_id != pane_id || request.generation == generation);
        self.seen
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
        self.active
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && !self.probe_pending
    }

    pub fn queued_len(&self) -> usize {
        self.requests.len()
    }

    pub fn seen_len(&self) -> usize {
        self.seen.len()
    }

    fn prune_queued_for_snapshot(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        keep: &HashSet<MimeWorkKey>,
    ) {
        let mut removed = Vec::new();
        self.requests.retain(|request| {
            if request.pane_id != pane_id || request.generation != generation {
                return true;
            }
            let key = MimeWorkKey::from_request(request);
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

pub fn mime_magic_probe_required(
    is_dir: bool,
    size_bytes: u64,
    mime_type: Option<&str>,
    mime_magic_checked: bool,
) -> bool {
    !mime_magic_checked && !is_dir && size_bytes > 0 && mime_type == Some(GENERIC_BINARY_MIME)
}

pub fn mime_probe_results_for_requests(requests: Vec<MimeProbeRequest>) -> Vec<MimeProbeResult> {
    mime_probe_results_with_worker(
        requests,
        MIME_PROBE_WORKER_LIMIT,
        mime_probe_result_for_request,
    )
}

pub fn apply_mime_probe_result_to_model(
    model: &mut DirectoryModel,
    result: MimeProbeResult,
) -> bool {
    let Some(index) = model.index_of_id(result.item_id) else {
        return false;
    };
    let Some(entry) = model.get(index) else {
        return false;
    };
    if entry.is_dir
        || entry.modified_secs != result.modified_secs
        || model.path_for_index(index).as_deref() != Some(result.path.as_path())
    {
        return false;
    }
    !model
        .set_mime_role(result.item_id, result.mime_type, result.mime_magic_checked)
        .is_empty()
}

fn mime_probe_results_with_worker(
    requests: Vec<MimeProbeRequest>,
    worker_limit: usize,
    worker: impl Fn(MimeProbeRequest) -> Option<MimeProbeResult> + Send + Sync,
) -> Vec<MimeProbeResult> {
    if requests.is_empty() {
        return Vec::new();
    }

    let worker_count = worker_limit.clamp(1, requests.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(requests)));
    let results = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let worker = &worker;
            scope.spawn(move || {
                loop {
                    let request = {
                        let Ok(mut queue) = queue.lock() else {
                            break;
                        };
                        queue.pop_front()
                    };
                    let Some(request) = request else {
                        break;
                    };
                    let Some(result) = worker(request) else {
                        continue;
                    };
                    if let Ok(mut results) = results.lock() {
                        results.push(result);
                    }
                }
            });
        }
    });

    Arc::into_inner(results)
        .and_then(|results| results.into_inner().ok())
        .unwrap_or_default()
}

fn mime_probe_result_for_request(request: MimeProbeRequest) -> Option<MimeProbeResult> {
    let mime_type = read_mime_magic(request.path())
        .ok()
        .flatten()
        .and_then(|magic| MimeDatabase::shared().mime_for_path(request.path(), false, Some(&magic)))
        .or_else(|| request.mime_type().map(Arc::from));
    Some(MimeProbeResult {
        pane_id: request.pane_id(),
        generation: request.generation(),
        item_id: request.item_id(),
        path: request.path().to_path_buf(),
        modified_secs: request.modified_secs(),
        mime_type,
        mime_magic_checked: true,
    })
}

fn read_mime_magic(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = vec![0; MIME_MAGIC_READ_LIMIT];
    let read = file.read(&mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    bytes.truncate(read);
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::super::super::entries::{Entry, EntryData};
    use super::super::super::model::{ChangedRoles, DirectoryModelSignal, ItemRange};
    use super::*;

    #[test]
    fn mime_probe_required_only_for_generic_nonempty_files() {
        assert!(mime_magic_probe_required(
            false,
            10,
            Some("application/octet-stream"),
            false
        ));
        assert!(!mime_magic_probe_required(
            true,
            10,
            Some("application/octet-stream"),
            false
        ));
        assert!(!mime_magic_probe_required(
            false,
            0,
            Some("application/octet-stream"),
            false
        ));
        assert!(!mime_magic_probe_required(
            false,
            10,
            Some("image/png"),
            false
        ));
        assert!(!mime_magic_probe_required(
            false,
            10,
            Some("application/octet-stream"),
            true
        ));
    }

    #[test]
    fn mime_probe_detects_magic_mime_for_visible_generic_file() {
        let root = temp_root("mime-probe-detect");
        let path = root.join("payload");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();
        let request = MimeProbeRequest::from_candidate(
            PaneId(1),
            Generation(2),
            MimeProbeCandidate {
                item_id: ItemId(3),
                path: path.clone(),
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(GENERIC_BINARY_MIME.to_string()),
                mime_magic_checked: false,
            },
        )
        .unwrap();

        let result = mime_probe_result_for_request(request).unwrap();

        assert_eq!(result.pane_id, PaneId(1));
        assert_eq!(result.generation, Generation(2));
        assert_eq!(result.item_id, ItemId(3));
        assert_eq!(result.path, path);
        assert_eq!(result.modified_secs, Some(42));
        assert_eq!(result.mime_type.as_deref(), Some("image/png"));
        assert!(result.mime_magic_checked);
    }

    #[test]
    fn mime_probe_candidate_requires_complete_metadata() {
        let request = MimeProbeRequest::from_candidate(
            PaneId(1),
            Generation(2),
            MimeProbeCandidate {
                item_id: ItemId(3),
                path: PathBuf::from("/tmp/fika-mime-incomplete/payload"),
                size_bytes: 12,
                modified_secs: None,
                metadata_complete: false,
                mime_type: Some(GENERIC_BINARY_MIME.to_string()),
                mime_magic_checked: false,
            },
        );

        assert_eq!(request, None);
    }

    #[test]
    fn mime_work_key_keeps_path_and_mime_identity_without_storing_paths() {
        let root = temp_root("mime-work-key");
        let pane_id = PaneId(1);
        let generation = Generation(1);
        let first = mime_candidate(1, root.join("first"));
        let renamed = mime_candidate(1, root.join("renamed"));
        let mut retagged = first.clone();
        retagged.mime_type = Some("application/x-custom".to_string());

        assert_ne!(
            MimeWorkKey::from_candidate(pane_id, generation, &first),
            MimeWorkKey::from_candidate(pane_id, generation, &renamed)
        );
        assert_ne!(
            MimeWorkKey::from_candidate(pane_id, generation, &first),
            MimeWorkKey::from_candidate(pane_id, generation, &retagged)
        );
    }

    #[test]
    fn mime_probe_scheduler_deduplicates_and_prunes_invisible_requests() {
        let root = temp_root("mime-probe-scheduler");
        let first = mime_candidate(1, root.join("first"));
        let second = mime_candidate(2, root.join("second"));
        let mut scheduler = MimeProbeScheduler::default();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![first.clone()]));
        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![first.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![second.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);
        let batch = scheduler.start_probe_batch(8).unwrap();
        assert_eq!(batch.requests[0].item_id(), second.item_id);
        scheduler.finish_probe_batch();
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.seen_len(), 0);
    }

    #[test]
    fn mime_probe_scheduler_releases_finished_active_keys_but_keeps_queued_keys() {
        let root = temp_root("mime-probe-scheduler-active");
        let first = mime_candidate(1, root.join("first"));
        let second = mime_candidate(2, root.join("second"));
        let mut scheduler = MimeProbeScheduler::default();

        assert!(scheduler.queue_candidates(
            PaneId(1),
            Generation(1),
            vec![first.clone(), second.clone()]
        ));
        assert_eq!(scheduler.seen_len(), 2);

        let batch = scheduler.start_probe_batch(1).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(scheduler.queued_len(), 1);
        scheduler.finish_probe_batch();

        assert_eq!(scheduler.seen_len(), 1);
        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![second]));
        assert_eq!(scheduler.queued_len(), 1);
    }

    #[test]
    fn mime_probe_result_applies_only_to_matching_model_item_path_and_mtime() {
        let root = PathBuf::from("/tmp/fika-mime-probe-result");
        let mut model = DirectoryModel::for_directory(root.clone());
        model.replace_listing(
            root.clone(),
            Arc::new(vec![Entry::new(EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from(GENERIC_BINARY_MIME)),
                mime_magic_checked: false,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let item_id = model.entries()[0].id;

        assert!(!apply_mime_probe_result_to_model(
            &mut model,
            MimeProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("other"),
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            },
        ));
        assert_eq!(
            model.entries()[0].mime_type.as_deref(),
            Some(GENERIC_BINARY_MIME)
        );

        assert!(!apply_mime_probe_result_to_model(
            &mut model,
            MimeProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                modified_secs: Some(43),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            },
        ));
        assert_eq!(
            model.entries()[0].mime_type.as_deref(),
            Some(GENERIC_BINARY_MIME)
        );

        assert!(apply_mime_probe_result_to_model(
            &mut model,
            MimeProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            },
        ));
        assert_eq!(model.entries()[0].mime_type.as_deref(), Some("image/png"));
        assert!(model.entries()[0].mime_magic_checked);
        assert_eq!(
            model.set_mime_role(item_id, Some(Arc::from("image/png")), true),
            Vec::<DirectoryModelSignal>::new()
        );
        assert_eq!(
            model.set_mime_role(item_id, Some(Arc::from("image/jpeg")), true),
            vec![DirectoryModelSignal::ItemsChanged(
                vec![ItemRange { start: 0, len: 1 }],
                ChangedRoles::metadata(),
            )]
        );
    }

    #[test]
    fn mime_probe_result_marks_same_generic_mime_as_checked() {
        let root = PathBuf::from("/tmp/fika-mime-probe-same-result");
        let mut model = DirectoryModel::for_directory(root.clone());
        model.replace_listing(
            root.clone(),
            Arc::new(vec![Entry::new(EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from(GENERIC_BINARY_MIME)),
                mime_magic_checked: false,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let item_id = model.entries()[0].id;

        assert!(apply_mime_probe_result_to_model(
            &mut model,
            MimeProbeResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                modified_secs: Some(42),
                mime_type: Some(Arc::from(GENERIC_BINARY_MIME)),
                mime_magic_checked: true,
            },
        ));

        assert_eq!(
            model.entries()[0].mime_type.as_deref(),
            Some(GENERIC_BINARY_MIME)
        );
        assert!(model.entries()[0].mime_magic_checked);
    }

    fn mime_candidate(item_id: u64, path: PathBuf) -> MimeProbeCandidate {
        MimeProbeCandidate {
            item_id: ItemId(item_id),
            path,
            size_bytes: 12,
            modified_secs: Some(42),
            metadata_complete: true,
            mime_type: Some(GENERIC_BINARY_MIME.to_string()),
            mime_magic_checked: false,
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "fika-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
