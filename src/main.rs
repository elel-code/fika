mod ui;

use fika_core::{
    CompactLayout, CreateUndoItem, CreatedItemKind, DirectoryListerEvent, Entry, OperationQueue,
    PaneController, PaneId, RenameUndoItem, SelectionMove, TransferUndoItem, TrashUndoItem,
    UndoPayload, UndoRecord, ViewPoint, ViewRect, ViewState, file_ops, nearest_existing_ancestor,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, IntoElement, ParentElement, Render, Styled, Window, WindowBounds,
    WindowOptions, div, px, rgb, size,
};
use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Manager,
    Chooser,
}

#[derive(Clone, Debug)]
struct Args {
    mode: Mode,
    start_dir: PathBuf,
    chooser_directories: bool,
    chooser_multiple: bool,
    chooser_title: Option<String>,
    chooser_accept_label: Option<String>,
    chooser_filter_index: usize,
    chooser_return_filter: bool,
    chooser_choices: Vec<String>,
    chooser_return_choices: bool,
}

impl Args {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut mode = Mode::Manager;
        let mut start_dir = None;
        let mut chooser_directories = false;
        let mut chooser_multiple = false;
        let mut chooser_title = None;
        let mut chooser_accept_label = None;
        let mut chooser_filter_index = 0usize;
        let mut chooser_return_filter = false;
        let mut chooser_choices = Vec::new();
        let mut chooser_return_choices = false;
        let mut pending_title = false;
        let mut pending_accept_label = false;
        let mut pending_filter_index = false;
        let mut pending_choices = false;
        let mut skip_next = false;

        for arg in args {
            if skip_next {
                skip_next = false;
                continue;
            }
            if pending_title {
                chooser_title = (!arg.is_empty()).then_some(arg);
                pending_title = false;
                continue;
            }
            if pending_accept_label {
                chooser_accept_label = (!arg.is_empty()).then_some(arg);
                pending_accept_label = false;
                continue;
            }
            if pending_filter_index {
                chooser_filter_index = arg.parse().unwrap_or_default();
                pending_filter_index = false;
                continue;
            }
            if pending_choices {
                chooser_choices = arg
                    .split('\n')
                    .filter(|choice| !choice.is_empty())
                    .map(str::to_string)
                    .collect();
                pending_choices = false;
                continue;
            }

            match arg.as_str() {
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--chooser" => mode = Mode::Chooser,
                "--chooser-directory" => {
                    mode = Mode::Chooser;
                    chooser_directories = true;
                }
                "--chooser-multiple" => {
                    mode = Mode::Chooser;
                    chooser_multiple = true;
                }
                "--chooser-save"
                | "--chooser-save-files"
                | "--chooser-filters"
                | "--chooser-parent-window" => {
                    mode = Mode::Chooser;
                    skip_next = true;
                }
                "--chooser-title" => {
                    mode = Mode::Chooser;
                    pending_title = true;
                }
                "--chooser-accept-label" => {
                    mode = Mode::Chooser;
                    pending_accept_label = true;
                }
                "--chooser-filter-index" => {
                    mode = Mode::Chooser;
                    pending_filter_index = true;
                }
                "--chooser-return-filter" => {
                    mode = Mode::Chooser;
                    chooser_return_filter = true;
                }
                "--chooser-choices" => {
                    mode = Mode::Chooser;
                    pending_choices = true;
                }
                "--chooser-return-choices" => {
                    mode = Mode::Chooser;
                    chooser_return_choices = true;
                }
                _ if start_dir.is_none() => start_dir = Some(expand_user_path(&arg)),
                _ => {}
            }
        }

        let start_dir = normalize_start_dir(start_dir.unwrap_or_else(home_dir));
        Self {
            mode,
            start_dir,
            chooser_directories,
            chooser_multiple,
            chooser_title,
            chooser_accept_label,
            chooser_filter_index,
            chooser_return_filter,
            chooser_choices,
            chooser_return_choices,
        }
    }
}

#[derive(Clone, Debug)]
struct ChooserState {
    directories: bool,
    multiple: bool,
    title: String,
    accept_label: String,
    filter_index: usize,
    return_filter: bool,
    choices: Vec<String>,
    return_choices: bool,
}

