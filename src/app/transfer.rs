use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::file_clipboard::sync_clipboard_ui;
use crate::app::geometry::MainGridLayout;
use crate::app::operation_controller::{
    OperationQueuePosition, operation_cancel_status, operation_queued_status,
    operation_started_status, transfer_conflict_apply_remaining_status,
    transfer_conflict_skip_status, transfer_request_conflict_destination,
    transfer_target_rejection,
};
use crate::app::pane::PaneTarget;
use crate::app::selection::filtered_entry_at;
use crate::app::state::{AppState, FileOperationRequest, TransferConflict};
use crate::fs::{file_ops, privilege};
use crate::{
    AppWindow, AsyncEvent, FileEntry, FileOperationProgress, FileOperationResult, set_status,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(crate) fn prepare_place_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    target_index: i32,
    x: f32,
    y: f32,
) -> bool {
    let state = state.borrow();
    let Ok(target_index) = usize::try_from(target_index) else {
        return false;
    };
    let Some(target) = state.places.get(target_index) else {
        return false;
    };
    prepare_transfer_menu(
        ui,
        source,
        path_label(source).as_str(),
        target.path.as_str(),
        target.label.as_str(),
        x,
        y,
    )
}

pub(crate) fn prepare_entry_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    target_index: i32,
    x: f32,
    y: f32,
) -> bool {
    let state = state.borrow();
    let target = usize::try_from(target_index)
        .ok()
        .and_then(|target_index| filtered_entry_at(&state, target_index));

    if let Some(target) = target.filter(|target| target.is_dir && target.path.as_str() != source) {
        return prepare_transfer_menu(
            ui,
            source,
            path_label(source).as_str(),
            target.path.as_str(),
            target.name.as_str(),
            x,
            y,
        );
    }

    prepare_current_dir_transfer_with_state(ui, &state, source, path_label(source).as_str(), x, y)
}

pub(crate) fn prepare_current_dir_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let state = state.borrow();
    prepare_current_dir_transfer_with_state(ui, &state, source, source_label, x, y)
}

pub(crate) fn prepare_inactive_pane_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    x: f32,
    y: f32,
) -> bool {
    let state = state.borrow();
    prepare_current_dir_transfer_for_target_with_state(
        ui,
        &state,
        PaneTarget::Inactive,
        source,
        path_label(source).as_str(),
        x,
        y,
    )
}

pub(crate) fn prepare_main_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let state = state.borrow();
    if let Some(target) = entry_at_main_point(ui, &state, x, y)
        .filter(|target| target.is_dir && target.path.as_str() != source)
    {
        return prepare_transfer_menu(
            ui,
            source,
            source_label,
            target.path.as_str(),
            target.name.as_str(),
            x,
            y,
        );
    }

    prepare_current_dir_transfer_with_state(ui, &state, source, source_label, x, y)
}

pub(crate) fn main_drop_allowed(
    ui: &AppWindow,
    state: &AppState,
    x: f32,
    y: f32,
    source: &Path,
) -> bool {
    main_drop_rejection(ui, state, x, y, source).is_none()
}

pub(crate) fn inactive_pane_drop_allowed(state: &AppState, source: &Path) -> bool {
    pane_current_dir(state, PaneTarget::Inactive)
        .is_some_and(|target_dir| transfer_target_rejection(source, target_dir).is_none())
}

pub(crate) fn main_drop_rejection(
    ui: &AppWindow,
    state: &AppState,
    x: f32,
    y: f32,
    source: &Path,
) -> Option<&'static str> {
    let target_dir = main_drop_target_dir(ui, state, x, y, source);
    transfer_target_rejection(source, &target_dir)
}

pub(crate) fn place_drop_allowed(state: &AppState, source: &Path, target_index: i32) -> bool {
    let Ok(target_index) = usize::try_from(target_index) else {
        return false;
    };
    let Some(target) = state.places.get(target_index) else {
        return false;
    };
    transfer_target_rejection(source, Path::new(target.path.as_str())).is_none()
}

pub(crate) fn entry_at_main_point(
    ui: &AppWindow,
    state: &AppState,
    x: f32,
    y: f32,
) -> Option<FileEntry> {
    let layout = MainGridLayout::from_ui(ui);
    let index = layout.index_at_point(x, y)?;
    filtered_entry_at(state, index)
}

