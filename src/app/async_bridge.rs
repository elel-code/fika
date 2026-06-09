use crate::AppWindow;
use crate::app::events::AsyncEvent;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;

#[derive(Debug, Default)]
pub(crate) struct DirectoryReadTracker {
    generation: u64,
    latest_request: u64,
}

impl DirectoryReadTracker {
    pub(crate) fn begin_request(&mut self, generation: u64) -> Option<u64> {
        if generation < self.generation {
            return None;
        }
        if generation > self.generation {
            self.generation = generation;
            self.latest_request = 0;
        }
        self.latest_request += 1;
        Some(self.latest_request)
    }

    pub(crate) fn is_current(&self, generation: u64, request: u64) -> bool {
        self.generation == generation && self.latest_request == request
    }
}

#[derive(Clone)]
pub(crate) struct AsyncBridge {
    pub(crate) handle: tokio::runtime::Handle,
    pub(crate) tx: mpsc::Sender<AsyncEvent>,
    pub(crate) ui_weak: slint::Weak<AppWindow>,
    pub(crate) directory_watchers: Rc<RefCell<HashMap<u64, notify::RecommendedWatcher>>>,
    pub(crate) directory_read_trackers: Rc<RefCell<HashMap<u64, Arc<Mutex<DirectoryReadTracker>>>>>,
    pub(crate) device_watch_debounce: Arc<AtomicU64>,
}

pub(crate) fn build_async_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .thread_name("fika-async")
        .enable_all()
        .build()
        .expect("failed to initialize async runtime")
}

pub(crate) fn send_async_event(
    async_tx: mpsc::Sender<AsyncEvent>,
    notify_ui: slint::Weak<AppWindow>,
    event: AsyncEvent,
) {
    let _ = async_tx.send(event);
    let _ = notify_ui.upgrade_in_event_loop(|ui| ui.invoke_async_results_ready());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_read_tracker_rejects_older_generation_without_invalidating_current_request() {
        let mut tracker = DirectoryReadTracker::default();

        let first = tracker.begin_request(1).expect("first load should start");
        assert!(tracker.is_current(1, first));

        let current = tracker
            .begin_request(2)
            .expect("newer load generation should start");
        assert!(tracker.is_current(2, current));

        assert!(
            tracker.begin_request(1).is_none(),
            "old watcher callbacks must not invalidate newer directory loads"
        );
        assert!(tracker.is_current(2, current));
        assert!(!tracker.is_current(1, first));
    }
}
