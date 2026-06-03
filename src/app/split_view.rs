use crate::app::async_bridge::AsyncBridge;
use crate::app::geometry::{
    PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, inactive_main_pane_width, main_pane_bounds,
};
use crate::app::model_update::{new_file_entries_model, update_file_entries_model};
use crate::app::pane::PaneTarget;
use crate::app::state::AppState;
use crate::app::virtual_view::{PanePreviewInput, prepare_pane_preview_update};
use crate::config::paths::home_dir;
use crate::fs;
use crate::{
    AppWindow, FileEntry, PaneSlotData, set_status, sync_virtual_entries, thumbnail_size_px,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(crate) fn pane_viewport_x_from_ui(ui: &AppWindow, slot: i32) -> Option<f32> {
    match slot {
        0 => Some(ui.get_main_viewport_x()),
        1 => Some(ui.get_inactive_pane_viewport_x()),
        _ => None,
    }
}

fn set_pane_viewport_ui_properties(ui: &AppWindow, slot: i32, viewport_x: f32) {
    match slot {
        0 => {
            ui.set_main_viewport_x(viewport_x);
            ui.set_main_viewport_offset(-viewport_x);
        }
        1 => {
            ui.set_inactive_pane_viewport_x(viewport_x);
            ui.set_inactive_pane_viewport_offset(-viewport_x);
        }
        _ => {}
    }
}

pub(crate) fn set_pane_viewport_ui(ui: &AppWindow, slot: i32, viewport_x: f32) {
    set_pane_viewport_ui_properties(ui, slot, viewport_x);
    sync_pane_slots_ui(ui);
}

pub(crate) fn sync_pane_slots_ui(ui: &AppWindow) {
    let slots = visible_pane_slots(ui)
        .into_iter()
        .map(|slot| pane_slot_data(ui, slot))
        .collect::<Vec<_>>();

    let current = ui.get_pane_slots();
    let same_slots = current.row_count() == slots.len()
        && slots.iter().enumerate().all(|(row, slot)| {
            current
                .row_data(row)
                .is_some_and(|current| current.slot == slot.slot)
        });
    if same_slots {
        for (row, slot) in slots.into_iter().enumerate() {
            if current.row_data(row).as_ref() != Some(&slot) {
                current.set_row_data(row, slot);
            }
        }
        return;
    }

    ui.set_pane_slots(ModelRc::new(Rc::new(VecModel::from(slots))));
}

pub(crate) fn sync_pane_slot_ui(ui: &AppWindow, slot: i32) {
    let current = ui.get_pane_slots();
    for row in 0..current.row_count() {
        let Some(current_slot) = current.row_data(row) else {
            continue;
        };
        if current_slot.slot == slot {
            let next = pane_slot_data(ui, slot);
            if current_slot != next {
                current.set_row_data(row, next);
            }
            return;
        }
    }

    sync_pane_slots_ui(ui);
}

fn visible_pane_slots(ui: &AppWindow) -> Vec<i32> {
    let mut slots = vec![0];
    if ui.get_split_view_open() {
        slots.push(1);
    }
    slots
}

fn pane_slot_data(ui: &AppWindow, slot: i32) -> PaneSlotData {
    let primary_slot = slot == 0;
    let search_query = ui.get_search_query();
    let search_filters_active = ui.get_search_kind_filter() != 0
        || ui.get_search_modified_filter() != 0
        || ui.get_search_size_filter() != 0;
    let search_panel_visible =
        ui.get_search_bar_open() || !search_query.is_empty() || search_filters_active;
    let chooser_choices = ui.get_chooser_choices();
    let undo_available = ui.get_undo_available();
    let undo_label = ui.get_undo_label();
    let chooser_mode = ui.get_chooser_mode();
    let chooser_select_directories = ui.get_chooser_select_directories();
    let chooser_save_mode = ui.get_chooser_save_mode();
    let chooser_accept_label = ui.get_chooser_accept_label();
    let chooser_filter_count = ui.get_chooser_filter_count();
    let chooser_filter_label = ui.get_chooser_filter_label();
    let focused_selected_path = ui.get_selected_path();
    let zoom_level = ui.get_icon_zoom_level();
    let selection_revision = ui.get_selection_revision();

    PaneSlotData {
        slot,
        current_path: pane_slot_current_path(ui, slot),
        path_text: pane_slot_path_text(ui, slot),
        path_focused: pane_slot_path_focused(ui, slot),
        can_go_back: pane_slot_can_go_back(ui, slot),
        can_go_forward: pane_slot_can_go_forward(ui, slot),
        search_panel_visible: primary_slot && search_panel_visible,
        search_panel_height_px: 0.0,
        search_loading: primary_slot && ui.get_search_loading(),
        search_filters_active: primary_slot && search_filters_active,
        search_kind_label: active_search_kind_label(ui),
        search_modified_label: active_search_modified_label(ui),
        search_size_label: active_search_size_label(ui),
        content_interactive: primary_slot
            .then(|| !ui.get_directory_loading())
            .unwrap_or(true),
        drop_ready: primary_slot
            .then(|| !ui.get_directory_loading())
            .unwrap_or(true),
        drop_trace_prefix: format!("pane-{slot}-").into(),
        entry_count: pane_slot_entry_count(ui, slot),
        entries: pane_slot_entries(ui, slot),
        virtual_start_index: pane_slot_virtual_start_index(ui, slot),
        virtual_start_column: pane_slot_virtual_start_column(ui, slot),
        viewport_x: pane_slot_viewport_x(ui, slot),
        zoom_level,
        selection_revision,
        show_location: pane_slot_in_trash(ui, slot)
            || (primary_slot && ui.get_recursive_search() && !search_query.is_empty()),
        empty_message_visible: primary_slot
            .then(|| !ui.get_directory_loading())
            .unwrap_or(true),
        empty_title: if primary_slot {
            active_empty_title(ui, &search_query)
        } else {
            "This folder is empty".into()
        },
        empty_subtitle: if primary_slot {
            active_empty_subtitle(ui, &search_query)
        } else {
            SharedString::new()
        },
        status: pane_slot_status(ui, slot),
        selected_count: pane_slot_selected_count(ui, slot),
        selected_status: pane_slot_selected_status(ui, slot),
        external_edit_active: pane_slot_external_edit_active(ui, slot),
        external_edit_status: pane_slot_external_edit_status(ui, slot),
        undo_available,
        undo_label,
        chooser_mode,
        chooser_select_directories,
        chooser_save_mode,
        chooser_accept_label,
        focused_selected_path,
        chooser_filter_count,
        chooser_filter_label,
        chooser_choices,
    }
}

fn pane_slot_current_path(ui: &AppWindow, slot: i32) -> SharedString {
    match slot {
        1 => ui.get_inactive_pane_path(),
        _ => ui.get_left_pane_path(),
    }
}

fn pane_slot_path_text(ui: &AppWindow, slot: i32) -> SharedString {
    match slot {
        1 => ui.get_inactive_pane_path_input_text(),
        _ => ui.get_left_pane_path_input_text(),
    }
}

fn pane_slot_path_focused(ui: &AppWindow, slot: i32) -> bool {
    match slot {
        1 => ui.get_inactive_pane_path_focused(),
        _ => ui.get_left_pane_path_focused(),
    }
}

fn pane_slot_can_go_back(ui: &AppWindow, slot: i32) -> bool {
    match slot {
        1 => ui.get_inactive_pane_can_go_back(),
        _ => ui.get_left_pane_can_go_back(),
    }
}

fn pane_slot_can_go_forward(ui: &AppWindow, slot: i32) -> bool {
    match slot {
        1 => ui.get_inactive_pane_can_go_forward(),
        _ => ui.get_left_pane_can_go_forward(),
    }
}

fn pane_slot_entry_count(ui: &AppWindow, slot: i32) -> i32 {
    match slot {
        1 => ui.get_inactive_pane_entry_count(),
        _ => ui.get_entry_count(),
    }
}

fn pane_slot_entries(ui: &AppWindow, slot: i32) -> ModelRc<FileEntry> {
    match slot {
        1 => ui.get_inactive_pane_entries(),
        _ => ui.get_virtual_entries(),
    }
}

fn pane_slot_virtual_start_index(ui: &AppWindow, slot: i32) -> i32 {
    match slot {
        1 => ui.get_inactive_pane_virtual_start_index(),
        _ => ui.get_virtual_start_index(),
    }
}

fn pane_slot_virtual_start_column(ui: &AppWindow, slot: i32) -> i32 {
    match slot {
        1 => ui.get_inactive_pane_virtual_start_column(),
        _ => ui.get_virtual_start_column(),
    }
}

fn pane_slot_viewport_x(ui: &AppWindow, slot: i32) -> f32 {
    match slot {
        1 => ui.get_inactive_pane_viewport_x(),
        _ => ui.get_main_viewport_x(),
    }
}

fn pane_slot_in_trash(ui: &AppWindow, slot: i32) -> bool {
    match slot {
        1 => ui.get_inactive_pane_in_trash(),
        _ => ui.get_left_pane_in_trash(),
    }
}

fn pane_slot_status(ui: &AppWindow, slot: i32) -> SharedString {
    match slot {
        1 => ui.get_inactive_pane_status(),
        _ => ui.get_left_pane_status(),
    }
}

fn pane_slot_selected_count(ui: &AppWindow, slot: i32) -> i32 {
    match slot {
        1 => ui.get_inactive_pane_selected_count(),
        _ => ui.get_left_pane_selected_count(),
    }
}

fn pane_slot_selected_status(ui: &AppWindow, slot: i32) -> SharedString {
    match slot {
        1 => ui.get_inactive_pane_selected_status(),
        _ => ui.get_left_pane_selected_status(),
    }
}

fn pane_slot_external_edit_active(ui: &AppWindow, slot: i32) -> bool {
    match slot {
        1 => ui.get_inactive_pane_external_edit_active(),
        _ => ui.get_left_pane_external_edit_active(),
    }
}

fn pane_slot_external_edit_status(ui: &AppWindow, slot: i32) -> SharedString {
    match slot {
        1 => ui.get_inactive_pane_external_edit_status(),
        _ => ui.get_left_pane_external_edit_status(),
    }
}

fn active_search_kind_label(ui: &AppWindow) -> SharedString {
    match ui.get_search_kind_filter() {
        1 => "Type: Folders",
        2 => "Type: Files",
        3 => "Type: Images",
        _ => "Type: All",
    }
    .into()
}

fn active_search_modified_label(ui: &AppWindow) -> SharedString {
    match ui.get_search_modified_filter() {
        1 => "Modified: Today",
        2 => "Modified: 7 days",
        3 => "Modified: 30 days",
        _ => "Modified: Any",
    }
    .into()
}

fn active_search_size_label(ui: &AppWindow) -> SharedString {
    match ui.get_search_size_filter() {
        1 => "Size: < 1 MB",
        2 => "Size: 1-100 MB",
        3 => "Size: > 100 MB",
        _ => "Size: Any",
    }
    .into()
}

fn active_empty_title(ui: &AppWindow, search_query: &SharedString) -> SharedString {
    if ui.get_search_loading() {
        "Searching...".into()
    } else if search_query.is_empty() {
        "This folder is empty".into()
    } else {
        "No matching items".into()
    }
}

fn active_empty_subtitle(ui: &AppWindow, search_query: &SharedString) -> SharedString {
    if ui.get_search_loading() {
        "Scanning subfolders.".into()
    } else if search_query.is_empty() {
        "This directory has no visible files.".into()
    } else {
        "Try another search term.".into()
    }
}

pub(crate) fn set_pane_viewport_ui_if_clamped(
    ui: &AppWindow,
    slot: i32,
    viewport_x: f32,
    viewport_clamped: bool,
) {
    if viewport_clamped {
        set_pane_viewport_ui(ui, slot, viewport_x);
    }
}

pub(crate) fn sync_pane_slot_preview_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    sync_pane_slot_preview_ui_impl(ui, state, slot, true);
}