fn main_drop_target_dir(
    ui: &AppWindow,
    state: &AppState,
    x: f32,
    y: f32,
    source: &Path,
) -> PathBuf {
    entry_at_main_point(ui, state, x, y)
        .filter(|target| target.is_dir && Path::new(target.path.as_str()) != source)
        .map(|target| PathBuf::from(target.path.as_str()))
        .unwrap_or_else(|| focused_current_dir(state).to_path_buf())
}

fn prepare_current_dir_transfer_with_state(
    ui: &AppWindow,
    state: &AppState,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let current_dir = focused_current_dir(state);
    let target_path = current_dir.display().to_string();
    let target_label = display_location_name(current_dir);
    prepare_transfer_menu(
        ui,
        source,
        source_label,
        target_path.as_str(),
        target_label.as_str(),
        x,
        y,
    )
}

fn prepare_current_dir_transfer_for_target_with_state(
    ui: &AppWindow,
    state: &AppState,
    target: PaneTarget,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let Some(current_dir) = pane_current_dir(state, target) else {
        set_status(ui, "No split pane target is available");
        ui.set_transfer_source_path("".into());
        ui.set_transfer_target_path("".into());
        return false;
    };
    let target_path = current_dir.display().to_string();
    let target_label = display_location_name(current_dir);
    prepare_transfer_menu(
        ui,
        source,
        source_label,
        target_path.as_str(),
        target_label.as_str(),
        x,
        y,
    )
}

fn focused_current_dir(state: &AppState) -> &Path {
    pane_current_dir(state, PaneTarget::Focused)
        .unwrap_or_else(|| state.panes.active.current_dir.as_path())
}

fn pane_current_dir(state: &AppState, target: PaneTarget) -> Option<&Path> {
    state
        .panes
        .pane_for_target(target)
        .map(|pane| pane.current_dir.as_path())
}

fn prepare_transfer_menu(
    ui: &AppWindow,
    source_path: &str,
    source_label: &str,
    target_path: &str,
    target_label: &str,
    x: f32,
    y: f32,
) -> bool {
    if let Some(reason) = transfer_target_rejection(Path::new(source_path), Path::new(target_path))
    {
        set_status(ui, reason);
        ui.set_transfer_source_path("".into());
        ui.set_transfer_target_path("".into());
        return false;
    }

    ui.set_transfer_source_path(source_path.into());
    ui.set_transfer_source_label(source_label.into());
    ui.set_transfer_target_path(target_path.into());
    ui.set_transfer_target_label(target_label.into());
    ui.set_transfer_menu_x(x);
    ui.set_transfer_menu_y(y);
    true
}

pub(crate) fn target_is_source_or_descendant(source: &Path, target_dir: &Path) -> bool {
    file_ops::target_is_source_or_descendant(source, target_dir)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferStart {
    Rejected,
    Queued,
    NeedsDecision,
}

pub(crate) fn start_transfer_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    operation: &str,
    source: &str,
    target_dir: &str,
) -> TransferStart {
    let source = PathBuf::from(source);
    let target_dir = PathBuf::from(target_dir);
    if let Some(reason) = transfer_start_rejection(&source, &target_dir) {
        set_status(ui, reason);
        return TransferStart::Rejected;
    }

    let request = FileOperationRequest {
        id: 0,
        operation: operation.to_string(),
        source,
        target_dir,
        conflict_policy: "ask".to_string(),
    };
    let start = if transfer_request_conflict_destination(&request)
        .ok()
        .flatten()
        .is_some()
    {
        TransferStart::NeedsDecision
    } else {
        TransferStart::Queued
    };
    queue_transfer_operation(ui, state, bridge, request, QueuePosition::Back);
    start
}

fn transfer_start_rejection(source: &Path, target_dir: &Path) -> Option<&'static str> {
    if !file_ops::path_exists(source) {
        return Some("Source no longer exists");
    }
    if !target_dir.is_dir() {
        return Some("Target is not a folder");
    }
    transfer_target_rejection(source, target_dir)
}

pub(crate) fn clear_accepted_cut_source(
    state: &mut AppState,
    operation: &str,
    source: &Path,
) -> bool {
    if operation != "move" || !state.clipboard_cut {
        return false;
    }

    let previous_len = state.clipboard_paths.len();
    state.clipboard_paths.retain(|path| path != source);
    if state.clipboard_paths.len() == previous_len {
        return false;
    }

    if state.clipboard_paths.is_empty() {
        state.clipboard_cut = false;
    }
    true
}

