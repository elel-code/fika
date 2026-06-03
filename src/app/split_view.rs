use crate::app::async_bridge::AsyncBridge;
use crate::app::geometry::{
    PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, inactive_main_pane_width, main_pane_bounds,
};
use crate::app::pane::{PaneSide, PaneTarget};
use crate::app::state::AppState;
use crate::app::virtual_view::{PanePreviewInput, prepare_pane_preview_update};
use crate::config::paths::home_dir;
use crate::fs;
use crate::{AppWindow, FileEntry, set_status, sync_virtual_entries, thumbnail_size_px};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(crate) fn sync_inactive_pane_view_from_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_mut_for_target(PaneTarget::Inactive) else {
            return;
        };
        pane.view.viewport_x = ui.get_inactive_pane_viewport_x();
    }
    sync_inactive_pane_ui(ui, state);
}

pub(crate) fn set_pane_viewport_ui(ui: &AppWindow, side: PaneSide, viewport_x: f32) {
    match side {
        PaneSide::Active => {
            ui.set_main_viewport_x(viewport_x);
            ui.set_main_viewport_offset(-viewport_x);
        }
        PaneSide::Inactive => {
            ui.set_inactive_pane_viewport_x(viewport_x);
            ui.set_inactive_pane_viewport_offset(-viewport_x);
        }
    }
}

pub(crate) fn set_pane_viewport_ui_if_clamped(
    ui: &AppWindow,
    side: PaneSide,
    viewport_x: f32,
    viewport_clamped: bool,
) {
    if viewport_clamped {
        set_pane_viewport_ui(ui, side, viewport_x);
    }
}

pub(crate) fn sync_inactive_pane_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let pane_bounds = main_pane_bounds(
        ui.get_sidebar_width_px(),
        window_size.width,
        window_size.height,
    );
    let main_width = (pane_bounds.right - pane_bounds.left).max(1.0);
    let inactive_width = inactive_main_pane_width(
        main_width,
        ui.get_split_view_open(),
        ui.get_split_pane_ratio(),
    )
    .max(1.0);
    let inactive_height =
        (pane_bounds.bottom - pane_bounds.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(1.0);

    let snapshot = {
        let mut state = state.borrow_mut();
        prepare_pane_preview_update(
            &mut state,
            PaneTarget::Inactive,
            PanePreviewInput {
                pane_width: inactive_width,
                pane_height: inactive_height,
                zoom_level: ui.get_icon_zoom_level(),
                thumbnail_size_px: thumbnail_size_px(ui),
                force_rebuild_model: ui.get_inactive_pane_entry_count() == 0,
            },
        )
    };

    let Some(update) = snapshot else {
        ui.set_inactive_pane_path(SharedString::new());
        ui.set_inactive_pane_path_input_text(SharedString::new());
        ui.set_inactive_pane_status(SharedString::new());
        ui.set_inactive_pane_in_trash(false);
        ui.set_inactive_pane_selected_count(0);
        ui.set_inactive_pane_selected_status(SharedString::new());
        ui.set_inactive_pane_can_go_back(false);
        ui.set_inactive_pane_can_go_forward(false);
        ui.set_inactive_pane_entry_count(0);
        ui.set_inactive_pane_virtual_start_index(0);
        ui.set_inactive_pane_virtual_start_column(0);
        set_pane_viewport_ui(ui, PaneSide::Inactive, 0.0);
        ui.set_inactive_pane_entries(ModelRc::new(Rc::new(VecModel::from(
            Vec::<FileEntry>::new(),
        ))));
        return;
    };

    let path = update.current_dir.display().to_string();
    ui.set_inactive_pane_path(path.as_str().into());
    ui.set_inactive_pane_in_trash(fs::file_ops::is_in_trash_files_dir(&update.current_dir));
    if !ui.get_inactive_pane_path_focused() {
        ui.set_inactive_pane_path_input_text(path.into());
    }
    let (can_go_back, can_go_forward) = {
        let state = state.borrow();
        state
            .panes
            .inactive()
            .map(|pane| (pane.history.back_len() > 0, pane.history.forward_len() > 0))
            .unwrap_or((false, false))
    };
    ui.set_inactive_pane_can_go_back(can_go_back);
    ui.set_inactive_pane_can_go_forward(can_go_forward);
    {
        let state = state.borrow();
        if let Some(pane) = state.panes.inactive() {
            if ui.get_inactive_pane_status().is_empty() {
                ui.set_inactive_pane_status(directory_status_text(pane.entries.iter()).into());
            }
            let selected_paths = pane.selection.paths.clone();
            ui.set_inactive_pane_selected_count(selected_paths.len() as i32);
            ui.set_inactive_pane_selected_status(selection_status_text(&selected_paths));
        }
    }
    set_pane_viewport_ui_if_clamped(
        ui,
        PaneSide::Inactive,
        update.viewport_x,
        update.viewport_clamped,
    );
    if update.rebuild_model {
        ui.set_inactive_pane_entries(ModelRc::new(Rc::new(VecModel::from(update.entries))));
        ui.set_inactive_pane_virtual_start_index(update.range.start as i32);
        ui.set_inactive_pane_virtual_start_column(update.start_column as i32);
    }
    ui.set_inactive_pane_entry_count(update.entry_count as i32);
}