pub(crate) fn sync_pane_slot_preview_viewport_ui(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
) {
    sync_pane_slot_preview_ui_impl(ui, state, slot, false);
}

fn sync_pane_slot_preview_ui_impl(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    full_sync: bool,
) {
    if slot == 0 {
        sync_pane_slots_ui(ui);
        return;
    }

    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let pane_bounds = main_pane_bounds(
        ui.get_sidebar_width_px(),
        window_size.width,
        window_size.height,
    );
    let main_width = (pane_bounds.right - pane_bounds.left).max(1.0);
    let pane_width = inactive_main_pane_width(
        main_width,
        ui.get_split_view_open(),
        ui.get_split_pane_ratio(),
    )
    .max(1.0);
    let pane_height =
        (pane_bounds.bottom - pane_bounds.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(1.0);

    let snapshot = {
        let mut state = state.borrow_mut();
        prepare_pane_preview_update(
            &mut state,
            PaneTarget::Slot(slot),
            PanePreviewInput {
                pane_width,
                pane_height,
                zoom_level: ui.get_icon_zoom_level(),
                thumbnail_size_px: thumbnail_size_px(ui),
                force_rebuild_model: pane_slot_entry_count(ui, slot) == 0,
            },
        )
    };

    let Some(update) = snapshot else {
        clear_pane_slot_cache(ui, slot);
        sync_pane_slots_ui(ui);
        return;
    };

    if !full_sync {
        if update.viewport_clamped {
            set_pane_viewport_ui_properties(ui, slot, update.viewport_x);
        }
        if update.rebuild_model {
            set_pane_slot_entries_ui(ui, slot, update.range.start, update.entries);
            set_pane_slot_virtual_range_ui(
                ui,
                slot,
                update.range.start as i32,
                update.start_column as i32,
            );
            set_pane_slot_entry_count_ui(ui, slot, update.entry_count as i32);
            sync_pane_slot_ui(ui, slot);
        } else if update.viewport_clamped {
            sync_pane_slot_ui(ui, slot);
        }
        return;
    }

    let path = update.current_dir.display().to_string();
    set_pane_slot_path(ui, slot, path.as_str(), &update.current_dir);
    if !pane_slot_path_focused(ui, slot) {
        set_pane_slot_path_text(ui, slot, path.as_str());
    }
    let (can_go_back, can_go_forward, selected_paths, needs_status) = {
        let state = state.borrow();
        state
            .panes
            .pane_for_slot(slot)
            .map(|pane| {
                (
                    pane.history.back_len() > 0,
                    pane.history.forward_len() > 0,
                    pane.selection.paths.clone(),
                    pane_slot_status(ui, slot)
                        .is_empty()
                        .then(|| directory_status_text(pane.entries.iter())),
                )
            })
            .unwrap_or((false, false, Vec::new(), None))
    };
    set_pane_slot_history_ui(ui, slot, can_go_back, can_go_forward);
    if let Some(status) = needs_status {
        set_pane_slot_status(ui, slot, status.as_str());
    }
    set_pane_slot_selection_ui(ui, slot, &selected_paths);
    set_pane_viewport_ui_if_clamped(ui, slot, update.viewport_x, update.viewport_clamped);
    if update.rebuild_model {
        set_pane_slot_entries_ui(ui, slot, update.range.start, update.entries);
        set_pane_slot_virtual_range_ui(
            ui,
            slot,
            update.range.start as i32,
            update.start_column as i32,
        );
    }
    set_pane_slot_entry_count_ui(ui, slot, update.entry_count as i32);
    sync_pane_slots_ui(ui);
}

fn clear_pane_slot_cache(ui: &AppWindow, slot: i32) {
    match slot {
        0 => {}
        1 => {
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
            set_pane_viewport_ui(ui, slot, 0.0);
            ui.set_inactive_pane_entries(new_file_entries_model(Vec::<FileEntry>::new()));
        }
        _ => {}
    }
}

fn set_pane_slot_path(ui: &AppWindow, slot: i32, path: &str, current_dir: &Path) {
    match slot {
        0 => {
            ui.set_left_pane_path(path.into());
            ui.set_left_pane_in_trash(fs::file_ops::is_in_trash_files_dir(current_dir));
        }
        1 => {
            ui.set_inactive_pane_path(path.into());
            ui.set_inactive_pane_in_trash(fs::file_ops::is_in_trash_files_dir(current_dir));
        }
        _ => {}
    }
}

fn set_pane_slot_path_text(ui: &AppWindow, slot: i32, path: &str) {
    match slot {
        0 => ui.set_left_pane_path_input_text(path.into()),
        1 => ui.set_inactive_pane_path_input_text(path.into()),
        _ => {}
    }
}

fn set_pane_slot_history_ui(ui: &AppWindow, slot: i32, can_go_back: bool, can_go_forward: bool) {
    match slot {
        0 => {
            ui.set_left_pane_can_go_back(can_go_back);
            ui.set_left_pane_can_go_forward(can_go_forward);
        }
        1 => {
            ui.set_inactive_pane_can_go_back(can_go_back);
            ui.set_inactive_pane_can_go_forward(can_go_forward);
        }
        _ => {}
    }
}

fn set_pane_slot_status(ui: &AppWindow, slot: i32, status: &str) {
    match slot {
        0 => ui.set_left_pane_status(status.into()),
        1 => ui.set_inactive_pane_status(status.into()),
        _ => {}
    }
}

fn set_pane_slot_selection_ui(ui: &AppWindow, slot: i32, selected_paths: &[String]) {
    let selected_count = selected_paths.len() as i32;
    let selected_status = selection_status_text(selected_paths);
    match slot {
        0 => {
            ui.set_left_pane_selected_count(selected_count);
            ui.set_left_pane_selected_status(selected_status);
        }
        1 => {
            ui.set_inactive_pane_selected_count(selected_count);
            ui.set_inactive_pane_selected_status(selected_status);
        }
        _ => {}
    }
}

fn set_pane_slot_entries_ui(
    ui: &AppWindow,
    slot: i32,
    start_index: usize,
    entries: Vec<FileEntry>,
) {
    match slot {
        1 => {
            let current = ui.get_inactive_pane_entries();
            let old_start = ui.get_inactive_pane_virtual_start_index().max(0) as usize;
            if let Some(model) =
                update_file_entries_model(&current, old_start, start_index, entries)
            {
                ui.set_inactive_pane_entries(model);
            }
        }
        _ => {}
    }
}

fn set_pane_slot_virtual_range_ui(ui: &AppWindow, slot: i32, start_index: i32, start_column: i32) {
    match slot {
        1 => {
            ui.set_inactive_pane_virtual_start_index(start_index);
            ui.set_inactive_pane_virtual_start_column(start_column);
        }
        _ => {}
    }
}

fn set_pane_slot_entry_count_ui(ui: &AppWindow, slot: i32, entry_count: i32) {
    match slot {
        1 => ui.set_inactive_pane_entry_count(entry_count),
        _ => {}
    }
}

pub(crate) fn sync_navigation_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let snapshot = {
        let state = state.borrow();
        let focused_slot = state.panes.focused_slot();
        let focused = state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(&state.panes.active());
        NavigationUiSnapshot {
            split_open: state.panes.is_split(),
            focused_slot,
            focused_dir: focused.current_dir.clone(),
            focused_selection: focused.selection.paths.clone(),
            left_dir: state.panes.active().current_dir.clone(),
            left_can_go_back: state.panes.active().history.back_len() > 0,
            left_can_go_forward: state.panes.active().history.forward_len() > 0,
            left_selection: state.panes.active().selection.paths.clone(),
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
            directory_status_text(state.panes.active().entries.iter())
        };
        ui.set_left_pane_status(left_status.into());
    }
    ui.set_left_pane_selected_count(snapshot.left_selection.len() as i32);
    ui.set_left_pane_selected_status(selection_status_text(&snapshot.left_selection));
    ui.set_split_view_open(snapshot.split_open);
    sync_focused_ui(
        ui,
        snapshot.focused_slot,
        &snapshot.focused_dir,
        &snapshot.focused_selection,
    );
    sync_pane_slot_preview_ui(ui, state, 1);
    sync_pane_slots_ui(ui);
}

