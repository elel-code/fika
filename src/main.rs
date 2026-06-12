mod cli;
mod ui;

use cli::{Args, Mode};
#[cfg(test)]
use fika_core::SystemdLaunchResult;
#[cfg(test)]
use fika_core::{
    CompactLayout, CompactLayoutOptions, ServiceMenuAction, ViewState, home_dir,
    is_network_root_path, network_root_path,
};
use fika_core::{
    CreateItemResult, FileTransferMode, RenameItemResult, TransferTaskResult, TrashSelectionResult,
    TrashViewOperation, TrashViewOperationResult, UndoTaskResult, action_status,
    create_item_result, created_item_label, rename_item_result, transfer_paths_result,
    trash_selection_result, trash_view_operation_result, undo_record_result,
};
use fika_core::{
    CreateUndoItem, CreatedItemKind, DeviceInfo, DeviceMonitorMessage, DevicePlaceOperation,
    DevicePlaceOperationResult, DirectoryListerEvent, ListingRequest, ListingWorker,
    LoadingPaneState, OperationQueue, PaneController, PaneId, RenameUndoItem, ScrollBounds,
    ScrollDragTracker, SelectionMove, SmoothScroll, SortDescriptor, SortOrder, SortRole,
    UndoPayload, UserPlace, ViewPoint, ViewRect, ZoomChange, breadcrumb_segments,
    complete_location_input, file_ops, listing_requests_from_events, nearest_existing_ancestor,
    perform_device_place_operation, resolve_location_input, update_loading_state_for_event,
};
use fika_core::{
    DesktopLaunchPlan, LauncherError, MimeApplication, MimeApplicationCache, NewWindowLaunchResult,
    OpenWithLaunchResult, ServiceMenuLaunchResult, ServiceMenuTarget, ark_compress_launch_plan,
    ark_extract_here_launch_plan, ark_extract_to_launch_plan, current_executable_launch_plan,
    launch_with_systemd_user, service_menu_target_label, set_default_mime_application,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, IntoElement, ParentElement, Render, ScrollDelta, Styled,
    Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use ui::application_chooser::{ApplicationChooserState, application_chooser_overlay};
use ui::chooser::{ChooserState, selected_choice_rows};
use ui::clipboard::{
    ClipboardMode, ClipboardState, paste_clipboard_result, primary_paste_clipboard_state,
    standard_paste_clipboard_state,
};
#[cfg(test)]
use ui::context_menu::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING, CONTEXT_MENU_VIEWPORT_MARGIN,
    ContextMenuIcon, context_menu_actions, context_menu_overlay_layout, context_submenu_actions,
};
use ui::context_menu::{
    ContextMenuAction, ContextMenuNestedSubmenu, ContextMenuOpenSubmenu, ContextMenuState,
    ContextMenuSubmenu, ContextMenuTarget, context_menu_icon_snapshots, context_menu_overlay,
};
use ui::drag_drop::{
    ActiveItemDrag, ItemDragPayload, ItemDropTarget, PlaceDropTarget, item_drag_paths,
    item_drop_reject_reason, item_drop_target_mode_for_directory, item_drop_target_mode_for_pane,
};
#[cfg(test)]
use ui::drag_drop::{
    drag_cursor_style_for_transfer_mode, file_transfer_mode_for_modifiers,
    place_drop_target_matches_insert, place_drop_target_mode_for_place,
};
use ui::file_grid::{
    CompactColumnWidthCache, ContentItemHit, PaneLayoutProjection, PaneViewportGeometry,
    VisibleItemSlotPool, VisibleItemSnapshot, compact_layout_for_filtered_model,
    compact_layout_for_model, compact_text_width, format_entry_kind_label,
    model_index_for_layout_index, visible_item_thumbnail_path,
};
use ui::filter_bar::{
    FilterBarSnapshot, FilteredModelCacheEntry, FilteredModelCacheKey, PaneFilterState,
    filter_source_revision,
};
use ui::icons::FileIconCache;
use ui::location_bar::{LocationDraft, LocationEditMetrics};
use ui::pane::{
    MIN_PANE_WIDTH, PANE_SPLITTER_WIDTH, PaneSnapshot, PaneSplitterDrag, normalize_pane_ratios,
    pane_row_width_from_child_bounds, pane_splitter, pane_width_available, sort_order_label,
    sort_role_label, split_ratio_eq, width_value_eq,
};
use ui::place_draft::{PlaceDraft, PlaceDraftField, place_draft_overlay};
use ui::places::{
    DEVICES_GROUP, PlaceEntry, PlaceSnapshot, REMOVABLE_DEVICES_GROUP, build_places,
    default_place_label, place_snapshots_for, read_live_device_snapshot,
    removable_device_place_entries,
};
#[cfg(test)]
use ui::places::{NETWORK_GROUP, active_place_index, build_places_with_devices, place_is_mounted};
use ui::properties_dialog::{
    PropertiesDialogState, properties_dialog_overlay, properties_for_path, properties_for_selection,
};
use ui::rename::RenameDraft;
use ui::rubber_band::RubberBandState;
use ui::scrollbar::{ActiveScrollBarDrag, HorizontalScrollBarTrack};
use ui::shortcuts::{
    FilterInputAction, LocationInputAction, PaneShortcut, PlaceInputAction, RenameInputAction,
    filter_input_action, location_input_action, pane_shortcut, place_input_action,
    rename_input_action, zoom_change_for_wheel_delta,
};
use ui::status_bar::{
    OperationProgressHandle, OperationProgressSnapshot, SpaceInfoCache, SpaceInfoSnapshot,
    StatusBarSnapshot, StatusSummaryCacheEntry, StatusSummaryCacheKey, filesystem_space_info,
    progress_delay_elapsed, status_summary_for_model, status_summary_for_model_indexes,
};
#[cfg(test)]
use ui::status_bar::{
    PROGRESS_DISPLAY_DELAY, parse_df_space_output, progress_percent, space_info_snapshot,
};

const DROP_TARGET_STALE_TIMEOUT: Duration = Duration::from_millis(3000);
const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const DEVICE_MONITOR_RETRY_INTERVAL: Duration = Duration::from_secs(60);

const CONTEXT_SUBMENU_HIDE_DELAY: Duration = Duration::from_millis(300);
pub(crate) struct FikaApp {
    pub(crate) panes: PaneController,
    places: Vec<PlaceEntry>,
    hidden_places: BTreeSet<PathBuf>,
    hidden_place_sections: BTreeSet<&'static str>,
    user_places_path: PathBuf,
    device_refresh_pending: bool,
    next_device_refresh_at: Instant,
    device_monitor_rx: Option<mpsc::Receiver<DeviceMonitorMessage>>,
    device_monitor_active: bool,
    next_device_monitor_start_at: Instant,
    file_icons: FileIconCache,
    mime_applications: MimeApplicationCache,
    space_info: SpaceInfoCache,
    status_summaries: HashMap<PaneId, StatusSummaryCacheEntry>,
    loading_panes: HashMap<PaneId, LoadingPaneState>,
    smooth_scrolls: HashMap<PaneId, SmoothScroll>,
    scroll_drag_trackers: HashMap<PaneId, ScrollDragTracker>,
    active_scrollbar_drag: Option<ActiveScrollBarDrag>,
    horizontal_scrollbar_tracks: HashMap<PaneId, HorizontalScrollBarTrack>,
    pane_viewport_geometries: HashMap<PaneId, PaneViewportGeometry>,
    pane_split_ratios: HashMap<PaneId, f32>,
    pane_row_width: f32,
    visible_item_slots: HashMap<PaneId, VisibleItemSlotPool>,
    compact_column_widths: HashMap<PaneId, CompactColumnWidthCache>,
    pane_filters: HashMap<PaneId, PaneFilterState>,
    filtered_models: HashMap<PaneId, FilteredModelCacheEntry>,
    operations: OperationQueue,
    clipboard: Option<ClipboardState>,
    active_item_drag: Option<ActiveItemDrag>,
    item_drop_target: Option<ItemDropTarget>,
    place_drop_target: Option<PlaceDropTarget>,
    drop_target_stale_generation: u64,
    drop_target_stale_timer_running: bool,
    rename_draft: Option<RenameDraft>,
    location_draft: Option<LocationDraft>,
    location_edit_metrics: HashMap<PaneId, LocationEditMetrics>,
    place_draft: Option<PlaceDraft>,
    chooser: Option<ChooserState>,
    listing_worker: ListingWorker,
    _keystroke_subscription: Option<gpui::Subscription>,
    pub(crate) rubber_band: Option<RubberBandState>,
    rubber_band_selection_panes: HashSet<PaneId>,
    context_menu: Option<ContextMenuState>,
    context_menu_tree_hovered: bool,
    context_submenu_hide_generation: u64,
    properties_dialog: Option<PropertiesDialogState>,
    application_chooser: Option<ApplicationChooserState>,
    pane_statuses: HashMap<PaneId, String>,
    operation_pending: bool,
    operation_pane: Option<PaneId>,
    operation_progress: Option<OperationProgressHandle>,
}

impl FikaApp {
    fn new(args: Args, cx: &mut Context<Self>) -> Self {
        let user_places_path = fika_core::default_user_places_path();
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
        let initial_devices = fika_core::read_mountinfo_devices().unwrap_or_default();
        let mut app = Self {
            panes: PaneController::new(args.start_dir.clone()),
            places: build_places(&user_places_path),
            hidden_places: BTreeSet::new(),
            hidden_place_sections: BTreeSet::new(),
            user_places_path,
            device_refresh_pending: false,
            next_device_refresh_at: Instant::now(),
            device_monitor_rx: None,
            device_monitor_active: false,
            next_device_monitor_start_at: Instant::now(),
            file_icons: FileIconCache::default(),
            mime_applications: MimeApplicationCache::load(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            smooth_scrolls: HashMap::new(),
            scroll_drag_trackers: HashMap::new(),
            active_scrollbar_drag: None,
            horizontal_scrollbar_tracks: HashMap::new(),
            pane_viewport_geometries: HashMap::new(),
            pane_split_ratios: HashMap::new(),
            pane_row_width: 0.0,
            visible_item_slots: HashMap::new(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            active_item_drag: None,
            item_drop_target: None,
            place_drop_target: None,
            drop_target_stale_generation: 0,
            drop_target_stale_timer_running: false,
            rename_draft: None,
            location_draft: None,
            location_edit_metrics: HashMap::new(),
            place_draft: None,
            chooser,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: None,
            rubber_band_selection_panes: HashSet::new(),
            context_menu: None,
            context_menu_tree_hovered: false,
            context_submenu_hide_generation: 0,
            properties_dialog: None,
            application_chooser: None,
            pane_statuses: HashMap::new(),
            operation_pending: false,
            operation_pane: None,
            operation_progress: None,
        };
        app.replace_removable_device_places(&initial_devices);
        app._keystroke_subscription = Some(cx.observe_keystrokes(|this, event, _window, cx| {
            if this.handle_keystroke(event, cx) {
                cx.notify();
            }
        }));
        let first = app.panes.focused().expect("initial pane exists");
        app.load_pane(first, args.start_dir);
        app.start_watchers();
        app.maybe_start_device_monitor(cx);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(350))
                            .await;
                        if this
                            .update(&mut cx, |app, cx| {
                                if app.drain_background_listing_results()
                                    | app.drain_watchers()
                                    | app.drain_device_monitor_messages()
                                    | app.operation_progress.is_some()
                                    | !app.loading_panes.is_empty()
                                {
                                    cx.notify();
                                }
                                app.maybe_start_device_monitor(cx);
                                app.maybe_start_device_refresh(cx);
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            },
        )
        .detach();
        app
    }

    fn active_filter_for_pane(&self, pane_id: PaneId) -> Option<fika_core::NameFilter> {
        self.pane_filters
            .get(&pane_id)
            .and_then(PaneFilterState::active_filter)
    }

    fn filtered_model_for_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<(fika_core::FilteredModel, u64)> {
        let Some(filter) = self.active_filter_for_pane(pane_id) else {
            self.filtered_models.remove(&pane_id);
            return None;
        };
        let source_revision = filter_source_revision(&filter);
        let model_generation = self.panes.pane(pane_id)?.model.data_generation();
        let key = FilteredModelCacheKey {
            model_generation,
            filter: filter.clone(),
        };
        if let Some(cached) = self
            .filtered_models
            .get(&pane_id)
            .filter(|cached| cached.key == key)
        {
            return Some((cached.model.clone(), source_revision));
        }

        let model = {
            let pane = self.panes.pane(pane_id)?;
            fika_core::FilteredModel::from_model(&pane.model, &filter)
        };
        self.filtered_models.insert(
            pane_id,
            FilteredModelCacheEntry {
                key,
                model: model.clone(),
            },
        );
        Some((model, source_revision))
    }

    fn filter_bar_snapshot(
        &self,
        pane_id: PaneId,
        focused_pane: Option<PaneId>,
        match_count: usize,
    ) -> Option<FilterBarSnapshot> {
        let filter = self
            .pane_filters
            .get(&pane_id)
            .filter(|filter| filter.visible)?;
        Some(FilterBarSnapshot {
            query: filter.query.clone(),
            focused: filter.focused && focused_pane == Some(pane_id),
            case_sensitive: filter.case_sensitive,
            mode: filter.mode,
            match_count,
        })
    }

    pub(crate) fn show_filter_bar(&mut self, pane_id: PaneId) {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        self.set_pane_status(pane_id, "Filter");
    }

    pub(crate) fn focus_filter_bar(&mut self, pane_id: PaneId) {
        self.show_filter_bar(pane_id);
    }

    pub(crate) fn close_filter_bar(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.visible = false;
            filter.focused = false;
            filter.query.clear();
        }
        self.invalidate_filter_projection(pane_id);
        self.set_pane_status(pane_id, "Filter closed");
    }

