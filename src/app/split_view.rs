use crate::app::async_bridge::AsyncBridge;
use crate::app::geometry::{MainItemViewLayout, active_main_pane_width, inactive_main_pane_width};
use crate::app::pane::{PaneEntrySnapshot, PaneTarget};
use crate::app::state::AppState;
use crate::config::paths::home_dir;
use crate::fs;
use crate::{
    AppWindow, ItemViewEntry, ItemViewHighlightEntry, PaneSlotData, PaneViewData, set_status,
    sync_virtual_entries_for_slot,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub(crate) fn pane_viewport_x_from_ui(
    _ui: &AppWindow,
    slot: i32,
    state: &Rc<RefCell<AppState>>,
) -> f32 {
    state
        .borrow()
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.viewport_x)
        .unwrap_or_default()
}

pub(crate) fn set_pane_viewport_ui(
    ui: &AppWindow,
    slot: i32,
    viewport_x: f32,
    state: &Rc<RefCell<AppState>>,
) {
    let updated = {
        let mut state = state.borrow_mut();
        state.panes.pane_mut_for_slot(slot).is_some_and(|pane| {
            pane.view.viewport_x = viewport_x;
            true
        })
    };
    if updated {
        sync_pane_view_viewport_ui(ui, state, slot, viewport_x);
    }
}

fn sync_pane_view_viewport_ui(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    viewport_x: f32,
) {
    let current = ui.get_pane_views();
    for row in 0..current.row_count() {
        let Some(mut current_view) = current.row_data(row) else {
            continue;
        };
        if current_view.slot == slot {
            if (current_view.viewport_x - viewport_x).abs() > f32::EPSILON {
                current_view.viewport_x = viewport_x;
                current.set_row_data(row, current_view);
            }
            return;
        }
    }

    sync_pane_view_ui(ui, state, slot);
}

pub(crate) fn sync_pane_slots_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let visible_slots = visible_pane_slots(ui);
    let slots = {
        let state_ref = state.borrow();
        visible_slots
            .iter()
            .copied()
            .map(|slot| pane_slot_data(ui, slot, &state_ref))
            .collect::<Vec<_>>()
    };
    let views = {
        let state_ref = state.borrow();
        visible_slots
            .iter()
            .copied()
            .map(|slot| pane_view_data(ui, slot, &state_ref))
            .collect::<Vec<_>>()
    };
    let entries = {
        let state_ref = state.borrow();
        visible_slots
            .iter()
            .copied()
            .map(|slot| (slot, pane_slot_entries(slot, &state_ref)))
            .collect::<Vec<_>>()
    };
    let highlights = {
        let state_ref = state.borrow();
        visible_slots
            .iter()
            .copied()
            .map(|slot| (slot, pane_slot_highlights(slot, &state_ref)))
            .collect::<Vec<_>>()
    };

    if ui.get_pane_slots().row_count() > slots.len() {
        sync_pane_slots_model(ui, slots);
        sync_pane_views_model(ui, views);
    } else {
        sync_pane_views_model(ui, views);
        sync_pane_slots_model(ui, slots);
    }
    sync_pane_entries_ui(ui, entries);
    sync_pane_highlights_ui(ui, highlights);
}

fn sync_pane_slots_model(ui: &AppWindow, slots: Vec<PaneSlotData>) {
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

pub(crate) fn sync_pane_view_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    let current = ui.get_pane_views();
    for row in 0..current.row_count() {
        let Some(current_view) = current.row_data(row) else {
            continue;
        };
        if current_view.slot == slot {
            let (next, entries, highlights) = {
                let state_ref = state.borrow();
                (
                    pane_view_data(ui, slot, &state_ref),
                    pane_slot_entries(slot, &state_ref),
                    pane_slot_highlights(slot, &state_ref),
                )
            };
            if current_view != next {
                current.set_row_data(row, next);
            }
            set_pane_entries_ui(ui, slot, entries);
            set_pane_highlights_ui(ui, slot, highlights);
            return;
        }
    }

    sync_pane_slots_ui(ui, state);
}

pub(crate) fn sync_pane_slot_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    let current = ui.get_pane_slots();
    for row in 0..current.row_count() {
        let Some(current_slot) = current.row_data(row) else {
            continue;
        };
        if current_slot.slot == slot {
            let next = {
                let state_ref = state.borrow();
                pane_slot_data(ui, slot, &state_ref)
            };
            if current_slot != next {
                current.set_row_data(row, next);
            }
            return;
        }
    }

    sync_pane_slots_ui(ui, state);
}

fn visible_pane_slots(ui: &AppWindow) -> Vec<i32> {
    let mut slots = vec![0];
    if ui.get_split_view_open() {
        slots.push(1);
    }
    slots
}

