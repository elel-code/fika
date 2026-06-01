use crate::AppWindow;
use crate::app::events::AsyncEvent;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;

#[derive(Clone)]
pub(crate) struct AsyncBridge {
    pub(crate) handle: tokio::runtime::Handle,
    pub(crate) tx: mpsc::Sender<AsyncEvent>,
    pub(crate) ui_weak: slint::Weak<AppWindow>,
    pub(crate) directory_watcher: Rc<RefCell<Option<notify::RecommendedWatcher>>>,
    pub(crate) directory_watch_debounce: Arc<AtomicU64>,
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
