use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::file_clipboard::sync_clipboard_ui;
use crate::app::geometry::{MainGridLayout, PopupPlacement, PopupPoint};
use crate::app::selection::filtered_entry_at;
use crate::app::state::{AppState, FileOperationRequest, TransferConflict};
use crate::fs::{file_ops, privilege};
use crate::{
    AppWindow, AsyncEvent, FileEntry, FileOperationProgress, FileOperationResult, set_status,
};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

const MENU_SCREEN_MARGIN: f32 = 12.0;
const MENU_POINTER_GAP: f32 = 8.0;
const TRANSFER_MENU_WIDTH: f32 = 240.0;
const TRANSFER_MENU_HEIGHT: f32 = 30.0 + 4.0 * 38.0 + 8.0;

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
    let target_dir = main_drop_target_dir(ui, state, x, y, source);
    transfer_target_rejection(source, &target_dir).is_none()
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
        .unwrap_or_else(|| state.current_dir.clone())
}

fn prepare_current_dir_transfer_with_state(
    ui: &AppWindow,
    state: &AppState,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let target_path = state.current_dir.display().to_string();
    let target_label = display_location_name(&state.current_dir);
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
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let menu_position = transfer_menu_position(window_size.width, window_size.height, x, y);
    ui.set_transfer_menu_x(menu_position.x);
    ui.set_transfer_menu_y(menu_position.y);
    true
}

