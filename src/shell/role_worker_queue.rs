use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::Receiver;

use fika_core::ThumbnailRequestPriority;

pub(crate) trait PriorityWorkerRequest {
    type Key: Clone + Eq + Hash;

    fn key(&self) -> &Self::Key;
    fn priority(&self) -> ThumbnailRequestPriority;
}

pub(crate) struct PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    visible: VecDeque<R>,
    deferred: VecDeque<R>,
    queued: HashMap<R::Key, ThumbnailRequestPriority>,
}

impl<R> Default for PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    fn default() -> Self {
        Self {
            visible: VecDeque::new(),
            deferred: VecDeque::new(),
            queued: HashMap::new(),
        }
    }
}

impl<R> PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    pub(crate) fn push(&mut self, request: R) {
        let key = request.key().clone();
        match self.queued.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => {}
            Some(ThumbnailRequestPriority::Deferred)
                if request.priority() == ThumbnailRequestPriority::Visible =>
            {
                self.deferred.retain(|queued| queued.key() != &key);
                self.queued.insert(key, ThumbnailRequestPriority::Visible);
                self.visible.push_back(request);
            }
            Some(ThumbnailRequestPriority::Deferred) => {}
            None => {
                let priority = request.priority();
                self.queued.insert(key, priority);
                match priority {
                    ThumbnailRequestPriority::Visible => self.visible.push_back(request),
                    ThumbnailRequestPriority::Deferred => self.deferred.push_back(request),
                }
            }
        }
    }

    pub(crate) fn next_request(&mut self, request_rx: &Receiver<R>) -> Option<R> {
        loop {
            while let Ok(request) = request_rx.try_recv() {
                self.push(request);
            }

            if let Some(request) = self.pop_ready() {
                return Some(request);
            }

            match request_rx.recv() {
                Ok(request) => self.push(request),
                Err(_) => return None,
            }
        }
    }

    pub(crate) fn pop_ready(&mut self) -> Option<R> {
        let request = self
            .visible
            .pop_front()
            .or_else(|| self.deferred.pop_front())?;
        self.queued.remove(request.key());
        Some(request)
    }
}
