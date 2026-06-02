use crate::app::state::{AppState, FileOperationRequest};
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
}
