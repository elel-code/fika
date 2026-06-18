use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::ui::icons::{FileIconCache, FileIconResolveRequest};

use super::snapshot::RawFileGridSnapshot;

pub(crate) const DOLPHIN_VISIBLE_ICON_SYNC_BUDGET: Duration = Duration::from_millis(200);
pub(crate) const FILE_ICON_RESOLVE_BATCH_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FileIconSyncStats {
    pub(crate) candidates: usize,
    pub(crate) cached: usize,
    pub(crate) queued: usize,
    pub(crate) resolved: usize,
    pub(crate) changed: usize,
    pub(crate) budget_exhausted: bool,
}

impl FileIconSyncStats {
    pub(crate) fn has_activity(self) -> bool {
        self.candidates > 0 || self.budget_exhausted
    }
}

#[derive(Debug, Default)]
pub(crate) struct FileIconResolveQueue {
    queued: VecDeque<FileIconResolveRequest>,
    seen: HashSet<FileIconResolveRequest>,
    pending: bool,
}

impl FileIconResolveQueue {
    pub(crate) fn queue(&mut self, request: FileIconResolveRequest) -> bool {
        if !self.seen.insert(request.clone()) {
            return false;
        }

        self.queued.push_back(request);
        true
    }

    pub(crate) fn start_next_batch(&mut self) -> Option<Vec<FileIconResolveRequest>> {
        if self.pending || self.queued.is_empty() {
            return None;
        }

        let mut requests = Vec::new();
        while requests.len() < FILE_ICON_RESOLVE_BATCH_SIZE {
            let Some(request) = self.queued.pop_front() else {
                break;
            };
            requests.push(request);
        }

        if requests.is_empty() {
            return None;
        }

        self.pending = true;
        Some(requests)
    }

    pub(crate) fn finish_batch(&mut self, finished_requests: Vec<FileIconResolveRequest>) {
        self.pending = false;
        for request in finished_requests {
            self.seen.remove(&request);
        }
    }

    pub(crate) fn contains(&self, request: &FileIconResolveRequest) -> bool {
        self.seen.contains(request)
    }
}

pub(crate) fn resolve_visible_file_icons_for_raw_grid_with_stats(
    file_icons: &mut FileIconCache,
    queue: &FileIconResolveQueue,
    raw_file_grid: &RawFileGridSnapshot,
    file_icon_size: f32,
    budget: Duration,
) -> FileIconSyncStats {
    let started = Instant::now();
    let mut stats = FileIconSyncStats::default();
    raw_file_grid.for_each_visible_file_icon_resolve_candidate(file_icon_size, |request| {
        if started.elapsed() >= budget {
            stats.budget_exhausted = true;
            return false;
        }
        stats.candidates += 1;
        let queued_request = file_icons.resolve_request_for(
            request.path,
            request.is_dir,
            request.mime_type.clone(),
            request.mime_magic_checked,
            request.icon_size,
        );
        if queued_request
            .as_ref()
            .is_some_and(|request| queue.contains(request))
        {
            stats.queued += 1;
            return true;
        }
        let changed = file_icons.resolve_now_for(
            request.path,
            request.is_dir,
            request.mime_type.clone(),
            request.mime_magic_checked,
            request.icon_size,
        );
        if changed {
            stats.resolved += 1;
            stats.changed += 1;
        } else {
            stats.cached += 1;
        }
        true
    });
    stats
}

pub(crate) fn queue_file_icon_resolve_work_for_raw_grid(
    file_icons: &FileIconCache,
    queue: &mut FileIconResolveQueue,
    raw_file_grid: &RawFileGridSnapshot,
    file_icon_size: f32,
) -> bool {
    raw_file_grid.queue_file_icon_resolve_candidates(file_icon_size, |request| {
        let request = file_icons.resolve_request_for(
            request.path,
            request.is_dir,
            request.mime_type.clone(),
            request.mime_magic_checked,
            request.icon_size,
        );
        request.is_some_and(|request| queue.queue(request))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::sync::Arc;

    use crate::ui::icons::FileIconCache;

    fn request(name: &str) -> FileIconResolveRequest {
        FileIconCache::default()
            .resolve_request_for(
                Path::new(name),
                false,
                Some(Arc::from("text/plain")),
                true,
                48.0,
            )
            .unwrap()
    }

    #[test]
    fn queue_deduplicates_queued_and_pending_requests() {
        let mut queue = FileIconResolveQueue::default();
        let request = request("alpha.txt");

        assert!(queue.queue(request.clone()));
        assert!(!queue.queue(request.clone()));

        let batch = queue.start_next_batch().unwrap();
        assert_eq!(batch, vec![request.clone()]);
        assert!(!queue.queue(request.clone()));

        queue.finish_batch(batch);
        assert!(queue.queue(request));
    }

    #[test]
    fn pending_batch_blocks_next_batch_until_finished() {
        let mut queue = FileIconResolveQueue::default();
        let alpha = request("alpha.txt");
        let beta = request("beta.md");

        assert!(queue.queue(alpha.clone()));
        let batch = queue.start_next_batch().unwrap();
        assert_eq!(batch, vec![alpha]);

        assert!(queue.queue(beta.clone()));
        assert!(queue.start_next_batch().is_none());

        queue.finish_batch(batch);
        assert_eq!(queue.start_next_batch().unwrap(), vec![beta]);
    }

    #[test]
    fn contains_tracks_queued_and_pending_requests() {
        let mut queue = FileIconResolveQueue::default();
        let request = request("alpha.txt");

        assert!(!queue.contains(&request));
        assert!(queue.queue(request.clone()));
        assert!(queue.contains(&request));
        let batch = queue.start_next_batch().unwrap();
        assert!(queue.contains(&request));
        queue.finish_batch(batch);
        assert!(!queue.contains(&request));
    }

    #[test]
    fn batch_size_is_bounded() {
        let mut queue = FileIconResolveQueue::default();
        for index in 0..(FILE_ICON_RESOLVE_BATCH_SIZE + 3) {
            assert!(queue.queue(request(&format!("file-{index}.ext{index}"))));
        }

        let batch = queue.start_next_batch().unwrap();

        assert_eq!(batch.len(), FILE_ICON_RESOLVE_BATCH_SIZE);
    }
}