pub(crate) fn resolve_transfer_conflict(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    decision: &str,
) {
    let (apply_to_remaining, decision) = decision
        .strip_prefix("all:")
        .map_or((false, decision), |decision| (true, decision));
    let conflict = state.borrow_mut().pending_transfer_conflict.take();
    ui.set_transfer_conflict_open(false);
    let Some(conflict) = conflict else {
        set_status(ui, "No transfer conflict is pending");
        return;
    };

    match decision {
        "skip" => {
            let skipped_remaining = if apply_to_remaining {
                apply_conflict_decision_to_queue(&mut state.borrow_mut().operation_queue, decision)
            } else {
                0
            };
            set_status(
                ui,
                &transfer_conflict_skip_status(&conflict.destination, skipped_remaining),
            );
            start_next_operation(ui, state, bridge);
        }
        "keep-both" | "overwrite" => {
            if decision == "overwrite" && conflict.destination == conflict.source {
                set_status(ui, "Cannot overwrite an item with itself");
                return;
            }
            let clipboard_changed = {
                let mut state_ref = state.borrow_mut();
                let mut clipboard_changed = clear_accepted_cut_source(
                    &mut state_ref,
                    conflict.operation.as_str(),
                    &conflict.source,
                );
                if apply_to_remaining {
                    clipboard_changed |=
                        clear_cut_sources_for_remaining_conflicts(&mut state_ref, decision);
                }
                clipboard_changed
            };
            queue_transfer_operation(
                ui,
                state,
                bridge,
                FileOperationRequest {
                    id: 0,
                    operation: conflict.operation,
                    source: conflict.source,
                    target_dir: conflict.target_dir,
                    conflict_policy: decision.to_string(),
                },
                QueuePosition::Front,
            );
            if clipboard_changed {
                sync_clipboard_ui(ui, state);
            }
            if apply_to_remaining {
                let applied = apply_conflict_decision_to_queue(
                    &mut state.borrow_mut().operation_queue,
                    decision,
                );
                if let Some(status) = transfer_conflict_apply_remaining_status(decision, applied) {
                    set_status(ui, &status);
                }
            }
        }
        _ if let Some(name) = decision.strip_prefix("rename:") => {
            let mut reserved_destinations =
                match file_ops::renamed_destination(&conflict.target_dir, name) {
                    Ok(destination) => vec![destination],
                    Err(err) => {
                        state.borrow_mut().pending_transfer_conflict = Some(conflict);
                        ui.set_transfer_conflict_open(true);
                        set_status(ui, &format!("Cannot rename transfer target: {err}"));
                        return;
                    }
                };
            let mut applied_remaining = 0;
            let clipboard_changed = {
                let mut state_ref = state.borrow_mut();
                let mut clipboard_changed = clear_accepted_cut_source(
                    &mut state_ref,
                    conflict.operation.as_str(),
                    &conflict.source,
                );
                if apply_to_remaining {
                    let (applied, changed) = apply_rename_to_remaining_conflicts(
                        &mut state_ref,
                        &mut reserved_destinations,
                    );
                    applied_remaining = applied;
                    clipboard_changed |= changed;
                }
                clipboard_changed
            };
            queue_transfer_operation(
                ui,
                state,
                bridge,
                FileOperationRequest {
                    id: 0,
                    operation: conflict.operation,
                    source: conflict.source,
                    target_dir: conflict.target_dir,
                    conflict_policy: decision.to_string(),
                },
                QueuePosition::Front,
            );
            if clipboard_changed {
                sync_clipboard_ui(ui, state);
            }
            if apply_to_remaining && applied_remaining > 0 {
                if let Some(status) =
                    transfer_conflict_apply_remaining_status("rename", applied_remaining)
                {
                    set_status(ui, &status);
                }
            }
        }
        _ => set_status(ui, "Unknown conflict decision"),
    }
}