fn sync_pane_views_model(ui: &AppWindow, views: Vec<PaneViewData>) {
    let current = ui.get_pane_views();
    let same_slots = current.row_count() == views.len()
        && views.iter().enumerate().all(|(row, view)| {
            current
                .row_data(row)
                .is_some_and(|current| current.slot == view.slot)
        });
    if same_slots {
        for (row, view) in views.into_iter().enumerate() {
            if current.row_data(row).as_ref() != Some(&view) {
                current.set_row_data(row, view);
            }
        }
        return;
    }

    ui.set_pane_views(ModelRc::new(Rc::new(VecModel::from(views))));
}

fn sync_pane_entries_ui(ui: &AppWindow, entries: Vec<(i32, ModelRc<ItemViewEntry>)>) {
    for (slot, model) in entries {
        set_pane_entries_ui(ui, slot, model);
    }
}

fn set_pane_entries_ui(ui: &AppWindow, slot: i32, entries: ModelRc<ItemViewEntry>) {
    match slot {
        0 => {
            if ui.get_pane_slot_0_entries() != entries {
                ui.set_pane_slot_0_entries(entries);
            }
        }
        1 => {
            if ui.get_pane_slot_1_entries() != entries {
                ui.set_pane_slot_1_entries(entries);
            }
        }
        _ => {}
    }
}

fn sync_pane_highlights_ui(
    ui: &AppWindow,
    highlights: Vec<(i32, ModelRc<ItemViewHighlightEntry>)>,
) {
    for (slot, model) in highlights {
        set_pane_highlights_ui(ui, slot, model);
    }
}

fn set_pane_highlights_ui(ui: &AppWindow, slot: i32, highlights: ModelRc<ItemViewHighlightEntry>) {
    match slot {
        0 => {
            if ui.get_pane_slot_0_highlights() != highlights {
                ui.set_pane_slot_0_highlights(highlights);
            }
        }
        1 => {
            if ui.get_pane_slot_1_highlights() != highlights {
                ui.set_pane_slot_1_highlights(highlights);
            }
        }
        _ => {}
    }
}

