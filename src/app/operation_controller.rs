use crate::app::events::FileOperationProgress;
use crate::app::state::{AppState, FileOperationRequest, FileUndo, TransferConflict};
use crate::fs::{file_ops, privilege};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperationQueuePosition {
    Front,
    Back,
}

#[derive(Clone, Debug)]
pub(crate) enum OperationStartDecision {
    Idle,
    NeedsConflict(TransferConflict),
    Skipped { status: String },
    Started(OperationStartSummary),
}

#[derive(Clone, Debug)]
pub(crate) struct OperationStartSummary {
    pub(crate) request: FileOperationRequest,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) pane_ids: Vec<u64>,
    pub(crate) status: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperationQueueSnapshot {
    pub(crate) id: u64,
    pub(crate) queued_len: usize,
    pub(crate) active: bool,
    pub(crate) pending_conflict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationCancelSummary {
    pub(crate) queued_cancelled: usize,
    pub(crate) active_cancelled: bool,
    pub(crate) pane_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OperationResultDisposition {
    Completed {
        destination: PathBuf,
        overwritten_backup: Option<PathBuf>,
        status: String,
    },
    RequestPrivilege {
        error: String,
    },
    Failed {
        status: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationCompletionSummary {
    pub(crate) disposition: OperationResultDisposition,
    pub(crate) refresh_current_dir: bool,
    pub(crate) refresh_pane_ids: Vec<u64>,
    pub(crate) remaining: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationProgressUpdate {
    pub(crate) status: String,
    pub(crate) pane_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(crate) enum FileUndoStartDecision {
    Empty { status: String },
    Started(FileUndoStartSummary),
}

#[derive(Clone, Debug)]
pub(crate) struct FileUndoStartSummary {
    pub(crate) undo: FileUndo,
    pub(crate) pane_ids: Vec<u64>,
    pub(crate) status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileUndoCompletionSummary {
    pub(crate) affected_dirs: Vec<PathBuf>,
    pub(crate) status: String,
    pub(crate) cleanup_backup: Option<PathBuf>,
    pub(crate) undo_available_changed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TransferConflictQueueUpdate {
    pub(crate) applied_remaining: usize,
    pub(crate) clipboard_changed: bool,
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
    pub(crate) fn replace_file_undo(&mut self, undo: Option<FileUndo>) -> Option<PathBuf> {
        let old_undo = std::mem::replace(&mut self.last_undo, undo);
        file_undo_backup_path(old_undo)
    }

    pub(crate) fn take_file_undo_start(&mut self) -> FileUndoStartDecision {
        let Some(undo) = self.last_undo.take() else {
            return FileUndoStartDecision::Empty {
                status: "Nothing to undo".to_string(),
            };
        };

        let affected_dirs = file_undo_affected_dirs(&undo);
        let pane_ids =
            affected_directory_pane_ids(self, affected_dirs.iter().map(|dir| dir.as_path()));
        let status = file_undo_started_status(&undo.operation);
        FileUndoStartDecision::Started(FileUndoStartSummary {
            undo,
            pane_ids,
            status,
        })
    }

    pub(crate) fn complete_file_undo(
        &mut self,
        undo: FileUndo,
        result: Result<String, String>,
    ) -> FileUndoCompletionSummary {
        let affected_dirs = file_undo_affected_dirs(&undo);
        match result {
            Ok(message) => FileUndoCompletionSummary {
                affected_dirs,
                status: file_undo_complete_status(&message),
                cleanup_backup: None,
                undo_available_changed: false,
            },
            Err(error) => {
                let restored = self.last_undo.is_none();
                let cleanup_backup = if restored {
                    self.last_undo = Some(undo);
                    None
                } else {
                    file_undo_backup_path(Some(undo))
                };
                FileUndoCompletionSummary {
                    affected_dirs,
                    status: file_undo_failed_status(&error, restored),
                    cleanup_backup,
                    undo_available_changed: restored,
                }
            }
        }
    }

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

    pub(crate) fn next_file_operation_start(&mut self) -> OperationStartDecision {
        if !self.can_start_file_operation() {
            return OperationStartDecision::Idle;
        }

        let Some(mut request) = self.operation_queue.pop_front() else {
            return OperationStartDecision::Idle;
        };

        match transfer_request_conflict_destination(&request) {
            Ok(Some(destination)) if request.conflict_policy == "ask" => {
                let conflict = TransferConflict {
                    operation: request.operation,
                    source: request.source,
                    target_dir: request.target_dir,
                    destination,
                };
                self.pending_transfer_conflict = Some(conflict.clone());
                OperationStartDecision::NeedsConflict(conflict)
            }
            Ok(_) => {
                if request.conflict_policy == "ask" {
                    request.conflict_policy = "keep-both".to_string();
                }
                let pane_ids = affected_directory_pane_ids(
                    self,
                    [Some(request.target_dir.as_path()), request.source.parent()]
                        .into_iter()
                        .flatten(),
                );
                let cancel = self.begin_file_operation_for_panes(request.id, pane_ids.clone());
                let status = operation_started_status(request.operation.as_str(), &request.source);
                OperationStartDecision::Started(OperationStartSummary {
                    request,
                    cancel,
                    pane_ids,
                    status,
                })
            }
            Err(err) => OperationStartDecision::Skipped {
                status: operation_skipped_status(&err),
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn begin_file_operation(&mut self, id: u64) -> Arc<AtomicBool> {
        self.begin_file_operation_for_panes(id, Vec::new())
    }

    pub(crate) fn begin_file_operation_for_panes(
        &mut self,
        id: u64,
        pane_ids: Vec<u64>,
    ) -> Arc<AtomicBool> {
        let controller = FileOperationController::new(id);
        let cancel = controller.cancel_handle();
        self.active_operation = Some(controller.id());
        self.active_operation_cancel = Some(controller.cancel_handle());
        self.active_operation_progress_key = None;
        self.active_operation_pane_ids = pane_ids;
        cancel
    }

    pub(crate) fn active_operation_id(&self) -> Option<u64> {
        self.active_operation
    }

    pub(crate) fn finish_file_operation(&mut self, id: u64) -> bool {
        if self.active_operation_id() == Some(id) {
            self.active_operation = None;
            self.active_operation_cancel = None;
            self.active_operation_progress_key = None;
            self.active_operation_pane_ids.clear();
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_file_operations(&mut self) -> OperationCancelSummary {
        let queued_cancelled = self.operation_queue.len();
        self.operation_queue.clear();
        let active_cancelled = self.active_operation_cancel.is_some();
        let pane_ids = if active_cancelled {
            self.active_operation_pane_ids.clone()
        } else {
            Vec::new()
        };
        if let Some(cancel) = &self.active_operation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        OperationCancelSummary {
            queued_cancelled,
            active_cancelled,
            pane_ids,
        }
    }

    pub(crate) fn complete_file_operation(
        &mut self,
        id: u64,
        operation: &str,
        source: &Path,
        target_dir: &Path,
        result: Result<file_ops::TransferOutcome, String>,
        can_request_privilege: bool,
    ) -> Option<OperationCompletionSummary> {
        if !self.finish_file_operation(id) {
            return None;
        }

        self.remove_directory_cache(target_dir);
        let source_parent = source.parent();
        if let Some(source_parent) = source_parent {
            self.remove_directory_cache(source_parent);
        }

        let refresh_pane_ids = affected_directory_pane_ids(
            self,
            [Some(target_dir), source_parent].into_iter().flatten(),
        );
        let refresh_current_dir = refresh_pane_ids
            .iter()
            .any(|id| *id == self.panes.focused().id);
        Some(OperationCompletionSummary {
            disposition: operation_result_disposition(operation, result, can_request_privilege),
            refresh_current_dir,
            refresh_pane_ids,
            remaining: self.operation_queue.len(),
        })
    }

    pub(crate) fn file_operation_progress_update(
        &mut self,
        progress: &FileOperationProgress,
    ) -> Option<OperationProgressUpdate> {
        if self.active_operation_id() != Some(progress.id) {
            return None;
        }

        let progress_key = (
            progress.id,
            operation_progress_bucket(progress.bytes_done, progress.bytes_total),
        );
        if self.active_operation_progress_key == Some(progress_key) {
            return None;
        }
        self.active_operation_progress_key = Some(progress_key);

        Some(OperationProgressUpdate {
            status: operation_progress_status(
                &progress.operation,
                &progress.source,
                progress.bytes_done,
                progress.bytes_total,
            ),
            pane_ids: self.active_operation_pane_ids.clone(),
        })
    }

    pub(crate) fn clear_accepted_cut_source(&mut self, operation: &str, source: &Path) -> bool {
        if operation != "move" || !self.clipboard_cut {
            return false;
        }

        let previous_len = self.clipboard_paths.len();
        self.clipboard_paths.retain(|path| path != source);
        if self.clipboard_paths.len() == previous_len {
            return false;
        }

        if self.clipboard_paths.is_empty() {
            self.clipboard_cut = false;
        }
        true
    }

    pub(crate) fn apply_transfer_conflict_decision_to_remaining(
        &mut self,
        decision: &str,
    ) -> TransferConflictQueueUpdate {
        match decision {
            "skip" => TransferConflictQueueUpdate {
                applied_remaining: apply_conflict_decision_to_queue(
                    &mut self.operation_queue,
                    decision,
                ),
                clipboard_changed: false,
            },
            "keep-both" | "overwrite" => {
                let clipboard_changed = self.clear_cut_sources_for_remaining_conflicts(decision);
                TransferConflictQueueUpdate {
                    applied_remaining: apply_conflict_decision_to_queue(
                        &mut self.operation_queue,
                        decision,
                    ),
                    clipboard_changed,
                }
            }
            _ => TransferConflictQueueUpdate::default(),
        }
    }

    pub(crate) fn apply_transfer_rename_to_remaining_conflicts(
        &mut self,
        reserved_destinations: &mut Vec<PathBuf>,
    ) -> TransferConflictQueueUpdate {
        let mut applied_remaining = 0;
        let mut renamed_cut_sources = Vec::new();
        for request in self.operation_queue.iter_mut() {
            if request.conflict_policy != "ask" {
                continue;
            }
            let Some(destination) = transfer_request_conflict_destination(request)
                .ok()
                .flatten()
            else {
                continue;
            };
            let Some(unique_name) =
                default_transfer_rename_policy(&destination, reserved_destinations)
            else {
                continue;
            };
            request.conflict_policy = unique_name;
            applied_remaining += 1;
            if request.operation == "move" {
                renamed_cut_sources.push(request.source.clone());
            }
        }

        let mut clipboard_changed = false;
        if self.clipboard_cut {
            for source in renamed_cut_sources {
                clipboard_changed |= self.clear_accepted_cut_source("move", &source);
            }
        }
        TransferConflictQueueUpdate {
            applied_remaining,
            clipboard_changed,
        }
    }

    fn clear_cut_sources_for_remaining_conflicts(&mut self, decision: &str) -> bool {
        if !matches!(decision, "keep-both" | "overwrite") || !self.clipboard_cut {
            return false;
        }

        let accepted_sources = accepted_remaining_conflict_sources(&self.operation_queue, decision);
        let mut changed = false;
        for source in accepted_sources {
            changed |= self.clear_accepted_cut_source("move", &source);
        }
        changed
    }
}

fn file_undo_affected_dirs(undo: &FileUndo) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_unique_parent(&mut dirs, &undo.original_source);
    push_unique_parent(&mut dirs, &undo.destination);
    for item in &undo.items {
        push_unique_parent(&mut dirs, &item.original_source);
        push_unique_parent(&mut dirs, &item.destination);
    }
    dirs
}

fn push_unique_parent(paths: &mut Vec<PathBuf>, path: &Path) {
    if let Some(parent) = path.parent() {
        push_unique_path(paths, parent.to_path_buf());
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

pub(crate) fn cleanup_file_undo_backup(backup: Option<PathBuf>) {
    if let Some(backup) = backup {
        let _ = file_ops::cleanup_overwrite_backup(&backup);
    }
}

fn file_undo_backup_path(undo: Option<FileUndo>) -> Option<PathBuf> {
    undo.and_then(|undo| undo.overwritten_backup)
}

pub(crate) fn affected_directory_pane_ids<'a>(
    state: &AppState,
    affected_dirs: impl IntoIterator<Item = &'a Path>,
) -> Vec<u64> {
    let affected_dirs = affected_dirs.into_iter().collect::<Vec<_>>();
    let mut pane_ids = Vec::new();
    let focused_id = state.panes.focused().id;
    if affected_dirs.contains(&state.panes.focused().current_dir.as_path()) {
        pane_ids.push(focused_id);
    }
    for (_slot, pane) in state.panes.iter().filter(|(_, p)| p.id != focused_id) {
        if affected_dirs.contains(&pane.current_dir.as_path()) {
            pane_ids.push(pane.id);
        }
    }
    pane_ids
}

pub(crate) fn operation_queued_status(snapshot: OperationQueueSnapshot) -> String {
    format!(
        "Queued operation #{} ({} pending)",
        snapshot.id, snapshot.queued_len
    )
}

pub(crate) fn operation_cancel_status(summary: &OperationCancelSummary) -> String {
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

pub(crate) fn file_undo_started_status(operation: &str) -> String {
    format!("Undoing {}...", operation_finished_label(operation))
}

pub(crate) fn file_undo_complete_status(message: &str) -> String {
    format!("Undo complete: {message}")
}

pub(crate) fn file_undo_failed_status(error: &str, restored: bool) -> String {
    if restored {
        format!("Undo failed: {error}; Undo can be retried")
    } else {
        format!("Undo failed: {error}; newer Undo is available")
    }
}

pub(crate) fn operation_skipped_status(error: &str) -> String {
    format!("Skipped transfer: {error}")
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

fn operation_progress_bucket(bytes_done: u64, bytes_total: u64) -> u64 {
    if bytes_total == 0 {
        u64::MAX
    } else {
        (bytes_done.saturating_mul(100) / bytes_total.max(1)).min(100)
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

pub(crate) fn operation_result_disposition(
    operation: &str,
    result: Result<file_ops::TransferOutcome, String>,
    can_request_privilege: bool,
) -> OperationResultDisposition {
    match result {
        Ok(outcome) => OperationResultDisposition::Completed {
            status: operation_complete_status(operation, &outcome.destination),
            destination: outcome.destination,
            overwritten_backup: outcome.overwritten_backup,
        },
        Err(error) if can_request_privilege && privilege::is_permission_error(&error) => {
            OperationResultDisposition::RequestPrivilege { error }
        }
        Err(error) => OperationResultDisposition::Failed {
            status: operation_failed_status(operation, &error),
        },
    }
}

pub(crate) fn operation_final_status(
    status: Option<String>,
    requested_privilege: bool,
    remaining: usize,
) -> Option<String> {
    match (status, requested_privilege, remaining) {
        (Some(status), _, 0) => Some(status),
        (Some(status), _, remaining) => Some(format!("{status}; {remaining} queued")),
        (None, true, remaining) if remaining > 0 => Some(format!(
            "Administrator privileges required; {remaining} queued"
        )),
        (None, _, _) => None,
    }
}

pub(crate) fn transfer_conflict_skip_status(
    destination: &Path,
    skipped_remaining: usize,
) -> String {
    if skipped_remaining > 0 {
        format!(
            "Skipped {} and {skipped_remaining} remaining conflict(s)",
            destination.display()
        )
    } else {
        format!("Skipped {}", destination.display())
    }
}

pub(crate) fn transfer_conflict_apply_remaining_status(
    decision: &str,
    applied: usize,
) -> Option<String> {
    (applied > 0).then(|| {
        format!(
            "Applied {} to {applied} remaining conflict(s)",
            transfer_conflict_decision_label(decision)
        )
    })
}

pub(crate) fn transfer_conflict_decision_label(decision: &str) -> &'static str {
    match decision {
        "keep-both" => "Keep Both",
        "overwrite" => "Overwrite",
        "rename" => "Rename",
        "skip" => "Skip",
        _ => "decision",
    }
}

pub(crate) fn transfer_target_rejection(source: &Path, target_dir: &Path) -> Option<&'static str> {
    file_ops::transfer_target_relation(source, target_dir).map(|relation| match relation {
        file_ops::TransferTargetRelation::Same => "Cannot drop an item onto itself",
        file_ops::TransferTargetRelation::Descendant => "Cannot drop a folder into itself",
    })
}

pub(crate) fn transfer_request_conflict_destination(
    request: &FileOperationRequest,
) -> Result<Option<PathBuf>, String> {
    if !file_ops::path_exists(&request.source) {
        return Err("source no longer exists".to_string());
    }
    if !request.target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    if let Some(reason) = transfer_target_rejection(&request.source, &request.target_dir) {
        return Err(reason.to_string());
    }
    let destination = file_ops::base_destination(&request.source, &request.target_dir)?;
    Ok(file_ops::path_exists(&destination).then_some(destination))
}

fn apply_conflict_decision_to_queue(
    queue: &mut VecDeque<FileOperationRequest>,
    decision: &str,
) -> usize {
    let mut applied = 0;
    match decision {
        "skip" => {
            queue.retain(|request| {
                if request.conflict_policy == "ask"
                    && transfer_request_conflict_destination(request)
                        .ok()
                        .flatten()
                        .is_some()
                {
                    applied += 1;
                    false
                } else {
                    true
                }
            });
        }
        "keep-both" | "overwrite" => {
            for request in queue.iter_mut() {
                if request.conflict_policy != "ask" {
                    continue;
                }
                let Some(destination) = transfer_request_conflict_destination(request)
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                if decision == "overwrite" && destination == request.source {
                    continue;
                }
                request.conflict_policy = decision.to_string();
                applied += 1;
            }
        }
        _ => {}
    }
    applied
}

fn accepted_remaining_conflict_sources(
    queue: &VecDeque<FileOperationRequest>,
    decision: &str,
) -> Vec<PathBuf> {
    queue
        .iter()
        .filter(|request| request.operation == "move" && request.conflict_policy == "ask")
        .filter_map(|request| {
            let destination = transfer_request_conflict_destination(request)
                .ok()
                .flatten()?;
            if decision == "overwrite" && destination == request.source {
                return None;
            }
            Some(request.source.clone())
        })
        .collect()
}

fn default_transfer_rename_policy(
    destination: &Path,
    reserved_destinations: &mut Vec<PathBuf>,
) -> Option<String> {
    let name = default_transfer_rename_suggestion_with_reserved(destination, reserved_destinations);
    let target_dir = destination.parent()?;
    let reserved = target_dir.join(&name);
    reserved_destinations.push(reserved);
    Some(format!("rename:{name}"))
}

pub(crate) fn default_transfer_rename_suggestion(destination: &Path) -> String {
    default_transfer_rename_suggestion_with_reserved(destination, &[])
}

fn default_transfer_rename_suggestion_with_reserved(
    destination: &Path,
    reserved_destinations: &[PathBuf],
) -> String {
    let Some(file_name) = destination.file_name() else {
        return transfer_path_label(destination);
    };
    let Some(parent) = destination.parent() else {
        return file_name.to_string_lossy().to_string();
    };
    let file_name_path = Path::new(file_name);
    let stem = file_name_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("item");
    let extension = file_name_path
        .extension()
        .and_then(|extension| extension.to_str());

    for index in 1.. {
        let suffix = if index == 1 {
            "copy".to_string()
        } else {
            format!("copy {index}")
        };
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} {suffix}.{extension}"),
            _ => format!("{stem} {suffix}"),
        };
        let candidate = parent.join(&candidate_name);
        if !file_ops::path_exists(&candidate) && !reserved_destinations.contains(&candidate) {
            return candidate_name;
        }
    }

    unreachable!("unbounded rename suggestion search should always return")
}

fn transfer_path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string_lossy().to_string())
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
    use crate::app::pane::PreparedDirectoryEntries;
    use crate::app::state::{AppState, FileUndo, FileUndoItem};
    use std::collections::VecDeque;
    use std::fs;
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

    fn undo(operation: &str, original_source: &str, destination: &str) -> FileUndo {
        FileUndo {
            operation: operation.to_string(),
            original_source: PathBuf::from(original_source),
            destination: PathBuf::from(destination),
            overwritten_backup: None,
            items: Vec::new(),
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
    fn complete_file_undo_restores_failed_undo_when_no_newer_undo_exists() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let undo = undo("copy", "/tmp/source.txt", "/tmp/target/source.txt");

        assert_eq!(
            state.complete_file_undo(undo.clone(), Err("permission denied".to_string())),
            FileUndoCompletionSummary {
                affected_dirs: vec![PathBuf::from("/tmp"), PathBuf::from("/tmp/target")],
                status: "Undo failed: permission denied; Undo can be retried".to_string(),
                cleanup_backup: None,
                undo_available_changed: true,
            }
        );

        let restored = state.last_undo.as_ref().unwrap();
        assert_eq!(restored.operation, undo.operation);
        assert_eq!(restored.original_source, undo.original_source);
        assert_eq!(restored.destination, undo.destination);
    }

    #[test]
    fn complete_file_undo_does_not_replace_newer_undo() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let newer = undo("move", "/tmp/new-source.txt", "/tmp/new-target.txt");
        state.last_undo = Some(newer.clone());
        let mut failed = undo("copy", "/tmp/source.txt", "/tmp/target/source.txt");
        failed.overwritten_backup = Some(PathBuf::from("/tmp/fika-backup"));

        assert_eq!(
            state.complete_file_undo(failed, Err("target changed".to_string())),
            FileUndoCompletionSummary {
                affected_dirs: vec![PathBuf::from("/tmp"), PathBuf::from("/tmp/target")],
                status: "Undo failed: target changed; newer Undo is available".to_string(),
                cleanup_backup: Some(PathBuf::from("/tmp/fika-backup")),
                undo_available_changed: false,
            }
        );

        let retained = state.last_undo.as_ref().unwrap();
        assert_eq!(retained.operation, newer.operation);
        assert_eq!(retained.original_source, newer.original_source);
        assert_eq!(retained.destination, newer.destination);
    }

    #[test]
    fn complete_file_undo_reports_success_without_restoring_undo() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        assert_eq!(
            state.complete_file_undo(
                undo("copy", "/tmp/source.txt", "/tmp/target/source.txt"),
                Ok("removed /tmp/target/source.txt".to_string())
            ),
            FileUndoCompletionSummary {
                affected_dirs: vec![PathBuf::from("/tmp"), PathBuf::from("/tmp/target")],
                status: "Undo complete: removed /tmp/target/source.txt".to_string(),
                cleanup_backup: None,
                undo_available_changed: false,
            }
        );
        assert!(state.last_undo.is_none());
    }

    #[test]
    fn replace_file_undo_returns_previous_backup_for_cleanup() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut previous = undo("copy", "/tmp/source.txt", "/tmp/target/source.txt");
        previous.overwritten_backup = Some(PathBuf::from("/tmp/old-backup"));
        state.last_undo = Some(previous);

        assert_eq!(
            state.replace_file_undo(Some(undo("move", "/tmp/a", "/tmp/b"))),
            Some(PathBuf::from("/tmp/old-backup"))
        );
        assert_eq!(
            state.last_undo.as_ref().map(|undo| undo.operation.as_str()),
            Some("move")
        );
    }

    #[test]
    fn take_file_undo_start_reports_empty_status_without_mutating_state() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        match state.take_file_undo_start() {
            FileUndoStartDecision::Empty { status } => {
                assert_eq!(status, "Nothing to undo");
            }
            FileUndoStartDecision::Started(_) => panic!("empty undo state should not start"),
        }

        assert!(state.last_undo.is_none());
    }

    #[test]
    fn take_file_undo_start_consumes_undo_and_routes_affected_panes() {
        let mut state = AppState::new(PathBuf::from("/tmp/source"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/target")));
        let active_id = state.panes.focused().id;
        let inactive_id = state.panes.pane_for_slot(1).expect("inactive pane").id;
        state.last_undo = Some(undo("copy", "/tmp/source/item.txt", "/tmp/target/item.txt"));

        let summary = match state.take_file_undo_start() {
            FileUndoStartDecision::Started(summary) => summary,
            FileUndoStartDecision::Empty { status } => {
                panic!("expected undo start, got status {status}")
            }
        };

        assert!(state.last_undo.is_none());
        assert_eq!(summary.undo.operation, "copy");
        assert_eq!(summary.pane_ids, vec![active_id, inactive_id]);
        assert_eq!(summary.status, "Undoing Copy...");
    }

    #[test]
    fn file_undo_affected_dirs_are_deduplicated_in_operation_order() {
        let mut undo = undo("copy", "/tmp/source/one.txt", "/tmp/target/one.txt");
        undo.items = vec![
            FileUndoItem {
                original_source: PathBuf::from("/tmp/source/two.txt"),
                destination: PathBuf::from("/tmp/target/two.txt"),
            },
            FileUndoItem {
                original_source: PathBuf::from("/tmp/other/three.txt"),
                destination: PathBuf::from("/tmp/target/three.txt"),
            },
        ];

        assert_eq!(
            file_undo_affected_dirs(&undo),
            vec![
                PathBuf::from("/tmp/source"),
                PathBuf::from("/tmp/target"),
                PathBuf::from("/tmp/other"),
            ]
        );
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
                pane_ids: Vec::new(),
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
            operation_cancel_status(&OperationCancelSummary {
                queued_cancelled: 0,
                active_cancelled: false,
                pane_ids: Vec::new(),
            }),
            "No queued operations to cancel"
        );
        assert_eq!(
            operation_cancel_status(&OperationCancelSummary {
                queued_cancelled: 4,
                active_cancelled: true,
                pane_ids: Vec::new(),
            }),
            "Cancelling active operation; removed 4 queued operation(s)"
        );
        assert_eq!(
            operation_started_status("copy", Path::new("/tmp/photo.jpg")),
            "Copying photo.jpg..."
        );
        assert_eq!(
            operation_skipped_status("source no longer exists"),
            "Skipped transfer: source no longer exists"
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

    #[test]
    fn transfer_conflict_status_text_is_stable_and_testable() {
        assert_eq!(
            transfer_conflict_skip_status(Path::new("/tmp/target/note.txt"), 0),
            "Skipped /tmp/target/note.txt"
        );
        assert_eq!(
            transfer_conflict_skip_status(Path::new("/tmp/target/note.txt"), 2),
            "Skipped /tmp/target/note.txt and 2 remaining conflict(s)"
        );
        assert_eq!(
            transfer_conflict_apply_remaining_status("keep-both", 3),
            Some("Applied Keep Both to 3 remaining conflict(s)".to_string())
        );
        assert_eq!(
            transfer_conflict_apply_remaining_status("overwrite", 1),
            Some("Applied Overwrite to 1 remaining conflict(s)".to_string())
        );
        assert_eq!(
            transfer_conflict_apply_remaining_status("rename", 2),
            Some("Applied Rename to 2 remaining conflict(s)".to_string())
        );
        assert_eq!(transfer_conflict_apply_remaining_status("rename", 0), None);
        assert_eq!(transfer_conflict_decision_label("skip"), "Skip");
    }

    #[test]
    fn transfer_target_validation_reports_self_and_descendant_rejections() {
        let temp = test_dir("transfer-target-validation");
        let source = temp.join("source");
        let child = source.join("child");
        let target = temp.join("target");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&target).unwrap();

        assert_eq!(
            transfer_target_rejection(&source, &source),
            Some("Cannot drop an item onto itself")
        );
        assert_eq!(
            transfer_target_rejection(&source, &child),
            Some("Cannot drop a folder into itself")
        );
        assert_eq!(transfer_target_rejection(&source, &target), None);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn transfer_conflict_destination_validation_is_controller_owned() {
        let temp = test_dir("transfer-conflict-destination");
        let source = temp.join("source").join("note.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();

        let request = FileOperationRequest {
            id: 1,
            operation: "copy".to_string(),
            source: source.clone(),
            target_dir: target_dir.clone(),
            conflict_policy: "ask".to_string(),
        };
        assert_eq!(
            transfer_request_conflict_destination(&request).unwrap(),
            None
        );

        let occupied = target_dir.join("note.txt");
        fs::write(&occupied, "old").unwrap();
        assert_eq!(
            transfer_request_conflict_destination(&request).unwrap(),
            Some(occupied)
        );

        let missing = FileOperationRequest {
            source: temp.join("missing.txt"),
            ..request
        };
        assert_eq!(
            transfer_request_conflict_destination(&missing),
            Err("source no longer exists".to_string())
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn transfer_conflict_destination_detects_broken_symlink_destination() {
        let temp = test_dir("queued-broken-symlink-conflict");
        let source = temp.join("source").join("note.txt");
        let target_dir = temp.join("target");
        let occupied = target_dir.join("note.txt");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();
        std::os::unix::fs::symlink("missing-target.txt", &occupied).unwrap();

        let request = transfer_request("copy", &source, &target_dir, "ask");

        assert!(!occupied.exists());
        assert_eq!(
            transfer_request_conflict_destination(&request).unwrap(),
            Some(occupied)
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn conflict_dialog_default_rename_uses_keep_both_style_name() {
        let temp = test_dir("rename-suggestion");
        fs::create_dir_all(&temp).unwrap();
        let destination = temp.join("note.txt");
        fs::write(&destination, "old").unwrap();

        assert_eq!(
            default_transfer_rename_suggestion(&destination),
            "note copy.txt"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn apply_skip_to_remaining_conflicts_removes_only_conflicted_ask_requests() {
        let temp = test_dir("apply-skip");
        let conflicted_source = temp.join("sources").join("conflicted.txt");
        let free_source = temp.join("sources").join("free.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(conflicted_source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&conflicted_source, "new").unwrap();
        fs::write(&free_source, "new").unwrap();
        fs::write(target_dir.join("conflicted.txt"), "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.operation_queue = VecDeque::from([
            transfer_request("copy", &conflicted_source, &target_dir, "ask"),
            transfer_request("copy", &free_source, &target_dir, "ask"),
        ]);

        assert_eq!(
            state.apply_transfer_conflict_decision_to_remaining("skip"),
            TransferConflictQueueUpdate {
                applied_remaining: 1,
                clipboard_changed: false,
            }
        );
        assert_eq!(state.operation_queue.len(), 1);
        assert_eq!(state.operation_queue[0].source, free_source);
        assert_eq!(state.operation_queue[0].conflict_policy, "ask");

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn apply_keep_both_to_remaining_conflicts_updates_only_conflicted_ask_requests() {
        let temp = test_dir("apply-keep-both");
        let conflicted_source = temp.join("sources").join("conflicted.txt");
        let free_source = temp.join("sources").join("free.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(conflicted_source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&conflicted_source, "new").unwrap();
        fs::write(&free_source, "new").unwrap();
        fs::write(target_dir.join("conflicted.txt"), "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.operation_queue = VecDeque::from([
            transfer_request("copy", &conflicted_source, &target_dir, "ask"),
            transfer_request("copy", &free_source, &target_dir, "ask"),
        ]);

        assert_eq!(
            state.apply_transfer_conflict_decision_to_remaining("keep-both"),
            TransferConflictQueueUpdate {
                applied_remaining: 1,
                clipboard_changed: false,
            }
        );
        assert_eq!(state.operation_queue[0].conflict_policy, "keep-both");
        assert_eq!(state.operation_queue[1].conflict_policy, "ask");

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn apply_rename_to_remaining_conflicts_uses_unique_names() {
        let temp = test_dir("apply-rename");
        let current_conflict_target = temp.join("target").join("conflicted.txt");
        let first_source = temp.join("sources").join("one").join("conflicted.txt");
        let second_source = temp.join("sources").join("two").join("conflicted.txt");
        let free_source = temp.join("sources").join("free.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(first_source.parent().unwrap()).unwrap();
        fs::create_dir_all(second_source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&first_source, "new").unwrap();
        fs::write(&second_source, "new").unwrap();
        fs::write(&free_source, "new").unwrap();
        fs::write(&current_conflict_target, "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.operation_queue = VecDeque::from([
            transfer_request("copy", &first_source, &target_dir, "ask"),
            transfer_request("copy", &second_source, &target_dir, "ask"),
            transfer_request("copy", &free_source, &target_dir, "ask"),
        ]);
        let mut reserved = vec![target_dir.join("custom.txt")];

        assert_eq!(
            state.apply_transfer_rename_to_remaining_conflicts(&mut reserved),
            TransferConflictQueueUpdate {
                applied_remaining: 2,
                clipboard_changed: false,
            }
        );
        assert_eq!(
            state.operation_queue[0].conflict_policy,
            "rename:conflicted copy.txt"
        );
        assert_eq!(
            state.operation_queue[1].conflict_policy,
            "rename:conflicted copy 2.txt"
        );
        assert_eq!(state.operation_queue[2].conflict_policy, "ask");
        assert_eq!(
            reserved,
            vec![
                target_dir.join("custom.txt"),
                target_dir.join("conflicted copy.txt"),
                target_dir.join("conflicted copy 2.txt"),
            ]
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn apply_rename_to_remaining_clears_accepted_move_cut_sources() {
        let temp = test_dir("apply-rename-cut");
        let conflicted_move = temp.join("sources").join("move.txt");
        let conflicted_copy = temp.join("sources").join("copy.txt");
        let free_move = temp.join("sources").join("free.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(conflicted_move.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&conflicted_move, "move").unwrap();
        fs::write(&conflicted_copy, "copy").unwrap();
        fs::write(&free_move, "move").unwrap();
        fs::write(target_dir.join("move.txt"), "old").unwrap();
        fs::write(target_dir.join("copy.txt"), "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![
            conflicted_move.clone(),
            free_move.clone(),
            conflicted_copy.clone(),
        ];
        state.operation_queue = VecDeque::from([
            transfer_request("move", &conflicted_move, &target_dir, "ask"),
            transfer_request("move", &free_move, &target_dir, "ask"),
            transfer_request("copy", &conflicted_copy, &target_dir, "ask"),
        ]);

        assert_eq!(
            state.apply_transfer_rename_to_remaining_conflicts(&mut Vec::new()),
            TransferConflictQueueUpdate {
                applied_remaining: 2,
                clipboard_changed: true,
            }
        );
        assert_eq!(state.clipboard_paths, vec![free_move, conflicted_copy]);
        assert!(state.clipboard_cut);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn rename_suggestion_respects_reserved_batch_destinations() {
        let temp = test_dir("rename-reserved");
        fs::create_dir_all(&temp).unwrap();
        let destination = temp.join("note.txt");
        fs::write(&destination, "old").unwrap();

        assert_eq!(
            default_transfer_rename_suggestion_with_reserved(
                &destination,
                &[temp.join("note copy.txt"), temp.join("note copy 2.txt")]
            ),
            "note copy 3.txt"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn accepted_cut_source_removes_matching_path_only() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];

        assert!(state.clear_accepted_cut_source("move", Path::new("/tmp/a")));

        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/b")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn accepted_cut_source_clears_cut_when_last_path_is_removed() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![PathBuf::from("/tmp/a")];

        assert!(state.clear_accepted_cut_source("move", Path::new("/tmp/a")));

        assert!(state.clipboard_paths.is_empty());
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn accepted_cut_source_ignores_copy_and_non_cut_clipboards() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![PathBuf::from("/tmp/a")];

        assert!(!state.clear_accepted_cut_source("copy", Path::new("/tmp/a")));
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/a")]);
        assert!(state.clipboard_cut);

        state.clipboard_cut = false;
        assert!(!state.clear_accepted_cut_source("move", Path::new("/tmp/a")));
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/a")]);
    }

    #[test]
    fn apply_to_remaining_acceptance_clears_only_conflicted_move_cut_sources() {
        let temp = test_dir("clear-remaining-cut");
        let conflicted_move = temp.join("sources").join("conflicted-move.txt");
        let free_move = temp.join("sources").join("free-move.txt");
        let conflicted_copy = temp.join("sources").join("conflicted-copy.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(conflicted_move.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&conflicted_move, "move").unwrap();
        fs::write(&free_move, "move").unwrap();
        fs::write(&conflicted_copy, "copy").unwrap();
        fs::write(target_dir.join("conflicted-move.txt"), "old").unwrap();
        fs::write(target_dir.join("conflicted-copy.txt"), "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![
            conflicted_move.clone(),
            free_move.clone(),
            conflicted_copy.clone(),
        ];
        state.operation_queue = VecDeque::from([
            transfer_request("move", &conflicted_move, &target_dir, "ask"),
            transfer_request("move", &free_move, &target_dir, "ask"),
            transfer_request("copy", &conflicted_copy, &target_dir, "ask"),
        ]);

        assert_eq!(
            state.apply_transfer_conflict_decision_to_remaining("keep-both"),
            TransferConflictQueueUpdate {
                applied_remaining: 2,
                clipboard_changed: true,
            }
        );

        assert_eq!(state.clipboard_paths, vec![free_move, conflicted_copy]);
        assert!(state.clipboard_cut);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn skip_remaining_conflicts_does_not_clear_cut_sources() {
        let temp = test_dir("skip-remaining-cut");
        let source = temp.join("sources").join("conflicted.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();
        fs::write(target_dir.join("conflicted.txt"), "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![source.clone()];
        state.operation_queue =
            VecDeque::from([transfer_request("move", &source, &target_dir, "ask")]);

        assert_eq!(
            state.apply_transfer_conflict_decision_to_remaining("skip"),
            TransferConflictQueueUpdate {
                applied_remaining: 1,
                clipboard_changed: false,
            }
        );

        assert_eq!(state.clipboard_paths, vec![source]);
        assert!(state.clipboard_cut);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn next_file_operation_start_skips_invalid_requests_without_ui_side_effects() {
        let temp = test_dir("operation-start-skip");
        let target_dir = temp.join("target");
        let source = temp.join("source").join("ok.txt");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();

        let mut state = AppState::new(target_dir.clone(), Vec::new());
        state.queue_file_operation(
            FileOperationRequest {
                source: temp.join("missing.txt"),
                target_dir: target_dir.clone(),
                ..request("copy")
            },
            OperationQueuePosition::Back,
        );
        state.queue_file_operation(
            FileOperationRequest {
                source: source.clone(),
                target_dir: target_dir.clone(),
                ..request("copy")
            },
            OperationQueuePosition::Back,
        );

        match state.next_file_operation_start() {
            OperationStartDecision::Skipped { status } => {
                assert_eq!(status, "Skipped transfer: source no longer exists");
            }
            other => panic!("expected skipped transfer, got {other:?}"),
        }

        match state.next_file_operation_start() {
            OperationStartDecision::Started(start) => {
                assert_eq!(start.request.source, source);
                assert_eq!(start.request.conflict_policy, "keep-both");
                assert_eq!(start.pane_ids, vec![state.panes.focused().id]);
                assert_eq!(start.status, "Copying ok.txt...");
                assert!(state.active_operation_cancel.is_some());
                assert!(!start.cancel.load(Ordering::Relaxed));
            }
            other => panic!("expected started transfer, got {other:?}"),
        }

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn next_file_operation_start_records_pending_conflict_for_ui_application() {
        let temp = test_dir("operation-start-conflict");
        let source = temp.join("source").join("note.txt");
        let target_dir = temp.join("target");
        let destination = target_dir.join("note.txt");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();
        fs::write(&destination, "old").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.queue_file_operation(
            FileOperationRequest {
                source: source.clone(),
                target_dir: target_dir.clone(),
                ..request("copy")
            },
            OperationQueuePosition::Back,
        );

        match state.next_file_operation_start() {
            OperationStartDecision::NeedsConflict(conflict) => {
                assert_eq!(
                    conflict,
                    TransferConflict {
                        operation: "copy".to_string(),
                        source,
                        target_dir,
                        destination,
                    }
                );
                assert_eq!(state.pending_transfer_conflict, Some(conflict));
            }
            other => panic!("expected transfer conflict, got {other:?}"),
        }
        assert!(!state.can_start_file_operation());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn operation_progress_update_ignores_stale_progress_ids() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.begin_file_operation_for_panes(7, vec![3, 5]);

        let stale = FileOperationProgress {
            id: 6,
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/photo.jpg"),
            bytes_done: 512,
            bytes_total: 2048,
        };
        assert_eq!(state.file_operation_progress_update(&stale), None);

        let current = FileOperationProgress {
            id: 7,
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/photo.jpg"),
            bytes_done: 512,
            bytes_total: 2048,
        };
        assert_eq!(
            state.file_operation_progress_update(&current),
            Some(OperationProgressUpdate {
                status: "Copying photo.jpg: 25% (512 B/2.0 KB)".to_string(),
                pane_ids: vec![3, 5],
            })
        );
        assert_eq!(state.file_operation_progress_update(&current), None);

        let same_bucket = FileOperationProgress {
            id: 7,
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/photo.jpg"),
            bytes_done: 520,
            bytes_total: 2048,
        };
        assert_eq!(state.file_operation_progress_update(&same_bucket), None);

        let next_bucket = FileOperationProgress {
            id: 7,
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/photo.jpg"),
            bytes_done: 1024,
            bytes_total: 2048,
        };
        assert_eq!(
            state.file_operation_progress_update(&next_bucket),
            Some(OperationProgressUpdate {
                status: "Copying photo.jpg: 50% (1.0 KB/2.0 KB)".to_string(),
                pane_ids: vec![3, 5],
            })
        );
        assert!(state.finish_file_operation(7));
        assert_eq!(state.active_operation_progress_key, None);
        assert!(state.active_operation_pane_ids.is_empty());
    }

    #[test]
    fn cancel_file_operations_reports_active_operation_pane_ids() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.queue_file_operation(request("copy"), OperationQueuePosition::Back);
        let cancel = state.begin_file_operation_for_panes(7, vec![3, 5]);

        assert_eq!(
            state.cancel_file_operations(),
            OperationCancelSummary {
                queued_cancelled: 1,
                active_cancelled: true,
                pane_ids: vec![3, 5],
            }
        );
        assert!(cancel.load(Ordering::Relaxed));
        assert_eq!(state.active_operation_pane_ids, vec![3, 5]);
    }

    #[test]
    fn operation_progress_update_reports_unknown_total_once() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.begin_file_operation(7);

        let unknown = FileOperationProgress {
            id: 7,
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/photo.jpg"),
            bytes_done: 0,
            bytes_total: 0,
        };
        assert_eq!(
            state.file_operation_progress_update(&unknown),
            Some(OperationProgressUpdate {
                status: "Copying photo.jpg...".to_string(),
                pane_ids: Vec::new(),
            })
        );
        assert_eq!(state.file_operation_progress_update(&unknown), None);

        let known = FileOperationProgress {
            bytes_done: 0,
            bytes_total: 2048,
            ..unknown
        };
        assert_eq!(
            state.file_operation_progress_update(&known),
            Some(OperationProgressUpdate {
                status: "Copying photo.jpg: 0% (0 B/2.0 KB)".to_string(),
                pane_ids: Vec::new(),
            })
        );
    }

    #[test]
    fn operation_result_disposition_separates_completion_privilege_and_failure() {
        let completed = operation_result_disposition(
            "copy",
            Ok(file_ops::TransferOutcome {
                destination: PathBuf::from("/tmp/copied.txt"),
                overwritten_backup: Some(PathBuf::from("/tmp/backup")),
            }),
            true,
        );
        assert_eq!(
            completed,
            OperationResultDisposition::Completed {
                destination: PathBuf::from("/tmp/copied.txt"),
                overwritten_backup: Some(PathBuf::from("/tmp/backup")),
                status: "Copy complete: /tmp/copied.txt".to_string(),
            }
        );

        assert_eq!(
            operation_result_disposition("move", Err("Permission denied".to_string()), true),
            OperationResultDisposition::RequestPrivilege {
                error: "Permission denied".to_string(),
            }
        );

        assert_eq!(
            operation_result_disposition("move", Err("Permission denied".to_string()), false),
            OperationResultDisposition::Failed {
                status: "Move failed: Permission denied".to_string(),
            }
        );
    }

    #[test]
    fn complete_file_operation_summarizes_state_and_invalidates_caches() {
        let mut state = AppState::new(PathBuf::from("/tmp/target"), Vec::new());
        let target_dir = PathBuf::from("/tmp/target");
        let source = PathBuf::from("/tmp/source/item.txt");
        let source_parent = source.parent().unwrap().to_path_buf();
        state.insert_directory_cache(target_dir.clone(), PreparedDirectoryEntries::default());
        state.insert_directory_cache(source_parent.clone(), PreparedDirectoryEntries::default());
        state.queue_file_operation(request("move"), OperationQueuePosition::Back);
        state.begin_file_operation(7);

        let summary = state
            .complete_file_operation(
                7,
                "copy",
                &source,
                &target_dir,
                Ok(file_ops::TransferOutcome {
                    destination: target_dir.join("item.txt"),
                    overwritten_backup: None,
                }),
                false,
            )
            .unwrap();

        assert!(summary.refresh_current_dir);
        assert_eq!(summary.refresh_pane_ids, vec![state.panes.focused().id]);
        assert_eq!(summary.remaining, 1);
        assert_eq!(
            summary.disposition,
            OperationResultDisposition::Completed {
                destination: target_dir.join("item.txt"),
                overwritten_backup: None,
                status: "Copy complete: /tmp/target/item.txt".to_string(),
            }
        );
        assert_eq!(state.active_operation_id(), None);
        assert!(!state.directory_cache.contains_key(&target_dir));
        assert!(!state.directory_cache.contains_key(&source_parent));
    }

    #[test]
    fn complete_file_operation_ignores_stale_result_ids() {
        let mut state = AppState::new(PathBuf::from("/tmp/target"), Vec::new());
        let target_dir = PathBuf::from("/tmp/target");
        state.insert_directory_cache(target_dir.clone(), PreparedDirectoryEntries::default());
        state.begin_file_operation(7);

        assert_eq!(
            state.complete_file_operation(
                99,
                "copy",
                Path::new("/tmp/source/item.txt"),
                &target_dir,
                Err("late result".to_string()),
                false,
            ),
            None
        );

        assert_eq!(state.active_operation_id(), Some(7));
        assert!(state.directory_cache.contains_key(&target_dir));
    }

    #[test]
    fn complete_file_operation_marks_inactive_pane_for_refresh() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/right")));
        let inactive_id = state.panes.pane_for_slot(1).expect("inactive pane").id;
        state.begin_file_operation(7);

        let summary = state
            .complete_file_operation(
                7,
                "copy",
                Path::new("/tmp/source/item.txt"),
                Path::new("/tmp/right"),
                Ok(file_ops::TransferOutcome {
                    destination: PathBuf::from("/tmp/right/item.txt"),
                    overwritten_backup: None,
                }),
                false,
            )
            .unwrap();

        assert!(!summary.refresh_current_dir);
        assert_eq!(summary.refresh_pane_ids, vec![inactive_id]);
    }

    #[test]
    fn complete_file_operation_marks_all_affected_split_panes_for_refresh() {
        let mut state = AppState::new(PathBuf::from("/tmp/source"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/target")));
        let active_id = state.panes.focused().id;
        let inactive_id = state.panes.pane_for_slot(1).expect("inactive pane").id;
        state.begin_file_operation(7);

        let summary = state
            .complete_file_operation(
                7,
                "move",
                Path::new("/tmp/source/item.txt"),
                Path::new("/tmp/target"),
                Ok(file_ops::TransferOutcome {
                    destination: PathBuf::from("/tmp/target/item.txt"),
                    overwritten_backup: None,
                }),
                false,
            )
            .unwrap();

        assert!(summary.refresh_current_dir);
        assert_eq!(summary.refresh_pane_ids, vec![active_id, inactive_id]);
    }

    #[test]
    fn affected_directory_pane_ids_deduplicates_matching_split_panes() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/right")));
        let active_id = state.panes.focused().id;
        let inactive_id = state.panes.pane_for_slot(1).expect("inactive pane").id;

        let pane_ids = affected_directory_pane_ids(
            &state,
            [
                Path::new("/tmp/active"),
                Path::new("/tmp/right"),
                Path::new("/tmp/active"),
                Path::new("/tmp/other"),
            ],
        );

        assert_eq!(pane_ids, vec![active_id, inactive_id]);
    }

    #[test]
    fn operation_final_status_preserves_prompt_and_queue_semantics() {
        assert_eq!(
            operation_final_status(Some("Copy complete: /tmp/a".to_string()), false, 0),
            Some("Copy complete: /tmp/a".to_string())
        );
        assert_eq!(
            operation_final_status(Some("Copy complete: /tmp/a".to_string()), false, 2),
            Some("Copy complete: /tmp/a; 2 queued".to_string())
        );
        assert_eq!(
            operation_final_status(None, true, 3),
            Some("Administrator privileges required; 3 queued".to_string())
        );
        assert_eq!(operation_final_status(None, true, 0), None);
        assert_eq!(operation_final_status(None, false, 3), None);
    }

    fn transfer_request(
        operation: &str,
        source: &Path,
        target_dir: &Path,
        conflict_policy: &str,
    ) -> FileOperationRequest {
        FileOperationRequest {
            id: 1,
            operation: operation.to_string(),
            source: source.to_path_buf(),
            target_dir: target_dir.to_path_buf(),
            conflict_policy: conflict_policy.to_string(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "fika-operation-controller-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