pub(crate) fn sync_navigation_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let snapshot = {
        let state = state.borrow();
        let focused_side = state.panes.focused_side();
        let focused = state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(&state.panes.active);
        NavigationUiSnapshot {
            split_open: state.panes.is_split(),
            focused_side,
            focused_dir: focused.current_dir.clone(),
            focused_selection: focused.selection.paths.clone(),
            left_dir: state.panes.active.current_dir.clone(),
            left_can_go_back: state.panes.active.history.back_len() > 0,
            left_can_go_forward: state.panes.active.history.forward_len() > 0,
            left_selection: state.panes.active.selection.paths.clone(),
        }
    };

    let left_path = snapshot.left_dir.display().to_string();
    ui.set_left_pane_path(left_path.as_str().into());
    if !ui.get_left_pane_path_focused() {
        ui.set_left_pane_path_input_text(left_path.into());
    }
    ui.set_left_pane_can_go_back(snapshot.left_can_go_back);
    ui.set_left_pane_can_go_forward(snapshot.left_can_go_forward);
    ui.set_left_pane_in_trash(fs::file_ops::is_in_trash_files_dir(&snapshot.left_dir));
    if ui.get_left_pane_status().is_empty() {
        let left_status = {
            let state = state.borrow();
            directory_status_text(state.panes.active.entries.iter())
        };
        ui.set_left_pane_status(left_status.into());
    }
    ui.set_left_pane_selected_count(snapshot.left_selection.len() as i32);
    ui.set_left_pane_selected_status(selection_status_text(&snapshot.left_selection));
    ui.set_split_view_open(snapshot.split_open);
    sync_focused_ui(
        ui,
        snapshot.focused_side,
        &snapshot.focused_dir,
        &snapshot.focused_selection,
    );
    sync_inactive_pane_ui(ui, state);
}

pub(crate) fn toggle_split_view(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let was_split = state.borrow().panes.is_split();
    if was_split {
        crate::remember_current_view_state(ui, state);
        crate::remember_inactive_view_state(ui, state);
    }

    let (opened, status) = {
        let mut state = state.borrow_mut();
        if state.panes.is_split() {
            let closed_side = state
                .panes
                .close_focused_split_pane()
                .map(|(side, _)| side)
                .unwrap_or(PaneSide::Inactive);
            let status = match closed_side {
                PaneSide::Active => "Split view closed; right pane kept".to_string(),
                PaneSide::Inactive => "Split view closed; left pane kept".to_string(),
            };
            (false, status)
        } else {
            let current_dir = state.panes.active.current_dir.clone();
            state.panes.open_inactive_from_active();
            state.panes.active.view.viewport_x = 0.0;
            state.panes.active.view.virtual_view.invalidate();
            if let Some(inactive) = state.panes.inactive_mut() {
                inactive.view.viewport_x = 0.0;
                inactive.view.virtual_view.invalidate();
            }
            (
                true,
                format!("Split view opened at {}", current_dir.display()),
            )
        }
    };

    if opened {
        set_pane_viewport_ui(ui, PaneSide::Active, 0.0);
        set_pane_viewport_ui(ui, PaneSide::Inactive, 0.0);
    }
    if !opened {
        let viewport_x = state.borrow().panes.active.view.viewport_x;
        set_pane_viewport_ui(ui, PaneSide::Active, viewport_x);
    }
    sync_navigation_ui(ui, state);
    sync_virtual_entries(ui, state, bridge, true);
    set_status(ui, &status);
}

#[derive(Debug)]
struct NavigationUiSnapshot {
    split_open: bool,
    focused_side: PaneSide,
    focused_dir: PathBuf,
    focused_selection: Vec<String>,
    left_dir: PathBuf,
    left_can_go_back: bool,
    left_can_go_forward: bool,
    left_selection: Vec<String>,
}

fn sync_focused_ui(ui: &AppWindow, side: PaneSide, current_dir: &Path, selected_paths: &[String]) {
    ui.set_focused_pane(match side {
        PaneSide::Active => 0,
        PaneSide::Inactive => 1,
    });
    ui.set_current_path(current_dir.display().to_string().into());
    ui.set_current_name(display_location_name(current_dir).into());
    ui.set_current_in_trash(fs::file_ops::is_in_trash_files_dir(current_dir));
    ui.set_selected_path(
        selected_paths
            .last()
            .map_or_else(SharedString::new, |path| path.as_str().into()),
    );
    ui.set_selected_count(selected_paths.len() as i32);
    ui.set_selected_status(selection_status_text(selected_paths));
    ui.set_selection_revision(ui.get_selection_revision() + 1);
}

pub(crate) fn directory_status_text<'a>(entries: impl Iterator<Item = &'a FileEntry>) -> String {
    let mut folders = 0usize;
    let mut files = 0usize;
    for entry in entries {
        if entry.is_dir {
            folders += 1;
        } else {
            files += 1;
        }
    }
    format!("{folders} folders, {files} files")
}

fn selection_status_text(selected_paths: &[String]) -> SharedString {
    match selected_paths {
        [] => SharedString::new(),
        [path] => format!("1 item selected: {path}").into(),
        paths => format!("{} items selected", paths.len()).into(),
    }
}

fn display_location_name(path: &Path) -> String {
    if path == home_dir() {
        "Home".to_string()
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("/")
            .to_string()
    }
}