fn pane_slot_data(ui: &AppWindow, slot: i32, state: &AppState) -> PaneSlotData {
    let is_focused = slot == state.panes.focused_slot();
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

    PaneSlotData {
        slot,
        current_path: pane_slot_current_path(state, slot),
        path_text: pane_slot_path_text(state, slot),
        path_focused: pane_slot_path_focused(state, slot),
        can_go_back: pane_slot_can_go_back(state, slot),
        can_go_forward: pane_slot_can_go_forward(state, slot),
        search_panel_visible: is_focused && search_panel_visible,
        search_panel_height_px: 0.0,
        search_query: if is_focused {
            search_query.clone()
        } else {
            SharedString::new()
        },
        recursive_search: is_focused && pane_slot_recursive_search(state, slot),
        search_kind_filter: ui.get_search_kind_filter(),
        search_modified_filter: ui.get_search_modified_filter(),
        search_size_filter: ui.get_search_size_filter(),
        search_loading: is_focused && ui.get_search_loading(),
        search_filters_active: is_focused && search_filters_active,
        search_kind_label: active_search_kind_label(ui),
        search_modified_label: active_search_modified_label(ui),
        search_size_label: active_search_size_label(ui),
        drop_trace_prefix: format!("pane-{slot}-").into(),
        status: pane_slot_status(state, slot),
        selected_count: pane_slot_selected_count(state, slot),
        selected_status: pane_slot_selected_status(state, slot),
        external_edit_active: pane_slot_external_edit_active(state, slot),
        external_edit_status: pane_slot_external_edit_status(state, slot),
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

fn pane_view_data(ui: &AppWindow, slot: i32, state: &AppState) -> PaneViewData {
    let is_focused = slot == state.panes.focused_slot();
    let search_query = ui.get_search_query();
    let item_view_metrics = pane_slot_item_view_metrics(ui, slot, state);

    PaneViewData {
        slot,
        entry_count: item_view_metrics.entry_count,
        virtual_start_column: pane_slot_virtual_start_column(state, slot),
        viewport_x: pane_slot_viewport_x(slot, state),
        item_view_rows_per_column: item_view_metrics.rows_per_column,
        item_view_cell_width: item_view_metrics.cell_width,
        item_view_column_width: item_view_metrics.column_width,
        item_view_column_offset: item_view_metrics.column_offset,
        item_view_row_height: item_view_metrics.row_height,
        item_view_padding: item_view_metrics.padding,
        item_view_content_width: item_view_metrics.content_width,
        item_view_virtual_slice_width: item_view_metrics.virtual_slice_width,
        item_view_scroll_max_x: item_view_metrics.scroll_max_x,
        selection_revision: ui.get_selection_revision(),
        show_location: pane_slot_show_location(state, slot),
        content_interactive: if is_focused {
            !ui.get_directory_loading()
        } else {
            true
        },
        drop_ready: if is_focused {
            !ui.get_directory_loading()
        } else {
            true
        },
        empty_message_visible: if is_focused {
            !ui.get_directory_loading()
        } else {
            true
        },
        empty_title: if is_focused {
            active_empty_title(ui, &search_query)
        } else {
            "This folder is empty".into()
        },
        empty_subtitle: if is_focused {
            active_empty_subtitle(ui, &search_query)
        } else {
            SharedString::new()
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemViewSlotMetrics {
    entry_count: i32,
    rows_per_column: i32,
    cell_width: f32,
    column_width: f32,
    column_offset: f32,
    row_height: f32,
    padding: f32,
    content_width: f32,
    virtual_slice_width: f32,
    scroll_max_x: f32,
}

fn pane_slot_item_view_metrics(ui: &AppWindow, slot: i32, state: &AppState) -> ItemViewSlotMetrics {
    let viewport_width = pane_slot_width(ui, slot);
    let search_panel_visible = state.panes.focused_slot() == slot;
    let text_line_count = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.item_view_text_line_count())
        .unwrap_or(1);
    let layout = MainItemViewLayout::from_ui_for_pane_width_with_text_lines(
        ui,
        viewport_width,
        search_panel_visible,
        text_line_count,
    );
    let (entry_count, virtual_slice_count) = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| {
            (
                pane.view.virtual_view.entry_count,
                pane.view.virtual_entries.row_count(),
            )
        })
        .unwrap_or((0, 0));
    let compact_item_view = layout.compact_item_view(entry_count);
    let virtual_slice_width = compact_item_view.virtual_slice_width(virtual_slice_count);

    ItemViewSlotMetrics {
        entry_count: entry_count as i32,
        rows_per_column: compact_item_view.rows_per_column as i32,
        cell_width: compact_item_view.cell_width,
        column_width: compact_item_view.column_width,
        column_offset: compact_item_view.column_offset,
        row_height: compact_item_view.row_height,
        padding: compact_item_view.padding,
        content_width: compact_item_view.content_width,
        virtual_slice_width,
        scroll_max_x: compact_item_view.scroll_max_x,
    }
}

fn pane_slot_width(ui: &AppWindow, slot: i32) -> f32 {
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let main_width = (window_size.width - ui.get_sidebar_width_px()).max(1.0);
    if slot == 0 {
        return active_main_pane_width(
            main_width,
            ui.get_split_view_open(),
            ui.get_split_pane_ratio(),
        );
    }

    inactive_main_pane_width(
        main_width,
        ui.get_split_view_open(),
        ui.get_split_pane_ratio(),
    )
}

fn pane_slot_current_path(state: &AppState, slot: i32) -> SharedString {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.current_dir.display().to_string().into())
        .unwrap_or_default()
}

fn pane_slot_path_text(state: &AppState, slot: i32) -> SharedString {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.path_input_text.as_str().into())
        .unwrap_or_default()
}

fn pane_slot_path_focused(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .is_some_and(|pane| pane.path_focused)
}

fn pane_slot_can_go_back(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .is_some_and(|pane| pane.history.back_len() > 0)
}

fn pane_slot_can_go_forward(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .is_some_and(|pane| pane.history.forward_len() > 0)
}

fn pane_slot_entries(slot: i32, state: &AppState) -> ModelRc<ItemViewEntry> {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.virtual_entries.clone())
        .unwrap_or_default()
}

fn pane_slot_highlights(slot: i32, state: &AppState) -> ModelRc<ItemViewHighlightEntry> {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| {
            let highlights = pane
                .view
                .virtual_entry_tokens
                .iter()
                .enumerate()
                .filter_map(|(row, token)| {
                    token.selected().then_some(ItemViewHighlightEntry {
                        slice_index: row as i32,
                        tile_width: token.tile_width(),
                        tile_height: token.tile_height(),
                    })
                })
                .collect::<Vec<_>>();
            ModelRc::new(Rc::new(VecModel::from(highlights)))
        })
        .unwrap_or_default()
}

fn pane_slot_virtual_start_column(state: &AppState, slot: i32) -> i32 {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.virtual_start_column as i32)
        .unwrap_or(0)
}

fn pane_slot_viewport_x(slot: i32, state: &AppState) -> f32 {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.viewport_x)
        .unwrap_or_default()
}

fn pane_slot_recursive_search(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .is_some_and(|pane| pane.search.recursive)
}

fn pane_slot_show_location(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .is_some_and(|pane| pane.show_item_locations())
}

fn pane_slot_status(state: &AppState, slot: i32) -> SharedString {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.status.as_str().into())
        .unwrap_or_default()
}

fn pane_slot_selected_count(state: &AppState, slot: i32) -> i32 {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.selection.paths.len() as i32)
        .unwrap_or(0)
}

