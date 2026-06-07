use crate::app::async_bridge::AsyncBridge;
use crate::app::geometry::{ItemViewLayoutEngine, ItemViewLayouter};
use crate::app::item_view_renderer::{
    ItemViewRenderGeometry, ItemViewRenderMetrics, ItemViewRenderPlanInput,
    ItemViewTileFrameRasterInput,
};
use crate::app::pane::{PaneEntrySnapshot, PaneSearch, PaneTarget};
use crate::app::state::AppState;
use crate::config::paths::home_dir;
use crate::fs;
use crate::{
    AppWindow, ItemViewMetadataEntry, ItemViewPaintEntry, PaneSlotData, PaneSurfaceData,
    PaneViewData, set_status, sync_virtual_entries_for_slot,
};
use slint::{Image, Model, ModelRc, SharedString, VecModel};
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
            if sync_pane_surface_viewport_ui(ui, slot, viewport_x) {
                return;
            }
            sync_pane_view_ui(ui, state, slot);
            return;
        }
    }

    sync_pane_view_ui(ui, state, slot);
}

fn sync_pane_surface_viewport_ui(ui: &AppWindow, slot: i32, viewport_x: f32) -> bool {
    let current = ui.get_pane_surfaces();
    for row in 0..current.row_count() {
        let Some(mut current_surface) = current.row_data(row) else {
            continue;
        };
        if current_surface.slot == slot {
            if (current_surface.view.viewport_x - viewport_x).abs() > f32::EPSILON {
                current_surface.view.viewport_x = viewport_x;
                current.set_row_data(row, current_surface);
            }
            return true;
        }
    }
    false
}

pub(crate) fn sync_pane_slots_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let visible_slots = visible_pane_slots(ui);
    let (slots, views, surfaces) = {
        let state_ref = state.borrow();
        let mut slots = Vec::with_capacity(visible_slots.len());
        let mut views = Vec::with_capacity(visible_slots.len());
        let mut surfaces = Vec::with_capacity(visible_slots.len());
        for slot in visible_slots.iter().copied() {
            let pane = pane_slot_data(ui, slot, &state_ref);
            let view = pane_view_data(ui, slot, &state_ref);
            surfaces.push(PaneSurfaceData {
                slot,
                pane: pane.clone(),
                view: view.clone(),
            });
            slots.push(pane);
            views.push(view);
        }
        (slots, views, surfaces)
    };
    if ui.get_pane_slots().row_count() > slots.len() {
        sync_pane_slots_model(ui, slots);
        sync_pane_views_model(ui, views);
        sync_pane_surfaces_model(ui, surfaces);
    } else {
        sync_pane_views_model(ui, views);
        sync_pane_slots_model(ui, slots);
        sync_pane_surfaces_model(ui, surfaces);
    }
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
    sync_pane_view_ui_with_raster_policy(ui, state, slot, false);
}

pub(crate) fn sync_pane_view_ui_defer_raster(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
) {
    sync_pane_view_ui_with_raster_policy(ui, state, slot, true);
}

fn sync_pane_view_ui_with_raster_policy(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    defer_raster: bool,
) {
    let current = ui.get_pane_views();
    for row in 0..current.row_count() {
        let Some(current_view) = current.row_data(row) else {
            continue;
        };
        if current_view.slot == slot {
            let next = {
                let state_ref = state.borrow();
                pane_view_data_with_raster_reuse(
                    ui,
                    slot,
                    &state_ref,
                    defer_raster.then_some(&current_view),
                )
            };
            let rebind_surface = pane_view_requires_surface_rebind(&current_view, &next);
            if current_view != next {
                current.set_row_data(row, next);
            }
            sync_pane_surface_ui_with_rebind_and_raster_policy(
                ui,
                state,
                slot,
                rebind_surface,
                defer_raster,
            );
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
            sync_pane_surface_ui(ui, state, slot);
            return;
        }
    }
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

fn sync_pane_surfaces_model(ui: &AppWindow, surfaces: Vec<PaneSurfaceData>) {
    let current = ui.get_pane_surfaces();
    let same_slots = current.row_count() == surfaces.len()
        && surfaces.iter().enumerate().all(|(row, surface)| {
            current
                .row_data(row)
                .is_some_and(|current| current.slot == surface.slot)
        });
    if same_slots {
        for (row, surface) in surfaces.into_iter().enumerate() {
            if current.row_data(row).as_ref() != Some(&surface) {
                current.set_row_data(row, surface);
            }
        }
        return;
    }

    ui.set_pane_surfaces(ModelRc::new(Rc::new(VecModel::from(surfaces))));
}

fn sync_pane_surface_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    sync_pane_surface_ui_with_rebind(ui, state, slot, false);
}