#[derive(Clone, Debug)]
struct PaneSnapshot {
    id: PaneId,
    path: PathBuf,
    entries: Vec<Entry>,
    view: ViewState,
    rubber_band: Option<ViewRect>,
    selected_paths: BTreeSet<PathBuf>,
    rename_draft: Option<RenameDraftSnapshot>,
    focused: bool,
    can_close: bool,
    can_go_back: bool,
    can_go_forward: bool,
    can_paste: bool,
    can_rename: bool,
    can_undo: bool,
    operation_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RubberBandState {
    pane_id: PaneId,
    start: ViewPoint,
    current: ViewPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RubberBandDrag {
    pane_id: PaneId,
}

impl RubberBandState {
    fn rect(self) -> ViewRect {
        let x = self.start.x.min(self.current.x);
        let y = self.start.y.min(self.current.y);
        ViewRect {
            x,
            y,
            width: self.start.x.max(self.current.x) - x,
            height: self.start.y.max(self.current.y) - y,
        }
    }

    fn visible_rect(self, scroll_x: f32, scroll_y: f32) -> ViewRect {
        let rect = self.rect();
        ViewRect {
            x: rect.x - scroll_x,
            y: rect.y - scroll_y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenameDraft {
    pane_id: PaneId,
    original_path: PathBuf,
    draft_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenameDraftSnapshot {
    original_path: PathBuf,
    draft_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClipboardMode {
    Copy,
    Cut,
}

impl ClipboardMode {
    fn operation(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "move",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Cut => "Move",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClipboardState {
    mode: ClipboardMode,
    paths: Vec<PathBuf>,
}

pub(crate) struct FikaApp {
    pub(crate) panes: PaneController,
    operations: OperationQueue,
    clipboard: Option<ClipboardState>,
    rename_draft: Option<RenameDraft>,
    chooser: Option<ChooserState>,
    _keystroke_subscription: Option<gpui::Subscription>,
    pub(crate) rubber_band: Option<RubberBandState>,
    operation_pending: bool,
    status: String,
}

impl FikaApp {
    fn new(args: Args, cx: &mut Context<Self>) -> Self {
        let chooser = (args.mode == Mode::Chooser).then(|| ChooserState {
            directories: args.chooser_directories,
            multiple: args.chooser_multiple,
            title: args
                .chooser_title
                .clone()
                .unwrap_or_else(|| "Fika File Chooser".to_string()),
            accept_label: args
                .chooser_accept_label
                .clone()
                .unwrap_or_else(|| "Choose".to_string()),
            filter_index: args.chooser_filter_index,
            return_filter: args.chooser_return_filter,
            choices: args.chooser_choices.clone(),
            return_choices: args.chooser_return_choices,
        });
        let mut app = Self {
            panes: PaneController::new(args.start_dir.clone()),
            operations: OperationQueue::new(),
            clipboard: None,
            rename_draft: None,
            chooser,
            _keystroke_subscription: None,
            rubber_band: None,
            operation_pending: false,
            status: String::new(),
        };
        app._keystroke_subscription = Some(cx.observe_keystrokes(|this, event, _window, cx| {
            if this.handle_keystroke(event, cx) {
                cx.notify();
            }
        }));
        let first = app.panes.focused().expect("initial pane exists");
        app.load_pane(first, args.start_dir);
        app.start_watchers();
        cx.spawn(|this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    async_io::Timer::after(Duration::from_millis(350)).await;
                    if this
                        .update(&mut cx, |app, cx| {
                            if app.drain_watchers() {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
        .detach();
        app
    }

    fn snapshots(&self) -> Vec<PaneSnapshot> {
        let can_close = self.panes.pane_ids().len() > 1;
        let can_paste = !self.operation_pending && self.clipboard.is_some();
        let can_undo = !self.operation_pending && self.operations.latest_undo().is_some();
        self.panes
            .pane_ids()
            .iter()
            .filter_map(|pane_id| {
                let pane = self.panes.pane(*pane_id)?;
                let selected_paths = self
                    .panes
                    .selected_paths(*pane_id)
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                let rename_draft = self
                    .rename_draft
                    .as_ref()
                    .filter(|draft| draft.pane_id == *pane_id)
                    .map(|draft| RenameDraftSnapshot {
                        original_path: draft.original_path.clone(),
                        draft_name: draft.draft_name.clone(),
                    });
                Some(PaneSnapshot {
                    id: *pane_id,
                    path: pane.current_dir.clone(),
                    entries: pane.model.entries().to_vec(),
                    view: pane.view.clone(),
                    rubber_band: self.rubber_band.and_then(|band| {
                        (band.pane_id == *pane_id)
                            .then(|| band.visible_rect(pane.view.scroll_x, pane.view.scroll_y))
                    }),
                    selected_paths,
                    rename_draft,
                    focused: self.panes.focused() == Some(*pane_id),
                    can_close,
                    can_go_back: pane.can_go_back(),
                    can_go_forward: pane.can_go_forward(),
                    can_paste,
                    can_rename: !self.operation_pending && pane.selection.len() == 1,
                    can_undo,
                    operation_pending: self.operation_pending,
                })
            })
            .collect()
    }

    fn load_pane(&mut self, pane_id: PaneId, path: PathBuf) {
        self.clear_rename_draft_for_pane(pane_id);
        let Some(event) = self.panes.load(pane_id, path.clone()) else {
            return;
        };
        self.apply_event(event);
        self.start_watcher(pane_id);
        self.status = format!("Loaded {}", path.display());
    }

    fn reload_pane(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.reload(pane_id) else {
            return;
        };
        self.apply_event(event);
        self.start_watcher(pane_id);
        if let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        {
            self.status = format!("Reloaded {}", path.display());
        }
    }

    fn go_back(&mut self, pane_id: PaneId) {
        self.clear_rename_draft_for_pane(pane_id);
        let Some(event) = self.panes.go_back(pane_id) else {
            return;
        };
        let path = event.path().to_path_buf();
        self.apply_event(event);
        self.start_watcher(pane_id);
        self.status = format!("Back to {}", path.display());
    }

    fn go_forward(&mut self, pane_id: PaneId) {
        self.clear_rename_draft_for_pane(pane_id);
        let Some(event) = self.panes.go_forward(pane_id) else {
            return;
        };
        let path = event.path().to_path_buf();
        self.apply_event(event);
        self.start_watcher(pane_id);
        self.status = format!("Forward to {}", path.display());
    }

    fn go_parent(&mut self, pane_id: PaneId) {
        let Some(parent) = self
            .panes
            .pane(pane_id)
            .and_then(|pane| pane.current_dir.parent().map(Path::to_path_buf))
        else {
            return;
        };
        self.load_pane(pane_id, parent);
    }

    fn split_pane(&mut self, pane_id: PaneId) {
        let Some(new_id) = self.panes.split(pane_id) else {
            return;
        };
        if let Some(path) = self.panes.pane(new_id).map(|pane| pane.current_dir.clone()) {
            self.load_pane(new_id, path);
        }
    }

    fn close_pane(&mut self, pane_id: PaneId) {
        if self.panes.close(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.status = format!("Closed pane {}", pane_id.0);
        }
    }

    fn select_only(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.select_only(pane_id, path) {
            self.clear_rename_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.status = format!("{selected} selected");
        }
    }

    fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.toggle_selection(pane_id, path).is_some() {
            self.clear_rename_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.status = format!("{selected} selected");
        }
    }

    fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) {
        if let Some(selected) = self.panes.select_range_to(pane_id, path) {
            self.clear_rename_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn select_all(&mut self, pane_id: PaneId) {
        if let Some(selected) = self.panes.select_all(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn clear_selection(&mut self, pane_id: PaneId) {
        if self.panes.clear_selection(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.status = "Selection cleared".to_string();
        }
    }

    fn move_selection(&mut self, pane_id: PaneId, direction: SelectionMove, extend: bool) {
        if let Some(selected) = self.panes.move_selection(pane_id, direction, extend) {
            self.clear_rename_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn start_rubber_band(&mut self, pane_id: PaneId, start: ViewPoint) {
        self.clear_rename_draft_for_pane(pane_id);
        self.rubber_band = Some(RubberBandState {
            pane_id,
            start,
            current: start,
        });
    }

    fn update_rubber_band(&mut self, pane_id: PaneId, current: ViewPoint, layout: CompactLayout) {
        let Some(mut band) = self.rubber_band else {
            return;
        };
        if band.pane_id != pane_id {
            return;
        }
        band.current = current;
        self.rubber_band = Some(band);
        let selection = layout.indexes_intersecting(band.rect());
        if let Some(selected) = self
            .panes
            .replace_selection_by_indexes(pane_id, selection.indexes().iter().copied())
        {
            self.status = format!("{selected} selected");
        }
    }

    fn finish_rubber_band(&mut self, pane_id: PaneId) {
        if self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            self.rubber_band = None;
        }
    }

    fn clear_rename_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .rename_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.rename_draft = None;
        }
    }

    fn start_rename_in_pane(&mut self, pane_id: PaneId) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        let [original_path] = selected_paths.as_slice() else {
            self.status = "Select one item to rename".to_string();
            return;
        };
        let Some(name) = original_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
        else {
            self.status = "Selected item cannot be renamed".to_string();
            return;
        };

        self.rename_draft = Some(RenameDraft {
            pane_id,
            original_path: original_path.clone(),
            draft_name: name.to_string(),
        });
        self.status = format!("Renaming {name}");
    }

    fn handle_rename_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft_pane_id) = self.rename_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match rename_input_action(keystroke) {
            RenameInputAction::Cancel => {
                self.rename_draft = None;
                self.status = "Rename cancelled".to_string();
            }
            RenameInputAction::Commit => self.commit_rename_draft(cx),
            RenameInputAction::Backspace => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.draft_name.pop();
                }
            }
            RenameInputAction::Insert(text) => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.draft_name.push_str(&text);
                }
            }
            RenameInputAction::Ignore => {}
        }
        true
    }

    fn commit_rename_draft(&mut self, cx: &mut Context<Self>) {
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(draft) = self.rename_draft.take() else {
            return;
        };
        let new_name = draft.draft_name.trim().to_string();
        if new_name.is_empty() {
            self.status = "Name cannot be empty".to_string();
            return;
        }
        if draft
            .original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == new_name)
        {
            let _ = self
                .panes
                .select_only(draft.pane_id, draft.original_path.clone());
            self.status = "Rename unchanged".to_string();
            return;
        }

        self.operation_pending = true;
        self.status = format!("Renaming {}", draft.original_path.display());
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            rename_item_result(draft.pane_id, draft.original_path, new_name)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_rename_item(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_rename_item(&mut self, result: RenameItemResult) {
        self.operation_pending = false;
        match result.result {
            Ok(renamed_path) => {
                self.operations.register_undo_with_payload(
                    "Rename".to_string(),
                    result.affected_dirs.clone(),
                    UndoPayload::Rename {
                        items: vec![RenameUndoItem {
                            original_path: result.original_path.clone(),
                            renamed_path: renamed_path.clone(),
                        }],
                    },
                );
                self.refresh_affected_dirs(&result.affected_dirs);
                let _ = self.panes.select_only(result.pane_id, renamed_path.clone());
                self.status = format!("Renamed to {}", renamed_path.display());
            }
            Err(err) => {
                self.status = format!("Cannot rename {}: {err}", result.original_path.display());
            }
        }
    }

    fn create_item_in_pane(
        &mut self,
        pane_id: PaneId,
        kind: CreatedItemKind,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(parent_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        self.operation_pending = true;
        self.status = format!("Creating {}", created_item_label(kind).to_ascii_lowercase());
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(
                            async move { create_item_result(pane_id, parent_dir, kind) },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_create_item(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_create_item(&mut self, result: CreateItemResult) {
        self.operation_pending = false;
        match result.result {
            Ok(path) => {
                self.operations.register_undo_with_payload(
                    format!("Create {}", created_item_label(result.kind)),
                    result.affected_dirs.clone(),
                    UndoPayload::Create {
                        items: vec![CreateUndoItem {
                            path: path.clone(),
                            kind: result.kind,
                        }],
                    },
                );
                self.refresh_affected_dirs(&result.affected_dirs);
                let _ = self.panes.select_only(result.pane_id, path.clone());
                self.status = format!("Created {}", path.display());
            }
            Err(err) => {
                self.status = format!(
                    "Cannot create {}: {err}",
                    created_item_label(result.kind).to_ascii_lowercase()
                );
            }
        }
    }

    fn store_selection_for_transfer(&mut self, pane_id: PaneId, mode: ClipboardMode) {
        if self.chooser.is_some() {
            return;
        }
        let paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if paths.is_empty() {
            self.status = format!("No selection to {}", mode.label().to_ascii_lowercase());
            return;
        }

        let count = paths.len();
        self.clipboard = Some(ClipboardState { mode, paths });
        self.status = format!("{} {} item(s)", mode.label(), count);
    }

    fn paste_into_pane(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(clipboard) = self.clipboard.clone() else {
            self.status = "Nothing to paste".to_string();
            return;
        };
        let Some(target_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        self.operation_pending = true;
        self.status = format!(
            "{}ing {} item(s)",
            clipboard.mode.label(),
            clipboard.paths.len()
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            paste_clipboard_result(pane_id, target_dir, clipboard)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_paste(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_paste(&mut self, result: PasteTaskResult) {
        self.operation_pending = false;
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                result.mode.label().to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Transfer {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            if result.mode == ClipboardMode::Cut {
                self.clipboard = None;
                let _ = self.panes.clear_selection(result.pane_id);
            }
        }

        self.status = action_status(
            &format!("{} complete", result.mode.label()),
            result.success_count,
            result.failure_count,
        );
    }

    fn trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.status = "No selection to trash".to_string();
            return;
        }

        self.operation_pending = true;
        self.status = format!("Moving {} item(s) to trash", selected_paths.len());
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(
                            async move { trash_selection_result(pane_id, selected_paths) },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_selection(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_trash_selection(&mut self, result: TrashSelectionResult) {
        self.operation_pending = false;
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                "Move to Trash".to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Trash {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            let _ = self.panes.clear_selection(result.pane_id);
        }

        self.status = action_status("Moved to trash", result.success_count, result.failure_count);
    }

    fn undo_latest(&mut self, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(record) = self.operations.latest_undo().cloned() else {
            self.status = "No operation to undo".to_string();
            return;
        };

        match &record.payload {
            UndoPayload::Create { .. } => {}
            UndoPayload::Rename { .. } => {}
            UndoPayload::Trash { .. } => {}
            UndoPayload::Transfer { .. } => {}
            UndoPayload::None => {
                self.status = format!("No undo action for {}", record.label);
                return;
            }
        }

        self.operation_pending = true;
        self.status = format!("Undoing {}", record.label);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move { undo_record_result(record) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_undo(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_undo(&mut self, result: UndoTaskResult) {
        self.operation_pending = false;
        match result.result {
            Ok(message) => {
                if self
                    .operations
                    .take_latest_undo(result.record.serial)
                    .is_none()
                {
                    self.status = "Undo result is stale".to_string();
                    return;
                }
                self.refresh_affected_dirs(&result.record.affected_dirs);
                self.status = format!("Undid {}: {message}", result.record.label);
            }
            Err(err) => {
                self.status = format!("Cannot undo {}: {err}", result.record.label);
            }
        }
    }

    fn refresh_affected_dirs(&mut self, affected_dirs: &[PathBuf]) {
        for refresh in OperationQueue::refresh_affected_panes(&mut self.panes, affected_dirs) {
            self.apply_event(refresh.event);
            self.start_watcher(refresh.pane_id);
        }
    }

    fn handle_keystroke(&mut self, event: &gpui::KeystrokeEvent, cx: &mut Context<Self>) -> bool {
        if self.handle_rename_keystroke(&event.keystroke, cx) {
            return true;
        }
        let Some(pane_id) = self.panes.focused() else {
            return false;
        };
        match pane_shortcut(&event.keystroke) {
            Some(PaneShortcut::SelectAll) => self.select_all(pane_id),
            Some(PaneShortcut::ClearSelection) => self.clear_selection(pane_id),
            Some(PaneShortcut::Refresh) => self.reload_pane(pane_id),
            Some(PaneShortcut::GoParent) => self.go_parent(pane_id),
            Some(PaneShortcut::GoBack) => self.go_back(pane_id),
            Some(PaneShortcut::GoForward) => self.go_forward(pane_id),
            Some(PaneShortcut::MoveSelection { direction, extend }) => {
                self.move_selection(pane_id, direction, extend)
            }
            Some(PaneShortcut::CreateFolder) => {
                self.create_item_in_pane(pane_id, CreatedItemKind::Folder, cx)
            }
            Some(PaneShortcut::RenameSelection) => self.start_rename_in_pane(pane_id),
            Some(PaneShortcut::CopySelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Copy)
            }
            Some(PaneShortcut::CutSelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Cut)
            }
            Some(PaneShortcut::PasteIntoPane) => self.paste_into_pane(pane_id, cx),
            Some(PaneShortcut::TrashSelection) => self.trash_selection(pane_id, cx),
            Some(PaneShortcut::Undo) => self.undo_latest(cx),
            None => return false,
        }
        true
    }

    fn confirm_chooser(&mut self) {
        if self.chooser.is_none() {
            return;
        }
        let selected_paths = self
            .panes
            .focused()
            .and_then(|pane_id| self.panes.selected_paths(pane_id))
            .unwrap_or_default();
        if selected_paths.is_empty() {
            if self
                .chooser
                .as_ref()
                .is_some_and(|chooser| chooser.directories)
            {
                if let Some(path) = self
                    .panes
                    .focused()
                    .and_then(|pane_id| self.panes.pane(pane_id))
                    .map(|pane| pane.current_dir.clone())
                {
                    self.choose_path(path);
                    return;
                }
            }
            self.status = "No chooser selection".to_string();
            return;
        }
        self.choose_paths(selected_paths);
    }

    fn choose_path(&mut self, path: PathBuf) {
        self.choose_paths(vec![path]);
    }

    fn choose_paths(&mut self, paths: Vec<PathBuf>) {
        if let Some(chooser) = &self.chooser {
            if chooser.return_filter {
                println!("FIKA_CHOOSER_FILTER\t{}", chooser.filter_index);
            }
            if chooser.return_choices {
                for choice in selected_choice_rows(&chooser.choices) {
                    println!("{choice}");
                }
            }
        }
        for path in paths {
            println!("{}", path.display());
        }
        std::process::exit(0);
    }

    fn apply_event(&mut self, event: DirectoryListerEvent) {
        if let DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } = &event {
            let still_current = self.panes.pane(*pane_id).is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            });
            if still_current {
                let fallback =
                    nearest_existing_ancestor(path).unwrap_or_else(|| PathBuf::from("/"));
                self.status = format!("{} was removed", path.display());
                self.load_pane(*pane_id, fallback);
            }
            return;
        }

        if let Some(signals) = self.panes.apply_lister_event(event) {
            if !signals.is_empty() {
                self.status = format!("{} model signal(s)", signals.len());
            }
        }
    }

    fn start_watchers(&mut self) {
        for pane_id in self.panes.pane_ids().to_vec() {
            self.start_watcher(pane_id);
        }
    }

    fn start_watcher(&mut self, pane_id: PaneId) {
        let Some(pane) = self.panes.pane_mut(pane_id) else {
            return;
        };
        if let Err(err) = pane.lister.start_watcher() {
            self.status = format!("Cannot watch {}: {err}", pane.current_dir.display());
        }
    }

    fn drain_watchers(&mut self) -> bool {
        let mut changed = false;
        let pane_ids = self.panes.pane_ids().to_vec();
        for pane_id in pane_ids {
            let events = self
                .panes
                .pane_mut(pane_id)
                .map(|pane| pane.lister.drain_watcher_events())
                .unwrap_or_default();
            for event in events {
                self.apply_event(event);
                changed = true;
            }
        }
        changed
    }
}

impl Render for FikaApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .chooser
            .as_ref()
            .map(|chooser| chooser.title.as_str())
            .unwrap_or("Fika");
        window.set_window_title(title);
        let snapshots = self.snapshots();
        let manager_mode = self.chooser.is_none();
        let file_grid_mode =
            self.chooser
                .as_ref()
                .map_or(ui::file_grid::FileGridMode::Manager, |chooser| {
                    ui::file_grid::FileGridMode::Chooser {
                        directories: chooser.directories,
                        multiple: chooser.multiple,
                    }
                });
        let chooser_action_label = self.chooser.as_ref().map(|chooser| {
            let target = if chooser.directories {
                "folders"
            } else {
                "files"
            };
            let count = if chooser.multiple {
                "multiple"
            } else {
                "single"
            };
            format!("{} - {} {}", chooser.accept_label, count, target)
        });
        let pane_elements = snapshots
            .into_iter()
            .map(|snapshot| {
                ui::pane::pane_view(
                    ui::pane::PaneProps {
                        snapshot,
                        manager_mode,
                        file_grid_mode,
                    },
                    cx,
                )
            })
            .collect::<Vec<_>>();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf0f2f5))
            .text_color(rgb(0x1f2328))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0xc8ced6))
                    .bg(rgb(0xffffff))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(
                        if self.chooser.is_some() {
                            "Fika Chooser"
                        } else {
                            "Fika"
                        },
                    ))
                    .child(
                        div().text_sm().text_color(rgb(0x59636e)).child(
                            chooser_action_label
                                .clone()
                                .unwrap_or_else(|| "GPUI directory shell".to_string()),
                        ),
                    )
                    .when(self.chooser.is_some(), |bar| {
                        bar.child(ui::pane::toolbar_button("choose", "Choose").on_click(
                            cx.listener(move |this, _event, _window, cx| {
                                this.confirm_chooser();
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_2()
                    .flex_1()
                    .children(pane_elements),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(rgb(0xc8ced6))
                    .bg(rgb(0xffffff))
                    .text_xs()
                    .text_color(rgb(0x59636e))
                    .child(if self.status.is_empty() {
                        "Ready".to_string()
                    } else {
                        self.status.clone()
                    }),
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneShortcut {
    SelectAll,
    ClearSelection,
    Refresh,
    GoParent,
    GoBack,
    GoForward,
    MoveSelection {
        direction: SelectionMove,
        extend: bool,
    },
    CreateFolder,
    RenameSelection,
    CopySelection,
    CutSelection,
    PasteIntoPane,
    TrashSelection,
    Undo,
}

fn pane_shortcut(keystroke: &gpui::Keystroke) -> Option<PaneShortcut> {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => Some(PaneShortcut::ClearSelection),
            "f5" => Some(PaneShortcut::Refresh),
            "f2" => Some(PaneShortcut::RenameSelection),
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: false,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: false,
            }),
            "backspace" => Some(PaneShortcut::GoParent),
            "delete" => Some(PaneShortcut::TrashSelection),
            _ => None,
        };
    }

    if keystroke.modifiers.shift && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: true,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: true,
            }),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary()
        && keystroke.modifiers.shift
        && keystroke.modifiers.number_of_modifiers() == 2
    {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "n" => Some(PaneShortcut::CreateFolder),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary() && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "a" => Some(PaneShortcut::SelectAll),
            "c" => Some(PaneShortcut::CopySelection),
            "v" => Some(PaneShortcut::PasteIntoPane),
            "x" => Some(PaneShortcut::CutSelection),
            "z" => Some(PaneShortcut::Undo),
            _ => None,
        };
    }

    if keystroke.modifiers.alt && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "left" => Some(PaneShortcut::GoBack),
            "right" => Some(PaneShortcut::GoForward),
            _ => None,
        };
    }

    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RenameInputAction {
    Cancel,
    Commit,
    Backspace,
    Insert(String),
    Ignore,
}

fn rename_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => RenameInputAction::Cancel,
            "enter" => RenameInputAction::Commit,
            "backspace" => RenameInputAction::Backspace,
            _ => rename_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return rename_text_input_action(keystroke);
    }

    RenameInputAction::Ignore
}

fn rename_text_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| RenameInputAction::Insert(text.clone()))
        .unwrap_or(RenameInputAction::Ignore)
}

fn has_no_modifiers(keystroke: &gpui::Keystroke) -> bool {
    !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
}

#[derive(Clone, Debug)]
struct TrashSelectionResult {
    pane_id: PaneId,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    undo_items: Vec<TrashUndoItem>,
}

#[derive(Clone, Debug)]
struct PasteTaskResult {
    pane_id: PaneId,
    mode: ClipboardMode,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    undo_items: Vec<TransferUndoItem>,
}

#[derive(Clone, Debug)]
struct RenameItemResult {
    pane_id: PaneId,
    original_path: PathBuf,
    affected_dirs: Vec<PathBuf>,
    result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
struct CreateItemResult {
    pane_id: PaneId,
    kind: CreatedItemKind,
    affected_dirs: Vec<PathBuf>,
    result: Result<PathBuf, String>,
}

fn rename_item_result(
    pane_id: PaneId,
    original_path: PathBuf,
    new_name: String,
) -> RenameItemResult {
    let mut affected_dirs = parent_dirs([original_path.clone()]);
    let result = file_ops::rename_path(&original_path, &new_name);
    if let Ok(renamed_path) = &result
        && let Some(parent) = renamed_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        push_unique_path(&mut affected_dirs, parent.to_path_buf());
    }

    RenameItemResult {
        pane_id,
        original_path,
        affected_dirs,
        result,
    }
}

fn create_item_result(
    pane_id: PaneId,
    parent_dir: PathBuf,
    kind: CreatedItemKind,
) -> CreateItemResult {
    let result = match kind {
        CreatedItemKind::File => {
            file_ops::create_file(&parent_dir, default_created_item_name(kind))
        }
        CreatedItemKind::Folder => {
            file_ops::create_folder(&parent_dir, default_created_item_name(kind))
        }
    };
    CreateItemResult {
        pane_id,
        kind,
        affected_dirs: vec![parent_dir],
        result,
    }
}

fn default_created_item_name(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "New File.txt",
        CreatedItemKind::Folder => "New Folder",
    }
}

fn created_item_label(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "File",
        CreatedItemKind::Folder => "Folder",
    }
}

fn paste_clipboard_result(
    pane_id: PaneId,
    target_dir: PathBuf,
    clipboard: ClipboardState,
) -> PasteTaskResult {
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut undo_items = Vec::new();
    let operation = clipboard.mode.operation();

    for source in clipboard.paths {
        match file_ops::perform_transfer_with_progress_outcome(
            operation,
            &source,
            &target_dir,
            "keep-both",
            None,
            |_| {},
        ) {
            Ok(outcome) => {
                success_count += 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                if clipboard.mode == ClipboardMode::Cut
                    && let Some(parent) = source
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                {
                    push_unique_path(&mut affected_dirs, parent.to_path_buf());
                }
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(_) => {
                failure_count += 1;
            }
        }
    }

    PasteTaskResult {
        pane_id,
        mode: clipboard.mode,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
    }
}

fn trash_selection_result(pane_id: PaneId, selected_paths: Vec<PathBuf>) -> TrashSelectionResult {
    let summary = file_ops::trash_paths(&selected_paths);
    let success_count = summary.successes.len();
    let failure_count = summary.failures.len();
    let undo_items = summary
        .successes
        .iter()
        .map(|record| TrashUndoItem {
            original_path: record.original_path.clone(),
            trash_path: record.trash_path.clone(),
        })
        .collect::<Vec<_>>();
    let mut affected_dirs = parent_dirs(
        summary
            .successes
            .iter()
            .map(|record| record.original_path.clone()),
    );
    if success_count > 0 {
        push_unique_path(&mut affected_dirs, file_ops::trash_files_dir());
    }

    TrashSelectionResult {
        pane_id,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
    }
}

#[derive(Clone, Debug)]
struct UndoTaskResult {
    record: UndoRecord,
    result: Result<String, String>,
}

fn undo_record_result(record: UndoRecord) -> UndoTaskResult {
    let result = match &record.payload {
        UndoPayload::Create { items } => {
            let mut removed_count = 0;
            for item in items {
                let result = match item.kind {
                    CreatedItemKind::File => file_ops::undo_create_file(&item.path),
                    CreatedItemKind::Folder => file_ops::undo_create_folder(&item.path),
                };
                if let Err(err) = result {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "removed {removed_count} item(s), then failed: {err}"
                        )),
                    };
                }
                removed_count += 1;
            }
            Ok(format!("removed {} item(s)", items.len()))
        }
        UndoPayload::Trash { items } => {
            let restore_pairs = items
                .iter()
                .map(|item| (item.original_path.clone(), item.trash_path.clone()))
                .collect::<Vec<_>>();
            file_ops::undo_trash(&restore_pairs)
        }
        UndoPayload::Rename { items } => {
            let mut restored_count = 0;
            for item in items {
                if let Err(err) = file_ops::undo_rename(&item.original_path, &item.renamed_path) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
                restored_count += 1;
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::Transfer { items } => {
            let mut restored_count = 0;
            for item in items {
                if let Err(err) = file_ops::undo_transfer_with_backup(
                    &item.operation,
                    &item.original_source,
                    &item.destination,
                    item.overwritten_backup.as_deref(),
                ) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
                restored_count += 1;
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::None => Err(format!("no undo action for {}", record.label)),
    };
    UndoTaskResult { record, result }
}

fn parent_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for path in paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            push_unique_path(&mut dirs, parent.to_path_buf());
        }
    }
    dirs
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn action_status(label: &str, success_count: usize, failure_count: usize) -> String {
    match (success_count, failure_count) {
        (0, 0) => format!("{label}: no changes"),
        (_, 0) => format!("{label}: {success_count} item(s)"),
        (0, _) => format!("{label} failed for {failure_count} item(s)"),
        (_, _) => format!("{label}: {success_count} item(s), {failure_count} failed"),
    }
}

fn selected_choice_rows(specs: &[String]) -> Vec<String> {
    specs
        .iter()
        .filter_map(|spec| {
            let mut parts = spec.split('\t');
            let id = parts.next()?;
            let _label = parts.next()?;
            let default = parts.next().unwrap_or_default();
            let options = parts.next().unwrap_or_default();
            let selected = if default.is_empty() {
                options
                    .split(';')
                    .next()
                    .and_then(|option| option.split_once('=').map(|(value, _)| value))
                    .unwrap_or_default()
            } else {
                default
            };
            (!id.is_empty() && !selected.is_empty())
                .then(|| format!("FIKA_CHOOSER_CHOICE\t{id}\t{selected}"))
        })
        .collect()
}

fn normalize_start_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        path
    } else {
        path.parent()
            .map(|parent| {
                if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                }
            })
            .unwrap_or_else(home_dir)
    }
}