fn pane_slot_selected_status(state: &AppState, slot: i32) -> SharedString {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| {
            let count = pane.selection.paths.len();
            if count == 0 {
                SharedString::new()
            } else if count == 1 {
                "1 item selected".into()
            } else {
                format!("{count} items selected").into()
            }
        })
        .unwrap_or_default()
}

fn pane_slot_external_edit_active(state: &AppState, slot: i32) -> bool {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| state.external_edits.iter().any(|e| e.pane_id == pane.id))
        .unwrap_or(false)
}

fn pane_slot_external_edit_status(state: &AppState, slot: i32) -> SharedString {
    let pane_id = match state.panes.pane_for_slot(slot) {
        Some(pane) => pane.id,
        None => return SharedString::default(),
    };
    let mut edits = state.external_edits.iter().filter(|e| e.pane_id == pane_id);
    let Some(first) = edits.next() else {
        return SharedString::default();
    };
    let extra = edits.count();
    if extra == 0 {
        let label = first
            .session
            .original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("protected file");
        format!("Admin write-back: {label}").into()
    } else {
        format!("{} admin write-backs pending", extra + 1).into()
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

pub(crate) fn sync_navigation_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let snapshot = {
        let state = state.borrow();
        let focused_slot = state.panes.focused_slot();
        let focused = state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(state.panes.focused());
        NavigationUiSnapshot {
            split_open: state.panes.is_split(),
            focused_slot,
            focused_dir: focused.current_dir.clone(),
            focused_selection: focused.selection.paths.clone(),
        }
    };

    ui.set_split_view_open(snapshot.split_open);
    sync_focused_ui(
        ui,
        snapshot.focused_slot,
        &snapshot.focused_dir,
        &snapshot.focused_selection,
        state,
    );
    sync_pane_slots_ui(ui, state);
}

pub(crate) fn sync_focus_navigation_ui(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    previous_slot: i32,
) {
    let (focused_slot, focused_dir, focused_selection) = {
        let state = state.borrow();
        let focused_slot = state.panes.focused_slot();
        let focused = state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(state.panes.focused());
        (
            focused_slot,
            focused.current_dir.clone(),
            focused.selection.paths.clone(),
        )
    };

    sync_focused_ui(ui, focused_slot, &focused_dir, &focused_selection, state);
    sync_pane_slot_ui(ui, state, previous_slot);
    sync_pane_view_ui(ui, state, previous_slot);
    if previous_slot != focused_slot {
        sync_pane_slot_ui(ui, state, focused_slot);
        sync_pane_view_ui(ui, state, focused_slot);
    }
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
                .expect("split is open, close must succeed")
                .0;
            let status = format!("Split view closed; slot {closed_slot} closed");
            (false, status)
        } else {
            let current_dir = state.panes.focused().current_dir.clone();
            state.panes.open_peer_from_focused();
            state.panes.focused_mut().view.viewport_x = 0.0;
            state.panes.focused_mut().view.invalidate_virtual_view();
            for (_slot, pane) in state.panes.iter_mut().skip(1) {
                pane.view.viewport_x = 0.0;
                pane.view.invalidate_virtual_view();
            }
            (
                true,
                format!("Split view opened at {}", current_dir.display()),
            )
        }
    };

    let slots: Vec<i32> = state.borrow().panes.iter().map(|(s, _)| s).collect();
    if opened {
        for slot in slots {
            set_pane_viewport_ui(ui, slot, 0.0, state);
        }
    }
    if !opened {
        let (viewport_x, slot) = {
            let s = state.borrow();
            (s.panes.focused().view.viewport_x, s.panes.focused_slot())
        };
        set_pane_viewport_ui(ui, slot, viewport_x, state);
    }
    sync_navigation_ui(ui, state);
    let slots = state
        .borrow()
        .panes
        .iter()
        .map(|(slot, _)| slot)
        .collect::<Vec<_>>();
    for slot in slots {
        sync_virtual_entries_for_slot(ui, state, bridge, slot, true);
    }
    set_status(ui, state, &status);
}

#[derive(Debug)]
struct NavigationUiSnapshot {
    split_open: bool,
    focused_slot: i32,
    focused_dir: PathBuf,
    focused_selection: Vec<String>,
}

fn sync_focused_ui(
    ui: &AppWindow,
    slot: i32,
    current_dir: &Path,
    selected_paths: &[String],
    state: &Rc<RefCell<AppState>>,
) {
    ui.set_focused_pane(slot);
    let current_path = current_dir.display().to_string();
    {
        let mut state_ref = state.borrow_mut();
        if let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) {
            pane.path_input_text = current_path.clone();
        }
    }
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
}

pub(crate) fn directory_status_text<'a>(
    entries: impl Iterator<Item = &'a PaneEntrySnapshot>,
) -> String {
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
