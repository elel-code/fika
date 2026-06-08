use std::env;
use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static PERF_ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn enabled() -> bool {
    *PERF_ENABLED.get_or_init(|| {
        env::var("FIKA_PERF_ITEM_VIEW")
            .ok()
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
    })
}

pub(crate) fn log(args: fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("[fika perf] {args}");
    }
}

pub(crate) struct PerfTimer {
    start: Option<Instant>,
}

impl PerfTimer {
    pub(crate) fn start() -> Self {
        Self {
            start: enabled().then(Instant::now),
        }
    }

    pub(crate) fn elapsed(&self) -> Option<Duration> {
        self.start.map(|start| start.elapsed())
    }

    pub(crate) fn elapsed_ms(&self) -> f64 {
        self.elapsed()
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_view_perf_is_disabled_by_default_for_tests() {
        if env::var_os("FIKA_PERF_ITEM_VIEW").is_none() {
            assert!(!enabled());
        }
    }

    #[test]
    fn disabled_perf_timer_has_zero_elapsed_ms() {
        let timer = PerfTimer::start();
        if !enabled() {
            assert_eq!(timer.elapsed_ms(), 0.0);
        }
    }
}