fn transfer_menu_position(
    view_width: f32,
    view_height: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> PopupPoint {
    PopupPlacement::new(
        view_width,
        view_height,
        MENU_SCREEN_MARGIN,
        MENU_POINTER_GAP,
    )
    .root_popup(
        anchor_x,
        anchor_y,
        TRANSFER_MENU_WIDTH,
        TRANSFER_MENU_HEIGHT,
    )
}

pub(crate) fn transfer_target_rejection(source: &Path, target_dir: &Path) -> Option<&'static str> {
    if source == target_dir {
        return Some("Cannot drop an item onto itself");
    }
    if target_dir.starts_with(source) {
        return Some("Cannot drop a folder into itself");
    }
    None
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
    if !source.exists() {
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
            if skipped_remaining > 0 {
                set_status(
                    ui,
                    &format!(
                        "Skipped {} and {skipped_remaining} remaining conflict(s)",
                        conflict.destination.display()
                    ),
                );
            } else {
                set_status(ui, &format!("Skipped {}", conflict.destination.display()));
            }
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
                if applied > 0 {
                    set_status(
                        ui,
                        &format!(
                            "Applied {} to {applied} remaining conflict(s)",
                            decision_label(decision)
                        ),
                    );
                }
            }
        }
        _ if let Some(name) = decision.strip_prefix("rename:") => {
            if let Err(err) = file_ops::renamed_destination(&conflict.target_dir, name) {
                state.borrow_mut().pending_transfer_conflict = Some(conflict);
                ui.set_transfer_conflict_open(true);
                set_status(ui, &format!("Cannot rename transfer target: {err}"));
                return;
            }
            let clipboard_changed = {
                let mut state_ref = state.borrow_mut();
                clear_accepted_cut_source(
                    &mut state_ref,
                    conflict.operation.as_str(),
                    &conflict.source,
                )
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

fn decision_label(decision: &str) -> &'static str {
    match decision {
        "keep-both" => "Keep Both",
        "overwrite" => "Overwrite",
        "skip" => "Skip",
        _ => "decision",
    }
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

fn queue_transfer_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    mut request: FileOperationRequest,
    position: QueuePosition,
) {
    let (id, queued_len, active, pending_conflict) = {
        let mut state = state.borrow_mut();
        let id = state.next_operation_id;
        state.next_operation_id += 1;
        request.id = id;
        match position {
            QueuePosition::Front => state.operation_queue.push_front(request),
            QueuePosition::Back => state.operation_queue.push_back(request),
        }
        (
            id,
            state.operation_queue.len(),
            state.active_operation,
            state.pending_transfer_conflict.is_some(),
        )
    };

    set_status(
        ui,
        &format!("Queued operation #{id} ({queued_len} pending)"),
    );
    if active.is_none() && !pending_conflict {
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
        if state.active_operation.is_some() || state.pending_transfer_conflict.is_some() {
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
        let cancel = Arc::new(AtomicBool::new(false));
        state.active_operation = Some(request.id);
        state.active_operation_cancel = Some(Arc::clone(&cancel));
        (request, cancel)
    };
    let (request, cancel) = request;

    set_status(
        ui,
        &format!(
            "{} {}...",
            operation_label(request.operation.as_str()),
            request
                .source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("item")
        ),
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

pub(crate) fn transfer_request_conflict_destination(
    request: &FileOperationRequest,
) -> Result<Option<PathBuf>, String> {
    if !request.source.exists() {
        return Err("source no longer exists".to_string());
    }
    if !request.target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    if let Some(reason) = transfer_target_rejection(&request.source, &request.target_dir) {
        return Err(reason.to_string());
    }
    let destination = file_ops::base_destination(&request.source, &request.target_dir)?;
    Ok(destination.exists().then_some(destination))
}

fn default_rename_suggestion(destination: &Path) -> String {
    let Some(file_name) = destination.file_name() else {
        return path_label(destination.to_string_lossy().as_ref());
    };
    let Some(parent) = destination.parent() else {
        return file_name.to_string_lossy().to_string();
    };
    file_ops::unique_destination(parent, file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| file_name.to_str().unwrap_or("item"))
        .to_string()
}

pub(crate) fn cancel_queued_operations(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let (queued_cancelled, active_cancelled) = {
        let mut state = state.borrow_mut();
        let queued_cancelled = state.operation_queue.len();
        state.operation_queue.clear();
        let active_cancelled = state.active_operation_cancel.is_some();
        if let Some(cancel) = &state.active_operation_cancel {
            cancel.store(true, AtomicOrdering::Relaxed);
        }
        (queued_cancelled, active_cancelled)
    };
    if queued_cancelled == 0 && !active_cancelled {
        set_status(ui, "No queued operations to cancel");
    } else if active_cancelled {
        set_status(
            ui,
            &format!("Cancelling active operation; removed {queued_cancelled} queued operation(s)"),
        );
    } else {
        set_status(
            ui,
            &format!("Cancelled {queued_cancelled} queued operation(s)"),
        );
    }
}

pub(crate) fn operation_label(operation: &str) -> &'static str {
    match operation {
        "move" => "Moving",
        "copy" => "Copying",
        "link" => "Linking",
        _ => "Processing",
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
        apply_conflict_decision_to_queue, clear_accepted_cut_source,
        clear_cut_sources_for_remaining_conflicts, default_rename_suggestion,
        transfer_menu_position, transfer_request_conflict_destination, transfer_start_rejection,
    };
    use crate::app::geometry::PopupPoint;
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

    #[test]
    fn transfer_menu_position_flips_and_clamps_like_context_menus() {
        assert_eq!(
            transfer_menu_position(800.0, 600.0, 100.0, 100.0),
            PopupPoint { x: 108.0, y: 108.0 }
        );
        assert_eq!(
            transfer_menu_position(800.0, 600.0, 790.0, 590.0),
            PopupPoint { x: 542.0, y: 392.0 }
        );
        assert_eq!(
            transfer_menu_position(200.0, 120.0, 10.0, 10.0),
            PopupPoint { x: 12.0, y: 12.0 }
        );
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
    fn apply_rename_to_remaining_conflicts_is_not_supported() {
        let temp = test_dir("apply-rename");
        let conflicted_source = temp.join("sources").join("conflicted.txt");
        let target_dir = temp.join("target");
        fs::create_dir_all(conflicted_source.parent().unwrap()).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(&conflicted_source, "new").unwrap();
        fs::write(target_dir.join("conflicted.txt"), "old").unwrap();

        let mut queue = VecDeque::from([transfer_request(
            "copy",
            &conflicted_source,
            &target_dir,
            "ask",
        )]);

        assert_eq!(
            apply_conflict_decision_to_queue(&mut queue, "rename:custom.txt"),
            0
        );
        assert_eq!(queue[0].conflict_policy, "ask");

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
