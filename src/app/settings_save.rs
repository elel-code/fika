use crate::config::settings::{AppSettings, save_settings};
use slint::{Timer, TimerMode};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;

const SETTINGS_SAVE_COALESCE: Duration = Duration::from_millis(120);

static SETTINGS_SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static SETTINGS_SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) struct SettingsSaveScheduler {
    timer: Timer,
    pending: Rc<RefCell<Option<(u64, AppSettings)>>>,
}

impl SettingsSaveScheduler {
    pub(crate) fn new(async_handle: tokio::runtime::Handle) -> Self {
        let timer = Timer::default();
        let pending = Rc::new(RefCell::new(None));
        let timer_pending = Rc::clone(&pending);

        timer.start(TimerMode::SingleShot, SETTINGS_SAVE_COALESCE, move || {
            let Some((generation, settings)) = timer_pending.borrow_mut().take() else {
                return;
            };
            spawn_settings_save(async_handle.clone(), generation, settings);
        });
        timer.stop();

        Self { timer, pending }
    }

    pub(crate) fn schedule(&self, settings: AppSettings) {
        let generation = SETTINGS_SAVE_GENERATION.fetch_add(1, AtomicOrdering::AcqRel) + 1;
        *self.pending.borrow_mut() = Some((generation, settings));
        self.timer.restart();
    }

    pub(crate) fn save_now(&self, settings: AppSettings) {
        self.timer.stop();
        self.pending.borrow_mut().take();
        save_settings_latest(settings);
    }
}

pub(crate) fn save_settings_latest(settings: AppSettings) {
    let generation = SETTINGS_SAVE_GENERATION.fetch_add(1, AtomicOrdering::AcqRel) + 1;
    save_settings_if_latest(generation, settings);
}

fn spawn_settings_save(
    async_handle: tokio::runtime::Handle,
    generation: u64,
    settings: AppSettings,
) {
    async_handle.spawn(async move {
        let _ = tokio::task::spawn_blocking(move || save_settings_if_latest(generation, settings))
            .await;
    });
}

fn save_settings_if_latest(generation: u64, settings: AppSettings) {
    let lock = SETTINGS_SAVE_LOCK.get_or_init(|| Mutex::new(()));
    let Ok(_guard) = lock.lock() else {
        return;
    };
    if SETTINGS_SAVE_GENERATION.load(AtomicOrdering::Acquire) != generation {
        return;
    }
    save_settings(&settings);
}