fn sync_pane_surface_ui_with_rebind(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    rebind: bool,
) {
    sync_pane_surface_ui_with_rebind_and_raster_policy(ui, state, slot, rebind, false);
}

fn sync_pane_surface_ui_with_rebind_and_raster_policy(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    rebind: bool,
    defer_raster: bool,
) {
    let current = ui.get_pane_surfaces();
    for row in 0..current.row_count() {
        let Some(current_surface) = current.row_data(row) else {
            continue;
        };
        if current_surface.slot == slot {
            if rebind {
                replace_pane_surfaces_model(ui, state);
                return;
            }
            let next = {
                let state_ref = state.borrow();
                PaneSurfaceData {
                    slot,
                    pane: pane_slot_data(ui, slot, &state_ref),
                    view: pane_view_data_with_raster_reuse(
                        ui,
                        slot,
                        &state_ref,
                        defer_raster.then_some(&current_surface.view),
                    ),
                }
            };
            if current_surface != next {
                current.set_row_data(row, next);
            }
            return;
        }
    }

    sync_pane_slots_ui(ui, state);
}

fn replace_pane_surfaces_model(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let visible_slots = visible_pane_slots(ui);
    let surfaces = {
        let state_ref = state.borrow();
        visible_slots
            .into_iter()
            .map(|slot| PaneSurfaceData {
                slot,
                pane: pane_slot_data(ui, slot, &state_ref),
                view: pane_view_data(ui, slot, &state_ref),
            })
            .collect::<Vec<_>>()
    };
    ui.set_pane_surfaces(ModelRc::new(Rc::new(VecModel::from(surfaces))));
}

fn pane_view_requires_surface_rebind(current: &PaneViewData, next: &PaneViewData) -> bool {
    (current.entry_count == 0) != (next.entry_count == 0)
        || (current.paint.row_count() == 0) != (next.paint.row_count() == 0)
}

fn pane_slot_data(ui: &AppWindow, slot: i32, state: &AppState) -> PaneSlotData {
    let search = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.search.clone())
        .unwrap_or_default();
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
        search_panel_visible: search.panel_visible(),
        search_panel_height_px: 0.0,
        search_query: search.query.as_str().into(),
        recursive_search: search.recursive,
        search_kind_filter: search.kind_filter,
        search_modified_filter: search.modified_filter,
        search_size_filter: search.size_filter,
        search_loading: search.loading,
        search_filters_active: search.filters_active(),
        search_focus_request: search.focus_request,
        search_kind_label: search_kind_label(search.kind_filter),
        search_modified_label: search_modified_label(search.modified_filter),
        search_size_label: search_size_label(search.size_filter),
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
    pane_view_data_with_raster_reuse(ui, slot, state, None)
}

fn pane_view_data_with_raster_reuse(
    ui: &AppWindow,
    slot: i32,
    state: &AppState,
    reuse_raster_from: Option<&PaneViewData>,
) -> PaneViewData {
    let is_focused = slot == state.panes.focused_slot();
    let search = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.search.clone())
        .unwrap_or_default();
    let item_view_metrics = pane_slot_item_view_metrics(ui, slot, state);
    let item_view_render_geometry =
        pane_slot_item_view_render_geometry(ui, slot, state, item_view_metrics.cell_width);
    let (item_view_raster_layer, item_view_raster_width, item_view_raster_height) =
        if let Some(current) = reuse_raster_from {
            (
                current.item_view_raster_layer.clone(),
                current.item_view_raster_width,
                current.item_view_raster_height,
            )
        } else {
            pane_slot_tile_frame_raster(
                ui,
                slot,
                state,
                item_view_metrics,
                item_view_render_geometry,
            )
        };

    PaneViewData {
        slot,
        paint: pane_slot_paint(slot, state),
        item_view_raster_layer,
        item_view_raster_width,
        item_view_raster_height,
        metadata: pane_slot_metadata(slot, state),
        entry_count: item_view_metrics.entry_count,
        virtual_start_column: item_view_metrics.virtual_start_column,
        virtual_start_row: item_view_metrics.virtual_start_row,
        viewport_x: pane_slot_viewport_x(slot, state),
        item_view_rows_per_column: item_view_metrics.rows_per_column,
        item_view_cell_width: item_view_metrics.cell_width,
        item_view_row_height: item_view_metrics.row_height,
        item_view_padding: item_view_metrics.padding,
        item_view_content_width: item_view_metrics.content_width,
        item_view_virtual_slice_start_x: item_view_metrics.virtual_slice_start_x,
        item_view_virtual_slice_width: item_view_metrics.virtual_slice_width,
        item_view_scroll_max_x: item_view_metrics.scroll_max_x,
        item_view_media_x: item_view_render_geometry.media_x,
        item_view_media_y: item_view_render_geometry.media_y,
        item_view_media_width: item_view_render_geometry.media_width,
        item_view_media_height: item_view_render_geometry.media_height,
        item_view_text_x: item_view_render_geometry.text_x,
        item_view_text_width: item_view_render_geometry.text_width,
        item_view_title_y: item_view_render_geometry.title_y,
        item_view_title_line_height: item_view_render_geometry.title_line_height,
        item_view_title_font_size: item_view_render_geometry.title_font_size,
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
        empty_title: empty_title_for_search(&search),
        empty_subtitle: empty_subtitle_for_search(&search),
    }
}

