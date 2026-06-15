use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use fika_core::OperationController;

#[derive(Clone, Debug)]
pub(crate) struct StatusBarSnapshot {
    pub(crate) message: String,
    pub(crate) item_summary: String,
    pub(crate) free_space: Option<SpaceInfoSnapshot>,
    pub(crate) zoom_level: i32,
    pub(crate) zoom_icon_size: f32,
    pub(crate) zoom_min: i32,
    pub(crate) zoom_max: i32,
    pub(crate) loading_pending: bool,
    pub(crate) operation_progress: Option<OperationProgressSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SpaceInfoSnapshot {
    pub(crate) free_label: String,
    pub(crate) detail_label: String,
    pub(crate) used_percent: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationProgressSnapshot {
    pub(crate) label: String,
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: u64,
    pub(crate) percent: Option<u8>,
    pub(crate) cancellable: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SpaceInfoCache {
    path: Option<PathBuf>,
    snapshot: Option<SpaceInfoSnapshot>,
    request_in_flight: bool,
    last_requested: Option<Instant>,
}

impl SpaceInfoCache {
    const RETRY_AFTER: Duration = Duration::from_secs(30);

    pub(crate) fn snapshot_for(&self, path: &Path) -> Option<SpaceInfoSnapshot> {
        (self.path.as_deref() == Some(path))
            .then(|| self.snapshot.clone())
            .flatten()
    }

    pub(crate) fn should_request(&self, path: &Path, now: Instant) -> bool {
        if self.request_in_flight && self.path.as_deref() == Some(path) {
            return false;
        }
        if self.path.as_deref() != Some(path) {
            return true;
        }
        if self.snapshot.is_some() {
            return false;
        }
        self.last_requested
            .is_none_or(|last_requested| now.duration_since(last_requested) >= Self::RETRY_AFTER)
    }

    pub(crate) fn start_request(&mut self, path: PathBuf, now: Instant) {
        self.path = Some(path);
        self.snapshot = None;
        self.request_in_flight = true;
        self.last_requested = Some(now);
    }

    pub(crate) fn finish_request(
        &mut self,
        path: &Path,
        snapshot: Option<SpaceInfoSnapshot>,
    ) -> bool {
        if self.path.as_deref() != Some(path) {
            return false;
        }
        self.request_in_flight = false;
        self.snapshot = snapshot;
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StatusSummaryCacheKey {
    pub(crate) model_generation: u64,
    pub(crate) model_len: usize,
    pub(crate) filter_revision: u64,
    pub(crate) visible_len: usize,
    pub(crate) selection_count: usize,
    pub(crate) selection_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StatusSummaryCacheEntry {
    pub(crate) key: StatusSummaryCacheKey,
    pub(crate) summary: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct OperationProgressHandle {
    pub(crate) label: String,
    pub(crate) controller: OperationController,
    pub(crate) cancellable: bool,
    pub(crate) started_at: Instant,
}

impl OperationProgressHandle {
    #[allow(dead_code)]
    pub(crate) fn snapshot(&self, now: Instant) -> Option<OperationProgressSnapshot> {
        if !progress_delay_elapsed(self.started_at, now) {
            return None;
        }
        let progress = self.controller.progress();
        Some(OperationProgressSnapshot {
            label: self.label.clone(),
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
            percent: progress_percent(progress.bytes_done, progress.bytes_total),
            cancellable: self.cancellable,
        })
    }
}

pub(crate) const PROGRESS_DISPLAY_DELAY: Duration = Duration::from_millis(500);

pub(crate) fn progress_percent(bytes_done: u64, bytes_total: u64) -> Option<u8> {
    if bytes_total == 0 {
        return None;
    }
    Some(((bytes_done.saturating_mul(100) + (bytes_total / 2)) / bytes_total).min(100) as u8)
}

pub(crate) fn progress_delay_elapsed(started_at: Instant, now: Instant) -> bool {
    now.duration_since(started_at) >= PROGRESS_DISPLAY_DELAY
}

pub(crate) fn filesystem_space_info(path: PathBuf) -> Option<SpaceInfoSnapshot> {
    let output = Command::new("df")
        .arg("-B1")
        .arg("--output=size,avail")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_df_space_output(std::str::from_utf8(&output.stdout).ok()?)
}

pub(crate) fn parse_df_space_output(output: &str) -> Option<SpaceInfoSnapshot> {
    let values = output.lines().skip(1).find_map(|line| {
        let mut parts = line.split_whitespace();
        let total = parts.next()?.parse::<u64>().ok()?;
        let available = parts.next()?.parse::<u64>().ok()?;
        Some((total, available))
    })?;
    space_info_snapshot(values.0, values.1)
}

pub(crate) fn space_info_snapshot(total: u64, available: u64) -> Option<SpaceInfoSnapshot> {
    if total == 0 {
        return None;
    }
    let available = available.min(total);
    let used = total.saturating_sub(available);
    let used_percent = ((used.saturating_mul(100) + (total / 2)) / total).min(100) as u8;
    Some(SpaceInfoSnapshot {
        free_label: format!("{} free", fika_core::format_size(available)),
        detail_label: format!(
            "{} free out of {} ({}% used)",
            fika_core::format_size(available),
            fika_core::format_size(total),
            used_percent
        ),
        used_percent,
    })
}