pub(crate) fn sync_focus_navigation_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let (focused_slot, focused_dir, focused_selection) = {
        let state = state.borrow();
        let focused_slot = state.panes.focused_slot();
        let focused = state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(&state.panes.active());
        (
            focused_slot,
            focused.current_dir.clone(),
            focused.selection.paths.clone(),
        )
    };

    sync_focused_ui(ui, focused_slot, &focused_dir, &focused_selection);
}

pub(crate) fn toggle_split_view(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let was_split = state.borrow().panes.is_split();
    if was_split {
        let slots = state
            .borrow()
            .panes
            .iter()
            .map(|(slot, _)| slot)
            .collect::<Vec<_>>();
        for slot in slots {
            crate::remember_pane_view_state(ui, state, slot);
        }
    }

    let (opened, status) = {
        let mut state = state.borrow_mut();
        if state.panes.is_split() {
            let closed_slot = state
                .panes
                .close_focused_pane_slot()
                .map(|(slot, _)| slot)
                .unwrap_or(1);
            let status = format!("Split view closed; slot {closed_slot} closed");
            (false, status)
        } else {
            let current_dir = state.panes.active().current_dir.clone();
            state.panes.open_inactive_from_active();
            state.panes.active_mut().view.viewport_x = 0.0;
            state.panes.active_mut().view.virtual_view.invalidate();
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
        set_pane_viewport_ui(ui, 0, 0.0);
        set_pane_viewport_ui(ui, 1, 0.0);
    }
    if !opened {
        let viewport_x = state.borrow().panes.active().view.viewport_x;
        set_pane_viewport_ui(ui, 0, viewport_x);
    }
    sync_navigation_ui(ui, state);
    sync_virtual_entries(ui, state, bridge, true);
    set_status(ui, &status);
}

#[derive(Debug)]
struct NavigationUiSnapshot {
    split_open: bool,
    focused_slot: i32,
    focused_dir: PathBuf,
    focused_selection: Vec<String>,
    left_dir: PathBuf,
    left_can_go_back: bool,
    left_can_go_forward: bool,
    left_selection: Vec<String>,
}

fn sync_focused_ui(ui: &AppWindow, slot: i32, current_dir: &Path, selected_paths: &[String]) {
    ui.set_focused_pane(slot);
    let current_path = current_dir.display().to_string();
    ui.set_focused_pane_path(current_path.as_str().into());
    ui.set_current_path(current_path.into());
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
    sync_pane_slots_ui(ui);
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