fn clear_cut_sources_for_remaining_conflicts(state: &mut AppState, decision: &str) -> bool {
    if !matches!(decision, "keep-both" | "overwrite") || !state.clipboard_cut {
        return false;
    }

    let accepted_sources = accepted_remaining_conflict_sources(&state.operation_queue, decision);
    let mut changed = false;
    for source in accepted_sources {
        changed |= clear_accepted_cut_source(state, "move", &source);
    }
    changed
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

fn apply_rename_to_remaining_conflicts(
    state: &mut AppState,
    reserved_destinations: &mut Vec<PathBuf>,
) -> (usize, bool) {
    let mut applied = 0;
    let mut renamed_cut_sources = Vec::new();
    for request in state.operation_queue.iter_mut() {
        if request.conflict_policy != "ask" {
            continue;
        }
        let Some(destination) = transfer_request_conflict_destination(request)
            .ok()
            .flatten()
        else {
            continue;
        };
        let Some(unique_name) = default_rename_policy(&destination, reserved_destinations) else {
            continue;
        };
        request.conflict_policy = unique_name;
        applied += 1;
        if request.operation == "move" {
            renamed_cut_sources.push(request.source.clone());
        }
    }

    let mut clipboard_changed = false;
    if state.clipboard_cut {
        for source in renamed_cut_sources {
            clipboard_changed |= clear_accepted_cut_source(state, "move", &source);
        }
    }
    (applied, clipboard_changed)
}

fn default_rename_policy(
    destination: &Path,
    reserved_destinations: &mut Vec<PathBuf>,
) -> Option<String> {
    let name = default_rename_suggestion_with_reserved(destination, reserved_destinations);
    let target_dir = destination.parent()?;
    let reserved = target_dir.join(&name);
    reserved_destinations.push(reserved);
    Some(format!("rename:{name}"))
}

pub(crate) fn apply_conflict_decision_to_queue(
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

#[derive(Clone, Copy, Debug)]
enum QueuePosition {
    Front,
    Back,
}

impl From<QueuePosition> for OperationQueuePosition {
    fn from(position: QueuePosition) -> Self {
        match position {
            QueuePosition::Front => OperationQueuePosition::Front,
            QueuePosition::Back => OperationQueuePosition::Back,
        }
    }
}

fn queue_transfer_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    request: FileOperationRequest,
    position: QueuePosition,
) {
    let snapshot = {
        state
            .borrow_mut()
            .queue_file_operation(request, position.into())
    };

    set_status(ui, &operation_queued_status(snapshot));
    if !snapshot.active && !snapshot.pending_conflict {
        start_next_operation(ui, state, bridge);
    }
}

pub(crate) fn start_next_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let request = {
        let mut state = state.borrow_mut();
        if !state.can_start_file_operation() {
            return;
        }
        let request = loop {
            let Some(mut request) = state.operation_queue.pop_front() else {
                return;
            };
            match transfer_request_conflict_destination(&request) {
                Ok(Some(destination)) if request.conflict_policy == "ask" => {
                    open_transfer_conflict(ui, &mut state, request, destination);
                    return;
                }
                Ok(_) => {
                    if request.conflict_policy == "ask" {
                        request.conflict_policy = "keep-both".to_string();
                    }
                    break request;
                }
                Err(err) => {
                    set_status(ui, &format!("Skipped transfer: {err}"));
                    continue;
                }
            }
        };
        let cancel = state.begin_file_operation(request.id);
        (request, cancel)
    };
    let (request, cancel) = request;

    set_status(
        ui,
        &operation_started_status(request.operation.as_str(), &request.source),
    );

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let task_operation = request.operation.clone();
        let task_source = request.source.clone();
        let task_target_dir = request.target_dir.clone();
        let task_conflict_policy = request.conflict_policy.clone();
        let privileged_command = privilege::PrivilegedCommand::Transfer {
            operation: request.operation.clone(),
            source: request.source.clone(),
            target_dir: request.target_dir.clone(),
        };
        let progress_tx = async_tx.clone();
        let progress_notify_ui = notify_ui.clone();
        let progress_operation = request.operation.clone();
        let progress_source = request.source.clone();
        let progress_id = request.id;
        let result = match tokio::task::spawn_blocking(move || {
            file_ops::perform_transfer_with_progress_outcome(
                &task_operation,
                &task_source,
                &task_target_dir,
                &task_conflict_policy,
                Some(cancel),
                move |progress| {
                    send_async_event(
                        progress_tx.clone(),
                        progress_notify_ui.clone(),
                        AsyncEvent::FileOperationProgress(FileOperationProgress {
                            id: progress_id,
                            operation: progress_operation.clone(),
                            source: progress_source.clone(),
                            bytes_done: progress.bytes_done,
                            bytes_total: progress.bytes_total,
                        }),
                    );
                },
            )
        })
        .await
        {
            Ok(result) => result,
            Err(err) => Err(format!("file operation task failed: {err}")),
        };

        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileOperationFinished(FileOperationResult {
                id: request.id,
                operation: request.operation,
                source: request.source,
                target_dir: request.target_dir,
                privileged_command: Some(privileged_command),
                result,
            }),
        );
    });
}