fn pane_slot_item_view_render_geometry(
    ui: &AppWindow,
    slot: i32,
    state: &AppState,
    cell_width: f32,
) -> ItemViewRenderGeometry {
    let (show_location, text_line_count) = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| (pane.show_item_locations(), pane.item_view_text_line_count()))
        .unwrap_or((false, 1));
    ItemViewRenderGeometry::from_plan_input(ItemViewRenderPlanInput {
        cell_width,
        render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(
            ui.get_icon_zoom_level(),
            text_line_count,
        ),
        show_location,
    })
}

fn pane_slot_tile_frame_raster(
    ui: &AppWindow,
    slot: i32,
    state: &AppState,
    metrics: ItemViewSlotMetrics,
    render_geometry: ItemViewRenderGeometry,
) -> (Image, f32, f32) {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return (Image::default(), 1.0, 1.0);
    };
    if pane.view.virtual_entry_tokens.is_empty()
        || pane.view.virtual_bounds_entries.row_count() == 0
    {
        return (Image::default(), 1.0, 1.0);
    }

    let raster_width = raster_dimension_px(metrics.virtual_slice_width);
    let raster_height =
        raster_dimension_px(metrics.rows_per_column.max(1) as f32 * metrics.row_height);
    let raster = pane
        .view
        .tile_frame_raster_layer(ItemViewTileFrameRasterInput {
            width: raster_width,
            height: raster_height,
            content_origin_x: metrics.virtual_slice_start_x,
            drop_target_slice_index: -1,
            dark: ui.get_dark_mode(),
            tile_height: metrics.row_height,
            media_x: render_geometry.media_x,
            media_y: render_geometry.media_y,
            media_width: render_geometry.media_width,
            media_height: render_geometry.media_height,
        });
    (raster.image, raster.width as f32, raster.height as f32)
}

fn raster_dimension_px(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    value.ceil().min(u32::MAX as f32) as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemViewSlotMetrics {
    entry_count: i32,
    virtual_start_column: i32,
    virtual_start_row: i32,
    rows_per_column: i32,
    cell_width: f32,
    row_height: f32,
    padding: f32,
    content_width: f32,
    virtual_slice_start_x: f32,
    virtual_slice_width: f32,
    scroll_max_x: f32,
}

fn pane_slot_item_view_metrics(
    _ui: &AppWindow,
    slot: i32,
    state: &AppState,
) -> ItemViewSlotMetrics {
    let (compact_item_view, virtual_slice_count, virtual_start_index) = state
        .panes
        .pane_for_slot(slot)
        .map(|pane| {
            (
                pane.view
                    .virtual_view
                    .layout
                    .clone()
                    .unwrap_or_else(|| ItemViewLayoutEngine::empty_compact().into()),
                pane.view.virtual_entries.row_count(),
                pane.view.virtual_start_index,
            )
        })
        .unwrap_or_else(|| (ItemViewLayoutEngine::empty_compact().into(), 0, 0));
    let layout_metrics = compact_item_view.layout_metrics();
    let virtual_anchor = compact_item_view.range_anchor(virtual_start_index);
    let virtual_slice_geometry =
        compact_item_view.virtual_slice_geometry(virtual_start_index, virtual_slice_count);

    ItemViewSlotMetrics {
        entry_count: layout_metrics.entry_count as i32,
        virtual_start_column: virtual_anchor.start_column as i32,
        virtual_start_row: virtual_anchor.start_row as i32,
        rows_per_column: layout_metrics.rows_per_column as i32,
        cell_width: layout_metrics.cell_width,
        row_height: layout_metrics.row_height,
        padding: layout_metrics.padding,
        content_width: layout_metrics.content_width,
        virtual_slice_start_x: virtual_slice_geometry.start_x,
        virtual_slice_width: virtual_slice_geometry.width,
        scroll_max_x: layout_metrics.scroll_max_x,
    }
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

fn pane_slot_paint(slot: i32, state: &AppState) -> ModelRc<ItemViewPaintEntry> {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.virtual_paint_entries.clone())
        .unwrap_or_default()
}

fn pane_slot_metadata(slot: i32, state: &AppState) -> ModelRc<ItemViewMetadataEntry> {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.virtual_metadata_entries.clone())
        .unwrap_or_default()
}

