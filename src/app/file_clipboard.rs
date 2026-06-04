use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::{AsyncEvent, ClipboardLoadResult, ClipboardPasteLoadResult};
use crate::app::pane::PaneTarget;
use crate::app::state::{AppState, FileUndo};
use crate::app::transfer::{
    TransferStart, start_transfer_operation, target_is_source_or_descendant,
};
use crate::desktop::clipboard;
use crate::fs::file_ops;
use crate::{AppWindow, set_status};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        ui.on_copy_paths(move |context_path, cut| {
            if let Some(ui) = ui_weak.upgrade() {
                copy_paths(&ui, &state, context_path.as_str(), cut);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_paste_into(move |target_dir| {
            if let Some(ui) = ui_weak.upgrade() {
                paste_into(&ui, &state, &bridge, target_dir.as_str());
            }
        });
    }

    {
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_refresh_clipboard_availability(move || {
            refresh_clipboard_availability_async(&state, &bridge);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        ui.on_sync_clipboard_state(move || {
            if let Some(ui) = ui_weak.upgrade() {
                sync_clipboard_ui(&ui, &state);
            }
        });
    }
}

pub(crate) fn sync_clipboard_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    ui.set_clipboard_has_paths(
        !state.clipboard_paths.is_empty() || state.clipboard_content_kind.is_some(),
    );
    ui.set_clipboard_cut(state.clipboard_cut);
}

fn copy_paths(ui: &AppWindow, state: &Rc<RefCell<AppState>>, context_path: &str, cut: bool) {
    let paths = {
        let state = state.borrow();
        clipboard_paths_for_focused_context(&state, context_path)
    };

    if paths.is_empty() {
        set_status(ui, state, "Nothing selected");
        return;
    }

    {
        let mut state = state.borrow_mut();
        state.clipboard_generation.next();
        state.clipboard_paths = paths;
        state.clipboard_cut = cut;
        state.clipboard_content_kind = None;
    }
    sync_clipboard_ui(ui, state);

    let desktop_clipboard = clipboard::copy_file_list(&state.borrow().clipboard_paths, cut);
    let count = state.borrow().clipboard_paths.len();
    let action = if cut { "Cut" } else { "Copied" };
    match desktop_clipboard {
        Ok(()) => set_status(
            ui,
            state,
            &format!("{action} {count} item(s) via Wayland data-control"),
        ),
        Err(err) => set_status(
            ui,
            state,
            &format!("{action} {count} item(s) in Fika; desktop clipboard unavailable: {err}"),
        ),
    }
}

fn paste_into(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    target_dir: &str,
) {
    let target_dir = PathBuf::from(target_dir);
    if !target_dir.is_dir() {
        set_status(ui, state, "Paste target is not a folder");
        return;
    }

    request_clipboard_paste(ui, state, bridge, target_dir);
}

fn start_file_clipboard_paste(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    target_dir: PathBuf,
    operation: &'static str,
    paths: Vec<PathBuf>,
) {
    let mut accepted = 0usize;
    let mut cut_clipboard_changed = false;
    for path in &paths {
        if target_is_source_or_descendant(path, &target_dir) {
            set_status(ui, state, "Cannot paste an item into itself");
            continue;
        }
        match start_transfer_operation(
            ui,
            state,
            bridge,
            operation,
            path.to_string_lossy().as_ref(),
            target_dir.to_string_lossy().as_ref(),
        ) {
            TransferStart::Queued => {
                accepted += 1;
                cut_clipboard_changed |= state
                    .borrow_mut()
                    .clear_accepted_cut_source(operation, path);
            }
            TransferStart::NeedsDecision => {
                accepted += 1;
            }
            TransferStart::Rejected => {}
        }
    }

    if cut_clipboard_changed {
        sync_clipboard_ui(ui, state);
    }

    if accepted > 1 {
        set_status(ui, state, &format!("Queued {accepted} paste operation(s)"));
    }
}

fn request_clipboard_paste(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    target_dir: PathBuf,
) {
    let generation = state.borrow_mut().clipboard_generation.next();
    set_status(ui, state, "Reading clipboard...");
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = tokio::task::spawn_blocking(clipboard::read_clipboard_snapshot)
            .await
            .unwrap_or_else(|err| Err(format!("clipboard read task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ClipboardPasteLoaded(ClipboardPasteLoadResult {
                generation,
                target_dir,
                result,
            }),
        );
    });
}

