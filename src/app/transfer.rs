use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::file_clipboard::sync_clipboard_ui;
use crate::app::item_view::entry_at_pane_point;
use crate::app::operation_controller::{
    OperationQueuePosition, OperationStartDecision, default_transfer_rename_suggestion,
    operation_cancel_status, operation_queued_status, transfer_conflict_apply_remaining_status,
    transfer_conflict_skip_status, transfer_request_conflict_destination,
    transfer_target_rejection,
};
use crate::app::pane::PaneTarget;
use crate::app::selection::filtered_entry_at;
use crate::app::state::{AppState, FileOperationRequest, TransferConflict};
use crate::fs::{file_ops, privilege};
use crate::{
    AppWindow, AsyncEvent, FileOperationProgress, FileOperationResult, set_status,
    set_status_for_panes,
};
use std::cell::RefCell;
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
    let Ok(target_index) = usize::try_from(target_index) else {
        return false;
    };
    let Some((target_path, target_label)) = ({
        let state_ref = state.borrow();
        state_ref
            .places
            .get(target_index)
            .map(|target| (target.path.clone(), target.label.clone()))
    }) else {
        return false;
    };
    let source_label = path_label(source);
    prepare_transfer_menu(
        ui,
        state,
        source,
        source_label.as_str(),
        target_path.as_str(),
        target_label.as_str(),
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
    let target = {
        let state_ref = state.borrow();
        usize::try_from(target_index)
            .ok()
            .and_then(|target_index| filtered_entry_at(&state_ref, target_index))
            .filter(|target| target.is_dir && target.path.as_str() != source)
            .map(|target| (target.path.to_string(), target.name.to_string()))
    };
    let source_label = path_label(source);

    if let Some((target_path, target_label)) = target {
        return prepare_transfer_menu(
            ui,
            state,
            source,
            source_label.as_str(),
            target_path.as_str(),
            target_label.as_str(),
            x,
            y,
        );
    }

    prepare_current_dir_transfer_with_state(ui, state, source, source_label.as_str(), x, y)
}

pub(crate) fn prepare_current_dir_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    prepare_current_dir_transfer_with_state(ui, state, source, source_label, x, y)
}

pub(crate) fn prepare_pane_transfer(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    source: &str,
    x: f32,
    y: f32,
) -> bool {
    let target = {
        let state_ref = state.borrow();
        entry_at_pane_point(ui, &state_ref, slot, x, y)
            .filter(|target| target.is_dir && target.path.as_str() != source)
            .map(|target| (target.path.to_string(), target.name.to_string()))
    };
    let source_label = path_label(source);

    if let Some((target_path, target_label)) = target {
        return prepare_transfer_menu(
            ui,
            state,
            source,
            source_label.as_str(),
            target_path.as_str(),
            target_label.as_str(),
            x,
            y,
        );
    }

    prepare_current_dir_transfer_for_target_with_state(
        ui,
        state,
        PaneTarget::Slot(slot),
        source,
        source_label.as_str(),
        x,
        y,
    )
}

