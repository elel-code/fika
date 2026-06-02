use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::{AsyncEvent, ClipboardLoadResult};
use crate::app::state::AppState;
use crate::app::transfer::{
    TransferStart, clear_accepted_cut_source, start_transfer_operation,
    target_is_source_or_descendant,
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
}

pub(crate) fn sync_clipboard_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    ui.set_clipboard_has_paths(!state.clipboard_paths.is_empty());
    ui.set_clipboard_cut(state.clipboard_cut);
}

fn copy_paths(ui: &AppWindow, state: &Rc<RefCell<AppState>>, context_path: &str, cut: bool) {
    let paths = {
        let state = state.borrow();
        clipboard_paths_for_context(&state.selected_paths, context_path)
    };

    if paths.is_empty() {
        set_status(ui, "Nothing selected");
        return;
    }

    {
        let mut state = state.borrow_mut();
        state.clipboard_generation.next();
        state.clipboard_paths = paths;
        state.clipboard_cut = cut;
    }
    sync_clipboard_ui(ui, state);

    let desktop_clipboard = clipboard::copy_file_list(&state.borrow().clipboard_paths, cut);
    let count = state.borrow().clipboard_paths.len();
    let action = if cut { "Cut" } else { "Copied" };
    match desktop_clipboard {
        Ok(helper) => set_status(ui, &format!("{action} {count} item(s) via {helper}")),
        Err(err) => set_status(
            ui,
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
        set_status(ui, "Paste target is not a folder");
        return;
    }

    let (operation, paths, pruned_missing) = {
        if state.borrow().clipboard_paths.is_empty() {
            refresh_clipboard_availability(ui, state);
        }
        let mut state_ref = state.borrow_mut();
        if state_ref.clipboard_paths.is_empty() {
            set_status(ui, "Clipboard is empty");
            return;
        }
        let (existing_paths, missing_count) =
            existing_clipboard_paths(&state_ref.clipboard_paths, file_ops::path_exists);
        if missing_count > 0 {
            state_ref.clipboard_paths = existing_paths;
            if state_ref.clipboard_paths.is_empty() {
                state_ref.clipboard_cut = false;
                drop(state_ref);
                sync_clipboard_ui(ui, state);
                set_status(ui, "Clipboard item(s) no longer exist");
                return;
            }
        }
        (
            if state_ref.clipboard_cut {
                "move"
            } else {
                "copy"
            },
            state_ref.clipboard_paths.clone(),
            missing_count > 0,
        )
    };
    if pruned_missing {
        sync_clipboard_ui(ui, state);
    }

    let mut accepted = 0usize;
    let mut cut_clipboard_changed = false;
    for path in &paths {
        if target_is_source_or_descendant(path, &target_dir) {
            set_status(ui, "Cannot paste an item into itself");
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
                cut_clipboard_changed |=
                    clear_accepted_cut_source(&mut state.borrow_mut(), operation, path);
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
        set_status(ui, &format!("Queued {accepted} paste operation(s)"));
    }
}

pub(crate) fn refresh_clipboard_availability_async(
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let generation = state.borrow_mut().clipboard_generation.next();
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = tokio::task::spawn_blocking(clipboard::read_file_list)
            .await
            .unwrap_or_else(|err| Err(format!("clipboard read task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ClipboardLoaded(ClipboardLoadResult { generation, result }),
        );
    });
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

fn refresh_clipboard_availability(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let generation = state.borrow_mut().clipboard_generation.next();
    if let Ok(clipboard) = clipboard::read_file_list() {
        let mut state_ref = state.borrow_mut();
        apply_clipboard_load(
            &mut state_ref,
            ClipboardLoadResult {
                generation,
                result: Ok(clipboard),
            },
            file_ops::path_exists,
        );
    }
    sync_clipboard_ui(ui, state);
}

fn apply_clipboard_load(
    state: &mut AppState,
    result: ClipboardLoadResult,
    exists: impl Fn(&Path) -> bool,
) -> bool {
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
    clipboard: clipboard::FileClipboard,
    exists: impl Fn(&Path) -> bool,
) -> usize {
    let paths = unique_paths(clipboard.paths);
    let (existing_paths, missing_count) = existing_clipboard_paths(&paths, exists);
    state.clipboard_paths = existing_paths;
    state.clipboard_cut = clipboard.cut && !state.clipboard_paths.is_empty();
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
        apply_clipboard_load, clipboard_paths_for_context, existing_clipboard_paths,
        merge_desktop_clipboard, unique_paths,
    };
    use crate::app::events::ClipboardLoadResult;
    use crate::app::state::AppState;
    use crate::app::transfer::target_is_source_or_descendant;
    use crate::desktop::clipboard::FileClipboard;
    use std::path::{Path, PathBuf};

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
    fn desktop_clipboard_replaces_stale_internal_clipboard() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.clipboard_paths = vec![PathBuf::from("/tmp/old")];
        state.clipboard_cut = false;

        merge_desktop_clipboard(
            &mut state,
            FileClipboard {
                paths: vec![PathBuf::from("/tmp/new-a"), PathBuf::from("/tmp/new-b")],
                cut: true,
                helper: "test".to_string(),
            },
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
            FileClipboard {
                paths: vec![
                    PathBuf::from("/tmp/new-a"),
                    PathBuf::from("/tmp/new-b"),
                    PathBuf::from("/tmp/new-a"),
                ],
                cut: false,
                helper: "test".to_string(),
            },
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
            FileClipboard {
                paths: vec![
                    PathBuf::from("/tmp/exists-a"),
                    PathBuf::from("/tmp/missing"),
                    PathBuf::from("/tmp/exists-b"),
                ],
                cut: true,
                helper: "test".to_string(),
            },
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
            FileClipboard {
                paths: vec![PathBuf::from("/tmp/missing")],
                cut: true,
                helper: "test".to_string(),
            },
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
                result: Ok(FileClipboard {
                    paths: vec![PathBuf::from("/tmp/external")],
                    cut: false,
                    helper: "test".to_string(),
                }),
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
                result: Ok(FileClipboard {
                    paths: vec![PathBuf::from("/tmp/external")],
                    cut: false,
                    helper: "test".to_string(),
                }),
            },
            |_| true,
        );

        assert!(applied);
        assert_eq!(state.clipboard_paths, vec![PathBuf::from("/tmp/external")]);
        assert!(!state.clipboard_cut);
    }
}