pub(crate) fn apply_clipboard_paste_load_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: ClipboardPasteLoadResult,
) {
    let target_dir = result.target_dir.clone();
    let plan = {
        let mut state_ref = state.borrow_mut();
        prepare_clipboard_paste(&mut state_ref, result, file_ops::path_exists)
    };
    if matches!(plan, ClipboardPastePlan::Stale) {
        return;
    }

    sync_clipboard_ui(ui, state);

    if !target_dir.is_dir() {
        set_status(ui, state, "Paste target is not a folder");
        return;
    }

    match plan {
        ClipboardPastePlan::Files {
            operation, paths, ..
        } => start_file_clipboard_paste(ui, state, bridge, target_dir, operation, paths),
        ClipboardPastePlan::NonFileContent => {
            paste_non_file_content_async(ui, state, bridge, target_dir);
        }
        ClipboardPastePlan::MissingItems => {
            set_status(ui, state, "Clipboard item(s) no longer exist");
        }
        ClipboardPastePlan::Empty { read_error } => {
            if let Some(err) = read_error {
                set_status(ui, state, &format!("Clipboard is not pasteable: {err}"));
            } else {
                set_status(ui, state, "Clipboard does not contain pasteable items");
            }
        }
        ClipboardPastePlan::Stale => {}
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ClipboardPastePlan {
    Files {
        operation: &'static str,
        paths: Vec<PathBuf>,
        pruned_missing: bool,
    },
    NonFileContent,
    MissingItems,
    Empty {
        read_error: Option<String>,
    },
    Stale,
}

fn prepare_clipboard_paste(
    state: &mut AppState,
    result: ClipboardPasteLoadResult,
    exists: impl Fn(&Path) -> bool,
) -> ClipboardPastePlan {
    if !state.clipboard_generation.is_current(result.generation) {
        return ClipboardPastePlan::Stale;
    }

    let mut read_error = None;
    let mut pruned_missing = false;
    match result.result {
        Ok(clipboard) => {
            pruned_missing = merge_desktop_clipboard(state, clipboard, &exists) > 0;
        }
        Err(err) => {
            read_error = Some(err);
        }
    }

    match clipboard_paste_plan_from_state(state, exists, pruned_missing) {
        ClipboardPastePlan::Empty { .. } => ClipboardPastePlan::Empty { read_error },
        plan => plan,
    }
}

fn clipboard_paste_plan_from_state(
    state: &mut AppState,
    exists: impl Fn(&Path) -> bool,
    already_pruned_missing: bool,
) -> ClipboardPastePlan {
    let mut pruned_missing = already_pruned_missing;
    if !state.clipboard_paths.is_empty() {
        let (existing_paths, missing_count) =
            existing_clipboard_paths(&state.clipboard_paths, exists);
        if missing_count > 0 {
            pruned_missing = true;
            state.clipboard_paths = existing_paths;
        }
        if state.clipboard_paths.is_empty() {
            state.clipboard_cut = false;
            return ClipboardPastePlan::MissingItems;
        }
        return ClipboardPastePlan::Files {
            operation: if state.clipboard_cut { "move" } else { "copy" },
            paths: state.clipboard_paths.clone(),
            pruned_missing,
        };
    }

    if already_pruned_missing && state.clipboard_content_kind.is_none() {
        state.clipboard_cut = false;
        ClipboardPastePlan::MissingItems
    } else if state.clipboard_content_kind.is_some() {
        ClipboardPastePlan::NonFileContent
    } else {
        state.clipboard_cut = false;
        ClipboardPastePlan::Empty { read_error: None }
    }
}

fn paste_non_file_content_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    target_dir: PathBuf,
) {
    set_status(ui, state, "Pasting clipboard contents...");
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let affected_dir = target_dir.clone();
        let result = tokio::task::spawn_blocking(move || {
            let content = clipboard::read_non_file_content()?;
            let extension = content
                .extension()
                .ok_or_else(|| format!("unsupported clipboard MIME type {}", content.mime_type))?;
            let destination = file_ops::write_unique_file(
                &target_dir,
                content.base_file_name(),
                extension,
                &content.data,
            )?;
            Ok((
                format!(
                    "{} saved to {}",
                    clipboard_content_label(content.kind),
                    destination.display()
                ),
                Some(FileUndo {
                    operation: "create-file".to_string(),
                    original_source: destination.clone(),
                    destination,
                    overwritten_backup: None,
                    items: Vec::new(),
                }),
            ))
        })
        .await
        .unwrap_or_else(|err| Err(format!("clipboard paste task failed: {err}")));
        let (result, undo) = match result {
            Ok((message, undo)) => (Ok(message), undo),
            Err(err) => (Err(err), None),
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileActionFinished(crate::fs::file_actions::FileActionResult {
                action: "Paste Clipboard",
                affected_dir,
                privileged_command: None,
                result,
                undo,
            }),
        );
    });
}

