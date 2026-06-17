use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use crate::ui::icons::FileIconResolveRequest;

pub(crate) const DOLPHIN_VISIBLE_ICON_SYNC_BUDGET: Duration = Duration::from_millis(200);
pub(crate) const FILE_ICON_RESOLVE_BATCH_SIZE: usize = 64;

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
    fn batch_size_is_bounded() {
        let mut queue = FileIconResolveQueue::default();
        for index in 0..(FILE_ICON_RESOLVE_BATCH_SIZE + 3) {
            assert!(queue.queue(request(&format!("file-{index}.ext{index}"))));
        }

        let batch = queue.start_next_batch().unwrap();

        assert_eq!(batch.len(), FILE_ICON_RESOLVE_BATCH_SIZE);
    }
}