fn pane_slot_viewport_x(slot: i32, state: &AppState) -> f32 {
    state
        .panes
        .pane_for_slot(slot)
        .map(|pane| pane.view.viewport_x)
        .unwrap_or_default()
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

fn search_kind_label(filter: i32) -> SharedString {
    match filter {
        1 => "Type: Folders",
        2 => "Type: Files",
        3 => "Type: Images",
        _ => "Type: All",
    }
    .into()
}

fn search_modified_label(filter: i32) -> SharedString {
    match filter {
        1 => "Modified: Today",
        2 => "Modified: 7 days",
        3 => "Modified: 30 days",
        _ => "Modified: Any",
    }
    .into()
}

fn search_size_label(filter: i32) -> SharedString {
    match filter {
        1 => "Size: < 1 MB",
        2 => "Size: 1-100 MB",
        3 => "Size: > 100 MB",
        _ => "Size: Any",
    }
    .into()
}

fn empty_title_for_search(search: &PaneSearch) -> SharedString {
    if search.loading {
        "Searching...".into()
    } else if search.query.is_empty() && !search.filters_active() {
        "This folder is empty".into()
    } else {
        "No matching items".into()
    }
}

fn empty_subtitle_for_search(search: &PaneSearch) -> SharedString {
    if search.loading {
        "Scanning subfolders.".into()
    } else if search.query.is_empty() && !search.filters_active() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Image;

    fn pane_view(entry_count: i32, visible_rows: usize) -> PaneViewData {
        PaneViewData {
            slot: 0,
            paint: if visible_rows == 0 {
                ModelRc::default()
            } else {
                ModelRc::new(Rc::new(VecModel::from(
                    (0..visible_rows)
                        .map(|index| ItemViewPaintEntry {
                            name: format!("item-{index}").into(),
                            x: index as f32 * 10.0,
                            y: 0.0,
                            width: 80.0,
                            text_width: 64.0,
                        })
                        .collect::<Vec<_>>(),
                )))
            },
            item_view_raster_layer: Image::default(),
            item_view_raster_width: 1.0,
            item_view_raster_height: 1.0,
            metadata: ModelRc::default(),
            entry_count,
            virtual_start_column: 0,
            virtual_start_row: 0,
            viewport_x: 0.0,
            item_view_rows_per_column: 4,
            item_view_cell_width: 120.0,
            item_view_row_height: 80.0,
            item_view_padding: 10.0,
            item_view_content_width: 300.0,
            item_view_virtual_slice_start_x: 0.0,
            item_view_virtual_slice_width: 240.0,
            item_view_scroll_max_x: 200.0,
            item_view_media_x: 0.0,
            item_view_media_y: 0.0,
            item_view_media_width: 32.0,
            item_view_media_height: 32.0,
            item_view_text_x: 40.0,
            item_view_text_width: 80.0,
            item_view_title_y: 10.0,
            item_view_title_line_height: 18.0,
            item_view_title_font_size: 14.0,
            show_location: false,
            content_interactive: true,
            drop_ready: true,
            empty_message_visible: true,
            empty_title: SharedString::new(),
            empty_subtitle: SharedString::new(),
        }
    }

    #[test]
    fn pane_surface_rebind_is_limited_to_empty_model_boundary() {
        let empty = pane_view(0, 0);
        let nonempty = pane_view(24, 8);

        assert!(pane_view_requires_surface_rebind(&empty, &nonempty));
        assert!(pane_view_requires_surface_rebind(&nonempty, &empty));

        let mut scrolled = nonempty.clone();
        scrolled.virtual_start_column = 3;
        scrolled.viewport_x = 240.0;
        assert!(!pane_view_requires_surface_rebind(&nonempty, &scrolled));

        let mut selected = nonempty.clone();
        selected.item_view_raster_width = 241.0;
        assert!(!pane_view_requires_surface_rebind(&nonempty, &selected));
    }
}
