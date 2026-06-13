use super::cache::DirectoryCache;
use super::directory::{DirectoryLister, DirectoryListerEvent, LoadMode, RefreshPair};
use super::entries::Entry;
use super::pane::{Generation, PaneId, PaneState, RequestSerial};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListingRequestKey {
    pub generation: Generation,
    pub request_serial: RequestSerial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadingPaneState {
    pub key: ListingRequestKey,
    pub started_at: Instant,
    pub previous_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingRequest {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub request_serial: RequestSerial,
    pub path: PathBuf,
    pub mode: LoadMode,
}

impl ListingRequest {
    pub fn from_event(event: &DirectoryListerEvent) -> Option<Self> {
        let DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            path,
            mode,
        } = event
        else {
            return None;
        };
        Some(Self {
            pane_id: *pane_id,
            generation: *generation,
            request_serial: *request_serial,
            path: path.clone(),
            mode: *mode,
        })
    }

    pub fn key(&self) -> ListingRequestKey {
        ListingRequestKey {
            generation: self.generation,
            request_serial: self.request_serial,
        }
    }
}

pub fn listing_requests_from_events<'a>(
    events: impl IntoIterator<Item = &'a DirectoryListerEvent>,
) -> Vec<ListingRequest> {
    events
        .into_iter()
        .filter_map(ListingRequest::from_event)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListingBatch {
    path: PathBuf,
    mode: LoadMode,
    requests: Vec<ListingRequest>,
}

impl ListingBatch {
    fn read_events_cancellable(
        &self,
        state: &Arc<(Mutex<ListingWorkerState>, Condvar)>,
    ) -> Option<Vec<DirectoryListerEvent>> {
        let request = self.requests.first()?;
        DirectoryLister::read_listing_events_cancellable(
            request.pane_id,
            request.generation,
            request.request_serial,
            self.path.clone(),
            self.mode,
            || listing_batch_cancelled(state, self),
        )
    }
}

#[derive(Debug, Default)]
struct ListingWorkerState {
    pending: VecDeque<ListingRequest>,
    latest_request_by_pane: HashMap<PaneId, ListingRequestKey>,
    results_by_pane: BTreeMap<PaneId, Vec<DirectoryListerEvent>>,
    cache: DirectoryCache,
    shutdown: bool,
}

impl ListingWorkerState {
    fn schedule(&mut self, request: ListingRequest) {
        if request.mode == LoadMode::Reload {
            self.cache.mark_stale(&request.path);
        }
        self.pending
            .retain(|pending| pending.pane_id != request.pane_id);
        self.latest_request_by_pane
            .insert(request.pane_id, request.key());
        self.results_by_pane.remove(&request.pane_id);
        self.pending.push_back(request);
    }

    fn cancel_pane(&mut self, pane_id: PaneId) {
        self.pending.retain(|pending| pending.pane_id != pane_id);
        self.latest_request_by_pane.remove(&pane_id);
        self.results_by_pane.remove(&pane_id);
    }

    fn mark_cache_stale(&mut self, path: &Path) {
        self.cache.mark_stale(path);
    }

    fn apply_cache_items_added(&mut self, path: &Path, entries: &[Entry]) -> bool {
        let applied = self.cache.apply_items_added(path, entries);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn apply_cache_items_deleted(&mut self, path: &Path, paths: &[PathBuf]) -> bool {
        let applied = self.cache.apply_items_deleted(path, paths);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn apply_cache_items_refreshed(&mut self, path: &Path, pairs: &[RefreshPair]) -> bool {
        let applied = self.cache.apply_items_refreshed(path, pairs);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn remove_cached_directory(&mut self, path: &Path) {
        self.cache.remove(path);
    }

    fn cache_listing_snapshot(&mut self, path: &Path, entries: Arc<Vec<Entry>>) -> bool {
        self.cache.insert_fresh(path, entries).is_some()
    }

    fn cached_events_for(&mut self, request: &ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if request.mode != LoadMode::Load {
            return None;
        }
        let snapshot = self.cache.get_fresh(&request.path)?;
        Some(vec![
            DirectoryListerEvent::ListingRefreshed {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
                entries: Arc::clone(snapshot.entries()),
            },
            DirectoryListerEvent::ListingCompleted {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
            },
        ])
    }

    fn schedule_or_cached(&mut self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if let Some(events) = self.cached_events_for(&request) {
            self.pending
                .retain(|pending| pending.pane_id != request.pane_id);
            self.latest_request_by_pane
                .insert(request.pane_id, request.key());
            self.results_by_pane.remove(&request.pane_id);
            return Some(events);
        }

        self.schedule(request);
        None
    }

    fn pop_batch(&mut self) -> Option<ListingBatch> {
        while let Some(leader) = self.pending.pop_front() {
            if !self.is_current(&leader) {
                continue;
            }

            let path = leader.path.clone();
            let mode = leader.mode;
            let mut requests = vec![leader];
            let mut index = 0;
            while index < self.pending.len() {
                let Some(pending) = self.pending.get(index) else {
                    break;
                };
                if !self.is_current(pending) {
                    self.pending.remove(index);
                    continue;
                }
                if pending.path == path && pending.mode == mode {
                    if let Some(request) = self.pending.remove(index) {
                        requests.push(request);
                    }
                    continue;
                }
                index += 1;
            }

            return Some(ListingBatch {
                path,
                mode,
                requests,
            });
        }
        None
    }

    fn is_current(&self, request: &ListingRequest) -> bool {
        self.latest_request_by_pane
            .get(&request.pane_id)
            .is_some_and(|key| *key == request.key())
    }

    fn publish_batch_if_current(
        &mut self,
        batch: &ListingBatch,
        events: &[DirectoryListerEvent],
    ) -> bool {
        if self.shutdown {
            return false;
        }
        let mut published = false;
        for request in &batch.requests {
            if !self.is_current(request) {
                continue;
            }
            self.results_by_pane
                .insert(request.pane_id, retarget_listing_events(events, request));
            published = true;
        }
        if published && let Some(entries) = listing_refreshed_entries(events) {
            self.cache.insert_fresh(&batch.path, entries);
        }
        published
    }

    fn drain_results(&mut self) -> Vec<Vec<DirectoryListerEvent>> {
        std::mem::take(&mut self.results_by_pane)
            .into_values()
            .collect()
    }
}

pub struct ListingWorker {
    state: Arc<(Mutex<ListingWorkerState>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl ListingWorker {
    pub fn new() -> Self {
        let state = Arc::new((Mutex::new(ListingWorkerState::default()), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let handle = std::thread::spawn(move || listing_worker_loop(worker_state));
        Self {
            state,
            handle: Some(handle),
        }
    }

    pub fn schedule_all(&self, requests: Vec<ListingRequest>) {
        if requests.is_empty() {
            return;
        }
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return;
        }
        for request in requests {
            state.schedule(request);
        }
        cvar.notify_one();
    }

    pub fn schedule_or_cached(&self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return None;
        }
        let cached_events = state.schedule_or_cached(request);
        if cached_events.is_none() {
            cvar.notify_one();
        }
        cached_events
    }

    pub fn mark_cache_stale(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.mark_cache_stale(path);
    }

    pub fn apply_cache_event(&self, event: &DirectoryListerEvent) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        match event {
            DirectoryListerEvent::ItemsAdded { path, entries, .. } => {
                state.apply_cache_items_added(path, entries)
            }
            DirectoryListerEvent::ItemsDeleted { path, paths, .. } => {
                state.apply_cache_items_deleted(path, paths)
            }
            DirectoryListerEvent::ItemsRefreshed { path, pairs, .. } => {
                state.apply_cache_items_refreshed(path, pairs)
            }
            DirectoryListerEvent::CurrentDirectoryRemoved { path, .. } => {
                state.remove_cached_directory(path);
                true
            }
            DirectoryListerEvent::LoadingStarted { mode, path, .. }
                if *mode == LoadMode::Reload =>
            {
                state.mark_cache_stale(path);
                false
            }
            _ => false,
        }
    }

    pub fn remove_cached_directory(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.remove_cached_directory(path);
    }

    pub fn cache_listing_snapshot(&self, path: &Path, entries: Arc<Vec<Entry>>) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return false;
        }
        state.cache_listing_snapshot(path, entries)
    }

    pub fn cancel_pane(&self, pane_id: PaneId) {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.cancel_pane(pane_id);
        cvar.notify_one();
    }

    pub fn drain_results(&self) -> Vec<Vec<DirectoryListerEvent>> {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .drain_results()
    }

    pub fn pending_count(&self) -> usize {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .pending
            .len()
    }
}

impl Default for ListingWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ListingWorker {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending.clear();
            state.results_by_pane.clear();
            cvar.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn listing_batch_cancelled(
    state: &Arc<(Mutex<ListingWorkerState>, Condvar)>,
    batch: &ListingBatch,
) -> bool {
    let (lock, _) = &**state;
    let guard = lock.lock().expect("listing worker state poisoned");
    guard.shutdown
        || !batch
            .requests
            .iter()
            .any(|request| guard.is_current(request))
}

fn retarget_listing_events(
    events: &[DirectoryListerEvent],
    target: &ListingRequest,
) -> Vec<DirectoryListerEvent> {
    events
        .iter()
        .map(|event| retarget_listing_event(event, target))
        .collect()
}

fn retarget_listing_event(
    event: &DirectoryListerEvent,
    target: &ListingRequest,
) -> DirectoryListerEvent {
    match event {
        DirectoryListerEvent::LoadingStarted { .. } => DirectoryListerEvent::LoadingStarted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            mode: target.mode,
        },
        DirectoryListerEvent::ItemsAdded { entries, .. } => DirectoryListerEvent::ItemsAdded {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            entries: entries.clone(),
        },
        DirectoryListerEvent::ItemsDeleted { paths, .. } => DirectoryListerEvent::ItemsDeleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            paths: paths.clone(),
        },
        DirectoryListerEvent::ItemsRefreshed { pairs, .. } => {
            DirectoryListerEvent::ItemsRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                pairs: pairs.clone(),
            }
        }
        DirectoryListerEvent::ListingRefreshed { entries, .. } => {
            DirectoryListerEvent::ListingRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                entries: Arc::clone(entries),
            }
        }
        DirectoryListerEvent::ListingCompleted { .. } => DirectoryListerEvent::ListingCompleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
        },
        DirectoryListerEvent::CurrentDirectoryRemoved { .. } => {
            DirectoryListerEvent::CurrentDirectoryRemoved {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
            }
        }
        DirectoryListerEvent::Error { message, .. } => DirectoryListerEvent::Error {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            message: message.clone(),
        },
    }
}