fn clipboard_content_label(kind: clipboard::ClipboardContentKind) -> &'static str {
    match kind {
        clipboard::ClipboardContentKind::Image => "Image",
        clipboard::ClipboardContentKind::Video => "Video",
        clipboard::ClipboardContentKind::Text => "Text",
    }
}

pub(crate) fn refresh_clipboard_availability_async(
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let Some(generation) = ({
        let mut state = state.borrow_mut();
        begin_clipboard_availability_refresh(&mut state)
    }) else {
        return;
    };
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = tokio::task::spawn_blocking(clipboard::read_clipboard_snapshot)
            .await
            .unwrap_or_else(|err| Err(format!("clipboard read task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ClipboardLoaded(ClipboardLoadResult { generation, result }),
        );
    });
}

fn begin_clipboard_availability_refresh(state: &mut AppState) -> Option<u64> {
    if let Some(pending) = state.clipboard_refresh_pending {
        if state.clipboard_generation.is_current(pending) {
            return None;
        }
        state.clipboard_refresh_pending = None;
    }
    let generation = state.clipboard_generation.next();
    state.clipboard_refresh_pending = Some(generation);
    Some(generation)
}

pub(crate) fn apply_clipboard_load_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    result: ClipboardLoadResult,
) {
    {
        let mut state_ref = state.borrow_mut();
        apply_clipboard_load(&mut state_ref, result, file_ops::path_exists);
    }
    sync_clipboard_ui(ui, state);
}

