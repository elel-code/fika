use std::path::PathBuf;

use fika_core::{FileTransferMode, TransferTaskResult};

use crate::wgpu_tasks::ShellTaskId;

#[derive(Clone, Debug)]
pub(crate) struct ShellPasteResult {
    pub(crate) mode: FileTransferMode,
    pub(crate) success_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) clear_clipboard: bool,
    pub(crate) privileged: bool,
    pub(crate) administrator_available: bool,
    pub(crate) first_error: Option<String>,
}

impl ShellPasteResult {
    pub(crate) fn from_transfer(execution: &ShellTransferExecution) -> Self {
        Self {
            mode: execution.result.mode,
            success_count: execution.result.success_count,
            failure_count: execution.result.failure_count,
            clear_clipboard: execution.result.clear_clipboard,
            privileged: execution.privileged,
            administrator_available: execution.administrator_available,
            first_error: execution.first_error.clone(),
        }
    }

    pub(crate) fn changed(&self) -> bool {
        self.success_count > 0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellTransferExecution {
    pub(crate) result: TransferTaskResult,
    pub(crate) privileged: bool,
    pub(crate) administrator_available: bool,
    pub(crate) first_error: Option<String>,
    pub(crate) cancelled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellAsyncTransferSource {
    Paste,
    Drop,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellAsyncTransferCompletion {
    pub(crate) task_id: ShellTaskId,
    pub(crate) source: ShellAsyncTransferSource,
    pub(crate) target_dir: PathBuf,
    pub(crate) transfer: ShellTransferExecution,
}

#[derive(Clone, Debug)]
pub(crate) enum ShellAsyncTaskResult {
    Transfer(ShellAsyncTransferCompletion),
}