pub(crate) fn pane_drop_allowed(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> bool {
    pane_drop_rejection(ui, state, slot, x, y, source).is_none()
}

#[cfg(test)]
fn pane_current_dir_drop_allowed(state: &AppState, slot: i32, source: &Path) -> bool {
    pane_current_dir(state, PaneTarget::Slot(slot))
        .is_some_and(|target_dir| transfer_target_rejection(source, target_dir).is_none())
}

pub(crate) fn pane_drop_target_path(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> Option<String> {
    entry_at_pane_point(ui, state, slot, x, y)
        .filter(|target| target.is_dir && Path::new(target.path.as_str()) != source)
        .map(|target| target.path.to_string())
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

fn pane_drop_rejection(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> Option<&'static str> {
    let target_dir = pane_drop_target_dir(ui, state, slot, x, y, source)?;
    transfer_target_rejection(source, &target_dir)
}

fn pane_drop_target_dir(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> Option<PathBuf> {
    entry_at_pane_point(ui, state, slot, x, y)
        .filter(|target| target.is_dir && Path::new(target.path.as_str()) != source)
        .map(|target| PathBuf::from(target.path.as_str()))
        .or_else(|| pane_current_dir(state, PaneTarget::Slot(slot)).map(Path::to_path_buf))
}

fn prepare_current_dir_transfer_with_state(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let (target_path, target_label) = {
        let state_ref = state.borrow();
        let current_dir = focused_current_dir(&state_ref);
        (
            current_dir.display().to_string(),
            display_location_name(current_dir),
        )
    };
    prepare_transfer_menu(
        ui,
        state,
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
    state: &Rc<RefCell<AppState>>,
    target: PaneTarget,
    source: &str,
    source_label: &str,
    x: f32,
    y: f32,
) -> bool {
    let target = {
        let state_ref = state.borrow();
        pane_current_dir(&state_ref, target).map(|current_dir| {
            (
                current_dir.display().to_string(),
                display_location_name(current_dir),
            )
        })
    };
    let Some((target_path, target_label)) = target else {
        set_status(ui, state, "No split pane target is available");
        ui.set_transfer_source_path("".into());
        ui.set_transfer_target_path("".into());
        return false;
    };
    prepare_transfer_menu(
        ui,
        state,
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
        .unwrap_or_else(|| state.panes.focused().current_dir.as_path())
}

fn pane_current_dir(state: &AppState, target: PaneTarget) -> Option<&Path> {
    state
        .panes
        .pane_for_target(target)
        .map(|pane| pane.current_dir.as_path())
}

#[allow(clippy::too_many_arguments)]
fn prepare_transfer_menu(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    source_path: &str,
    source_label: &str,
    target_path: &str,
    target_label: &str,
    x: f32,
    y: f32,
) -> bool {
    if let Some(reason) = transfer_target_rejection(Path::new(source_path), Path::new(target_path))
    {
        set_status(ui, state, reason);
        ui.set_transfer_source_path("".into());
        ui.set_transfer_target_path("".into());
        ui.set_transfer_move_available(true);
        return false;
    }

    ui.set_transfer_move_available(move_transfer_available(
        Path::new(source_path),
        Path::new(target_path),
    ));
    ui.set_transfer_source_path(source_path.into());
    ui.set_transfer_source_label(source_label.into());
    ui.set_transfer_target_path(target_path.into());
    ui.set_transfer_target_label(target_label.into());
    ui.set_transfer_menu_x(x);
    ui.set_transfer_menu_y(y);
    true
}

fn move_transfer_available(source_path: &Path, target_dir: &Path) -> bool {
    source_path.parent() != Some(target_dir)
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
    if let Some(reason) = transfer_operation_start_rejection(operation, &source, &target_dir) {
        set_status(ui, state, reason);
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

fn transfer_operation_start_rejection(
    operation: &str,
    source: &Path,
    target_dir: &Path,
) -> Option<&'static str> {
    transfer_start_rejection(source, target_dir).or_else(|| {
        (operation == "move" && !move_transfer_available(source, target_dir))
            .then_some("Cannot move an item to its current folder")
    })
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
        set_status(ui, state, "No transfer conflict is pending");
        return;
    };

    match decision {
        "skip" => {
            let summary = if apply_to_remaining {
                state
                    .borrow_mut()
                    .apply_transfer_conflict_decision_to_remaining(decision)
            } else {
                Default::default()
            };
            set_status(
                ui,
                state,
                &transfer_conflict_skip_status(&conflict.destination, summary.applied_remaining),
            );
            start_next_operation(ui, state, bridge);
        }
        "keep-both" | "overwrite" => {
            if decision == "overwrite" && conflict.destination == conflict.source {
                set_status(ui, state, "Cannot overwrite an item with itself");
                return;
            }
            let mut applied_remaining = 0;
            let clipboard_changed = {
                let mut state_ref = state.borrow_mut();
                let mut clipboard_changed = state_ref
                    .clear_accepted_cut_source(conflict.operation.as_str(), &conflict.source);
                if apply_to_remaining {
                    let summary = state_ref.apply_transfer_conflict_decision_to_remaining(decision);
                    applied_remaining = summary.applied_remaining;
                    clipboard_changed |= summary.clipboard_changed;
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
            if apply_to_remaining
                && let Some(status) =
                    transfer_conflict_apply_remaining_status(decision, applied_remaining)
            {
                set_status(ui, state, &status);
            }
        }
        _ if let Some(name) = decision.strip_prefix("rename:") => {
            let mut reserved_destinations =
                match file_ops::renamed_destination(&conflict.target_dir, name) {
                    Ok(destination) => vec![destination],
                    Err(err) => {
                        state.borrow_mut().pending_transfer_conflict = Some(conflict);
                        ui.set_transfer_conflict_open(true);
                        set_status(ui, state, &format!("Cannot rename transfer target: {err}"));
                        return;
                    }
                };
            let mut applied_remaining = 0;
            let clipboard_changed = {
                let mut state_ref = state.borrow_mut();
                let mut clipboard_changed = state_ref
                    .clear_accepted_cut_source(conflict.operation.as_str(), &conflict.source);
                if apply_to_remaining {
                    let summary = state_ref
                        .apply_transfer_rename_to_remaining_conflicts(&mut reserved_destinations);
                    applied_remaining = summary.applied_remaining;
                    clipboard_changed |= summary.clipboard_changed;
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
            if apply_to_remaining
                && applied_remaining > 0
                && let Some(status) =
                    transfer_conflict_apply_remaining_status("rename", applied_remaining)
            {
                set_status(ui, state, &status);
            }
        }
        _ => set_status(ui, state, "Unknown conflict decision"),
    }
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

    set_status(ui, state, &operation_queued_status(snapshot));
    if !snapshot.active && !snapshot.pending_conflict {
        start_next_operation(ui, state, bridge);
    }
}

pub(crate) fn start_next_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let start = loop {
        let decision = state.borrow_mut().next_file_operation_start();
        match decision {
            OperationStartDecision::Idle => return,
            OperationStartDecision::NeedsConflict(conflict) => {
                open_transfer_conflict(ui, state, &conflict);
                return;
            }
            OperationStartDecision::Skipped { status } => {
                set_status(ui, state, &status);
                continue;
            }
            OperationStartDecision::Started(start) => break start,
        }
    };
    let request = start.request;
    let cancel = start.cancel;
    let pane_ids = start.pane_ids;

    set_status_for_panes(ui, state, &pane_ids, &start.status);

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
    state: &Rc<RefCell<AppState>>,
    conflict: &TransferConflict,
) {
    let source_label = path_label(conflict.source.to_string_lossy().as_ref());
    let target_label = path_label(conflict.destination.to_string_lossy().as_ref());
    ui.set_transfer_conflict_source(source_label.as_str().into());
    ui.set_transfer_conflict_target(target_label.as_str().into());
    ui.set_transfer_conflict_rename_name(
        default_transfer_rename_suggestion(&conflict.destination).into(),
    );
    ui.set_transfer_conflict_open(true);
    set_status(ui, state, "Transfer needs a conflict decision");
}

pub(crate) fn cancel_queued_operations(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let summary = state.borrow_mut().cancel_file_operations();
    let status = operation_cancel_status(&summary);
    set_status_for_panes(ui, state, &summary.pane_ids, &status);
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
        focused_current_dir, move_transfer_available, pane_current_dir_drop_allowed,
        target_is_source_or_descendant, transfer_operation_start_rejection,
        transfer_start_rejection,
    };
    use crate::app::state::AppState;
    use std::fs;
    use std::path::{Path, PathBuf};

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
    fn same_directory_transfer_hides_and_rejects_noop_move() {
        let temp = test_dir("same-directory-transfer");
        let source = temp.join("note.txt");
        fs::create_dir_all(&temp).unwrap();
        fs::write(&source, "new").unwrap();

        assert!(!move_transfer_available(&source, &temp));
        assert!(move_transfer_available(&source, &temp.join("subdir")));
        assert_eq!(transfer_start_rejection(&source, &temp), None);
        assert_eq!(
            transfer_operation_start_rejection("copy", &source, &temp),
            None
        );
        assert_eq!(
            transfer_operation_start_rejection("link", &source, &temp),
            None
        );
        assert_eq!(
            transfer_operation_start_rejection("move", &source, &temp),
            Some("Cannot move an item to its current folder")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn current_dir_transfer_target_uses_focused_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-left"));

        assert!(state.panes.open_pane(PathBuf::from("/tmp/fika-right")));
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-left"));

        assert!(state.panes.focus_slot(1));
        assert_eq!(focused_current_dir(&state), Path::new("/tmp/fika-right"));
    }

    #[test]
    fn pane_current_dir_drop_allowed_requires_existing_slot() {
        let state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());

        assert!(!pane_current_dir_drop_allowed(
            &state,
            1,
            Path::new("/tmp/fika-source")
        ));
    }

    #[test]
    fn pane_current_dir_drop_allowed_accepts_file_into_slot_directory() {
        let mut state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/fika-right")));

        assert!(pane_current_dir_drop_allowed(
            &state,
            1,
            Path::new("/tmp/fika-left/note.txt")
        ));
    }

    #[test]
    fn pane_current_dir_drop_allowed_rejects_self_and_descendant_targets() {
        let source = PathBuf::from("/tmp/fika-source");
        let mut same_target = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(same_target.panes.open_pane(source.clone()));

        assert!(!pane_current_dir_drop_allowed(&same_target, 1, &source));

        let mut descendant_target = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(descendant_target.panes.open_pane(source.join("child")));

        assert!(!pane_current_dir_drop_allowed(
            &descendant_target,
            1,
            &source
        ));
    }

    #[test]
    fn transfer_menu_preparation_releases_state_borrow_before_status_updates() {
        let source = include_str!("transfer.rs");

        let place_body = source
            .split_once("pub(crate) fn prepare_place_transfer(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn prepare_entry_transfer("))
            .map(|(body, _)| body)
            .expect("prepare_place_transfer body should be present");
        let place_borrow_end = place_body
            .find("}) else")
            .expect("place target lookup should leave a scoped borrow");
        let place_menu = place_body
            .find("prepare_transfer_menu(")
            .expect("place transfer should prepare a menu");

        assert!(
            place_body.contains("let state_ref = state.borrow();")
                && place_body
                    .contains(".map(|target| (target.path.clone(), target.label.clone()))")
                && place_borrow_end < place_menu,
            "place transfer target data must be copied before prepare_transfer_menu can call set_status"
        );

        for (name, start, end) in [
            (
                "entry",
                "pub(crate) fn prepare_entry_transfer(",
                "pub(crate) fn prepare_current_dir_transfer(",
            ),
            (
                "pane",
                "pub(crate) fn prepare_pane_transfer(",
                "pub(crate) fn pane_drop_allowed(",
            ),
            (
                "focused current directory",
                "fn prepare_current_dir_transfer_with_state(",
                "fn prepare_current_dir_transfer_for_target_with_state(",
            ),
            (
                "target current directory",
                "fn prepare_current_dir_transfer_for_target_with_state(",
                "fn focused_current_dir(",
            ),
        ] {
            let body = source
                .split_once(start)
                .and_then(|(_, rest)| rest.split_once(end))
                .map(|(body, _)| body)
                .expect("transfer preparation body should be present");
            let borrow_end = body
                .find("};\n")
                .expect("state borrow should be scoped before menu preparation");
            let menu = body
                .find("prepare_transfer_menu(")
                .expect("transfer preparation should call prepare_transfer_menu");

            assert!(
                body.contains("let state_ref = state.borrow();") && borrow_end < menu,
                "{name} transfer target lookup must end its state borrow before prepare_transfer_menu"
            );
        }
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
    fn started_transfer_status_uses_affected_pane_route() {
        let source = include_str!("transfer.rs");
        let body = source
            .split_once("pub(crate) fn start_next_operation(")
            .and_then(|(_, rest)| rest.split_once("fn open_transfer_conflict("))
            .map(|(body, _)| body)
            .expect("start_next_operation body should be present");

        assert!(
            body.contains("state.borrow_mut().next_file_operation_start()")
                && body.contains("OperationStartDecision::Started(start) => break start")
                && body.contains("let pane_ids = start.pane_ids;")
                && body.contains("set_status_for_panes(ui, state, &pane_ids, &start.status);"),
            "file operation start status should use the controller's affected-pane route"
        );
        assert!(
            !body.contains("state_ref.operation_queue.pop_front()")
                && !body.contains("begin_file_operation_for_panes")
                && !body.contains("operation_started_status")
                && !body.contains("open_transfer_conflict(ui, state, request, destination)")
                && !body.contains("Skipped transfer: {err}"),
            "file operation start decisions should stay in operation_controller.rs, with UI effects applied after the state borrow ends"
        );

        let conflict_body = source
            .split_once("fn open_transfer_conflict(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn cancel_queued_operations("))
            .map(|(body, _)| body)
            .expect("open_transfer_conflict body should be present");
        assert!(
            !conflict_body.contains("pending_transfer_conflict = Some"),
            "conflict registration belongs to operation_controller.rs; the popup helper should only apply UI state"
        );
    }

    #[test]
    fn conflict_resolution_queue_mutation_stays_controller_owned() {
        let source = include_str!("transfer.rs");
        let body = source
            .split_once("pub(crate) fn resolve_transfer_conflict(")
            .and_then(|(_, rest)| rest.split_once("#[derive(Clone, Copy, Debug)]"))
            .map(|(body, _)| body)
            .expect("resolve_transfer_conflict body should be present");

        assert!(
            body.contains("apply_transfer_conflict_decision_to_remaining(decision)")
                && body.contains("apply_transfer_rename_to_remaining_conflicts("),
            "transfer.rs should delegate apply-to-remaining conflict state changes to operation_controller.rs"
        );
        assert!(
            !body.contains("operation_queue")
                && !body.contains("apply_conflict_decision_to_queue")
                && !body.contains("clear_cut_sources_for_remaining_conflicts"),
            "transfer.rs should only apply UI side effects after controller state updates"
        );
    }

    #[test]
    fn cancel_transfer_status_uses_active_operation_pane_route() {
        let source = include_str!("transfer.rs");
        let body = source
            .split_once("pub(crate) fn cancel_queued_operations(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn path_label("))
            .map(|(body, _)| body)
            .expect("cancel_queued_operations body should be present");

        assert!(
            body.contains("let status = operation_cancel_status(&summary);")
                && body.contains("set_status_for_panes(ui, state, &summary.pane_ids, &status);"),
            "operation cancellation status should use the active operation's affected-pane route"
        );
        assert!(
            !body.contains("set_status(ui, state, &operation_cancel_status(summary));"),
            "operation cancellation status must not jump to whichever pane is focused"
        );
    }

    fn test_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("fika-transfer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
