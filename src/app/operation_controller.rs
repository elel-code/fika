use crate::app::state::{AppState, FileOperationRequest};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperationQueuePosition {
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperationQueueSnapshot {
    pub(crate) id: u64,
    pub(crate) queued_len: usize,
    pub(crate) active: bool,
    pub(crate) pending_conflict: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperationCancelSummary {
    pub(crate) queued_cancelled: usize,
    pub(crate) active_cancelled: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FileOperationController {
    id: u64,
    cancel: Arc<AtomicBool>,
}

impl FileOperationController {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            id,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl AppState {
    pub(crate) fn queue_file_operation(
        &mut self,
        mut request: FileOperationRequest,
        position: OperationQueuePosition,
    ) -> OperationQueueSnapshot {
        let id = self.next_operation_id;
        self.next_operation_id += 1;
        request.id = id;
        match position {
            OperationQueuePosition::Front => self.operation_queue.push_front(request),
            OperationQueuePosition::Back => self.operation_queue.push_back(request),
        }

        OperationQueueSnapshot {
            id,
            queued_len: self.operation_queue.len(),
            active: self.active_operation.is_some(),
            pending_conflict: self.pending_transfer_conflict.is_some(),
        }
    }

    pub(crate) fn can_start_file_operation(&self) -> bool {
        self.active_operation.is_none() && self.pending_transfer_conflict.is_none()
    }

    pub(crate) fn begin_file_operation(&mut self, id: u64) -> Arc<AtomicBool> {
        let controller = FileOperationController::new(id);
        let cancel = controller.cancel_handle();
        self.active_operation = Some(controller.id());
        self.active_operation_cancel = Some(controller.cancel_handle());
        cancel
    }

    pub(crate) fn active_operation_id(&self) -> Option<u64> {
        self.active_operation
    }

    pub(crate) fn finish_file_operation(&mut self, id: u64) -> bool {
        if self.active_operation_id() == Some(id) {
            self.active_operation = None;
            self.active_operation_cancel = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_file_operations(&mut self) -> OperationCancelSummary {
        let queued_cancelled = self.operation_queue.len();
        self.operation_queue.clear();
        let active_cancelled = self.active_operation_cancel.is_some();
        if let Some(cancel) = &self.active_operation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        OperationCancelSummary {
            queued_cancelled,
            active_cancelled,
        }
    }
}

pub(crate) fn operation_queued_status(snapshot: OperationQueueSnapshot) -> String {
    format!(
        "Queued operation #{} ({} pending)",
        snapshot.id, snapshot.queued_len
    )
}

pub(crate) fn operation_cancel_status(summary: OperationCancelSummary) -> String {
    if summary.queued_cancelled == 0 && !summary.active_cancelled {
        "No queued operations to cancel".to_string()
    } else if summary.active_cancelled {
        format!(
            "Cancelling active operation; removed {} queued operation(s)",
            summary.queued_cancelled
        )
    } else {
        format!("Cancelled {} queued operation(s)", summary.queued_cancelled)
    }
}

pub(crate) fn operation_started_status(operation: &str, source: &Path) -> String {
    format!(
        "{} {}...",
        operation_label(operation),
        operation_item_label(source)
    )
}

pub(crate) fn operation_progress_status(
    operation: &str,
    source: &Path,
    bytes_done: u64,
    bytes_total: u64,
) -> String {
    let label = operation_item_label(source);
    if bytes_total == 0 {
        format!("{} {label}...", operation_label(operation))
    } else {
        let percent = (bytes_done.saturating_mul(100) / bytes_total.max(1)).min(100);
        format!(
            "{} {label}: {percent}% ({}/{})",
            operation_label(operation),
            format_bytes(bytes_done),
            format_bytes(bytes_total)
        )
    }
}

pub(crate) fn operation_complete_status(operation: &str, destination: &Path) -> String {
    format!(
        "{} complete: {}",
        operation_finished_label(operation),
        destination.display()
    )
}

pub(crate) fn operation_failed_status(operation: &str, error: &str) -> String {
    format!("{} failed: {error}", operation_finished_label(operation))
}

pub(crate) fn operation_label(operation: &str) -> &'static str {
    match operation {
        "move" => "Moving",
        "copy" => "Copying",
        "link" => "Linking",
        _ => "Processing",
    }
}

pub(crate) fn operation_finished_label(operation: &str) -> &'static str {
    match operation {
        "move" => "Move",
        "copy" => "Copy",
        "link" => "Link",
        "create-folder" => "Create Folder",
        "create-file" => "Create File",
        "rename" => "Rename",
        "trash" => "Move to Trash",
        _ => "Operation",
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn operation_item_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("item")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use std::path::PathBuf;

    fn request(operation: &str) -> FileOperationRequest {
        FileOperationRequest {
            id: 0,
            operation: operation.to_string(),
            source: PathBuf::from("/tmp/source"),
            target_dir: PathBuf::from("/tmp/target"),
            conflict_policy: "ask".to_string(),
        }
    }

    #[test]
    fn queue_file_operation_assigns_ids_and_reports_start_state() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        let first = state.queue_file_operation(request("copy"), OperationQueuePosition::Back);
        assert_eq!(
            first,
            OperationQueueSnapshot {
                id: 1,
                queued_len: 1,
                active: false,
                pending_conflict: false,
            }
        );

        state.begin_file_operation(first.id);
        let second = state.queue_file_operation(request("move"), OperationQueuePosition::Front);

        assert_eq!(second.id, 2);
        assert_eq!(second.queued_len, 2);
        assert!(second.active);
    }

    #[test]
    fn active_operation_lifecycle_and_cancel_summary_are_controller_backed() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.queue_file_operation(request("copy"), OperationQueuePosition::Back);
        let cancel = state.begin_file_operation(7);

        assert_eq!(state.active_operation_id(), Some(7));
        let summary = state.cancel_file_operations();

        assert_eq!(
            summary,
            OperationCancelSummary {
                queued_cancelled: 1,
                active_cancelled: true,
            }
        );
        assert!(cancel.load(Ordering::Relaxed));

        assert!(state.finish_file_operation(7));
        assert_eq!(state.active_operation_id(), None);
        assert!(state.active_operation_cancel.is_none());
        assert!(!state.finish_file_operation(7));
    }

    #[test]
    fn operation_status_text_is_stable_and_testable() {
        assert_eq!(
            operation_queued_status(OperationQueueSnapshot {
                id: 3,
                queued_len: 2,
                active: false,
                pending_conflict: false,
            }),
            "Queued operation #3 (2 pending)"
        );
        assert_eq!(
            operation_cancel_status(OperationCancelSummary {
                queued_cancelled: 0,
                active_cancelled: false,
            }),
            "No queued operations to cancel"
        );
        assert_eq!(
            operation_cancel_status(OperationCancelSummary {
                queued_cancelled: 4,
                active_cancelled: true,
            }),
            "Cancelling active operation; removed 4 queued operation(s)"
        );
        assert_eq!(
            operation_started_status("copy", Path::new("/tmp/photo.jpg")),
            "Copying photo.jpg..."
        );
        assert_eq!(
            operation_progress_status("copy", Path::new("/tmp/photo.jpg"), 512, 2048),
            "Copying photo.jpg: 25% (512 B/2.0 KB)"
        );
        assert_eq!(
            operation_complete_status("move", Path::new("/tmp/done.txt")),
            "Move complete: /tmp/done.txt"
        );
        assert_eq!(
            operation_failed_status("link", "permission denied"),
            "Link failed: permission denied"
        );
    }
}