fn expand_user_path(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn print_help() {
    println!(
        "Usage: fika [--chooser] [START_DIR]\n\n\
         Options:\n\
           --chooser                 Start the GPUI file chooser shell.\n\
           --chooser-directory       Select folders instead of files.\n\
           --chooser-multiple        Select more than one path before confirmation.\n\
           --chooser-title TITLE     Use TITLE as the chooser window title.\n\
           --chooser-accept-label L  Use L in the chooser chrome.\n\
           --chooser-filter-index N  Return N as selected filter metadata.\n\
           --chooser-return-filter   Print selected filter metadata before paths.\n\
           --chooser-choices LIST    Preserve portal choice metadata.\n\
           --chooser-return-choices  Print selected choice metadata before paths.\n\
           -h, --help                Show this help."
    );
}

fn main() {
    let args = Args::parse(env::args().skip(1));
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| FikaApp::new(args.clone(), cx)),
        )
        .expect("failed to open Fika GPUI window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooser_choice_defaults_are_returned_without_ui_state() {
        let rows = selected_choice_rows(&[
            "encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()
        ]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tencoding\tutf8"]);
    }

    #[test]
    fn chooser_choice_falls_back_to_first_option() {
        let rows = selected_choice_rows(&["quality\tQuality\t\tlow=Low;high=High".to_string()]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tquality\tlow"]);
    }

    #[test]
    fn parses_chooser_mode_without_versioned_dependencies() {
        let args = Args::parse(
            ["--chooser", "--chooser-directory", "/tmp"]
                .into_iter()
                .map(str::to_string),
        );
        assert_eq!(args.mode, Mode::Chooser);
        assert!(args.chooser_directories);
    }

    #[test]
    fn select_all_keystroke_uses_secondary_modifier() {
        let mut keystroke = gpui::Keystroke::parse("secondary-a").unwrap();
        assert_eq!(pane_shortcut(&keystroke), Some(PaneShortcut::SelectAll));

        keystroke.modifiers.shift = true;
        assert_eq!(pane_shortcut(&keystroke), None);
    }

    #[test]
    fn pane_shortcuts_classify_navigation_and_selection_keys() {
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("escape").unwrap()),
            Some(PaneShortcut::ClearSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f5").unwrap()),
            Some(PaneShortcut::Refresh)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f2").unwrap()),
            Some(PaneShortcut::RenameSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("up").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: false,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("right").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: false,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-left").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: true,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-down").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: true,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("backspace").unwrap()),
            Some(PaneShortcut::GoParent)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("alt-left").unwrap()),
            Some(PaneShortcut::GoBack)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("alt-right").unwrap()),
            Some(PaneShortcut::GoForward)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("delete").unwrap()),
            Some(PaneShortcut::TrashSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-z").unwrap()),
            Some(PaneShortcut::Undo)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-c").unwrap()),
            Some(PaneShortcut::CopySelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-v").unwrap()),
            Some(PaneShortcut::PasteIntoPane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-x").unwrap()),
            Some(PaneShortcut::CutSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-shift-n").unwrap()),
            Some(PaneShortcut::CreateFolder)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-f5").unwrap()),
            None
        );
    }

    #[test]
    fn rename_input_action_classifies_controls_and_text() {
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            RenameInputAction::Cancel
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            RenameInputAction::Commit
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            RenameInputAction::Backspace
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("a->a").unwrap()),
            RenameInputAction::Insert("a".to_string())
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            RenameInputAction::Insert("A".to_string())
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("secondary-a").unwrap()),
            RenameInputAction::Ignore
        );
    }

    #[test]
    fn rename_item_result_renames_item_and_records_affected_dir() {
        let temp = test_dir("rename-item");
        std::fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        let renamed = temp.join("new.txt");
        std::fs::write(&original, "rename").unwrap();

        let result = rename_item_result(PaneId(11), original.clone(), "new.txt".to_string());
        let renamed_path = result.result.unwrap();

        assert_eq!(result.pane_id, PaneId(11));
        assert_eq!(result.original_path, original.clone());
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(renamed_path, renamed);
        assert!(!original.exists());
        assert_eq!(std::fs::read_to_string(&renamed_path).unwrap(), "rename");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn create_item_result_creates_default_folder_and_records_affected_dir() {
        let temp = test_dir("create-folder");
        std::fs::create_dir_all(&temp).unwrap();

        let result = create_item_result(PaneId(5), temp.clone(), CreatedItemKind::Folder);
        let created = result.result.unwrap();

        assert_eq!(result.pane_id, PaneId(5));
        assert_eq!(result.kind, CreatedItemKind::Folder);
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(created.file_name().unwrap(), "New Folder");
        assert!(created.is_dir());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn create_item_result_uses_keep_both_name_for_default_file() {
        let temp = test_dir("create-file");
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("New File.txt"), "occupied").unwrap();

        let result = create_item_result(PaneId(6), temp.clone(), CreatedItemKind::File);
        let created = result.result.unwrap();

        assert_eq!(result.kind, CreatedItemKind::File);
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(created.file_name().unwrap(), "New File copy.txt");
        assert!(created.is_file());
        assert!(temp.join("New File.txt").exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_copies_item_and_records_transfer_undo() {
        let temp = test_dir("paste-copy");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "copy").unwrap();

        let result = paste_clipboard_result(
            PaneId(7),
            target_dir.clone(),
            ClipboardState {
                mode: ClipboardMode::Copy,
                paths: vec![source.clone()],
            },
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.pane_id, PaneId(7));
        assert_eq!(result.mode, ClipboardMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(result.affected_dirs, vec![target_dir.clone()]);
        assert_eq!(
            result.undo_items,
            vec![TransferUndoItem {
                operation: "copy".to_string(),
                original_source: source.clone(),
                destination: destination.clone(),
                overwritten_backup: None,
            }]
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "copy");
        assert!(source.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_moves_item_and_marks_both_directories() {
        let temp = test_dir("paste-move");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "move").unwrap();

        let result = paste_clipboard_result(
            PaneId(8),
            target_dir.clone(),
            ClipboardState {
                mode: ClipboardMode::Cut,
                paths: vec![source.clone()],
            },
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(
            result.affected_dirs,
            vec![target_dir.clone(), source_dir.clone()]
        );
        assert_eq!(result.undo_items[0].operation, "move");
        assert_eq!(result.undo_items[0].original_source, source);
        assert_eq!(result.undo_items[0].destination, destination.clone());
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "move");
        assert!(!source_dir.join("note.txt").exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_restores_transfer_payload() {
        let temp = test_dir("undo-transfer");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        let destination = target_dir.join("note.txt");
        std::fs::write(&source, "undo").unwrap();

        let paste = paste_clipboard_result(
            PaneId(9),
            target_dir,
            ClipboardState {
                mode: ClipboardMode::Cut,
                paths: vec![source.clone()],
            },
        );
        assert_eq!(paste.success_count, 1);
        assert!(destination.exists());
        assert!(!source.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Move".to_string(),
            affected_dirs: paste.affected_dirs,
            payload: UndoPayload::Transfer {
                items: paste.undo_items,
            },
        });

        assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "undo");
        assert!(!destination.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_restores_rename_payload() {
        let temp = test_dir("undo-rename");
        std::fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        std::fs::write(&original, "undo rename").unwrap();
        let rename = rename_item_result(PaneId(12), original.clone(), "new.txt".to_string());
        let renamed = rename.result.unwrap();
        assert!(renamed.exists());
        assert!(!original.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Rename".to_string(),
            affected_dirs: rename.affected_dirs,
            payload: UndoPayload::Rename {
                items: vec![RenameUndoItem {
                    original_path: original.clone(),
                    renamed_path: renamed.clone(),
                }],
            },
        });

        assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "undo rename");
        assert!(!renamed.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_removes_created_payload() {
        let temp = test_dir("undo-create");
        std::fs::create_dir_all(&temp).unwrap();
        let create = create_item_result(PaneId(10), temp.clone(), CreatedItemKind::File);
        let created = create.result.unwrap();
        assert!(created.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Create File".to_string(),
            affected_dirs: create.affected_dirs,
            payload: UndoPayload::Create {
                items: vec![CreateUndoItem {
                    path: created.clone(),
                    kind: CreatedItemKind::File,
                }],
            },
        });

        assert_eq!(undo.result, Ok("removed 1 item(s)".to_string()));
        assert!(!created.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn affected_parent_dirs_are_stable_and_deduplicated() {
        let dirs = parent_dirs([
            PathBuf::from("/tmp/a/one.txt"),
            PathBuf::from("/tmp/a/two.txt"),
            PathBuf::from("/tmp/b/three.txt"),
        ]);

        assert_eq!(dirs, vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
    }

    #[test]
    fn action_status_reports_mixed_file_operation_results() {
        assert_eq!(action_status("Moved", 2, 0), "Moved: 2 item(s)");
        assert_eq!(action_status("Moved", 0, 1), "Moved failed for 1 item(s)");
        assert_eq!(action_status("Moved", 2, 1), "Moved: 2 item(s), 1 failed");
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-gpui-{name}-{}-{nanos}", std::process::id()))
    }
}