fn listing_refreshed_entries(events: &[DirectoryListerEvent]) -> Option<Arc<Vec<Entry>>> {
    events.iter().find_map(|event| {
        if let DirectoryListerEvent::ListingRefreshed { entries, .. } = event {
            Some(Arc::clone(entries))
        } else {
            None
        }
    })
}

pub fn update_loading_state_for_event(
    loading_panes: &mut HashMap<PaneId, LoadingPaneState>,
    pane: Option<&PaneState>,
    event: &DirectoryListerEvent,
    now: Instant,
    previous_summary: Option<String>,
) {
    match event {
        DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            if pane.is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            }) {
                loading_panes.insert(
                    *pane_id,
                    LoadingPaneState {
                        key: ListingRequestKey {
                            generation: *generation,
                            request_serial: *request_serial,
                        },
                        started_at: now,
                        previous_summary,
                    },
                );
            } else {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::Error {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::CurrentDirectoryRemoved {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let key = ListingRequestKey {
                generation: *generation,
                request_serial: *request_serial,
            };
            if loading_panes
                .get(pane_id)
                .is_some_and(|state| state.key == key)
            {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsDeleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let Some(pane) = pane else {
                loading_panes.remove(pane_id);
                return;
            };
            if pane.generation != *generation
                || loading_panes
                    .get(pane_id)
                    .is_some_and(|state| state.key.request_serial < *request_serial)
            {
                loading_panes.remove(pane_id);
            }
        }
    }
}

fn listing_worker_loop(state: Arc<(Mutex<ListingWorkerState>, Condvar)>) {
    loop {
        let batch = {
            let (lock, cvar) = &*state;
            let mut guard = lock.lock().expect("listing worker state poisoned");
            while guard.pending.is_empty() && !guard.shutdown {
                guard = cvar.wait(guard).expect("listing worker state poisoned");
            }
            if guard.shutdown {
                return;
            }
            guard
                .pop_batch()
                .expect("pending listing request disappeared")
        };

        let Some(events) = batch.read_events_cancellable(&state) else {
            continue;
        };
        let (lock, _) = &*state;
        let mut guard = lock.lock().expect("listing worker state poisoned");
        if guard.shutdown {
            return;
        }
        guard.publish_batch_if_current(&batch, &events);
    }
}

#[cfg(test)]
mod tests {
    use super::super::cache::DirectoryCacheState;
    use super::*;
    use std::fs;
    use std::process;
    use std::time::Duration;

    #[test]
    fn listing_requests_from_events_keeps_only_loading_events() {
        let first = listing_request(1, 1);
        let second = listing_request(2, 1);
        let events = vec![
            listing_started(&first),
            listing_completed(&first),
            listing_started(&second),
        ];

        assert_eq!(
            listing_requests_from_events(events.iter()),
            vec![first, second]
        );
    }

    #[test]
    fn listing_worker_state_keeps_latest_pending_request_per_pane() {
        let mut state = ListingWorkerState::default();
        let old_first = listing_request(1, 1);
        let second = listing_request(2, 1);
        let new_first = listing_request(1, 2);

        state.schedule(old_first);
        state.schedule(second.clone());
        state.schedule(new_first.clone());

        assert_eq!(
            state.pop_batch().map(|batch| batch.requests),
            Some(vec![second])
        );
        assert_eq!(
            state.pop_batch().map(|batch| batch.requests),
            Some(vec![new_first])
        );
        assert_eq!(state.pop_batch(), None);
    }

    #[test]
    fn listing_worker_state_batches_same_path_requests() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let different = listing_request_at(2, 1, "/tmp/fika-other-listing");
        let second = listing_request_at(3, 1, "/tmp/fika-shared-listing");

        state.schedule(first.clone());
        state.schedule(different.clone());
        state.schedule(second.clone());

        let shared_batch = state.pop_batch().unwrap();
        assert_eq!(shared_batch.path, PathBuf::from("/tmp/fika-shared-listing"));
        assert_eq!(shared_batch.requests, vec![first, second]);

        let different_batch = state.pop_batch().unwrap();
        assert_eq!(different_batch.requests, vec![different]);
        assert_eq!(state.pop_batch(), None);
    }

    #[test]
    fn retarget_listing_events_preserves_shared_listing_entries() {
        let source = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let target = listing_request_at(2, 7, "/tmp/fika-shared-listing");
        let entries = Arc::new(vec![Entry::new(super::super::entries::EntryData {
            name: Arc::from("shared.txt"),
            name_width_units: 10,
            size_bytes: 4,
            modified_secs: None,
            thumbnail_path: None,
            trash_original_path: None,
            trash_deletion_time: None,
            mime_type: None,
            is_dir: false,
        })]);
        let events = vec![DirectoryListerEvent::ListingRefreshed {
            pane_id: source.pane_id,
            generation: source.generation,
            request_serial: source.request_serial,
            path: source.path.clone(),
            entries: Arc::clone(&entries),
        }];

        let retargeted = retarget_listing_events(&events, &target);

        let DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            path,
            entries: retargeted_entries,
        } = &retargeted[0]
        else {
            panic!("expected retargeted listing");
        };
        assert_eq!(*pane_id, target.pane_id);
        assert_eq!(*generation, target.generation);
        assert_eq!(*request_serial, target.request_serial);
        assert_eq!(path, &target.path);
        assert!(Arc::ptr_eq(&entries, retargeted_entries));
    }

    #[test]
    fn listing_worker_state_drops_stale_results() {
        let mut state = ListingWorkerState::default();
        let old = listing_request(1, 1);
        let new = listing_request(1, 2);

        state.schedule(old.clone());
        let old_batch = listing_batch(vec![old.clone()]);
        let old_events = vec![listing_completed(&old)];
        assert!(state.publish_batch_if_current(&old_batch, &old_events));
        assert_eq!(state.results_by_pane.len(), 1);

        state.schedule(new.clone());
        assert!(state.results_by_pane.is_empty());
        assert!(!state.publish_batch_if_current(&old_batch, &old_events));
        assert!(state.drain_results().is_empty());

        let new_batch = listing_batch(vec![new.clone()]);
        let new_events = vec![listing_completed(&new)];
        assert!(state.publish_batch_if_current(&new_batch, &new_events));
        let results = state.drain_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0][0].request_serial(), RequestSerial(2));
    }

    #[test]
    fn listing_worker_state_cancels_closed_pane_work() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
        state.schedule(first.clone());
        state.schedule(second.clone());

        let batch = listing_batch(vec![first.clone(), second.clone()]);
        let events = vec![listing_completed(&first)];
        assert!(state.publish_batch_if_current(&batch, &events));
        assert_eq!(state.results_by_pane.len(), 2);

        state.cancel_pane(first.pane_id);

        assert!(!state.latest_request_by_pane.contains_key(&first.pane_id));
        assert!(!state.results_by_pane.contains_key(&first.pane_id));
        assert!(
            state
                .pending
                .iter()
                .all(|pending| pending.pane_id != first.pane_id)
        );
        assert!(state.results_by_pane.contains_key(&second.pane_id));
    }

    #[test]
    fn listing_worker_cache_serves_load_with_shared_entries() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

        let cached = state.cached_events_for(&second).expect("cache miss");
        let DirectoryListerEvent::ListingRefreshed {
            pane_id,
            request_serial,
            entries: cached_entries,
            ..
        } = &cached[0]
        else {
            panic!("expected cached listing refresh");
        };
        assert_eq!(*pane_id, second.pane_id);
        assert_eq!(*request_serial, second.request_serial);
        assert!(Arc::ptr_eq(&entries, cached_entries));
        assert!(matches!(
            cached[1],
            DirectoryListerEvent::ListingCompleted { .. }
        ));
    }

    #[test]
    fn listing_worker_cache_serves_promoted_model_snapshot() {
        let mut state = ListingWorkerState::default();
        let request = listing_request_at(7, 1, "/tmp/fika-promoted-listing");
        let entries = test_entries(&["promoted.txt"]);

        assert!(state.cache_listing_snapshot(&request.path, Arc::clone(&entries)));

        let cached = state
            .schedule_or_cached(request.clone())
            .expect("promoted snapshot should be served from cache");
        assert!(state.pending.is_empty());
        let DirectoryListerEvent::ListingRefreshed {
            pane_id,
            request_serial,
            entries: cached_entries,
            ..
        } = &cached[0]
        else {
            panic!("expected cached listing refresh");
        };
        assert_eq!(*pane_id, request.pane_id);
        assert_eq!(*request_serial, request.request_serial);
        assert!(Arc::ptr_eq(cached_entries, &entries));
    }

    #[test]
    fn listing_worker_cache_hit_does_not_schedule_background_reload() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        let first_batch = state
            .pop_batch()
            .expect("scheduled listing should be pending");
        assert_eq!(first_batch.requests, vec![first]);
        assert!(state.publish_batch_if_current(&first_batch, &events));

        let cached = state
            .schedule_or_cached(second.clone())
            .expect("fresh cache should serve request directly");

        assert_eq!(cached.len(), 2);
        assert!(state.pending.is_empty());
        assert_eq!(
            state.latest_request_by_pane.get(&second.pane_id),
            Some(&second.key())
        );
    }

    #[test]
    fn listing_worker_reloads_when_fresh_cache_metadata_is_stale() {
        let root = temp_root("fresh-cache-stale");
        let request = listing_request_at(1, 1, root.to_str().unwrap());
        let entries = test_entries(&["cached.txt"]);
        let mut state = ListingWorkerState::default();
        assert!(state.cache_listing_snapshot(&root, Arc::clone(&entries)));

        std::thread::sleep(Duration::from_millis(20));
        fs::write(root.join("new.txt"), b"changed").unwrap();

        assert!(state.schedule_or_cached(request.clone()).is_none());
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0], request);
        let snapshot = state.cache.get(&root).unwrap();
        assert_eq!(snapshot.state(), DirectoryCacheState::Stale);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn listing_worker_cache_ignores_reload_and_can_remove_directory() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let mut reload = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        reload.mode = LoadMode::Reload;
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

        assert!(state.cached_events_for(&reload).is_none());
        state.schedule(reload);
        let snapshot = state
            .cache
            .get(Path::new("/tmp/fika-cached-listing"))
            .expect("cache should retain stale payload");
        assert_eq!(snapshot.state(), DirectoryCacheState::Stale);
        assert!(
            state
                .cached_events_for(&listing_request_at(3, 3, "/tmp/fika-cached-listing"))
                .is_none()
        );

        state.remove_cached_directory(Path::new("/tmp/fika-cached-listing"));
        assert!(
            state
                .cache
                .get(Path::new("/tmp/fika-cached-listing"))
                .is_none()
        );
    }

    #[test]
    fn listing_worker_cache_applies_incremental_delta_for_next_load() {
        let root = temp_root("incremental-cache");
        let next = listing_request_at(2, 2, root.to_str().unwrap());
        let mut state = ListingWorkerState::default();
        assert!(state.cache_listing_snapshot(&root, test_entries(&["a.txt"])));

        fs::write(root.join("b.txt"), b"b").unwrap();
        let added = test_entries(&["b.txt"]);
        assert!(state.apply_cache_items_added(&root, added.as_slice()));

        let cached = state
            .schedule_or_cached(next.clone())
            .expect("incremental cache should serve next load");
        assert!(state.pending.is_empty());
        let DirectoryListerEvent::ListingRefreshed {
            entries: cached_entries,
            request_serial,
            ..
        } = &cached[0]
        else {
            panic!("expected cached listing refresh");
        };
        assert_eq!(*request_serial, next.request_serial);
        assert_eq!(entry_names(cached_entries), vec!["a.txt", "b.txt"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn listing_batch_cancelled_only_when_all_requests_are_stale() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
        state.schedule(first.clone());
        state.schedule(second.clone());
        let batch = listing_batch(vec![first.clone(), second.clone()]);
        let shared = Arc::new((Mutex::new(state), Condvar::new()));

        {
            let (lock, _) = &*shared;
            lock.lock()
                .expect("listing worker state poisoned")
                .cancel_pane(first.pane_id);
        }
        assert!(!listing_batch_cancelled(&shared, &batch));

        {
            let (lock, _) = &*shared;
            lock.lock()
                .expect("listing worker state poisoned")
                .cancel_pane(second.pane_id);
        }
        assert!(listing_batch_cancelled(&shared, &batch));
    }

    #[test]
    fn loading_state_tracks_current_request_and_ignores_stale_events() {
        let mut controller =
            super::super::pane::PaneController::new(PathBuf::from("/tmp/fika-loading"));
        let pane_id = controller.focused().unwrap();
        let start = controller.reload(pane_id).unwrap();
        let mut loading = HashMap::new();
        let now = Instant::now();

        update_loading_state_for_event(
            &mut loading,
            controller.pane(pane_id),
            &start,
            now,
            Some("2 folders, 3 files".to_string()),
        );
        assert_eq!(
            loading.get(&pane_id).map(|state| state.key),
            Some(ListingRequestKey {
                generation: start.generation(),
                request_serial: start.request_serial(),
            })
        );
        assert_eq!(
            loading
                .get(&pane_id)
                .and_then(|state| state.previous_summary.as_deref()),
            Some("2 folders, 3 files")
        );

        let stale = DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: start.generation(),
            request_serial: RequestSerial(start.request_serial().0 + 1),
            path: start.path().to_path_buf(),
        };
        update_loading_state_for_event(&mut loading, controller.pane(pane_id), &stale, now, None);
        assert!(loading.contains_key(&pane_id));

        let completed = DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: start.generation(),
            request_serial: start.request_serial(),
            path: start.path().to_path_buf(),
        };
        update_loading_state_for_event(
            &mut loading,
            controller.pane(pane_id),
            &completed,
            now,
            None,
        );
        assert!(!loading.contains_key(&pane_id));
    }

    #[test]
    fn loading_state_rejects_stale_started_event_for_old_generation() {
        let mut controller =
            super::super::pane::PaneController::new(PathBuf::from("/tmp/fika-loading-a"));
        let pane_id = controller.focused().unwrap();
        let stale = controller.reload(pane_id).unwrap();
        controller.load(pane_id, PathBuf::from("/tmp/fika-loading-b"));
        let mut loading = HashMap::new();

        update_loading_state_for_event(
            &mut loading,
            controller.pane(pane_id),
            &stale,
            Instant::now(),
            None,
        );

        assert!(loading.is_empty());
    }

    fn listing_request(pane: u64, serial: u64) -> ListingRequest {
        listing_request_at(pane, serial, &format!("/tmp/fika-listing-{pane}"))
    }

    fn listing_request_at(pane: u64, serial: u64, path: &str) -> ListingRequest {
        ListingRequest {
            pane_id: PaneId(pane),
            generation: Generation(1),
            request_serial: RequestSerial(serial),
            path: PathBuf::from(path),
            mode: LoadMode::Load,
        }
    }

    fn listing_batch(requests: Vec<ListingRequest>) -> ListingBatch {
        ListingBatch {
            path: requests[0].path.clone(),
            mode: requests[0].mode,
            requests,
        }
    }

    fn listing_started(request: &ListingRequest) -> DirectoryListerEvent {
        DirectoryListerEvent::LoadingStarted {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
            mode: request.mode,
        }
    }

    fn listing_completed(request: &ListingRequest) -> DirectoryListerEvent {
        DirectoryListerEvent::ListingCompleted {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
        }
    }

    fn listing_refreshed(
        request: &ListingRequest,
        entries: Arc<Vec<Entry>>,
    ) -> DirectoryListerEvent {
        DirectoryListerEvent::ListingRefreshed {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
            entries,
        }
    }

    fn test_entries(names: &[&str]) -> Arc<Vec<Entry>> {
        Arc::new(
            names
                .iter()
                .map(|name| {
                    Entry::new(super::super::entries::EntryData {
                        name: Arc::from(*name),
                        name_width_units: name.len() as u16,
                        size_bytes: 0,
                        modified_secs: None,
                        thumbnail_path: None,
                        trash_original_path: None,
                        trash_deletion_time: None,
                        mime_type: None,
                        is_dir: false,
                    })
                })
                .collect(),
        )
    }

    fn entry_names(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|entry| entry.name.to_string()).collect()
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("fika-listing-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
}