fn open_transfer_conflict(
    ui: &AppWindow,
    state: &mut AppState,
    request: FileOperationRequest,
    destination: PathBuf,
) {
    let source_label = path_label(request.source.to_string_lossy().as_ref());
    let target_label = path_label(destination.to_string_lossy().as_ref());
    ui.set_transfer_conflict_source(source_label.as_str().into());
    ui.set_transfer_conflict_target(target_label.as_str().into());
    ui.set_transfer_conflict_rename_name(default_rename_suggestion(&destination).into());
    ui.set_transfer_conflict_open(true);
    state.pending_transfer_conflict = Some(TransferConflict {
        operation: request.operation,
        source: request.source,
        target_dir: request.target_dir,
        destination,
    });
    set_status(ui, "Transfer needs a conflict decision");
}

fn default_rename_suggestion(destination: &Path) -> String {
    default_rename_suggestion_with_reserved(destination, &[])
}

fn default_rename_suggestion_with_reserved(
    destination: &Path,
    reserved_destinations: &[PathBuf],
) -> String {
    let Some(file_name) = destination.file_name() else {
        return path_label(destination.to_string_lossy().as_ref());
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

pub(crate) fn cancel_queued_operations(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let summary = state.borrow_mut().cancel_file_operations();
    set_status(ui, &operation_cancel_status(summary));
}

pub(crate) fn path_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn display_location_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_str().unwrap_or("/"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_conflict_decision_to_queue, apply_rename_to_remaining_conflicts,
        clear_accepted_cut_source, clear_cut_sources_for_remaining_conflicts,
        default_rename_suggestion, default_rename_suggestion_with_reserved, focused_current_dir,
        inactive_pane_drop_allowed, target_is_source_or_descendant,
        transfer_request_conflict_destination, transfer_start_rejection,
    };
    use crate::app::state::{AppState, FileOperationRequest};
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn queued_transfer_detects_conflict_at_execution_time() {
        let temp = test_dir("queued-conflict");
        let source = temp.join("source").join("note.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();
        fs::write(target_dir.join("note.txt"), "old").unwrap();

        let request = transfer_request("copy", &source, &target_dir, "ask");

        assert_eq!(
            transfer_request_conflict_destination(&request).unwrap(),
            Some(target_dir.join("note.txt"))
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn queued_transfer_detects_conflict_with_broken_symlink_destination() {
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
    fn queued_transfer_reports_no_conflict_for_free_destination() {
        let temp = test_dir("queued-free");
        let source = temp.join("source").join("note.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();

        let request = transfer_request("copy", &source, &target_dir, "ask");

        assert_eq!(
            transfer_request_conflict_destination(&request).unwrap(),
            None
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn transfer_start_rejection_matches_paste_acceptance_rules() {
        let temp = test_dir("start-rejection");
        let source = temp.join("source").join("note.txt");
        let folder_source = temp.join("folder-source");
        let target_dir = temp.join("target");
        let target_file = temp.join("target-file.txt");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(&folder_source).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&source, "new").unwrap();
        fs::write(&target_file, "not a folder").unwrap();

        assert_eq!(
            transfer_start_rejection(&temp.join("missing.txt"), &target_dir),
            Some("Source no longer exists")
        );
        assert_eq!(
            transfer_start_rejection(&source, &target_file),
            Some("Target is not a folder")
        );
        assert_eq!(
            transfer_start_rejection(&folder_source, &folder_source),
            Some("Cannot drop an item onto itself")
        );
        assert_eq!(
            transfer_start_rejection(&folder_source, &folder_source.join("child")),
            Some("Target is not a folder")
        );
        fs::create_dir_all(folder_source.join("child")).unwrap();
        assert_eq!(
            transfer_start_rejection(&folder_source, &folder_source.join("child")),
            Some("Cannot drop a folder into itself")
        );
        assert_eq!(transfer_start_rejection(&source, &target_dir), None);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn current_dir_transfer_target_uses_focused_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-left"));

        assert!(state.panes.open_inactive(PathBuf::from("/tmp/fika-right")));
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-left"));

        assert!(state.panes.focus_inactive());
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-right"));
    }

    #[test]
    fn inactive_pane_drop_allowed_requires_split_target() {
        let state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());

        assert!(!inactive_pane_drop_allowed(
            &state,
            Path::new("/tmp/fika-source")
        ));
    }

    #[test]
    fn inactive_pane_drop_allowed_accepts_file_into_inactive_directory() {
        let mut state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(state.panes.open_inactive(PathBuf::from("/tmp/fika-right")));

        assert!(inactive_pane_drop_allowed(
            &state,
            Path::new("/tmp/fika-left/note.txt")
        ));
    }

    #[test]
    fn inactive_pane_drop_allowed_rejects_self_and_descendant_targets() {
        let source = PathBuf::from("/tmp/fika-source");
        let mut same_target = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(same_target.panes.open_inactive(source.clone()));

        assert!(!inactive_pane_drop_allowed(&same_target, &source));

        let mut descendant_target = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(descendant_target.panes.open_inactive(source.join("child")));

        assert!(!inactive_pane_drop_allowed(&descendant_target, &source));
    }

    #[cfg(unix)]
    #[test]
    fn transfer_target_rejects_symlinked_descendant_directory() {
        let temp = test_dir("symlink-descendant");
        let source = temp.join("source");
        let child = source.join("child");
        let link = temp.join("link-to-child");
        fs::create_dir_all(&child).unwrap();
        std::os::unix::fs::symlink(&child, &link).unwrap();

        assert!(target_is_source_or_descendant(&source, &link));
        assert_eq!(
            transfer_start_rejection(&source, &link),
            Some("Cannot drop a folder into itself")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn conflict_dialog_default_rename_uses_keep_both_style_name() {
        let temp = test_dir("rename-suggestion");
        fs::create_dir_all(&temp).unwrap();
        let destination = temp.join("note.txt");
        fs::write(&destination, "old").unwrap();

        assert_eq!(default_rename_suggestion(&destination), "note copy.txt");

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

        let mut queue = VecDeque::from([
            transfer_request("copy", &conflicted_source, &target_dir, "ask"),
            transfer_request("copy", &free_source, &target_dir, "ask"),
        ]);

        assert_eq!(apply_conflict_decision_to_queue(&mut queue, "skip"), 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].source, free_source);
        assert_eq!(queue[0].conflict_policy, "ask");

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

        let mut queue = VecDeque::from([
            transfer_request("copy", &conflicted_source, &target_dir, "ask"),
            transfer_request("copy", &free_source, &target_dir, "ask"),
        ]);

        assert_eq!(apply_conflict_decision_to_queue(&mut queue, "keep-both"), 1);
        assert_eq!(queue[0].conflict_policy, "keep-both");
        assert_eq!(queue[1].conflict_policy, "ask");

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
            apply_rename_to_remaining_conflicts(&mut state, &mut reserved),
            (2, false)
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
            apply_rename_to_remaining_conflicts(&mut state, &mut Vec::new()),
            (2, true)
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
            default_rename_suggestion_with_reserved(
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

        assert!(clear_accepted_cut_source(
            &mut state,
            "move",
            Path::new("/tmp/a")
        ));

        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/b")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn accepted_cut_source_clears_cut_when_last_path_is_removed() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![PathBuf::from("/tmp/a")];

        assert!(clear_accepted_cut_source(
            &mut state,
            "move",
            Path::new("/tmp/a")
        ));

        assert!(state.clipboard_paths.is_empty());
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn accepted_cut_source_ignores_copy_and_non_cut_clipboards() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_cut = true;
        state.clipboard_paths = vec![PathBuf::from("/tmp/a")];

        assert!(!clear_accepted_cut_source(
            &mut state,
            "copy",
            Path::new("/tmp/a")
        ));
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/a")]);
        assert!(state.clipboard_cut);

        state.clipboard_cut = false;
        assert!(!clear_accepted_cut_source(
            &mut state,
            "move",
            Path::new("/tmp/a")
        ));
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

        assert!(clear_cut_sources_for_remaining_conflicts(
            &mut state,
            "keep-both"
        ));

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

        assert!(!clear_cut_sources_for_remaining_conflicts(
            &mut state, "skip"
        ));

        assert_eq!(state.clipboard_paths, vec![source]);
        assert!(state.clipboard_cut);

        let _ = fs::remove_dir_all(temp);
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
        let path =
            std::env::temp_dir().join(format!("fika-transfer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