fn existing_clipboard_paths(
    paths: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> (Vec<PathBuf>, usize) {
    let mut existing = Vec::with_capacity(paths.len());
    let mut missing = 0usize;
    for path in paths {
        if exists(path) {
            existing.push(path.clone());
        } else {
            missing += 1;
        }
    }
    (existing, missing)
}

fn apply_clipboard_load(
    state: &mut AppState,
    result: ClipboardLoadResult,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    if state.clipboard_refresh_pending == Some(result.generation) {
        state.clipboard_refresh_pending = None;
    }
    if !state.clipboard_generation.is_current(result.generation) {
        return false;
    }
    let Ok(clipboard) = result.result else {
        return false;
    };
    merge_desktop_clipboard(state, clipboard, exists);
    true
}

fn merge_desktop_clipboard(
    state: &mut AppState,
    clipboard: clipboard::ClipboardSnapshot,
    exists: impl Fn(&Path) -> bool,
) -> usize {
    state.clipboard_content_kind = clipboard.content_kind;
    let Some(files) = clipboard.files else {
        state.clipboard_paths.clear();
        state.clipboard_cut = false;
        return 0;
    };

    let paths = unique_paths(files.paths);
    let (existing_paths, missing_count) = existing_clipboard_paths(&paths, exists);
    state.clipboard_paths = existing_paths;
    state.clipboard_cut = files.cut && !state.clipboard_paths.is_empty();
    if !state.clipboard_paths.is_empty() {
        state.clipboard_content_kind = None;
    }
    missing_count
}

fn clipboard_paths_for_context(selected_paths: &[String], context_path: &str) -> Vec<PathBuf> {
    let paths = if selected_paths.len() > 1
        && selected_paths
            .iter()
            .any(|selected| selected.as_str() == context_path)
    {
        selected_paths.iter().map(PathBuf::from).collect::<Vec<_>>()
    } else {
        vec![PathBuf::from(context_path)]
    };
    unique_paths(paths)
}

fn clipboard_paths_for_focused_context(state: &AppState, context_path: &str) -> Vec<PathBuf> {
    let selected_paths = state
        .panes
        .pane_for_target(PaneTarget::Focused)
        .map(|pane| pane.selection.paths.as_slice())
        .unwrap_or_else(|| state.panes.focused().selection.paths.as_slice());
    clipboard_paths_for_context(selected_paths, context_path)
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::with_capacity(paths.len());
    for path in paths {
        if !unique.iter().any(|existing| existing == &path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardPastePlan, apply_clipboard_load, begin_clipboard_availability_refresh,
        clipboard_paths_for_context, clipboard_paths_for_focused_context, existing_clipboard_paths,
        merge_desktop_clipboard, prepare_clipboard_paste, unique_paths,
    };
    use crate::app::events::{ClipboardLoadResult, ClipboardPasteLoadResult};
    use crate::app::state::AppState;
    use crate::app::transfer::target_is_source_or_descendant;
    use crate::desktop::clipboard::{ClipboardContentKind, ClipboardSnapshot, FileClipboard};
    use std::path::{Path, PathBuf};

    fn file_snapshot(paths: Vec<PathBuf>, cut: bool) -> ClipboardSnapshot {
        ClipboardSnapshot {
            files: Some(FileClipboard { paths, cut }),
            content_kind: None,
        }
    }

    #[test]
    fn paste_target_rejects_self_and_descendant() {
        assert!(target_is_source_or_descendant(
            Path::new("/home/user/folder"),
            Path::new("/home/user/folder")
        ));
        assert!(target_is_source_or_descendant(
            Path::new("/home/user/folder"),
            Path::new("/home/user/folder/child")
        ));
        assert!(!target_is_source_or_descendant(
            Path::new("/home/user/folder"),
            Path::new("/home/user/other")
        ));
    }

    #[test]
    fn paste_filters_missing_clipboard_paths_before_queueing() {
        let paths = vec![
            PathBuf::from("/tmp/exists-a"),
            PathBuf::from("/tmp/missing"),
            PathBuf::from("/tmp/exists-b"),
        ];
        let (existing, missing) =
            existing_clipboard_paths(&paths, |path| path.to_string_lossy().contains("exists"));

        assert_eq!(
            existing,
            vec![
                PathBuf::from("/tmp/exists-a"),
                PathBuf::from("/tmp/exists-b")
            ]
        );
        assert_eq!(missing, 1);
    }

    #[test]
    fn clipboard_context_inside_multiselection_copies_all_selected_paths() {
        let selected = vec![
            "/tmp/a".to_string(),
            "/tmp/b".to_string(),
            "/tmp/a".to_string(),
        ];

        assert_eq!(
            clipboard_paths_for_context(&selected, "/tmp/b"),
            vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
        );
    }

    #[test]
    fn clipboard_context_outside_multiselection_copies_only_context_path() {
        let selected = vec!["/tmp/a".to_string(), "/tmp/b".to_string()];

        assert_eq!(
            clipboard_paths_for_context(&selected, "/tmp/c"),
            vec![PathBuf::from("/tmp/c")]
        );
    }

    #[test]
    fn clipboard_context_uses_focused_pane_selection() {
        let mut state = AppState::new(PathBuf::from("/tmp/left"), Vec::new());
        state.panes.focused_mut().selection.paths =
            vec!["/tmp/left/a".to_string(), "/tmp/left/b".to_string()];
        assert!(state.panes.open_pane(PathBuf::from("/tmp/right")));
        assert!(state.panes.focus_slot(1));
        state
            .panes
            .pane_mut_for_slot(1)
            .expect("inactive pane")
            .selection
            .paths = vec!["/tmp/right/a".to_string(), "/tmp/right/b".to_string()];

        assert_eq!(
            clipboard_paths_for_focused_context(&state, "/tmp/right/b"),
            vec![PathBuf::from("/tmp/right/a"), PathBuf::from("/tmp/right/b")]
        );
        assert_eq!(
            clipboard_paths_for_focused_context(&state, "/tmp/left/b"),
            vec![PathBuf::from("/tmp/left/b")]
        );
    }

    #[test]
    fn desktop_clipboard_replaces_stale_internal_clipboard() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_paths = vec![PathBuf::from("/tmp/old")];
        state.clipboard_cut = false;

        merge_desktop_clipboard(
            &mut state,
            file_snapshot(
                vec![PathBuf::from("/tmp/new-a"), PathBuf::from("/tmp/new-b")],
                true,
            ),
            |_| true,
        );

        assert_eq!(
            state.clipboard_paths,
            vec![PathBuf::from("/tmp/new-a"), PathBuf::from("/tmp/new-b")]
        );
        assert!(state.clipboard_cut);
    }

    #[test]
    fn clipboard_paths_are_deduplicated_without_reordering() {
        assert_eq!(
            unique_paths(vec![
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/b"),
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/c"),
                PathBuf::from("/tmp/b"),
            ]),
            vec![
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/b"),
                PathBuf::from("/tmp/c"),
            ]
        );
    }

    #[test]
    fn desktop_clipboard_merge_deduplicates_paths() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        merge_desktop_clipboard(
            &mut state,
            file_snapshot(
                vec![
                    PathBuf::from("/tmp/new-a"),
                    PathBuf::from("/tmp/new-b"),
                    PathBuf::from("/tmp/new-a"),
                ],
                false,
            ),
            |_| true,
        );

        assert_eq!(
            state.clipboard_paths,
            vec![PathBuf::from("/tmp/new-a"), PathBuf::from("/tmp/new-b")]
        );
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn desktop_clipboard_merge_filters_missing_paths_and_clears_empty_cut() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        let missing = merge_desktop_clipboard(
            &mut state,
            file_snapshot(
                vec![
                    PathBuf::from("/tmp/exists-a"),
                    PathBuf::from("/tmp/missing"),
                    PathBuf::from("/tmp/exists-b"),
                ],
                true,
            ),
            |path| path.to_string_lossy().contains("exists"),
        );

        assert_eq!(missing, 1);
        assert_eq!(
            state.clipboard_paths,
            vec![
                PathBuf::from("/tmp/exists-a"),
                PathBuf::from("/tmp/exists-b")
            ]
        );
        assert!(state.clipboard_cut);

        let missing = merge_desktop_clipboard(
            &mut state,
            file_snapshot(vec![PathBuf::from("/tmp/missing")], true),
            |_| false,
        );

        assert_eq!(missing, 1);
        assert!(state.clipboard_paths.is_empty());
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn stale_async_clipboard_load_does_not_replace_newer_internal_clipboard() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let stale_generation = state.clipboard_generation.next();
        state.clipboard_generation.next();
        state.clipboard_paths = vec![PathBuf::from("/tmp/internal")];
        state.clipboard_cut = true;

        let applied = apply_clipboard_load(
            &mut state,
            ClipboardLoadResult {
                generation: stale_generation,
                result: Ok(file_snapshot(vec![PathBuf::from("/tmp/external")], false)),
            },
            |_| true,
        );

        assert!(!applied);
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/internal")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn current_async_clipboard_load_updates_cached_paste_state() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.clipboard_generation.next();

        let applied = apply_clipboard_load(
            &mut state,
            ClipboardLoadResult {
                generation,
                result: Ok(file_snapshot(vec![PathBuf::from("/tmp/external")], false)),
            },
            |_| true,
        );

        assert!(applied);
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/external")]);
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn clipboard_availability_refresh_is_single_flight_until_result_returns() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        let first = begin_clipboard_availability_refresh(&mut state).unwrap();
        assert_eq!(state.clipboard_refresh_pending, Some(first));
        assert_eq!(begin_clipboard_availability_refresh(&mut state), None);
        assert!(state.clipboard_generation.is_current(first));

        let applied = apply_clipboard_load(
            &mut state,
            ClipboardLoadResult {
                generation: first,
                result: Err("Wayland selection unavailable".to_string()),
            },
            |_| true,
        );

        assert!(!applied);
        assert_eq!(state.clipboard_refresh_pending, None);

        let second = begin_clipboard_availability_refresh(&mut state).unwrap();
        assert_eq!(second, first + 1);
        assert_eq!(state.clipboard_refresh_pending, Some(second));
    }

    #[test]
    fn stale_availability_result_clears_pending_without_replacing_internal_clipboard() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let pending = begin_clipboard_availability_refresh(&mut state).unwrap();
        state.clipboard_generation.next();
        state.clipboard_paths = vec![PathBuf::from("/tmp/internal")];
        state.clipboard_cut = true;

        let applied = apply_clipboard_load(
            &mut state,
            ClipboardLoadResult {
                generation: pending,
                result: Ok(file_snapshot(vec![PathBuf::from("/tmp/external")], false)),
            },
            |_| true,
        );

        assert!(!applied);
        assert_eq!(state.clipboard_refresh_pending, None);
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/internal")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn desktop_clipboard_content_snapshot_clears_stale_file_paths() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_paths = vec![PathBuf::from("/tmp/old")];
        state.clipboard_cut = true;

        merge_desktop_clipboard(
            &mut state,
            ClipboardSnapshot {
                files: None,
                content_kind: Some(ClipboardContentKind::Image),
            },
            |_| true,
        );

        assert!(state.clipboard_paths.is_empty());
        assert!(!state.clipboard_cut);
        assert_eq!(
            state.clipboard_content_kind,
            Some(ClipboardContentKind::Image)
        );
    }

    #[test]
    fn paste_load_uses_desktop_clipboard_without_blocking_ui_thread() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_paths = vec![PathBuf::from("/tmp/internal")];
        state.clipboard_cut = true;
        let generation = state.clipboard_generation.next();

        let plan = prepare_clipboard_paste(
            &mut state,
            ClipboardPasteLoadResult {
                generation,
                target_dir: PathBuf::from("/tmp/target"),
                result: Ok(file_snapshot(vec![PathBuf::from("/tmp/external")], false)),
            },
            |_| true,
        );

        assert_eq!(
            plan,
            ClipboardPastePlan::Files {
                operation: "copy",
                paths: vec![PathBuf::from("/tmp/external")],
                pruned_missing: false,
            }
        );
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/external")]);
        assert!(!state.clipboard_cut);
    }

    #[test]
    fn paste_load_uses_current_internal_clipboard_on_wayland_read_error() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_paths = vec![PathBuf::from("/tmp/internal")];
        state.clipboard_cut = true;
        let generation = state.clipboard_generation.next();

        let plan = prepare_clipboard_paste(
            &mut state,
            ClipboardPasteLoadResult {
                generation,
                target_dir: PathBuf::from("/tmp/target"),
                result: Err("Wayland data-control clipboard protocol is not available".to_string()),
            },
            |_| true,
        );

        assert_eq!(
            plan,
            ClipboardPastePlan::Files {
                operation: "move",
                paths: vec![PathBuf::from("/tmp/internal")],
                pruned_missing: false,
            }
        );
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/internal")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn stale_paste_load_does_not_apply_or_queue_internal_clipboard() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let stale_generation = state.clipboard_generation.next();
        state.clipboard_generation.next();
        state.clipboard_paths = vec![PathBuf::from("/tmp/internal")];
        state.clipboard_cut = true;

        let plan = prepare_clipboard_paste(
            &mut state,
            ClipboardPasteLoadResult {
                generation: stale_generation,
                target_dir: PathBuf::from("/tmp/target"),
                result: Ok(file_snapshot(vec![PathBuf::from("/tmp/external")], false)),
            },
            |_| true,
        );

        assert_eq!(plan, ClipboardPastePlan::Stale);
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/internal")]);
        assert!(state.clipboard_cut);
    }

    #[test]
    fn paste_into_does_not_synchronously_read_wayland_clipboard() {
        let source = include_str!("file_clipboard.rs");
        let paste_section = source
            .split_once("fn paste_into(")
            .expect("paste_into should exist")
            .1
            .split_once("fn start_file_clipboard_paste(")
            .expect("start_file_clipboard_paste should follow paste_into")
            .0;

        assert!(
            !paste_section.contains("read_clipboard_snapshot"),
            "paste_into must request an async clipboard read instead of blocking the UI thread"
        );
        assert!(
            source.contains("AsyncEvent::ClipboardPasteLoaded"),
            "paste requests should resume through the async event bridge"
        );
    }
}
