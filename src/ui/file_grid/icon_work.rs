use std::collections::{HashSet, VecDeque};
use std::env;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::ui::icons::{FileIconCache, FileIconResolveCoverKey, FileIconResolveRequest};

use super::snapshot::RawFileGridSnapshot;

pub(crate) const DOLPHIN_VISIBLE_ICON_SYNC_BUDGET: Duration = Duration::from_millis(200);
pub(crate) const FILE_ICON_RESOLVE_BATCH_SIZE: usize = 128;

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
    covered: HashSet<FileIconResolveCoverKey>,
    pending: bool,
}

impl FileIconResolveQueue {
    pub(crate) fn queue(&mut self, request: FileIconResolveRequest) -> bool {
        if !self.seen.insert(request.clone()) {
            return false;
        }
        let cover_key = request.cover_key();
        if !self.covered.insert(cover_key) {
            self.seen.remove(&request);
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
            self.covered.remove(&request.cover_key());
            self.seen.remove(&request);
        }
    }

    pub(crate) fn contains(&self, request: &FileIconResolveRequest) -> bool {
        self.seen.contains(request)
    }

    pub(crate) fn contains_covered(&self, request: &FileIconResolveRequest) -> bool {
        self.contains(request) || self.covered.contains(&request.cover_key())
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
        let Some(queued_request) = file_icons.resolve_request_for(
            request.path,
            request.is_dir,
            request.mime_type.clone(),
            request.mime_magic_checked,
            request.icon_size,
        ) else {
            stats.cached += 1;
            return true;
        };
        if queue.contains_covered(&queued_request) {
            stats.queued += 1;
            return true;
        }
        let resolve_started = debug_icon_sync_enabled().then(Instant::now);
        let changed = file_icons.resolve_now_for(
            request.path,
            request.is_dir,
            request.mime_type.clone(),
            request.mime_magic_checked,
            request.icon_size,
        );
        if let Some(resolve_started) = resolve_started
            && changed
        {
            eprintln!(
                "[fika icon-sync-resolve] path={} is_dir={} mime={} magic_checked={} size={} total={}us",
                request.path.display(),
                request.is_dir,
                request.mime_type.as_deref().unwrap_or("<none>"),
                request.mime_magic_checked,
                request.icon_size,
                resolve_started.elapsed().as_micros(),
            );
        }
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

pub(crate) fn queue_file_icon_resolve_work_for_raw_grid_sizes(
    file_icons: &FileIconCache,
    queue: &mut FileIconResolveQueue,
    raw_file_grid: &RawFileGridSnapshot,
    file_icon_sizes: &[f32],
) -> bool {
    let mut queued = false;
    for file_icon_size in file_icon_sizes {
        raw_file_grid.for_each_visible_file_icon_resolve_candidate(*file_icon_size, |request| {
            queued |= queue_file_icon_resolve_request(file_icons, queue, request);
            true
        });
    }
    for file_icon_size in file_icon_sizes {
        queued |= raw_file_grid.queue_file_icon_resolve_candidates(*file_icon_size, |request| {
            queue_file_icon_resolve_request(file_icons, queue, request)
        });
    }
    queued
}

fn queue_file_icon_resolve_request(
    file_icons: &FileIconCache,
    queue: &mut FileIconResolveQueue,
    request: super::snapshot::FileGridIconRequest<'_>,
) -> bool {
    let request = file_icons.resolve_request_for(
        request.path,
        request.is_dir,
        request.mime_type.clone(),
        request.mime_magic_checked,
        request.icon_size,
    );
    request.is_some_and(|request| queue.queue(request))
}

fn debug_icon_sync_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env::var("FIKA_DEBUG_ICON_SYNC").is_ok_and(|value| env_flag_is_truthy(&value))
    })
}

fn env_flag_is_truthy(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    !normalized.is_empty() && normalized != "0" && normalized != "false" && normalized != "no"
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::sync::Arc;

    use crate::ui::icons::FileIconCache;

    fn request(name: &str) -> FileIconResolveRequest {
        request_with_mime(name, "text/plain")
    }

    fn request_with_mime(name: &str, mime: &str) -> FileIconResolveRequest {
        FileIconCache::default()
            .resolve_request_for(Path::new(name), false, Some(Arc::from(mime)), true, 48.0)
            .unwrap()
    }

    #[test]
    fn queue_deduplicates_queued_and_pending_requests() {
        let mut queue = FileIconResolveQueue::default();
        let request = request_with_mime("alpha.txt", "text/alpha");

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
        let alpha = request_with_mime("alpha.txt", "text/alpha");
        let beta = request_with_mime("beta.md", "text/beta");

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
    fn contains_covered_tracks_equivalent_mime_requests() {
        let mut queue = FileIconResolveQueue::default();
        let alpha = request("alpha.conf");
        let beta = request("beta.txt");

        assert!(queue.queue(alpha.clone()));
        assert!(!queue.contains(&beta));
        assert!(queue.contains_covered(&beta));
        assert!(!queue.queue(beta.clone()));

        let batch = queue.start_next_batch().unwrap();
        assert_eq!(batch, vec![alpha]);
        queue.finish_batch(batch);

        assert!(!queue.contains_covered(&beta));
        assert!(queue.queue(beta));
    }

    #[test]
    fn batch_size_is_bounded() {
        let mut queue = FileIconResolveQueue::default();
        for index in 0..(FILE_ICON_RESOLVE_BATCH_SIZE + 3) {
            assert!(queue.queue(request_with_mime(
                &format!("file-{index}.ext{index}"),
                &format!("application/x-test-{index}"),
            )));
        }

        let batch = queue.start_next_batch().unwrap();

        assert_eq!(batch.len(), FILE_ICON_RESOLVE_BATCH_SIZE);
    }
}