    fn set_filter_query(&mut self, pane_id: PaneId, query: String) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        if filter.query == query {
            return;
        }
        filter.query = query;
        self.invalidate_filter_projection(pane_id);
        self.set_pane_status(pane_id, "Filtering");
    }

    pub(crate) fn toggle_filter_case_sensitive(&mut self, pane_id: PaneId) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        filter.case_sensitive = !filter.case_sensitive;
        let enabled = filter.case_sensitive;
        self.invalidate_filter_projection(pane_id);
        let message = if enabled {
            "Filter match case"
        } else {
            "Filter ignore case"
        };
        self.set_pane_status(pane_id, message);
    }

    pub(crate) fn toggle_filter_mode(&mut self, pane_id: PaneId) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        filter.mode = match filter.mode {
            fika_core::NameFilterMode::PlainText => fika_core::NameFilterMode::Glob,
            fika_core::NameFilterMode::Glob => fika_core::NameFilterMode::PlainText,
        };
        let mode = filter.mode;
        self.invalidate_filter_projection(pane_id);
        let message = match mode {
            fika_core::NameFilterMode::PlainText => "Plain text filter",
            fika_core::NameFilterMode::Glob => "Glob filter",
        };
        self.set_pane_status(pane_id, message);
    }

    fn clear_filter_query_for_pane(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.query.clear();
        }
        self.invalidate_filter_projection(pane_id);
    }

    fn clear_filter_query_for_url_change(&mut self, pane_id: PaneId) {
        let Some(filter) = self.pane_filters.get_mut(&pane_id) else {
            return;
        };
        if filter.query.is_empty() {
            return;
        }
        filter.query.clear();
        filter.focused = false;
        self.filtered_models.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
    }

    fn invalidate_filter_projection(&mut self, pane_id: PaneId) {
        self.invalidate_pane_layout_projection(pane_id, true);
    }

    fn invalidate_pane_layout_projection(&mut self, pane_id: PaneId, reset_scroll: bool) {
        self.visible_item_slots.remove(&pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        if let Some(pane) = self.panes.pane_mut(pane_id) {
            if reset_scroll {
                pane.view.reset_scroll();
            }
        }
    }

    fn clear_filter_focus_for_pane(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.focused = false;
        }
    }

    fn handle_filter_keystroke(&mut self, pane_id: PaneId, keystroke: &gpui::Keystroke) -> bool {
        if !self
            .pane_filters
            .get(&pane_id)
            .is_some_and(|filter| filter.visible && filter.focused)
        {
            return false;
        }

        match filter_input_action(keystroke) {
            FilterInputAction::Cancel => {
                let query_empty = self
                    .pane_filters
                    .get(&pane_id)
                    .is_none_or(|filter| filter.query.is_empty());
                if query_empty {
                    self.close_filter_bar(pane_id);
                } else {
                    self.clear_filter_query_for_pane(pane_id);
                    self.set_pane_status(pane_id, "Filter cleared");
                }
            }
            FilterInputAction::FocusView => {
                self.clear_filter_focus_for_pane(pane_id);
            }
            FilterInputAction::Backspace => {
                let next = self
                    .pane_filters
                    .get(&pane_id)
                    .map(|filter| {
                        let mut query = filter.query.clone();
                        query.pop();
                        query
                    })
                    .unwrap_or_default();
                self.set_filter_query(pane_id, next);
            }
            FilterInputAction::Insert(text) => {
                let mut next = self
                    .pane_filters
                    .get(&pane_id)
                    .map(|filter| filter.query.clone())
                    .unwrap_or_default();
                next.push_str(&text);
                self.set_filter_query(pane_id, next);
            }
            FilterInputAction::PassToView => {
                self.clear_filter_focus_for_pane(pane_id);
                return false;
            }
            FilterInputAction::Ignore => return false,
        }
        true
    }

    fn snapshots(&mut self, cx: &mut Context<Self>) -> Vec<PaneSnapshot> {
        let focused_pane = self.panes.focused();
        let pane_ids = self.panes.pane_ids().to_vec();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| {
                let filtered_model = self.filtered_model_for_pane(pane_id);
                let split_ratio = self.pane_split_ratio(pane_id);
                let projected_viewport_width = self.projected_pane_width(pane_id);
                let item_drop_target = self.item_drop_target.clone();
                let pane_drop_target =
                    item_drop_target_mode_for_pane(item_drop_target.as_ref(), pane_id);
                let (
                    breadcrumbs,
                    location_draft,
                    filter_bar,
                    layout,
                    view,
                    rubber_band,
                    focused,
                    selection_count,
                    visible_data,
                ) = {
                    let pane = self.panes.pane(pane_id)?;
                    let mut view = pane.view.clone();
                    if let Some(projected_viewport_width) = projected_viewport_width
                        && projected_viewport_width > 0.0
                    {
                        view.viewport_width = projected_viewport_width.floor();
                    }
                    let filtered = filtered_model.as_ref().map(|(model, _)| model);
                    let source_revision =
                        filtered_model.as_ref().map_or(0, |(_, revision)| *revision);
                    let rename_draft = self
                        .rename_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id);
                    let location_draft = self
                        .location_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id)
                        .map(LocationDraft::snapshot);
                    let layout = match filtered {
                        Some(filtered) => compact_layout_for_filtered_model(
                            self.compact_column_widths.entry(pane_id).or_default(),
                            &pane.model,
                            filtered,
                            source_revision,
                            &view,
                        ),
                        None => compact_layout_for_model(
                            self.compact_column_widths.entry(pane_id).or_default(),
                            &pane.model,
                            &view,
                        ),
                    };
                    let selection_count = pane.selection.count_for_model(pane.model.len());
                    let visible_data = layout
                        .visible_items()
                        .filter_map(|visible_item| {
                            let layout_index = visible_item.model_index;
                            let model_index = model_index_for_layout_index(filtered, layout_index)?;
                            let entry = pane.model.get(model_index)?;
                            let path = pane.model.path_for_index(model_index)?;
                            let item_layout = layout.item_with_required_text_width(
                                layout_index,
                                Some(compact_text_width(entry.name_width_units)),
                            )?;
                            let selected = pane.selection.is_selected(entry.id);
                            let drop_target = item_drop_target_mode_for_directory(
                                item_drop_target.as_ref(),
                                pane_id,
                                &path,
                            );
                            let draft_name = rename_draft
                                .filter(|draft| draft.original_path == path)
                                .map(|draft| draft.draft_name.clone());
                            Some((
                                item_layout,
                                entry.id,
                                path,
                                entry.is_dir,
                                entry.name.clone(),
                                format_entry_kind_label(entry),
                                visible_item_thumbnail_path(entry),
                                entry.mime_type.clone(),
                                selected,
                                drop_target,
                                draft_name,
                            ))
                        })
                        .collect::<Vec<_>>();
                    (
                        breadcrumb_segments(&pane.current_dir),
                        location_draft,
                        self.filter_bar_snapshot(
                            pane_id,
                            focused_pane,
                            filtered
                                .map_or_else(|| pane.model.len(), fika_core::FilteredModel::len),
                        ),
                        layout,
                        view.clone(),
                        self.rubber_band.and_then(|band| {
                            (band.pane_id == pane_id).then(|| band.viewport_rect(&view))
                        }),
                        focused_pane == Some(pane_id),
                        selection_count,
                        visible_data,
                    )
                };
                let visible_ids = visible_data
                    .iter()
                    .map(|(_, item_id, _, _, _, _, _, _, _, _, _)| *item_id);
                let slot_by_item_id = self
                    .visible_item_slots
                    .entry(pane_id)
                    .or_default()
                    .slots_for_items(visible_ids);
                let visible_items = visible_data
                    .into_iter()
                    .filter_map(
                        |(
                            layout,
                            item_id,
                            path,
                            is_dir,
                            name,
                            kind_label,
                            thumbnail_path,
                            mime_type,
                            selected,
                            drop_target,
                            draft_name,
                        )| {
                            let slot_id = slot_by_item_id.get(&item_id).copied()?;
                            let icon = self.file_icons.icon_for(
                                &path,
                                is_dir,
                                mime_type,
                                layout.icon_rect.width,
                            );
                            Some(VisibleItemSnapshot {
                                slot_id,
                                layout,
                                path,
                                is_dir,
                                name,
                                kind_label,
                                thumbnail_path,
                                icon,
                                selected,
                                selection_count,
                                drop_target,
                                draft_name,
                            })
                        },
                    )
                    .collect::<Vec<_>>();
                let status_bar = self.status_bar_snapshot_for_pane(pane_id, cx);
                Some(PaneSnapshot {
                    id: pane_id,
                    split_ratio,
                    breadcrumbs,
                    location_draft,
                    filter_bar,
                    status_bar,
                    layout,
                    visible_items,
                    view,
                    rubber_band,
                    drop_target: pane_drop_target,
                    scrollbar_drag_active: self
                        .active_scrollbar_drag
                        .is_some_and(|drag| drag.pane_id == pane_id),
                    focused,
                })
            })
            .collect()
    }

    fn status_bar_snapshot_for_pane(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> StatusBarSnapshot {
        let now = Instant::now();
        let message = self.status_message_for_pane(pane_id);
        let operation_pending = self.operation_pane == Some(pane_id) && self.operation_pending;
        let Some((path, zoom_level, zoom_icon_size)) = self.panes.pane(pane_id).map(|pane| {
            (
                pane.current_dir.clone(),
                pane.view.zoom_level,
                pane.view.icon_size(),
            )
        }) else {
            return StatusBarSnapshot {
                message,
                item_summary: "0 folders, 0 files".to_string(),
                free_space: None,
                zoom_level: fika_core::DEFAULT_ZOOM_LEVEL,
                zoom_icon_size: fika_core::icon_size_for_zoom_level(fika_core::DEFAULT_ZOOM_LEVEL),
                zoom_min: fika_core::MIN_ZOOM_LEVEL,
                zoom_max: fika_core::MAX_ZOOM_LEVEL,
                operation_pending,
                operation_progress: self.operation_progress_snapshot_for_pane(pane_id, now),
            };
        };

        self.request_space_info_if_needed(path.clone(), cx);
        let operation_progress = self
            .operation_progress_snapshot_for_pane(pane_id, now)
            .or_else(|| self.loading_progress_snapshot(pane_id, now));
        let item_summary = self
            .loading_panes
            .get(&pane_id)
            .and_then(|loading| loading.previous_summary.clone())
            .or_else(|| self.status_summary_for_pane(pane_id))
            .unwrap_or_else(|| "0 folders, 0 files".to_string());
        StatusBarSnapshot {
            message,
            item_summary,
            free_space: self.space_info.snapshot_for(&path),
            zoom_level,
            zoom_icon_size,
            zoom_min: fika_core::MIN_ZOOM_LEVEL,
            zoom_max: fika_core::MAX_ZOOM_LEVEL,
            operation_pending,
            operation_progress,
        }
    }

    fn status_message_for_pane(&self, pane_id: PaneId) -> String {
        self.pane_statuses
            .get(&pane_id)
            .filter(|message| !message.is_empty())
            .cloned()
            .unwrap_or_else(|| "Ready".to_string())
    }

    fn set_pane_status(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.pane_statuses.insert(pane_id, message.into());
    }

    fn begin_pane_operation(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.operation_pending = true;
        self.operation_pane = Some(pane_id);
        self.set_pane_status(pane_id, message);
    }

    fn finish_pane_operation(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.operation_pending = false;
        self.operation_pane = None;
        self.set_pane_status(pane_id, message);
    }

    fn operation_progress_snapshot_for_pane(
        &self,
        pane_id: PaneId,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        self.operation_progress
            .as_ref()
            .filter(|progress| progress.pane_id == pane_id)
            .and_then(|progress| progress.snapshot(now))
    }

    fn loading_progress_snapshot(
        &self,
        pane_id: PaneId,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        self.loading_panes.get(&pane_id).and_then(|loading| {
            progress_delay_elapsed(loading.started_at, now).then(|| OperationProgressSnapshot {
                label: "Loading".to_string(),
                bytes_done: 0,
                bytes_total: 0,
                percent: None,
                cancellable: true,
            })
        })
    }

    fn start_transfer_progress(
        &mut self,
        pane_id: PaneId,
        label: String,
    ) -> (Arc<AtomicBool>, Arc<Mutex<file_ops::TransferProgress>>) {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(file_ops::TransferProgress::default()));
        self.operation_progress = Some(OperationProgressHandle {
            pane_id,
            label,
            progress: Arc::clone(&progress),
            cancel: Some(Arc::clone(&cancel)),
            started_at: Instant::now(),
        });
        (cancel, progress)
    }

    fn clear_operation_progress(&mut self) {
        self.operation_progress = None;
    }

    pub(crate) fn cancel_operation_or_loading(&mut self, pane_id: PaneId) {
        if let Some(progress) = &self.operation_progress
            && progress.pane_id == pane_id
            && let Some(cancel) = &progress.cancel
        {
            cancel.store(true, Ordering::Relaxed);
            self.set_pane_status(pane_id, format!("Cancelling {}", progress.label));
            return;
        }
        self.cancel_loading(pane_id);
    }

    pub(crate) fn cancel_loading(&mut self, pane_id: PaneId) {
        if self.loading_panes.remove(&pane_id).is_some() {
            self.listing_worker.cancel_pane(pane_id);
            self.set_pane_status(pane_id, "Loading stopped");
        }
    }

    fn status_summary_for_pane(&mut self, pane_id: PaneId) -> Option<String> {
        let filtered = self.filtered_model_for_pane(pane_id);
        let (key, summary) = {
            let pane = self.panes.pane(pane_id)?;
            let filter_revision = filtered.as_ref().map_or(0, |(_, revision)| *revision);
            let visible_len = filtered
                .as_ref()
                .map_or_else(|| pane.model.len(), |(filtered, _)| filtered.len());
            let selection_count = pane.selection.count_for_model(pane.model.len());
            let key = StatusSummaryCacheKey {
                model_generation: pane.model.data_generation(),
                model_len: pane.model.len(),
                filter_revision,
                visible_len,
                selection_count,
                selection_revision: pane.selection.revision(),
            };
            if let Some(cached) = self
                .status_summaries
                .get(&pane_id)
                .filter(|cached| cached.key == key)
            {
                return Some(cached.summary.clone());
            }
            let summary = match filtered {
                Some((filtered, _)) if pane.selection.is_empty() => {
                    status_summary_for_model_indexes(
                        pane.model.entries(),
                        filtered.iter_model_indexes(),
                        &pane.selection,
                    )
                }
                _ => status_summary_for_model(pane.model.entries(), &pane.selection),
            };
            (key, summary)
        };
        self.status_summaries.insert(
            pane_id,
            StatusSummaryCacheEntry {
                key,
                summary: summary.clone(),
            },
        );
        Some(summary)
    }

    fn request_space_info_if_needed(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let now = Instant::now();
        if !self.space_info.should_request(&path, now) {
            return;
        }
        self.space_info.start_request(path.clone(), now);

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let request_path = path.clone();
                    let snapshot = cx
                        .background_spawn(async move { filesystem_space_info(request_path) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        if app.finish_space_info_request(path, snapshot) {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
    }

    fn finish_space_info_request(
        &mut self,
        path: PathBuf,
        snapshot: Option<SpaceInfoSnapshot>,
    ) -> bool {
        self.space_info.finish_request(&path, snapshot)
    }

    fn maybe_start_device_monitor(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.device_monitor_active || now < self.next_device_monitor_start_at {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.device_monitor_rx = Some(receiver);
        self.device_monitor_active = true;
        self.next_device_monitor_start_at = now + DEVICE_MONITOR_RETRY_INTERVAL;

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(
                            async move { fika_core::watch_udisks2_devices(sender).await },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, _| {
                        app.device_monitor_active = false;
                        app.next_device_monitor_start_at =
                            Instant::now() + DEVICE_MONITOR_RETRY_INTERVAL;
                        let _ = result;
                    });
                }
            },
        )
        .detach();
    }

    fn drain_device_monitor_messages(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        loop {
            let Some(receiver) = self.device_monitor_rx.as_ref() else {
                break;
            };
            match receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.device_monitor_rx = None;
        }
        let mut changed = false;
        for message in messages {
            match message {
                DeviceMonitorMessage::Snapshot(devices) => {
                    changed |= self.apply_device_snapshot(&devices);
                }
                DeviceMonitorMessage::Events { devices, .. } => {
                    changed |= self.apply_device_snapshot(&devices);
                }
            }
        }
        changed
    }

    fn maybe_start_device_refresh(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.device_monitor_active
            || self.device_refresh_pending
            || now < self.next_device_refresh_at
        {
            return;
        }
        self.next_device_refresh_at = now + DEVICE_REFRESH_INTERVAL;
        self.request_device_snapshot_refresh(cx);
    }

    fn request_device_snapshot_refresh(&mut self, cx: &mut Context<Self>) {
        if self.device_refresh_pending {
            return;
        }
        self.device_refresh_pending = true;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let devices = cx.background_spawn(read_live_device_snapshot()).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        if app.finish_device_refresh(devices) {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
    }

    fn finish_device_refresh(&mut self, devices: Vec<DeviceInfo>) -> bool {
        self.device_refresh_pending = false;
        self.apply_device_snapshot(&devices)
    }

    fn apply_device_snapshot(&mut self, devices: &[DeviceInfo]) -> bool {
        self.replace_removable_device_places(devices)
    }

    fn place_snapshots(&mut self) -> Vec<PlaceSnapshot> {
        let current_dir = self
            .panes
            .focused()
            .and_then(|pane_id| self.panes.pane(pane_id))
            .map(|pane| pane.current_dir.as_path());
        place_snapshots_for(
            &self.places,
            current_dir,
            &self.hidden_place_sections,
            &self.hidden_places,
            self.place_drop_target.as_ref(),
            &mut self.file_icons,
        )
    }

    fn replace_removable_device_places(&mut self, devices: &[DeviceInfo]) -> bool {
        let existing_paths = self
            .places
            .iter()
            .filter(|place| place.group != REMOVABLE_DEVICES_GROUP)
            .map(|place| place.path.clone())
            .collect::<BTreeSet<_>>();
        let entries = removable_device_place_entries(devices, &existing_paths);
        let old_entries = self
            .places
            .iter()
            .filter(|place| place.group == REMOVABLE_DEVICES_GROUP)
            .cloned()
            .collect::<Vec<_>>();
        if old_entries == entries {
            return false;
        }

        self.places
            .retain(|place| place.group != REMOVABLE_DEVICES_GROUP);
        let insert_at = self
            .places
            .iter()
            .position(|place| place.group == DEVICES_GROUP)
            .unwrap_or(self.places.len());
        for entry in entries.into_iter().rev() {
            self.places.insert(insert_at, entry);
        }
        true
    }

    fn open_place(&mut self, path: PathBuf) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        if path == file_ops::trash_files_dir() {
            let _ = file_ops::ensure_trash_dirs();
        }
        self.load_pane(pane_id, path);
    }

    pub(crate) fn activate_place(
        &mut self,
        path: PathBuf,
        mounted: bool,
        device: bool,
        network: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        if network {
            self.set_pane_status(pane_id, "Network locations are not connected yet");
            return;
        }
        if mounted {
            self.open_place(path);
        } else if device {
            self.run_device_place_operation(pane_id, path, DevicePlaceOperation::Mount, cx);
        }
    }

    fn run_device_place_operation(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        operation: DevicePlaceOperation,
        cx: &mut Context<Self>,
    ) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        self.begin_pane_operation(pane_id, operation.in_progress_message(&path));
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = perform_device_place_operation(pane_id, path, operation).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_device_place_operation(result, cx);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_device_place_operation(
        &mut self,
        result: DevicePlaceOperationResult,
        cx: &mut Context<Self>,
    ) {
        match result.result {
            Ok(Some(mount_point)) => {
                self.finish_pane_operation(
                    result.pane_id,
                    format!("Mounted {}", result.path.display()),
                );
                self.request_device_snapshot_refresh(cx);
                self.load_pane(result.pane_id, mount_point);
            }
            Ok(None) => {
                self.finish_pane_operation(
                    result.pane_id,
                    result.operation.success_message(&result.path),
                );
                self.request_device_snapshot_refresh(cx);
            }
            Err(error) => self.finish_pane_operation(
                result.pane_id,
                result.operation.error_message(&result.path, &error),
            ),
        }
    }

    pub(crate) fn drop_place_drag_to_pane(&mut self, target_pane: PaneId, path: PathBuf) {
        if path == file_ops::trash_files_dir() {
            let _ = file_ops::ensure_trash_dirs();
        }
        self.panes.focus(target_pane);
        self.finish_rubber_band(target_pane);
        self.dismiss_context_menu();
        self.clear_item_drop_target();
        self.clear_place_drop_target();
        self.load_pane(target_pane, path);
    }

    pub(crate) fn drop_place_drag_to_current_place_target(
        &mut self,
        source_index: usize,
        fallback_index: usize,
    ) {
        let index = match self.place_drop_target.clone() {
            Some(PlaceDropTarget::Insert { index }) => index,
            _ => fallback_index,
        };
        self.drop_place_drag_to_place_insert(source_index, index);
    }

    pub(crate) fn drop_place_drag_to_place_insert(&mut self, source_index: usize, index: usize) {
        let Some(status_pane) = self.panes.focused() else {
            self.clear_place_drop_target();
            return;
        };
        self.dismiss_context_menu();
        self.finish_rubber_band(status_pane);
        self.clear_item_drop_target();
        self.clear_place_drop_target();
        self.move_user_place_to_insert_index(status_pane, source_index, index);
    }

    pub(crate) fn show_place_context_menu(
        &mut self,
        place: PlaceSnapshot,
        position: gpui::Point<gpui::Pixels>,
    ) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Place {
                path: place.path,
                mounted: place.mounted,
                device: place.device,
                device_ejectable: place.device_ejectable,
                device_can_power_off: place.device_can_power_off,
                trash_place: place.trash_place,
                trash_has_items: place.trash_has_items,
                editable: place.editable,
                removable: place.removable,
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_place_section_context_menu(
        &mut self,
        group: &'static str,
        position: gpui::Point<gpui::Pixels>,
    ) {
        if group.is_empty() || !self.places.iter().any(|place| place.group == group) {
            return;
        }
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlaceSection { group },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_places_blank_context_menu(&mut self, position: gpui::Point<gpui::Pixels>) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlacesBlank {
                has_hidden_places: !self.hidden_place_sections.is_empty()
                    || !self.hidden_places.is_empty(),
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    fn load_pane(&mut self, pane_id: PaneId, path: PathBuf) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let url_changed = self
            .panes
            .pane(pane_id)
            .is_some_and(|pane| pane.current_dir != path);
        let Some(event) = self.panes.load(pane_id, path.clone()) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id);
        if url_changed {
            self.clear_filter_query_for_url_change(pane_id);
        }
        let cached_events = self.schedule_listing(&event);
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
    }

    fn reload_pane(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.reload(pane_id) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id);
        let cached_events = self.schedule_listing(&event);
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        if let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        {
            self.set_pane_status(pane_id, format!("Reloading {}", path.display()));
        }
    }

    fn go_back(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.go_back(pane_id) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
    }

    fn go_forward(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.go_forward(pane_id) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
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
        self.split_pane_ratio(pane_id, new_id);
        self.start_watcher(new_id);
        self.set_pane_status(new_id, format!("Split pane {}", new_id.0));
    }

    fn open_path_in_new_pane(&mut self, source_pane_id: PaneId, path: PathBuf) {
        let Some(new_id) = self.panes.split(source_pane_id) else {
            return;
        };
        self.split_pane_ratio(source_pane_id, new_id);
        self.load_pane(new_id, path);
    }

    fn close_pane(&mut self, pane_id: PaneId) {
        let closing_snapshot = self.panes.pane(pane_id).map(|pane| {
            (
                pane.model.directory().to_path_buf(),
                pane.model.listing_snapshot(),
            )
        });
        if self.panes.close(pane_id) {
            if let Some((path, entries)) = closing_snapshot {
                self.listing_worker.cache_listing_snapshot(&path, entries);
            }
            self.listing_worker.cancel_pane(pane_id);
            self.clear_pane_lifecycle_state(pane_id);
            self.pane_filters.remove(&pane_id);
            self.normalize_current_pane_ratios();
            if let Some(focused_pane) = self.panes.focused() {
                self.set_pane_status(focused_pane, format!("Closed pane {}", pane_id.0));
            }
        }
    }

    fn clear_pane_content_state(&mut self, pane_id: PaneId) {
        self.visible_item_slots.remove(&pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.loading_panes.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        self.pane_viewport_geometries.remove(&pane_id);
        self.rubber_band_selection_panes.remove(&pane_id);
        self.pane_statuses.remove(&pane_id);
        self.location_edit_metrics.remove(&pane_id);
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload.source_pane == pane_id)
        {
            self.active_item_drag = None;
            self.place_drop_target = None;
        }
        if self
            .item_drop_target
            .as_ref()
            .is_some_and(|target| match target {
                ItemDropTarget::Pane {
                    pane_id: target_pane,
                    ..
                }
                | ItemDropTarget::Directory {
                    pane_id: target_pane,
                    ..
                } => *target_pane == pane_id,
            })
        {
            self.item_drop_target = None;
        }
        if self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            self.rubber_band = None;
        }
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.pane_id == pane_id)
        {
            self.dismiss_context_menu();
        }
        self.properties_dialog = None;
        self.clear_application_chooser_for_pane(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
    }

    fn begin_pane_loading_transition(&mut self, pane_id: PaneId) {
        self.status_summaries.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        self.location_edit_metrics.remove(&pane_id);
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload.source_pane == pane_id)
        {
            self.active_item_drag = None;
            self.place_drop_target = None;
        }
        if self
            .item_drop_target
            .as_ref()
            .is_some_and(|target| match target {
                ItemDropTarget::Pane {
                    pane_id: target_pane,
                    ..
                }
                | ItemDropTarget::Directory {
                    pane_id: target_pane,
                    ..
                } => *target_pane == pane_id,
            })
        {
            self.item_drop_target = None;
        }
        if self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            self.rubber_band = None;
        }
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.pane_id == pane_id)
        {
            self.dismiss_context_menu();
        }
        self.properties_dialog = None;
        self.clear_application_chooser_for_pane(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
    }

    fn clear_pane_lifecycle_state(&mut self, pane_id: PaneId) {
        self.clear_pane_content_state(pane_id);
        self.pane_split_ratios.remove(&pane_id);
    }

    fn select_only(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.select_only(pane_id, path) {
            self.rubber_band_selection_panes.remove(&pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.toggle_selection(pane_id, path).is_some() {
            self.rubber_band_selection_panes.remove(&pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.select_filtered_range_to(pane_id, &filtered, path)
        } else {
            self.panes.select_range_to(pane_id, path)
        };
        if let Some(selected) = selected {
            self.rubber_band_selection_panes.remove(&pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn select_all(&mut self, pane_id: PaneId) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.select_all_filtered(pane_id, &filtered)
        } else {
            self.panes.select_all(pane_id)
        };
        if let Some(selected) = selected {
            self.rubber_band_selection_panes.remove(&pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn clear_selection(&mut self, pane_id: PaneId) {
        self.rubber_band_selection_panes.remove(&pane_id);
        if self.panes.clear_selection(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, "Selection cleared");
        }
    }

    fn move_selection(&mut self, pane_id: PaneId, direction: SelectionMove, extend: bool) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.move_filtered_selection(pane_id, &filtered, direction, extend)
        } else {
            self.panes.move_selection(pane_id, direction, extend)
        };
        if let Some(selected) = selected {
            self.rubber_band_selection_panes.remove(&pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn rubber_band_selection_active(&self, pane_id: PaneId) -> bool {
        self.rubber_band_selection_panes.contains(&pane_id)
            && self
                .panes
                .selected_count(pane_id)
                .is_some_and(|selected| selected > 0)
    }

    fn select_all_filtered(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
    ) -> Option<usize> {
        let pane = self.panes.pane_mut(pane_id)?;
        let ids = filtered
            .iter_model_indexes()
            .filter_map(|index| pane.model.get(index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection.replace(ids);
        Some(count)
    }

    fn select_filtered_range_to(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
        path: PathBuf,
    ) -> Option<usize> {
        let pane = self.panes.pane_mut(pane_id)?;
        let target_model_index = pane.model.index_of_path(&path)?;
        let target_layout_index = filtered.layout_index_for_model_index(target_model_index)?;
        let target_id = pane.model.get(target_model_index)?.id;
        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| {
                pane.model
                    .index_of_id(*id)
                    .and_then(|index| filtered.layout_index_for_model_index(index))
                    .is_some()
            })
            .unwrap_or(target_id);
        let anchor_layout_index = pane
            .model
            .index_of_id(anchor_id)
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .unwrap_or(target_layout_index);
        let (start, end) = if anchor_layout_index <= target_layout_index {
            (anchor_layout_index, target_layout_index)
        } else {
            (target_layout_index, anchor_layout_index)
        };
        let ids = filtered.as_slice()[start..=end]
            .iter()
            .filter_map(|index| pane.model.get(*index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    fn move_filtered_selection(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
        direction: SelectionMove,
        extend: bool,
    ) -> Option<usize> {
        if filtered.is_empty() {
            return None;
        }
        let pane = self.panes.pane_mut(pane_id)?;
        let current_layout_index = pane
            .selection
            .active_id()
            .and_then(|active| pane.model.index_of_id(active))
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .or_else(|| {
                pane.selection
                    .selected_ids()
                    .last()
                    .and_then(|id| pane.model.index_of_id(*id))
                    .and_then(|index| filtered.layout_index_for_model_index(index))
            });
        let target_layout_index = match (current_layout_index, direction) {
            (Some(index), SelectionMove::Previous) => index.saturating_sub(1),
            (Some(index), SelectionMove::Next) => (index + 1).min(filtered.len() - 1),
            (None, SelectionMove::Previous) => filtered.len() - 1,
            (None, SelectionMove::Next) => 0,
        };
        let target_model_index = filtered.model_index(target_layout_index)?;
        let target_id = pane.model.get(target_model_index)?.id;

        if !extend {
            pane.selection.select_only(target_id);
            return Some(1);
        }

        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| {
                pane.model
                    .index_of_id(*id)
                    .and_then(|index| filtered.layout_index_for_model_index(index))
                    .is_some()
            })
            .unwrap_or(target_id);
        let anchor_layout_index = pane
            .model
            .index_of_id(anchor_id)
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .unwrap_or(target_layout_index);
        let (start, end) = if anchor_layout_index <= target_layout_index {
            (anchor_layout_index, target_layout_index)
        } else {
            (target_layout_index, anchor_layout_index)
        };
        let ids = filtered.as_slice()[start..=end]
            .iter()
            .filter_map(|index| pane.model.get(*index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level) else {
            return;
        };
        let Some(view) = self.panes.apply_zoom_change(pane_id, change) else {
            return;
        };
        if view.zoom_level == previous_level {
            self.set_pane_status(
                pane_id,
                format!(
                    "Zoom level {} ({} px)",
                    view.zoom_level,
                    view.icon_size() as i32
                ),
            );
            return;
        }
        self.compact_column_widths.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    pub(crate) fn zoom_pane_from_wheel(&mut self, pane_id: PaneId, delta: ScrollDelta) {
        if let Some(change) = zoom_change_for_wheel_delta(delta) {
            self.finish_rubber_band(pane_id);
            self.apply_zoom_change(pane_id, change);
        }
    }

    pub(crate) fn set_zoom_level(&mut self, pane_id: PaneId, level: i32) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level) else {
            return;
        };
        let Some(view) = self.panes.set_zoom_level(pane_id, level) else {
            return;
        };
        if view.zoom_level != previous_level {
            self.compact_column_widths.remove(&pane_id);
            self.smooth_scrolls.remove(&pane_id);
            self.scroll_drag_trackers.remove(&pane_id);
            self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        }
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    fn set_pane_sort_role(&mut self, pane_id: PaneId, role: SortRole) {
        let Some((sort, signals)) = self.panes.set_sort_role(pane_id, role) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_order(&mut self, pane_id: PaneId, order: SortOrder) {
        let Some((sort, signals)) = self.panes.set_sort_order(pane_id, order) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_folders_first(&mut self, pane_id: PaneId, folders_first: bool) {
        let Some((sort, signals)) = self.panes.set_sort_folders_first(pane_id, folders_first)
        else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_hidden_last(&mut self, pane_id: PaneId, hidden_last: bool) {
        let Some((sort, signals)) = self.panes.set_sort_hidden_last(pane_id, hidden_last) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn finish_pane_sort(
        &mut self,
        pane_id: PaneId,
        sort: SortDescriptor,
        signals: &[fika_core::DirectoryModelSignal],
    ) {
        if !signals.is_empty() {
            self.invalidate_pane_layout_projection(pane_id, true);
        }
        self.set_pane_status(
            pane_id,
            format!(
                "Sorted by {} ({})",
                sort_role_label(sort.role),
                sort_order_label(sort.order)
            ),
        );
    }

    pub(crate) fn scroll_pane_smooth(
        &mut self,
        pane_id: PaneId,
        delta_x: f32,
        delta_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        if delta_x.abs() <= f32::EPSILON && delta_y.abs() <= f32::EPSILON {
            return;
        }
        let Some(current) = self.panes.pane(pane_id).map(|pane| ViewPoint {
            x: pane.view.scroll_x,
            y: pane.view.scroll_y,
        }) else {
            return;
        };
        let bounds = ScrollBounds::new(max_scroll_x, max_scroll_y);
        let target = bounds.clamp(ViewPoint {
            x: current.x + delta_x,
            y: current.y + delta_y,
        });
        if target == current {
            return;
        }

        self.smooth_scrolls.remove(&pane_id);
        if let Some(view) =
            self.panes
                .set_view_scroll(pane_id, target.x, target.y, bounds.max_x, bounds.max_y)
            && ((view.scroll_x - current.x).abs() > f32::EPSILON
                || (view.scroll_y - current.y).abs() > f32::EPSILON)
        {
            cx.notify();
        }
        self.scroll_drag_trackers.remove(&pane_id);
    }

    pub(crate) fn set_pane_scroll_immediate(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) {
        self.smooth_scrolls.remove(&pane_id);
        if let Some(view) =
            self.panes
                .set_view_scroll(pane_id, scroll_x, scroll_y, max_scroll_x, max_scroll_y)
        {
            self.scroll_drag_trackers
                .entry(pane_id)
                .or_default()
                .sample(
                    ViewPoint {
                        x: view.scroll_x,
                        y: view.scroll_y,
                    },
                    Instant::now(),
                );
        }
    }

    pub(crate) fn finish_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        max_scroll_x: f32,
        max_scroll_y: f32,
        _cx: &mut Context<Self>,
    ) {
        let _ = (max_scroll_x, max_scroll_y);
        self.scroll_drag_trackers.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
    }

    pub(crate) fn finish_scrollbar_drag_for_content_width(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        cx: &mut Context<Self>,
    ) {
        let visible_width = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.viewport_width)
            .unwrap_or_default();
        self.finish_scrollbar_drag(pane_id, (content_width - visible_width).max(0.0), 0.0, cx);
    }

    pub(crate) fn set_pane_viewport_geometry(
        &mut self,
        pane_id: PaneId,
        window_rect: ViewRect,
    ) -> bool {
        let window_rect = ViewRect {
            x: window_rect.x,
            y: window_rect.y,
            width: fika_core::normalize_viewport_extent(window_rect.width).max(0.0),
            height: fika_core::normalize_viewport_extent(window_rect.height).max(0.0),
        };
        let geometry = PaneViewportGeometry { window_rect };
        if self.pane_viewport_geometries.get(&pane_id) == Some(&geometry) {
            return false;
        }
        self.pane_viewport_geometries.insert(pane_id, geometry);
        true
    }

    fn pane_split_ratio(&self, pane_id: PaneId) -> f32 {
        let pane_ids = self.panes.pane_ids();
        let ratios = self.normalized_pane_ratios_for_ids(pane_ids);
        pane_ids
            .iter()
            .position(|id| *id == pane_id)
            .and_then(|index| ratios.get(index).copied())
            .unwrap_or(1.0)
    }

    fn projected_pane_width(&self, pane_id: PaneId) -> Option<f32> {
        if self.pane_row_width <= 0.0 {
            return None;
        }
        let pane_ids = self.panes.pane_ids();
        let index = pane_ids.iter().position(|id| *id == pane_id)?;
        let ratios = self.normalized_pane_ratios_for_ids(pane_ids);
        let available = pane_width_available(self.pane_row_width, pane_ids.len());
        (available > 0.0).then(|| ratios[index] * available)
    }

    fn normalized_pane_ratios_for_ids(&self, pane_ids: &[PaneId]) -> Vec<f32> {
        if pane_ids.is_empty() {
            return Vec::new();
        }
        let equal = 1.0 / pane_ids.len() as f32;
        normalize_pane_ratios(
            pane_ids
                .iter()
                .map(|pane_id| {
                    self.pane_split_ratios
                        .get(pane_id)
                        .copied()
                        .unwrap_or(equal)
                })
                .collect(),
        )
    }

    fn store_pane_ratios(&mut self, pane_ids: &[PaneId], ratios: Vec<f32>) {
        let normalized = normalize_pane_ratios(ratios);
        self.pane_split_ratios
            .retain(|pane_id, _| pane_ids.contains(pane_id));
        for (pane_id, ratio) in pane_ids.iter().copied().zip(normalized) {
            self.pane_split_ratios.insert(pane_id, ratio);
        }
    }

    fn normalize_current_pane_ratios(&mut self) {
        let pane_ids = self.panes.pane_ids().to_vec();
        let ratios = self.normalized_pane_ratios_for_ids(&pane_ids);
        self.store_pane_ratios(&pane_ids, ratios);
    }

    fn split_pane_ratio(&mut self, source: PaneId, new_id: PaneId) {
        let pane_ids = self.panes.pane_ids().to_vec();
        let old_ids = pane_ids
            .iter()
            .copied()
            .filter(|pane_id| *pane_id != new_id)
            .collect::<Vec<_>>();
        let old_ratios = self.normalized_pane_ratios_for_ids(&old_ids);
        let old_ratio_for = |pane_id: PaneId| {
            old_ids
                .iter()
                .position(|old_id| *old_id == pane_id)
                .and_then(|index| old_ratios.get(index).copied())
                .unwrap_or(1.0)
        };
        let source_ratio = old_ratio_for(source);
        let ratios = pane_ids
            .iter()
            .map(|pane_id| {
                if *pane_id == source || *pane_id == new_id {
                    source_ratio / 2.0
                } else {
                    old_ratio_for(*pane_id)
                }
            })
            .collect::<Vec<_>>();
        self.store_pane_ratios(&pane_ids, ratios);
    }

    fn set_pane_row_width(&mut self, width: f32) -> bool {
        let width = width.max(0.0).round();
        if width_value_eq(self.pane_row_width, width) {
            return false;
        }
        self.pane_row_width = width;
        true
    }

    pub(crate) fn reset_pane_pair_ratio(&mut self, left: PaneId, right: PaneId) -> bool {
        let pane_ids = self.panes.pane_ids().to_vec();
        let Some(left_index) = pane_ids.windows(2).position(|pair| pair == [left, right]) else {
            return false;
        };
        let mut ratios = self.normalized_pane_ratios_for_ids(&pane_ids);
        let pair_ratio = ratios[left_index] + ratios[left_index + 1];
        let next_ratio = pair_ratio / 2.0;
        if split_ratio_eq(ratios[left_index], next_ratio)
            && split_ratio_eq(ratios[left_index + 1], next_ratio)
        {
            return false;
        }
        ratios[left_index] = next_ratio;
        ratios[left_index + 1] = next_ratio;
        self.store_pane_ratios(&pane_ids, ratios);
        true
    }

    fn resize_pane_pair_from_row_drag(
        &mut self,
        left: PaneId,
        right: PaneId,
        divider_x: f32,
        row_x: f32,
        row_width: f32,
    ) -> bool {
        let pane_ids = self.panes.pane_ids().to_vec();
        let Some(left_index) = pane_ids.windows(2).position(|pair| pair == [left, right]) else {
            return false;
        };
        let row_width = row_width.max(0.0).floor();
        self.set_pane_row_width(row_width);
        let available = pane_width_available(row_width, pane_ids.len());
        if available <= 0.0 {
            return false;
        }
        let mut ratios = self.normalized_pane_ratios_for_ids(&pane_ids);

        let pair_start = row_x
            + ratios[..left_index].iter().sum::<f32>() * available
            + left_index as f32 * PANE_SPLITTER_WIDTH;
        let pair_available = ((ratios[left_index] + ratios[left_index + 1]) * available).max(1.0);
        let min_width = MIN_PANE_WIDTH.min(pair_available / 2.0);
        let left_width = (divider_x - pair_start).clamp(min_width, pair_available - min_width);
        let right_width = pair_available - left_width;
        let left_ratio = left_width / available;
        let right_ratio = right_width / available;
        if split_ratio_eq(ratios[left_index], left_ratio)
            && split_ratio_eq(ratios[left_index + 1], right_ratio)
        {
            return false;
        }
        ratios[left_index] = left_ratio;
        ratios[left_index + 1] = right_ratio;
        self.store_pane_ratios(&pane_ids, ratios);
        true
    }

    fn set_pane_viewport_bounds(
        &mut self,
        pane_id: PaneId,
        viewport_width: f32,
        viewport_height: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> bool {
        let new_bounds = ScrollBounds::new(max_scroll_x, max_scroll_y);
        let changed = self
            .panes
            .set_viewport_bounds(
                pane_id,
                viewport_width,
                viewport_height,
                max_scroll_x,
                max_scroll_y,
            )
            .unwrap_or(false);
        if self
            .smooth_scrolls
            .get(&pane_id)
            .is_some_and(|scroll| !scroll.maximum_matches(new_bounds))
        {
            self.smooth_scrolls.remove(&pane_id);
            self.scroll_drag_trackers.remove(&pane_id);
            self.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        }
        changed
    }

    fn content_point_from_window(
        &self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ViewPoint> {
        let geometry = *self.pane_viewport_geometries.get(&pane_id)?;
        let view = &self.panes.pane(pane_id)?.view;
        let window_point = ViewPoint {
            x: position.x.as_f32(),
            y: position.y.as_f32(),
        };
        if !geometry.window_rect.contains(window_point) {
            return None;
        }
        let local_x = window_point.x - geometry.window_rect.x;
        let local_y = window_point.y - geometry.window_rect.y;
        Some(ViewPoint {
            x: local_x + view.scroll_x,
            y: local_y + view.scroll_y,
        })
    }

    fn clamped_content_point_from_window(
        &self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ViewPoint> {
        let geometry = *self.pane_viewport_geometries.get(&pane_id)?;
        let view = &self.panes.pane(pane_id)?.view;
        let local_x =
            (position.x.as_f32() - geometry.window_rect.x).clamp(0.0, geometry.window_rect.width);
        let local_y =
            (position.y.as_f32() - geometry.window_rect.y).clamp(0.0, geometry.window_rect.height);
        Some(ViewPoint {
            x: local_x + view.scroll_x,
            y: local_y + view.scroll_y,
        })
    }

    fn layout_projection_for_pane(&mut self, pane_id: PaneId) -> Option<PaneLayoutProjection> {
        let filtered_model = self.filtered_model_for_pane(pane_id);
        let pane = self.panes.pane(pane_id)?;
        let layout = match filtered_model.as_ref() {
            Some((filtered, source_revision)) => compact_layout_for_filtered_model(
                self.compact_column_widths.entry(pane_id).or_default(),
                &pane.model,
                filtered,
                *source_revision,
                &pane.view,
            ),
            None => compact_layout_for_model(
                self.compact_column_widths.entry(pane_id).or_default(),
                &pane.model,
                &pane.view,
            ),
        };
        Some(PaneLayoutProjection::new(
            layout,
            filtered_model.map(|(filtered, _)| filtered),
        ))
    }

    fn item_at_content_point(
        &mut self,
        pane_id: PaneId,
        point: ViewPoint,
    ) -> Option<ContentItemHit> {
        let projection = self.layout_projection_for_pane(pane_id)?;
        let layout_index = projection.layout.hit_test_content_point(point)?;
        let model_index = projection.model_index_for_layout_index(layout_index)?;
        let pane = self.panes.pane(pane_id)?;
        let entry = pane.model.get(model_index)?;
        let item_layout = projection.layout.item_with_required_text_width(
            layout_index,
            Some(compact_text_width(entry.name_width_units)),
        )?;
        if !item_layout.visual_rect.contains(point) {
            return None;
        }
        Some(ContentItemHit {
            model_index,
            path: pane.model.path_for_index(model_index)?,
            is_dir: entry.is_dir,
        })
    }

    fn indexes_intersecting_visual_rect(&mut self, pane_id: PaneId, rect: ViewRect) -> Vec<usize> {
        let Some(projection) = self.layout_projection_for_pane(pane_id) else {
            return Vec::new();
        };
        let candidate_indexes = projection
            .layout
            .indexes_intersecting(rect)
            .indexes()
            .to_vec();
        let Some(pane) = self.panes.pane(pane_id) else {
            return Vec::new();
        };
        candidate_indexes
            .into_iter()
            .filter_map(|layout_index| {
                let model_index = projection.model_index_for_layout_index(layout_index)?;
                let Some(entry) = pane.model.get(model_index) else {
                    return None;
                };
                projection
                    .layout
                    .item_with_required_text_width(
                        layout_index,
                        Some(compact_text_width(entry.name_width_units)),
                    )
                    .is_some_and(|item| item.visual_rect.intersects(rect))
                    .then_some(model_index)
            })
            .collect()
    }

    fn handle_blank_click(&mut self, pane_id: PaneId, position: gpui::Point<gpui::Pixels>) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        if self.item_at_content_point(pane_id, point).is_some() {
            return false;
        }
        self.clear_selection_from_blank(pane_id);
        true
    }

    fn clear_selection_from_blank(&mut self, pane_id: PaneId) {
        self.clear_selection(pane_id);
    }

    fn start_rubber_band_from_blank(&mut self, pane_id: PaneId, start: ViewPoint) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        if self.item_at_content_point(pane_id, start).is_some() {
            return false;
        }
        self.clear_selection_from_blank(pane_id);
        self.start_rubber_band(pane_id, start);
        true
    }

    pub(crate) fn start_rubber_band_from_window_if_blank(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(start) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        self.start_rubber_band_from_blank(pane_id, start)
    }

    pub(crate) fn update_rubber_band_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        if !self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            return false;
        }
        let Some(current) = self.clamped_content_point_from_window(pane_id, position) else {
            return false;
        };
        self.update_rubber_band(pane_id, current);
        true
    }

    pub(crate) fn window_position_is_blank_in_pane(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        self.item_at_content_point(pane_id, point).is_none()
    }

    fn start_rubber_band(&mut self, pane_id: PaneId, start: ViewPoint) {
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.rubber_band = Some(RubberBandState {
            pane_id,
            start,
            current: start,
        });
    }

    fn update_rubber_band(&mut self, pane_id: PaneId, current: ViewPoint) {
        let Some(mut band) = self.rubber_band else {
            return;
        };
        if band.pane_id != pane_id {
            return;
        }
        band.current = current;
        self.rubber_band = Some(band);
        let selection = self.indexes_intersecting_visual_rect(pane_id, band.rect());
        if let Some(selected) = self
            .panes
            .replace_selection_by_indexes(pane_id, selection.iter().copied())
        {
            if selected > 0 {
                self.rubber_band_selection_panes.insert(pane_id);
            } else {
                self.rubber_band_selection_panes.remove(&pane_id);
            }
            self.set_pane_status(pane_id, format!("{selected} selected"));
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

    fn clear_location_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .location_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.location_draft = None;
        }
    }

    fn clear_place_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .place_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.place_draft = None;
        }
    }

    fn clear_application_chooser_for_pane(&mut self, pane_id: PaneId) {
        if self
            .application_chooser
            .as_ref()
            .is_some_and(|chooser| chooser.pane_id == pane_id)
        {
            self.application_chooser = None;
        }
    }

    fn start_add_place(&mut self, pane_id: PaneId) {
        let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: None,
            focus: PlaceDraftField::Label,
            label: default_place_label(&path),
            path: path.display().to_string(),
        });
        self.set_pane_status(pane_id, format!("Adding place {}", path.display()));
    }

    fn start_edit_place(&mut self, pane_id: PaneId, path: PathBuf) {
        let Some(place) = self
            .places
            .iter()
            .find(|place| place.path == path && place.editable)
            .cloned()
        else {
            self.set_pane_status(pane_id, "Place cannot be edited");
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: Some(place.path.clone()),
            focus: PlaceDraftField::Label,
            label: place.label,
            path: place.path.display().to_string(),
        });
        self.set_pane_status(pane_id, "Editing place");
    }

    fn remove_place(&mut self, pane_id: PaneId, path: &Path) {
        let Some(index) = self
            .places
            .iter()
            .position(|place| place.path == path && place.removable)
        else {
            self.set_pane_status(pane_id, "Place cannot be removed");
            return;
        };
        let removed = self.places.remove(index);
        if self
            .place_draft
            .as_ref()
            .and_then(|draft| draft.editing_path.as_deref())
            == Some(removed.path.as_path())
        {
            self.place_draft = None;
        }
        self.hidden_places.remove(&removed.path);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, format!("Removed place {}", removed.label));
    }

    fn handle_place_draft_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(draft_pane_id) = self.place_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match place_input_action(keystroke) {
            PlaceInputAction::Cancel => {
                self.place_draft = None;
                self.set_pane_status(draft_pane_id, "Place edit cancelled");
            }
            PlaceInputAction::Commit => self.commit_place_draft(),
            PlaceInputAction::NextField => {
                if let Some(draft) = &mut self.place_draft {
                    draft.focus = match draft.focus {
                        PlaceDraftField::Label => PlaceDraftField::Path,
                        PlaceDraftField::Path => PlaceDraftField::Label,
                    };
                }
            }
            PlaceInputAction::Backspace => {
                if let Some(draft) = &mut self.place_draft {
                    match draft.focus {
                        PlaceDraftField::Label => {
                            draft.label.pop();
                        }
                        PlaceDraftField::Path => {
                            draft.path.pop();
                        }
                    }
                }
            }
            PlaceInputAction::Insert(text) => {
                if let Some(draft) = &mut self.place_draft {
                    match draft.focus {
                        PlaceDraftField::Label => draft.label.push_str(&text),
                        PlaceDraftField::Path => draft.path.push_str(&text),
                    }
                }
            }
            PlaceInputAction::Ignore => return false,
        }
        true
    }

    pub(crate) fn commit_place_draft(&mut self) {
        let Some(draft) = self.place_draft.take() else {
            return;
        };
        let label = draft.label.trim().to_string();
        if label.is_empty() {
            self.set_pane_status(draft.pane_id, "Place label cannot be empty");
            return;
        }
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(path) = resolve_location_input(&current_dir, &draft.path) else {
            self.set_pane_status(draft.pane_id, "Place path cannot be empty");
            return;
        };
        if !path.is_dir() {
            self.set_pane_status(
                draft.pane_id,
                format!("Place path is not a folder: {}", path.display()),
            );
            return;
        }
        let duplicate = self.places.iter().position(|place| place.path == path);
        if let Some(editing_path) = draft.editing_path {
            let Some(index) = self
                .places
                .iter()
                .position(|place| place.path == editing_path && place.editable)
            else {
                self.set_pane_status(draft.pane_id, "Place cannot be edited");
                return;
            };
            if duplicate.is_some_and(|duplicate| duplicate != index) {
                self.set_pane_status(draft.pane_id, "Place already exists");
                return;
            }
            self.places[index].label = label.clone();
            self.places[index].path = path.clone();
            if let Err(error) = self.save_user_places() {
                self.set_pane_status(draft.pane_id, error);
                return;
            }
            self.set_pane_status(draft.pane_id, format!("Updated place {label}"));
            return;
        }

        if duplicate.is_some() {
            self.set_pane_status(draft.pane_id, "Place already exists");
            return;
        }
        self.insert_user_place(label.clone(), path);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(draft.pane_id, error);
            return;
        }
        self.set_pane_status(draft.pane_id, format!("Added place {label}"));
    }

    fn insert_user_place(&mut self, label: String, path: PathBuf) {
        let insert_at = self.user_place_insert_index(self.places.len());
        self.insert_user_place_at(label, path, insert_at);
    }

    fn insert_user_place_at(&mut self, label: String, path: PathBuf, index: usize) {
        let entry = PlaceEntry {
            group: "",
            marker: "B",
            label,
            path,
            editable: true,
            removable: true,
            device_ejectable: false,
            device_can_power_off: false,
        };
        let insert_at = self.user_place_insert_index(index);
        self.places.insert(insert_at, entry);
    }

    fn move_user_place_to_insert_index(
        &mut self,
        pane_id: PaneId,
        source_index: usize,
        index: usize,
    ) {
        let Some(source) = self.places.get(source_index) else {
            self.set_pane_status(pane_id, "Place cannot be moved");
            return;
        };
        if !(source.editable && source.removable) {
            self.set_pane_status(pane_id, "Place cannot be moved");
            return;
        }

        let target_index = self.user_place_insert_index(index);
        if target_index == source_index || target_index == source_index + 1 {
            self.set_pane_status(pane_id, "Place already there");
            return;
        }

        let label = source.label.clone();
        let place = self.places.remove(source_index);
        let insert_at = if source_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        };
        let insert_at = self.user_place_insert_index(insert_at);
        self.places.insert(insert_at, place);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, format!("Moved place {label}"));
    }

    fn user_place_insert_index(&self, index: usize) -> usize {
        let first_grouped = self
            .places
            .iter()
            .position(|place| !place.group.is_empty())
            .unwrap_or(self.places.len());
        let first_user = self
            .places
            .iter()
            .position(|place| place.editable && place.removable)
            .unwrap_or(first_grouped);
        index.clamp(first_user, first_grouped)
    }

    fn insert_place_from_dropped_paths(
        &mut self,
        pane_id: PaneId,
        paths: Vec<PathBuf>,
        index: usize,
    ) {
        let [path] = paths.as_slice() else {
            self.set_pane_status(pane_id, "Drop one folder to add a place");
            return;
        };
        if !path.is_dir() {
            self.set_pane_status(pane_id, "Only folders can be added to Places");
            return;
        }
        if self.places.iter().any(|place| place.path == *path) {
            self.set_pane_status(pane_id, "Place already exists");
            return;
        }
        let label = default_place_label(path);
        self.insert_user_place_at(label.clone(), path.clone(), index);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, format!("Added place {label}"));
    }

    fn hide_place(&mut self, pane_id: PaneId, path: PathBuf) {
        let Some(place) = self.places.iter().find(|place| place.path == path) else {
            self.set_pane_status(pane_id, "Place cannot be hidden");
            return;
        };
        let label = place.label.clone();
        self.hidden_places.insert(path);
        self.set_pane_status(pane_id, format!("Hidden place {label}"));
    }

    fn hide_place_section(&mut self, pane_id: PaneId, group: &'static str) {
        if group.is_empty() || !self.places.iter().any(|place| place.group == group) {
            self.set_pane_status(pane_id, "Place section cannot be hidden");
            return;
        }
        self.hidden_place_sections.insert(group);
        self.set_pane_status(pane_id, format!("Hidden places section {group}"));
    }

    fn show_hidden_places(&mut self, pane_id: PaneId) {
        if self.hidden_places.is_empty() && self.hidden_place_sections.is_empty() {
            self.set_pane_status(pane_id, "No hidden places");
            return;
        }
        self.hidden_places.clear();
        self.hidden_place_sections.clear();
        self.set_pane_status(pane_id, "Showing hidden places");
    }

    fn user_places(&self) -> Vec<UserPlace> {
        self.places
            .iter()
            .filter(|place| place.editable && place.removable)
            .map(|place| UserPlace::new(place.label.clone(), place.path.clone()))
            .collect()
    }

    fn save_user_places(&self) -> Result<(), String> {
        fika_core::save_user_places(&self.user_places_path, &self.user_places())
    }

    pub(crate) fn update_location_edit_metrics(
        &mut self,
        pane_id: PaneId,
        value: String,
        origin_x: f32,
        scroll_x: f32,
        visible_width: f32,
        byte_positions: Vec<(usize, f32)>,
    ) {
        let Some(draft) = self
            .location_draft
            .as_mut()
            .filter(|draft| draft.pane_id == pane_id && draft.value == value)
        else {
            self.location_edit_metrics.remove(&pane_id);
            return;
        };
        draft.scroll_x = scroll_x.max(0.0);
        self.location_edit_metrics.insert(
            pane_id,
            LocationEditMetrics {
                value,
                origin_x,
                scroll_x,
                visible_width,
                byte_positions,
            },
        );
    }

    pub(crate) fn set_location_caret_from_window_x(&mut self, pane_id: PaneId, window_x: f32) {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        let Some(draft) = self
            .location_draft
            .as_mut()
            .filter(|draft| draft.pane_id == pane_id)
        else {
            return;
        };
        let Some(metrics) = self
            .location_edit_metrics
            .get(&pane_id)
            .filter(|metrics| metrics.value == draft.value)
        else {
            draft.move_to_end();
            return;
        };
        let local_x =
            (window_x - metrics.origin_x).clamp(0.0, metrics.visible_width) + metrics.scroll_x;
        let caret = metrics
            .byte_positions
            .iter()
            .min_by(|left, right| {
                (left.1 - local_x)
                    .abs()
                    .total_cmp(&(right.1 - local_x).abs())
            })
            .map(|(index, _)| *index)
            .unwrap_or(draft.value.len());
        draft.set_caret(caret);
    }

    pub(crate) fn start_location_edit(&mut self, pane_id: PaneId) {
        let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.location_draft = Some(LocationDraft::new(pane_id, path.display().to_string()));
        self.set_pane_status(pane_id, format!("Location {}", path.display()));
    }

    pub(crate) fn open_location_segment(&mut self, pane_id: PaneId, path: PathBuf) {
        self.panes.focus(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        if self
            .panes
            .pane(pane_id)
            .is_some_and(|pane| pane.current_dir == path)
        {
            return;
        }
        self.load_pane(pane_id, path);
    }

    fn handle_location_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(draft_pane_id) = self.location_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match location_input_action(keystroke) {
            LocationInputAction::Cancel => {
                self.location_draft = None;
                self.set_pane_status(draft_pane_id, "Location edit cancelled");
            }
            LocationInputAction::Commit => self.commit_location_draft(),
            LocationInputAction::Complete => self.complete_location_draft(),
            LocationInputAction::MoveStart => {
                if let Some(draft) = &mut self.location_draft {
                    draft.move_to_start();
                }
            }
            LocationInputAction::MoveEnd => {
                if let Some(draft) = &mut self.location_draft {
                    draft.move_to_end();
                }
            }
            LocationInputAction::MoveBackward => {
                if let Some(draft) = &mut self.location_draft {
                    draft.move_backward();
                }
            }
            LocationInputAction::MoveForward => {
                if let Some(draft) = &mut self.location_draft {
                    draft.move_forward();
                }
            }
            LocationInputAction::Backspace => {
                if let Some(draft) = &mut self.location_draft {
                    draft.delete_backward();
                }
            }
            LocationInputAction::Delete => {
                if let Some(draft) = &mut self.location_draft {
                    draft.delete_forward();
                }
            }
            LocationInputAction::Insert(text) => {
                if let Some(draft) = &mut self.location_draft {
                    draft.insert(&text);
                }
            }
            LocationInputAction::Ignore => return true,
        }
        true
    }

    fn commit_location_draft(&mut self) {
        let Some(draft) = self.location_draft.take() else {
            return;
        };
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(path) = resolve_location_input(&current_dir, &draft.value) else {
            self.set_pane_status(draft.pane_id, "Location is empty");
            return;
        };
        if !path.is_dir() {
            self.set_pane_status(
                draft.pane_id,
                format!("Location is not a folder: {}", path.display()),
            );
            return;
        }
        if path == current_dir {
            self.set_pane_status(draft.pane_id, format!("Location {}", path.display()));
            return;
        }
        self.load_pane(draft.pane_id, path);
    }

    fn complete_location_draft(&mut self) {
        let Some(draft) = self.location_draft.clone() else {
            return;
        };
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(completed) = complete_location_input(&current_dir, &draft.value) else {
            self.set_pane_status(draft.pane_id, "No location completion");
            return;
        };
        if let Some(active) = &mut self.location_draft {
            active.value = completed;
            active.move_to_end();
        }
    }

    fn start_rename_in_pane(&mut self, pane_id: PaneId) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        let [original_path] = selected_paths.as_slice() else {
            self.set_pane_status(pane_id, "Select one item to rename");
            return;
        };
        let Some(name) = original_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
        else {
            self.set_pane_status(pane_id, "Selected item cannot be renamed");
            return;
        };

        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.rename_draft = Some(RenameDraft {
            pane_id,
            original_path: original_path.clone(),
            draft_name: name.to_string(),
        });
        self.set_pane_status(pane_id, format!("Renaming {name}"));
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
                self.set_pane_status(draft_pane_id, "Rename cancelled");
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
        let Some(draft_pane_id) = self.rename_draft.as_ref().map(|draft| draft.pane_id) else {
            return;
        };
        if self.operation_pending {
            self.set_pane_status(draft_pane_id, "File operation already running");
            return;
        }
        let Some(draft) = self.rename_draft.take() else {
            return;
        };
        let new_name = draft.draft_name.trim().to_string();
        if new_name.is_empty() {
            self.set_pane_status(draft.pane_id, "Name cannot be empty");
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
            self.set_pane_status(draft.pane_id, "Rename unchanged");
            return;
        }

        self.begin_pane_operation(
            draft.pane_id,
            format!("Renaming {}", draft.original_path.display()),
        );
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
                self.finish_pane_operation(
                    result.pane_id,
                    format!("Renamed to {}", renamed_path.display()),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    result.pane_id,
                    format!("Cannot rename {}: {err}", result.original_path.display()),
                );
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
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let Some(parent_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        self.create_item_in_directory(pane_id, parent_dir, kind, cx);
    }

    fn create_item_in_directory(
        &mut self,
        pane_id: PaneId,
        parent_dir: PathBuf,
        kind: CreatedItemKind,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        self.begin_pane_operation(
            pane_id,
            format!("Creating {}", created_item_label(kind).to_ascii_lowercase()),
        );
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
                self.finish_pane_operation(result.pane_id, format!("Created {}", path.display()));
            }
            Err(err) => {
                self.finish_pane_operation(
                    result.pane_id,
                    format!(
                        "Cannot create {}: {err}",
                        created_item_label(result.kind).to_ascii_lowercase()
                    ),
                );
            }
        }
    }

    fn store_selection_for_transfer(
        &mut self,
        pane_id: PaneId,
        mode: ClipboardMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        let paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if paths.is_empty() {
            self.set_pane_status(
                pane_id,
                format!("No selection to {}", mode.label().to_ascii_lowercase()),
            );
            return;
        }

        let count = paths.len();
        let clipboard = ClipboardState::files(mode, paths);
        let item = clipboard.to_clipboard_item();
        cx.write_to_clipboard(item.clone());
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        cx.write_to_primary(item);
        self.clipboard = Some(clipboard);
        self.set_pane_status(pane_id, format!("{} {} item(s)", mode.label(), count));
    }

    fn import_system_clipboard(&mut self, cx: &mut Context<Self>) {
        let system_clipboard = cx.read_from_clipboard();
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let primary_selection = cx.read_from_primary();
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let primary_selection: Option<ClipboardItem> = None;

        if let Some(clipboard) =
            standard_paste_clipboard_state(system_clipboard.as_ref(), primary_selection.as_ref())
        {
            self.clipboard = Some(clipboard);
        }
    }

    fn import_primary_selection(&mut self, cx: &mut Context<Self>) -> Option<ClipboardState> {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let primary_selection = cx.read_from_primary();
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let primary_selection: Option<ClipboardItem> = None;

        let clipboard = primary_paste_clipboard_state(primary_selection.as_ref())?;
        self.clipboard = Some(clipboard.clone());
        Some(clipboard)
    }

    fn paste_into_pane(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        let Some(target_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.paste_into_directory(pane_id, target_dir, cx);
    }

    pub(crate) fn paste_primary_into_pane_if_blank(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.window_position_is_blank_in_pane(pane_id, position) {
            return false;
        }
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.finish_rubber_band(pane_id);
        self.paste_primary_into_pane(pane_id, cx);
        true
    }

    fn paste_primary_into_pane(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        let Some(target_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.paste_primary_into_directory(pane_id, target_dir, cx);
    }

    pub(crate) fn paste_primary_into_directory(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.finish_rubber_band(pane_id);
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let Some(clipboard) = self.import_primary_selection(cx) else {
            self.set_pane_status(pane_id, "Nothing to paste from primary selection");
            return;
        };
        self.start_clipboard_transfer(pane_id, target_dir, clipboard, cx);
    }

    fn paste_into_directory(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        self.import_system_clipboard(cx);
        let Some(clipboard) = self.clipboard.clone() else {
            self.set_pane_status(pane_id, "Nothing to paste");
            return;
        };
        self.start_clipboard_transfer(pane_id, target_dir, clipboard, cx);
    }

    fn start_clipboard_transfer(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        clipboard: ClipboardState,
        cx: &mut Context<Self>,
    ) {
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot paste into {}", target_dir.display()),
            );
            return;
        }

        let progress_label = clipboard.progress_label();
        self.begin_pane_operation(pane_id, progress_label.clone());
        let (cancel, progress) = self.start_transfer_progress(pane_id, progress_label);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            paste_clipboard_result(
                                pane_id,
                                target_dir,
                                clipboard,
                                Some(cancel),
                                Some(progress),
                            )
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_transfer(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    pub(crate) fn begin_item_drag(&mut self, payload: ItemDragPayload) {
        let paths = item_drag_paths(&self.panes, &payload);
        self.active_item_drag = Some(ActiveItemDrag { payload, paths });
        self.item_drop_target = None;
        self.place_drop_target = None;
    }

    fn active_item_drag_paths(&self, payload: &ItemDragPayload) -> Option<Vec<PathBuf>> {
        self.active_item_drag
            .as_ref()
            .filter(|drag| drag.payload == *payload)
            .map(|drag| drag.paths.clone())
    }

    fn clear_item_drag(&mut self, payload: &ItemDragPayload) {
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload == *payload)
        {
            self.active_item_drag = None;
            self.item_drop_target = None;
            self.place_drop_target = None;
        }
    }

    pub(crate) fn set_item_drag_drop_target_for_pane(
        &mut self,
        pane_id: PaneId,
        mode: FileTransferMode,
    ) -> bool {
        let target = Some(ItemDropTarget::Pane { pane_id, mode });
        if self.item_drop_target == target && self.place_drop_target.is_none() {
            self.touch_drop_target_stale_generation();
            return false;
        }
        self.item_drop_target = target;
        self.place_drop_target = None;
        self.touch_drop_target_stale_generation();
        true
    }

    pub(crate) fn set_item_drag_drop_target_for_directory(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        mode: FileTransferMode,
    ) -> bool {
        let target = Some(ItemDropTarget::Directory {
            pane_id,
            path,
            mode,
        });
        if self.item_drop_target == target && self.place_drop_target.is_none() {
            self.touch_drop_target_stale_generation();
            return false;
        }
        self.item_drop_target = target;
        self.place_drop_target = None;
        self.touch_drop_target_stale_generation();
        true
    }

    fn clear_item_drop_target(&mut self) -> bool {
        let had_target = self.item_drop_target.is_some();
        self.item_drop_target = None;
        if had_target {
            self.touch_drop_target_stale_generation();
        }
        had_target
    }

    pub(crate) fn set_place_drag_drop_target_for_path(
        &mut self,
        path: PathBuf,
        mode: FileTransferMode,
    ) -> bool {
        let target = Some(PlaceDropTarget::Place { path, mode });
        if self.place_drop_target == target && self.item_drop_target.is_none() {
            self.touch_drop_target_stale_generation();
            return false;
        }
        self.place_drop_target = target;
        self.item_drop_target = None;
        self.touch_drop_target_stale_generation();
        true
    }

    pub(crate) fn set_place_drag_drop_target_for_insert(&mut self, index: usize) -> bool {
        let index = self.user_place_insert_index(index);
        let target = Some(PlaceDropTarget::Insert { index });
        if self.place_drop_target == target && self.item_drop_target.is_none() {
            self.touch_drop_target_stale_generation();
            return false;
        }
        self.place_drop_target = target;
        self.item_drop_target = None;
        self.touch_drop_target_stale_generation();
        true
    }

    fn clear_place_drop_target(&mut self) -> bool {
        let had_target = self.place_drop_target.is_some();
        self.place_drop_target = None;
        if had_target {
            self.touch_drop_target_stale_generation();
        }
        had_target
    }

    pub(crate) fn clear_drag_drop_targets(&mut self) -> bool {
        let had_target = self.item_drop_target.is_some() || self.place_drop_target.is_some();
        self.item_drop_target = None;
        self.place_drop_target = None;
        if had_target {
            self.touch_drop_target_stale_generation();
        }
        had_target
    }

    fn touch_drop_target_stale_generation(&mut self) {
        self.drop_target_stale_generation = self.drop_target_stale_generation.wrapping_add(1);
    }

    pub(crate) fn schedule_drop_target_stale_clear(&mut self, cx: &mut Context<Self>) {
        if self.drop_target_stale_timer_running
            || (self.item_drop_target.is_none() && self.place_drop_target.is_none())
        {
            return;
        }
        self.drop_target_stale_timer_running = true;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        let Ok(generation) =
                            this.update(&mut cx, |app, _cx| app.drop_target_stale_generation)
                        else {
                            break;
                        };
                        cx.background_executor()
                            .timer(DROP_TARGET_STALE_TIMEOUT)
                            .await;
                        let Ok(keep_running) = this.update(&mut cx, |app, cx| {
                            if app.drop_target_stale_generation == generation {
                                let changed =
                                    app.clear_stale_drop_targets_for_generation(generation);
                                app.drop_target_stale_timer_running = false;
                                if changed {
                                    cx.notify();
                                }
                                false
                            } else if app.item_drop_target.is_some()
                                || app.place_drop_target.is_some()
                            {
                                true
                            } else {
                                app.drop_target_stale_timer_running = false;
                                false
                            }
                        }) else {
                            break;
                        };
                        if !keep_running {
                            break;
                        }
                    }
                }
            },
        )
        .detach();
    }

    fn clear_stale_drop_targets_for_generation(&mut self, generation: u64) -> bool {
        if self.drop_target_stale_generation != generation {
            return false;
        }
        let had_target = self.item_drop_target.is_some() || self.place_drop_target.is_some();
        self.item_drop_target = None;
        self.place_drop_target = None;
        if had_target {
            self.touch_drop_target_stale_generation();
        }
        had_target
    }

    pub(crate) fn drop_item_drag_to_pane(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        let Some(target_dir) = self
            .panes
            .pane(target_pane)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.drop_item_drag_to_directory(target_pane, payload, target_dir, mode, cx);
    }

    pub(crate) fn drop_external_paths_to_pane(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        let Some(target_dir) = self
            .panes
            .pane(target_pane)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.drop_external_paths_to_directory(target_pane, paths, target_dir, mode, cx);
    }

    pub(crate) fn drop_external_paths_to_directory(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        self.panes.focus(target_pane);
        self.finish_rubber_band(target_pane);
        self.dismiss_context_menu();
        self.clear_item_drop_target();
        self.clear_place_drop_target();

        if self.operation_pending {
            self.set_pane_status(target_pane, "File operation already running");
            return;
        }
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(target_pane, reason);
            return;
        }

        self.start_file_transfer(target_pane, target_dir, mode, paths, cx);
    }

    pub(crate) fn drop_item_drag_to_place(
        &mut self,
        payload: ItemDragPayload,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        let status_pane = self
            .panes
            .focused()
            .or_else(|| self.panes.pane(payload.source_pane).map(|pane| pane.id));
        if self.chooser.is_some() {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            return;
        }
        let Some(status_pane) = status_pane else {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            return;
        };
        self.dismiss_context_menu();
        self.finish_rubber_band(status_pane);
        self.clear_place_drop_target();

        if self.operation_pending {
            self.clear_item_drag(&payload);
            self.set_pane_status(status_pane, "File operation already running");
            return;
        }

        let Some(paths) = self.active_item_drag_paths(&payload) else {
            self.clear_item_drag(&payload);
            self.set_pane_status(status_pane, "No active item drag");
            return;
        };
        self.clear_item_drag(&payload);
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(status_pane, reason);
            return;
        }

        self.start_file_transfer(status_pane, target_dir, mode, paths, cx);
    }

    pub(crate) fn drop_external_paths_to_place(
        &mut self,
        paths: Vec<PathBuf>,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        let Some(status_pane) = self.panes.focused() else {
            self.clear_place_drop_target();
            return;
        };
        if self.chooser.is_some() {
            self.clear_place_drop_target();
            return;
        }
        self.dismiss_context_menu();
        self.finish_rubber_band(status_pane);
        self.clear_place_drop_target();

        if self.operation_pending {
            self.set_pane_status(status_pane, "File operation already running");
            return;
        }
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(status_pane, reason);
            return;
        }

        self.start_file_transfer(status_pane, target_dir, mode, paths, cx);
    }

    pub(crate) fn drop_item_drag_to_current_place_target(
        &mut self,
        payload: ItemDragPayload,
        fallback_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        match self
            .place_drop_target
            .clone()
            .unwrap_or(PlaceDropTarget::Place {
                path: fallback_dir,
                mode,
            }) {
            PlaceDropTarget::Place { path, .. } => {
                self.drop_item_drag_to_place(payload, path, mode, cx);
            }
            PlaceDropTarget::Insert { index } => {
                self.drop_item_drag_to_place_insert(payload, index);
            }
        }
    }

    pub(crate) fn drop_external_paths_to_current_place_target(
        &mut self,
        paths: Vec<PathBuf>,
        fallback_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        match self
            .place_drop_target
            .clone()
            .unwrap_or(PlaceDropTarget::Place {
                path: fallback_dir,
                mode,
            }) {
            PlaceDropTarget::Place { path, .. } => {
                self.drop_external_paths_to_place(paths, path, mode, cx);
            }
            PlaceDropTarget::Insert { index } => {
                self.drop_external_paths_to_place_insert(paths, index);
            }
        }
    }

    pub(crate) fn drop_item_drag_to_place_insert(
        &mut self,
        payload: ItemDragPayload,
        index: usize,
    ) {
        let status_pane = self
            .panes
            .focused()
            .or_else(|| self.panes.pane(payload.source_pane).map(|pane| pane.id));
        if self.chooser.is_some() {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            return;
        }
        let Some(status_pane) = status_pane else {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            return;
        };
        self.dismiss_context_menu();
        self.finish_rubber_band(status_pane);
        self.clear_place_drop_target();

        let Some(paths) = self.active_item_drag_paths(&payload) else {
            self.clear_item_drag(&payload);
            self.set_pane_status(status_pane, "No active item drag");
            return;
        };
        self.clear_item_drag(&payload);
        self.insert_place_from_dropped_paths(status_pane, paths, index);
    }

    pub(crate) fn drop_external_paths_to_place_insert(
        &mut self,
        paths: Vec<PathBuf>,
        index: usize,
    ) {
        let Some(status_pane) = self.panes.focused() else {
            self.clear_place_drop_target();
            return;
        };
        if self.chooser.is_some() {
            self.clear_place_drop_target();
            return;
        }
        self.dismiss_context_menu();
        self.finish_rubber_band(status_pane);
        self.clear_place_drop_target();
        self.insert_place_from_dropped_paths(status_pane, paths, index);
    }

    pub(crate) fn drop_item_drag_to_location(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drag(&payload);
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        if self.operation_pending {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            self.set_pane_status(target_pane, "File operation already running");
            return;
        }
        let Some(paths) = self.active_item_drag_paths(&payload) else {
            self.clear_item_drag(&payload);
            self.set_pane_status(target_pane, "No active item drag");
            return;
        };
        self.clear_item_drag(&payload);
        self.clear_place_drop_target();
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(target_pane, reason);
            return;
        }

        self.load_pane(target_pane, target_dir.clone());
        self.start_file_transfer(target_pane, target_dir, mode, paths, cx);
    }

    pub(crate) fn drop_external_paths_to_location(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        if self.operation_pending {
            self.clear_place_drop_target();
            self.set_pane_status(target_pane, "File operation already running");
            return;
        }
        self.clear_place_drop_target();
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(target_pane, reason);
            return;
        }

        self.load_pane(target_pane, target_dir.clone());
        self.start_file_transfer(target_pane, target_dir, mode, paths, cx);
    }

    pub(crate) fn drop_item_drag_to_directory(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        target_dir: PathBuf,
        mode: FileTransferMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drag(&payload);
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        self.panes.focus(target_pane);
        self.finish_rubber_band(target_pane);
        self.dismiss_context_menu();
        self.clear_item_drop_target();
        self.clear_place_drop_target();

        if self.operation_pending {
            self.clear_item_drag(&payload);
            self.clear_place_drop_target();
            self.set_pane_status(target_pane, "File operation already running");
            return;
        }

        let Some(paths) = self.active_item_drag_paths(&payload) else {
            self.clear_item_drag(&payload);
            self.set_pane_status(target_pane, "No active item drag");
            return;
        };
        self.clear_item_drag(&payload);
        self.clear_place_drop_target();
        if paths.is_empty() {
            self.set_pane_status(target_pane, "No dragged items");
            return;
        }
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(target_pane, reason);
            return;
        }

        self.start_file_transfer(target_pane, target_dir, mode, paths, cx);
    }

    fn start_file_transfer(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        mode: FileTransferMode,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot drop into {}", target_dir.display()),
            );
            return;
        }

        let progress_label = mode.progress_label(paths.len());
        self.begin_pane_operation(pane_id, progress_label.clone());
        let (cancel, progress) = self.start_transfer_progress(pane_id, progress_label);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            transfer_paths_result(
                                pane_id,
                                target_dir,
                                mode,
                                paths,
                                mode.label(),
                                false,
                                Some(cancel),
                                Some(progress),
                            )
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_transfer(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_transfer(&mut self, result: TransferTaskResult) {
        self.clear_operation_progress();
        let TransferTaskResult {
            pane_id,
            mode,
            label,
            clear_clipboard,
            success_count,
            failure_count,
            affected_dirs,
            undo_items,
            created_items,
        } = result;

        if success_count > 0 {
            let created_selection = created_items.first().map(|item| item.path.clone());
            let has_transfer_items = !undo_items.is_empty();
            if has_transfer_items {
                self.operations.register_undo_with_payload(
                    mode.label().to_string(),
                    affected_dirs.clone(),
                    UndoPayload::Transfer { items: undo_items },
                );
            }
            if !created_items.is_empty() {
                self.operations.register_undo_with_payload(
                    label.to_string(),
                    affected_dirs.clone(),
                    UndoPayload::Create {
                        items: created_items,
                    },
                );
            }
            self.refresh_affected_dirs(&affected_dirs);
            if let Some(path) = created_selection {
                self.rubber_band_selection_panes.remove(&pane_id);
                let _ = self.panes.select_only(pane_id, path);
            }
            if clear_clipboard && has_transfer_items {
                self.clipboard = None;
                self.rubber_band_selection_panes.remove(&pane_id);
                let _ = self.panes.clear_selection(pane_id);
            }
        }

        self.finish_pane_operation(
            pane_id,
            action_status(&format!("{label} complete"), success_count, failure_count),
        );
    }

    fn trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, "No selection to trash");
            return;
        }

        self.begin_pane_operation(
            pane_id,
            format!("Moving {} item(s) to trash", selected_paths.len()),
        );
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
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                "Move to Trash".to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Trash {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            self.rubber_band_selection_panes.remove(&result.pane_id);
            let _ = self.panes.clear_selection(result.pane_id);
        }

        self.finish_pane_operation(
            result.pane_id,
            action_status("Moved to trash", result.success_count, result.failure_count),
        );
    }

    fn restore_trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        self.start_trash_view_selection_operation(
            pane_id,
            TrashViewOperation::Restore,
            "No trash selection to restore",
            cx,
        );
    }

    fn delete_trash_selection_permanently(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        self.start_trash_view_selection_operation(
            pane_id,
            TrashViewOperation::DeletePermanently,
            "No trash selection to delete",
            cx,
        );
    }

    fn start_trash_view_selection_operation(
        &mut self,
        pane_id: PaneId,
        operation: TrashViewOperation,
        empty_selection_status: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        if !self.trash_view_state(pane_id).0 {
            self.set_pane_status(pane_id, "Trash action is only available in Trash");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, empty_selection_status);
            return;
        }
        self.start_trash_view_operation(pane_id, operation, selected_paths, cx);
    }

    fn empty_trash(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let (trash_view, trash_has_items) = self.trash_view_state(pane_id);
        if !trash_view {
            self.set_pane_status(pane_id, "Empty Trash is only available in Trash");
            return;
        }
        if !trash_has_items {
            self.set_pane_status(pane_id, "Trash is empty");
            return;
        }
        self.start_trash_view_operation(pane_id, TrashViewOperation::Empty, Vec::new(), cx);
    }

    fn empty_trash_from_place(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        if !file_ops::trash_has_items() {
            self.set_pane_status(pane_id, "Trash is empty");
            return;
        }
        self.start_trash_view_operation(pane_id, TrashViewOperation::Empty, Vec::new(), cx);
    }

    fn start_trash_view_operation(
        &mut self,
        pane_id: PaneId,
        operation: TrashViewOperation,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.begin_pane_operation(pane_id, operation.progress_label(paths.len()));
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            trash_view_operation_result(pane_id, operation, paths)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_view_operation(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_trash_view_operation(&mut self, result: TrashViewOperationResult) {
        if result.success_count > 0 {
            self.refresh_affected_dirs(&result.affected_dirs);
            self.rubber_band_selection_panes.remove(&result.pane_id);
            let _ = self.panes.clear_selection(result.pane_id);
        }
        self.finish_pane_operation(
            result.pane_id,
            action_status(
                result.operation.completed_label(),
                result.success_count,
                result.failure_count,
            ),
        );
    }

    fn undo_latest(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let Some(record) = self.operations.latest_undo().cloned() else {
            self.set_pane_status(pane_id, "No operation to undo");
            return;
        };

        match &record.payload {
            UndoPayload::Create { .. } => {}
            UndoPayload::Rename { .. } => {}
            UndoPayload::Trash { .. } => {}
            UndoPayload::Transfer { .. } => {}
            UndoPayload::None => {
                self.set_pane_status(pane_id, format!("No undo action for {}", record.label));
                return;
            }
        }

        self.begin_pane_operation(pane_id, format!("Undoing {}", record.label));
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move { undo_record_result(record) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_undo(pane_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_undo(&mut self, pane_id: PaneId, result: UndoTaskResult) {
        match result.result {
            Ok(message) => {
                if self
                    .operations
                    .take_latest_undo(result.record.serial)
                    .is_none()
                {
                    self.finish_pane_operation(pane_id, "Undo result is stale");
                    return;
                }
                self.refresh_affected_dirs(&result.record.affected_dirs);
                self.finish_pane_operation(
                    pane_id,
                    format!("Undid {}: {message}", result.record.label),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    pane_id,
                    format!("Cannot undo {}: {err}", result.record.label),
                );
            }
        }
    }

    fn refresh_affected_dirs(&mut self, affected_dirs: &[PathBuf]) {
        let refreshes = OperationQueue::refresh_affected_panes(&mut self.panes, affected_dirs);
        self.schedule_listings(refreshes.iter().map(|refresh| &refresh.event));
        for refresh in refreshes {
            self.apply_event(refresh.event);
            self.start_watcher(refresh.pane_id);
        }
    }

    fn show_blank_context_menu_if_blank(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.panes.focus(pane_id);
        self.finish_rubber_band(pane_id);
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        if self.item_at_content_point(pane_id, point).is_some() {
            return false;
        }
        if self.rubber_band_selection_active(pane_id) {
            self.dismiss_context_menu();
            self.clear_selection(pane_id);
            return false;
        }
        self.show_blank_context_menu(
            pane_id,
            ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
        );
        true
    }

    fn show_blank_context_menu(&mut self, pane_id: PaneId, position: ViewPoint) {
        let (trash_view, trash_has_items) = self.trash_view_state(pane_id);
        let service_actions = if trash_view {
            Vec::new()
        } else {
            self.mime_applications
                .service_actions_for_target(Some("inode/directory"), true)
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Blank {
                trash_view,
                trash_has_items,
                service_actions,
            },
            position,
            active_submenu: None,
        });
    }

    fn show_item_context_menu(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.panes.focus(pane_id);
        self.finish_rubber_band(pane_id);
        let item_selected = self.panes.is_selected(pane_id, &path);
        if self.rubber_band_selection_active(pane_id) && !item_selected {
            self.dismiss_context_menu();
            self.clear_selection(pane_id);
            return false;
        }
        if !item_selected {
            self.select_only(pane_id, path.clone());
        }
        let selection_count = self.panes.selected_count(pane_id).unwrap_or(1).max(1);
        let trash_view = self.trash_view_state(pane_id).0;
        let trash_can_restore = trash_view && file_ops::trash_metadata(&path).is_ok();
        let mime_type = self
            .mime_type_for_pane_path(pane_id, &path)
            .or_else(|| is_dir.then(|| Arc::from("inode/directory")));
        let open_with_apps = mime_type
            .as_deref()
            .map(|mime| self.mime_applications.applications_for_mime(mime))
            .unwrap_or_default();
        let service_actions = self.mime_applications.service_actions_for_targets(
            &self.service_menu_targets_for_context(pane_id, &path, is_dir, selection_count),
        );
        let menu_position = ViewPoint {
            x: position.x.as_f32(),
            y: position.y.as_f32(),
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Item {
                path,
                is_dir,
                selection_count,
                trash_view,
                trash_can_restore,
                mime_type,
                open_with_apps,
                service_actions,
            },
            position: menu_position,
            active_submenu: None,
        });
        true
    }

    fn mime_type_for_pane_path(&self, pane_id: PaneId, path: &Path) -> Option<Arc<str>> {
        let pane = self.panes.pane(pane_id)?;
        let index = pane.model.index_of_path(path)?;
        pane.model.get(index)?.mime_type.clone()
    }

    fn service_menu_targets_for_context(
        &self,
        pane_id: PaneId,
        path: &Path,
        is_dir: bool,
        selection_count: usize,
    ) -> Vec<ServiceMenuTarget> {
        if selection_count > 1
            && let Some(pane) = self.panes.pane(pane_id)
        {
            let targets = pane
                .model
                .entries()
                .iter()
                .filter(|entry| pane.selection.is_selected(entry.id))
                .map(|entry| ServiceMenuTarget {
                    mime_type: entry.mime_type.as_deref().map(str::to_string),
                    is_dir: entry.is_dir,
                })
                .collect::<Vec<_>>();
            if !targets.is_empty() {
                return targets;
            }
        }
        vec![ServiceMenuTarget::new(
            self.mime_type_for_pane_path(pane_id, path).as_deref(),
            is_dir,
        )]
    }

    fn service_menu_paths_for_context(
        &self,
        pane_id: PaneId,
        path: PathBuf,
        selection_count: usize,
    ) -> Vec<PathBuf> {
        if selection_count > 1 {
            let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
            if !selected_paths.is_empty() {
                return selected_paths;
            }
        }
        vec![path]
    }

    fn trash_view_state(&self, pane_id: PaneId) -> (bool, bool) {
        self.panes
            .pane(pane_id)
            .map(|pane| {
                let trash_view = file_ops::is_trash_files_dir(&pane.current_dir);
                let trash_has_items = trash_view && !pane.model.is_empty();
                (trash_view, trash_has_items)
            })
            .unwrap_or_default()
    }

    fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
        self.context_menu_tree_hovered = false;
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
    }

    fn set_context_menu(&mut self, menu: ContextMenuState) {
        self.context_menu = Some(menu);
        self.context_menu_tree_hovered = true;
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
    }

    fn open_context_submenu(&mut self, submenu: ContextMenuSubmenu, parent_index: usize) {
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
        self.context_menu_tree_hovered = true;
        if let Some(menu) = self.context_menu.as_mut() {
            menu.active_submenu = Some(ContextMenuOpenSubmenu {
                submenu,
                parent_index,
                nested: None,
            });
        }
    }

    fn open_context_nested_submenu(&mut self, submenu: ContextMenuSubmenu, parent_index: usize) {
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
        self.context_menu_tree_hovered = true;
        if let Some(open) = self
            .context_menu
            .as_mut()
            .and_then(|menu| menu.active_submenu.as_mut())
        {
            open.nested = Some(ContextMenuNestedSubmenu {
                submenu,
                parent_index,
            });
        }
    }

    fn clear_context_nested_submenu(&mut self) -> bool {
        let Some(open) = self
            .context_menu
            .as_mut()
            .and_then(|menu| menu.active_submenu.as_mut())
        else {
            return false;
        };
        if open.nested.is_none() {
            return false;
        }
        open.nested = None;
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
        true
    }

    fn set_context_menu_tree_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) -> bool {
        if self.context_menu_tree_hovered == hovered {
            return false;
        }
        self.context_menu_tree_hovered = hovered;
        if hovered {
            self.cancel_context_submenu_hide();
        } else {
            self.schedule_context_submenu_hide(cx);
        }
        true
    }

    fn cancel_context_submenu_hide(&mut self) {
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
    }

    fn schedule_context_submenu_hide(&mut self, cx: &mut Context<Self>) {
        if self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.active_submenu)
            .is_none()
        {
            return;
        }
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
        let generation = self.context_submenu_hide_generation;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    cx.background_executor()
                        .timer(CONTEXT_SUBMENU_HIDE_DELAY)
                        .await;
                    if this
                        .update(&mut cx, |app, cx| {
                            if app.clear_context_submenu_if_generation(generation) {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            },
        )
        .detach();
    }

    fn clear_context_submenu_if_generation(&mut self, generation: u64) -> bool {
        if self.context_submenu_hide_generation != generation {
            return false;
        }
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        if menu.active_submenu.is_none() {
            return false;
        }
        menu.active_submenu = None;
        self.context_submenu_hide_generation = self.context_submenu_hide_generation.wrapping_add(1);
        true
    }

    pub(crate) fn dismiss_place_draft(&mut self) {
        self.place_draft = None;
    }

    pub(crate) fn set_place_draft_focus(&mut self, field: PlaceDraftField) {
        if let Some(draft) = &mut self.place_draft {
            draft.focus = field;
        }
    }

    pub(crate) fn dismiss_properties_dialog(&mut self) {
        self.properties_dialog = None;
    }

    fn dismiss_application_chooser(&mut self) {
        self.application_chooser = None;
    }

    fn show_application_chooser(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        mime_type: Option<Arc<str>>,
    ) {
        let applications = self.application_chooser_applications(mime_type.as_deref());
        if applications.is_empty() {
            self.set_pane_status(pane_id, "No applications found");
            return;
        }
        self.application_chooser = Some(ApplicationChooserState {
            pane_id,
            path,
            mime_type,
            applications,
            scroll_handle: gpui::UniformListScrollHandle::new(),
        });
    }

    fn application_chooser_applications(&self, mime_type: Option<&str>) -> Vec<MimeApplication> {
        let default_ids = mime_type
            .map(|mime| {
                self.mime_applications
                    .applications_for_mime(mime)
                    .into_iter()
                    .filter(|app| app.is_default)
                    .map(|app| app.id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut applications = self.mime_applications.all_applications();
        for app in &mut applications {
            app.is_default = default_ids.contains(&app.id);
        }
        applications
    }

    fn choose_application_for_open_with(&mut self, desktop_id: String, cx: &mut Context<Self>) {
        let Some(chooser) = self.application_chooser.take() else {
            return;
        };
        self.open_with_application(chooser.pane_id, &desktop_id, chooser.path, cx);
    }

    fn set_default_open_with_application(&mut self, desktop_id: String) {
        let Some((pane_id, mime_type)) = self.application_chooser.as_ref().and_then(|chooser| {
            chooser
                .mime_type
                .clone()
                .map(|mime| (chooser.pane_id, mime))
        }) else {
            return;
        };
        if self.mime_applications.application(&desktop_id).is_none() {
            self.set_pane_status(pane_id, "Application is no longer available");
            return;
        }
        match set_default_mime_application(&mime_type, &desktop_id) {
            Ok(path) => {
                self.mime_applications = MimeApplicationCache::load();
                let applications = self.application_chooser_applications(Some(&mime_type));
                if let Some(chooser) = &mut self.application_chooser {
                    chooser.applications = applications;
                }
                self.set_pane_status(
                    pane_id,
                    format!(
                        "Set default application for {} in {}",
                        mime_type,
                        path.display()
                    ),
                );
            }
            Err(error) => {
                self.set_pane_status(
                    pane_id,
                    format!("Could not set default application: {error}"),
                );
            }
        }
    }

    fn show_properties_for_context(&mut self, pane_id: PaneId, target: ContextMenuTarget) {
        let dialog = match target {
            ContextMenuTarget::Blank { .. } => {
                let Some(path) = self
                    .panes
                    .pane(pane_id)
                    .map(|pane| pane.current_dir.clone())
                else {
                    return;
                };
                properties_for_path(&path)
            }
            ContextMenuTarget::Item {
                path,
                selection_count,
                ..
            } if selection_count > 1 => {
                let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
                if selected_paths.is_empty() {
                    properties_for_path(&path)
                } else {
                    properties_for_selection(&selected_paths)
                }
            }
            ContextMenuTarget::Item { path, .. } => properties_for_path(&path),
            ContextMenuTarget::Place { path, .. } => properties_for_path(&path),
            ContextMenuTarget::PlacesBlank { .. } | ContextMenuTarget::PlaceSection { .. } => {
                return;
            }
        };
        self.properties_dialog = Some(dialog);
    }

    fn run_context_menu_action(&mut self, action: ContextMenuAction, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.clone() else {
            return;
        };
        self.dismiss_context_menu();
        self.panes.focus(menu.pane_id);

        match (action, menu.target) {
            (
                ContextMenuAction::Open,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.load_pane(menu.pane_id, path),
            (
                ContextMenuAction::OpenInNewPane,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.open_path_in_new_pane(menu.pane_id, path),
            (
                ContextMenuAction::OpenInNewWindow,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.open_path_in_new_window(menu.pane_id, path, cx),
            (
                ContextMenuAction::Open,
                ContextMenuTarget::Item {
                    path,
                    is_dir: false,
                    ..
                },
            ) => {
                self.select_only(menu.pane_id, path.clone());
                self.set_pane_status(
                    menu.pane_id,
                    format!("Open With menu unavailable for {}", path.display()),
                );
            }
            (
                ContextMenuAction::OpenWithApplication { desktop_id },
                ContextMenuTarget::Item { path, .. },
            ) => self.open_with_application(menu.pane_id, &desktop_id, path, cx),
            (
                ContextMenuAction::OtherApplication,
                ContextMenuTarget::Item {
                    path, mime_type, ..
                },
            ) => self.show_application_chooser(menu.pane_id, path, mime_type),
            (
                ContextMenuAction::RunServiceMenuAction { action_id },
                ContextMenuTarget::Item {
                    path,
                    selection_count,
                    ..
                },
            ) => {
                let paths =
                    self.service_menu_paths_for_context(menu.pane_id, path, selection_count);
                self.run_service_menu_action(menu.pane_id, &action_id, paths, cx);
            }
            (
                ContextMenuAction::RunServiceMenuAction { action_id },
                ContextMenuTarget::Blank { .. },
            ) => {
                if let Some(path) = self
                    .panes
                    .pane(menu.pane_id)
                    .map(|pane| pane.current_dir.clone())
                {
                    self.run_service_menu_action(menu.pane_id, &action_id, vec![path], cx);
                }
            }
            (
                ContextMenuAction::CompressWithArk,
                ContextMenuTarget::Item {
                    path,
                    selection_count,
                    ..
                },
            ) => {
                let paths =
                    self.service_menu_paths_for_context(menu.pane_id, path, selection_count);
                self.run_ark_compress_fallback(menu.pane_id, paths, cx);
            }
            (
                ContextMenuAction::ExtractHereWithArk,
                ContextMenuTarget::Item {
                    path,
                    is_dir: false,
                    ..
                },
            ) => self.run_ark_extract_here_fallback(menu.pane_id, path, cx),
            (
                ContextMenuAction::ExtractToWithArk,
                ContextMenuTarget::Item {
                    path,
                    is_dir: false,
                    ..
                },
            ) => self.run_ark_extract_to_fallback(menu.pane_id, path, cx),
            (
                ContextMenuAction::Open,
                ContextMenuTarget::Place {
                    path,
                    mounted: true,
                    ..
                },
            ) => {
                self.open_place(path);
            }
            (
                ContextMenuAction::OpenInNewPane,
                ContextMenuTarget::Place {
                    path,
                    mounted: true,
                    ..
                },
            ) => {
                self.open_path_in_new_pane(menu.pane_id, path);
            }
            (
                ContextMenuAction::OpenInNewWindow,
                ContextMenuTarget::Place {
                    path,
                    mounted: true,
                    ..
                },
            ) => {
                self.open_path_in_new_window(menu.pane_id, path, cx);
            }
            (
                ContextMenuAction::Open
                | ContextMenuAction::OpenInNewPane
                | ContextMenuAction::OpenInNewWindow,
                ContextMenuTarget::Place { mounted: false, .. },
            ) => {}
            (
                ContextMenuAction::MountDevice,
                ContextMenuTarget::Place {
                    path, device: true, ..
                },
            ) => {
                self.run_device_place_operation(menu.pane_id, path, DevicePlaceOperation::Mount, cx)
            }
            (
                ContextMenuAction::UnmountDevice,
                ContextMenuTarget::Place {
                    path, device: true, ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                path,
                DevicePlaceOperation::Unmount,
                cx,
            ),
            (
                ContextMenuAction::EjectDevice,
                ContextMenuTarget::Place {
                    path, device: true, ..
                },
            ) => {
                self.run_device_place_operation(menu.pane_id, path, DevicePlaceOperation::Eject, cx)
            }
            (
                ContextMenuAction::SafelyRemoveDevice,
                ContextMenuTarget::Place {
                    path, device: true, ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                path,
                DevicePlaceOperation::SafelyRemove,
                cx,
            ),
            (ContextMenuAction::AddPlace, ContextMenuTarget::PlacesBlank { .. }) => {
                self.start_add_place(menu.pane_id);
            }
            (
                ContextMenuAction::EditPlace,
                ContextMenuTarget::Place {
                    path,
                    editable: true,
                    ..
                },
            ) => self.start_edit_place(menu.pane_id, path),
            (
                ContextMenuAction::RemovePlace,
                ContextMenuTarget::Place {
                    path,
                    removable: true,
                    ..
                },
            ) => self.remove_place(menu.pane_id, &path),
            (ContextMenuAction::HidePlace, ContextMenuTarget::Place { path, .. }) => {
                self.hide_place(menu.pane_id, path);
            }
            (ContextMenuAction::HidePlaceSection, ContextMenuTarget::PlaceSection { group }) => {
                self.hide_place_section(menu.pane_id, group);
            }
            (ContextMenuAction::ShowHiddenPlaces, ContextMenuTarget::PlacesBlank { .. }) => {
                self.show_hidden_places(menu.pane_id);
            }
            (ContextMenuAction::Rename, ContextMenuTarget::Item { path, .. }) => {
                self.select_only(menu.pane_id, path);
                self.start_rename_in_pane(menu.pane_id);
            }
            (ContextMenuAction::Copy, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Blank { .. }) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Copy, cx)
            }
            (ContextMenuAction::CopyLocation, ContextMenuTarget::Item { path, .. }) => {
                let location = path.display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(location));
                self.set_pane_status(menu.pane_id, format!("Copied location {}", path.display()));
            }
            (ContextMenuAction::CopyLocation, ContextMenuTarget::Place { path, .. }) => {
                let location = path.display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(location));
                self.set_pane_status(menu.pane_id, format!("Copied location {}", path.display()));
            }
            (ContextMenuAction::Cut, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Blank { .. }) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Cut, cx)
            }
            (ContextMenuAction::Trash, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Blank { .. }) => {
                self.trash_selection(menu.pane_id, cx)
            }
            (ContextMenuAction::RestoreFromTrash, ContextMenuTarget::Item { .. }) => {
                self.restore_trash_selection(menu.pane_id, cx)
            }
            (ContextMenuAction::DeletePermanently, ContextMenuTarget::Item { .. }) => {
                self.delete_trash_selection_permanently(menu.pane_id, cx)
            }
            (ContextMenuAction::EmptyTrash, ContextMenuTarget::Blank { .. }) => {
                self.empty_trash(menu.pane_id, cx)
            }
            (
                ContextMenuAction::EmptyTrash,
                ContextMenuTarget::Place {
                    trash_place: true, ..
                },
            ) => self.empty_trash_from_place(menu.pane_id, cx),
            (ContextMenuAction::Properties, target) => {
                self.show_properties_for_context(menu.pane_id, target)
            }
            (
                ContextMenuAction::CreateFolder,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.create_item_in_directory(menu.pane_id, path, CreatedItemKind::Folder, cx),
            (
                ContextMenuAction::CreateFile,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.create_item_in_directory(menu.pane_id, path, CreatedItemKind::File, cx),
            (ContextMenuAction::CreateFolder, ContextMenuTarget::Blank { .. }) => {
                self.create_item_in_pane(menu.pane_id, CreatedItemKind::Folder, cx)
            }
            (ContextMenuAction::CreateFile, ContextMenuTarget::Blank { .. }) => {
                self.create_item_in_pane(menu.pane_id, CreatedItemKind::File, cx)
            }
            (
                ContextMenuAction::CreateFolder | ContextMenuAction::CreateFile,
                ContextMenuTarget::Place { .. },
            )
            | (ContextMenuAction::Paste, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::SelectAll, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Refresh, ContextMenuTarget::Place { .. }) => {}
            (
                ContextMenuAction::Paste,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.paste_into_directory(menu.pane_id, path, cx),
            (ContextMenuAction::Paste, _) => self.paste_into_pane(menu.pane_id, cx),
            (ContextMenuAction::SelectAll, _) => self.select_all(menu.pane_id),
            (ContextMenuAction::Refresh, _) => self.reload_pane(menu.pane_id),
            (ContextMenuAction::ViewCompact, _) => {
                self.set_pane_status(menu.pane_id, "Compact view")
            }
            (ContextMenuAction::SortByName, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Name)
            }
            (ContextMenuAction::SortByModified, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Modified)
            }
            (ContextMenuAction::SortBySize, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Size)
            }
            (ContextMenuAction::SortByOriginalPath, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::TrashOriginalPath)
            }
            (ContextMenuAction::SortByDeletionTime, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::TrashDeletionTime)
            }
            (ContextMenuAction::SortAscending, _) => {
                self.set_pane_sort_order(menu.pane_id, SortOrder::Ascending)
            }
            (ContextMenuAction::SortDescending, _) => {
                self.set_pane_sort_order(menu.pane_id, SortOrder::Descending)
            }
            (ContextMenuAction::SortFoldersFirst, _) => {
                let folders_first = self
                    .panes
                    .sort_descriptor(menu.pane_id)
                    .map(|sort| !sort.folders_first)
                    .unwrap_or(true);
                self.set_pane_sort_folders_first(menu.pane_id, folders_first);
            }
            (ContextMenuAction::SortHiddenLast, _) => {
                let hidden_last = self
                    .panes
                    .sort_descriptor(menu.pane_id)
                    .map(|sort| !sort.hidden_last)
                    .unwrap_or(false);
                self.set_pane_sort_hidden_last(menu.pane_id, hidden_last);
            }
            (
                ContextMenuAction::CreateNewSubmenu
                | ContextMenuAction::CreateFolder
                | ContextMenuAction::CreateFile
                | ContextMenuAction::SortBySubmenu
                | ContextMenuAction::OpenWithSubmenu
                | ContextMenuAction::ServiceMenuSubmenu
                | ContextMenuAction::ServiceMenuGroupSubmenu { .. }
                | ContextMenuAction::ViewModeSubmenu
                | ContextMenuAction::ViewIcons
                | ContextMenuAction::ViewDetails,
                _,
            ) => {}
            (ContextMenuAction::Open, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Open, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::OpenInNewPane, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::OpenInNewWindow, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Open, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::OpenInNewPane, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::OpenInNewWindow, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::OpenInNewPane, _)
            | (ContextMenuAction::OpenInNewWindow, _)
            | (ContextMenuAction::AddPlace, _)
            | (ContextMenuAction::EditPlace, _)
            | (ContextMenuAction::RemovePlace, _)
            | (ContextMenuAction::HidePlace, _)
            | (ContextMenuAction::HidePlaceSection, _)
            | (ContextMenuAction::ShowHiddenPlaces, _)
            | (ContextMenuAction::Rename, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::RestoreFromTrash, _)
            | (ContextMenuAction::DeletePermanently, _)
            | (ContextMenuAction::EmptyTrash, _)
            | (ContextMenuAction::OpenWithApplication { .. }, _)
            | (ContextMenuAction::OtherApplication, _)
            | (ContextMenuAction::RunServiceMenuAction { .. }, _)
            | (ContextMenuAction::CompressWithArk, _)
            | (ContextMenuAction::ExtractHereWithArk, _)
            | (ContextMenuAction::ExtractToWithArk, _)
            | (ContextMenuAction::MountDevice, _)
            | (ContextMenuAction::UnmountDevice, _)
            | (ContextMenuAction::EjectDevice, _)
            | (ContextMenuAction::SafelyRemoveDevice, _) => {}
        }
    }

    fn open_with_launch_plan(
        &self,
        desktop_id: &str,
        path: &Path,
    ) -> Result<DesktopLaunchPlan, String> {
        let Some(application) = self.mime_applications.application(desktop_id) else {
            return Err(format!("Application not found: {desktop_id}"));
        };
        application
            .launch_plan(&[path.to_path_buf()])
            .ok_or_else(|| format!("Cannot build Open With command for {}", application.name))
    }

    fn new_window_launch_plan(&self, path: &Path) -> Result<DesktopLaunchPlan, LauncherError> {
        current_executable_launch_plan("fika-new-window", "Fika", vec![path.display().to_string()])
    }

    fn open_path_in_new_window(&mut self, pane_id: PaneId, path: PathBuf, cx: &mut Context<Self>) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let plan = match self.new_window_launch_plan(&path) {
            Ok(plan) => plan,
            Err(err) => {
                self.set_pane_status(pane_id, format!("Cannot open new window: {err}"));
                return;
            }
        };
        self.begin_pane_operation(
            pane_id,
            format!("Opening new window for {}", path.display()),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_open_in_new_window(NewWindowLaunchResult {
                            pane_id,
                            path,
                            result,
                        });
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn open_with_application(
        &mut self,
        pane_id: PaneId,
        desktop_id: &str,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let plan = match self.open_with_launch_plan(desktop_id, &path) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        let _ = self.panes.select_only(pane_id, path.clone());
        self.begin_pane_operation(
            pane_id,
            format!("Opening {} with {}", path.display(), app_name),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_open_with_application(OpenWithLaunchResult {
                            pane_id,
                            path,
                            app_name,
                            result,
                        });
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn service_menu_launch_plan(
        &self,
        action_id: &str,
        paths: &[PathBuf],
    ) -> Result<DesktopLaunchPlan, String> {
        self.mime_applications
            .service_action_launch_plan(action_id, paths)
            .ok_or_else(|| format!("Service action not found: {action_id}"))
    }

    fn run_service_menu_action(
        &mut self,
        pane_id: PaneId,
        action_id: &str,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        if paths.is_empty() {
            self.set_pane_status(pane_id, "No item selected");
            return;
        }
        let plan = match self.service_menu_launch_plan(action_id, &paths) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(ServiceMenuLaunchResult {
                            pane_id,
                            target_label,
                            app_name,
                            result,
                        });
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn run_ark_compress_fallback(
        &mut self,
        pane_id: PaneId,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let plan = match ark_compress_launch_plan(&paths) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(ServiceMenuLaunchResult {
                            pane_id,
                            target_label,
                            app_name,
                            result,
                        });
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn run_ark_extract_here_fallback(
        &mut self,
        pane_id: PaneId,
        archive: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.run_ark_extract_fallback(pane_id, archive, false, cx);
    }

    fn run_ark_extract_to_fallback(
        &mut self,
        pane_id: PaneId,
        archive: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.run_ark_extract_fallback(pane_id, archive, true, cx);
    }

    fn run_ark_extract_fallback(
        &mut self,
        pane_id: PaneId,
        archive: PathBuf,
        dialog: bool,
        cx: &mut Context<Self>,
    ) {
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let plan = match if dialog {
            ark_extract_to_launch_plan(&archive)
        } else {
            ark_extract_here_launch_plan(&archive)
        } {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        let target_label = archive.display().to_string();
        self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(ServiceMenuLaunchResult {
                            pane_id,
                            target_label,
                            app_name,
                            result,
                        });
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_open_with_application(&mut self, result: OpenWithLaunchResult) {
        self.finish_pane_operation(result.pane_id, result.status_message());
    }

    fn finish_open_in_new_window(&mut self, result: NewWindowLaunchResult) {
        self.finish_pane_operation(result.pane_id, result.status_message());
    }

    fn finish_service_menu_action(&mut self, result: ServiceMenuLaunchResult) {
        self.finish_pane_operation(result.pane_id, result.status_message());
    }

    fn handle_keystroke(&mut self, event: &gpui::KeystrokeEvent, cx: &mut Context<Self>) -> bool {
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.properties_dialog.is_some() {
            self.dismiss_properties_dialog();
            return true;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.application_chooser.is_some()
        {
            self.dismiss_application_chooser();
            return true;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.context_menu.is_some() {
            self.dismiss_context_menu();
            return true;
        }
        if self.handle_location_keystroke(&event.keystroke) {
            return true;
        }
        if self.handle_rename_keystroke(&event.keystroke, cx) {
            return true;
        }
        if self.handle_place_draft_keystroke(&event.keystroke) {
            return true;
        }
        let Some(pane_id) = self.panes.focused() else {
            return false;
        };
        if self.handle_filter_keystroke(pane_id, &event.keystroke) {
            return true;
        }
        match pane_shortcut(&event.keystroke) {
            Some(PaneShortcut::SelectAll) => self.select_all(pane_id),
            Some(PaneShortcut::ClearSelection) => self.clear_selection(pane_id),
            Some(PaneShortcut::Refresh) => self.reload_pane(pane_id),
            Some(PaneShortcut::GoParent) => self.go_parent(pane_id),
            Some(PaneShortcut::GoBack) => self.go_back(pane_id),
            Some(PaneShortcut::GoForward) => self.go_forward(pane_id),
            Some(PaneShortcut::SplitPane) => self.split_pane(pane_id),
            Some(PaneShortcut::ClosePane) => self.close_pane(pane_id),
            Some(PaneShortcut::EditLocation) => self.start_location_edit(pane_id),
            Some(PaneShortcut::ShowFilter) => self.show_filter_bar(pane_id),
            Some(PaneShortcut::Zoom(change)) => self.apply_zoom_change(pane_id, change),
            Some(PaneShortcut::MoveSelection { direction, extend }) => {
                self.move_selection(pane_id, direction, extend)
            }
            Some(PaneShortcut::CreateFolder) => {
                self.create_item_in_pane(pane_id, CreatedItemKind::Folder, cx)
            }
            Some(PaneShortcut::RenameSelection) => self.start_rename_in_pane(pane_id),
            Some(PaneShortcut::CopySelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Copy, cx)
            }
            Some(PaneShortcut::CutSelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Cut, cx)
            }
            Some(PaneShortcut::PasteIntoPane) => self.paste_into_pane(pane_id, cx),
            Some(PaneShortcut::TrashSelection) => self.trash_selection(pane_id, cx),
            Some(PaneShortcut::Undo) => self.undo_latest(pane_id, cx),
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
            if let Some(pane_id) = self.panes.focused() {
                self.set_pane_status(pane_id, "No chooser selection");
            }
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
        self.apply_event_with_previous_summary(event, None);
    }

    fn apply_event_with_previous_summary(
        &mut self,
        event: DirectoryListerEvent,
        previous_summary: Option<String>,
    ) {
        self.update_loading_state(&event, previous_summary);
        if let DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } = &event {
            self.listing_worker.remove_cached_directory(path);
            let still_current = self.panes.pane(*pane_id).is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            });
            if still_current {
                let fallback =
                    nearest_existing_ancestor(path).unwrap_or_else(|| PathBuf::from("/"));
                self.set_pane_status(*pane_id, format!("{} was removed", path.display()));
                self.load_pane(*pane_id, fallback);
            }
            return;
        }

        match &event {
            DirectoryListerEvent::ItemsAdded { path, .. }
            | DirectoryListerEvent::ItemsDeleted { path, .. }
            | DirectoryListerEvent::ItemsRefreshed { path, .. } => {
                self.listing_worker.mark_cache_stale(path);
            }
            _ => {}
        }

        let pane_id = event.pane_id();
        if let Some(signals) = self.panes.apply_lister_event(event) {
            if !signals.is_empty() {
                self.invalidate_pane_layout_projection(pane_id, false);
                self.set_pane_status(pane_id, format!("{} model signal(s)", signals.len()));
            }
        }
    }

    fn update_loading_state(
        &mut self,
        event: &DirectoryListerEvent,
        previous_summary: Option<String>,
    ) {
        let previous_summary = previous_summary.or_else(|| {
            matches!(event, DirectoryListerEvent::LoadingStarted { .. })
                .then(|| self.status_summary_for_pane(event.pane_id()))
                .flatten()
        });
        update_loading_state_for_event(
            &mut self.loading_panes,
            self.panes.pane(event.pane_id()),
            event,
            Instant::now(),
            previous_summary,
        );
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
        let current_dir = pane.current_dir.clone();
        if let Err(err) = pane.lister.start_watcher() {
            self.set_pane_status(
                pane_id,
                format!("Cannot watch {}: {err}", current_dir.display()),
            );
        }
    }

    fn schedule_listing(&self, event: &DirectoryListerEvent) -> Option<Vec<DirectoryListerEvent>> {
        let request = ListingRequest::from_event(event)?;
        self.listing_worker.schedule_or_cached(request)
    }

    fn schedule_listings<'a>(&self, events: impl IntoIterator<Item = &'a DirectoryListerEvent>) {
        self.listing_worker
            .schedule_all(listing_requests_from_events(events));
    }

    fn apply_cached_listing_events(&mut self, events: Option<Vec<DirectoryListerEvent>>) {
        for event in events.unwrap_or_default() {
            self.apply_event(event);
        }
    }

    fn drain_background_listing_results(&mut self) -> bool {
        let mut changed = false;
        for events in self.listing_worker.drain_results() {
            for event in events {
                self.apply_event(event);
                changed = true;
            }
        }
        changed
    }

    fn drain_watchers(&mut self) -> bool {
        let mut changed = false;
        let pane_ids = self.panes.pane_ids().to_vec();
        let mut events = Vec::new();
        for pane_id in pane_ids {
            events.extend(
                self.panes
                    .pane_mut(pane_id)
                    .map(|pane| pane.lister.drain_watcher_events())
                    .unwrap_or_default(),
            );
        }
        self.schedule_listings(events.iter());
        for event in events {
            self.apply_event(event);
            changed = true;
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
        let viewport_size = window.viewport_size();
        let places = self.place_snapshots();
        let snapshots = self.snapshots(cx);
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
        let pane_ids = snapshots
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>();
        let mouse_overlay_active = self.context_menu.is_some()
            || self.properties_dialog.is_some()
            || self.application_chooser.is_some()
            || self.place_draft.is_some();
        let mut pane_elements = Vec::with_capacity(pane_ids.len().saturating_mul(2));
        for (index, snapshot) in snapshots.into_iter().enumerate() {
            let left = snapshot.id;
            pane_elements.push(ui::pane::pane_view(
                ui::pane::PaneProps {
                    snapshot,
                    file_grid_mode,
                    mouse_overlay_active,
                },
                cx,
            ));
            if let Some(right) = pane_ids.get(index + 1).copied() {
                pane_elements.push(pane_splitter(left, right, cx));
            }
        }
        let context_menu = self.context_menu.clone();
        let properties_dialog = self.properties_dialog.clone();
        let application_chooser = self.application_chooser.clone();
        let place_draft = self.place_draft.clone();
        let clipboard_available = self.clipboard.is_some();
        let context_menu_icons = context_menu
            .as_ref()
            .map(|menu| {
                context_menu_icon_snapshots(&mut self.file_icons, menu, clipboard_available)
            })
            .unwrap_or_default();
        let app = cx.weak_entity();
        div()
            .relative()
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
                        bar.child(ui::controls::toolbar_button("choose", "Choose").on_click(
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
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(ui::places::places_sidebar(places, cx))
                    .child(
                        div()
                            .flex()
                            .p_2()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .max_w_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .on_children_prepainted(move |bounds, _window, cx| {
                                        let Some(width) = pane_row_width_from_child_bounds(&bounds)
                                        else {
                                            return;
                                        };
                                        let _ = app.update(cx, |this, cx| {
                                            if this.set_pane_row_width(width) {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .id("pane-row")
                                    .flex()
                                    .flex_row()
                                    .size_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .on_drag_move::<PaneSplitterDrag>(cx.listener(
                                        move |this,
                                              event: &gpui::DragMoveEvent<PaneSplitterDrag>,
                                              _window,
                                              cx| {
                                            let drag = *event.drag(cx);
                                            if this.resize_pane_pair_from_row_drag(
                                                drag.left,
                                                drag.right,
                                                event.event.position.x.as_f32(),
                                                event.bounds.origin.x.as_f32(),
                                                event.bounds.size.width.as_f32(),
                                            ) {
                                                cx.notify();
                                            }
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .on_drop::<PaneSplitterDrag>(cx.listener(
                                        |_this, _drag: &PaneSplitterDrag, _window, cx| {
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .children(pane_elements),
                            ),
                    ),
            )
            .when_some(context_menu, |root, menu| {
                root.child(context_menu_overlay(
                    menu,
                    clipboard_available,
                    context_menu_icons,
                    viewport_size.width.as_f32(),
                    viewport_size.height.as_f32(),
                    cx,
                ))
            })
            .when_some(properties_dialog, |root, dialog| {
                root.child(properties_dialog_overlay(dialog, cx))
            })
            .when_some(application_chooser, |root, chooser| {
                root.child(application_chooser_overlay(chooser, cx))
            })
            .when_some(place_draft, |root, draft| {
                root.child(place_draft_overlay(draft, cx))
            })
    }
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
    use fika_core::ServiceMenuPriority;

    #[test]
    fn active_place_prefers_longest_path_prefix() {
        let places = vec![
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: PathBuf::from("/home/yk"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "Down",
                label: "Downloads".to_string(),
                path: PathBuf::from("/home/yk/Downloads"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        assert_eq!(
            active_place_index(&places, Path::new("/home/yk/Downloads/archive")),
            Some(2)
        );
    }

    #[test]
    fn context_menu_actions_track_blank_paste_availability() {
        let blank = context_blank_target();
        let without_clipboard = context_menu_actions(&blank, false);
        let with_clipboard = context_menu_actions(&blank, true);

        assert_eq!(
            without_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(false)
        );
        assert_eq!(
            with_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            with_clipboard
                .iter()
                .any(|item| item.action == ContextMenuAction::Properties)
        );
    }

    #[test]
    fn context_menu_actions_offer_blank_sort_and_view_submenus() {
        let blank = context_blank_target();
        let actions = context_menu_actions(&blank, false);

        assert_eq!(
            actions
                .iter()
                .find(|item| item.action == ContextMenuAction::SortBySubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::SortBy)
        );
        assert_eq!(
            actions
                .iter()
                .find(|item| item.action == ContextMenuAction::ViewModeSubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::ViewMode)
        );
    }

    #[test]
    fn context_menu_actions_offer_blank_directory_service_actions() {
        let mut blank = context_blank_target();
        if let ContextMenuTarget::Blank {
            service_actions, ..
        } = &mut blank
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:terminal.desktop::open-here".to_string(),
                label: "Open Terminal Here".to_string(),
                source_name: "Terminal".to_string(),
                icon: Some("utilities-terminal".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&blank, false);

        assert!(actions.iter().any(|item| {
            item.action
                == (ContextMenuAction::RunServiceMenuAction {
                    action_id: "service-menu:terminal.desktop::open-here".to_string(),
                })
        }));
    }

    #[test]
    fn blank_context_menu_requires_viewport_geometry_and_keeps_directory_service_actions() {
        let mut app = test_app_with_entries("/tmp/fika-blank-menu-service", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.mime_applications = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
            Vec::new(),
            vec![fika_core::DesktopServiceMenu {
                id: "terminal.desktop".to_string(),
                desktop_file: PathBuf::from("/menus/terminal.desktop"),
                name: "Terminal".to_string(),
                icon: Some("utilities-terminal".to_string()),
                mime_types: vec!["inode/directory".to_string()],
                service_types: vec!["KonqPopupMenu/Plugin".to_string()],
                protocols: Vec::new(),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
                required_url_count: None,
                min_url_count: None,
                max_url_count: None,
                show_if_executable: None,
                actions: vec![fika_core::DesktopAction {
                    id: "open-here".to_string(),
                    name: "Open Terminal Here".to_string(),
                    exec: "konsole --workdir %f".to_string(),
                    icon: Some("utilities-terminal".to_string()),
                }],
            }],
            &[],
        );

        assert!(!app.show_blank_context_menu_if_blank(pane_id, gpui::point(px(500.0), px(300.0))));
        assert!(app.context_menu.is_none());
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            }
        ));
        assert!(app.show_blank_context_menu_if_blank(pane_id, gpui::point(px(500.0), px(300.0))));

        let Some(ContextMenuState {
            target: ContextMenuTarget::Blank {
                service_actions, ..
            },
            ..
        }) = app.context_menu
        else {
            panic!("expected blank context menu");
        };
        assert!(service_actions.iter().any(|action| {
            action.id == "service-menu:terminal.desktop::open-here"
                && action.icon.as_deref() == Some("utilities-terminal")
        }));
    }

    #[test]
    fn context_menu_actions_do_not_add_builtin_terminal_entries() {
        let blank = context_blank_target();
        assert!(
            !context_menu_actions(&blank, false)
                .iter()
                .any(|item| item.label == "Open Terminal Here")
        );

        let dir_target = context_item_target("/tmp", true, 1);
        assert!(
            !context_menu_actions(&dir_target, false)
                .iter()
                .any(|item| item.label == "Open Terminal Here")
        );
    }

    #[test]
    fn context_menu_actions_group_blank_menu_like_dolphin() {
        let blank = context_blank_target();
        let separators = context_menu_actions(&blank, true)
            .into_iter()
            .map(|item| (item.action, item.separator_before))
            .collect::<Vec<_>>();

        assert_eq!(
            separators,
            vec![
                (ContextMenuAction::CreateNewSubmenu, false),
                (ContextMenuAction::Paste, true),
                (ContextMenuAction::SortBySubmenu, true),
                (ContextMenuAction::ViewModeSubmenu, false),
                (ContextMenuAction::SelectAll, true),
                (ContextMenuAction::Refresh, false),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_offer_create_new_submenu_for_blank_and_directories() {
        let blank = context_blank_target();
        let blank_actions = context_menu_actions(&blank, false);
        assert_eq!(
            blank_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::CreateNewSubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::CreateNew)
        );

        let dir_target = context_item_target("/tmp/project", true, 1);
        let dir_actions = context_menu_actions(&dir_target, false);
        assert_eq!(
            dir_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::CreateNewSubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::CreateNew)
        );

        let file_target = context_item_target("/tmp/readme.txt", false, 1);
        assert!(
            !context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::CreateNewSubmenu)
        );
    }

    #[test]
    fn context_submenu_actions_offer_dolphin_create_new_entries() {
        let blank = context_blank_target();
        let actions = context_submenu_actions(ContextMenuSubmenu::CreateNew, &blank)
            .into_iter()
            .map(|item| (item.action, item.label, item.icon))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (
                    ContextMenuAction::CreateFolder,
                    "Folder".to_string(),
                    Some(ContextMenuIcon::NewFolder),
                ),
                (
                    ContextMenuAction::CreateFile,
                    "Text File".to_string(),
                    Some(ContextMenuIcon::NewFile),
                ),
            ]
        );
    }

    #[test]
    fn context_submenu_actions_enable_sort_but_keep_unimplemented_view_modes_disabled() {
        let target = context_blank_target();
        let sort_actions = context_submenu_actions(ContextMenuSubmenu::SortBy, &target)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            sort_actions,
            vec![
                (ContextMenuAction::SortByName, true),
                (ContextMenuAction::SortByModified, true),
                (ContextMenuAction::SortBySize, true),
                (ContextMenuAction::SortAscending, true),
                (ContextMenuAction::SortDescending, true),
                (ContextMenuAction::SortFoldersFirst, true),
                (ContextMenuAction::SortHiddenLast, true),
            ]
        );

        let trash_sort_actions = context_submenu_actions(ContextMenuSubmenu::TrashSortBy, &target)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            trash_sort_actions,
            vec![
                (ContextMenuAction::SortByName, true),
                (ContextMenuAction::SortByOriginalPath, true),
                (ContextMenuAction::SortByDeletionTime, true),
                (ContextMenuAction::SortAscending, true),
                (ContextMenuAction::SortDescending, true),
                (ContextMenuAction::SortFoldersFirst, true),
                (ContextMenuAction::SortHiddenLast, true),
            ]
        );

        let view_actions = context_submenu_actions(ContextMenuSubmenu::ViewMode, &target)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            view_actions,
            vec![
                (ContextMenuAction::ViewCompact, true),
                (ContextMenuAction::ViewIcons, false),
                (ContextMenuAction::ViewDetails, false),
            ]
        );
    }

    #[test]
    fn context_menu_layout_clamps_root_inside_viewport() {
        let layout = context_menu_overlay_layout(
            ViewPoint { x: 295.0, y: 190.0 },
            8,
            None,
            0,
            0,
            320.0,
            220.0,
        );

        assert_eq!(layout.root.width, 196.0);
        assert_eq!(layout.root.max_height, 204.0);
        assert_eq!(layout.root.x, 99.0);
        assert_eq!(layout.root.y, 8.0);
        assert!(layout.submenu.is_none());
    }

    #[test]
    fn context_menu_layout_flips_root_around_mouse_when_space_exists() {
        let horizontal = context_menu_overlay_layout(
            ViewPoint { x: 280.0, y: 24.0 },
            2,
            None,
            0,
            0,
            420.0,
            240.0,
        );
        assert_eq!(horizontal.root.x, 84.0);
        assert_eq!(horizontal.root.y, 24.0);

        let vertical = context_menu_overlay_layout(
            ViewPoint { x: 24.0, y: 220.0 },
            4,
            None,
            0,
            0,
            420.0,
            260.0,
        );
        assert_eq!(vertical.root.x, 24.0);
        assert_eq!(vertical.root.y, 100.0);
    }

    #[test]
    fn context_menu_layout_shrinks_for_narrow_viewports() {
        let layout =
            context_menu_overlay_layout(ViewPoint { x: 0.0, y: 0.0 }, 2, None, 0, 0, 80.0, 100.0);

        assert_eq!(layout.root.width, 64.0);
        assert_eq!(layout.root.x, 8.0);
        assert_eq!(layout.root.max_height, 64.0);
    }

    #[test]
    fn context_menu_layout_flips_submenu_left_at_right_edge() {
        let layout = context_menu_overlay_layout(
            ViewPoint { x: 170.0, y: 20.0 },
            7,
            Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::SortBy,
                parent_index: 2,
                nested: None,
            }),
            7,
            0,
            420.0,
            400.0,
        );

        let submenu = layout.submenu.unwrap();
        assert!(submenu.x < layout.root.x);
        assert_eq!(submenu.x, 8.0);
        assert_eq!(
            submenu.y,
            layout.root.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0 * CONTEXT_MENU_ROW_HEIGHT
        );
        assert!(submenu.x + submenu.width <= 420.0 - CONTEXT_MENU_VIEWPORT_MARGIN);
    }

    #[test]
    fn context_menu_layout_cascades_nested_submenu_from_first_submenu() {
        let layout = context_menu_overlay_layout(
            ViewPoint { x: 20.0, y: 20.0 },
            5,
            Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::ServiceMenu,
                parent_index: 1,
                nested: Some(ContextMenuNestedSubmenu {
                    submenu: ContextMenuSubmenu::ServiceMenuGroup(0),
                    parent_index: 2,
                }),
            }),
            4,
            3,
            720.0,
            420.0,
        );

        let submenu = layout.submenu.unwrap();
        let nested = layout.nested_submenu.unwrap();
        assert!(submenu.x > layout.root.x);
        assert!(nested.x > submenu.x);
        assert_eq!(
            nested.y,
            submenu.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0 * CONTEXT_MENU_ROW_HEIGHT
        );
        assert!(nested.x + nested.width <= 720.0 - CONTEXT_MENU_VIEWPORT_MARGIN);
    }

    #[test]
    fn context_submenu_generation_ignores_stale_hide_requests() {
        let mut app = test_app_with_entries("/tmp/fika-context-submenu-stale", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.context_menu = Some(ContextMenuState {
            pane_id,
            target: context_blank_target(),
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: None,
        });

        app.open_context_submenu(ContextMenuSubmenu::SortBy, 2);
        let stale_generation = app.context_submenu_hide_generation;
        app.open_context_submenu(ContextMenuSubmenu::ViewMode, 3);

        assert!(!app.clear_context_submenu_if_generation(stale_generation));
        assert_eq!(
            app.context_menu
                .as_ref()
                .and_then(|menu| menu.active_submenu),
            Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::ViewMode,
                parent_index: 3,
                nested: None,
            })
        );
    }

    #[test]
    fn context_submenu_generation_clears_only_current_pending_hide() {
        let mut app = test_app_with_entries("/tmp/fika-context-submenu-current", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.context_menu = Some(ContextMenuState {
            pane_id,
            target: context_blank_target(),
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: None,
        });

        app.open_context_submenu(ContextMenuSubmenu::SortBy, 2);
        app.cancel_context_submenu_hide();
        let generation = app.context_submenu_hide_generation;

        assert!(app.clear_context_submenu_if_generation(generation));
        assert_eq!(
            app.context_menu
                .as_ref()
                .and_then(|menu| menu.active_submenu),
            None
        );
        assert!(app.context_submenu_hide_generation > generation);
    }

    #[test]
    fn places_blank_context_menu_offers_add_and_show_hidden_places() {
        let target = ContextMenuTarget::PlacesBlank {
            has_hidden_places: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::AddPlace, true),
                (ContextMenuAction::ShowHiddenPlaces, false),
            ]
        );

        let target = ContextMenuTarget::PlacesBlank {
            has_hidden_places: true,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::AddPlace, true),
                (ContextMenuAction::ShowHiddenPlaces, true),
            ]
        );
    }

    #[test]
    fn places_section_context_menu_offers_hide_section() {
        let target = ContextMenuTarget::PlaceSection { group: "Devices" };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(actions, vec![(ContextMenuAction::HidePlaceSection, true)]);
    }

    #[test]
    fn places_user_bookmark_context_menu_enables_edit_and_remove() {
        let target = ContextMenuTarget::Place {
            path: PathBuf::from("/tmp/fika-user-place"),
            mounted: true,
            device: false,
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
            device_ejectable: false,
            device_can_power_off: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::OpenInNewWindow, true),
                (ContextMenuAction::EditPlace, true),
                (ContextMenuAction::RemovePlace, true),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn unmounted_device_place_context_menu_disables_open_actions() {
        let target = ContextMenuTarget::Place {
            path: PathBuf::from("/dev/sdz1"),
            mounted: false,
            device: true,
            trash_place: false,
            trash_has_items: false,
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, false),
                (ContextMenuAction::OpenInNewPane, false),
                (ContextMenuAction::OpenInNewWindow, false),
                (ContextMenuAction::MountDevice, true),
                (ContextMenuAction::EditPlace, false),
                (ContextMenuAction::RemovePlace, false),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn mounted_device_place_context_menu_offers_unmount_and_eject() {
        let target = ContextMenuTarget::Place {
            path: PathBuf::from("/run/media/yk/USB"),
            mounted: true,
            device: true,
            trash_place: false,
            trash_has_items: false,
            editable: false,
            removable: false,
            device_ejectable: true,
            device_can_power_off: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::OpenInNewWindow, true),
                (ContextMenuAction::UnmountDevice, true),
                (ContextMenuAction::EjectDevice, true),
                (ContextMenuAction::EditPlace, false),
                (ContextMenuAction::RemovePlace, false),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn device_place_context_menu_offers_safely_remove_when_power_off_supported() {
        let target = ContextMenuTarget::Place {
            path: PathBuf::from("/run/media/yk/USB"),
            mounted: true,
            device: true,
            trash_place: false,
            trash_has_items: false,
            editable: false,
            removable: false,
            device_ejectable: true,
            device_can_power_off: true,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::OpenInNewWindow, true),
                (ContextMenuAction::UnmountDevice, true),
                (ContextMenuAction::EjectDevice, true),
                (ContextMenuAction::SafelyRemoveDevice, true),
                (ContextMenuAction::EditPlace, false),
                (ContextMenuAction::RemovePlace, false),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_offer_directory_only_open_helpers() {
        let dir_target = context_item_target("/tmp", true, 1);
        let file_target = context_item_target("/tmp/readme.txt", false, 1);

        assert!(
            context_menu_actions(&dir_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewPane)
        );
        assert!(
            context_menu_actions(&dir_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewWindow)
        );
        assert!(
            !context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewPane)
        );
        assert!(
            !context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewWindow)
        );
        assert!(
            !context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.label == "Open Terminal Here")
        );
        assert!(
            context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::CopyLocation)
        );
    }

    #[test]
    fn context_menu_actions_offer_open_with_submenu_for_single_files() {
        let mut file_target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            mime_type,
            open_with_apps,
            ..
        } = &mut file_target
        {
            *mime_type = Some(Arc::from("text/plain"));
            open_with_apps.push(MimeApplication {
                id: "viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "viewer %f".to_string(),
                icon: Some("accessories-text-editor".to_string()),
                is_default: true,
            });
            open_with_apps.push(MimeApplication {
                id: "viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "viewer %f".to_string(),
                icon: Some("accessories-text-editor".to_string()),
                is_default: false,
            });
            open_with_apps.push(MimeApplication {
                id: "other-viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/other-viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "other-viewer %f".to_string(),
                icon: None,
                is_default: false,
            });
        }

        let actions = context_menu_actions(&file_target, false);
        assert_eq!(
            actions.first().map(|item| (&item.action, item.submenu)),
            Some((
                &ContextMenuAction::OpenWithSubmenu,
                Some(ContextMenuSubmenu::OpenWith)
            ))
        );

        let submenu = context_submenu_actions(ContextMenuSubmenu::OpenWith, &file_target);
        assert_eq!(
            submenu.first().map(|item| &item.action),
            Some(&ContextMenuAction::OpenWithApplication {
                desktop_id: "viewer.desktop".to_string()
            })
        );
        assert_eq!(
            submenu.first().map(|item| item.label.as_str()),
            Some("Viewer")
        );
        assert_eq!(
            submenu.first().and_then(|item| item.icon.as_ref()),
            Some(&ContextMenuIcon::Named(
                "accessories-text-editor".to_string()
            ))
        );
        assert_eq!(
            submenu
                .iter()
                .filter(|item| matches!(item.action, ContextMenuAction::OpenWithApplication { .. }))
                .count(),
            1
        );
        assert!(
            submenu
                .iter()
                .any(|item| item.action == ContextMenuAction::OtherApplication && item.enabled)
        );
    }

    #[test]
    fn context_menu_icon_snapshots_cover_root_and_open_with_submenu() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item { open_with_apps, .. } = &mut target {
            open_with_apps.push(MimeApplication {
                id: "viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "viewer %f".to_string(),
                icon: Some("accessories-text-editor".to_string()),
                is_default: true,
            });
        }
        let menu = ContextMenuState {
            pane_id: PaneId(1),
            target,
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::OpenWith,
                parent_index: 0,
                nested: None,
            }),
        };
        let mut cache = FileIconCache::default();

        let snapshots = context_menu_icon_snapshots(&mut cache, &menu, false);

        assert!(snapshots.contains_key(&ContextMenuIcon::OpenWith));
        assert!(snapshots.contains_key(&ContextMenuIcon::Named(
            "accessories-text-editor".to_string()
        )));
        assert!(snapshots.contains_key(&ContextMenuIcon::Cut));
    }

    #[test]
    fn context_menu_icon_snapshots_include_dolphin_window_and_create_new_icons() {
        let menu = ContextMenuState {
            pane_id: PaneId(1),
            target: context_item_target("/tmp/project", true, 1),
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::CreateNew,
                parent_index: 3,
                nested: None,
            }),
        };
        let mut cache = FileIconCache::default();

        let snapshots = context_menu_icon_snapshots(&mut cache, &menu, false);

        assert!(snapshots.contains_key(&ContextMenuIcon::NewWindow));
        assert!(snapshots.contains_key(&ContextMenuIcon::CreateNew));
        assert!(snapshots.contains_key(&ContextMenuIcon::NewFolder));
        assert!(snapshots.contains_key(&ContextMenuIcon::NewFile));
    }

    #[test]
    fn context_menu_actions_promote_small_service_menu_action_sets() {
        let mut file_target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut file_target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:archive.desktop::compress".to_string(),
                label: "Compress".to_string(),
                source_name: "Archive Tools".to_string(),
                icon: Some("archive-insert".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&file_target, false);
        assert!(actions.iter().any(|item| {
            item.action
                == (ContextMenuAction::RunServiceMenuAction {
                    action_id: "service-menu:archive.desktop::compress".to_string(),
                })
                && item.label == "Compress"
                && item.icon == Some(ContextMenuIcon::Named("archive-insert".to_string()))
        }));
        assert!(
            !actions
                .iter()
                .any(|item| item.action == ContextMenuAction::ServiceMenuSubmenu)
        );
        assert!(
            actions
                .iter()
                .find(|item| matches!(item.action, ContextMenuAction::RunServiceMenuAction { .. }))
                .is_some_and(|item| item.separator_before)
        );
    }

    #[test]
    fn context_menu_icon_snapshots_include_service_menu_named_icons() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:archive.desktop::compress".to_string(),
                label: "Compress".to_string(),
                source_name: "Archive Tools".to_string(),
                icon: Some("archive-insert".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }
        let menu = ContextMenuState {
            pane_id: PaneId(1),
            target,
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: None,
        };
        let mut cache = FileIconCache::default();

        let snapshots = context_menu_icon_snapshots(&mut cache, &menu, false);

        assert!(snapshots.contains_key(&ContextMenuIcon::Named("archive-insert".to_string())));
    }

    #[test]
    fn context_menu_actions_keep_kde_submenu_actions_nested_even_when_small() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:tools.desktop::checksum".to_string(),
                label: "Checksum".to_string(),
                source_name: "Tools".to_string(),
                icon: None,
                submenu: Some("Tools".to_string()),
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&target, false);

        assert!(
            actions
                .iter()
                .any(|item| item.action == ContextMenuAction::ServiceMenuSubmenu)
        );
        assert!(
            !actions
                .iter()
                .any(|item| matches!(item.action, ContextMenuAction::RunServiceMenuAction { .. }))
        );
        let more_actions = context_submenu_actions(ContextMenuSubmenu::ServiceMenu, &target);
        assert_eq!(
            more_actions
                .iter()
                .map(|item| (item.action.clone(), item.submenu, item.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(
                ContextMenuAction::ServiceMenuGroupSubmenu { group_index: 0 },
                Some(ContextMenuSubmenu::ServiceMenuGroup(0)),
                "Tools"
            )]
        );
        let tools = context_submenu_actions(ContextMenuSubmenu::ServiceMenuGroup(0), &target);
        assert_eq!(
            tools.first().map(|item| item.label.as_str()),
            Some("Checksum")
        );
    }

    #[test]
    fn context_menu_actions_promote_service_menu_action_for_multi_selection() {
        let mut target = context_item_target("/tmp/readme.txt", false, 3);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:archive.desktop::compress".to_string(),
                label: "Compress".to_string(),
                source_name: "Archive Tools".to_string(),
                icon: None,
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.submenu))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Cut, None),
                (ContextMenuAction::Copy, None),
                (
                    ContextMenuAction::RunServiceMenuAction {
                        action_id: "service-menu:archive.desktop::compress".to_string(),
                    },
                    None
                ),
                (ContextMenuAction::Trash, None),
                (ContextMenuAction::Properties, None),
            ]
        );
    }

    #[test]
    fn context_menu_actions_offer_compress_fallback_when_service_menu_missing() {
        let file_target = context_item_target("/tmp/readme.txt", false, 1);
        let dir_target = context_item_target("/tmp/project", true, 1);
        let multi_target = context_item_target("/tmp/readme.txt", false, 3);

        for target in [file_target, dir_target, multi_target] {
            let actions = context_menu_actions(&target, false);
            assert!(actions.iter().any(|item| {
                item.action == ContextMenuAction::CompressWithArk
                    && item.label == "Compress..."
                    && item.icon == Some(ContextMenuIcon::Archive)
            }));
        }
    }

    #[test]
    fn context_menu_actions_hide_compress_fallback_when_service_menu_matches() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:ark.desktop::compress".to_string(),
                label: "Compress".to_string(),
                source_name: "Ark".to_string(),
                icon: Some("ark".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&target, false);
        assert!(
            !actions
                .iter()
                .any(|item| item.action == ContextMenuAction::CompressWithArk)
        );
        assert!(
            actions
                .iter()
                .any(|item| matches!(item.action, ContextMenuAction::RunServiceMenuAction { .. }))
        );
    }

    #[test]
    fn context_menu_actions_keep_compress_fallback_when_only_extract_service_exists() {
        let mut target = context_item_target("/tmp/archive.zip", false, 3);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            service_actions.push(ServiceMenuAction {
                id: "service-menu:ark.desktop::extract-here".to_string(),
                label: "Extract Here".to_string(),
                source_name: "Ark".to_string(),
                icon: Some("ark".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&target, false);
        assert!(
            actions
                .iter()
                .any(|item| item.action == ContextMenuAction::CompressWithArk)
        );
    }

    #[test]
    fn context_menu_actions_do_not_offer_compress_fallback_for_single_archive_file() {
        let mut target = context_item_target("/tmp/archive.zip", false, 1);
        if let ContextMenuTarget::Item { mime_type, .. } = &mut target {
            *mime_type = Some(Arc::from("application/zip"));
        }

        assert!(
            !context_menu_actions(&target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::CompressWithArk)
        );
    }

    #[test]
    fn context_menu_actions_offer_extract_fallback_for_single_archive_file() {
        let mut target = context_item_target("/tmp/archive.zip", false, 1);
        if let ContextMenuTarget::Item { mime_type, .. } = &mut target {
            *mime_type = Some(Arc::from("application/zip"));
        }

        let actions = context_menu_actions(&target, false);
        assert!(actions.iter().any(|item| {
            item.action == ContextMenuAction::ExtractHereWithArk
                && item.label == "Extract Here"
                && item.icon == Some(ContextMenuIcon::Archive)
        }));
        assert!(actions.iter().any(|item| {
            item.action == ContextMenuAction::ExtractToWithArk
                && item.label == "Extract To..."
                && item.icon == Some(ContextMenuIcon::Archive)
        }));
    }

    #[test]
    fn context_menu_actions_hide_extract_fallback_when_service_menu_matches() {
        let mut target = context_item_target("/tmp/archive.zip", false, 1);
        if let ContextMenuTarget::Item {
            mime_type,
            service_actions,
            ..
        } = &mut target
        {
            *mime_type = Some(Arc::from("application/zip"));
            service_actions.push(ServiceMenuAction {
                id: "service-menu:ark.desktop::extract-here".to_string(),
                label: "Extract Here".to_string(),
                source_name: "Ark".to_string(),
                icon: Some("ark".to_string()),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }

        let actions = context_menu_actions(&target, false);
        assert!(
            !actions
                .iter()
                .any(|item| item.action == ContextMenuAction::ExtractHereWithArk)
        );
        assert!(
            !actions
                .iter()
                .any(|item| item.action == ContextMenuAction::ExtractToWithArk)
        );
        assert!(
            actions
                .iter()
                .any(|item| matches!(item.action, ContextMenuAction::RunServiceMenuAction { .. }))
        );
    }

    #[test]
    fn context_menu_actions_do_not_offer_extract_fallback_for_normal_or_multi_file() {
        let normal_target = context_item_target("/tmp/readme.txt", false, 1);
        let mut multi_archive_target = context_item_target("/tmp/archive.zip", false, 2);
        if let ContextMenuTarget::Item { mime_type, .. } = &mut multi_archive_target {
            *mime_type = Some(Arc::from("application/zip"));
        }

        for target in [normal_target, multi_archive_target] {
            let actions = context_menu_actions(&target, false);
            assert!(
                !actions
                    .iter()
                    .any(|item| item.action == ContextMenuAction::ExtractHereWithArk)
            );
            assert!(
                !actions
                    .iter()
                    .any(|item| item.action == ContextMenuAction::ExtractToWithArk)
            );
        }
    }

    #[test]
    fn context_menu_actions_group_single_item_menu_like_dolphin() {
        let dir_target = context_item_target("/tmp", true, 1);
        let actions = context_menu_actions(&dir_target, true)
            .into_iter()
            .map(|item| (item.action, item.separator_before))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, false),
                (ContextMenuAction::OpenInNewPane, false),
                (ContextMenuAction::OpenInNewWindow, false),
                (ContextMenuAction::CreateNewSubmenu, false),
                (ContextMenuAction::Cut, true),
                (ContextMenuAction::Copy, false),
                (ContextMenuAction::CopyLocation, false),
                (ContextMenuAction::Paste, false),
                (ContextMenuAction::CompressWithArk, true),
                (ContextMenuAction::Rename, true),
                (ContextMenuAction::Trash, false),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_keep_excess_service_actions_in_more_actions() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            for label in ["Compress", "Extract", "Terminal", "Checksum", "Encrypt"] {
                service_actions.push(ServiceMenuAction {
                    id: format!("service-menu:tools.desktop::{label}"),
                    label: label.to_string(),
                    source_name: "Tools".to_string(),
                    icon: None,
                    submenu: None,
                    priority: ServiceMenuPriority::Normal,
                });
            }
        }

        let actions = context_menu_actions(&target, false);
        assert!(
            actions
                .iter()
                .any(|item| item.action == ContextMenuAction::ServiceMenuSubmenu
                    && item.label == "More Actions")
        );
        let submenu = context_submenu_actions(ContextMenuSubmenu::ServiceMenu, &target);
        assert!(
            submenu
                .iter()
                .any(|item| item.label == "Checksum" || item.label == "Encrypt")
        );
    }

    #[test]
    fn context_menu_service_menu_more_actions_nest_kde_submenus() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            for (label, submenu) in [
                ("Inspect", None),
                ("Checksum", Some("Tools")),
                ("Encrypt", Some("Tools")),
                ("Share", Some("Send To")),
                ("Convert", Some("Tools")),
            ] {
                service_actions.push(ServiceMenuAction {
                    id: format!("service-menu:tools.desktop::{label}"),
                    label: label.to_string(),
                    source_name: "Tools".to_string(),
                    icon: None,
                    submenu: submenu.map(str::to_string),
                    priority: ServiceMenuPriority::Normal,
                });
            }
        }

        let submenu = context_submenu_actions(ContextMenuSubmenu::ServiceMenu, &target);
        let labels = submenu
            .iter()
            .map(|item| {
                (
                    item.label.as_str(),
                    item.enabled,
                    item.separator_before,
                    item.submenu,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                ("Inspect", true, false, None),
                (
                    "Tools",
                    true,
                    true,
                    Some(ContextMenuSubmenu::ServiceMenuGroup(0)),
                ),
                (
                    "Send To",
                    true,
                    false,
                    Some(ContextMenuSubmenu::ServiceMenuGroup(1)),
                ),
            ]
        );
        assert!(matches!(
            submenu[1].action,
            ContextMenuAction::ServiceMenuGroupSubmenu { group_index: 0 }
        ));
        let tools_submenu =
            context_submenu_actions(ContextMenuSubmenu::ServiceMenuGroup(0), &target)
                .into_iter()
                .map(|item| item.label)
                .collect::<Vec<_>>();
        assert_eq!(tools_submenu, vec!["Checksum", "Encrypt", "Convert"]);
    }

    #[test]
    fn context_menu_actions_promote_top_level_service_menu_priority() {
        let mut target = context_item_target("/tmp/readme.txt", false, 1);
        if let ContextMenuTarget::Item {
            service_actions, ..
        } = &mut target
        {
            for label in ["Checksum", "Encrypt", "Share", "Inspect", "Convert"] {
                service_actions.push(ServiceMenuAction {
                    id: format!("service-menu:tools.desktop::{label}"),
                    label: label.to_string(),
                    source_name: "Tools".to_string(),
                    icon: None,
                    submenu: None,
                    priority: if label == "Checksum" {
                        ServiceMenuPriority::TopLevel
                    } else {
                        ServiceMenuPriority::Normal
                    },
                });
            }
        }

        let actions = context_menu_actions(&target, false);

        assert!(actions.iter().any(|item| {
            item.action
                == (ContextMenuAction::RunServiceMenuAction {
                    action_id: "service-menu:tools.desktop::Checksum".to_string(),
                })
                && item.label == "Checksum"
        }));
        assert!(
            context_submenu_actions(ContextMenuSubmenu::ServiceMenu, &target)
                .iter()
                .all(|item| item.label != "Checksum")
        );
    }

    #[test]
    fn context_menu_actions_offer_paste_only_for_single_directory_targets() {
        let dir_target = context_item_target("/tmp", true, 1);
        let file_target = context_item_target("/tmp/readme.txt", false, 1);

        assert_eq!(
            context_menu_actions(&dir_target, true)
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            !context_menu_actions(&file_target, true)
                .iter()
                .any(|item| item.action == ContextMenuAction::Paste)
        );
    }

    #[test]
    fn open_with_application_builds_systemd_launch_plan() {
        let mut app = test_app_with_entries("/tmp/fika-open-with", &["note.txt"]);
        app.mime_applications = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "viewer.desktop",
                "Viewer",
                "viewer %f",
                &["text/plain"],
            )],
            &[],
        );

        let plan = app
            .open_with_launch_plan("viewer.desktop", Path::new("/tmp/fika-open-with/note.txt"))
            .unwrap();

        assert_eq!(plan.app_name, "Viewer");
        assert_eq!(plan.commands.len(), 1);
        assert_eq!(plan.commands[0].program, "viewer");
        assert_eq!(plan.commands[0].args, vec!["/tmp/fika-open-with/note.txt"]);
    }

    #[test]
    fn open_in_new_window_builds_current_executable_systemd_launch_plan() {
        let app = test_app_with_entries("/tmp/fika-new-window", &[]);

        let plan = app
            .new_window_launch_plan(Path::new("/tmp/fika-new-window"))
            .unwrap();

        assert_eq!(plan.desktop_id, "fika-new-window");
        assert_eq!(plan.app_name, "Fika");
        assert_eq!(plan.commands.len(), 1);
        assert!(Path::new(&plan.commands[0].program).is_absolute());
        assert_eq!(plan.commands[0].args, vec!["/tmp/fika-new-window"]);
    }

    #[test]
    fn other_application_picker_lists_all_apps_and_reuses_systemd_launch_plan() {
        let mut app = test_app_with_entries("/tmp/fika-open-with-other", &["note.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.mime_applications = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                test_desktop_application("writer.desktop", "Writer", "writer %f", &[]),
            ],
            &[],
        );

        app.show_application_chooser(
            pane_id,
            PathBuf::from("/tmp/fika-open-with-other/note.txt"),
            Some(Arc::from("text/plain")),
        );

        let chooser = app.application_chooser.as_ref().unwrap();
        assert_eq!(chooser.pane_id, pane_id);
        assert_eq!(chooser.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(
            chooser
                .applications
                .iter()
                .map(|app| app.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Viewer", "Writer"]
        );

        let plan = app
            .open_with_launch_plan(
                "writer.desktop",
                Path::new("/tmp/fika-open-with-other/note.txt"),
            )
            .unwrap();
        assert_eq!(plan.app_name, "Writer");
        assert_eq!(plan.commands[0].program, "writer");
    }

    #[test]
    fn application_chooser_visible_icon_snapshots_use_desktop_icons_only_for_visible_rows() {
        let applications = vec![
            MimeApplication {
                id: "viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "viewer %f".to_string(),
                icon: Some("accessories-text-editor".to_string()),
                is_default: true,
            },
            MimeApplication {
                id: "writer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/writer.desktop"),
                name: "Writer".to_string(),
                exec: "writer %f".to_string(),
                icon: None,
                is_default: false,
            },
        ];
        let mut cache = FileIconCache::default();

        let snapshots = ui::application_chooser::application_chooser_visible_icon_snapshots(
            &mut cache,
            &applications,
            0..1,
        );

        assert_eq!(snapshots.len(), 1);
        let viewer = snapshots.get(&0).unwrap();
        assert_eq!(viewer.icon_name, "accessories-text-editor");
        assert_eq!(viewer.fallback_marker, "VI");
        assert!(!snapshots.contains_key(&1));

        let snapshots = ui::application_chooser::application_chooser_visible_icon_snapshots(
            &mut cache,
            &applications,
            1..2,
        );
        let writer = snapshots.get(&1).unwrap();
        assert_eq!(writer.icon_name, "application-x-executable");
        assert_eq!(writer.fallback_marker, "WR");
    }

    #[test]
    fn application_chooser_marks_current_default_application() {
        let mut app = test_app_with_entries("/tmp/fika-open-with-default", &["note.txt"]);
        let list = fika_core::parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=writer.desktop;\n",
        );
        app.mime_applications = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                test_desktop_application("writer.desktop", "Writer", "writer %f", &["text/plain"]),
            ],
            &[list],
        );

        let applications = app.application_chooser_applications(Some("text/plain"));

        assert_eq!(
            applications
                .iter()
                .map(|app| (app.name.as_str(), app.is_default))
                .collect::<Vec<_>>(),
            vec![("Viewer", false), ("Writer", true)]
        );
    }

    #[test]
    fn service_menu_action_builds_systemd_launch_plan() {
        let mut app = test_app_with_entries("/tmp/fika-service-menu", &["note.txt"]);
        app.mime_applications = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
            Vec::new(),
            vec![fika_core::DesktopServiceMenu {
                id: "archive.desktop".to_string(),
                desktop_file: PathBuf::from("/menus/archive.desktop"),
                name: "Archive Tools".to_string(),
                icon: None,
                mime_types: vec!["all/allfiles".to_string()],
                service_types: vec!["KonqPopupMenu/Plugin".to_string()],
                protocols: Vec::new(),
                submenu: None,
                priority: ServiceMenuPriority::Normal,
                required_url_count: None,
                min_url_count: None,
                max_url_count: None,
                show_if_executable: None,
                actions: vec![fika_core::DesktopAction {
                    id: "compress".to_string(),
                    name: "Compress".to_string(),
                    exec: "ark --add %F".to_string(),
                    icon: None,
                }],
            }],
            &[],
        );
        let action_id = app
            .mime_applications
            .service_actions_for_target(Some("text/plain"), false)
            .remove(0)
            .id;

        let plan = app
            .service_menu_launch_plan(
                &action_id,
                &[
                    PathBuf::from("/tmp/fika-service-menu/note.txt"),
                    PathBuf::from("/tmp/fika-service-menu/todo.txt"),
                ],
            )
            .unwrap();

        assert_eq!(plan.app_name, "Archive Tools: Compress");
        assert_eq!(plan.commands[0].program, "ark");
        assert_eq!(
            plan.commands[0].args,
            vec![
                "--add",
                "/tmp/fika-service-menu/note.txt",
                "/tmp/fika-service-menu/todo.txt"
            ]
        );
    }

    #[test]
    fn open_with_application_finish_reports_systemd_result_to_pane() {
        let mut app = test_app_with_entries("/tmp/fika-open-with-finish", &["note.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.begin_pane_operation(pane_id, "Opening");

        app.finish_open_with_application(OpenWithLaunchResult {
            pane_id,
            path: PathBuf::from("/tmp/fika-open-with-finish/note.txt"),
            app_name: "Viewer".to_string(),
            result: Ok(SystemdLaunchResult {
                units: vec!["fika-open-with-viewer-0.service".to_string()],
            }),
        });

        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Opened /tmp/fika-open-with-finish/note.txt with Viewer via 1 systemd unit(s)"
        );
        assert!(!app.operation_pending);
    }

    #[test]
    fn open_in_new_window_finish_reports_systemd_result_to_pane() {
        let mut app = test_app_with_entries("/tmp/fika-new-window-finish", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.begin_pane_operation(pane_id, "Opening new window");

        app.finish_open_in_new_window(NewWindowLaunchResult {
            pane_id,
            path: PathBuf::from("/tmp/fika-new-window-finish"),
            result: Ok(SystemdLaunchResult {
                units: vec!["fika-open-with-fika-new-window-0.service".to_string()],
            }),
        });

        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Opened /tmp/fika-new-window-finish in new window via 1 systemd unit(s)"
        );
        assert!(!app.operation_pending);
    }

    #[test]
    fn context_menu_actions_use_batch_actions_for_multi_selection() {
        let target = context_item_target("/tmp/readme.txt", false, 3);
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| item.action)
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                ContextMenuAction::Cut,
                ContextMenuAction::Copy,
                ContextMenuAction::CompressWithArk,
                ContextMenuAction::Trash,
                ContextMenuAction::Properties
            ]
        );
    }

    #[test]
    fn context_menu_actions_use_trash_view_actions() {
        let blank = ContextMenuTarget::Blank {
            trash_view: true,
            trash_has_items: false,
            service_actions: Vec::new(),
        };
        let blank_actions = context_menu_actions(&blank, false);
        assert_eq!(
            blank_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(false)
        );
        assert!(
            !blank_actions
                .iter()
                .any(|item| item.action == ContextMenuAction::CreateFolder)
        );
        assert_eq!(
            blank_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::SortBySubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::TrashSortBy)
        );

        let item = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp/fika-trash-item"),
            is_dir: false,
            selection_count: 2,
            trash_view: true,
            trash_can_restore: true,
            mime_type: None,
            open_with_apps: Vec::new(),
            service_actions: Vec::new(),
        };
        let item_actions = context_menu_actions(&item, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            item_actions,
            vec![
                (ContextMenuAction::RestoreFromTrash, true),
                (ContextMenuAction::Copy, true),
                (ContextMenuAction::DeletePermanently, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_use_place_actions_for_trash_place() {
        let empty_trash = context_place_target(file_ops::trash_files_dir(), true, false);
        let empty_actions = context_menu_actions(&empty_trash, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            empty_actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::OpenInNewWindow, true),
                (ContextMenuAction::EmptyTrash, false),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );

        let non_empty_trash = context_place_target(file_ops::trash_files_dir(), true, true);
        assert_eq!(
            context_menu_actions(&non_empty_trash, false)
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            !context_menu_actions(&non_empty_trash, true)
                .iter()
                .any(|item| matches!(
                    item.action,
                    ContextMenuAction::CreateFolder
                        | ContextMenuAction::Paste
                        | ContextMenuAction::Trash
                ))
        );
    }

    #[test]
    fn context_menu_actions_use_basic_actions_for_normal_places() {
        let home = context_place_target(PathBuf::from("/home/yk"), false, false);
        let actions = context_menu_actions(&home, true)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::OpenInNewWindow, true),
                (ContextMenuAction::EditPlace, false),
                (ContextMenuAction::RemovePlace, false),
                (ContextMenuAction::HidePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn build_places_loads_persistent_user_bookmarks_before_grouped_devices() {
        let root = test_dir("places-load");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("user-places.xbel");
        fika_core::save_user_places(
            &path,
            &[
                UserPlace::new("Bookmark".to_string(), bookmark.clone()),
                UserPlace::new("Duplicate Root".to_string(), PathBuf::from("/")),
            ],
        )
        .unwrap();

        let places = build_places(&path);
        let bookmark_index = places
            .iter()
            .position(|place| place.path == bookmark)
            .expect("persistent bookmark should be loaded");
        let root_index = places
            .iter()
            .position(|place| place.path == PathBuf::from("/"))
            .expect("root device place should exist");

        assert!(bookmark_index < root_index);
        assert_eq!(places[bookmark_index].label, "Bookmark");
        assert_eq!(places[bookmark_index].marker, "B");
        assert!(places[bookmark_index].editable);
        assert!(places[bookmark_index].removable);
        assert_eq!(places[root_index].label, "Root");
        assert!(!places[root_index].editable);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_places_adds_network_root_before_devices_without_persisting() {
        let root = test_dir("places-network-root");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("user-places.xbel");
        fika_core::save_user_places(
            &path,
            &[
                UserPlace::new("Bookmark".to_string(), bookmark.clone()),
                UserPlace::new("Duplicate Network".to_string(), network_root_path()),
            ],
        )
        .unwrap();

        let places = build_places(&path);
        let bookmark_index = places
            .iter()
            .position(|place| place.path == bookmark)
            .expect("persistent bookmark should be loaded");
        let network_index = places
            .iter()
            .position(|place| is_network_root_path(&place.path))
            .expect("network root should exist");
        let root_index = places
            .iter()
            .position(|place| place.path == PathBuf::from("/"))
            .expect("root device place should exist");
        let network = &places[network_index];

        assert!(bookmark_index < network_index);
        assert!(network_index < root_index);
        assert_eq!(network.group, NETWORK_GROUP);
        assert_eq!(network.marker, "Net");
        assert_eq!(network.label, fika_core::NETWORK_ROOT_LABEL);
        assert!(!network.editable);
        assert!(!network.removable);
        assert!(!place_is_mounted(network));
        assert_eq!(
            places
                .iter()
                .filter(|place| is_network_root_path(&place.path))
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn test_device(path: &str, label: &str, removable: bool) -> DeviceInfo {
        DeviceInfo {
            device_path: PathBuf::from(format!("/dev/{label}")),
            mount_point: Some(PathBuf::from(path)),
            filesystem_type: Some("exfat".to_string()),
            label: Some(label.to_string()),
            capacity_bytes: Some(1024),
            removable,
            ejectable: false,
            can_power_off: false,
        }
    }

    #[test]
    fn build_places_projects_removable_devices_before_root() {
        let root = test_dir("places-devices");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("user-places.xbel");
        fika_core::save_user_places(
            &path,
            &[UserPlace::new("Bookmark".to_string(), bookmark.clone())],
        )
        .unwrap();
        let devices = vec![
            test_device("/run/media/yk/Zed", "Zed", true),
            test_device("/run/media/yk/Alpha", "Alpha", true),
            test_device("/run/media/yk/Internal", "Internal", false),
            DeviceInfo {
                device_path: PathBuf::from("/dev/sdz1"),
                mount_point: None,
                filesystem_type: Some("exfat".to_string()),
                label: Some("Unmounted".to_string()),
                capacity_bytes: Some(1024),
                removable: true,
                ejectable: true,
                can_power_off: true,
            },
            test_device(bookmark.to_str().unwrap(), "Duplicate Bookmark", true),
        ];

        let places = build_places_with_devices(&path, &devices);
        let labels = places
            .iter()
            .map(|place| {
                (
                    place.group,
                    place.label.as_str(),
                    place.editable,
                    place.removable,
                )
            })
            .collect::<Vec<_>>();

        assert!(
            labels
                .iter()
                .any(|(group, label, editable, removable)| *group == ""
                    && *label == "Bookmark"
                    && *editable
                    && *removable)
        );
        assert_eq!(
            labels
                .iter()
                .filter(|(group, _, _, _)| *group == REMOVABLE_DEVICES_GROUP)
                .map(|(_, label, editable, removable)| (*label, *editable, *removable))
                .collect::<Vec<_>>(),
            vec![
                ("Alpha", false, false),
                ("Unmounted", false, false),
                ("Zed", false, false),
            ]
        );
        assert_eq!(
            places
                .iter()
                .find(|place| place.label == "Unmounted")
                .map(|place| {
                    (
                        place.path.clone(),
                        place_is_mounted(place),
                        place.device_ejectable,
                        place.device_can_power_off,
                    )
                }),
            Some((PathBuf::from("/dev/sdz1"), false, true, true))
        );
        let alpha_index = places
            .iter()
            .position(|place| place.label == "Alpha")
            .unwrap();
        let root_index = places
            .iter()
            .position(|place| place.path == PathBuf::from("/"))
            .unwrap();
        assert!(alpha_index < root_index);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replace_removable_device_places_updates_dynamic_section_without_persisting() {
        let current = test_dir("places-devices-current");
        let user = test_dir("places-devices-user");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: DEVICES_GROUP,
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.save_user_places().unwrap();

        assert!(app.replace_removable_device_places(&[
            test_device("/run/media/yk/USB", "USB", true),
            test_device("/run/media/yk/Backup", "Backup", true),
        ]));
        assert_eq!(
            app.places
                .iter()
                .map(|place| (place.group, place.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("", "Home"),
                ("", "User"),
                (REMOVABLE_DEVICES_GROUP, "Backup"),
                (REMOVABLE_DEVICES_GROUP, "USB"),
                (DEVICES_GROUP, "Root"),
            ]
        );
        assert!(!app.replace_removable_device_places(&[
            test_device("/run/media/yk/USB", "USB", true),
            test_device("/run/media/yk/Backup", "Backup", true),
        ]));
        assert!(app.replace_removable_device_places(&[test_device(
            "/run/media/yk/Camera",
            "Camera",
            true,
        )]));
        assert_eq!(
            app.places
                .iter()
                .filter(|place| place.group == REMOVABLE_DEVICES_GROUP)
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Camera"]
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user)])
        );

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn finish_device_refresh_updates_places_and_clears_pending_state() {
        let current = test_dir("places-device-refresh-current");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        app.places = vec![PlaceEntry {
            group: DEVICES_GROUP,
            marker: "/",
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        }];
        app.device_refresh_pending = true;

        let devices = vec![test_device("/run/media/yk/USB", "USB", true)];

        assert!(app.finish_device_refresh(devices.clone()));
        assert!(!app.device_refresh_pending);
        assert_eq!(
            app.places
                .iter()
                .map(|place| (place.group, place.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(REMOVABLE_DEVICES_GROUP, "USB"), (DEVICES_GROUP, "Root")]
        );

        app.device_refresh_pending = true;
        assert!(!app.finish_device_refresh(devices));
        assert!(!app.device_refresh_pending);

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn drain_device_monitor_messages_updates_dynamic_places() {
        let current = test_dir("places-device-monitor-current");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        app.places = vec![PlaceEntry {
            group: DEVICES_GROUP,
            marker: "/",
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        }];
        let (sender, receiver) = mpsc::channel();
        app.device_monitor_rx = Some(receiver);
        sender
            .send(DeviceMonitorMessage::Events {
                events: vec![fika_core::DeviceEvent::Added(test_device(
                    "/run/media/yk/USB",
                    "USB",
                    true,
                ))],
                devices: vec![test_device("/run/media/yk/USB", "USB", true)],
            })
            .unwrap();
        drop(sender);

        assert!(app.drain_device_monitor_messages());
        assert!(app.device_monitor_rx.is_none());
        assert_eq!(
            app.places
                .iter()
                .map(|place| (place.group, place.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(REMOVABLE_DEVICES_GROUP, "USB"), (DEVICES_GROUP, "Root")]
        );

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn hiding_place_section_filters_snapshots_without_persisting_or_deleting_places() {
        let current = test_dir("places-hide-current");
        let user = test_dir("places-hide-user");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.save_user_places().unwrap();

        app.hide_place_section(pane_id, "Devices");

        assert_eq!(app.places.len(), 3);
        assert!(app.hidden_place_sections.contains("Devices"));
        assert_eq!(
            app.place_snapshots()
                .into_iter()
                .map(|place| place.label)
                .collect::<Vec<_>>(),
            vec!["Home".to_string(), "User".to_string()]
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user.clone())])
        );
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Hidden places section Devices"
        );

        app.show_hidden_places(pane_id);

        assert!(app.hidden_place_sections.is_empty());
        assert_eq!(
            app.place_snapshots()
                .into_iter()
                .map(|place| place.label)
                .collect::<Vec<_>>(),
            vec!["Home".to_string(), "User".to_string(), "Root".to_string()]
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user)])
        );
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Showing hidden places"
        );
    }

    #[test]
    fn hiding_place_filters_snapshot_without_persisting_or_deleting_bookmark() {
        let current = test_dir("places-hide-place-current");
        let user = test_dir("places-hide-place-user");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.save_user_places().unwrap();

        app.hide_place(pane_id, user.clone());

        assert_eq!(app.places.len(), 2);
        assert!(app.hidden_places.contains(&user));
        assert_eq!(
            app.place_snapshots()
                .into_iter()
                .map(|place| place.label)
                .collect::<Vec<_>>(),
            vec!["Home".to_string()]
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user.clone())])
        );
        assert_eq!(app.status_message_for_pane(pane_id), "Hidden place User");

        app.show_hidden_places(pane_id);

        assert!(app.hidden_places.is_empty());
        assert_eq!(
            app.place_snapshots()
                .into_iter()
                .map(|place| place.label)
                .collect::<Vec<_>>(),
            vec!["Home".to_string(), "User".to_string()]
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user)])
        );
    }

    #[test]
    fn hiding_places_refuses_default_or_unknown_sections() {
        let current = test_dir("places-hide-refuse-current");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![PlaceEntry {
            group: "",
            marker: "H",
            label: "Home".to_string(),
            path: current,
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        }];

        app.hide_place_section(pane_id, "");
        assert!(app.hidden_place_sections.is_empty());
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place section cannot be hidden"
        );

        app.hide_place_section(pane_id, "Devices");
        assert!(app.hidden_place_sections.is_empty());
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place section cannot be hidden"
        );
    }

    #[test]
    fn add_place_inserts_user_bookmark_before_grouped_entries() {
        let current = test_dir("place-add-current");
        std::fs::create_dir_all(&current).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: home_dir(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        app.start_add_place(pane_id);
        assert_eq!(
            app.place_draft.as_ref().map(|draft| draft.path.as_str()),
            Some(current_arg.as_str())
        );
        app.commit_place_draft();

        assert_eq!(app.places.len(), 3);
        assert_eq!(app.places[1].path, current);
        assert_eq!(app.places[1].group, "");
        assert_eq!(app.places[1].marker, "B");
        assert_eq!(
            app.places[1].label,
            default_place_label(&app.places[1].path)
        );
        assert!(app.places[1].editable);
        assert!(app.places[1].removable);
        assert_eq!(app.places[2].group, "Devices");
        assert!(app.place_draft.is_none());
        assert!(
            app.status_message_for_pane(pane_id)
                .starts_with("Added place ")
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new(
                default_place_label(&current),
                current.clone()
            )])
        );

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn place_drop_target_tracks_row_insert_and_clears_pane_target() {
        let current = test_dir("place-drop-target-current");
        let user = test_dir("place-drop-target-user");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        assert!(app.set_place_drag_drop_target_for_path(user.clone(), FileTransferMode::Copy));
        assert!(app.item_drop_target.is_none());
        assert_eq!(
            place_drop_target_mode_for_place(app.place_drop_target.as_ref(), &user),
            Some(FileTransferMode::Copy)
        );
        assert!(
            app.place_snapshots()
                .into_iter()
                .find(|place| place.path == user)
                .is_some_and(|place| place.drop_target == Some(FileTransferMode::Copy))
        );

        assert!(app.set_place_drag_drop_target_for_insert(0));
        assert!(place_drop_target_matches_insert(
            app.place_drop_target.as_ref(),
            1
        ));
        assert!(
            app.place_snapshots()
                .into_iter()
                .find(|place| place.label == "User")
                .is_some_and(|place| place.insert_before)
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(user);
    }

    #[test]
    fn drop_target_stale_generation_clears_only_current_target() {
        let current = test_dir("drop-target-stale-current");
        let target = test_dir("drop-target-stale-target");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        let stale_generation = app.drop_target_stale_generation;
        assert!(app.set_place_drag_drop_target_for_path(target.clone(), FileTransferMode::Copy));
        assert!(!app.clear_stale_drop_targets_for_generation(stale_generation));
        assert_eq!(
            place_drop_target_mode_for_place(app.place_drop_target.as_ref(), &target),
            Some(FileTransferMode::Copy)
        );

        let current_generation = app.drop_target_stale_generation;
        assert!(app.clear_stale_drop_targets_for_generation(current_generation));
        assert!(app.item_drop_target.is_none());
        assert!(app.place_drop_target.is_none());

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(target);
    }

    #[test]
    fn repeated_drop_target_refresh_extends_stale_generation() {
        let current = test_dir("drop-target-refresh-current");
        std::fs::create_dir_all(&current).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        let first_generation = app.drop_target_stale_generation;
        assert!(!app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        assert!(app.drop_target_stale_generation > first_generation);
        assert!(!app.clear_stale_drop_targets_for_generation(first_generation));
        assert_eq!(
            item_drop_target_mode_for_pane(app.item_drop_target.as_ref(), pane_id),
            Some(FileTransferMode::Copy)
        );

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn place_insert_drop_adds_directory_bookmark_at_user_position() {
        let current = test_dir("place-drop-insert-current");
        let dropped = test_dir("place-drop-insert-dropped");
        let existing = test_dir("place-drop-insert-existing");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&dropped).unwrap();
        std::fs::create_dir_all(&existing).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Existing".to_string(),
                path: existing.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        let payload = ItemDragPayload {
            source_pane: pane_id,
            source_path: dropped.clone(),
            source_selected: false,
        };

        app.begin_item_drag(payload.clone());
        app.drop_item_drag_to_place_insert(payload, 0);

        assert_eq!(app.places[1].path, dropped);
        assert_eq!(
            app.places[1].label,
            default_place_label(&app.places[1].path)
        );
        assert!(app.places[1].editable);
        assert!(app.places[1].removable);
        assert!(app.active_item_drag.is_none());
        assert!(app.place_drop_target.is_none());
        assert!(
            app.status_message_for_pane(pane_id)
                .starts_with("Added place ")
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new(
                    default_place_label(&app.places[1].path),
                    app.places[1].path.clone()
                ),
                UserPlace::new("Existing".to_string(), existing.clone()),
            ])
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(app.places[1].path.clone());
        let _ = std::fs::remove_dir_all(existing);
    }

    #[test]
    fn place_insert_drop_rejects_non_folder_or_multiple_paths() {
        let current = test_dir("place-drop-reject-current");
        let folder = test_dir("place-drop-reject-folder");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        let file = current.join("note.txt");
        std::fs::write(&file, "note").unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        app.insert_place_from_dropped_paths(pane_id, vec![file], 0);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Only folders can be added to Places"
        );
        assert!(app.places.is_empty());

        app.insert_place_from_dropped_paths(pane_id, vec![folder.clone(), current.clone()], 0);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Drop one folder to add a place"
        );
        assert!(app.places.is_empty());

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn place_drag_reorder_moves_user_bookmark_and_persists_order() {
        let current = test_dir("place-reorder-current");
        let alpha = test_dir("place-reorder-alpha");
        let beta = test_dir("place-reorder-beta");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Alpha".to_string(),
                path: alpha.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Beta".to_string(),
                path: beta.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        assert!(app.set_place_drag_drop_target_for_insert(1));
        app.drop_place_drag_to_place_insert(2, 1);

        assert_eq!(
            app.places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Beta", "Alpha", "Root"]
        );
        assert!(app.place_drop_target.is_none());
        assert_eq!(app.status_message_for_pane(pane_id), "Moved place Beta");
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new("Beta".to_string(), beta.clone()),
                UserPlace::new("Alpha".to_string(), alpha.clone()),
            ])
        );

        app.drop_place_drag_to_place_insert(2, app.places.len());

        assert_eq!(
            app.places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Beta", "Alpha", "Root"]
        );
        assert_eq!(app.status_message_for_pane(pane_id), "Place already there");

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(alpha);
        let _ = std::fs::remove_dir_all(beta);
    }

    #[test]
    fn place_drag_reorder_refuses_builtin_places() {
        let current = test_dir("place-reorder-refuse-current");
        let user = test_dir("place-reorder-refuse-user");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.save_user_places().unwrap();

        app.drop_place_drag_to_place_insert(0, 2);

        assert_eq!(
            app.places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "User"]
        );
        assert!(app.place_drop_target.is_none());
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place cannot be moved"
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user.clone())])
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(user);
    }

    #[test]
    fn place_drag_drop_to_pane_navigates_target_pane() {
        let first_dir = test_dir("place-drag-pane-first");
        let second_dir = test_dir("place-drag-pane-second");
        let place_dir = test_dir("place-drag-pane-target");
        std::fs::create_dir_all(&first_dir).unwrap();
        std::fs::create_dir_all(&second_dir).unwrap();
        std::fs::create_dir_all(&place_dir).unwrap();
        let first_arg = first_dir.display().to_string();
        let mut app = test_app_with_entries(&first_arg, &[]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(720.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);
        app.load_pane(second, second_dir.clone());
        assert!(app.set_item_drag_drop_target_for_pane(first, FileTransferMode::Copy));
        assert!(app.set_place_drag_drop_target_for_insert(0));

        app.drop_place_drag_to_pane(second, place_dir.clone());

        assert_eq!(app.panes.focused(), Some(second));
        assert_eq!(
            app.panes
                .pane(second)
                .map(|pane| pane.current_dir.as_path()),
            Some(place_dir.as_path())
        );
        assert!(app.item_drop_target.is_none());
        assert!(app.place_drop_target.is_none());
        assert_eq!(
            app.status_message_for_pane(second),
            format!("Loading {}", place_dir.display())
        );

        let _ = std::fs::remove_dir_all(first_dir);
        let _ = std::fs::remove_dir_all(second_dir);
        let _ = std::fs::remove_dir_all(place_dir);
    }

    #[test]
    fn edit_place_updates_only_editable_user_bookmarks_and_rejects_duplicates() {
        let current = test_dir("place-edit-current");
        let original = test_dir("place-edit-original");
        let duplicate = test_dir("place-edit-duplicate");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&original).unwrap();
        std::fs::create_dir_all(&duplicate).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Original".to_string(),
                path: original.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Duplicate".to_string(),
                path: duplicate.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        app.start_edit_place(pane_id, duplicate.clone());
        if let Some(draft) = &mut app.place_draft {
            draft.label = "Rejected".to_string();
            draft.path = original.display().to_string();
        }
        app.commit_place_draft();
        assert_eq!(app.status_message_for_pane(pane_id), "Place already exists");
        assert_eq!(app.places[1].label, "Duplicate");
        assert_eq!(app.places[1].path, duplicate);

        app.start_edit_place(pane_id, original.clone());
        if let Some(draft) = &mut app.place_draft {
            draft.label = "Edited".to_string();
            draft.path = current.display().to_string();
        }
        app.commit_place_draft();

        assert_eq!(app.places[0].label, "Edited");
        assert_eq!(app.places[0].path, current);
        assert!(app.places[0].editable);
        assert!(app.places[0].removable);
        assert_eq!(app.status_message_for_pane(pane_id), "Updated place Edited");
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new("Edited".to_string(), current.clone()),
                UserPlace::new("Duplicate".to_string(), duplicate.clone()),
            ])
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(original);
        let _ = std::fs::remove_dir_all(duplicate);
    }

    #[test]
    fn remove_place_only_removes_removable_user_bookmarks() {
        let current = test_dir("place-remove-current");
        let user = test_dir("place-remove-user");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: Some(user.clone()),
            focus: PlaceDraftField::Label,
            label: "User".to_string(),
            path: user.display().to_string(),
        });

        app.remove_place(pane_id, &current);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place cannot be removed"
        );
        assert_eq!(app.places.len(), 2);

        app.remove_place(pane_id, &user);
        assert_eq!(app.places.len(), 1);
        assert_eq!(app.places[0].label, "Home");
        assert_eq!(app.status_message_for_pane(pane_id), "Removed place User");
        assert!(app.place_draft.is_none());
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(Vec::new())
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(user);
    }

    #[test]
    fn properties_for_path_reports_file_metadata_without_recursive_work() {
        let temp = test_dir("properties-file");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("note.txt");
        std::fs::write(&file, "properties").unwrap();

        let dialog = properties_for_path(&file);

        assert_eq!(dialog.title, "Properties - note.txt");
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "File")
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Size" && row.value == "10 B")
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn properties_for_selection_summarizes_selected_items() {
        let temp = test_dir("properties-selection");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("note.txt");
        let folder = temp.join("folder");
        std::fs::write(&file, "abc").unwrap();
        std::fs::create_dir_all(&folder).unwrap();

        let dialog = properties_for_selection(&[file, folder]);

        assert_eq!(dialog.title, "Properties - 2 items");
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value.contains("1 folder"))
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value.contains("1 file"))
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Size" && row.value == "3 B")
        );
        let _ = std::fs::remove_dir_all(temp);
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
            pane_shortcut(&gpui::Keystroke::parse("f3").unwrap()),
            Some(PaneShortcut::SplitPane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f2").unwrap()),
            Some(PaneShortcut::RenameSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f6").unwrap()),
            Some(PaneShortcut::EditLocation)
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
            pane_shortcut(&gpui::Keystroke::parse("alt-d").unwrap()),
            Some(PaneShortcut::EditLocation)
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
            pane_shortcut(&gpui::Keystroke::parse("/").unwrap()),
            Some(PaneShortcut::ShowFilter)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-i").unwrap()),
            Some(PaneShortcut::ShowFilter)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-=").unwrap()),
            Some(PaneShortcut::Zoom(ZoomChange::In))
        );
        let mut shifted_plus = gpui::Keystroke::parse("secondary-shift-=").unwrap();
        shifted_plus.key_char = Some("+".to_string());
        assert_eq!(
            pane_shortcut(&shifted_plus),
            Some(PaneShortcut::Zoom(ZoomChange::In))
        );
        let mut zoom_out = gpui::Keystroke::parse("secondary-x").unwrap();
        zoom_out.key = "-".to_string();
        assert_eq!(
            pane_shortcut(&zoom_out),
            Some(PaneShortcut::Zoom(ZoomChange::Out))
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-0").unwrap()),
            Some(PaneShortcut::Zoom(ZoomChange::Reset))
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-l").unwrap()),
            Some(PaneShortcut::EditLocation)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-v").unwrap()),
            Some(PaneShortcut::PasteIntoPane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-w").unwrap()),
            Some(PaneShortcut::ClosePane)
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
    fn wheel_delta_maps_to_zoom_direction() {
        assert_eq!(
            zoom_change_for_wheel_delta(ScrollDelta::Lines(gpui::point(0.0, -1.0))),
            Some(ZoomChange::In)
        );
        assert_eq!(
            zoom_change_for_wheel_delta(ScrollDelta::Lines(gpui::point(0.0, 1.0))),
            Some(ZoomChange::Out)
        );
        assert_eq!(
            zoom_change_for_wheel_delta(ScrollDelta::Pixels(gpui::point(px(0.0), px(0.0)))),
            None
        );
    }

    #[test]
    fn compact_layout_options_derive_size_from_zoom_level() {
        let default_options = ui::file_grid::compact_layout_options(&ViewState::default(), 0.0);
        assert_eq!(default_options.icon_size, 48.0);
        assert_eq!(default_options.item_width, 168.0);
        assert_eq!(default_options.item_height, 76.0);

        let zoomed_options = ui::file_grid::compact_layout_options(
            &ViewState {
                zoom_level: fika_core::MAX_ZOOM_LEVEL,
                ..ViewState::default()
            },
            0.0,
        );
        assert_eq!(zoomed_options.icon_size, 256.0);
        assert_eq!(zoomed_options.item_width, 376.0);
        assert_eq!(zoomed_options.item_height, 284.0);
    }

    #[test]
    fn pane_ratios_are_owned_by_splitter_state() {
        let mut app = test_app_with_entries("/tmp/fika-panes", &[]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(820.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);
        let third = app.panes.split(second).unwrap();
        app.split_pane_ratio(second, third);
        let pane_ids = app.panes.pane_ids().to_vec();

        let available = pane_width_available(820.0, 3);
        assert!(split_ratio_eq(app.pane_split_ratio(pane_ids[0]), 0.5));
        assert!(split_ratio_eq(app.pane_split_ratio(pane_ids[1]), 0.25));
        assert!(split_ratio_eq(app.pane_split_ratio(pane_ids[2]), 0.25));
        assert!(width_value_eq(
            app.projected_pane_width(pane_ids[0]).unwrap(),
            available / 2.0
        ));
        assert!(width_value_eq(
            app.projected_pane_width(pane_ids[1]).unwrap()
                + app.projected_pane_width(pane_ids[2]).unwrap(),
            available / 2.0
        ));
    }

    #[test]
    fn pane_row_width_is_derived_from_actual_child_bounds() {
        let bounds = vec![
            Bounds::new(gpui::point(px(18.0), px(0.0)), size(px(120.0), px(10.0))),
            Bounds::new(gpui::point(px(138.0), px(0.0)), size(px(1.0), px(10.0))),
            Bounds::new(gpui::point(px(139.0), px(0.0)), size(px(180.0), px(10.0))),
        ];

        assert_eq!(pane_row_width_from_child_bounds(&bounds), Some(301.0));
        assert_eq!(pane_row_width_from_child_bounds(&[]), None);
    }

    #[test]
    fn pane_splitter_resize_updates_only_adjacent_widths() {
        let mut app = test_app_with_entries("/tmp/fika-panes", &[]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(820.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);
        let third = app.panes.split(second).unwrap();
        app.split_pane_ratio(second, third);
        let pane_ids = app.panes.pane_ids().to_vec();

        let first_ratio = app.pane_split_ratio(pane_ids[0]);
        let pair_ratio = app.pane_split_ratio(pane_ids[1]) + app.pane_split_ratio(pane_ids[2]);
        assert!(app.resize_pane_pair_from_row_drag(pane_ids[1], pane_ids[2], 700.0, 10.0, 820.0));

        assert!(split_ratio_eq(
            app.pane_split_ratio(pane_ids[0]),
            first_ratio
        ));
        assert!(app.pane_split_ratio(pane_ids[1]) > app.pane_split_ratio(pane_ids[2]));
        assert!(split_ratio_eq(
            app.pane_split_ratio(pane_ids[1]) + app.pane_split_ratio(pane_ids[2]),
            pair_ratio
        ));
    }

    #[test]
    fn pane_content_clear_preserves_split_geometry() {
        let mut app = test_app_with_entries("/tmp/fika-panes-clear", &[]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(720.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);
        let first_ratio = app.pane_split_ratio(first);
        let second_ratio = app.pane_split_ratio(second);

        app.clear_pane_content_state(second);

        assert!(split_ratio_eq(app.pane_split_ratio(first), first_ratio));
        assert!(split_ratio_eq(app.pane_split_ratio(second), second_ratio));
    }

    #[test]
    fn pane_load_preserves_split_geometry() {
        let mut app = test_app_with_entries("/tmp/fika-panes-load-a", &["short"]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(720.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);
        assert!(app.resize_pane_pair_from_row_drag(first, second, 420.0, 10.0, 720.0));
        let ratios = app.pane_split_ratios.clone();

        app.load_pane(second, PathBuf::from("/tmp/fika-panes-load-b"));

        assert_eq!(app.pane_split_ratios, ratios);
    }

    #[test]
    fn closing_pane_promotes_visible_model_snapshot_to_listing_cache() {
        let cached_path = PathBuf::from("/tmp/fika-close-cache");
        let mut app = test_app_with_entries(cached_path.to_str().unwrap(), &["cached.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.close_pane(second);
        app.load_pane(first, PathBuf::from("/tmp/fika-close-other"));
        assert!(app.loading_panes.contains_key(&first));

        app.load_pane(first, cached_path.clone());

        let pane = app.panes.pane(first).unwrap();
        assert_eq!(pane.current_dir, cached_path);
        assert_eq!(pane.model.directory(), Path::new("/tmp/fika-close-cache"));
        assert_eq!(
            pane.model.path_for_index(0),
            Some(PathBuf::from("/tmp/fika-close-cache/cached.txt"))
        );
        assert!(!app.loading_panes.contains_key(&first));
        assert_eq!(app.listing_worker.pending_count(), 0);
    }

    #[test]
    fn pane_load_keeps_previous_view_until_listing_refresh() {
        let mut app = test_app_with_entries("/tmp/fika-load-old", &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let item_ids = app
            .panes
            .pane(pane_id)
            .unwrap()
            .model
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        app.visible_item_slots
            .entry(pane_id)
            .or_default()
            .slots_for_items(item_ids);
        app.compact_column_widths
            .insert(pane_id, CompactColumnWidthCache::default());

        app.load_pane(pane_id, PathBuf::from("/tmp/fika-load-new"));

        let pane = app.panes.pane(pane_id).unwrap();
        assert_eq!(pane.current_dir, PathBuf::from("/tmp/fika-load-new"));
        assert_eq!(pane.model.directory(), Path::new("/tmp/fika-load-old"));
        assert_eq!(
            pane.model.path_for_index(0),
            Some(PathBuf::from("/tmp/fika-load-old/old.txt"))
        );
        assert!(app.visible_item_slots.contains_key(&pane_id));
        assert!(app.compact_column_widths.contains_key(&pane_id));

        let loading = app.loading_panes.get(&pane_id).unwrap();
        app.apply_event(DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation: loading.key.generation,
            request_serial: loading.key.request_serial,
            path: PathBuf::from("/tmp/fika-load-new"),
            entries: test_entries(&["new.txt"]),
        });

        let pane = app.panes.pane(pane_id).unwrap();
        assert_eq!(pane.model.directory(), Path::new("/tmp/fika-load-new"));
        assert_eq!(
            pane.model.path_for_index(0),
            Some(PathBuf::from("/tmp/fika-load-new/new.txt"))
        );
    }

    #[test]
    fn status_bar_zoom_track_maps_drag_position_to_level() {
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                -10.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            fika_core::MIN_ZOOM_LEVEL
        );
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                80.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            8
        );
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                200.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            fika_core::MAX_ZOOM_LEVEL
        );
    }

    #[test]
    fn status_messages_are_pane_local() {
        let mut app = test_app_with_entries("/tmp/fika-status-a", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        app.panes.focus(second);

        app.set_pane_status(first, "First pane");

        assert_eq!(app.status_message_for_pane(first), "First pane");
        assert_eq!(app.status_message_for_pane(second), "Ready");

        app.set_pane_status(second, "Second pane");

        assert_eq!(app.status_message_for_pane(first), "First pane");
        assert_eq!(app.status_message_for_pane(second), "Second pane");
    }

    #[test]
    fn zoom_status_updates_only_target_pane() {
        let mut app = test_app_with_entries("/tmp/fika-status-zoom", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        app.panes.focus(second);
        let next_level = fika_core::DEFAULT_ZOOM_LEVEL + 1;

        app.set_zoom_level(first, next_level);

        assert_eq!(
            app.status_message_for_pane(first),
            format!(
                "Zoom level {next_level} ({} px)",
                fika_core::icon_size_for_zoom_level(next_level) as i32
            )
        );
        assert_eq!(app.status_message_for_pane(second), "Ready");
    }

    #[test]
    fn operation_progress_snapshot_is_pane_local() {
        let mut app = test_app_with_entries("/tmp/fika-status-progress", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.begin_pane_operation(first, "Copying");
        let (_cancel, progress) = app.start_transfer_progress(first, "Copy".to_string());
        {
            let mut progress = progress.lock().unwrap();
            progress.bytes_done = 40;
            progress.bytes_total = 100;
        }
        let now = app.operation_progress.as_ref().unwrap().started_at + PROGRESS_DISPLAY_DELAY;

        let snapshot = app
            .operation_progress_snapshot_for_pane(first, now)
            .unwrap();

        assert_eq!(app.status_message_for_pane(first), "Copying");
        assert_eq!(snapshot.label, "Copy");
        assert_eq!(snapshot.percent, Some(40));
        assert!(snapshot.cancellable);
        assert!(
            app.operation_progress_snapshot_for_pane(second, now)
                .is_none()
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
    fn location_input_action_classifies_controls_completion_and_text() {
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            LocationInputAction::Cancel
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            LocationInputAction::Commit
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("tab").unwrap()),
            LocationInputAction::Complete
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("home").unwrap()),
            LocationInputAction::MoveStart
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("end").unwrap()),
            LocationInputAction::MoveEnd
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("left").unwrap()),
            LocationInputAction::MoveBackward
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("right").unwrap()),
            LocationInputAction::MoveForward
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            LocationInputAction::Backspace
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("delete").unwrap()),
            LocationInputAction::Delete
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("/->/").unwrap()),
            LocationInputAction::Insert("/".to_string())
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            LocationInputAction::Insert("A".to_string())
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("secondary-l").unwrap()),
            LocationInputAction::Ignore
        );
    }

    #[test]
    fn location_caret_click_uses_recorded_text_metrics() {
        let mut app = test_app_with_entries("/tmp/fika-location-click", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.location_draft = Some(LocationDraft::new(pane_id, "abcd".to_string()));
        app.update_location_edit_metrics(
            pane_id,
            "abcd".to_string(),
            100.0,
            12.0,
            80.0,
            vec![(0, 0.0), (1, 8.0), (2, 16.0), (3, 24.0), (4, 32.0)],
        );

        app.set_location_caret_from_window_x(pane_id, 106.0);

        assert_eq!(app.location_draft.as_ref().unwrap().caret, 2);
        assert_eq!(app.location_draft.as_ref().unwrap().scroll_x, 12.0);
    }

    #[test]
    fn place_input_action_classifies_controls_field_switching_and_text() {
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            PlaceInputAction::Cancel
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            PlaceInputAction::Commit
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("tab").unwrap()),
            PlaceInputAction::NextField
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            PlaceInputAction::Backspace
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("/->/").unwrap()),
            PlaceInputAction::Insert("/".to_string())
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            PlaceInputAction::Insert("A".to_string())
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("secondary-a").unwrap()),
            PlaceInputAction::Ignore
        );
    }

    #[test]
    fn filter_input_action_classifies_controls_navigation_and_text() {
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            FilterInputAction::Cancel
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            FilterInputAction::FocusView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("down").unwrap()),
            FilterInputAction::PassToView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("pageup").unwrap()),
            FilterInputAction::PassToView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            FilterInputAction::Backspace
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("a->a").unwrap()),
            FilterInputAction::Insert("a".to_string())
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            FilterInputAction::Insert("A".to_string())
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("secondary-i").unwrap()),
            FilterInputAction::Ignore
        );
    }

    #[test]
    fn filter_projection_is_pane_local_and_navigation_clears_query() {
        let mut app = test_app_with_entries("/tmp/fika-filter-a", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.set_filter_query(first, "*.rs".to_string());
        let first_filtered = app.filtered_model_for_pane(first).unwrap().0;
        assert_eq!(first_filtered.as_slice(), &[0]);
        assert!(app.filtered_model_for_pane(second).is_none());
        assert!(!app.panes.can_go_back(first));

        let next_dir = PathBuf::from("/tmp/fika-filter-b");
        app.load_pane(first, next_dir.clone());
        let first_filter = app.pane_filters.get(&first).unwrap();
        assert!(first_filter.query.is_empty());
        assert!(!first_filter.focused);
        assert!(app.filtered_models.get(&first).is_none());
        assert!(app.panes.can_go_back(first));
        assert_eq!(
            app.panes.pane(first).map(|pane| pane.current_dir.as_path()),
            Some(next_dir.as_path())
        );
    }

    #[test]
    fn filter_projection_rebuilds_after_model_signal() {
        let mut app = test_app_with_entries("/tmp/fika-filter-model", &["alpha.rs", "beta.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.set_filter_query(pane_id, "*.rs".to_string());
        assert!(app.filtered_model_for_pane(pane_id).is_some());
        assert!(app.filtered_models.contains_key(&pane_id));

        let generation = app.panes.pane(pane_id).unwrap().generation;
        app.apply_event(DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial: fika_core::RequestSerial(0),
            path: PathBuf::from("/tmp/fika-filter-model"),
            entries: vec![test_entry("gamma.rs")],
        });

        assert!(app.filtered_models.get(&pane_id).is_none());
        let filtered = app.filtered_model_for_pane(pane_id).unwrap().0;
        assert_eq!(filtered.as_slice(), &[0, 2]);
    }

    #[test]
    fn pane_sort_updates_only_target_pane_and_drops_target_layout_caches() {
        let mut app = test_app_with_entries("/tmp/fika-sort-a", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        let first_alpha = PathBuf::from("/tmp/fika-sort-a/alpha.rs");

        app.select_only(first, first_alpha.clone());
        app.set_filter_query(first, "*.rs".to_string());
        app.set_filter_query(second, "*.rs".to_string());
        assert!(app.filtered_model_for_pane(first).is_some());
        assert!(app.filtered_model_for_pane(second).is_some());
        assert!(app.status_summary_for_pane(first).is_some());
        assert!(app.status_summary_for_pane(second).is_some());

        let first_ids = app
            .panes
            .pane(first)
            .unwrap()
            .model
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let second_ids = app
            .panes
            .pane(second)
            .unwrap()
            .model
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        app.visible_item_slots
            .entry(first)
            .or_default()
            .slots_for_items(first_ids);
        app.visible_item_slots
            .entry(second)
            .or_default()
            .slots_for_items(second_ids);
        app.compact_column_widths
            .insert(first, CompactColumnWidthCache::default());
        app.compact_column_widths
            .insert(second, CompactColumnWidthCache::default());
        app.panes.pane_mut(first).unwrap().view.scroll_x = 64.0;
        app.panes.pane_mut(second).unwrap().view.scroll_x = 32.0;

        app.set_pane_sort_order(first, SortOrder::Descending);

        let pane_names = |app: &FikaApp, pane_id: PaneId| {
            app.panes
                .pane(pane_id)
                .unwrap()
                .model
                .entries()
                .iter()
                .map(|entry| entry.name.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(pane_names(&app, first), vec!["beta.txt", "alpha.rs"]);
        assert_eq!(pane_names(&app, second), vec!["alpha.rs", "beta.txt"]);
        assert!(app.panes.is_selected(first, &first_alpha));
        assert_eq!(app.panes.pane(first).unwrap().view.scroll_x, 0.0);
        assert_eq!(app.panes.pane(second).unwrap().view.scroll_x, 32.0);
        assert!(!app.visible_item_slots.contains_key(&first));
        assert!(app.visible_item_slots.contains_key(&second));
        assert!(!app.compact_column_widths.contains_key(&first));
        assert!(app.compact_column_widths.contains_key(&second));
        assert!(!app.filtered_models.contains_key(&first));
        assert!(app.filtered_models.contains_key(&second));
        assert!(!app.status_summaries.contains_key(&first));
        assert!(app.status_summaries.contains_key(&second));
        assert_eq!(
            app.status_message_for_pane(first),
            "Sorted by Name (Descending)"
        );
        assert_eq!(app.status_message_for_pane(second), "Filtering");
    }

    #[test]
    fn status_summary_reports_current_model_without_selection() {
        let entries = vec![
            status_entry(1, "folder", true, 0),
            status_entry(2, "large.bin", false, 1536),
            status_entry(3, "small.txt", false, 512),
        ];

        assert_eq!(
            status_summary_for_model(&entries, &fika_core::SelectionState::default()),
            "1 folder, 2 files (2.0 KB)"
        );
    }

    #[test]
    fn status_summary_reports_selected_items_without_path_expansion() {
        let entries = vec![
            status_entry(1, "folder", true, 0),
            status_entry(2, "large.bin", false, 1536),
            status_entry(3, "small.txt", false, 512),
        ];
        let mut selection = fika_core::SelectionState::default();
        selection.select_all(Some(fika_core::ItemId(1)));
        assert_eq!(selection.toggle(fika_core::ItemId(2)), false);

        assert_eq!(
            status_summary_for_model(&entries, &selection),
            "1 folder selected, 1 file selected (512 B)"
        );
    }

    #[test]
    fn space_info_snapshot_formats_free_space_and_used_percent() {
        let snapshot = space_info_snapshot(4096, 1024).unwrap();

        assert_eq!(snapshot.free_label, "1.0 KB free");
        assert_eq!(
            snapshot.detail_label,
            "1.0 KB free out of 4.0 KB (75% used)"
        );
        assert_eq!(snapshot.used_percent, 75);
        assert_eq!(
            parse_df_space_output("1B-blocks Avail\n4096 1024\n"),
            Some(snapshot)
        );
    }

    #[test]
    fn progress_percent_handles_unknown_and_complete_totals() {
        assert_eq!(progress_percent(0, 0), None);
        assert_eq!(progress_percent(50, 100), Some(50));
        assert_eq!(progress_percent(128, 128), Some(100));
        assert_eq!(progress_percent(256, 128), Some(100));
    }

    #[test]
    fn progress_delay_matches_dolphin_delayed_progress_bar() {
        let started = Instant::now();

        assert!(!progress_delay_elapsed(
            started,
            started + PROGRESS_DISPLAY_DELAY - Duration::from_millis(1)
        ));
        assert!(progress_delay_elapsed(
            started,
            started + PROGRESS_DISPLAY_DELAY
        ));
    }

    #[test]
    fn item_drag_paths_resolve_selection_only_when_source_item_is_selected() {
        let temp = test_dir("item-drag-paths");
        std::fs::create_dir_all(&temp).unwrap();
        let mut controller = PaneController::new(temp.clone());
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            test_entries(&["alpha.txt", "beta.txt", "gamma.txt"]),
        );
        let alpha = temp.join("alpha.txt");
        let beta = temp.join("beta.txt");
        let gamma = temp.join("gamma.txt");

        assert!(controller.select_only(pane_id, alpha.clone()));
        assert_eq!(
            controller.toggle_selection(pane_id, beta.clone()),
            Some(true)
        );

        let selected_drag = ItemDragPayload {
            source_pane: pane_id,
            source_path: alpha.clone(),
            source_selected: true,
        };
        assert_eq!(
            item_drag_paths(&controller, &selected_drag),
            vec![alpha.clone(), beta.clone()]
        );

        let unselected_drag = ItemDragPayload {
            source_pane: pane_id,
            source_path: gamma.clone(),
            source_selected: false,
        };
        assert_eq!(item_drag_paths(&controller, &unselected_drag), vec![gamma]);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn item_drop_rejects_drop_onto_same_directory() {
        let temp = test_dir("item-drop-same-directory");
        let source = temp.join("source");
        std::fs::create_dir_all(&source).unwrap();

        assert_eq!(
            item_drop_reject_reason(std::slice::from_ref(&source), &source),
            Some("Cannot drop an item onto itself".to_string())
        );

        let target = temp.join("target");
        std::fs::create_dir_all(&target).unwrap();
        assert_eq!(item_drop_reject_reason(&[source], &target), None);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn item_drop_target_tracks_pane_directory_and_lifecycle_clear() {
        let temp = test_dir("item-drop-target");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        assert_eq!(
            item_drop_target_mode_for_pane(app.item_drop_target.as_ref(), pane_id),
            Some(FileTransferMode::Copy)
        );
        assert_eq!(
            item_drop_target_mode_for_directory(
                app.item_drop_target.as_ref(),
                pane_id,
                &target_dir
            ),
            None
        );
        assert!(!app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));

        assert!(app.set_item_drag_drop_target_for_directory(
            pane_id,
            target_dir.clone(),
            FileTransferMode::Copy
        ));
        assert_eq!(
            item_drop_target_mode_for_pane(app.item_drop_target.as_ref(), pane_id),
            None
        );
        assert_eq!(
            item_drop_target_mode_for_directory(
                app.item_drop_target.as_ref(),
                pane_id,
                &target_dir
            ),
            Some(FileTransferMode::Copy)
        );

        app.clear_pane_content_state(pane_id);

        assert!(app.item_drop_target.is_none());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn blank_press_clears_selection_and_starts_rubber_band() {
        let mut app = test_app_with_entries("/tmp/fika-blank-press", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(pane_id, PathBuf::from("/tmp/fika-blank-press/alpha.txt"));

        assert_eq!(app.panes.selected_count(pane_id), Some(1));

        assert!(app.start_rubber_band_from_blank(
            pane_id,
            ViewPoint {
                x: 10_000.0,
                y: 10_000.0
            }
        ));

        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(
            app.rubber_band
                .as_ref()
                .is_some_and(|band| band.pane_id == pane_id)
        );
    }

    #[test]
    fn blank_window_press_uses_viewport_geometry_for_rubber_band() {
        let mut app = test_app_with_entries("/tmp/fika-blank-window-press", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-blank-window-press/alpha.txt"),
        );
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 800.0,
                height: 600.0,
            }
        ));

        assert!(
            app.start_rubber_band_from_window_if_blank(pane_id, gpui::point(px(500.0), px(300.0)))
        );

        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(app.rubber_band.as_ref().is_some_and(|band| {
            band.pane_id == pane_id && band.start == ViewPoint { x: 400.0, y: 250.0 }
        }));
    }

    #[test]
    fn blank_window_press_outside_viewport_does_not_clear_selection() {
        let mut app = test_app_with_entries("/tmp/fika-blank-window-outside", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-blank-window-outside/alpha.txt"),
        );
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 200.0,
            }
        ));

        assert!(!app.start_rubber_band_from_window_if_blank(
            pane_id,
            gpui::point(px(500.0), px(300.0)),
        ));

        assert_eq!(app.panes.selected_count(pane_id), Some(1));
        assert!(app.rubber_band.is_none());
    }

    #[test]
    fn rubber_band_window_update_clamps_to_viewport() {
        let mut app = test_app_with_entries("/tmp/fika-rubber-clamp", &[]);
        let pane_id = app.panes.focused().unwrap();
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 200.0,
            }
        ));
        assert!(app.set_pane_viewport_bounds(pane_id, 300.0, 200.0, 0.0, 0.0));
        assert!(
            app.start_rubber_band_from_window_if_blank(pane_id, gpui::point(px(120.0), px(70.0)),)
        );

        assert!(app.update_rubber_band_from_window(pane_id, gpui::point(px(1000.0), px(900.0)),));

        let band = app.rubber_band.unwrap();
        assert_eq!(band.current, ViewPoint { x: 300.0, y: 200.0 });
        let view = &app.panes.pane(pane_id).unwrap().view;
        assert_eq!(
            band.viewport_rect(view),
            ViewRect {
                x: 20.0,
                y: 20.0,
                width: 280.0,
                height: 180.0,
            }
        );
    }

    #[test]
    fn blank_window_press_without_viewport_geometry_does_not_clear_selection() {
        let mut app = test_app_with_entries("/tmp/fika-blank-missing-origin", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-blank-missing-origin/alpha.txt"),
        );

        assert!(
            !app.start_rubber_band_from_window_if_blank(pane_id, gpui::point(px(500.0), px(300.0)))
        );

        assert_eq!(app.panes.selected_count(pane_id), Some(1));
        assert!(app.rubber_band.is_none());
    }

    #[test]
    fn rubber_band_selection_blank_right_click_clears_without_menu() {
        let mut app =
            test_app_with_entries("/tmp/fika-rubber-context-blank", &["alpha.txt", "beta.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-rubber-context-blank/alpha.txt"),
        );
        app.toggle_selection(
            pane_id,
            PathBuf::from("/tmp/fika-rubber-context-blank/beta.txt"),
        );
        app.rubber_band_selection_panes.insert(pane_id);
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            }
        ));

        assert!(!app.show_blank_context_menu_if_blank(pane_id, gpui::point(px(500.0), px(300.0))));

        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(app.context_menu.is_none());
        assert!(!app.rubber_band_selection_panes.contains(&pane_id));
    }

    #[test]
    fn rubber_band_selection_item_menu_requires_selected_item() {
        let mut app = test_app_with_entries(
            "/tmp/fika-rubber-context-item",
            &["alpha.txt", "beta.txt", "gamma.txt"],
        );
        let pane_id = app.panes.focused().unwrap();
        let alpha = PathBuf::from("/tmp/fika-rubber-context-item/alpha.txt");
        let beta = PathBuf::from("/tmp/fika-rubber-context-item/beta.txt");
        let gamma = PathBuf::from("/tmp/fika-rubber-context-item/gamma.txt");
        app.select_only(pane_id, alpha);
        app.toggle_selection(pane_id, beta.clone());
        app.rubber_band_selection_panes.insert(pane_id);

        assert!(
            app.show_item_context_menu(pane_id, beta, false, gpui::point(px(120.0), px(80.0)),)
        );
        assert!(matches!(
            app.context_menu.as_ref().map(|menu| &menu.target),
            Some(ContextMenuTarget::Item {
                selection_count: 2,
                ..
            })
        ));

        app.dismiss_context_menu();
        assert!(!app.show_item_context_menu(
            pane_id,
            gamma,
            false,
            gpui::point(px(180.0), px(80.0)),
        ));
        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(app.context_menu.is_none());
        assert!(!app.rubber_band_selection_panes.contains(&pane_id));
    }

    #[test]
    fn rubber_band_selection_right_click_outside_selected_visual_clears() {
        let mut app = test_app_with_entries("/tmp/fika-rubber-context-visual", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-rubber-context-visual/alpha.txt"),
        );
        app.rubber_band_selection_panes.insert(pane_id);
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            }
        ));

        let name_width_units = app
            .panes
            .pane(pane_id)
            .and_then(|pane| pane.model.get(0))
            .map(|entry| entry.name_width_units)
            .unwrap();
        let layout = app.layout_projection_for_pane(pane_id).unwrap().layout;
        let item = layout
            .item_with_required_text_width(0, Some(compact_text_width(name_width_units)))
            .unwrap();
        let click = gpui::point(
            px(item.item_rect.right() - 1.0),
            px(item.visual_rect.y + 1.0),
        );
        assert!(!item.visual_rect.contains(ViewPoint {
            x: click.x.as_f32(),
            y: click.y.as_f32(),
        }));

        assert!(!app.show_blank_context_menu_if_blank(pane_id, click));
        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(app.context_menu.is_none());
        assert!(!app.rubber_band_selection_panes.contains(&pane_id));
    }

    #[test]
    fn horizontal_scrollbar_drag_updates_pane_scroll_without_hover_requirement() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-drag", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.begin_horizontal_scrollbar_drag(pane_id, 1600.0, 0.0, 80.0, 240.0));
        let start_scroll = app
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();

        assert!(app.update_horizontal_scrollbar_drag(pane_id, 120.0, 240.0));
        let moved_scroll = app
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();

        assert!(moved_scroll > start_scroll);
    }

    #[test]
    fn horizontal_scrollbar_drag_can_start_from_cached_track_bounds() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-track", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_horizontal_scrollbar_track(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 400.0,
                width: 240.0,
                height: 12.0,
            },
            1600.0,
            0.0,
        ));
        assert!(app.begin_horizontal_scrollbar_drag_from_window(
            pane_id,
            gpui::point(px(180.0), px(406.0)),
        ));

        assert!(app.active_scrollbar_drag.is_some_and(|drag| {
            drag.pane_id == pane_id
                && (drag.track_window_rect.x - 100.0).abs() <= 0.01
                && (drag.handle_grab_x - 18.0).abs() <= 0.01
        }));
        app.clear_horizontal_scrollbar_drag_for_pane(pane_id);
        assert!(!app.horizontal_scrollbar_tracks.contains_key(&pane_id));
    }

    #[test]
    fn horizontal_scrollbar_drag_can_start_from_live_track_bounds() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-live-track", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.begin_horizontal_scrollbar_drag_from_window_track(
            pane_id,
            1600.0,
            0.0,
            ViewRect {
                x: 100.0,
                y: 400.0,
                width: 240.0,
                height: 12.0,
            },
            gpui::point(px(180.0), px(406.0)),
        ));

        assert!(app.active_scrollbar_drag.is_some_and(|drag| {
            drag.pane_id == pane_id
                && (drag.track_window_rect.x - 100.0).abs() <= 0.01
                && (drag.track_window_rect.y - 400.0).abs() <= 0.01
        }));
    }

    #[test]
    fn horizontal_scrollbar_drag_rejects_points_outside_track_height() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-track-y", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_horizontal_scrollbar_track(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 400.0,
                width: 240.0,
                height: 12.0,
            },
            1600.0,
            0.0,
        ));

        assert!(!app.begin_horizontal_scrollbar_drag_from_window(
            pane_id,
            gpui::point(px(180.0), px(390.0)),
        ));
        assert!(app.active_scrollbar_drag.is_none());
    }

    #[test]
    fn horizontal_scrollbar_drag_updates_from_window_position() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-window", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_horizontal_scrollbar_track(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 400.0,
                width: 240.0,
                height: 12.0,
            },
            1600.0,
            0.0,
        ));
        assert!(app.begin_horizontal_scrollbar_drag_from_window(
            pane_id,
            gpui::point(px(180.0), px(406.0)),
        ));
        assert!(app.update_horizontal_scrollbar_drag_from_window(
            pane_id,
            gpui::point(px(260.0), px(406.0)),
        ));

        assert!(
            app.panes
                .pane(pane_id)
                .is_some_and(|pane| pane.view.scroll_x > 0.0)
        );
    }

    #[test]
    fn item_drop_target_mode_changes_refresh_target_state() {
        let temp = test_dir("item-drop-target-mode");
        std::fs::create_dir_all(&temp).unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Copy));
        let first_generation = app.drop_target_stale_generation;
        assert_eq!(
            item_drop_target_mode_for_pane(app.item_drop_target.as_ref(), pane_id),
            Some(FileTransferMode::Copy)
        );

        assert!(app.set_item_drag_drop_target_for_pane(pane_id, FileTransferMode::Move));

        assert!(app.drop_target_stale_generation > first_generation);
        assert_eq!(
            item_drop_target_mode_for_pane(app.item_drop_target.as_ref(), pane_id),
            Some(FileTransferMode::Move)
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn file_transfer_mode_tracks_drag_modifiers() {
        let mut secondary_shift = gpui::Modifiers::secondary_key();
        secondary_shift.shift = true;

        assert_eq!(
            file_transfer_mode_for_modifiers(gpui::Modifiers::none()),
            FileTransferMode::Copy
        );
        assert_eq!(
            file_transfer_mode_for_modifiers(gpui::Modifiers::secondary_key()),
            FileTransferMode::Copy
        );
        assert_eq!(
            file_transfer_mode_for_modifiers(gpui::Modifiers {
                shift: true,
                ..Default::default()
            }),
            FileTransferMode::Move
        );
        assert_eq!(
            file_transfer_mode_for_modifiers(secondary_shift),
            FileTransferMode::Link
        );
        assert_eq!(
            file_transfer_mode_for_modifiers(gpui::Modifiers {
                alt: true,
                ..Default::default()
            }),
            FileTransferMode::Link
        );
    }

    #[test]
    fn drag_cursor_style_tracks_transfer_mode() {
        assert_eq!(
            drag_cursor_style_for_transfer_mode(FileTransferMode::Copy),
            gpui::CursorStyle::DragCopy
        );
        assert_eq!(
            drag_cursor_style_for_transfer_mode(FileTransferMode::Move),
            gpui::CursorStyle::Arrow
        );
        assert_eq!(
            drag_cursor_style_for_transfer_mode(FileTransferMode::Link),
            gpui::CursorStyle::DragLink
        );
    }

    #[test]
    fn visible_item_slot_pool_reuses_offscreen_slots() {
        let mut pool = VisibleItemSlotPool::default();
        let first = pool.slots_for_items([fika_core::ItemId(1), fika_core::ItemId(2)]);
        assert_eq!(first.len(), 2);

        let slot_for_one = first[&fika_core::ItemId(1)];
        let slot_for_two = first[&fika_core::ItemId(2)];
        let second = pool.slots_for_items([fika_core::ItemId(2), fika_core::ItemId(3)]);

        assert_eq!(second[&fika_core::ItemId(2)], slot_for_two);
        assert_eq!(second[&fika_core::ItemId(3)], slot_for_one);
        assert_eq!(pool.slot_by_item_id.len(), 2);
    }

    #[test]
    fn visible_item_slot_pool_caps_recycled_slots() {
        let mut pool = VisibleItemSlotPool::default();
        let visible = (1..=150).map(fika_core::ItemId).collect::<Vec<_>>();
        let first = pool.slots_for_items(visible);
        assert_eq!(first.len(), 150);

        let second = pool.slots_for_items(std::iter::empty::<fika_core::ItemId>());

        assert!(second.is_empty());
        assert_eq!(pool.free_slots.len(), VisibleItemSlotPool::MAX_FREE_SLOTS);
    }

    #[test]
    fn compact_column_width_cache_resolves_all_columns_for_stable_scrollbar() {
        let mut model = fika_core::DirectoryModel::for_directory(PathBuf::from("/tmp"));
        let entries = (0..120)
            .map(|index| {
                let name = if index == 80 {
                    format!(
                        "{index:03}-very-long-name-that-should-not-be-measured-until-scrolled.txt"
                    )
                } else {
                    format!("{index:03}.txt")
                };
                fika_core::Entry::new(fika_core::EntryData {
                    name: Arc::from(name.as_str()),
                    name_width_units: name.len() as u16,
                    size_bytes: 0,
                    modified_secs: None,
                    thumbnail_path: None,
                    trash_original_path: None,
                    trash_deletion_time: None,
                    mime_type: None,
                    is_dir: false,
                })
            })
            .collect::<Vec<_>>();
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(entries));

        let mut cache = CompactColumnWidthCache::default();
        let options = CompactLayoutOptions {
            viewport_width: 140.0,
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            scroll_x: 0.0,
            ..CompactLayoutOptions::default()
        };
        let rows_per_column = CompactLayout::rows_per_column_for_options(options);
        let metrics = cache.metrics_for_model(&model, rows_per_column, options);
        let column_count = model.len().div_ceil(rows_per_column);
        let resolved_count = cache.cached[0]
            .resolved_columns
            .iter()
            .filter(|resolved| **resolved)
            .count();

        assert_eq!(resolved_count, column_count);
        let far_column = 80 / rows_per_column;
        assert!(
            metrics.column_width(far_column).unwrap() > options.item_width,
            "far column width should be resolved before scrolling reaches it"
        );
    }

    fn test_app_with_entries(path: &str, names: &[&str]) -> FikaApp {
        let path = PathBuf::from(path);
        let mut panes = PaneController::new(path.clone());
        let pane_id = panes.focused().unwrap();
        panes
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(path, test_entries(names));
        FikaApp {
            panes,
            places: Vec::new(),
            hidden_places: BTreeSet::new(),
            hidden_place_sections: BTreeSet::new(),
            user_places_path: test_dir("user-places").join("user-places.xbel"),
            device_refresh_pending: false,
            next_device_refresh_at: Instant::now(),
            device_monitor_rx: None,
            device_monitor_active: false,
            next_device_monitor_start_at: Instant::now(),
            file_icons: FileIconCache::default(),
            mime_applications: MimeApplicationCache::empty(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            smooth_scrolls: HashMap::new(),
            scroll_drag_trackers: HashMap::new(),
            active_scrollbar_drag: None,
            horizontal_scrollbar_tracks: HashMap::new(),
            pane_viewport_geometries: HashMap::new(),
            pane_split_ratios: HashMap::new(),
            pane_row_width: 0.0,
            visible_item_slots: HashMap::new(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            active_item_drag: None,
            item_drop_target: None,
            place_drop_target: None,
            drop_target_stale_generation: 0,
            drop_target_stale_timer_running: false,
            rename_draft: None,
            location_draft: None,
            location_edit_metrics: HashMap::new(),
            place_draft: None,
            chooser: None,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: None,
            rubber_band_selection_panes: HashSet::new(),
            context_menu: None,
            context_menu_tree_hovered: false,
            context_submenu_hide_generation: 0,
            properties_dialog: None,
            application_chooser: None,
            pane_statuses: HashMap::new(),
            operation_pending: false,
            operation_pane: None,
            operation_progress: None,
        }
    }

    fn test_entry(name: &str) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            thumbnail_path: None,
            trash_original_path: None,
            trash_deletion_time: None,
            mime_type: None,
            is_dir: false,
        })
    }

    fn test_desktop_application(
        id: &str,
        name: &str,
        exec: &str,
        mime_types: &[&str],
    ) -> fika_core::DesktopApplication {
        fika_core::DesktopApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(format!("/apps/{id}")),
            name: name.to_string(),
            exec: exec.to_string(),
            icon: None,
            mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
            actions: Vec::new(),
        }
    }

    fn test_entries(names: &[&str]) -> Arc<Vec<fika_core::Entry>> {
        Arc::new(names.iter().map(|name| test_entry(name)).collect())
    }

    fn status_entry(
        id: u64,
        name: &'static str,
        is_dir: bool,
        size_bytes: u64,
    ) -> fika_core::ModelEntry {
        fika_core::ModelEntry {
            id: fika_core::ItemId(id),
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from(name),
                name_width_units: name.len() as u16,
                size_bytes,
                modified_secs: None,
                thumbnail_path: None,
                trash_original_path: None,
                trash_deletion_time: None,
                mime_type: None,
                is_dir,
            }),
        }
    }

    fn context_blank_target() -> ContextMenuTarget {
        ContextMenuTarget::Blank {
            trash_view: false,
            trash_has_items: false,
            service_actions: Vec::new(),
        }
    }

    fn context_item_target(path: &str, is_dir: bool, selection_count: usize) -> ContextMenuTarget {
        ContextMenuTarget::Item {
            path: PathBuf::from(path),
            is_dir,
            selection_count,
            trash_view: false,
            trash_can_restore: false,
            mime_type: None,
            open_with_apps: Vec::new(),
            service_actions: Vec::new(),
        }
    }

    fn context_place_target(
        path: PathBuf,
        trash_place: bool,
        trash_has_items: bool,
    ) -> ContextMenuTarget {
        ContextMenuTarget::Place {
            path,
            mounted: true,
            device: false,
            trash_place,
            trash_has_items,
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-gpui-{name}-{}-{nanos}", std::process::id()))
    }
}
