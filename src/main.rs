mod cli;
mod ui;

use cli::{Args, Mode};
use fika_core::Operation;
#[cfg(test)]
use fika_core::SystemdLaunchResult;
use fika_core::{
    AppSettings, CreateItemResult, FileTransferMode, PlacesSidebarSettings, PrivilegedCommand,
    PrivilegedOperationResult, RenameItemResult, TransferTaskResult, TrashSelectionResult,
    TrashViewOperation, TrashViewOperationResult, UndoTaskResult, action_status,
    create_item_result_async, created_item_label, default_app_settings_path,
    default_created_item_name, load_app_settings, rename_item_result_async, run_operation_task,
    run_registered_operation, run_via_dbus, save_app_settings, transfer_paths_result_async,
    trash_selection_result_async, trash_view_operation_result_async, undo_record_result_async,
};
use fika_core::{
    CreateUndoItem, CreatedItemKind, DeviceMonitorMessage, DevicePlaceOperation,
    DevicePlaceOperationResult, DirectoryCacheDebugSnapshot, DirectoryListerEvent, ItemId,
    ListingRequest, ListingWorker, LoadingPaneState, MetadataRoleScheduler, OperationQueue,
    OperationRuntime, OperationSnapshot, PaneController, PaneId, RefreshPair, RenameUndoItem,
    SelectionMove, SortDescriptor, SortOrder, SortRole, ThumbnailScheduler, TrashEmptinessMonitor,
    UndoPayload, ViewMode, ViewPoint, ViewRect, ZoomChange, breadcrumb_segments,
    complete_location_input, file_ops, is_network_path, listing_requests_from_events,
    nearest_existing_ancestor, parent_location, perform_device_place_operation,
    resolve_location_input, update_loading_state_for_event,
};
use fika_core::{
    DesktopLaunchPlan, LauncherError, MimeApplication, MimeApplicationCache, NewWindowLaunchResult,
    OpenWithLaunchResult, ServiceMenuLaunchResult, ServiceMenuTarget, ark_compress_launch_plan,
    ark_extract_here_launch_plan, ark_extract_to_launch_plan, current_executable_launch_plan,
    launch_with_systemd_user, service_menu_target_label, set_default_mime_application,
};
#[cfg(test)]
use fika_core::{
    DeviceInfo, Generation, MetadataRoleResult, ServiceMenuAction, ThumbnailCandidate,
    ThumbnailProbeResult, ThumbnailRequestPriority, UserPlace, ViewState, home_dir,
    is_network_root_path, network_root_path,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, ExternalPaths, IntoElement, ParentElement, Render,
    ScrollDelta, ScrollHandle, ScrollStrategy, Styled, Window, WindowBounds, WindowOptions, div,
    px, rgb, size,
};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use ui::application_chooser::{
    ApplicationChooserState, application_chooser_filtered_applications,
    application_chooser_overlay, dedup_application_chooser_applications,
};
use ui::background_tasks::{
    BackgroundTaskDetailDialog, BackgroundTaskHistorySnapshot, BackgroundTaskId,
    BackgroundTaskSnapshot, BackgroundTaskState, BackgroundTasksSnapshot,
    background_task_detail_dialog_overlay,
};
use ui::chooser::{ChooserState, selected_choice_rows};
use ui::clipboard::{
    ClipboardMode, ClipboardState, paste_clipboard_result_async, primary_paste_clipboard_state,
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
    ActiveItemDrag, DropTargetState, ItemDragPayload, ItemDropTarget, PathListDropTarget,
    PathListDropTargetKind, PathListDropTargetUpdate, item_drag_export_payload, item_drag_paths,
    item_drop_reject_reason, item_drop_target_matches_pane, normalized_drag_paths,
};
#[cfg(test)]
use ui::drag_drop::{
    drag_cursor_style_for_transfer_mode, item_drop_target_matches_directory,
    place_drop_target_matches_insert, place_drop_target_matches_place,
};
use ui::file_grid::{
    CompactColumnWidthCache, ContentItemHit, DetailsTextShapeCache, FileIconResolveQueue, ItemDrag,
    ItemPaintSlotCache, ItemViewAutosmokeScenario, ItemViewPerfState, PaneLayoutProjection,
    PaneLayoutProjectionInput, PaneViewportGeometry, PaneVisibleWorkKey, RetainedHoveredItem,
    StaticItemTextShapeCache, VisibleItemSlotPool, VisibleItemSnapshotCache,
    clamped_content_point_from_window_position, compact_text_width, compact_text_width_for_name,
    content_point_from_window_position, item_view_perf_enabled, pane_at_window_position,
    pane_content_item_hit_at_point, pane_layout_projection,
    pane_model_indexes_intersecting_visual_rect, rename_editor_required_text_width,
    start_item_view_autosmoke,
};
#[cfg(test)]
use ui::file_grid::{RawFileGridSnapshot, THUMBNAIL_PROBE_BATCH_SIZE};
use ui::filter_bar::{
    FILTER_BAR_HEIGHT, FilterBarSnapshot, FilteredModelCacheEntry, PaneFilterState,
    cached_filtered_model_for_pane, filter_toggle_snapshot,
};
use ui::icons::{FileIconCache, ThemeIconImageReadiness};
use ui::item_view::{
    ItemViewScrollState, begin_item_view_scrollbar_drag as begin_item_view_scrollbar_drag_state,
    finish_item_view_scrollbar_drag as finish_item_view_scrollbar_drag_state,
    item_view_scroll_handle_for_pane as item_view_scroll_handle_for_pane_state,
    preserve_item_view_scroll_for_layout_change as preserve_item_view_scroll_for_layout_change_state,
    projected_item_viewport_width_for_pane_width,
    remove_item_view_scroll_for_pane as remove_item_view_scroll_for_pane_state,
    reset_item_view_scroll_for_pane as reset_item_view_scroll_for_pane_state,
    scroll_pane_from_item_view_wheel as scroll_item_view_pane_from_wheel,
    sync_item_view_scroll_handle_to_pane_view as sync_item_view_handle_to_pane_view_state,
    sync_item_view_scroll_handle_to_view_authoritatively as sync_item_view_handle_to_view_authoritatively,
    sync_pane_view_after_item_view_bounds_update as sync_item_view_pane_after_bounds_update,
    sync_pane_view_from_authoritative_item_view_scroll_handle as sync_item_view_pane_from_authoritative_scroll_handle,
    sync_pane_view_from_item_view_scroll_handle as sync_item_view_pane_from_scroll_handle,
    viewport_extents_after_view_mode_axis_change,
    viewport_height_after_filter_bar_visibility_change, window_resize_viewport_prime,
};
#[cfg(test)]
use ui::item_view::{
    item_view_scroll_has_authoritative_scroll, item_view_scroll_is_scrollbar_dragging,
};
use ui::location_bar::{LocationDraft, LocationEditMetrics};
use ui::network_auth::{
    NetworkAuthDraft, NetworkAuthField, NetworkAuthInputResult, apply_network_auth_input_action,
    network_auth_overlay,
};
use ui::pane::{
    MIN_PANE_WIDTH, PANE_SPLITTER_WIDTH, PaneSnapshot, PaneSplitterDrag, normalize_pane_ratios,
    pane_close_icon_snapshot, pane_row_width_from_child_bounds, pane_split_icon_snapshot,
    pane_splitter, pane_width_available, sort_order_label, sort_role_label, split_ratio_eq,
    width_value_eq,
};
use ui::place_draft::{
    PlaceDraft, PlaceDraftField, PlaceDraftInputResult, apply_place_input_action,
    clear_place_draft_for_pane as clear_place_draft_state_for_pane, place_draft_overlay,
    set_place_draft_focus as set_place_draft_state_focus,
};
use ui::places::PlacePaintSlotCache;
#[cfg(test)]
use ui::places::{
    DEVICES_GROUP, NETWORK_GROUP, REMOVABLE_DEVICES_GROUP, active_place_index,
    build_places_with_devices, default_place_label, place_is_mounted,
    places_sidebar_width_from_drag,
};
use ui::places::{
    PLACES_SIDEBAR_DEFAULT_WIDTH, PlaceDrag, PlaceEntry, PlaceSnapshot, PlacesAutosmokeScenario,
    PlacesLayoutAutosmokeState, PlacesRowTextShapeCache, PlacesSidebarResizeDrag, build_places,
    clamp_places_sidebar_width, places_panel_button, places_panel_icon_snapshot,
    places_sidebar_splitter, read_live_device_snapshot, start_places_autosmoke,
};
use ui::properties_dialog::{
    PropertiesDialogState, properties_dialog_overlay, properties_for_path, properties_for_selection,
};
use ui::rename::{RENAME_TEXT_INSET_X, RenameDraft};
use ui::rubber_band::RubberBandController;
#[cfg(test)]
use ui::shortcuts::PlaceInputAction;
use ui::shortcuts::{
    ApplicationChooserInputAction, FilterInputAction, LocationInputAction, PaneShortcut,
    RenameInputAction, application_chooser_input_action, filter_input_action,
    location_input_action, pane_shortcut, place_input_action, rename_input_action,
    zoom_change_for_wheel_delta,
};
use ui::status_bar::progress_percent;
use ui::status_bar::{
    OperationProgressSnapshot, SpaceInfoCache, SpaceInfoSnapshot, StatusBarSnapshot,
    StatusSummaryCacheEntry, StatusSummaryCacheKey, filesystem_space_info, progress_delay_elapsed,
    status_summary_for_model, status_summary_for_model_indexes,
};
#[cfg(test)]
use ui::status_bar::{PROGRESS_DISPLAY_DELAY, parse_df_space_output, space_info_snapshot};
use ui::trash_conflict::{TrashConflictDialogState, trash_conflict_dialog_overlay};

const DROP_TARGET_LEASE_TIMEOUT: Duration = Duration::from_millis(3000);
const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const DEVICE_MONITOR_RETRY_INTERVAL: Duration = Duration::from_secs(60);
const DEBUG_CACHE_ENV: &str = "FIKA_DEBUG_CACHE";
const DEBUG_DND_ENV: &str = "FIKA_DEBUG_DND";
const DEBUG_NAV_ENV: &str = "FIKA_DEBUG_NAV";
const BACKGROUND_TASK_HISTORY_LIMIT: usize = 8;

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name).is_ok_and(|value| env_flag_is_truthy(&value))
}

fn listing_cache_debug_enabled() -> bool {
    env_flag_enabled(DEBUG_CACHE_ENV) || env_flag_enabled(DEBUG_NAV_ENV)
}

pub(crate) fn dnd_debug_enabled() -> bool {
    env_flag_enabled(DEBUG_DND_ENV)
}

fn listing_cache_debug_summary(
    reason: &str,
    snapshot: &DirectoryCacheDebugSnapshot,
    pending_count: usize,
) -> String {
    let stats = snapshot.stats();
    let largest_skipped = snapshot
        .skipped_large_directories()
        .iter()
        .max_by_key(|summary| summary.entry_count())
        .map(|summary| format!(" {}:{}", summary.path().display(), summary.entry_count()))
        .unwrap_or_default();

    format!(
        "[fika cache] {reason}: pending={pending_count} cached_dirs={} cached_entries={} hits={} misses={} stale_invalidations={} evicted_dirs={} skipped_large={} large_summaries={} largest_skipped={largest_skipped}",
        snapshot.cached_directories().len(),
        stats.cached_entries,
        stats.hits,
        stats.misses,
        stats.stale_invalidations,
        stats.evicted_directories,
        stats.skipped_large_directories,
        snapshot.skipped_large_directories().len(),
    )
}
const VISIBLE_METADATA_ROLE_SYNC_BUDGET: Duration = Duration::from_millis(12);
const PANE_HORIZONTAL_BORDER_EXTENT: f32 = 2.0;

const CONTEXT_SUBMENU_HIDE_DELAY: Duration = Duration::from_millis(300);
const APP_SETTINGS_SAVE_DELAY: Duration = Duration::from_millis(120);

fn zoom_level_after_change(current: i32, change: ZoomChange) -> i32 {
    match change {
        ZoomChange::In => current + 1,
        ZoomChange::Out => current - 1,
        ZoomChange::Reset => fika_core::DEFAULT_ZOOM_LEVEL,
    }
    .clamp(fika_core::MIN_ZOOM_LEVEL, fika_core::MAX_ZOOM_LEVEL)
}

fn view_mode_status(view_mode: ViewMode) -> &'static str {
    match view_mode {
        ViewMode::Icons => "Icons view",
        ViewMode::Compact => "Compact view",
        ViewMode::Details => "Details view",
    }
}

fn background_task_state_for_message(message: &str) -> BackgroundTaskState {
    let message = message.trim().to_ascii_lowercase();
    if message.starts_with("cannot ")
        || message.contains(" failed")
        || message.contains("failed ")
        || message.contains("cancelled")
        || message.contains("canceled")
        || message.contains(" stale")
    {
        BackgroundTaskState::Failed
    } else {
        BackgroundTaskState::Complete
    }
}

fn pane_loading_status_matches_path(message: &str, path: &Path) -> bool {
    let display_path = path.display().to_string();
    message == format!("Loading {display_path}") || message == format!("Reloading {display_path}")
}

fn default_open_with_application_id(applications: &[MimeApplication]) -> Option<&str> {
    applications
        .iter()
        .find(|application| application.is_default)
        .or_else(|| applications.first())
        .map(|application| application.id.as_str())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneLoadingScrollPolicy {
    Reset,
    Preserve,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BackgroundTaskHistoryRecord {
    title: String,
    detail: String,
    state: BackgroundTaskState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PrivilegedTaskResult {
    pane_id: PaneId,
    title: String,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    clear_clipboard: bool,
    detail: String,
}

async fn privileged_task_result_for_commands(
    pane_id: PaneId,
    title: String,
    commands: Vec<PrivilegedCommand>,
    clear_clipboard: bool,
) -> PrivilegedTaskResult {
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut detail_lines = vec![format!("{title}")];

    for command in commands {
        let summary = command.summary();
        let PrivilegedOperationResult {
            affected_dirs: result_dirs,
            result,
            ..
        } = run_via_dbus(command).await;
        match result {
            Ok(message) => {
                success_count += 1;
                detail_lines.push(format!("OK: {summary}"));
                if !message.trim().is_empty() {
                    detail_lines.push(format!("  {message}"));
                }
                for dir in result_dirs {
                    if !affected_dirs.iter().any(|existing| existing == &dir) {
                        affected_dirs.push(dir);
                    }
                }
            }
            Err(error) => {
                failure_count += 1;
                detail_lines.push(format!("Failed: {summary}"));
                detail_lines.push(format!("  {error}"));
            }
        }
    }

    PrivilegedTaskResult {
        pane_id,
        title,
        success_count,
        failure_count,
        affected_dirs,
        clear_clipboard,
        detail: detail_lines.join("\n"),
    }
}

pub(crate) struct FikaApp {
    pub(crate) panes: PaneController,
    places: Vec<PlaceEntry>,
    trash_has_items: bool,
    trash_monitor: TrashEmptinessMonitor,
    hidden_places: BTreeSet<PathBuf>,
    hidden_place_sections: BTreeSet<&'static str>,
    place_paint_slots: PlacePaintSlotCache,
    place_row_text_shape_cache: PlacesRowTextShapeCache,
    places_sidebar_width: f32,
    places_sidebar_visible: bool,
    places_layout_autosmoke_original: Option<PlacesLayoutAutosmokeState>,
    app_settings_path: PathBuf,
    app_settings_save_generation: u64,
    app_settings_save_task_running: bool,
    user_places_path: PathBuf,
    device_refresh_pending: bool,
    next_device_refresh_at: Instant,
    device_monitor_rx: Option<mpsc::Receiver<DeviceMonitorMessage>>,
    device_monitor_active: bool,
    next_device_monitor_start_at: Instant,
    file_icons: FileIconCache,
    file_icon_resolve_queue: FileIconResolveQueue,
    theme_icon_readiness: ThemeIconImageReadiness,
    mime_applications: MimeApplicationCache,
    space_info: SpaceInfoCache,
    status_summaries: HashMap<PaneId, StatusSummaryCacheEntry>,
    loading_panes: HashMap<PaneId, LoadingPaneState>,
    item_view_scroll: ItemViewScrollState,
    metadata_role_scheduler: MetadataRoleScheduler,
    thumbnail_scheduler: ThumbnailScheduler,
    visible_work_keys: HashMap<PaneId, PaneVisibleWorkKey>,
    pane_viewport_geometries: HashMap<PaneId, PaneViewportGeometry>,
    pane_split_ratios: HashMap<PaneId, f32>,
    pane_resize_notify_pending: bool,
    last_render_viewport_size: Option<(f32, f32)>,
    pane_row_width: f32,
    visible_item_slots: HashMap<PaneId, VisibleItemSlotPool>,
    item_paint_slots: HashMap<PaneId, ItemPaintSlotCache>,
    visible_item_snapshot_caches: HashMap<PaneId, VisibleItemSnapshotCache>,
    static_item_text_shape_caches: HashMap<PaneId, StaticItemTextShapeCache>,
    details_text_shape_caches: HashMap<PaneId, DetailsTextShapeCache>,
    item_view_perf: ItemViewPerfState,
    hovered_item: RetainedHoveredItem,
    compact_column_widths: HashMap<PaneId, CompactColumnWidthCache>,
    pane_filters: HashMap<PaneId, PaneFilterState>,
    filtered_models: HashMap<PaneId, FilteredModelCacheEntry>,
    operations: OperationQueue,
    clipboard: Option<ClipboardState>,
    active_item_drag: Option<ActiveItemDrag>,
    drop_targets: DropTargetState,
    drop_target_lease_timer_running: bool,
    rename_draft: Option<RenameDraft>,
    rename_next_after_operation: Option<(PaneId, PathBuf)>,
    location_draft: Option<LocationDraft>,
    location_edit_metrics: HashMap<PaneId, LocationEditMetrics>,
    place_draft: Option<PlaceDraft>,
    network_auth_draft: Option<NetworkAuthDraft>,
    chooser: Option<ChooserState>,
    listing_worker: ListingWorker,
    _keystroke_subscription: Option<gpui::Subscription>,
    rubber_band: RubberBandController,
    context_menu: Option<ContextMenuState>,
    context_menu_tree_hovered: bool,
    context_submenu_hide_generation: u64,
    properties_dialog: Option<PropertiesDialogState>,
    trash_conflict_dialog: Option<TrashConflictDialogState>,
    application_chooser: Option<ApplicationChooserState>,
    pane_statuses: HashMap<PaneId, String>,
    background_tasks_expanded: bool,
    background_task_history: VecDeque<BackgroundTaskHistoryRecord>,
    background_task_detail_dialog: Option<BackgroundTaskDetailDialog>,
}

impl FikaApp {
    fn new(args: Args, cx: &mut Context<Self>) -> Self {
        let user_places_path = fika_core::default_user_places_path();
        let app_settings_path = default_app_settings_path();
        let app_settings = load_app_settings(&app_settings_path).unwrap_or_default();
        let places_sidebar_width = app_settings
            .places_sidebar
            .width
            .map(clamp_places_sidebar_width)
            .unwrap_or(PLACES_SIDEBAR_DEFAULT_WIDTH);
        let places_sidebar_visible = app_settings.places_sidebar.visible.unwrap_or(true);
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
        let initial_devices = fika_core::read_gio_devices().unwrap_or_default();
        let trash_monitor = TrashEmptinessMonitor::new();
        let (listing_results_tx, listing_results_rx) = mpsc::channel();
        let mut app = Self {
            panes: PaneController::new(args.start_dir.clone()),
            places: build_places(&user_places_path),
            trash_has_items: trash_monitor.has_items(),
            trash_monitor,
            hidden_places: BTreeSet::new(),
            hidden_place_sections: BTreeSet::new(),
            place_paint_slots: PlacePaintSlotCache::default(),
            place_row_text_shape_cache: PlacesRowTextShapeCache::default(),
            places_sidebar_width,
            places_sidebar_visible,
            places_layout_autosmoke_original: None,
            app_settings_path,
            app_settings_save_generation: 0,
            app_settings_save_task_running: false,
            user_places_path,
            device_refresh_pending: false,
            next_device_refresh_at: Instant::now(),
            device_monitor_rx: None,
            device_monitor_active: false,
            next_device_monitor_start_at: Instant::now(),
            file_icons: FileIconCache::default(),
            file_icon_resolve_queue: FileIconResolveQueue::default(),
            theme_icon_readiness: ThemeIconImageReadiness::default(),
            mime_applications: MimeApplicationCache::load(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            item_view_scroll: ItemViewScrollState::default(),
            metadata_role_scheduler: MetadataRoleScheduler::default(),
            thumbnail_scheduler: ThumbnailScheduler::default(),
            visible_work_keys: HashMap::new(),
            pane_viewport_geometries: HashMap::new(),
            pane_split_ratios: HashMap::new(),
            pane_resize_notify_pending: false,
            last_render_viewport_size: None,
            pane_row_width: 0.0,
            visible_item_slots: HashMap::new(),
            item_paint_slots: HashMap::new(),
            visible_item_snapshot_caches: HashMap::new(),
            static_item_text_shape_caches: HashMap::new(),
            details_text_shape_caches: HashMap::new(),
            item_view_perf: ItemViewPerfState::default(),
            hovered_item: RetainedHoveredItem::default(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            active_item_drag: None,
            drop_targets: DropTargetState::default(),
            drop_target_lease_timer_running: false,
            rename_draft: None,
            rename_next_after_operation: None,
            location_draft: None,
            location_edit_metrics: HashMap::new(),
            place_draft: None,
            network_auth_draft: None,
            chooser,
            listing_worker: ListingWorker::with_result_notifier(listing_results_tx),
            _keystroke_subscription: None,
            rubber_band: RubberBandController::default(),
            context_menu: None,
            context_menu_tree_hovered: false,
            context_submenu_hide_generation: 0,
            properties_dialog: None,
            trash_conflict_dialog: None,
            application_chooser: None,
            pane_statuses: HashMap::new(),
            background_tasks_expanded: false,
            background_task_history: VecDeque::new(),
            background_task_detail_dialog: None,
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
        app.start_trash_monitor();
        app.maybe_start_device_monitor(cx);
        Self::start_listing_result_monitor(listing_results_rx, cx);
        if let Some(scenario) = ItemViewAutosmokeScenario::from_env() {
            start_item_view_autosmoke(first, scenario, cx);
        }
        if let Some(scenario) = PlacesAutosmokeScenario::from_env() {
            start_places_autosmoke(scenario, cx);
        }
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
                                let mut changed = false;
                                if app.drain_watchers() {
                                    changed = true;
                                }
                                if app.drain_trash_monitor() {
                                    changed = true;
                                }
                                if app.drain_device_monitor_messages() {
                                    changed = true;
                                }
                                if app.drain_background_listing_results() {
                                    changed = true;
                                }
                                if !app.operation_snapshots().is_empty()
                                    || !app.loading_panes.is_empty()
                                {
                                    changed = true;
                                }
                                if changed {
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

    fn filtered_model_for_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<(fika_core::FilteredModel, u64)> {
        cached_filtered_model_for_pane(
            pane_id,
            &self.pane_filters,
            &mut self.filtered_models,
            self.panes.pane(pane_id).map(|pane| &pane.model),
        )
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

    fn set_filter_bar_visible(&mut self, pane_id: PaneId, visible: bool) -> bool {
        let was_visible = self
            .pane_filters
            .get(&pane_id)
            .is_some_and(|filter| filter.visible);
        if was_visible == visible {
            return false;
        }
        if visible {
            self.pane_filters.entry(pane_id).or_default().visible = true;
        } else if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.visible = false;
        } else {
            return false;
        }
        self.prime_pane_viewport_for_filter_bar_change(pane_id, visible);
        true
    }

    fn prime_pane_viewport_for_filter_bar_change(&mut self, pane_id: PaneId, visible: bool) {
        let Some(view) = self.panes.pane_mut(pane_id).map(|pane| {
            pane.view.viewport_height = viewport_height_after_filter_bar_visibility_change(
                pane.view.viewport_height,
                visible,
                FILTER_BAR_HEIGHT,
            );
            pane.view.clone()
        }) else {
            return;
        };

        let _ = sync_item_view_handle_to_view_authoritatively(
            &mut self.item_view_scroll,
            pane_id,
            &view,
        );
    }

    pub(crate) fn show_filter_bar(&mut self, pane_id: PaneId) {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.set_filter_bar_visible(pane_id, true);
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.focused = true;
        self.set_pane_status(pane_id, "Filter");
    }

    pub(crate) fn focus_filter_bar(&mut self, pane_id: PaneId) {
        self.show_filter_bar(pane_id);
    }

    pub(crate) fn toggle_filter_bar_from_button(&mut self, pane_id: PaneId) {
        if self
            .pane_filters
            .get(&pane_id)
            .is_some_and(|filter| filter.visible)
        {
            self.panes.focus(pane_id);
            self.close_filter_bar(pane_id);
        } else {
            self.show_filter_bar(pane_id);
        }
    }

    pub(crate) fn toggle_pane_layout_from_button(&mut self, pane_id: PaneId) {
        self.panes.focus(pane_id);
        if self.panes.pane_ids().len() <= 1 {
            self.split_pane(pane_id);
        } else {
            self.close_pane(pane_id);
        }
    }

    fn current_app_settings(&self) -> AppSettings {
        AppSettings {
            places_sidebar: PlacesSidebarSettings {
                width: Some(self.places_sidebar_width),
                visible: Some(self.places_sidebar_visible),
            },
        }
    }

    fn schedule_app_settings_save(&mut self, cx: &mut Context<Self>) {
        self.app_settings_save_generation = self.app_settings_save_generation.wrapping_add(1);
        if self.app_settings_save_task_running {
            return;
        }
        self.app_settings_save_task_running = true;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor()
                            .timer(APP_SETTINGS_SAVE_DELAY)
                            .await;
                        let Ok((path, settings, generation)) = this.update(&mut cx, |app, _cx| {
                            (
                                app.app_settings_path.clone(),
                                app.current_app_settings(),
                                app.app_settings_save_generation,
                            )
                        }) else {
                            break;
                        };
                        let result = cx
                            .background_spawn(async move { save_app_settings(&path, &settings) })
                            .await;
                        let keep_running = this
                            .update(&mut cx, |app, _cx| {
                                if let Err(err) = result {
                                    eprintln!("[fika settings] save failed: {err}");
                                }
                                if app.app_settings_save_generation != generation {
                                    true
                                } else {
                                    app.app_settings_save_task_running = false;
                                    false
                                }
                            })
                            .unwrap_or(false);
                        if !keep_running {
                            break;
                        }
                    }
                }
            },
        )
        .detach();
    }

    pub(crate) fn close_filter_bar(&mut self, pane_id: PaneId) {
        self.set_filter_bar_visible(pane_id, false);
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.focused = false;
            filter.query.clear();
        }
        self.invalidate_filter_projection_preserving_scroll(pane_id);
        self.set_pane_status(pane_id, "Filter closed");
    }

    fn set_filter_query(&mut self, pane_id: PaneId, query: String) {
        self.set_filter_bar_visible(pane_id, true);
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.focused = true;
        if filter.query == query {
            return;
        }
        filter.query = query;
        self.invalidate_filter_projection(pane_id);
        self.set_pane_status(pane_id, "Filtering");
    }

    pub(crate) fn toggle_filter_case_sensitive(&mut self, pane_id: PaneId) {
        self.set_filter_bar_visible(pane_id, true);
        let filter = self.pane_filters.entry(pane_id).or_default();
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

    pub(crate) fn set_filter_mode(&mut self, pane_id: PaneId, mode: fika_core::NameFilterMode) {
        self.set_filter_bar_visible(pane_id, true);
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.focused = true;
        if filter.mode == mode {
            return;
        }
        filter.mode = mode;
        self.invalidate_filter_projection(pane_id);
        let message = match mode {
            fika_core::NameFilterMode::PlainText => "Plain text filter",
            fika_core::NameFilterMode::Glob => "Glob pattern filter",
        };
        self.set_pane_status(pane_id, message);
    }

    fn clear_filter_query_for_pane(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.query.clear();
        }
        self.invalidate_filter_projection_preserving_scroll(pane_id);
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
        self.visible_work_keys.remove(&pane_id);
    }

    fn invalidate_filter_projection(&mut self, pane_id: PaneId) {
        self.invalidate_pane_layout_projection(pane_id, true);
    }

    fn invalidate_filter_projection_preserving_scroll(&mut self, pane_id: PaneId) {
        self.invalidate_pane_layout_projection(pane_id, false);
    }

    fn invalidate_pane_layout_projection(&mut self, pane_id: PaneId, reset_scroll: bool) {
        self.clear_file_grid_projection_state(pane_id);
        self.filtered_models.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
        if reset_scroll {
            if let Some(pane) = self.panes.pane_mut(pane_id) {
                pane.view.reset_scroll();
            }
            self.reset_item_view_scroll_for_pane(pane_id);
        }
    }

    pub(crate) fn set_hovered_item(&mut self, pane_id: PaneId, item_id: ItemId) -> bool {
        self.hovered_item.set(pane_id, item_id)
    }

    pub(crate) fn clear_hovered_item(&mut self, pane_id: PaneId, item_id: ItemId) -> bool {
        self.hovered_item.clear_item(pane_id, item_id)
    }

    pub(crate) fn clear_hovered_item_for_pane(&mut self, pane_id: PaneId) -> bool {
        self.hovered_item.clear_pane(pane_id)
    }

    fn item_view_scroll_handle_for_pane(&mut self, pane_id: PaneId) -> ScrollHandle {
        item_view_scroll_handle_for_pane_state(&mut self.item_view_scroll, pane_id)
    }

    fn sync_pane_view_from_item_view_scroll_handle(&mut self, pane_id: PaneId) -> bool {
        sync_item_view_pane_from_scroll_handle(&mut self.item_view_scroll, &mut self.panes, pane_id)
    }

    fn sync_pane_view_from_authoritative_item_view_scroll_handle(
        &mut self,
        pane_id: PaneId,
    ) -> bool {
        sync_item_view_pane_from_authoritative_scroll_handle(
            &mut self.item_view_scroll,
            &mut self.panes,
            pane_id,
        )
    }

    pub(crate) fn begin_item_view_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        begin_item_view_scrollbar_drag_state(&mut self.item_view_scroll, pane_id)
    }

    pub(crate) fn update_item_view_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        self.sync_pane_view_from_authoritative_item_view_scroll_handle(pane_id)
    }

    pub(crate) fn finish_item_view_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        finish_item_view_scrollbar_drag_state(&mut self.item_view_scroll, &mut self.panes, pane_id)
    }

    fn preserve_item_view_scroll_for_layout_change(&mut self, pane_id: PaneId) {
        preserve_item_view_scroll_for_layout_change_state(
            &mut self.item_view_scroll,
            &mut self.panes,
            pane_id,
        );
    }

    fn reset_item_view_scroll_for_pane(&mut self, pane_id: PaneId) {
        reset_item_view_scroll_for_pane_state(&mut self.item_view_scroll, pane_id);
    }

    fn sync_item_view_scroll_handle_to_pane_view(&mut self, pane_id: PaneId) {
        let _ = sync_item_view_handle_to_pane_view_state(
            &mut self.item_view_scroll,
            &self.panes,
            pane_id,
        );
    }

    fn remove_item_view_scroll_for_pane(&mut self, pane_id: PaneId) {
        remove_item_view_scroll_for_pane_state(&mut self.item_view_scroll, pane_id);
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
        let perf_enabled = item_view_perf_enabled();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| {
                let pane_started = perf_enabled.then(Instant::now);
                self.sync_pane_view_from_item_view_scroll_handle(pane_id);
                let scroll_handle = self.item_view_scroll_handle_for_pane(pane_id);
                let filtered_model = self.filtered_model_for_pane(pane_id);
                let split_ratio = self.pane_split_ratio(pane_id);
                let item_drop_target = self.drop_targets.item().cloned();
                let pane_drop_target =
                    item_drop_target_matches_pane(item_drop_target.as_ref(), pane_id);
                let rename_draft = self
                    .rename_draft
                    .as_ref()
                    .filter(|draft| draft.pane_id == pane_id)
                    .cloned();
                let location_draft = self
                    .location_draft
                    .as_ref()
                    .filter(|draft| draft.pane_id == pane_id)
                    .map(LocationDraft::snapshot);
                let (breadcrumbs, view, generation, focused, selection_count, trash_view) = {
                    let pane = self.panes.pane(pane_id)?;
                    let mut view = pane.view.clone();
                    if let Some(projected_viewport_width) =
                        self.projected_item_viewport_width(pane_id, view.view_mode)
                        && projected_viewport_width > 0.0
                    {
                        view.viewport_width = projected_viewport_width;
                    }
                    (
                        breadcrumb_segments(&pane.current_dir),
                        view.clone(),
                        pane.generation,
                        focused_pane == Some(pane_id),
                        pane.selection.count_for_model(pane.model.len()),
                        file_ops::is_trash_files_dir(&pane.current_dir),
                    )
                };
                let filtered = filtered_model.as_ref().map(|(model, _)| model);
                let source_revision = filtered_model.as_ref().map_or(0, |(_, revision)| *revision);
                let file_grid_frame = self.pane_file_grid_render_frame_for_pane(
                    pane_id,
                    generation,
                    &view,
                    selection_count,
                    filtered,
                    source_revision,
                    rename_draft.as_ref(),
                    item_drop_target.as_ref(),
                    VISIBLE_METADATA_ROLE_SYNC_BUDGET,
                    perf_enabled,
                    cx,
                )?;
                let item_count = file_grid_frame.item_count;
                let rubber_band = self
                    .rubber_band
                    .active_viewport_rect_for_pane(pane_id, &view);
                let filter_bar = self.filter_bar_snapshot(pane_id, focused_pane, item_count);
                let status_bar = self.status_bar_snapshot_for_pane(pane_id, cx);
                if let Some(pane_started) = pane_started {
                    file_grid_frame.emit_perf_log(pane_id, view.view_mode, pane_started.elapsed());
                }
                Some(PaneSnapshot {
                    id: pane_id,
                    split_ratio,
                    breadcrumbs,
                    location_draft,
                    filter_bar,
                    status_bar,
                    file_grid: file_grid_frame.file_grid,
                    theme_icon_readiness: self.theme_icon_readiness.snapshot(),
                    trash_view,
                    scroll_handle,
                    view,
                    rubber_band,
                    drop_target: pane_drop_target,
                    focused,
                })
            })
            .collect()
    }

    fn start_listing_result_monitor(receiver: mpsc::Receiver<()>, cx: &mut Context<Self>) {
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let mut receiver = receiver;
                    loop {
                        let (next_receiver, connected) = cx
                            .background_spawn(async move {
                                let connected = receiver.recv().is_ok();
                                if connected {
                                    while receiver.try_recv().is_ok() {}
                                }
                                (receiver, connected)
                            })
                            .await;
                        receiver = next_receiver;
                        if !connected {
                            break;
                        }
                        if this
                            .update(&mut cx, |app, cx| {
                                if app.drain_background_listing_results() {
                                    cx.notify();
                                }
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
    }

    fn status_bar_snapshot_for_pane(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> StatusBarSnapshot {
        let now = Instant::now();
        let message = self.status_message_for_pane(pane_id);
        let loading_pending = self.loading_panes.contains_key(&pane_id);
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
                loading_pending,
                operation_progress: self.loading_progress_snapshot(pane_id, now),
            };
        };

        self.request_space_info_if_needed(path.clone(), cx);
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
            loading_pending,
            operation_progress: self.loading_progress_snapshot(pane_id, now),
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

    fn clear_loading_status_for_path(&mut self, pane_id: PaneId, path: &Path) {
        if self
            .pane_statuses
            .get(&pane_id)
            .is_some_and(|message| pane_loading_status_matches_path(message, path))
        {
            self.pane_statuses.remove(&pane_id);
        }
    }

    fn event_finishes_current_loading(&self, event: &DirectoryListerEvent) -> bool {
        if !matches!(
            event,
            DirectoryListerEvent::ListingCompleted { .. }
                | DirectoryListerEvent::CurrentDirectoryRemoved { .. }
                | DirectoryListerEvent::Error { .. }
                | DirectoryListerEvent::NetworkAuthRequired { .. }
        ) {
            return false;
        }

        self.loading_panes
            .get(&event.pane_id())
            .is_some_and(|loading| {
                loading.key.generation == event.generation()
                    && loading.key.request_serial == event.request_serial()
            })
    }

    fn finish_current_loading_status(
        &mut self,
        event: &DirectoryListerEvent,
        finishes_current_loading: bool,
    ) {
        if !finishes_current_loading {
            return;
        }

        match event {
            DirectoryListerEvent::ListingCompleted { pane_id, path, .. }
            | DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } => {
                self.clear_loading_status_for_path(*pane_id, path);
            }
            DirectoryListerEvent::Error {
                pane_id,
                path,
                message,
                ..
            } => {
                if self
                    .pane_statuses
                    .get(pane_id)
                    .is_some_and(|status| pane_loading_status_matches_path(status, path))
                {
                    self.set_pane_status(
                        *pane_id,
                        format!("Cannot load {}: {message}", path.display()),
                    );
                }
            }
            DirectoryListerEvent::NetworkAuthRequired { pane_id, path, .. } => {
                if self
                    .pane_statuses
                    .get(pane_id)
                    .is_some_and(|status| pane_loading_status_matches_path(status, path))
                {
                    self.set_pane_status(
                        *pane_id,
                        format!("Authentication required for {}", path.display()),
                    );
                }
            }
            _ => {}
        }
    }

    fn begin_pane_operation(
        &mut self,
        pane_id: PaneId,
        message: impl Into<String>,
    ) -> Option<BackgroundTaskId> {
        self.begin_operation(Operation::External {
            pane_id,
            title: message.into(),
            detail: None,
            cancellable: false,
        })
    }

    fn begin_privileged_operation(
        &mut self,
        pane_id: PaneId,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Option<BackgroundTaskId> {
        self.begin_operation(Operation::External {
            pane_id,
            title: title.into(),
            detail: Some(detail.into()),
            cancellable: false,
        })
    }

    fn begin_operation(&mut self, operation: Operation) -> Option<BackgroundTaskId> {
        let pane_id = operation.pane_id();
        match OperationRuntime::shared() {
            Ok(runtime) => {
                let handle = runtime.register_operation(operation);
                Some(handle.id)
            }
            Err(err) => {
                self.set_pane_status(pane_id, format!("Cannot start operation: {err}"));
                None
            }
        }
    }

    fn operation_snapshots(&self) -> Vec<OperationSnapshot> {
        OperationRuntime::shared()
            .map(OperationRuntime::active_operations)
            .unwrap_or_default()
    }

    fn operation_progress_snapshot_from_operation(
        operation: &OperationSnapshot,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        progress_delay_elapsed(operation.started_at, now).then(|| OperationProgressSnapshot {
            label: operation.operation.progress_label(),
            bytes_done: operation.progress.bytes_done,
            bytes_total: operation.progress.bytes_total,
            percent: progress_percent(
                operation.progress.bytes_done,
                operation.progress.bytes_total,
            ),
            cancellable: operation.operation.cancellable(),
        })
    }

    fn background_task_snapshot(
        &self,
        operation: OperationSnapshot,
        now: Instant,
    ) -> BackgroundTaskSnapshot {
        let progress = Self::operation_progress_snapshot_from_operation(&operation, now);
        let percent = progress.as_ref().and_then(|progress| progress.percent);
        let title = if operation.cancelled && operation.operation.cancellable() {
            format!("Cancelling {}", operation.operation.progress_label())
        } else if operation.operation.cancellable() {
            operation.operation.progress_label()
        } else {
            operation.operation.title()
        };
        let detail = match percent {
            Some(percent) => format!("{percent}% complete"),
            None if let Some(detail) = operation.operation.detail() => detail.to_string(),
            None if progress.is_some() => "Working".to_string(),
            None if operation.operation.cancellable() => "Preparing".to_string(),
            None => "Working".to_string(),
        };
        BackgroundTaskSnapshot {
            id: operation.id,
            pane_id: operation.operation.pane_id(),
            title,
            detail,
            percent,
            cancellable: operation.operation.cancellable(),
        }
    }

    fn finish_pane_operation(
        &mut self,
        task_id: BackgroundTaskId,
        pane_id: PaneId,
        message: impl Into<String>,
    ) {
        let message = message.into();
        self.record_background_task_result(task_id, &message, &message);
        self.set_pane_status(pane_id, message);
    }

    fn finish_pane_operation_with_detail(
        &mut self,
        task_id: BackgroundTaskId,
        pane_id: PaneId,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let message = message.into();
        let detail = detail.into();
        self.record_background_task_result(task_id, &message, &detail);
        self.set_pane_status(pane_id, message);
    }

    fn finish_privileged_task(&mut self, task_id: BackgroundTaskId, result: PrivilegedTaskResult) {
        if !result.affected_dirs.is_empty() {
            self.refresh_affected_dirs(&result.affected_dirs);
        }
        if result.clear_clipboard && result.failure_count == 0 && result.success_count > 0 {
            self.clipboard = None;
            self.rubber_band
                .clear_selection_activity_for_pane(result.pane_id);
            let _ = self.panes.clear_selection(result.pane_id);
        }
        let status = match (result.success_count, result.failure_count) {
            (0, 0) => format!("{}: no changes", result.title),
            (_, 0) => format!("{}: {} operation(s)", result.title, result.success_count),
            (0, _) => format!(
                "{} failed for {} operation(s)",
                result.title, result.failure_count
            ),
            (_, _) => format!(
                "{}: {} operation(s), {} failed",
                result.title, result.success_count, result.failure_count
            ),
        };
        self.finish_pane_operation_with_detail(task_id, result.pane_id, status, result.detail);
    }

    fn run_privileged_commands(
        &mut self,
        pane_id: PaneId,
        title: impl Into<String>,
        commands: Vec<PrivilegedCommand>,
        clear_clipboard: bool,
        cx: &mut Context<Self>,
    ) {
        if commands.is_empty() {
            self.set_pane_status(pane_id, "No administrator operation to run");
            return;
        }
        for command in &commands {
            if let Err(err) = command.validate_local_paths() {
                self.set_pane_status(pane_id, err);
                return;
            }
        }

        let title = title.into();
        let summaries = commands
            .iter()
            .map(PrivilegedCommand::summary)
            .collect::<Vec<_>>()
            .join("\n");
        let detail = format!("Waiting for administrator authorization.\n{summaries}");
        let Some(task_id) = self.begin_privileged_operation(pane_id, title.clone(), detail) else {
            return;
        };
        self.set_pane_status(
            pane_id,
            format!("Waiting for administrator authorization: {title}"),
        );

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let fallback_title = title.clone();
                    let result =
                        match run_registered_operation(task_id, move |_controller| async move {
                            privileged_task_result_for_commands(
                                pane_id,
                                title,
                                commands,
                                clear_clipboard,
                            )
                            .await
                        })
                        .await
                        {
                            Ok(result) => result,
                            Err(err) => PrivilegedTaskResult {
                                pane_id,
                                title: fallback_title.clone(),
                                success_count: 0,
                                failure_count: 1,
                                affected_dirs: Vec::new(),
                                clear_clipboard: false,
                                detail: format!(
                                    "{fallback_title}\nCould not start administrator task: {err}"
                                ),
                            },
                        };
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_privileged_task(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn background_tasks_snapshot(&self, now: Instant) -> Option<BackgroundTasksSnapshot> {
        let active = self.background_task_snapshots(now);
        let history = self
            .background_task_history
            .iter()
            .map(|record| BackgroundTaskHistorySnapshot {
                title: record.title.clone(),
                detail: record.detail.clone(),
                state: record.state,
            })
            .collect::<Vec<_>>();
        if active.is_empty() && history.is_empty() {
            return None;
        }
        Some(BackgroundTasksSnapshot {
            active,
            history,
            expanded: self.background_tasks_expanded,
        })
    }

    fn background_task_snapshots(&self, now: Instant) -> Vec<BackgroundTaskSnapshot> {
        self.operation_snapshots()
            .into_iter()
            .map(|operation| self.background_task_snapshot(operation, now))
            .collect()
    }

    fn record_background_task_result(
        &mut self,
        task_id: BackgroundTaskId,
        message: &str,
        detail: &str,
    ) {
        let title = OperationRuntime::shared()
            .ok()
            .and_then(|runtime| runtime.complete_operation(task_id))
            .map(|operation| operation.operation.progress_label())
            .unwrap_or_else(|| message.to_string());
        self.background_task_history
            .push_front(BackgroundTaskHistoryRecord {
                title,
                detail: detail.to_string(),
                state: background_task_state_for_message(message),
            });
        while self.background_task_history.len() > BACKGROUND_TASK_HISTORY_LIMIT {
            self.background_task_history.pop_back();
        }
    }

    pub(crate) fn toggle_background_tasks_details(&mut self) {
        self.background_tasks_expanded = !self.background_tasks_expanded;
    }

    pub(crate) fn clear_background_task_history(&mut self) {
        self.background_task_history.clear();
        if self.operation_snapshots().is_empty() {
            self.background_tasks_expanded = false;
        }
    }

    pub(crate) fn show_background_task_detail_dialog(
        &mut self,
        title: String,
        detail: String,
        state: Option<BackgroundTaskState>,
    ) {
        self.background_task_detail_dialog = Some(BackgroundTaskDetailDialog {
            title,
            detail,
            state,
        });
    }

    pub(crate) fn dismiss_background_task_detail_dialog(&mut self) {
        self.background_task_detail_dialog = None;
    }

    fn has_active_background_task_for_pane(&self, pane_id: PaneId) -> bool {
        self.operation_snapshots()
            .iter()
            .any(|operation| operation.operation.pane_id() == pane_id)
    }

    #[cfg(test)]
    fn operation_progress_snapshot_for_pane(
        &self,
        pane_id: PaneId,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        self.operation_snapshots()
            .iter()
            .filter(|operation| operation.operation.pane_id() == pane_id)
            .find_map(|operation| Self::operation_progress_snapshot_from_operation(operation, now))
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

    pub(crate) fn cancel_operation_or_loading(&mut self, pane_id: PaneId) {
        let Some(operation) = self.operation_snapshots().into_iter().find(|operation| {
            operation.operation.pane_id() == pane_id && operation.operation.cancellable()
        }) else {
            self.cancel_loading(pane_id);
            return;
        };
        if let Ok(runtime) = OperationRuntime::shared() {
            runtime.cancel_operation(operation.id);
        }
        self.set_pane_status(
            pane_id,
            format!("Cancelling {}", operation.operation.progress_label()),
        );
    }

    pub(crate) fn cancel_background_operation(&mut self, task_id: BackgroundTaskId) {
        if let Ok(runtime) = OperationRuntime::shared() {
            runtime.cancel_operation(task_id);
        }
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
            let model_generation = if selection_count == 0 {
                pane.model.structure_generation()
            } else {
                pane.model.data_generation()
            };
            let key = StatusSummaryCacheKey {
                model_generation,
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
                        .background_spawn(async move { fika_core::watch_devices(sender).await })
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

    fn set_trash_has_items(&mut self, has_items: bool) -> bool {
        self.trash_monitor.set_known_state(has_items);
        if self.trash_has_items == has_items {
            return false;
        }
        self.trash_has_items = has_items;
        self.refresh_context_menu_trash_state();
        true
    }

    fn refresh_trash_emptiness_state(&mut self) -> bool {
        let has_items = self
            .trash_monitor
            .refresh()
            .unwrap_or_else(|| self.trash_monitor.has_items());
        self.set_trash_has_items(has_items)
    }

    fn start_trash_monitor(&mut self) {
        if let Err(err) = self.trash_monitor.start()
            && let Some(pane_id) = self.panes.focused()
        {
            self.set_pane_status(pane_id, format!("Cannot watch Trash: {err}"));
        }
        self.set_trash_has_items(self.trash_monitor.has_items());
    }

    fn drain_trash_monitor(&mut self) -> bool {
        let changes = self.trash_monitor.drain_changes();
        changes.into_iter().fold(false, |changed, has_items| {
            self.set_trash_has_items(has_items) | changed
        })
    }

    fn refresh_context_menu_trash_state(&mut self) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        match &mut menu.target {
            ContextMenuTarget::Blank {
                trash_view: true,
                trash_has_items,
                ..
            }
            | ContextMenuTarget::Place {
                trash_place: true,
                trash_has_items,
                ..
            } => *trash_has_items = self.trash_has_items,
            _ => {}
        }
    }

    fn update_trash_emptiness_state_from_lister_event(
        &mut self,
        event: &DirectoryListerEvent,
    ) -> bool {
        if !file_ops::is_trash_files_dir(event.path()) {
            return false;
        }
        let pane_has_items = self.panes.pane(event.pane_id()).and_then(|pane| {
            event
                .matches_target(pane.id, pane.generation, &pane.current_dir)
                .then(|| !pane.model.is_empty())
        });
        match event {
            DirectoryListerEvent::ItemsAdded { entries, .. } if !entries.is_empty() => {
                self.set_trash_has_items(true)
            }
            DirectoryListerEvent::ItemsAdded { .. } => false,
            DirectoryListerEvent::ListingRefreshed { entries, .. } => {
                self.set_trash_has_items(!entries.is_empty())
            }
            DirectoryListerEvent::ItemsDeleted { .. }
            | DirectoryListerEvent::ItemsRefreshed { .. }
            | DirectoryListerEvent::ListingCompleted { .. } => pane_has_items
                .map(|has_items| self.set_trash_has_items(has_items))
                .unwrap_or(false),
            DirectoryListerEvent::CurrentDirectoryRemoved { .. }
            | DirectoryListerEvent::Error { .. }
            | DirectoryListerEvent::NetworkAuthRequired { .. } => {
                self.refresh_trash_emptiness_state()
            }
            DirectoryListerEvent::LoadingStarted { .. } => false,
        }
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
        device_id: Option<String>,
        label: String,
        mounted: bool,
        device: bool,
        _network: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        if mounted {
            self.open_place(path);
        } else if device && let Some(device_id) = device_id {
            self.run_device_place_operation(
                pane_id,
                device_id,
                label,
                DevicePlaceOperation::Mount,
                cx,
            );
        }
    }

    fn run_device_place_operation(
        &mut self,
        pane_id: PaneId,
        device_id: String,
        label: String,
        operation: DevicePlaceOperation,
        cx: &mut Context<Self>,
    ) {
        let Some(task_id) =
            self.begin_pane_operation(pane_id, operation.in_progress_message(&label))
        else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            perform_device_place_operation(pane_id, device_id, label, operation)
                                .await
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_device_place_operation(task_id, result, cx);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_device_place_operation(
        &mut self,
        task_id: BackgroundTaskId,
        result: DevicePlaceOperationResult,
        cx: &mut Context<Self>,
    ) {
        match result.result {
            Ok(Some(mount_point)) => {
                self.finish_pane_operation(
                    task_id,
                    result.pane_id,
                    result.operation.success_message(&result.label),
                );
                self.request_device_snapshot_refresh(cx);
                self.load_pane(result.pane_id, mount_point);
            }
            Ok(None) => {
                self.finish_pane_operation(
                    task_id,
                    result.pane_id,
                    result.operation.success_message(&result.label),
                );
                self.request_device_snapshot_refresh(cx);
            }
            Err(error) => self.finish_pane_operation(
                task_id,
                result.pane_id,
                result.operation.error_message(&result.label, &error),
            ),
        }
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
                label: place.label,
                path: place.path,
                device_id: place.device_id,
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

    pub(crate) fn open_directory_from_item(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        is_dir_hint: bool,
    ) -> bool {
        if !self.item_path_is_directory(pane_id, &path, is_dir_hint) {
            return false;
        }

        self.load_pane(pane_id, path);
        true
    }

    pub(crate) fn open_default_application_for_item(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mime_type) = self.mime_type_for_pane_path(pane_id, &path) else {
            self.set_pane_status(
                pane_id,
                format!("No application found for {}", path.display()),
            );
            return true;
        };
        let applications = self.mime_applications.applications_for_mime(&mime_type);
        let Some(desktop_id) =
            default_open_with_application_id(&applications).map(ToOwned::to_owned)
        else {
            self.set_pane_status(
                pane_id,
                format!("No application found for {}", path.display()),
            );
            return true;
        };
        self.open_with_application(pane_id, &desktop_id, path, cx);
        true
    }

    pub(crate) fn item_path_is_directory(
        &self,
        pane_id: PaneId,
        path: &Path,
        is_dir_hint: bool,
    ) -> bool {
        is_dir_hint
            || self
                .panes
                .pane(pane_id)
                .and_then(|pane| {
                    pane.model
                        .index_of_path(path)
                        .and_then(|index| pane.model.get(index))
                })
                .is_some_and(|entry| entry.is_dir)
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
        self.begin_pane_loading_transition(pane_id, PaneLoadingScrollPolicy::Reset);
        if url_changed {
            self.clear_filter_query_for_url_change(pane_id);
        }
        let cached_events = self.schedule_listing(&event);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
    }

    fn reload_pane(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.reload(pane_id) else {
            return;
        };
        let path = event.path().to_path_buf();
        self.begin_pane_loading_transition(pane_id, PaneLoadingScrollPolicy::Preserve);
        let cached_events = self.schedule_listing(&event);
        self.set_pane_status(pane_id, format!("Reloading {}", path.display()));
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
    }

    fn go_back(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.go_back(pane_id) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id, PaneLoadingScrollPolicy::Reset);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
    }

    fn go_forward(&mut self, pane_id: PaneId) {
        let previous_summary = self.status_summary_for_pane(pane_id);
        let Some(event) = self.panes.go_forward(pane_id) else {
            return;
        };
        self.begin_pane_loading_transition(pane_id, PaneLoadingScrollPolicy::Reset);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
        self.apply_event_with_previous_summary(event, previous_summary);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
    }

    fn go_parent(&mut self, pane_id: PaneId) {
        let Some(parent) = self
            .panes
            .pane(pane_id)
            .and_then(|pane| parent_location(&pane.current_dir))
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
        let closing_snapshot = match self.panes.pane(pane_id) {
            Some(pane) if self.listing_worker.can_cache_entry_count(pane.model.len()) => Some((
                pane.model.directory().to_path_buf(),
                pane.model.listing_snapshot(),
            )),
            Some(pane) => {
                let path = pane.model.directory().to_path_buf();
                let entry_count = pane.model.len();
                self.listing_worker
                    .record_uncached_directory(&path, entry_count);
                self.log_listing_cache_debug(&format!("close-pane-uncached {}", path.display()));
                None
            }
            None => None,
        };
        if self.panes.close(pane_id) {
            if let Some((path, entries)) = closing_snapshot {
                if self.listing_worker.cache_listing_snapshot(&path, entries) {
                    self.log_listing_cache_debug(&format!("close-pane-cached {}", path.display()));
                }
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
        self.clear_file_grid_projection_state(pane_id);
        self.status_summaries.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.loading_panes.remove(&pane_id);
        self.remove_item_view_scroll_for_pane(pane_id);
        self.cancel_metadata_role_work_for_pane(pane_id);
        self.cancel_thumbnail_work_for_pane(pane_id);
        self.pane_viewport_geometries.remove(&pane_id);
        self.rubber_band.clear_selection_activity_for_pane(pane_id);
        self.pane_statuses.remove(&pane_id);
        self.location_edit_metrics.remove(&pane_id);
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload.source_pane == pane_id)
        {
            self.active_item_drag = None;
            self.drop_targets.clear_without_touch();
        }
        if self.drop_targets.item().is_some_and(|target| match target {
            ItemDropTarget::Pane {
                pane_id: target_pane,
                ..
            }
            | ItemDropTarget::Directory {
                pane_id: target_pane,
                ..
            } => *target_pane == pane_id,
        }) {
            self.drop_targets.clear_item();
        }
        self.rubber_band.clear_active_for_pane(pane_id);
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.pane_id == pane_id)
        {
            self.dismiss_context_menu();
        }
        self.clear_trash_conflict_dialog_for_pane(pane_id);
        self.properties_dialog = None;
        self.clear_application_chooser_for_pane(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.clear_network_auth_draft_for_pane(pane_id);
    }

    fn begin_pane_loading_transition(
        &mut self,
        pane_id: PaneId,
        scroll_policy: PaneLoadingScrollPolicy,
    ) {
        self.status_summaries.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.visible_work_keys.remove(&pane_id);
        match scroll_policy {
            PaneLoadingScrollPolicy::Reset => self.reset_item_view_scroll_for_pane(pane_id),
            PaneLoadingScrollPolicy::Preserve => {
                self.sync_item_view_scroll_handle_to_pane_view(pane_id)
            }
        }
        self.cancel_stale_metadata_role_work_for_pane(pane_id);
        self.cancel_stale_thumbnail_work_for_pane(pane_id);
        self.location_edit_metrics.remove(&pane_id);
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload.source_pane == pane_id)
        {
            self.active_item_drag = None;
            self.drop_targets.clear_without_touch();
        }
        if self.drop_targets.item().is_some_and(|target| match target {
            ItemDropTarget::Pane {
                pane_id: target_pane,
                ..
            }
            | ItemDropTarget::Directory {
                pane_id: target_pane,
                ..
            } => *target_pane == pane_id,
        }) {
            self.drop_targets.clear_item();
        }
        self.rubber_band.clear_active_for_pane(pane_id);
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.pane_id == pane_id)
        {
            self.dismiss_context_menu();
        }
        self.clear_trash_conflict_dialog_for_pane(pane_id);
        self.properties_dialog = None;
        self.clear_application_chooser_for_pane(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.clear_network_auth_draft_for_pane(pane_id);
    }

    fn clear_pane_lifecycle_state(&mut self, pane_id: PaneId) {
        self.clear_pane_content_state(pane_id);
        self.pane_split_ratios.remove(&pane_id);
    }

    fn select_only(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.select_only(pane_id, path) {
            self.rubber_band.clear_selection_activity_for_pane(pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.toggle_selection(pane_id, path).is_some() {
            self.rubber_band.clear_selection_activity_for_pane(pane_id);
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
            self.rubber_band.clear_selection_activity_for_pane(pane_id);
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
            self.rubber_band.clear_selection_activity_for_pane(pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn clear_selection(&mut self, pane_id: PaneId) {
        self.rubber_band.clear_selection_activity_for_pane(pane_id);
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
            self.rubber_band.clear_selection_activity_for_pane(pane_id);
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn rubber_band_selection_active(&self, pane_id: PaneId) -> bool {
        self.rubber_band
            .selection_activity_is_active(pane_id, self.panes.selected_count(pane_id))
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

    #[cfg(test)]
    fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) {
        self.apply_zoom_change_impl(pane_id, change);
    }

    pub(crate) fn apply_zoom_change_with_context(
        &mut self,
        pane_id: PaneId,
        change: ZoomChange,
        _cx: &mut Context<Self>,
    ) {
        self.apply_zoom_change_impl(pane_id, change);
    }

    fn apply_zoom_change_impl(&mut self, pane_id: PaneId, change: ZoomChange) {
        let Some(previous_view) = self.panes.pane(pane_id).map(|pane| pane.view.clone()) else {
            return;
        };
        let next_level = zoom_level_after_change(previous_view.zoom_level, change);
        if next_level == previous_view.zoom_level {
            self.set_pane_status(
                pane_id,
                format!(
                    "Zoom level {} ({} px)",
                    previous_view.zoom_level,
                    previous_view.icon_size() as i32
                ),
            );
            return;
        }
        self.preserve_item_view_scroll_for_layout_change(pane_id);
        let Some(view) = self.panes.set_zoom_level(pane_id, next_level) else {
            return;
        };
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    pub(crate) fn zoom_pane_from_wheel(
        &mut self,
        pane_id: PaneId,
        delta: ScrollDelta,
        cx: &mut Context<Self>,
    ) {
        if let Some(change) = zoom_change_for_wheel_delta(delta) {
            self.finish_rubber_band(pane_id);
            self.apply_zoom_change_with_context(pane_id, change, cx);
        }
    }

    pub(crate) fn scroll_pane_from_wheel(&mut self, pane_id: PaneId, delta: ScrollDelta) -> bool {
        scroll_item_view_pane_from_wheel(
            &mut self.item_view_scroll,
            &mut self.panes,
            pane_id,
            delta,
        )
    }

    #[cfg(test)]
    pub(crate) fn set_zoom_level(&mut self, pane_id: PaneId, level: i32) {
        self.set_zoom_level_impl(pane_id, level);
    }

    pub(crate) fn set_zoom_level_with_context(
        &mut self,
        pane_id: PaneId,
        level: i32,
        _cx: &mut Context<Self>,
    ) {
        self.set_zoom_level_impl(pane_id, level);
    }

    fn set_zoom_level_impl(&mut self, pane_id: PaneId, level: i32) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level) else {
            return;
        };
        let next_level = level.clamp(fika_core::MIN_ZOOM_LEVEL, fika_core::MAX_ZOOM_LEVEL);
        if next_level != previous_level {
            self.preserve_item_view_scroll_for_layout_change(pane_id);
        }
        let Some(view) = self.panes.set_zoom_level(pane_id, next_level) else {
            return;
        };
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    fn set_pane_view_mode(&mut self, pane_id: PaneId, view_mode: ViewMode) {
        let Some(previous_mode) = self.panes.pane(pane_id).map(|pane| pane.view.view_mode) else {
            return;
        };
        if previous_mode == view_mode {
            self.set_pane_status(pane_id, view_mode_status(view_mode));
            return;
        }

        self.finish_rubber_band(pane_id);
        let Some(view) = self.panes.set_view_mode(pane_id, view_mode) else {
            return;
        };
        self.prime_pane_viewport_for_view_mode_axis_change(pane_id, previous_mode, view_mode);
        self.reset_item_view_scroll_for_pane(pane_id);
        self.clear_file_grid_mode_switch_state(pane_id);
        self.set_pane_status(pane_id, view_mode_status(view.view_mode));
    }

    fn prime_pane_viewport_for_view_mode_axis_change(
        &mut self,
        pane_id: PaneId,
        previous_mode: ViewMode,
        next_mode: ViewMode,
    ) {
        let Some(pane) = self.panes.pane_mut(pane_id) else {
            return;
        };
        let Some((viewport_width, viewport_height)) = viewport_extents_after_view_mode_axis_change(
            pane.view.viewport_width,
            pane.view.viewport_height,
            previous_mode,
            next_mode,
        ) else {
            return;
        };
        pane.view.viewport_width = viewport_width;
        pane.view.viewport_height = viewport_height;
        pane.view.max_scroll_x = 0.0;
        pane.view.max_scroll_y = 0.0;
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

    pub(crate) fn projected_item_viewport_width(
        &self,
        pane_id: PaneId,
        view_mode: ViewMode,
    ) -> Option<f32> {
        let pane_width = self.projected_pane_width(pane_id)?;
        Some(projected_item_viewport_width_for_pane_width(
            pane_width,
            view_mode,
            PANE_HORIZONTAL_BORDER_EXTENT,
        ))
    }

    fn prime_pane_viewports_for_window_resize(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        let prime = window_resize_viewport_prime(
            self.last_render_viewport_size,
            viewport_width,
            viewport_height,
        );
        self.last_render_viewport_size = Some((prime.viewport_width, prime.viewport_height));
        let Some(resize) = prime.resize else {
            return;
        };

        if resize.width_changed && self.pane_row_width > 0.0 {
            self.pane_row_width = resize.apply_width_delta(self.pane_row_width);
        }

        if resize.height_changed {
            for pane_id in self.panes.pane_ids().to_vec() {
                if let Some(pane) = self.panes.pane_mut(pane_id) {
                    pane.view.viewport_height =
                        resize.apply_height_delta(pane.view.viewport_height);
                }
            }
        }

        if resize.width_changed {
            let projected_widths = self
                .panes
                .pane_ids()
                .iter()
                .filter_map(|pane_id| {
                    let view_mode = self.panes.pane(*pane_id)?.view.view_mode;
                    self.projected_item_viewport_width(*pane_id, view_mode)
                        .map(|width| (*pane_id, width))
                })
                .collect::<Vec<_>>();
            for (pane_id, viewport_width) in projected_widths {
                if let Some(pane) = self.panes.pane_mut(pane_id) {
                    pane.view.viewport_width = viewport_width;
                }
            }
        }
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

    fn request_pane_resize_notify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_resize_notify_pending {
            return;
        }
        self.pane_resize_notify_pending = true;
        cx.on_next_frame(window, |this, _window, cx| {
            this.pane_resize_notify_pending = false;
            cx.notify();
        });
    }

    fn set_pane_viewport_bounds(
        &mut self,
        pane_id: PaneId,
        viewport_width: f32,
        viewport_height: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> bool {
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
        let scroll_changed = sync_item_view_pane_after_bounds_update(
            &mut self.item_view_scroll,
            &mut self.panes,
            pane_id,
        );
        changed || scroll_changed
    }

    fn content_point_from_window(
        &self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ViewPoint> {
        let geometry = *self.pane_viewport_geometries.get(&pane_id)?;
        let view = &self.panes.pane(pane_id)?.view;
        content_point_from_window_position(geometry, view, position)
    }

    fn pane_at_window_position(&self, position: gpui::Point<gpui::Pixels>) -> Option<PaneId> {
        pane_at_window_position(
            self.panes.pane_ids(),
            &self.pane_viewport_geometries,
            position,
        )
    }

    pub(crate) fn window_position_is_in_pane_viewport(
        &self,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.pane_at_window_position(position).is_some()
    }

    fn clamped_content_point_from_window(
        &self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ViewPoint> {
        let geometry = *self.pane_viewport_geometries.get(&pane_id)?;
        let view = &self.panes.pane(pane_id)?.view;
        Some(clamped_content_point_from_window_position(
            geometry, view, position,
        ))
    }

    fn with_pane_layout_projection_input<R>(
        &mut self,
        pane_id: PaneId,
        f: impl FnOnce(PaneLayoutProjectionInput<'_>) -> R,
    ) -> Option<R> {
        let filtered_model = self.filtered_model_for_pane(pane_id);
        let pane = self.panes.pane(pane_id)?;
        let filtered = filtered_model.as_ref().map(|(filtered, _)| filtered);
        let source_revision = filtered_model.as_ref().map_or(0, |(_, revision)| *revision);
        let rename_draft = self
            .rename_draft
            .as_ref()
            .filter(|draft| draft.pane_id == pane_id);
        Some(f(PaneLayoutProjectionInput {
            model: &pane.model,
            view: &pane.view,
            filtered,
            source_revision,
            rename_draft,
            trash_view: file_ops::is_trash_files_dir(&pane.current_dir),
            compact_column_widths: self.compact_column_widths.entry(pane_id).or_default(),
        }))
    }

    fn layout_projection_for_pane(&mut self, pane_id: PaneId) -> Option<PaneLayoutProjection> {
        self.with_pane_layout_projection_input(pane_id, pane_layout_projection)
    }

    fn item_at_content_point(
        &mut self,
        pane_id: PaneId,
        point: ViewPoint,
    ) -> Option<ContentItemHit> {
        self.with_pane_layout_projection_input(pane_id, |input| {
            pane_content_item_hit_at_point(input, point)
        })?
    }

    fn indexes_intersecting_visual_rect(&mut self, pane_id: PaneId, rect: ViewRect) -> Vec<usize> {
        self.with_pane_layout_projection_input(pane_id, |input| {
            pane_model_indexes_intersecting_visual_rect(input, rect)
        })
        .unwrap_or_default()
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

    fn press_rubber_band_from_blank(&mut self, pane_id: PaneId, start: ViewPoint) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        if self.item_at_content_point(pane_id, start).is_some() {
            return false;
        }
        self.clear_selection_from_blank(pane_id);
        self.rubber_band.press_pending_for_pane(pane_id, start);
        true
    }

    pub(crate) fn press_rubber_band_from_window_if_blank(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(start) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        self.press_rubber_band_from_blank(pane_id, start)
    }

    pub(crate) fn activate_pending_rubber_band_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(current) = self.clamped_content_point_from_window(pane_id, position) else {
            return false;
        };
        let Some(start) = self.rubber_band.pending_activation_start(pane_id, current) else {
            return false;
        };
        self.start_rubber_band(pane_id, start);
        self.update_rubber_band(pane_id, current);
        true
    }

    pub(crate) fn update_rubber_band_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        if !self.rubber_band.active_is_for_pane(pane_id) {
            return false;
        }
        let Some(current) = self.clamped_content_point_from_window(pane_id, position) else {
            return false;
        };
        self.update_rubber_band(pane_id, current);
        true
    }

    pub(crate) fn move_rubber_band_drag_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        if self.rubber_band.active_is_for_pane(pane_id) {
            self.update_rubber_band_from_window(pane_id, position)
        } else {
            self.activate_pending_rubber_band_from_window(pane_id, position)
        }
    }

    pub(crate) fn window_position_is_blank_in_pane(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.item_at_window_position(pane_id, position).is_none()
    }

    pub(crate) fn item_at_window_position(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ContentItemHit> {
        let point = self.content_point_from_window(pane_id, position)?;
        self.item_at_content_point(pane_id, point)
    }

    fn dragged_paths_drop_target_from_window_position(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
        source_paths: &[PathBuf],
    ) -> Option<PathListDropTarget> {
        let source_paths = normalized_drag_paths(source_paths.to_vec());
        if source_paths.is_empty() {
            return None;
        }
        let target =
            self.path_list_drop_target_candidate_from_window_position(pane_id, position)?;
        if item_drop_reject_reason(&source_paths, target.target_dir()).is_some() {
            return None;
        }
        Some(target)
    }

    fn path_list_drop_target_candidate_from_window_position(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<PathListDropTarget> {
        match self.item_at_window_position(pane_id, position) {
            Some(hit) if hit.is_dir => Some(PathListDropTarget::directory(pane_id, hit.path)),
            _ => {
                let target_dir = self
                    .panes
                    .pane(pane_id)
                    .map(|pane| pane.current_dir.clone())?;
                Some(PathListDropTarget::pane(pane_id, target_dir))
            }
        }
    }

    pub(crate) fn update_dragged_paths_drop_target_from_window_position(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
        source_paths: &[PathBuf],
    ) -> PathListDropTargetUpdate {
        let Some(target) =
            self.dragged_paths_drop_target_from_window_position(pane_id, position, source_paths)
        else {
            return PathListDropTargetUpdate {
                changed: self.clear_drag_drop_targets(),
                kind: None,
            };
        };
        let kind = target.kind();
        PathListDropTargetUpdate {
            changed: self.set_path_list_drop_target(target),
            kind: Some(kind),
        }
    }

    pub(crate) fn update_dragged_paths_drop_target_from_any_window_position(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        source_paths: &[PathBuf],
    ) -> (Option<PaneId>, PathListDropTargetUpdate) {
        let Some(pane_id) = self.pane_at_window_position(position) else {
            return (
                None,
                PathListDropTargetUpdate {
                    changed: self.clear_item_drop_target(),
                    kind: None,
                },
            );
        };
        (
            Some(pane_id),
            self.update_dragged_paths_drop_target_from_window_position(
                pane_id,
                position,
                source_paths,
            ),
        )
    }

    fn set_path_list_drop_target(&mut self, target: PathListDropTarget) -> bool {
        self.drop_targets.set_item(target.into_item_target())
    }

    fn start_rubber_band(&mut self, pane_id: PaneId, start: ViewPoint) {
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.rubber_band.start_active_for_pane(pane_id, start);
    }

    fn update_rubber_band(&mut self, pane_id: PaneId, current: ViewPoint) {
        let Some(band) = self.rubber_band.update_active_for_pane(pane_id, current) else {
            return;
        };
        let selection = self.indexes_intersecting_visual_rect(pane_id, band.rect());
        if let Some(selected) = self
            .panes
            .replace_selection_by_indexes(pane_id, selection.iter().copied())
        {
            self.rubber_band
                .set_selection_activity_for_count(pane_id, selected);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn finish_rubber_band(&mut self, pane_id: PaneId) {
        let _ = self.rubber_band.finish_for_pane(pane_id);
    }

    fn clear_rename_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .rename_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.rename_draft = None;
        }
        if self
            .rename_next_after_operation
            .as_ref()
            .is_some_and(|(pending_pane_id, _)| *pending_pane_id == pane_id)
        {
            self.rename_next_after_operation = None;
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
        clear_place_draft_state_for_pane(&mut self.place_draft, pane_id);
    }

    fn clear_network_auth_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .network_auth_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.network_auth_draft = None;
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

    fn start_add_network_drive(&mut self, pane_id: PaneId) {
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: None,
            focus: PlaceDraftField::Path,
            label: "Network Drive".to_string(),
            path: "smb://server/share/".to_string(),
        });
        self.set_pane_status(pane_id, "Adding network drive");
    }

    fn handle_place_draft_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(draft_pane_id) = self.place_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        let result = {
            let Some(draft) = &mut self.place_draft else {
                return false;
            };
            apply_place_input_action(draft, place_input_action(keystroke))
        };

        match result {
            PlaceDraftInputResult::Cancel => {
                self.place_draft = None;
                self.set_pane_status(draft_pane_id, "Place edit cancelled");
            }
            PlaceDraftInputResult::Commit => self.commit_place_draft(),
            PlaceDraftInputResult::Edited => {}
            PlaceDraftInputResult::Ignore => return false,
        }
        true
    }

    fn handle_network_auth_draft_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        if self.network_auth_draft.is_none() {
            return false;
        }

        let result = {
            let Some(draft) = &mut self.network_auth_draft else {
                return false;
            };
            apply_network_auth_input_action(draft, place_input_action(keystroke))
        };

        match result {
            NetworkAuthInputResult::Cancel => self.dismiss_network_auth_draft(),
            NetworkAuthInputResult::Commit => self.commit_network_auth_draft(),
            NetworkAuthInputResult::Edited => {}
            NetworkAuthInputResult::Ignore => {}
        }
        true
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
        self.rename_draft = Some(RenameDraft::new(
            pane_id,
            original_path.clone(),
            name.to_string(),
        ));
        self.set_pane_status(pane_id, format!("Renaming {name}"));
    }

    fn start_rename_as_administrator_in_pane(&mut self, pane_id: PaneId) {
        if self.chooser.is_some() {
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        let [original_path] = selected_paths.as_slice() else {
            self.set_pane_status(pane_id, "Select one item to rename as administrator");
            return;
        };
        if is_network_path(original_path) {
            self.set_pane_status(
                pane_id,
                "Remote rename is not available with administrator privileges",
            );
            return;
        }
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
        self.rename_draft = Some(RenameDraft::new_privileged(
            pane_id,
            original_path.clone(),
            name.to_string(),
        ));
        self.set_pane_status(pane_id, format!("Renaming {name} as administrator"));
    }

    pub(crate) fn set_rename_caret_from_window_position(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        let Some((original_path, draft_name)) = self
            .rename_draft
            .as_ref()
            .filter(|draft| draft.pane_id == pane_id)
            .map(|draft| (draft.original_path.clone(), draft.draft_name.clone()))
        else {
            return false;
        };
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        let Some(projection) = self.layout_projection_for_pane(pane_id) else {
            return false;
        };
        let Some((model_index, name_width_units)) = self.panes.pane(pane_id).and_then(|pane| {
            let model_index = pane.model.index_of_path(&original_path)?;
            let entry = pane.model.get(model_index)?;
            Some((model_index, entry.name_width_units))
        }) else {
            return false;
        };
        let Some(layout_index) = projection.layout_index_for_model_index(model_index) else {
            return false;
        };
        let required_text_width = compact_text_width(name_width_units).max(
            rename_editor_required_text_width(compact_text_width_for_name(&draft_name)),
        );
        let Some(item) = projection
            .layout
            .item_with_required_text_width(layout_index, Some(required_text_width))
        else {
            return false;
        };
        let local_x =
            (point.x - item.text_rect.x - RENAME_TEXT_INSET_X).clamp(0.0, item.text_rect.width);
        let Some(draft) = self
            .rename_draft
            .as_mut()
            .filter(|draft| draft.pane_id == pane_id)
            .filter(|draft| draft.original_path == original_path)
            .filter(|draft| draft.draft_name == draft_name)
        else {
            return false;
        };
        draft.set_caret_from_local_x(local_x);
        true
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
                self.clear_pending_rename_next_for_pane(draft_pane_id);
                self.set_pane_status(draft_pane_id, "Rename cancelled");
            }
            RenameInputAction::Commit => self.commit_rename_draft(cx),
            RenameInputAction::CommitAndRenameNext => self.commit_rename_draft_and_rename_next(cx),
            RenameInputAction::MoveStart => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.move_to_start();
                }
            }
            RenameInputAction::MoveEnd => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.move_to_end();
                }
            }
            RenameInputAction::MoveBackward => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.move_backward();
                }
            }
            RenameInputAction::MoveForward => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.move_forward();
                }
            }
            RenameInputAction::SelectAll => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.select_all();
                }
            }
            RenameInputAction::SelectStart => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.select_to_start();
                }
            }
            RenameInputAction::SelectEnd => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.select_to_end();
                }
            }
            RenameInputAction::SelectBackward => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.select_backward();
                }
            }
            RenameInputAction::SelectForward => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.select_forward();
                }
            }
            RenameInputAction::Backspace => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.delete_backward();
                    draft.error = None;
                }
            }
            RenameInputAction::Delete => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.delete_forward();
                    draft.error = None;
                }
            }
            RenameInputAction::Insert(text) => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.insert(&text);
                    draft.error = None;
                }
            }
            RenameInputAction::Ignore => {}
        }
        true
    }

    fn commit_rename_draft_and_rename_next(&mut self, cx: &mut Context<Self>) {
        let Some((pane_id, original_path)) = self
            .rename_draft
            .as_ref()
            .map(|draft| (draft.pane_id, draft.original_path.clone()))
        else {
            return;
        };
        self.rename_next_after_operation = self
            .next_rename_path_after(pane_id, &original_path)
            .map(|path| (pane_id, path));
        self.commit_rename_draft(cx);
        if self.rename_draft.is_some() {
            self.clear_pending_rename_next_for_pane(pane_id);
            return;
        }
        if !self.has_active_background_task_for_pane(pane_id) {
            self.start_pending_rename_next_for_pane(pane_id);
        }
    }

    fn commit_rename_draft(&mut self, cx: &mut Context<Self>) {
        let Some((draft_pane_id, original_path, new_name)) =
            self.rename_draft.as_ref().map(|draft| {
                (
                    draft.pane_id,
                    draft.original_path.clone(),
                    draft.draft_name.trim().to_string(),
                )
            })
        else {
            return;
        };
        if new_name.is_empty() {
            self.set_rename_draft_error(draft_pane_id, "Name cannot be empty");
            return;
        }
        if is_network_path(&original_path) {
            self.set_rename_draft_error(draft_pane_id, "Remote rename is not available yet");
            return;
        }
        if original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == new_name)
        {
            self.rename_draft = None;
            let _ = self.panes.select_only(draft_pane_id, original_path.clone());
            self.set_pane_status(draft_pane_id, "Rename unchanged");
            return;
        }
        let Some(parent) = original_path.parent() else {
            self.set_rename_draft_error(draft_pane_id, "Item has no parent folder");
            return;
        };
        let destination = parent.join(&new_name);
        if destination != original_path && file_ops::path_exists(&destination) {
            self.set_rename_draft_error(draft_pane_id, "An item with that name already exists");
            return;
        }

        let Some(draft) = self.rename_draft.take() else {
            return;
        };
        let pane_id = draft.pane_id;
        let original_path = draft.original_path.clone();
        if draft.privileged {
            self.clear_pending_rename_next_for_pane(pane_id);
            self.run_privileged_commands(
                pane_id,
                "Administrator: Rename",
                vec![PrivilegedCommand::Rename {
                    path: original_path,
                    new_name,
                }],
                false,
                cx,
            );
            return;
        }
        let Some(task_id) = self.begin_operation(Operation::Rename {
            pane_id,
            path: original_path.clone(),
        }) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let fallback_path = original_path.clone();
                    let result =
                        match run_registered_operation(task_id, move |_controller| async move {
                            rename_item_result_async(pane_id, original_path, new_name).await
                        })
                        .await
                        {
                            Ok(result) => result,
                            Err(err) => RenameItemResult {
                                pane_id,
                                original_path: fallback_path.clone(),
                                affected_dirs: fika_core::parent_dirs([fallback_path.clone()]),
                                result: Err(err.to_string()),
                            },
                        };
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_rename_item(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn set_rename_draft_error(&mut self, pane_id: PaneId, message: impl Into<String>) {
        let message = message.into();
        if let Some(draft) = &mut self.rename_draft
            && draft.pane_id == pane_id
        {
            draft.error = Some(message.clone());
        }
        self.set_pane_status(pane_id, message);
    }

    fn next_rename_path_after(&self, pane_id: PaneId, path: &Path) -> Option<PathBuf> {
        let pane = self.panes.pane(pane_id)?;
        let index = pane.model.index_of_path(path)?;
        pane.model.path_for_index(index + 1)
    }

    fn start_rename_for_path(&mut self, pane_id: PaneId, path: PathBuf) -> bool {
        if !self.panes.select_only(pane_id, path) {
            return false;
        }
        self.start_rename_in_pane(pane_id);
        self.rename_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
    }

    fn start_pending_rename_next_for_pane(&mut self, pane_id: PaneId) {
        let Some((pending_pane_id, path)) = self.rename_next_after_operation.take() else {
            return;
        };
        if pending_pane_id != pane_id {
            self.rename_next_after_operation = Some((pending_pane_id, path));
            return;
        }
        if !self.start_rename_for_path(pane_id, path) {
            self.set_pane_status(pane_id, "Next rename item is no longer available");
        }
    }

    fn clear_pending_rename_next_for_pane(&mut self, pane_id: PaneId) {
        if self
            .rename_next_after_operation
            .as_ref()
            .is_some_and(|(pending_pane_id, _)| *pending_pane_id == pane_id)
        {
            self.rename_next_after_operation = None;
        }
    }

    fn finish_rename_item(&mut self, task_id: BackgroundTaskId, result: RenameItemResult) {
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
                    task_id,
                    result.pane_id,
                    format!("Renamed to {}", renamed_path.display()),
                );
                self.start_pending_rename_next_for_pane(result.pane_id);
            }
            Err(err) => {
                self.clear_pending_rename_next_for_pane(result.pane_id);
                self.finish_pane_operation(
                    task_id,
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
        if is_network_path(&parent_dir) {
            self.set_pane_status(pane_id, "Remote item creation is not available yet");
            return;
        }
        let Some(task_id) = self.begin_operation(Operation::Create { pane_id, kind }) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let fallback_parent = parent_dir.clone();
                    let result =
                        match run_registered_operation(task_id, move |_controller| async move {
                            create_item_result_async(pane_id, parent_dir, kind).await
                        })
                        .await
                        {
                            Ok(result) => result,
                            Err(err) => CreateItemResult {
                                pane_id,
                                kind,
                                affected_dirs: vec![fallback_parent],
                                result: Err(err.to_string()),
                            },
                        };
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_create_item(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn create_item_in_directory_as_administrator(
        &mut self,
        pane_id: PaneId,
        parent_dir: PathBuf,
        kind: CreatedItemKind,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if is_network_path(&parent_dir) {
            self.set_pane_status(
                pane_id,
                "Remote item creation is not available with administrator privileges",
            );
            return;
        }
        let name = default_created_item_name(kind).to_string();
        let command = match kind {
            CreatedItemKind::Folder => PrivilegedCommand::CreateFolder {
                parent: parent_dir,
                name,
            },
            CreatedItemKind::File => PrivilegedCommand::CreateFile {
                parent: parent_dir,
                name,
            },
        };
        self.run_privileged_commands(
            pane_id,
            format!("Administrator: Create {}", created_item_label(kind)),
            vec![command],
            false,
            cx,
        );
    }

    fn finish_create_item(&mut self, task_id: BackgroundTaskId, result: CreateItemResult) {
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
                self.finish_pane_operation(
                    task_id,
                    result.pane_id,
                    format!("Created {}", path.display()),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    task_id,
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
        if mode == ClipboardMode::Cut && paths.iter().any(|path| is_network_path(path)) {
            self.set_pane_status(pane_id, "Remote cut is not available yet");
            return;
        }
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
        self.import_system_clipboard(cx);
        let Some(clipboard) = self.clipboard.clone() else {
            self.set_pane_status(pane_id, "Nothing to paste");
            return;
        };
        self.start_clipboard_transfer(pane_id, target_dir, clipboard, cx);
    }

    fn paste_into_directory_as_administrator(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        self.import_system_clipboard(cx);
        let Some(clipboard) = self.clipboard.clone() else {
            self.set_pane_status(pane_id, "Nothing to paste as administrator");
            return;
        };
        if clipboard.text.is_some() {
            self.set_pane_status(
                pane_id,
                "Pasting text as administrator is not available yet",
            );
            return;
        }
        if is_network_path(&target_dir) || clipboard.paths.iter().any(|path| is_network_path(path))
        {
            self.set_pane_status(
                pane_id,
                "Remote paste is not available with administrator privileges",
            );
            return;
        }
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot paste into {}", target_dir.display()),
            );
            return;
        }
        let mode = clipboard.mode.transfer_mode();
        let operation = mode.operation().to_string();
        let commands = clipboard
            .paths
            .iter()
            .cloned()
            .map(|source| PrivilegedCommand::Transfer {
                operation: operation.clone(),
                source,
                target_dir: target_dir.clone(),
            })
            .collect::<Vec<_>>();
        self.run_privileged_commands(
            pane_id,
            format!("Administrator: {}", clipboard.action_label()),
            commands,
            clipboard.mode == ClipboardMode::Cut,
            cx,
        );
    }

    fn start_clipboard_transfer(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        clipboard: ClipboardState,
        cx: &mut Context<Self>,
    ) {
        if is_network_path(&target_dir) || clipboard.paths.iter().any(|path| is_network_path(path))
        {
            self.set_pane_status(pane_id, "Remote paste is not available yet");
            return;
        }
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot paste into {}", target_dir.display()),
            );
            return;
        }

        let operation = if clipboard.text.is_some() {
            Operation::PasteText { pane_id }
        } else {
            Operation::Transfer {
                pane_id,
                mode: clipboard.mode.transfer_mode(),
                item_count: clipboard.paths.len(),
                label: clipboard.action_label().to_string(),
                clear_clipboard: clipboard.mode == ClipboardMode::Cut,
            }
        };
        let Some(task_id) = self.begin_operation(operation) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let fallback_target = target_dir.clone();
                    let fallback_mode = if clipboard.text.is_some() {
                        FileTransferMode::Copy
                    } else {
                        clipboard.mode.transfer_mode()
                    };
                    let fallback_label = clipboard.action_label();
                    let fallback_clear_clipboard = clipboard.mode == ClipboardMode::Cut;
                    let result =
                        match run_registered_operation(task_id, move |controller| async move {
                            paste_clipboard_result_async(
                                pane_id,
                                target_dir,
                                clipboard,
                                Some(controller),
                            )
                            .await
                        })
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => TransferTaskResult {
                                pane_id,
                                mode: fallback_mode,
                                label: fallback_label,
                                clear_clipboard: fallback_clear_clipboard,
                                success_count: 0,
                                failure_count: 1,
                                affected_dirs: Vec::new(),
                                refresh_dirs: vec![fallback_target],
                                undo_items: Vec::new(),
                                created_items: Vec::new(),
                            },
                        };
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_transfer(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    pub(crate) fn begin_item_drag(&mut self, payload: ItemDragPayload) {
        let paths = normalized_drag_paths(item_drag_paths(&self.panes, &payload));
        let export = item_drag_export_payload(&self.panes, &payload);
        self.active_item_drag = Some(ActiveItemDrag {
            payload,
            paths,
            export,
        });
        self.drop_targets.clear_without_touch();
    }

    fn active_item_drag_paths(&self, payload: &ItemDragPayload) -> Option<Vec<PathBuf>> {
        self.active_item_drag
            .as_ref()
            .filter(|drag| drag.payload == *payload)
            .map(|drag| drag.paths.clone())
    }

    pub(crate) fn item_drag_source_paths(&self, payload: &ItemDragPayload) -> Vec<PathBuf> {
        self.active_item_drag_paths(payload)
            .unwrap_or_else(|| normalized_drag_paths(item_drag_paths(&self.panes, payload)))
    }

    pub(crate) fn update_active_item_drag_drop_target_from_window_position(
        &mut self,
        source_pane: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<(Option<PaneId>, PathListDropTargetUpdate, Vec<PathBuf>)> {
        let paths = self
            .active_item_drag
            .as_ref()
            .filter(|drag| drag.payload.source_pane == source_pane)
            .map(|drag| drag.paths.clone())?;
        let (target_pane, update) =
            self.update_dragged_paths_drop_target_from_any_window_position(position, &paths);
        Some((target_pane, update, paths))
    }

    pub(crate) fn external_drag_source_paths(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        normalized_drag_paths(paths.to_vec())
    }

    pub(crate) fn dragged_paths_can_add_place(&self, paths: &[PathBuf]) -> bool {
        let paths = normalized_drag_paths(paths.to_vec());
        let [path] = paths.as_slice() else {
            return false;
        };
        path.is_dir() && !self.places.iter().any(|place| place.path == *path)
    }

    fn clear_item_drag(&mut self, payload: &ItemDragPayload) {
        if self
            .active_item_drag
            .as_ref()
            .is_some_and(|drag| drag.payload == *payload)
        {
            self.active_item_drag = None;
        }
    }

    fn window_position_to_view_point(position: gpui::Point<gpui::Pixels>) -> ViewPoint {
        ViewPoint {
            x: position.x.as_f32(),
            y: position.y.as_f32(),
        }
    }

    pub(crate) fn set_dragged_paths_drop_target_for_directory(
        &mut self,
        pane_id: PaneId,
        source_paths: &[PathBuf],
        target_dir: PathBuf,
    ) -> bool {
        if item_drop_reject_reason(source_paths, &target_dir).is_some() {
            return self.clear_drag_drop_targets();
        }
        self.set_path_list_drop_target(PathListDropTarget::directory(pane_id, target_dir))
    }

    fn clear_item_drop_target(&mut self) -> bool {
        self.drop_targets.clear_item()
    }

    pub(crate) fn clear_item_drop_target_for_pane(&mut self, pane_id: PaneId) -> bool {
        self.drop_targets.clear_item_for_pane(pane_id)
    }

    pub(crate) fn clear_item_drop_target_for_directory(
        &mut self,
        pane_id: PaneId,
        path: &Path,
    ) -> bool {
        self.drop_targets.clear_item_for_directory(pane_id, path)
    }

    pub(crate) fn clear_drag_drop_targets(&mut self) -> bool {
        self.drop_targets.clear_all()
    }

    pub(crate) fn refresh_drop_target_lease(&mut self, cx: &mut Context<Self>) {
        if self.drop_target_lease_timer_running || !self.drop_targets.has_target() {
            return;
        }
        self.drop_target_lease_timer_running = true;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        let Ok(generation) =
                            this.update(&mut cx, |app, _cx| app.drop_targets.lease_generation())
                        else {
                            break;
                        };
                        cx.background_executor()
                            .timer(DROP_TARGET_LEASE_TIMEOUT)
                            .await;
                        let Ok(keep_running) = this.update(&mut cx, |app, cx| {
                            if app.drop_targets.lease_generation() == generation {
                                let changed =
                                    app.clear_drop_targets_for_lease_generation(generation);
                                app.drop_target_lease_timer_running = false;
                                if changed {
                                    cx.notify();
                                }
                                false
                            } else if app.drop_targets.has_target() {
                                true
                            } else {
                                app.drop_target_lease_timer_running = false;
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

    fn clear_drop_targets_for_lease_generation(&mut self, generation: u64) -> bool {
        self.drop_targets.clear_for_lease_generation(generation)
    }

    fn take_active_item_drag_paths_for_drop(
        &mut self,
        payload: &ItemDragPayload,
        status_pane: PaneId,
    ) -> Option<Vec<PathBuf>> {
        let Some(paths) = self.active_item_drag_paths(payload) else {
            self.clear_item_drag(payload);
            self.clear_drag_drop_targets();
            self.set_pane_status(status_pane, "No active item drag");
            return None;
        };
        self.clear_item_drag(payload);
        Some(paths)
    }

    fn show_path_drop_operation_menu_for_paths(
        &mut self,
        status_pane: PaneId,
        target_dir: PathBuf,
        paths: Vec<PathBuf>,
        load_target_dir: bool,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let paths = normalized_drag_paths(paths);
        if self.chooser.is_some() {
            self.clear_drag_drop_targets();
            return;
        }
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.clear_drag_drop_targets();
            self.set_pane_status(status_pane, reason);
            return;
        }

        self.show_drop_operation_menu(status_pane, target_dir, paths, load_target_dir, position);
        cx.notify();
    }

    pub(crate) fn drop_item_drag_to_pane(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(target_dir) = self
            .panes
            .pane(target_pane)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.drop_item_drag_to_directory(target_pane, payload, target_dir, false, position, cx);
    }

    pub(crate) fn drop_item_drag_to_position_in_pane(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let source_paths = self.item_drag_source_paths(&payload);
        let Some(target) =
            self.path_list_drop_target_candidate_from_window_position(target_pane, position)
        else {
            return;
        };
        let kind = target.kind();
        let target_dir = target.target_dir().to_path_buf();
        match kind {
            PathListDropTargetKind::Directory => {
                let _ = self.set_dragged_paths_drop_target_for_directory(
                    target_pane,
                    &source_paths,
                    target_dir.clone(),
                );
                self.drop_item_drag_to_directory(
                    target_pane,
                    payload,
                    target_dir,
                    false,
                    position,
                    cx,
                );
            }
            PathListDropTargetKind::Pane => {
                let _ = self.set_path_list_drop_target(target);
                self.drop_item_drag_to_pane(target_pane, payload, position, cx);
            }
        }
    }

    pub(crate) fn drop_external_paths_to_pane(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(target_dir) = self
            .panes
            .pane(target_pane)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.drop_external_paths_to_directory(target_pane, paths, target_dir, false, position, cx);
    }

    pub(crate) fn drop_external_paths_to_position_in_pane(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let paths = normalized_drag_paths(paths);
        let Some(target) =
            self.path_list_drop_target_candidate_from_window_position(target_pane, position)
        else {
            return;
        };
        let kind = target.kind();
        let target_dir = target.target_dir().to_path_buf();
        match kind {
            PathListDropTargetKind::Directory => {
                let _ = self.set_dragged_paths_drop_target_for_directory(
                    target_pane,
                    &paths,
                    target_dir.clone(),
                );
                self.drop_external_paths_to_directory(
                    target_pane,
                    paths,
                    target_dir,
                    false,
                    position,
                    cx,
                );
            }
            PathListDropTargetKind::Pane => {
                let _ = self.set_path_list_drop_target(target);
                self.drop_external_paths_to_pane(target_pane, paths, position, cx);
            }
        }
    }

    pub(crate) fn drop_external_paths_to_directory(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        target_dir: PathBuf,
        load_target_dir: bool,
        position: gpui::Point<gpui::Pixels>,
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
        self.show_path_drop_operation_menu_for_paths(
            target_pane,
            target_dir,
            paths,
            load_target_dir,
            position,
            cx,
        );
    }

    pub(crate) fn drop_item_drag_to_location(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        target_dir: PathBuf,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drag(&payload);
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        let Some(paths) = self.take_active_item_drag_paths_for_drop(&payload, target_pane) else {
            return;
        };
        self.clear_place_drop_target();
        self.show_path_drop_operation_menu_for_paths(
            target_pane,
            target_dir,
            paths,
            true,
            position,
            cx,
        );
    }

    pub(crate) fn drop_external_paths_to_location(
        &mut self,
        target_pane: PaneId,
        paths: Vec<PathBuf>,
        target_dir: PathBuf,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            self.clear_item_drop_target();
            self.clear_place_drop_target();
            return;
        }
        self.clear_place_drop_target();
        self.show_path_drop_operation_menu_for_paths(
            target_pane,
            target_dir,
            paths,
            true,
            position,
            cx,
        );
    }

    pub(crate) fn drop_item_drag_to_directory(
        &mut self,
        target_pane: PaneId,
        payload: ItemDragPayload,
        target_dir: PathBuf,
        load_target_dir: bool,
        position: gpui::Point<gpui::Pixels>,
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

        let Some(paths) = self.take_active_item_drag_paths_for_drop(&payload, target_pane) else {
            return;
        };
        self.clear_place_drop_target();
        self.show_path_drop_operation_menu_for_paths(
            target_pane,
            target_dir,
            paths,
            load_target_dir,
            position,
            cx,
        );
    }

    fn show_drop_operation_menu(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        paths: Vec<PathBuf>,
        load_target_dir: bool,
        position: gpui::Point<gpui::Pixels>,
    ) {
        let position = Self::window_position_to_view_point(position);
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::DropOperation {
                target_dir,
                paths,
                load_target_dir,
            },
            position,
            active_submenu: None,
        });
    }

    fn run_drop_operation(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        paths: Vec<PathBuf>,
        mode: FileTransferMode,
        load_target_dir: bool,
        cx: &mut Context<Self>,
    ) {
        self.finish_rubber_band(pane_id);
        self.clear_drag_drop_targets();
        if self.chooser.is_some() {
            return;
        }
        if let Some(reason) = item_drop_reject_reason(&paths, &target_dir) {
            self.set_pane_status(pane_id, reason);
            return;
        }
        if load_target_dir {
            self.load_pane(pane_id, target_dir.clone());
        }
        self.start_file_transfer(pane_id, target_dir, mode, paths, cx);
    }

    fn start_file_transfer(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        mode: FileTransferMode,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if is_network_path(&target_dir) || paths.iter().any(|path| is_network_path(path)) {
            self.set_pane_status(pane_id, "Remote file transfer is not available yet");
            return;
        }
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot drop into {}", target_dir.display()),
            );
            return;
        }

        let progress_label = mode.progress_label(paths.len());
        let Some(task_id) = self.begin_pane_operation(pane_id, progress_label.clone()) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let paths_len = paths.len();
                    let fallback_dir = target_dir.clone();
                    let result =
                        match run_registered_operation(task_id, move |controller| async move {
                            transfer_paths_result_async(
                                pane_id,
                                target_dir,
                                mode,
                                paths,
                                mode.label(),
                                false,
                                Some(controller),
                            )
                            .await
                        })
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => TransferTaskResult {
                                pane_id,
                                mode,
                                label: mode.label(),
                                clear_clipboard: false,
                                success_count: 0,
                                failure_count: paths_len,
                                affected_dirs: Vec::new(),
                                refresh_dirs: vec![fallback_dir],
                                undo_items: Vec::new(),
                                created_items: Vec::new(),
                            },
                        };
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_transfer(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_transfer(&mut self, task_id: BackgroundTaskId, result: TransferTaskResult) {
        let TransferTaskResult {
            pane_id,
            mode,
            label,
            clear_clipboard,
            success_count,
            failure_count,
            affected_dirs,
            refresh_dirs,
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
            if let Some(path) = created_selection {
                self.rubber_band.clear_selection_activity_for_pane(pane_id);
                let _ = self.panes.select_only(pane_id, path);
            }
            if clear_clipboard && has_transfer_items {
                self.clipboard = None;
                self.rubber_band.clear_selection_activity_for_pane(pane_id);
                let _ = self.panes.clear_selection(pane_id);
            }
        }
        if !refresh_dirs.is_empty() {
            self.refresh_affected_dirs(&refresh_dirs);
        }

        self.finish_pane_operation(
            task_id,
            pane_id,
            action_status(&format!("{label} complete"), success_count, failure_count),
        );
    }

    fn trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, "No selection to trash");
            return;
        }
        if selected_paths.iter().any(|path| is_network_path(path)) {
            self.set_pane_status(pane_id, "Remote trash is not available yet");
            return;
        }

        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Moving {} item(s) to trash", selected_paths.len()),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let paths_len = selected_paths.len();
                    let result = run_operation_task(move || async move {
                        trash_selection_result_async(pane_id, selected_paths).await
                    })
                    .await
                    .unwrap_or_else(|_| TrashSelectionResult {
                        pane_id,
                        success_count: 0,
                        failure_count: paths_len,
                        affected_dirs: Vec::new(),
                        undo_items: Vec::new(),
                    });
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_selection(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn trash_selection_as_administrator(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, "No selection to trash as administrator");
            return;
        }
        if selected_paths.iter().any(|path| is_network_path(path)) {
            self.set_pane_status(
                pane_id,
                "Remote trash is not available with administrator privileges",
            );
            return;
        }
        self.run_privileged_commands(
            pane_id,
            "Administrator: Move to Trash",
            vec![PrivilegedCommand::Trash {
                paths: selected_paths,
            }],
            false,
            cx,
        );
    }

    fn finish_trash_selection(&mut self, task_id: BackgroundTaskId, result: TrashSelectionResult) {
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                "Move to Trash".to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Trash {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            self.rubber_band
                .clear_selection_activity_for_pane(result.pane_id);
            let _ = self.panes.clear_selection(result.pane_id);
        }

        self.finish_pane_operation(
            task_id,
            result.pane_id,
            action_status("Moved to trash", result.success_count, result.failure_count),
        );
    }

    fn restore_trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        self.start_trash_view_selection_operation(
            pane_id,
            TrashViewOperation::Restore {
                conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
            },
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
        if !self.trash_has_items {
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
        let Some(task_id) =
            self.begin_pane_operation(pane_id, operation.progress_label(paths.len()))
        else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let paths_len = paths.len();
                    let result = run_operation_task(move || async move {
                        trash_view_operation_result_async(pane_id, operation, paths).await
                    })
                    .await
                    .unwrap_or_else(|_| TrashViewOperationResult {
                        pane_id,
                        operation,
                        success_count: 0,
                        failure_count: paths_len,
                        affected_dirs: Vec::new(),
                        restore_conflicts: Vec::new(),
                    });
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_view_operation(task_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_trash_view_operation(
        &mut self,
        task_id: BackgroundTaskId,
        result: TrashViewOperationResult,
    ) {
        if result.success_count > 0 {
            self.refresh_affected_dirs(&result.affected_dirs);
            self.rubber_band
                .clear_selection_activity_for_pane(result.pane_id);
            let _ = self.panes.clear_selection(result.pane_id);
        }
        let restore_conflict_count = result.restore_conflicts.len();
        if restore_conflict_count > 0 {
            self.trash_conflict_dialog = Some(TrashConflictDialogState {
                pane_id: result.pane_id,
                conflicts: result.restore_conflicts.clone(),
            });
        }
        let failure_count = result.failure_count + restore_conflict_count;
        self.finish_pane_operation(
            task_id,
            result.pane_id,
            action_status(
                result.operation.completed_label(),
                result.success_count,
                failure_count,
            ),
        );
    }

    fn undo_latest(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
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

        let Some(task_id) = self.begin_pane_operation(pane_id, format!("Undoing {}", record.label))
        else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let fallback_record = record.clone();
                    let result = run_operation_task(move || async move {
                        undo_record_result_async(record).await
                    })
                    .await
                    .unwrap_or_else(|err| UndoTaskResult {
                        record: fallback_record,
                        result: Err(err.to_string()),
                    });
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_undo(task_id, pane_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_undo(&mut self, task_id: BackgroundTaskId, pane_id: PaneId, result: UndoTaskResult) {
        match result.result {
            Ok(message) => {
                if self
                    .operations
                    .take_latest_undo(result.record.serial)
                    .is_none()
                {
                    self.finish_pane_operation(task_id, pane_id, "Undo result is stale");
                    return;
                }
                self.refresh_affected_dirs(&result.record.affected_dirs);
                self.finish_pane_operation(
                    task_id,
                    pane_id,
                    format!("Undid {}: {message}", result.record.label),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    task_id,
                    pane_id,
                    format!("Cannot undo {}: {err}", result.record.label),
                );
            }
        }
    }

    fn refresh_affected_dirs(&mut self, affected_dirs: &[PathBuf]) {
        let touches_trash = affected_dirs
            .iter()
            .any(|path| file_ops::is_trash_files_dir(path));
        let refreshes = OperationQueue::refresh_affected_panes(&mut self.panes, affected_dirs);
        self.schedule_listings(refreshes.iter().map(|refresh| &refresh.event));
        for refresh in refreshes {
            self.apply_event(refresh.event);
            self.start_watcher(refresh.pane_id);
        }
        if touches_trash {
            self.refresh_trash_emptiness_state();
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
        let path = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
            .unwrap_or_else(|| PathBuf::from("/"));
        let open_with_apps = if trash_view {
            Vec::new()
        } else {
            self.mime_applications
                .applications_for_mime("inode/directory")
        };
        let service_actions = if trash_view {
            Vec::new()
        } else {
            self.mime_applications
                .service_actions_for_target(Some("inode/directory"), true)
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Blank {
                path,
                trash_view,
                trash_has_items,
                open_with_apps,
                service_actions,
            },
            position,
            active_submenu: None,
        });
    }

    pub(crate) fn show_item_context_menu(
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
        pane.model.get(index)?.effective_mime_type_cloned()
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
                    mime_type: entry
                        .effective_mime_type()
                        .map(|mime| mime.as_ref().to_string()),
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
                let trash_has_items = trash_view && self.trash_has_items;
                (trash_view, trash_has_items)
            })
            .unwrap_or_default()
    }

    fn dismiss_context_menu(&mut self) {
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| matches!(menu.target, ContextMenuTarget::DropOperation { .. }))
        {
            self.clear_drag_drop_targets();
        }
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
        set_place_draft_state_focus(&mut self.place_draft, field);
    }

    pub(crate) fn dismiss_network_auth_draft(&mut self) {
        if let Some(draft) = self.network_auth_draft.take() {
            self.set_pane_status(draft.pane_id, "Network authentication cancelled");
        }
    }

    pub(crate) fn set_network_auth_draft_focus(&mut self, field: NetworkAuthField) {
        if let Some(draft) = &mut self.network_auth_draft {
            draft.focus = field;
        }
    }

    pub(crate) fn commit_network_auth_draft(&mut self) {
        let Some(draft) = self.network_auth_draft.take() else {
            return;
        };
        if let Err(err) = fika_core::remember_network_auth(&draft.uri, draft.to_auth()) {
            self.set_pane_status(draft.pane_id, err.to_string());
            return;
        }
        let pane_still_at_path = self
            .panes
            .pane(draft.pane_id)
            .is_some_and(|pane| pane.current_dir == draft.path);
        if pane_still_at_path {
            self.set_pane_status(draft.pane_id, format!("Connecting to {}", draft.uri));
            self.reload_pane(draft.pane_id);
        }
    }

    pub(crate) fn dismiss_properties_dialog(&mut self) {
        self.properties_dialog = None;
    }

    pub(crate) fn dismiss_trash_conflict_dialog(&mut self) {
        self.trash_conflict_dialog = None;
    }

    fn clear_trash_conflict_dialog_for_pane(&mut self, pane_id: PaneId) {
        if self
            .trash_conflict_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.pane_id == pane_id)
        {
            self.trash_conflict_dialog = None;
        }
    }

    pub(crate) fn replace_trash_restore_conflicts(
        &mut self,
        pane_id: PaneId,
        conflicts: Vec<file_ops::TrashRestoreConflict>,
        cx: &mut Context<Self>,
    ) {
        if conflicts.is_empty() {
            self.dismiss_trash_conflict_dialog();
            return;
        }
        if self.panes.pane(pane_id).is_none() {
            self.dismiss_trash_conflict_dialog();
            return;
        }
        self.dismiss_trash_conflict_dialog();
        let paths = conflicts
            .into_iter()
            .map(|conflict| conflict.trash_path)
            .collect();
        self.start_trash_view_operation(
            pane_id,
            TrashViewOperation::Restore {
                conflict_policy: file_ops::TrashRestoreConflictPolicy::Replace,
            },
            paths,
            cx,
        );
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
            query: String::new(),
            query_caret: 0,
            query_text_rect: None,
            scroll_handle: gpui::UniformListScrollHandle::new(),
            scrollbar_drag_grab_y: None,
            set_default_on_choose: false,
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
        dedup_application_chooser_applications(applications)
    }

    fn handle_application_chooser_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(chooser) = self.application_chooser.as_mut() else {
            return false;
        };

        match application_chooser_input_action(keystroke) {
            ApplicationChooserInputAction::Cancel => {
                if chooser.query.is_empty() {
                    self.dismiss_application_chooser();
                } else {
                    chooser.clear_query();
                    chooser
                        .scroll_handle
                        .scroll_to_item_strict(0, ScrollStrategy::Top);
                }
                cx.notify();
                true
            }
            ApplicationChooserInputAction::ChooseFirst => {
                let first_application = application_chooser_filtered_applications(
                    &chooser.applications,
                    &chooser.query,
                )
                .first()
                .map(|app| app.id.clone());
                if let Some(desktop_id) = first_application {
                    self.choose_application_for_open_with(desktop_id, cx);
                }
                true
            }
            ApplicationChooserInputAction::MoveStart => {
                if chooser.move_query_caret_to_start() {
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::MoveEnd => {
                if chooser.move_query_caret_to_end() {
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::MoveBackward => {
                if chooser.move_query_caret_backward() {
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::MoveForward => {
                if chooser.move_query_caret_forward() {
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::Backspace => {
                if chooser.backspace_query() {
                    chooser
                        .scroll_handle
                        .scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::Delete => {
                if chooser.delete_query_forward() {
                    chooser
                        .scroll_handle
                        .scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::Insert(text) => {
                if chooser.insert_query_text(&text) {
                    chooser
                        .scroll_handle
                        .scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                }
                true
            }
            ApplicationChooserInputAction::PassToView | ApplicationChooserInputAction::Ignore => {
                true
            }
        }
    }

    fn choose_application_for_open_with(&mut self, desktop_id: String, cx: &mut Context<Self>) {
        if self
            .application_chooser
            .as_ref()
            .is_some_and(|chooser| chooser.set_default_on_choose)
        {
            self.set_default_open_with_application(desktop_id.clone());
        }
        let Some(chooser) = self.application_chooser.take() else {
            return;
        };
        self.open_with_application(chooser.pane_id, &desktop_id, chooser.path, cx);
    }

    fn toggle_application_chooser_set_default(&mut self) {
        if let Some(chooser) = &mut self.application_chooser
            && chooser.mime_type.is_some()
        {
            chooser.set_default_on_choose = !chooser.set_default_on_choose;
        }
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
            ContextMenuTarget::PlacesBlank { .. }
            | ContextMenuTarget::PlaceSection { .. }
            | ContextMenuTarget::DropOperation { .. } => return,
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
                ContextMenuAction::DropCopy,
                ContextMenuTarget::DropOperation {
                    target_dir,
                    paths,
                    load_target_dir,
                },
            ) => self.run_drop_operation(
                menu.pane_id,
                target_dir,
                paths,
                FileTransferMode::Copy,
                load_target_dir,
                cx,
            ),
            (
                ContextMenuAction::DropMove,
                ContextMenuTarget::DropOperation {
                    target_dir,
                    paths,
                    load_target_dir,
                },
            ) => self.run_drop_operation(
                menu.pane_id,
                target_dir,
                paths,
                FileTransferMode::Move,
                load_target_dir,
                cx,
            ),
            (
                ContextMenuAction::DropLink,
                ContextMenuTarget::DropOperation {
                    target_dir,
                    paths,
                    load_target_dir,
                },
            ) => self.run_drop_operation(
                menu.pane_id,
                target_dir,
                paths,
                FileTransferMode::Link,
                load_target_dir,
                cx,
            ),
            (ContextMenuAction::DropCancel, ContextMenuTarget::DropOperation { .. }) => {
                self.clear_drag_drop_targets();
            }
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
                ContextMenuAction::OpenWithApplication { desktop_id },
                ContextMenuTarget::Blank { path, .. },
            ) => self.open_with_application(menu.pane_id, &desktop_id, path, cx),
            (
                ContextMenuAction::OtherApplication,
                ContextMenuTarget::Item {
                    path, mime_type, ..
                },
            ) => self.show_application_chooser(menu.pane_id, path, mime_type),
            (ContextMenuAction::OtherApplication, ContextMenuTarget::Blank { path, .. }) => self
                .show_application_chooser(menu.pane_id, path, Some(Arc::from("inode/directory"))),
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
                    device_id: Some(device_id),
                    label,
                    device: true,
                    ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                device_id,
                label,
                DevicePlaceOperation::Mount,
                cx,
            ),
            (
                ContextMenuAction::UnmountDevice,
                ContextMenuTarget::Place {
                    device_id: Some(device_id),
                    label,
                    device: true,
                    ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                device_id,
                label,
                DevicePlaceOperation::Unmount,
                cx,
            ),
            (
                ContextMenuAction::EjectDevice,
                ContextMenuTarget::Place {
                    device_id: Some(device_id),
                    label,
                    device: true,
                    ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                device_id,
                label,
                DevicePlaceOperation::Eject,
                cx,
            ),
            (
                ContextMenuAction::SafelyRemoveDevice,
                ContextMenuTarget::Place {
                    device_id: Some(device_id),
                    label,
                    device: true,
                    ..
                },
            ) => self.run_device_place_operation(
                menu.pane_id,
                device_id,
                label,
                DevicePlaceOperation::SafelyRemove,
                cx,
            ),
            (ContextMenuAction::AddPlace, ContextMenuTarget::PlacesBlank { .. }) => {
                self.start_add_place(menu.pane_id);
            }
            (ContextMenuAction::AddNetworkDrive, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::AddNetworkDrive, ContextMenuTarget::Place { .. }) => {
                self.start_add_network_drive(menu.pane_id);
            }
            (ContextMenuAction::AddNetworkDrive, _) => {}
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
            (ContextMenuAction::RenameAsAdministrator, ContextMenuTarget::Item { path, .. }) => {
                self.select_only(menu.pane_id, path);
                self.start_rename_as_administrator_in_pane(menu.pane_id);
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
            (ContextMenuAction::TrashAsAdministrator, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::TrashAsAdministrator, ContextMenuTarget::Blank { .. }) => {
                self.trash_selection_as_administrator(menu.pane_id, cx)
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
            (
                ContextMenuAction::CreateFolderAsAdministrator,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.create_item_in_directory_as_administrator(
                menu.pane_id,
                path,
                CreatedItemKind::Folder,
                cx,
            ),
            (
                ContextMenuAction::CreateFileAsAdministrator,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.create_item_in_directory_as_administrator(
                menu.pane_id,
                path,
                CreatedItemKind::File,
                cx,
            ),
            (ContextMenuAction::CreateFolder, ContextMenuTarget::Blank { .. }) => {
                self.create_item_in_pane(menu.pane_id, CreatedItemKind::Folder, cx)
            }
            (ContextMenuAction::CreateFile, ContextMenuTarget::Blank { .. }) => {
                self.create_item_in_pane(menu.pane_id, CreatedItemKind::File, cx)
            }
            (
                ContextMenuAction::CreateFolderAsAdministrator,
                ContextMenuTarget::Blank { path, .. },
            ) => self.create_item_in_directory_as_administrator(
                menu.pane_id,
                path,
                CreatedItemKind::Folder,
                cx,
            ),
            (
                ContextMenuAction::CreateFileAsAdministrator,
                ContextMenuTarget::Blank { path, .. },
            ) => self.create_item_in_directory_as_administrator(
                menu.pane_id,
                path,
                CreatedItemKind::File,
                cx,
            ),
            (
                ContextMenuAction::CreateFolder
                | ContextMenuAction::CreateFile
                | ContextMenuAction::CreateFolderAsAdministrator
                | ContextMenuAction::CreateFileAsAdministrator,
                ContextMenuTarget::Place { .. },
            )
            | (
                ContextMenuAction::Paste | ContextMenuAction::PasteAsAdministrator,
                ContextMenuTarget::Place { .. },
            )
            | (ContextMenuAction::SelectAll, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Refresh, ContextMenuTarget::Place { .. }) => {}
            (
                ContextMenuAction::Paste,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.paste_into_directory(menu.pane_id, path, cx),
            (
                ContextMenuAction::PasteAsAdministrator,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.paste_into_directory_as_administrator(menu.pane_id, path, cx),
            (ContextMenuAction::PasteAsAdministrator, ContextMenuTarget::Blank { path, .. }) => {
                self.paste_into_directory_as_administrator(menu.pane_id, path, cx)
            }
            (ContextMenuAction::Paste, _) => self.paste_into_pane(menu.pane_id, cx),
            (ContextMenuAction::SelectAll, _) => self.select_all(menu.pane_id),
            (ContextMenuAction::Refresh, _) => self.reload_pane(menu.pane_id),
            (ContextMenuAction::ViewCompact, _) => {
                self.set_pane_view_mode(menu.pane_id, ViewMode::Compact)
            }
            (ContextMenuAction::ViewIcons, _) => {
                self.set_pane_view_mode(menu.pane_id, ViewMode::Icons)
            }
            (ContextMenuAction::ViewDetails, _) => {
                self.set_pane_view_mode(menu.pane_id, ViewMode::Details)
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
                | ContextMenuAction::CreateFolderAsAdministrator
                | ContextMenuAction::CreateFileAsAdministrator
                | ContextMenuAction::SortBySubmenu
                | ContextMenuAction::OpenWithSubmenu
                | ContextMenuAction::ServiceMenuSubmenu
                | ContextMenuAction::ServiceMenuGroupSubmenu { .. }
                | ContextMenuAction::ViewModeSubmenu,
                _,
            ) => {}
            (ContextMenuAction::Open, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::TrashAsAdministrator, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::TrashAsAdministrator, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::RenameAsAdministrator, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Open, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::OpenInNewPane, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::OpenInNewWindow, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::TrashAsAdministrator, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::RenameAsAdministrator, ContextMenuTarget::PlaceSection { .. })
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
            | (ContextMenuAction::RenameAsAdministrator, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::RenameAsAdministrator, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::PasteAsAdministrator, _)
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
            | (ContextMenuAction::SafelyRemoveDevice, _)
            | (
                ContextMenuAction::DropCopy
                | ContextMenuAction::DropMove
                | ContextMenuAction::DropLink
                | ContextMenuAction::DropCancel,
                _,
            )
            | (_, ContextMenuTarget::DropOperation { .. }) => {}
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
        let plan = match self.new_window_launch_plan(&path) {
            Ok(plan) => plan,
            Err(err) => {
                self.set_pane_status(pane_id, format!("Cannot open new window: {err}"));
                return;
            }
        };
        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Opening new window for {}", path.display()),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_open_in_new_window(
                            task_id,
                            NewWindowLaunchResult {
                                pane_id,
                                path,
                                result,
                            },
                        );
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
        let plan = match self.open_with_launch_plan(desktop_id, &path) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        if self
            .panes
            .pane(pane_id)
            .is_some_and(|pane| pane.model.index_of_path(&path).is_some())
        {
            let _ = self.panes.select_only(pane_id, path.clone());
        }
        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Opening {} with {}", path.display(), app_name),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_open_with_application(
                            task_id,
                            OpenWithLaunchResult {
                                pane_id,
                                path,
                                app_name,
                                result,
                            },
                        );
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
        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(
                            task_id,
                            ServiceMenuLaunchResult {
                                pane_id,
                                target_label,
                                app_name,
                                result,
                            },
                        );
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
        let plan = match ark_compress_launch_plan(&paths) {
            Ok(plan) => plan,
            Err(message) => {
                self.set_pane_status(pane_id, message);
                return;
            }
        };
        let app_name = plan.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(
                            task_id,
                            ServiceMenuLaunchResult {
                                pane_id,
                                target_label,
                                app_name,
                                result,
                            },
                        );
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
        let Some(task_id) = self.begin_pane_operation(
            pane_id,
            format!("Running {} for {}", app_name, target_label),
        ) else {
            return;
        };
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = launch_with_systemd_user(plan).await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_service_menu_action(
                            task_id,
                            ServiceMenuLaunchResult {
                                pane_id,
                                target_label,
                                app_name,
                                result,
                            },
                        );
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_open_with_application(
        &mut self,
        task_id: BackgroundTaskId,
        result: OpenWithLaunchResult,
    ) {
        self.finish_pane_operation(task_id, result.pane_id, result.status_message());
    }

    fn finish_open_in_new_window(
        &mut self,
        task_id: BackgroundTaskId,
        result: NewWindowLaunchResult,
    ) {
        self.finish_pane_operation(task_id, result.pane_id, result.status_message());
    }

    fn finish_service_menu_action(
        &mut self,
        task_id: BackgroundTaskId,
        result: ServiceMenuLaunchResult,
    ) {
        self.finish_pane_operation(task_id, result.pane_id, result.status_message());
    }

    fn handle_keystroke(&mut self, event: &gpui::KeystrokeEvent, cx: &mut Context<Self>) -> bool {
        if event.keystroke.key.eq_ignore_ascii_case("escape")
            && self.background_task_detail_dialog.is_some()
        {
            self.dismiss_background_task_detail_dialog();
            return true;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.properties_dialog.is_some() {
            self.dismiss_properties_dialog();
            return true;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape")
            && self.trash_conflict_dialog.is_some()
        {
            self.dismiss_trash_conflict_dialog();
            return true;
        }
        if self.application_chooser.is_some() {
            return self.handle_application_chooser_keystroke(&event.keystroke, cx);
        }
        if self.handle_network_auth_draft_keystroke(&event.keystroke) {
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
            Some(PaneShortcut::TogglePlacesSidebar) => self.toggle_places_sidebar_from_shortcut(cx),
            Some(PaneShortcut::SplitPane) => self.split_pane(pane_id),
            Some(PaneShortcut::ClosePane) => self.close_pane(pane_id),
            Some(PaneShortcut::EditLocation) => self.start_location_edit(pane_id),
            Some(PaneShortcut::ShowFilter) => self.show_filter_bar(pane_id),
            Some(PaneShortcut::Zoom(change)) => {
                self.apply_zoom_change_with_context(pane_id, change, cx)
            }
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
        let finishes_current_loading = self.event_finishes_current_loading(&event);
        self.update_loading_state(&event, previous_summary);
        self.finish_current_loading_status(&event, finishes_current_loading);
        if let DirectoryListerEvent::NetworkAuthRequired {
            pane_id,
            path,
            uri,
            message,
            default_username,
            default_domain,
            ..
        } = &event
        {
            let still_current = self.panes.pane(*pane_id).is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            });
            if still_current {
                self.network_auth_draft = Some(NetworkAuthDraft::new(
                    *pane_id,
                    path.clone(),
                    uri.clone(),
                    message.clone(),
                    default_username.clone(),
                    default_domain.clone(),
                ));
                self.set_pane_status(*pane_id, format!("Authentication required for {uri}"));
            }
            return;
        }
        if let DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } = &event {
            self.listing_worker.remove_cached_directory(path);
            self.log_listing_cache_debug(&format!("current-directory-removed {}", path.display()));
            self.update_trash_emptiness_state_from_lister_event(&event);
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

        if self.listing_worker.apply_cache_event(&event) {
            self.log_listing_cache_debug("lister-cache-delta");
        }

        self.retarget_rename_draft_for_lister_event(&event);

        let pane_id = event.pane_id();
        let trash_state_event = event.clone();
        if let Some(signals) = self.panes.apply_lister_event(event) {
            self.update_trash_emptiness_state_from_lister_event(&trash_state_event);
            self.cache_completed_listing_from_event(&trash_state_event);
            if !signals.is_empty() {
                self.invalidate_pane_layout_projection(pane_id, false);
            }
        }
    }

    fn cache_completed_listing_from_event(&mut self, event: &DirectoryListerEvent) {
        let DirectoryListerEvent::ListingCompleted { pane_id, path, .. } = event else {
            return;
        };
        let Some(pane) = self.panes.pane(*pane_id) else {
            return;
        };
        if !event.matches_target(pane.id, pane.generation, &pane.current_dir)
            || pane.model.directory() != path
        {
            return;
        }

        let entry_count = pane.model.len();
        if self.listing_worker.can_cache_entry_count(entry_count) {
            let entries = pane.model.listing_snapshot();
            if self.listing_worker.cache_listing_snapshot(path, entries) {
                self.log_listing_cache_debug(&format!("load-completed-cached {}", path.display()));
            }
        } else if self
            .listing_worker
            .record_uncached_directory(path, entry_count)
        {
            self.log_listing_cache_debug(&format!("load-completed-uncached {}", path.display()));
        }
    }

    fn retarget_rename_draft_for_lister_event(&mut self, event: &DirectoryListerEvent) -> bool {
        let DirectoryListerEvent::ItemsRefreshed { pane_id, pairs, .. } = event else {
            return false;
        };
        let Some(pane) = self.panes.pane(*pane_id) else {
            return false;
        };
        if !event.matches_target(pane.id, pane.generation, &pane.current_dir) {
            return false;
        }
        let Some(original_path) = self
            .rename_draft
            .as_ref()
            .filter(|draft| draft.pane_id == *pane_id)
            .map(|draft| draft.original_path.clone())
        else {
            return false;
        };
        let Some(retargeted_path) = Self::rename_draft_retarget_path_from_refresh_pairs(
            &original_path,
            &pane.current_dir,
            pairs,
        ) else {
            return false;
        };
        if retargeted_path == original_path {
            return false;
        }
        let Some(draft) = self
            .rename_draft
            .as_mut()
            .filter(|draft| draft.pane_id == *pane_id)
            .filter(|draft| draft.original_path == original_path)
        else {
            return false;
        };
        draft.retarget_original_path(retargeted_path);
        true
    }

    fn rename_draft_retarget_path_from_refresh_pairs(
        original_path: &Path,
        current_dir: &Path,
        pairs: &[RefreshPair],
    ) -> Option<PathBuf> {
        pairs
            .iter()
            .find(|pair| pair.old_path == original_path)
            .and_then(|pair| {
                pair.entry
                    .as_ref()
                    .map(|entry| current_dir.join(entry.name.as_ref()))
            })
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

    fn log_listing_cache_debug(&self, reason: &str) {
        if !listing_cache_debug_enabled() {
            return;
        }
        eprintln!(
            "{}",
            listing_cache_debug_summary(
                reason,
                &self.listing_worker.cache_debug_snapshot(),
                self.listing_worker.pending_count(),
            )
        );
    }

    fn schedule_listing(&self, event: &DirectoryListerEvent) -> Option<Vec<DirectoryListerEvent>> {
        let request = ListingRequest::from_event(event)?;
        let path = request.path.clone();
        let cached_events = self.listing_worker.schedule_or_cached(request);
        let reason = if cached_events.is_some() {
            format!("load-cache-hit {}", path.display())
        } else {
            format!("load-scheduled {}", path.display())
        };
        self.log_listing_cache_debug(&reason);
        cached_events
    }

    fn schedule_listings<'a>(&self, events: impl IntoIterator<Item = &'a DirectoryListerEvent>) {
        let requests = listing_requests_from_events(events);
        if requests.is_empty() {
            return;
        }
        self.listing_worker.schedule_all(requests);
        self.log_listing_cache_debug("batch-scheduled");
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
        // Clean up stale loading_panes entries: if the listing worker has no
        // pending requests, any remaining loading_panes entries are orphans from
        // superseded listing requests whose completion events were discarded.
        if !changed && !self.loading_panes.is_empty() && self.listing_worker.pending_count() == 0 {
            for pane_id in self.loading_panes.keys().copied().collect::<Vec<_>>() {
                if let Some(path) = self
                    .panes
                    .pane(pane_id)
                    .map(|pane| pane.current_dir.clone())
                {
                    self.clear_loading_status_for_path(pane_id, &path);
                }
            }
            self.loading_panes.clear();
            changed = true;
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
        let perf_enabled = item_view_perf_enabled();
        let render_started = perf_enabled.then(Instant::now);
        let title = self
            .chooser
            .as_ref()
            .map(|chooser| chooser.title.as_str())
            .unwrap_or("Fika");
        window.set_window_title(title);
        let viewport_size = window.viewport_size();
        self.prime_pane_viewports_for_window_resize(
            viewport_size.width.as_f32(),
            viewport_size.height.as_f32(),
        );
        let places_sidebar_visible = self.places_sidebar_visible;
        let places_sidebar_width = self.places_sidebar_width;
        let places_started = perf_enabled.then(Instant::now);
        let places = self.place_snapshots();
        let places_elapsed = places_started.map(|started| started.elapsed());
        let background_tasks_started = perf_enabled.then(Instant::now);
        let background_tasks = self.background_tasks_snapshot(Instant::now());
        let background_tasks_elapsed = background_tasks_started.map(|started| started.elapsed());
        let snapshots_started = perf_enabled.then(Instant::now);
        let snapshots = self.snapshots(cx);
        let snapshots_elapsed = snapshots_started.map(|started| started.elapsed());
        let file_grid_mode =
            self.chooser
                .as_ref()
                .map_or(ui::file_grid::FileGridMode::Manager, |chooser| {
                    ui::file_grid::FileGridMode::Chooser {
                        directories: chooser.directories,
                        multiple: chooser.multiple,
                    }
                });
        let pane_ids = snapshots
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>();
        let focused_pane = self.panes.focused();
        let pane_count = pane_ids.len();
        let focused_filter_active = focused_pane.is_some_and(|pane_id| {
            self.pane_filters
                .get(&pane_id)
                .is_some_and(|filter| filter.visible)
        });
        let focused_filter_toggle = focused_pane.map(|pane_id| {
            (
                pane_id,
                filter_toggle_snapshot(&mut self.file_icons, focused_filter_active),
            )
        });
        let chooser_accept_label = self
            .chooser
            .as_ref()
            .map(|chooser| chooser.accept_label.clone());
        let split_icon = pane_split_icon_snapshot(&mut self.file_icons);
        let close_icon = pane_close_icon_snapshot(&mut self.file_icons);
        let places_panel_icon =
            places_panel_icon_snapshot(&mut self.file_icons, places_sidebar_visible);
        let pane_elements_started = perf_enabled.then(Instant::now);
        let mut pane_elements = Vec::with_capacity(pane_ids.len().saturating_mul(2));
        for (index, snapshot) in snapshots.into_iter().enumerate() {
            let left = snapshot.id;
            pane_elements.push(ui::pane::pane_view(
                ui::pane::PaneProps {
                    snapshot,
                    file_grid_mode,
                },
                window,
                cx,
            ));
            if let Some(right) = pane_ids.get(index + 1).copied() {
                pane_elements.push(pane_splitter(left, right, cx));
            }
        }
        let pane_elements_elapsed = pane_elements_started.map(|started| started.elapsed());
        let context_menu = self.context_menu.clone();
        let properties_dialog = self.properties_dialog.clone();
        let trash_conflict_dialog = self.trash_conflict_dialog.clone();
        let application_chooser = self.application_chooser.clone();
        let place_draft = self.place_draft.clone();
        let network_auth_draft = self.network_auth_draft.clone();
        let background_task_detail_dialog = self.background_task_detail_dialog.clone();
        let clipboard_available = self.clipboard.is_some();
        let context_menu_icons = context_menu
            .as_ref()
            .map(|menu| {
                context_menu_icon_snapshots(&mut self.file_icons, menu, clipboard_available)
            })
            .unwrap_or_default();
        let app = cx.weak_entity();
        let root_started = perf_enabled.then(Instant::now);
        let root = div()
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
                    .px_2()
                    .pt_2()
                    .child(places_panel_button(
                        places_sidebar_visible,
                        places_panel_icon,
                        cx,
                    ))
                    .child(div().flex_1())
                    .when_some(focused_filter_toggle, |bar, (pane_id, filter_toggle)| {
                        bar.child(
                            div()
                                .id("app-pane-toolbar")
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(ui::pane::filter_pane_button(pane_id, filter_toggle, cx))
                                .child(ui::pane::pane_layout_button(
                                    pane_id, pane_count, split_icon, close_icon, cx,
                                )),
                        )
                    })
                    .when_some(chooser_accept_label, |bar, accept_label| {
                        bar.child(
                            ui::controls::toolbar_button("choose", accept_label).on_click(
                                cx.listener(move |this, _event, _window, cx| {
                                    this.confirm_chooser();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .on_drag_move::<PlacesSidebarResizeDrag>(cx.listener(
                        |this, event: &gpui::DragMoveEvent<PlacesSidebarResizeDrag>, window, cx| {
                            if this.resize_places_sidebar_from_row_drag(
                                event.event.position.x.as_f32(),
                                event.bounds.origin.x.as_f32(),
                                cx,
                            ) {
                                this.request_pane_resize_notify(window, cx);
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .on_drop::<PlacesSidebarResizeDrag>(cx.listener(
                        |this, _drag: &PlacesSidebarResizeDrag, _window, cx| {
                            this.pane_resize_notify_pending = false;
                            cx.notify();
                            cx.stop_propagation();
                        },
                    ))
                    .when(places_sidebar_visible, |row| {
                        row.child(ui::places::places_sidebar(
                            places,
                            background_tasks,
                            places_sidebar_width,
                            window,
                            cx,
                        ))
                        .child(places_sidebar_splitter(cx))
                    })
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
                                        let pane_row_started = perf_enabled.then(Instant::now);
                                        let Some(width) = pane_row_width_from_child_bounds(&bounds)
                                        else {
                                            return;
                                        };
                                        let mut changed = false;
                                        let _ = app.update(cx, |this, cx| {
                                            if this.set_pane_row_width(width) {
                                                changed = true;
                                                cx.notify();
                                            }
                                        });
                                        if let Some(started) = pane_row_started {
                                            eprintln!(
                                                "[fika pane-row] width={} changed={} total={}us",
                                                width,
                                                changed,
                                                started.elapsed().as_micros(),
                                            );
                                        }
                                    })
                                    .id("pane-row")
                                    .flex()
                                    .flex_row()
                                    .size_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .on_drag_move::<ItemDrag>(cx.listener(
                                        |this,
                                         event: &gpui::DragMoveEvent<ItemDrag>,
                                         _window,
                                         cx| {
                                            if event.bounds.contains(&event.event.position)
                                                && this.clear_place_drop_target()
                                            {
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .on_drag_move::<ExternalPaths>(cx.listener(
                                        |this,
                                         event: &gpui::DragMoveEvent<ExternalPaths>,
                                         _window,
                                         cx| {
                                            if event.bounds.contains(&event.event.position)
                                                && this.clear_place_drop_target()
                                            {
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .on_drag_move::<PlaceDrag>(cx.listener(
                                        |this,
                                         event: &gpui::DragMoveEvent<PlaceDrag>,
                                         _window,
                                         cx| {
                                            if event.bounds.contains(&event.event.position)
                                                && this.clear_place_drop_target()
                                            {
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .on_drag_move::<PaneSplitterDrag>(cx.listener(
                                        move |this,
                                              event: &gpui::DragMoveEvent<PaneSplitterDrag>,
                                              window,
                                              cx| {
                                            let drag = *event.drag(cx);
                                            if this.resize_pane_pair_from_row_drag(
                                                drag.left,
                                                drag.right,
                                                event.event.position.x.as_f32(),
                                                event.bounds.origin.x.as_f32(),
                                                event.bounds.size.width.as_f32(),
                                            ) {
                                                this.request_pane_resize_notify(window, cx);
                                            }
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .on_drop::<PaneSplitterDrag>(cx.listener(
                                        |this, _drag: &PaneSplitterDrag, _window, cx| {
                                            this.pane_resize_notify_pending = false;
                                            cx.notify();
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
            .when_some(trash_conflict_dialog, |root, dialog| {
                root.child(trash_conflict_dialog_overlay(dialog, cx))
            })
            .when_some(application_chooser, |root, chooser| {
                root.child(application_chooser_overlay(chooser, cx))
            })
            .when_some(place_draft, |root, draft| {
                root.child(place_draft_overlay(draft, cx))
            })
            .when_some(network_auth_draft, |root, draft| {
                root.child(network_auth_overlay(draft, cx))
            })
            .when_some(background_task_detail_dialog, |root, dialog| {
                root.child(background_task_detail_dialog_overlay(dialog, cx))
            });
        if let Some(started) = render_started {
            eprintln!(
                "[fika render] panes={} viewport={}x{} places={}us tasks={}us snapshots={}us pane_elements={}us root={}us total={}us",
                pane_count,
                viewport_size.width.as_f32(),
                viewport_size.height.as_f32(),
                places_elapsed.map_or(0, |elapsed| elapsed.as_micros()),
                background_tasks_elapsed.map_or(0, |elapsed| elapsed.as_micros()),
                snapshots_elapsed.map_or(0, |elapsed| elapsed.as_micros()),
                pane_elements_elapsed.map_or(0, |elapsed| elapsed.as_micros()),
                root_started.map_or(0, |started| started.elapsed().as_micros()),
                started.elapsed().as_micros(),
            );
        }
        root
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

    fn set_test_item_drop_target_for_pane(app: &mut FikaApp, pane_id: PaneId) -> bool {
        app.drop_targets.set_item(ItemDropTarget::Pane { pane_id })
    }

    fn assert_raw_grid_marks_directory_drop_target(
        app: &mut FikaApp,
        pane_id: PaneId,
        target_dir: &Path,
    ) {
        let view = app.panes.pane(pane_id).unwrap().view.clone();
        let item_drop_target = app.drop_targets.item().cloned();
        let snapshot = app
            .raw_file_grid_snapshot_for_pane(
                pane_id,
                &view,
                None,
                0,
                None,
                item_drop_target.as_ref(),
            )
            .unwrap();
        match snapshot {
            RawFileGridSnapshot::Compact { items, .. }
            | RawFileGridSnapshot::Icons { items, .. } => {
                let target = items
                    .iter()
                    .find(|item| item.path == target_dir)
                    .expect("target directory snapshot");
                assert!(target.drop_target);
            }
            RawFileGridSnapshot::Details { items, .. } => {
                let target = items
                    .iter()
                    .find(|item| item.path == target_dir)
                    .expect("target directory row snapshot");
                assert!(target.drop_target);
            }
        }
    }

    #[test]
    fn app_env_flag_truthy_values_are_explicit() {
        assert!(env_flag_is_truthy("1"));
        assert!(env_flag_is_truthy(" true "));
        assert!(env_flag_is_truthy("YES"));
        assert!(env_flag_is_truthy("on"));
        assert!(!env_flag_is_truthy(""));
        assert!(!env_flag_is_truthy("0"));
        assert!(!env_flag_is_truthy("false"));
        assert!(!env_flag_is_truthy("disabled"));
    }

    #[test]
    fn listing_cache_debug_summary_reports_cache_and_large_directory_state() {
        let mut cache = fika_core::DirectoryCache::with_limits(fika_core::DirectoryCacheLimits {
            max_dirs: 4,
            max_entries: 4,
            max_entries_per_dir: 2,
        });
        assert!(
            cache
                .insert_fresh("/tmp/fika-small-cache", test_entries(&["a", "b"]))
                .is_some()
        );
        assert!(cache.record_uncached_directory(Path::new("/tmp/fika-large-cache"), 3));

        let line = listing_cache_debug_summary("test", &cache.debug_snapshot(), 7);

        assert!(line.contains("[fika cache] test"));
        assert!(line.contains("pending=7"));
        assert!(line.contains("cached_dirs=1"));
        assert!(line.contains("cached_entries=2"));
        assert!(line.contains("skipped_large=1"));
        assert!(line.contains("large_summaries=1"));
        assert!(line.contains("/tmp/fika-large-cache:3"));
    }

    #[test]
    fn active_place_prefers_longest_path_prefix() {
        let places = vec![
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
            without_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::PasteAsAdministrator)
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
        assert_eq!(
            with_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::PasteAsAdministrator)
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
    fn context_menu_actions_offer_drop_operation_choices() {
        let target = ContextMenuTarget::DropOperation {
            target_dir: PathBuf::from("/tmp/fika-drop-target"),
            paths: vec![PathBuf::from("/tmp/fika-drop-source.txt")],
            load_target_dir: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.separator_before))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::DropCopy, false),
                (ContextMenuAction::DropMove, false),
                (ContextMenuAction::DropLink, false),
                (ContextMenuAction::DropCancel, true),
            ]
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
    fn context_menu_actions_offer_open_with_submenu_for_current_directory() {
        let mut blank = context_blank_target();
        if let ContextMenuTarget::Blank { open_with_apps, .. } = &mut blank {
            open_with_apps.push(MimeApplication {
                id: "files.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/files.desktop"),
                name: "Files".to_string(),
                exec: "files %f".to_string(),
                icon: Some("system-file-manager".to_string()),
                is_default: true,
            });
        }

        let actions = context_menu_actions(&blank, false);
        assert!(actions.iter().any(|item| {
            item.action == ContextMenuAction::OpenWithSubmenu
                && item.submenu == Some(ContextMenuSubmenu::OpenWith)
        }));

        let submenu = context_submenu_actions(ContextMenuSubmenu::OpenWith, &blank);
        assert_eq!(
            submenu.first().map(|item| &item.action),
            Some(&ContextMenuAction::OpenWithApplication {
                desktop_id: "files.desktop".to_string()
            })
        );
        assert!(
            submenu
                .iter()
                .any(|item| item.action == ContextMenuAction::OtherApplication && item.enabled)
        );
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
                (ContextMenuAction::PasteAsAdministrator, false),
                (ContextMenuAction::OpenWithSubmenu, false),
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
                (
                    ContextMenuAction::CreateFolderAsAdministrator,
                    "Folder as Administrator".to_string(),
                    Some(ContextMenuIcon::Administrator),
                ),
                (
                    ContextMenuAction::CreateFileAsAdministrator,
                    "Text File as Administrator".to_string(),
                    Some(ContextMenuIcon::Administrator),
                ),
            ]
        );
    }

    #[test]
    fn context_submenu_actions_enable_sort_and_implemented_view_modes() {
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
                (ContextMenuAction::ViewIcons, true),
                (ContextMenuAction::ViewDetails, true),
            ]
        );
    }

    #[test]
    fn set_pane_view_mode_is_pane_local_and_resets_item_view_scroll() {
        let mut app = test_app_with_entries("/tmp/fika-view-mode-switch", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        let first_item_id = app.panes.pane(first).unwrap().model.entries()[0].id;
        app.visible_item_slots
            .entry(first)
            .or_default()
            .update_visible_items([first_item_id]);
        let first_item_slot = app
            .visible_item_slots
            .get(&first)
            .and_then(|slots| slots.slot_for_item(first_item_id))
            .unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(first);
        scroll_handle.set_offset(gpui::point(px(-180.0), px(-40.0)));
        app.panes
            .set_view_scroll(first, 180.0, 40.0, 1_000.0, 500.0)
            .unwrap();
        app.compact_column_widths
            .insert(first, CompactColumnWidthCache::default());

        app.set_pane_view_mode(first, ViewMode::Details);

        let first_view = &app.panes.pane(first).unwrap().view;
        assert_eq!(first_view.view_mode, ViewMode::Details);
        assert_eq!(first_view.scroll_x, 0.0);
        assert_eq!(first_view.scroll_y, 0.0);
        assert_eq!(scroll_handle.offset(), gpui::point(px(0.0), px(0.0)));
        assert_eq!(
            app.visible_item_slots
                .get(&first)
                .and_then(|slots| slots.slot_for_item(first_item_id)),
            Some(first_item_slot)
        );
        assert!(!app.compact_column_widths.contains_key(&first));
        assert_eq!(
            app.panes.pane(second).unwrap().view.view_mode,
            ViewMode::Compact
        );
        assert_eq!(app.status_message_for_pane(first), "Details view");

        app.set_pane_view_mode(first, ViewMode::Icons);

        assert_eq!(
            app.panes.pane(first).unwrap().view.view_mode,
            ViewMode::Icons
        );
        assert_eq!(app.status_message_for_pane(first), "Icons view");

        app.set_pane_view_mode(first, ViewMode::Compact);

        assert_eq!(
            app.panes.pane(first).unwrap().view.view_mode,
            ViewMode::Compact
        );
        assert_eq!(app.status_message_for_pane(first), "Compact view");
    }

    #[test]
    fn set_pane_view_mode_primes_viewport_for_scrollbar_axis_change() {
        let mut app = test_app_with_entries("/tmp/fika-view-mode-axis-switch", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        {
            let pane = app.panes.pane_mut(pane_id).unwrap();
            pane.view.view_mode = ViewMode::Icons;
            pane.view.viewport_width = 626.0;
            pane.view.viewport_height = 360.0;
            pane.view.scroll_y = 120.0;
            pane.view.max_scroll_y = 1_000.0;
        }

        app.set_pane_view_mode(pane_id, ViewMode::Compact);

        let view = &app.panes.pane(pane_id).unwrap().view;
        assert_eq!(view.view_mode, ViewMode::Compact);
        assert_eq!(view.viewport_width, 640.0);
        assert_eq!(view.viewport_height, 346.0);
        assert_eq!(view.scroll_x, 0.0);
        assert_eq!(view.scroll_y, 0.0);
        assert_eq!(view.max_scroll_x, 0.0);
        assert_eq!(view.max_scroll_y, 0.0);

        app.set_pane_view_mode(pane_id, ViewMode::Details);

        let view = &app.panes.pane(pane_id).unwrap().view;
        assert_eq!(view.view_mode, ViewMode::Details);
        assert_eq!(view.viewport_width, 626.0);
        assert_eq!(view.viewport_height, 360.0);
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
    fn context_menu_actions_do_not_offer_administrator_actions_for_network_items() {
        let admin_actions = [
            ContextMenuAction::RenameAsAdministrator,
            ContextMenuAction::TrashAsAdministrator,
            ContextMenuAction::CreateFolderAsAdministrator,
            ContextMenuAction::CreateFileAsAdministrator,
            ContextMenuAction::PasteAsAdministrator,
        ];

        for target in [
            context_item_target("smb://server/share/folder", true, 1),
            context_item_target("smb://server/share/readme.txt", false, 1),
            context_item_target("smb://server/share/readme.txt", false, 2),
        ] {
            let actions = context_menu_actions(&target, true);
            assert!(
                admin_actions
                    .iter()
                    .all(|admin_action| !actions.iter().any(|item| item.action == *admin_action))
            );
        }
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
            label: "Place".to_string(),
            device_id: None,
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
            label: "USB".to_string(),
            device_id: Some("gio:test:usb".to_string()),
            path: PathBuf::from("gio:test:usb"),
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
            label: "USB".to_string(),
            device_id: Some("gio:test:usb".to_string()),
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
            label: "USB".to_string(),
            device_id: Some("gio:test:usb".to_string()),
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
    fn default_open_with_application_prefers_marked_default() {
        let apps = vec![
            MimeApplication {
                id: "first.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/first.desktop"),
                name: "First".to_string(),
                exec: "first %f".to_string(),
                icon: None,
                is_default: false,
            },
            MimeApplication {
                id: "default.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/default.desktop"),
                name: "Default".to_string(),
                exec: "default %f".to_string(),
                icon: None,
                is_default: true,
            },
        ];

        assert_eq!(
            default_open_with_application_id(&apps),
            Some("default.desktop")
        );
        assert_eq!(
            default_open_with_application_id(&apps[..1]),
            Some("first.desktop")
        );
        assert_eq!(default_open_with_application_id(&[]), None);
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
                icon: Some("tools-checksum".to_string()),
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
        assert_eq!(
            tools.first().and_then(|item| item.icon.as_ref()),
            Some(&ContextMenuIcon::Named("tools-checksum".to_string()))
        );
        let menu = ContextMenuState {
            pane_id: PaneId(1),
            target,
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: Some(ContextMenuOpenSubmenu {
                submenu: ContextMenuSubmenu::ServiceMenu,
                parent_index: 0,
                nested: Some(ContextMenuNestedSubmenu {
                    submenu: ContextMenuSubmenu::ServiceMenuGroup(0),
                    parent_index: 0,
                }),
            }),
        };
        let mut cache = FileIconCache::default();
        let snapshots = context_menu_icon_snapshots(&mut cache, &menu, false);

        assert!(snapshots.contains_key(&ContextMenuIcon::Named("tools-checksum".to_string())));
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
                (ContextMenuAction::TrashAsAdministrator, None),
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
                (ContextMenuAction::PasteAsAdministrator, false),
                (ContextMenuAction::CompressWithArk, true),
                (ContextMenuAction::Rename, true),
                (ContextMenuAction::RenameAsAdministrator, false),
                (ContextMenuAction::Trash, false),
                (ContextMenuAction::TrashAsAdministrator, false),
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
        assert_eq!(
            context_menu_actions(&dir_target, true)
                .iter()
                .find(|item| item.action == ContextMenuAction::PasteAsAdministrator)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            !context_menu_actions(&file_target, true)
                .iter()
                .any(|item| item.action == ContextMenuAction::Paste)
        );
        assert!(
            !context_menu_actions(&file_target, true)
                .iter()
                .any(|item| item.action == ContextMenuAction::PasteAsAdministrator)
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
        assert_eq!(viewer.icon_name.as_ref(), "accessories-text-editor");
        assert_eq!(viewer.fallback_marker.as_ref(), "VI");
        assert!(!snapshots.contains_key(&1));

        let snapshots = ui::application_chooser::application_chooser_visible_icon_snapshots(
            &mut cache,
            &applications,
            1..2,
        );
        let writer = snapshots.get(&1).unwrap();
        assert_eq!(writer.icon_name.as_ref(), "application-x-executable");
        assert_eq!(writer.fallback_marker.as_ref(), "WR");
    }

    #[test]
    fn application_chooser_list_height_is_stable_for_virtualized_rows() {
        assert_eq!(
            ui::application_chooser::application_chooser_list_height(0),
            44.0
        );
        assert_eq!(
            ui::application_chooser::application_chooser_list_height(3),
            132.0
        );
        assert_eq!(
            ui::application_chooser::application_chooser_list_height(100),
            360.0
        );
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
    fn application_chooser_deduplicates_all_application_list() {
        let mut app = test_app_with_entries("/tmp/fika-open-with-dedup", &["note.txt"]);
        let mut duplicate =
            test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]);
        duplicate.desktop_file = PathBuf::from("/home/me/.local/share/applications/viewer.desktop");
        let list = fika_core::parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=viewer.desktop;\n",
        );
        app.mime_applications = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                duplicate,
                test_desktop_application("viewer-copy.desktop", "Viewer", "viewer %f", &[]),
                test_desktop_application("writer.desktop", "Writer", "writer %f", &[]),
            ],
            &[list],
        );

        let applications = app.application_chooser_applications(Some("text/plain"));

        assert_eq!(
            applications
                .iter()
                .map(|app| (app.name.as_str(), app.is_default))
                .collect::<Vec<_>>(),
            vec![("Viewer", true), ("Writer", false)]
        );
    }

    #[test]
    fn application_chooser_search_filters_all_display_fields() {
        let kate =
            test_desktop_application("org.kde.kate.desktop", "Kate", "kate %U", &["text/plain"]);
        let nautilus = test_desktop_application(
            "org.gnome.Nautilus.desktop",
            "Files",
            "nautilus %U",
            &["inode/directory"],
        );
        let applications = vec![
            MimeApplication::from((&kate, false)),
            MimeApplication::from((&nautilus, false)),
        ];

        assert_eq!(
            application_chooser_filtered_applications(&applications, "kde kate")
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["org.kde.kate.desktop"]
        );
        assert_eq!(
            application_chooser_filtered_applications(&applications, "nautilus")
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["org.gnome.Nautilus.desktop"]
        );
        assert!(application_chooser_filtered_applications(&applications, "missing").is_empty());
    }

    #[test]
    fn application_chooser_default_toggle_is_dialog_level_state() {
        let mut app = test_app_with_entries("/tmp/fika-open-with-default-toggle", &["note.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.mime_applications = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "viewer.desktop",
                "Viewer",
                "viewer %f",
                &["text/plain"],
            )],
            &[],
        );

        app.show_application_chooser(
            pane_id,
            PathBuf::from("/tmp/fika-open-with-default-toggle/note.txt"),
            Some(Arc::from("text/plain")),
        );
        assert!(
            !app.application_chooser
                .as_ref()
                .unwrap()
                .set_default_on_choose
        );

        app.toggle_application_chooser_set_default();
        assert!(
            app.application_chooser
                .as_ref()
                .unwrap()
                .set_default_on_choose
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
        let Some(task_id) = app.begin_pane_operation(pane_id, "Opening") else {
            panic!("no task");
        };

        app.finish_open_with_application(
            task_id,
            OpenWithLaunchResult {
                pane_id,
                path: PathBuf::from("/tmp/fika-open-with-finish/note.txt"),
                app_name: "Viewer".to_string(),
                result: Ok(SystemdLaunchResult {
                    units: vec!["fika-open-with-viewer-0.service".to_string()],
                }),
            },
        );

        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Opened /tmp/fika-open-with-finish/note.txt with Viewer via 1 systemd unit(s)"
        );
    }

    #[test]
    fn open_in_new_window_finish_reports_systemd_result_to_pane() {
        let mut app = test_app_with_entries("/tmp/fika-new-window-finish", &[]);
        let pane_id = app.panes.focused().unwrap();
        let Some(task_id) = app.begin_pane_operation(pane_id, "Opening new window") else {
            panic!("no task");
        };

        app.finish_open_in_new_window(
            task_id,
            NewWindowLaunchResult {
                pane_id,
                path: PathBuf::from("/tmp/fika-new-window-finish"),
                result: Ok(SystemdLaunchResult {
                    units: vec!["fika-open-with-fika-new-window-0.service".to_string()],
                }),
            },
        );

        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Opened /tmp/fika-new-window-finish in new window via 1 systemd unit(s)"
        );
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
                ContextMenuAction::TrashAsAdministrator,
                ContextMenuAction::Properties
            ]
        );
    }

    #[test]
    fn context_menu_actions_use_trash_view_actions() {
        let blank = ContextMenuTarget::Blank {
            path: PathBuf::from("/tmp/fika-trash"),
            trash_view: true,
            trash_has_items: false,
            open_with_apps: Vec::new(),
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
        let path = root.join("places.xbel");
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
    fn build_places_applies_persistent_primary_place_order() {
        let root = test_dir("places-order-load");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("places.xbel");
        fika_core::save_user_places(
            &path,
            &[UserPlace::new("Bookmark".to_string(), bookmark.clone())],
        )
        .unwrap();
        let order_path = fika_core::place_order_path_for_user_places_path(&path);
        fika_core::save_place_order(&order_path, &[bookmark.clone(), home_dir()]).unwrap();

        let places = build_places(&path);
        let bookmark_index = places
            .iter()
            .position(|place| place.path == bookmark)
            .expect("persistent bookmark should be loaded");
        let home_index = places
            .iter()
            .position(|place| place.path == home_dir())
            .expect("home place should exist");

        assert!(bookmark_index < home_index);
        assert_eq!(places[bookmark_index].group, "");
        assert_eq!(places[bookmark_index].label, "Bookmark");
        assert_eq!(places[home_index].label, "Home");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_places_adds_network_root_before_devices_without_persisting() {
        let root = test_dir("places-network-root");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("places.xbel");
        fika_core::save_user_places(
            &path,
            &[
                UserPlace::new("Bookmark".to_string(), bookmark.clone()),
                UserPlace::new(
                    "Team Share".to_string(),
                    PathBuf::from("smb://server/share/"),
                ),
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
        let share_index = places
            .iter()
            .position(|place| place.path == PathBuf::from("smb://server/share/"))
            .expect("network bookmark should be loaded");
        let network = &places[network_index];
        let share = &places[share_index];

        assert!(bookmark_index < network_index);
        assert!(network_index < share_index);
        assert!(share_index < root_index);
        assert!(network_index < root_index);
        assert_eq!(network.group, NETWORK_GROUP);
        assert_eq!(network.marker, "Net");
        assert_eq!(network.label, fika_core::NETWORK_ROOT_LABEL);
        assert!(!network.editable);
        assert!(!network.removable);
        assert!(place_is_mounted(network));
        assert_eq!(share.group, NETWORK_GROUP);
        assert_eq!(share.marker, "Net");
        assert_eq!(share.label, "Team Share");
        assert!(share.editable);
        assert!(share.removable);
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
            id: format!("gio:test:{label}"),
            mount_point: Some(PathBuf::from(path)),
            uri: Some(format!("file://{path}")),
            filesystem_type: Some("exfat".to_string()),
            label: Some(label.to_string()),
            capacity_bytes: Some(1024),
            removable,
            mounted: true,
            ejectable: false,
            can_power_off: false,
        }
    }

    #[test]
    fn build_places_projects_removable_devices_before_root() {
        let root = test_dir("places-devices");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("places.xbel");
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
                id: "gio:test:unmounted".to_string(),
                mount_point: None,
                uri: None,
                filesystem_type: Some("exfat".to_string()),
                label: Some("Unmounted".to_string()),
                capacity_bytes: Some(1024),
                removable: true,
                mounted: false,
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
            Some((PathBuf::from("gio:test:unmounted"), false, true, true))
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
            device_id: None,
            device_mounted: true,
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
            device_id: None,
            device_mounted: true,
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
    fn places_trash_snapshot_uses_app_owned_emptiness_state() {
        let current = test_dir("places-trash-state-current");
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        app.places = vec![PlaceEntry {
            group: "",
            marker: "Tr",
            label: "Trash".to_string(),
            path: file_ops::trash_files_dir(),
            device_id: None,
            device_mounted: true,
            editable: false,
            removable: false,
            device_ejectable: false,
            device_can_power_off: false,
        }];

        app.trash_has_items = false;
        let empty = app.place_snapshots();
        assert!(empty[0].trash_place);
        assert!(!empty[0].trash_has_items);

        assert!(app.set_trash_has_items(true));
        let non_empty = app.place_snapshots();
        assert!(non_empty[0].trash_place);
        assert!(non_empty[0].trash_has_items);

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn trash_emptiness_state_updates_open_context_menu() {
        let mut app = test_app_with_entries("/tmp/fika-trash-context-state", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.trash_has_items = false;
        app.context_menu = Some(ContextMenuState {
            pane_id,
            target: context_place_target(file_ops::trash_files_dir(), true, false),
            position: ViewPoint { x: 0.0, y: 0.0 },
            active_submenu: None,
        });

        let empty_actions = context_menu_actions(&app.context_menu.as_ref().unwrap().target, false);
        assert_eq!(
            empty_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(false)
        );

        assert!(app.set_trash_has_items(true));

        let non_empty_target = &app.context_menu.as_ref().unwrap().target;
        assert!(matches!(
            non_empty_target,
            ContextMenuTarget::Place {
                trash_has_items: true,
                ..
            }
        ));
        let non_empty_actions = context_menu_actions(non_empty_target, false);
        assert_eq!(
            non_empty_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(true)
        );

        assert!(app.set_trash_has_items(false));
        let empty_target = &app.context_menu.as_ref().unwrap().target;
        assert!(matches!(
            empty_target,
            ContextMenuTarget::Place {
                trash_has_items: false,
                ..
            }
        ));
    }

    #[test]
    fn trash_lister_events_update_app_owned_emptiness_state() {
        let trash = file_ops::trash_files_dir();
        let trash_arg = trash.display().to_string();
        let mut app = test_app_with_entries(&trash_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        let generation = app.panes.pane(pane_id).unwrap().generation;
        app.trash_has_items = false;

        app.apply_event(DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial: fika_core::RequestSerial(1),
            path: trash.clone(),
            entries: vec![test_entry("deleted.txt")],
        });

        assert!(app.trash_has_items);

        app.apply_event(DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial: fika_core::RequestSerial(2),
            path: trash,
            entries: Arc::new(Vec::new()),
        });

        assert!(!app.trash_has_items);
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
            device_id: None,
            device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
    fn add_network_drive_starts_path_draft_and_persists_network_bookmark() {
        let current = test_dir("network-place-add-current");
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
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: NETWORK_GROUP,
                marker: "Net",
                label: "Network".to_string(),
                path: network_root_path(),
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        app.start_add_network_drive(pane_id);
        let draft = app.place_draft.as_mut().unwrap();
        assert_eq!(draft.focus, PlaceDraftField::Path);
        assert_eq!(draft.label, "Network Drive");
        assert_eq!(draft.path, "smb://server/share/");
        draft.label = "Team Share".to_string();
        app.commit_place_draft();

        assert_eq!(app.places.len(), 4);
        assert_eq!(app.places[2].label, "Team Share");
        assert_eq!(app.places[2].path, PathBuf::from("smb://server/share/"));
        assert_eq!(app.places[2].group, NETWORK_GROUP);
        assert_eq!(app.places[2].marker, "Net");
        assert!(app.places[2].editable);
        assert!(app.places[2].removable);
        assert_eq!(app.places[3].group, "Devices");
        assert!(app.place_draft.is_none());
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Added place Team Share"
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new(
                "Team Share".to_string(),
                PathBuf::from("smb://server/share/")
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        assert!(app.set_place_drag_drop_target_for_path(user.clone()));
        assert!(app.drop_targets.item().is_none());
        assert!(place_drop_target_matches_place(
            app.drop_targets.place(),
            &user
        ));
        assert!(
            app.place_snapshots()
                .into_iter()
                .find(|place| place.path == user)
                .is_some_and(|place| place.drop_target)
        );

        assert!(app.set_place_drag_drop_target_for_insert(0));
        assert!(place_drop_target_matches_insert(
            app.drop_targets.place(),
            0
        ));
        assert!(
            app.place_snapshots()
                .into_iter()
                .find(|place| place.label == "Home")
                .is_some_and(|place| place.insert_before)
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(user);
    }

    #[test]
    fn pane_drop_target_clears_place_insert_target() {
        let current = test_dir("pane-drop-target-clears-place-insert");
        std::fs::create_dir_all(&current).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.set_place_drag_drop_target_for_insert(0));
        assert!(place_drop_target_matches_insert(
            app.drop_targets.place(),
            0
        ));

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        assert!(app.drop_targets.place().is_none());
        assert!(item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn drop_target_lease_generation_clears_only_current_target() {
        let current = test_dir("drop-target-lease-current");
        let target = test_dir("drop-target-lease-target");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        let old_generation = app.drop_targets.lease_generation();
        assert!(app.set_place_drag_drop_target_for_path(target.clone()));
        assert!(!app.clear_drop_targets_for_lease_generation(old_generation));
        assert!(place_drop_target_matches_place(
            app.drop_targets.place(),
            &target
        ));

        let current_generation = app.drop_targets.lease_generation();
        assert!(app.clear_drop_targets_for_lease_generation(current_generation));
        assert!(app.drop_targets.item().is_none());
        assert!(app.drop_targets.place().is_none());

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(target);
    }

    #[test]
    fn repeated_drop_target_refresh_extends_lease_generation() {
        let current = test_dir("drop-target-refresh-current");
        std::fs::create_dir_all(&current).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        let first_generation = app.drop_targets.lease_generation();
        assert!(!set_test_item_drop_target_for_pane(&mut app, pane_id));
        assert!(app.drop_targets.lease_generation() > first_generation);
        assert!(!app.clear_drop_targets_for_lease_generation(first_generation));
        assert!(item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));

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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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

        assert_eq!(app.places[0].path, dropped);
        assert_eq!(
            app.places[0].label,
            default_place_label(&app.places[0].path)
        );
        assert!(app.places[0].editable);
        assert!(app.places[0].removable);
        assert!(app.active_item_drag.is_none());
        assert!(app.drop_targets.place().is_none());
        assert!(
            app.status_message_for_pane(pane_id)
                .starts_with("Added place ")
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new(
                    default_place_label(&app.places[0].path),
                    app.places[0].path.clone()
                ),
                UserPlace::new("Existing".to_string(), existing.clone()),
            ])
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(app.places[0].path.clone());
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
        let home = home_dir();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: home.clone(),
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
        assert!(app.drop_targets.place().is_none());
        assert_eq!(app.status_message_for_pane(pane_id), "Moved place Beta");
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new("Beta".to_string(), beta.clone()),
                UserPlace::new("Alpha".to_string(), alpha.clone()),
            ])
        );
        assert_eq!(
            fika_core::load_place_order(&fika_core::place_order_path_for_user_places_path(
                &app.user_places_path
            )),
            Ok(vec![home.clone(), beta.clone(), alpha.clone()])
        );
        let rebuilt_primary_labels = build_places(&app.user_places_path)
            .into_iter()
            .filter(|place| place.group.is_empty())
            .take(3)
            .map(|place| place.label)
            .collect::<Vec<_>>();
        assert_eq!(rebuilt_primary_labels, vec!["Home", "Beta", "Alpha"]);

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
    fn place_order_persists_after_bookmark_insert_and_remove() {
        let current = test_dir("place-order-mutate-current");
        let alpha = test_dir("place-order-mutate-alpha");
        let dropped = test_dir("place-order-mutate-dropped");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&dropped).unwrap();
        let current_arg = current.display().to_string();
        let home = home_dir();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: home.clone(),
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        app.drop_place_drag_to_place_insert(0, 2);
        app.insert_place_from_dropped_paths(pane_id, vec![dropped.clone()], 1);

        assert_eq!(
            app.places
                .iter()
                .map(|place| place.label.clone())
                .collect::<Vec<_>>(),
            vec![
                "Alpha".to_string(),
                default_place_label(&dropped),
                "Home".to_string(),
                "Root".to_string()
            ]
        );
        assert_eq!(
            fika_core::load_place_order(&fika_core::place_order_path_for_user_places_path(
                &app.user_places_path
            )),
            Ok(vec![alpha.clone(), dropped.clone(), home.clone()])
        );
        let rebuilt_primary_labels = build_places(&app.user_places_path)
            .into_iter()
            .filter(|place| place.group.is_empty())
            .take(3)
            .map(|place| place.label)
            .collect::<Vec<_>>();
        assert_eq!(
            rebuilt_primary_labels,
            vec![
                "Alpha".to_string(),
                default_place_label(&dropped),
                "Home".to_string()
            ]
        );

        app.remove_place(pane_id, &dropped);

        assert_eq!(
            fika_core::load_place_order(&fika_core::place_order_path_for_user_places_path(
                &app.user_places_path
            )),
            Ok(vec![alpha.clone(), home.clone()])
        );
        let rebuilt_primary_labels = build_places(&app.user_places_path)
            .into_iter()
            .filter(|place| place.group.is_empty())
            .take(2)
            .map(|place| place.label)
            .collect::<Vec<_>>();
        assert_eq!(rebuilt_primary_labels, vec!["Alpha", "Home"]);

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(alpha);
        let _ = std::fs::remove_dir_all(dropped);
    }

    #[test]
    fn place_drag_reorder_allows_active_and_builtin_primary_places() {
        let current = test_dir("place-reorder-active-current");
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "Network",
                marker: "Net",
                label: "Network".to_string(),
                path: network_root_path(),
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
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
            vec!["User", "Home", "Network"]
        );
        assert!(app.drop_targets.place().is_none());
        assert_eq!(app.status_message_for_pane(pane_id), "Moved place Home");
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new("User".to_string(), user.clone())])
        );
        assert_eq!(
            fika_core::load_place_order(&fika_core::place_order_path_for_user_places_path(
                &app.user_places_path
            )),
            Ok(vec![user.clone(), current.clone()])
        );

        app.drop_place_drag_to_place_insert(2, 0);

        assert_eq!(
            app.places
                .iter()
                .map(|place| place.label.as_str())
                .collect::<Vec<_>>(),
            vec!["User", "Home", "Network"]
        );
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place cannot be moved"
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
        assert!(set_test_item_drop_target_for_pane(&mut app, first));
        assert!(app.set_place_drag_drop_target_for_insert(0));

        app.drop_place_drag_to_pane(second, place_dir.clone());

        assert_eq!(app.panes.focused(), Some(second));
        assert_eq!(
            app.panes
                .pane(second)
                .map(|pane| pane.current_dir.as_path()),
            Some(place_dir.as_path())
        );
        assert!(app.drop_targets.item().is_none());
        assert!(app.drop_targets.place().is_none());
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
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
                device_id: None,
                device_mounted: true,
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];
        app.place_draft = Some(PlaceDraft::for_edit(pane_id, "User".to_string(), &user));

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
            pane_shortcut(&gpui::Keystroke::parse("f9").unwrap()),
            Some(PaneShortcut::TogglePlacesSidebar)
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
            pane_shortcut(&gpui::Keystroke::parse("secondary-f").unwrap()),
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
        assert_eq!(default_options.padding, 2.0);
        assert_eq!(default_options.side_padding, 8.0);
        assert_eq!(default_options.gap, 8.0);
        assert_eq!(default_options.text_gap, 4.0);
        assert_eq!(default_options.item_width, 146.0);
        assert_eq!(default_options.item_height, 52.0);
        assert_eq!(default_options.text_height, 40.0);

        let zoomed_options = ui::file_grid::compact_layout_options(
            &ViewState {
                zoom_level: fika_core::MAX_ZOOM_LEVEL,
                ..ViewState::default()
            },
            0.0,
        );
        assert_eq!(zoomed_options.icon_size, 256.0);
        assert_eq!(zoomed_options.item_width, 354.0);
        assert_eq!(zoomed_options.item_height, 260.0);
        assert_eq!(zoomed_options.text_height, 40.0);
    }

    #[test]
    fn icons_layout_options_follow_dolphin_width_and_reserve_three_name_lines() {
        let icon_text_height = ui::file_grid::ITEM_NAME_LINE_HEIGHT
            * ui::file_grid::DOLPHIN_ICON_MAX_TEXT_LINES as f32;
        let default_options = ui::file_grid::icons_layout_options(&ViewState::default(), 0.0);
        assert_eq!(default_options.icon_size, 48.0);
        assert_eq!(default_options.padding, 2.0);
        assert_eq!(default_options.gap, 8.0);
        assert_eq!(default_options.item_width, 96.0);
        assert_eq!(default_options.item_height, 108.0);
        assert_eq!(default_options.text_height, icon_text_height);

        let zoomed_options = ui::file_grid::icons_layout_options(
            &ViewState {
                zoom_level: fika_core::MAX_ZOOM_LEVEL,
                ..ViewState::default()
            },
            0.0,
        );
        assert_eq!(zoomed_options.icon_size, 256.0);
        assert_eq!(zoomed_options.item_width, 269.0);
        assert_eq!(zoomed_options.item_height, 316.0);
        assert_eq!(zoomed_options.text_height, icon_text_height);
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
    fn projected_item_viewport_width_matches_scrollbar_axis() {
        let mut app = test_app_with_entries("/tmp/fika-panes-viewport", &[]);
        let first = app.panes.focused().unwrap();
        assert!(app.set_pane_row_width(820.0));
        let second = app.panes.split(first).unwrap();
        app.split_pane_ratio(first, second);

        assert!(width_value_eq(
            app.projected_pane_width(first).unwrap(),
            (820.0 - PANE_SPLITTER_WIDTH) / 2.0
        ));
        assert_eq!(
            app.projected_item_viewport_width(first, ViewMode::Icons),
            Some(393.0)
        );
        assert_eq!(
            app.projected_item_viewport_width(first, ViewMode::Details),
            Some(393.0)
        );
        assert_eq!(
            app.projected_item_viewport_width(first, ViewMode::Compact),
            Some(407.0)
        );
    }

    #[test]
    fn window_resize_primes_icon_viewport_before_prepaint_bounds_arrive() {
        let mut app = test_app_with_entries("/tmp/fika-window-resize-prime", &[]);
        let pane_id = app.panes.focused().unwrap();
        {
            let pane = app.panes.pane_mut(pane_id).unwrap();
            pane.view.view_mode = ViewMode::Icons;
            pane.view.viewport_width = 604.0;
            pane.view.viewport_height = 360.0;
        }
        assert!(app.set_pane_row_width(620.0));

        app.prime_pane_viewports_for_window_resize(1024.0, 768.0);
        assert_eq!(app.pane_row_width, 620.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_width, 604.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_height, 360.0);

        app.prime_pane_viewports_for_window_resize(1224.0, 918.0);

        assert_eq!(app.pane_row_width, 820.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_width, 804.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_height, 510.0);
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
    fn places_sidebar_width_clamps_and_resets() {
        let mut app = test_app_with_entries("/tmp/fika-places-sidebar-width", &[]);

        assert_eq!(app.places_sidebar_width, PLACES_SIDEBAR_DEFAULT_WIDTH);
        assert!(app.set_places_sidebar_width(80.0));
        assert_eq!(app.places_sidebar_width, clamp_places_sidebar_width(80.0));
        assert!(app.set_places_sidebar_width(999.0));
        assert_eq!(app.places_sidebar_width, clamp_places_sidebar_width(999.0));
        assert!(app.set_places_sidebar_width(PLACES_SIDEBAR_DEFAULT_WIDTH));
        assert_eq!(app.places_sidebar_width, PLACES_SIDEBAR_DEFAULT_WIDTH);
        assert!(!app.set_places_sidebar_width(PLACES_SIDEBAR_DEFAULT_WIDTH));
    }

    #[test]
    fn places_sidebar_drag_uses_row_origin() {
        let mut app = test_app_with_entries("/tmp/fika-places-sidebar-drag", &[]);

        assert!(app.set_places_sidebar_width(places_sidebar_width_from_drag(315.0, 12.0)));
        assert_eq!(app.places_sidebar_width, clamp_places_sidebar_width(303.0));
        assert!(app.set_places_sidebar_width(places_sidebar_width_from_drag(-100.0, 12.0)));
        assert_eq!(app.places_sidebar_width, clamp_places_sidebar_width(-112.0));
    }

    #[test]
    fn places_sidebar_visibility_toggle_preserves_width() {
        let mut app = test_app_with_entries("/tmp/fika-places-sidebar-toggle", &[]);
        assert!(app.set_places_sidebar_width(276.0));

        assert!(app.toggle_places_sidebar_visibility());
        assert!(!app.places_sidebar_visible);
        assert_eq!(app.places_sidebar_width, 276.0);

        assert!(app.toggle_places_sidebar_visibility());
        assert!(app.places_sidebar_visible);
        assert_eq!(app.places_sidebar_width, 276.0);
    }

    #[test]
    fn app_settings_snapshot_contains_places_sidebar_layout() {
        let mut app = test_app_with_entries("/tmp/fika-places-sidebar-settings", &[]);
        assert!(app.set_places_sidebar_width(276.0));
        app.places_sidebar_visible = false;

        assert_eq!(
            app.current_app_settings(),
            AppSettings {
                places_sidebar: PlacesSidebarSettings {
                    width: Some(276.0),
                    visible: Some(false),
                },
            }
        );
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
    fn item_directory_activation_uses_model_when_snapshot_hint_is_stale() {
        let root = test_dir("activate-model-directory-root");
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let root_arg = root.display().to_string();
        let mut app = test_app_with_entries(&root_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            root.clone(),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("child"),
                name_width_units: 5,
                target_path: None,
                size_bytes: 0,
                modified_secs: None,
                metadata_complete: true,
                trash_original_path: None,
                trash_deletion_time: None,
                mime_type: Some(Arc::from("inode/directory")),
                mime_magic_checked: true,
                is_dir: true,
            })]),
        );

        assert!(app.open_directory_from_item(pane_id, child.clone(), false));

        assert_eq!(
            app.panes
                .pane(pane_id)
                .map(|pane| pane.current_dir.as_path()),
            Some(child.as_path())
        );

        let _ = std::fs::remove_dir_all(root);
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
            .update_visible_items(item_ids);
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
    fn schedule_listings_ignores_empty_and_non_loading_batches() {
        let app = test_app_with_entries("/tmp/fika-empty-batch", &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();

        let no_events: Vec<DirectoryListerEvent> = Vec::new();
        app.schedule_listings(no_events.iter());
        assert_eq!(app.listing_worker.pending_count(), 0);

        let non_loading_events = vec![DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: pane.generation,
            request_serial: fika_core::RequestSerial(0),
            path: pane.current_dir.clone(),
        }];
        app.schedule_listings(non_loading_events.iter());
        assert_eq!(app.listing_worker.pending_count(), 0);
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
    fn cached_load_clears_transient_loading_status() {
        let initial = test_dir("status-cached-initial");
        let cached = test_dir("status-cached-target");
        std::fs::create_dir_all(&initial).unwrap();
        std::fs::create_dir_all(&cached).unwrap();
        let mut app = test_app_with_entries(initial.to_str().unwrap(), &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(
            app.listing_worker
                .cache_listing_snapshot(&cached, test_entries(&["cached.txt"]))
        );

        app.load_pane(pane_id, cached.clone());

        assert!(!app.loading_panes.contains_key(&pane_id));
        assert_eq!(app.status_message_for_pane(pane_id), "Ready");

        let _ = std::fs::remove_dir_all(initial);
        let _ = std::fs::remove_dir_all(cached);
    }

    #[test]
    fn loading_completion_preserves_overridden_status() {
        let initial = test_dir("status-loading-initial");
        let target = test_dir("status-loading-target");
        std::fs::create_dir_all(&initial).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let mut app = test_app_with_entries(initial.to_str().unwrap(), &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();

        app.load_pane(pane_id, target.clone());
        let loading = app.loading_panes.get(&pane_id).unwrap().clone();
        app.set_pane_status(pane_id, "1 selected");
        app.apply_event(DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: loading.key.generation,
            request_serial: loading.key.request_serial,
            path: target.clone(),
        });

        assert!(!app.loading_panes.contains_key(&pane_id));
        assert_eq!(app.status_message_for_pane(pane_id), "1 selected");

        let _ = std::fs::remove_dir_all(initial);
        let _ = std::fs::remove_dir_all(target);
    }

    #[test]
    fn network_auth_event_opens_prompt_for_current_pane() {
        let current = PathBuf::from("smb://server/share/");
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.reload_pane(pane_id);
        let loading = app.loading_panes.get(&pane_id).unwrap().clone();

        app.apply_event(DirectoryListerEvent::NetworkAuthRequired {
            pane_id,
            generation: loading.key.generation,
            request_serial: loading.key.request_serial,
            path: current.clone(),
            uri: "smb://server/share/".to_string(),
            message: "Password required".to_string(),
            default_username: Some("yk".to_string()),
            default_domain: Some("WORKGROUP".to_string()),
        });

        let draft = app.network_auth_draft.as_ref().unwrap();
        assert_eq!(draft.pane_id, pane_id);
        assert_eq!(draft.path, current);
        assert_eq!(draft.uri, "smb://server/share/");
        assert_eq!(draft.username, "yk");
        assert_eq!(draft.domain, "WORKGROUP");
        assert_eq!(draft.focus, NetworkAuthField::Password);
        assert!(!app.loading_panes.contains_key(&pane_id));
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Authentication required for smb://server/share/"
        );
    }

    #[test]
    fn stale_network_auth_event_does_not_open_prompt() {
        let current = PathBuf::from("smb://server/share/");
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.reload_pane(pane_id);
        let loading = app.loading_panes.get(&pane_id).unwrap().clone();

        app.apply_event(DirectoryListerEvent::NetworkAuthRequired {
            pane_id,
            generation: Generation(loading.key.generation.0 + 1),
            request_serial: loading.key.request_serial,
            path: current,
            uri: "smb://server/share/".to_string(),
            message: "Password required".to_string(),
            default_username: None,
            default_domain: None,
        });

        assert!(app.network_auth_draft.is_none());
        assert!(app.loading_panes.contains_key(&pane_id));
    }

    #[test]
    fn committing_network_auth_prompt_retries_listing() {
        let current = PathBuf::from("smb://server/share/");
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.network_auth_draft = Some(NetworkAuthDraft::new(
            pane_id,
            current.clone(),
            "smb://server/share/".to_string(),
            "Password required".to_string(),
            Some("yk".to_string()),
            Some("WORKGROUP".to_string()),
        ));
        let draft = app.network_auth_draft.as_mut().unwrap();
        draft.password = "secret".to_string();

        app.commit_network_auth_draft();

        assert!(app.network_auth_draft.is_none());
        assert!(app.loading_panes.contains_key(&pane_id));
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Reloading smb://server/share/"
        );
        assert!(fika_core::forget_network_auth("smb://server/share/").is_ok());
    }

    #[test]
    fn network_auth_prompt_consumes_shortcuts_without_focused_pane_match() {
        let current = PathBuf::from("smb://server/share/");
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        let other_pane = app.panes.split(pane_id).unwrap();
        app.panes.focus(other_pane);
        app.network_auth_draft = Some(NetworkAuthDraft::new(
            pane_id,
            current,
            "smb://server/share/".to_string(),
            "Password required".to_string(),
            None,
            None,
        ));

        assert!(app.handle_network_auth_draft_keystroke(&gpui::Keystroke::parse("f5").unwrap()));
        assert!(app.network_auth_draft.is_some());
        assert!(!app.loading_panes.contains_key(&other_pane));

        assert!(
            app.handle_network_auth_draft_keystroke(&gpui::Keystroke::parse("escape").unwrap())
        );
        assert!(app.network_auth_draft.is_none());
    }

    #[test]
    fn orphan_loading_cleanup_clears_transient_status() {
        let initial = test_dir("status-orphan-initial");
        let target = test_dir("status-orphan-target");
        std::fs::create_dir_all(&initial).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let mut app = test_app_with_entries(initial.to_str().unwrap(), &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();

        app.load_pane(pane_id, target.clone());
        app.listing_worker.cancel_pane(pane_id);

        assert!(app.loading_panes.contains_key(&pane_id));
        assert_eq!(
            app.status_message_for_pane(pane_id),
            format!("Loading {}", target.display())
        );

        assert!(app.drain_background_listing_results());

        assert!(!app.loading_panes.contains_key(&pane_id));
        assert_eq!(app.status_message_for_pane(pane_id), "Ready");

        let _ = std::fs::remove_dir_all(initial);
        let _ = std::fs::remove_dir_all(target);
    }

    #[test]
    fn trash_restore_conflict_dialog_is_pane_local_and_cleared_with_pane() {
        let mut app = test_app_with_entries("/tmp/fika-trash-conflict-dialog", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let conflict = file_ops::TrashRestoreConflict {
            original_path: PathBuf::from("/tmp/fika-original.txt"),
            trash_path: PathBuf::from("/tmp/fika-trash/files/fika-original.txt"),
        };
        let Some(task_id) = app.begin_pane_operation(pane_id, "Restoring from trash") else {
            panic!("no task");
        };

        app.finish_trash_view_operation(
            task_id,
            TrashViewOperationResult {
                pane_id,
                operation: TrashViewOperation::Restore {
                    conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
                },
                success_count: 0,
                failure_count: 0,
                affected_dirs: Vec::new(),
                restore_conflicts: vec![conflict.clone()],
            },
        );

        let dialog = app.trash_conflict_dialog.as_ref().unwrap();
        assert_eq!(dialog.pane_id, pane_id);
        assert_eq!(dialog.conflicts, vec![conflict]);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Restored from trash failed for 1 item(s)"
        );

        app.clear_pane_content_state(pane_id);

        assert!(app.trash_conflict_dialog.is_none());
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
    fn zoom_preserves_item_view_scroll_offset() {
        let mut app = test_app_with_entries("/tmp/fika-zoom-scroll", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(-180.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 180.0, 0.0, 1000.0, 0.0)
            .unwrap();
        app.compact_column_widths
            .insert(pane_id, CompactColumnWidthCache::default());

        app.apply_zoom_change(pane_id, ZoomChange::In);

        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);
        assert!(app.compact_column_widths.contains_key(&pane_id));

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.sync_pane_view_from_item_view_scroll_handle(pane_id);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0));
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.sync_pane_view_from_item_view_scroll_handle(pane_id);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        scroll_handle.set_offset(gpui::point(px(-220.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 220.0, 0.0, 1000.0, 0.0)
            .unwrap();

        app.set_zoom_level(pane_id, fika_core::DEFAULT_ZOOM_LEVEL + 2);

        assert_eq!(scroll_handle.offset().x, px(-220.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 220.0);

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.sync_pane_view_from_item_view_scroll_handle(pane_id);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 220.0);

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);
        assert_eq!(scroll_handle.offset().x, px(-220.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 220.0);
    }

    #[test]
    fn zoom_preserves_vertical_item_view_scroll_offset() {
        let mut app = test_app_with_entries("/tmp/fika-zoom-vertical-scroll", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(0.0), px(-240.0)));
        app.panes
            .set_view_scroll(pane_id, 0.0, 240.0, 0.0, 1_000.0)
            .unwrap();

        app.apply_zoom_change(pane_id, ZoomChange::In);

        assert_eq!(scroll_handle.offset().y, px(-240.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 240.0);

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.sync_pane_view_from_item_view_scroll_handle(pane_id);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 240.0);

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 0.0, 1_000.0));
        assert_eq!(scroll_handle.offset().y, px(-240.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 240.0);

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 0.0, 0.0));
        assert_eq!(scroll_handle.offset().y, px(0.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 0.0);
    }

    #[test]
    fn repeated_zoom_ignores_lagging_handle_maximum() {
        let mut app = test_app_with_entries("/tmp/fika-zoom-scroll-lagging-max", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(-180.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 180.0, 0.0, 1_000.0, 0.0)
            .unwrap();

        app.apply_zoom_change(pane_id, ZoomChange::In);
        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.sync_pane_view_from_item_view_scroll_handle(pane_id);

        app.apply_zoom_change(pane_id, ZoomChange::In);

        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);
    }

    #[test]
    fn zoom_syncs_handle_when_layout_bounds_arrive_after_lagging_handle_maximum() {
        let mut app =
            test_app_with_entries("/tmp/fika-zoom-scroll-lagging-max-bounds", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(-180.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 180.0, 0.0, 1_000.0, 0.0)
            .unwrap();

        app.apply_zoom_change(pane_id, ZoomChange::In);
        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);

        app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0);
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);
    }

    #[test]
    fn refresh_transition_preserves_item_view_scroll_handle() {
        let mut app = test_app_with_entries("/tmp/fika-refresh-scroll", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(0.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 140.0, 32.0, 1_000.0, 500.0)
            .unwrap();
        let view = app.panes.pane(pane_id).unwrap().view.clone();
        sync_item_view_handle_to_view_authoritatively(&mut app.item_view_scroll, pane_id, &view);
        app.begin_item_view_scrollbar_drag(pane_id);

        app.begin_pane_loading_transition(pane_id, PaneLoadingScrollPolicy::Preserve);

        assert_eq!(scroll_handle.offset(), gpui::point(px(-140.0), px(-32.0)));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 140.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 32.0);
        assert!(!item_view_scroll_has_authoritative_scroll(
            &app.item_view_scroll,
            pane_id
        ));
        assert!(!item_view_scroll_is_scrollbar_dragging(
            &app.item_view_scroll,
            pane_id
        ));
    }

    #[test]
    fn scrollbar_drag_keeps_handle_offset_authoritative_during_bounds_sync() {
        let mut app = test_app_with_entries("/tmp/fika-scrollbar-drag", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0));
        assert!(app.begin_item_view_scrollbar_drag(pane_id));
        scroll_handle.set_offset(gpui::point(px(-320.0), px(0.0)));

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0));
        assert_eq!(scroll_handle.offset().x, px(-320.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 320.0);

        scroll_handle.set_offset(gpui::point(px(-480.0), px(0.0)));
        assert!(app.update_item_view_scrollbar_drag(pane_id));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 480.0);

        assert!(app.finish_item_view_scrollbar_drag(pane_id));
        assert!(!item_view_scroll_is_scrollbar_dragging(
            &app.item_view_scroll,
            pane_id
        ));
        assert_eq!(scroll_handle.offset().x, px(-480.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 480.0);
    }

    #[test]
    fn zoom_clamps_to_zero_when_layout_really_has_no_scroll_range() {
        let mut app = test_app_with_entries("/tmp/fika-zoom-scroll-zero-bounds", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let scroll_handle = app.item_view_scroll_handle_for_pane(pane_id);

        scroll_handle.set_offset(gpui::point(px(-180.0), px(0.0)));
        app.panes
            .set_view_scroll(pane_id, 180.0, 0.0, 1_000.0, 0.0)
            .unwrap();

        app.apply_zoom_change(pane_id, ZoomChange::In);

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 1_000.0, 0.0));
        assert_eq!(scroll_handle.offset().x, px(-180.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 180.0);

        assert!(app.set_pane_viewport_bounds(pane_id, 640.0, 360.0, 0.0, 0.0));
        assert_eq!(scroll_handle.offset().x, px(0.0));
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_x, 0.0);
    }

    #[test]
    fn operation_progress_snapshot_is_pane_local() {
        let mut app = test_app_with_entries("/tmp/fika-status-progress", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        let Some(task_id) = app.begin_pane_operation(first, "Copying") else {
            panic!("no task");
        };
        if let Ok(runtime) = OperationRuntime::shared() {
            if let Some(controller) = runtime.operation_controller(task_id) {
                controller.set_progress(file_ops::TransferProgress {
                    bytes_done: 40,
                    bytes_total: 100,
                });
            }
        }
        let now = app
            .operation_snapshots()
            .into_iter()
            .find(|s| s.id == task_id)
            .unwrap()
            .started_at
            + PROGRESS_DISPLAY_DELAY;

        let snapshot = app
            .operation_progress_snapshot_for_pane(first, now)
            .unwrap();
        let task = app
            .background_task_snapshots(now)
            .into_iter()
            .find(|snapshot| snapshot.id == task_id)
            .unwrap();

        assert_eq!(app.status_message_for_pane(first), "Ready");
        assert!(snapshot.label.contains("Copying"));
        assert_eq!(snapshot.percent, Some(40));
        assert!(!snapshot.cancellable);
        assert_eq!(task.title, "Copying");
        assert_eq!(task.percent, Some(40));
        assert!(!task.cancellable);
        assert!(
            app.operation_progress_snapshot_for_pane(second, now)
                .is_none()
        );
        app.finish_pane_operation(task_id, first, "Copied: 1 item(s)");
    }

    #[test]
    fn background_tasks_snapshot_lists_multiple_active_tasks_by_id() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-multi", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        let Some(first_task) = app.begin_pane_operation(first, "Copying 1 item(s)") else {
            panic!("no first task");
        };
        let Some(second_task) = app.begin_pane_operation(second, "Moving 2 item(s)") else {
            panic!("no second task");
        };

        let snapshot = app.background_tasks_snapshot(Instant::now()).unwrap();
        let first_snapshot = snapshot
            .active
            .iter()
            .find(|task| task.id == first_task)
            .unwrap();
        assert_eq!(first_snapshot.pane_id, first);
        assert_eq!(first_snapshot.title, "Copying 1 item(s)");
        let second_snapshot = snapshot
            .active
            .iter()
            .find(|task| task.id == second_task)
            .unwrap();
        assert_eq!(second_snapshot.pane_id, second);
        assert_eq!(second_snapshot.title, "Moving 2 item(s)");

        app.finish_pane_operation(first_task, first, "Copied: 1 item(s)");
        let snapshot = app.background_tasks_snapshot(Instant::now()).unwrap();
        assert!(!snapshot.active.iter().any(|task| task.id == first_task));
        assert!(snapshot.active.iter().any(|task| task.id == second_task));
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(snapshot.history[0].title, "Copying 1 item(s)");
        app.finish_pane_operation(second_task, second, "Moved: 2 item(s)");
    }

    #[test]
    fn background_task_cancel_targets_only_selected_task() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-cancel", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();

        let Some(first_task) = app.begin_pane_operation(pane_id, "Copying 1 item(s)") else {
            panic!("no first task");
        };
        let Some(second_task) = app.begin_pane_operation(pane_id, "Moving 2 item(s)") else {
            panic!("no second task");
        };

        app.cancel_background_operation(second_task);

        if let Ok(runtime) = OperationRuntime::shared() {
            assert!(
                !runtime
                    .operation_controller(first_task)
                    .is_some_and(|c| c.is_cancelled())
            );
            assert!(
                runtime
                    .operation_controller(second_task)
                    .is_some_and(|c| c.is_cancelled())
            );
            runtime.complete_operation(first_task);
            runtime.complete_operation(second_task);
        }
    }

    #[test]
    fn finished_background_tasks_enter_sidebar_history() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-history", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();

        let Some(task_id) = app.begin_pane_operation(pane_id, "Copying 1 item(s)") else {
            panic!("no task");
        };
        app.finish_pane_operation(task_id, pane_id, "Copied: 1 item(s)");

        let snapshot = app.background_tasks_snapshot(Instant::now()).unwrap();
        assert!(!snapshot.expanded);
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(snapshot.history[0].title, "Copying 1 item(s)");
        assert_eq!(snapshot.history[0].detail, "Copied: 1 item(s)");
        assert_eq!(snapshot.history[0].state, BackgroundTaskState::Complete);

        app.toggle_background_tasks_details();
        assert!(
            app.background_tasks_snapshot(Instant::now())
                .unwrap()
                .expanded
        );

        app.clear_background_task_history();
        assert!(app.background_task_history.is_empty());
        if app.operation_snapshots().is_empty() {
            assert!(app.background_tasks_snapshot(Instant::now()).is_none());
            assert!(!app.background_tasks_expanded);
        }
    }

    #[test]
    fn external_background_tasks_surface_full_detail() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-detail", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let detail = "Waiting for administrator authorization.\nCreate folder /tmp/fika-admin";

        let Some(task_id) =
            app.begin_privileged_operation(pane_id, "Administrator: Create Folder", detail)
        else {
            panic!("no task");
        };

        let active = app
            .background_task_snapshots(Instant::now())
            .into_iter()
            .find(|snapshot| snapshot.id == task_id)
            .unwrap();
        assert_eq!(active.title, "Administrator: Create Folder");
        assert_eq!(active.detail, detail);

        let final_detail =
            "Administrator: Create Folder\nOK: create folder /tmp/fika-admin/New Folder";
        app.finish_pane_operation_with_detail(
            task_id,
            pane_id,
            "Administrator: Create Folder: 1 operation(s)",
            final_detail,
        );

        let history = app
            .background_tasks_snapshot(Instant::now())
            .unwrap()
            .history;
        assert_eq!(history[0].title, "Administrator: Create Folder");
        assert_eq!(history[0].detail, final_detail);
        assert_eq!(history[0].state, BackgroundTaskState::Complete);
    }

    #[test]
    fn background_task_detail_dialog_tracks_selected_task_detail() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-dialog", &[]);

        app.show_background_task_detail_dialog(
            "Administrator: Rename".to_string(),
            "Failed: rename /root/a to b\n  Permission denied".to_string(),
            Some(BackgroundTaskState::Failed),
        );

        let dialog = app.background_task_detail_dialog.as_ref().unwrap();
        assert_eq!(dialog.title, "Administrator: Rename");
        assert!(dialog.detail.contains("Permission denied"));
        assert_eq!(dialog.state, Some(BackgroundTaskState::Failed));

        app.dismiss_background_task_detail_dialog();
        assert!(app.background_task_detail_dialog.is_none());
    }

    #[test]
    fn background_task_history_marks_failures_and_caps_recent_items() {
        let mut app = test_app_with_entries("/tmp/fika-background-task-history-cap", &["one.txt"]);
        let pane_id = app.panes.focused().unwrap();

        let Some(task_id) = app.begin_pane_operation(pane_id, "Copying broken item") else {
            panic!("no task");
        };
        app.finish_pane_operation(task_id, pane_id, "Copy failed for 1 item(s)");
        assert_eq!(
            app.background_task_history[0].state,
            BackgroundTaskState::Failed
        );

        for index in 0..(BACKGROUND_TASK_HISTORY_LIMIT + 2) {
            let Some(task_id) = app.begin_pane_operation(pane_id, format!("Task {index}")) else {
                panic!("no task");
            };
            app.finish_pane_operation(task_id, pane_id, format!("Task {index}: done"));
        }

        assert_eq!(
            app.background_task_history.len(),
            BACKGROUND_TASK_HISTORY_LIMIT
        );
        assert_eq!(app.background_task_history[0].title, "Task 9");
        assert_eq!(app.background_task_history[7].title, "Task 2");
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
            rename_input_action(&gpui::Keystroke::parse("tab").unwrap()),
            RenameInputAction::CommitAndRenameNext
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("home").unwrap()),
            RenameInputAction::MoveStart
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("end").unwrap()),
            RenameInputAction::MoveEnd
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("left").unwrap()),
            RenameInputAction::MoveBackward
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("right").unwrap()),
            RenameInputAction::MoveForward
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-home").unwrap()),
            RenameInputAction::SelectStart
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-end").unwrap()),
            RenameInputAction::SelectEnd
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-left").unwrap()),
            RenameInputAction::SelectBackward
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-right").unwrap()),
            RenameInputAction::SelectForward
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            RenameInputAction::Backspace
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("delete").unwrap()),
            RenameInputAction::Delete
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
            RenameInputAction::SelectAll
        );
    }

    #[test]
    fn rename_next_path_follows_current_model_order() {
        let app = test_app_with_entries(
            "/tmp/fika-rename-next",
            &["alpha.txt", "beta.txt", "gamma.txt"],
        );
        let pane_id = app.panes.focused().unwrap();

        assert_eq!(
            app.next_rename_path_after(pane_id, Path::new("/tmp/fika-rename-next/alpha.txt")),
            Some(PathBuf::from("/tmp/fika-rename-next/beta.txt"))
        );
        assert_eq!(
            app.next_rename_path_after(pane_id, Path::new("/tmp/fika-rename-next/beta.txt")),
            Some(PathBuf::from("/tmp/fika-rename-next/gamma.txt"))
        );
        assert_eq!(
            app.next_rename_path_after(pane_id, Path::new("/tmp/fika-rename-next/gamma.txt")),
            None
        );
    }

    #[test]
    fn pending_rename_next_is_pane_local() {
        let mut app =
            test_app_with_entries("/tmp/fika-rename-next-pane", &["alpha.txt", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        let beta = PathBuf::from("/tmp/fika-rename-next-pane/beta.txt");
        app.rename_next_after_operation = Some((first, beta.clone()));

        app.start_pending_rename_next_for_pane(second);

        assert!(app.rename_draft.is_none());
        assert_eq!(app.rename_next_after_operation, Some((first, beta.clone())));

        app.start_pending_rename_next_for_pane(first);

        assert!(app.rename_next_after_operation.is_none());
        let draft = app.rename_draft.as_ref().unwrap();
        assert_eq!(draft.pane_id, first);
        assert_eq!(draft.original_path, beta);
        assert_eq!(draft.draft_name, "beta.txt");
        assert_eq!(draft.caret, "beta".len());
    }

    #[test]
    fn start_rename_for_path_selects_item_and_creates_draft() {
        let mut app = test_app_with_entries("/tmp/fika-rename-path", &["alpha.txt", "beta.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(
            app.start_rename_for_path(pane_id, PathBuf::from("/tmp/fika-rename-path/beta.txt"),)
        );

        let draft = app.rename_draft.as_ref().unwrap();
        assert_eq!(draft.pane_id, pane_id);
        assert_eq!(
            draft.original_path,
            PathBuf::from("/tmp/fika-rename-path/beta.txt")
        );
        assert_eq!(draft.draft_name, "beta.txt");
        assert_eq!(draft.caret, "beta".len());
        assert_eq!(draft.selection, Some((0, "beta".len())));
        assert!(draft.error.is_none());
    }

    #[test]
    fn start_rename_as_administrator_creates_privileged_draft() {
        let mut app = test_app_with_entries("/tmp/fika-rename-admin", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(pane_id, PathBuf::from("/tmp/fika-rename-admin/alpha.txt"));

        app.start_rename_as_administrator_in_pane(pane_id);

        let draft = app.rename_draft.as_ref().unwrap();
        assert_eq!(draft.pane_id, pane_id);
        assert_eq!(
            draft.original_path,
            PathBuf::from("/tmp/fika-rename-admin/alpha.txt")
        );
        assert_eq!(draft.draft_name, "alpha.txt");
        assert!(draft.privileged);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Renaming alpha.txt as administrator"
        );
    }

    #[test]
    fn watcher_rename_retargets_active_rename_draft_without_resetting_input() {
        let root = PathBuf::from("/tmp/fika-rename-watch");
        let old_path = root.join("old.txt");
        let new_path = root.join("new.txt");
        let mut app = test_app_with_entries(root.to_str().unwrap(), &["old.txt"]);
        let pane_id = app.panes.focused().unwrap();

        assert!(app.start_rename_for_path(pane_id, old_path.clone()));
        {
            let draft = app.rename_draft.as_mut().unwrap();
            draft.draft_name = "user-input.md".to_string();
            draft.caret = 4;
            draft.selection = Some((0, 4));
            draft.error = Some("keep me".to_string());
        }
        let generation = app.panes.pane(pane_id).unwrap().generation;

        app.apply_event(DirectoryListerEvent::ItemsRefreshed {
            pane_id,
            generation,
            request_serial: fika_core::RequestSerial(1),
            path: root,
            pairs: vec![RefreshPair {
                old_path: old_path.clone(),
                entry: Some(test_entry("new.txt")),
            }],
        });

        let draft = app.rename_draft.as_ref().unwrap();
        assert_eq!(draft.original_path, new_path);
        assert_eq!(draft.draft_name, "user-input.md");
        assert_eq!(draft.caret, 4);
        assert_eq!(draft.selection, Some((0, 4)));
        assert_eq!(draft.error.as_deref(), Some("keep me"));

        let pane = app.panes.pane(pane_id).unwrap();
        assert_eq!(pane.model.index_of_path(&old_path), None);
        assert_eq!(pane.model.index_of_path(&draft.original_path), Some(0));
        assert_eq!(
            app.panes.selected_paths(pane_id).unwrap(),
            vec![draft.original_path.clone()]
        );
    }

    #[test]
    fn rename_click_positions_caret_from_window_x() {
        let mut app = test_app_with_entries("/tmp/fika-rename-click", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        assert!(
            app.start_rename_for_path(pane_id, PathBuf::from("/tmp/fika-rename-click/alpha.txt"),)
        );
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 200.0,
                width: 500.0,
                height: 300.0,
            },
        ));

        let item = app
            .layout_projection_for_pane(pane_id)
            .unwrap()
            .layout
            .item_with_required_text_width(
                0,
                Some(rename_editor_required_text_width(compact_text_width(
                    "alpha.txt".len() as u16,
                ))),
            )
            .unwrap();
        let window_x = 100.0 + item.text_rect.x + RENAME_TEXT_INSET_X + 6.0;
        let window_y = 200.0 + item.text_rect.y + 4.0;

        assert!(app.set_rename_caret_from_window_position(
            pane_id,
            gpui::point(px(window_x), px(window_y)),
        ));

        let draft = app.rename_draft.as_ref().unwrap();
        assert_eq!(draft.caret, 1);
        assert_eq!(draft.selection, None);
    }

    #[test]
    fn rename_draft_width_expands_layout_and_caret_hit_test() {
        let mut app = test_app_with_entries("/tmp/fika-rename-width", &["a.txt"]);
        let pane_id = app.panes.focused().unwrap();
        assert!(app.start_rename_for_path(pane_id, PathBuf::from("/tmp/fika-rename-width/a.txt"),));
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 200.0,
                width: 700.0,
                height: 360.0,
            },
        ));
        let base_width = app
            .layout_projection_for_pane(pane_id)
            .unwrap()
            .layout
            .item_with_required_text_width(0, None)
            .unwrap()
            .item_rect
            .width;
        let long_name = "much-much-much-longer-rename-target-name.txt";
        app.rename_draft.as_mut().unwrap().draft_name = long_name.to_string();

        let item = app
            .layout_projection_for_pane(pane_id)
            .unwrap()
            .layout
            .item_with_required_text_width(
                0,
                Some(rename_editor_required_text_width(
                    compact_text_width_for_name(long_name),
                )),
            )
            .unwrap();
        assert!(item.item_rect.width > base_width);
        assert!(item.text_rect.width > compact_text_width_for_name("a.txt"));

        let window_x = 100.0 + item.text_rect.x + RENAME_TEXT_INSET_X + item.text_rect.width - 4.0;
        let window_y = 200.0 + item.text_rect.y + 4.0;
        assert!(app.set_rename_caret_from_window_position(
            pane_id,
            gpui::point(px(window_x), px(window_y)),
        ));

        assert!(app.rename_draft.as_ref().unwrap().caret > "a.txt".len());
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
    fn application_chooser_input_action_classifies_caret_editing_and_text() {
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            ApplicationChooserInputAction::Cancel
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            ApplicationChooserInputAction::ChooseFirst
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("left").unwrap()),
            ApplicationChooserInputAction::MoveBackward
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("right").unwrap()),
            ApplicationChooserInputAction::MoveForward
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("home").unwrap()),
            ApplicationChooserInputAction::MoveStart
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("end").unwrap()),
            ApplicationChooserInputAction::MoveEnd
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("delete").unwrap()),
            ApplicationChooserInputAction::Delete
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("a->a").unwrap()),
            ApplicationChooserInputAction::Insert("a".to_string())
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            ApplicationChooserInputAction::Insert("A".to_string())
        );
        assert_eq!(
            application_chooser_input_action(&gpui::Keystroke::parse("secondary-i").unwrap()),
            ApplicationChooserInputAction::Ignore
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
    fn filter_button_routes_open_and_close_to_target_pane() {
        let mut app = test_app_with_entries("/tmp/fika-filter-button", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.toggle_filter_bar_from_button(second);
        let second_filter = app.pane_filters.get(&second).unwrap();
        assert_eq!(app.panes.focused(), Some(second));
        assert!(second_filter.visible);
        assert!(second_filter.focused);
        assert!(app.pane_filters.get(&first).is_none());

        app.set_filter_query(second, "*.rs".to_string());
        assert!(app.filtered_model_for_pane(second).is_some());
        app.panes.focus(first);
        app.toggle_filter_bar_from_button(second);

        let second_filter = app.pane_filters.get(&second).unwrap();
        assert_eq!(app.panes.focused(), Some(second));
        assert!(!second_filter.visible);
        assert!(!second_filter.focused);
        assert!(second_filter.query.is_empty());
        assert!(app.filtered_models.get(&second).is_none());
    }

    #[test]
    fn filter_mode_button_sets_requested_mode() {
        let mut app = test_app_with_entries("/tmp/fika-filter-mode-button", &["alpha.rs"]);
        let pane_id = app.panes.focused().unwrap();

        app.set_filter_mode(pane_id, fika_core::NameFilterMode::PlainText);
        let filter = app.pane_filters.get(&pane_id).unwrap();
        assert!(filter.visible);
        assert!(filter.focused);
        assert_eq!(filter.mode, fika_core::NameFilterMode::PlainText);

        app.set_filter_mode(pane_id, fika_core::NameFilterMode::Glob);
        assert_eq!(
            app.pane_filters.get(&pane_id).map(|filter| filter.mode),
            Some(fika_core::NameFilterMode::Glob)
        );
    }

    #[test]
    fn filter_bar_visibility_primes_pane_viewport_height() {
        let mut app = test_app_with_entries("/tmp/fika-filter-viewport", &["alpha.rs"]);
        let pane_id = app.panes.focused().unwrap();
        {
            let pane = app.panes.pane_mut(pane_id).unwrap();
            pane.view.viewport_height = 360.0;
            pane.view.scroll_y = 120.0;
            pane.view.max_scroll_y = 480.0;
        }

        app.show_filter_bar(pane_id);

        assert_eq!(
            app.panes.pane(pane_id).unwrap().view.viewport_height,
            360.0 - FILTER_BAR_HEIGHT
        );

        app.show_filter_bar(pane_id);

        assert_eq!(
            app.panes.pane(pane_id).unwrap().view.viewport_height,
            360.0 - FILTER_BAR_HEIGHT
        );

        app.close_filter_bar(pane_id);

        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_height, 360.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 120.0);

        app.close_filter_bar(pane_id);

        assert_eq!(app.panes.pane(pane_id).unwrap().view.viewport_height, 360.0);
        assert_eq!(app.panes.pane(pane_id).unwrap().view.scroll_y, 120.0);
    }

    #[test]
    fn pane_toolbar_layout_button_toggles_split_state() {
        let mut app = test_app_with_entries("/tmp/fika-pane-toolbar", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();

        app.toggle_pane_layout_from_button(first);
        let split_ids = app.panes.pane_ids().to_vec();
        assert_eq!(split_ids.len(), 2);
        assert_eq!(app.panes.focused(), Some(split_ids[1]));

        app.toggle_pane_layout_from_button(split_ids[1]);
        let remaining = app.panes.pane_ids().to_vec();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], first);
        assert_eq!(app.panes.focused(), Some(first));

        app.toggle_pane_layout_from_button(first);
        assert_eq!(app.panes.pane_ids().len(), 2);
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
            .update_visible_items(first_ids);
        app.visible_item_slots
            .entry(second)
            .or_default()
            .update_visible_items(second_ids);
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
    fn begin_item_drag_captures_external_uri_list_payload() {
        let temp = test_dir("item-drag-export");
        std::fs::create_dir_all(&temp).unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &["alpha.txt", "beta.txt"]);
        let pane_id = app.panes.focused().unwrap();
        let alpha = temp.join("alpha.txt");
        let beta = temp.join("beta.txt");

        assert!(app.panes.select_only(pane_id, alpha.clone()));
        assert_eq!(
            app.panes.toggle_selection(pane_id, beta.clone()),
            Some(true)
        );
        app.begin_item_drag(ItemDragPayload {
            source_pane: pane_id,
            source_path: alpha.clone(),
            source_selected: true,
        });

        let export = app
            .active_item_drag
            .as_ref()
            .and_then(|drag| drag.export.as_ref())
            .unwrap();
        assert_eq!(export.uri_list_mime, "text/uri-list");
        assert_eq!(export.plain_text_mime, "text/plain");
        assert_eq!(export.paths, vec![alpha.clone(), beta.clone()]);
        assert_eq!(
            export.uri_list,
            format!("file://{}\nfile://{}", alpha.display(), beta.display())
        );
        assert_eq!(
            export.plain_text,
            format!("{}\n{}", alpha.display(), beta.display())
        );
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
        let source = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        assert!(item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));
        assert!(!item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        assert!(!set_test_item_drop_target_for_pane(&mut app, pane_id));

        assert!(app.set_dragged_paths_drop_target_for_directory(
            pane_id,
            std::slice::from_ref(&source),
            target_dir.clone()
        ));
        assert!(!item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));

        app.clear_pane_content_state(pane_id);

        assert!(app.drop_targets.item().is_none());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn item_drop_target_leave_clears_only_matching_target() {
        let temp = test_dir("item-drop-target-leave-clear");
        let first_dir = temp.join("first");
        let second_dir = temp.join("second");
        let source = temp.join("source.txt");
        std::fs::create_dir_all(&first_dir).unwrap();
        std::fs::create_dir_all(&second_dir).unwrap();
        std::fs::write(&source, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        let pane_generation = app.drop_targets.lease_generation();
        assert!(!app.clear_item_drop_target_for_pane(PaneId(999)));
        assert!(item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));
        assert!(app.clear_item_drop_target_for_pane(pane_id));
        assert!(app.drop_targets.lease_generation() > pane_generation);
        assert!(app.drop_targets.item().is_none());

        assert!(app.set_dragged_paths_drop_target_for_directory(
            pane_id,
            std::slice::from_ref(&source),
            first_dir.clone()
        ));
        let directory_generation = app.drop_targets.lease_generation();
        assert!(!app.clear_item_drop_target_for_directory(pane_id, &second_dir));
        assert!(!app.clear_item_drop_target_for_directory(PaneId(999), &first_dir));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &first_dir
        ));
        assert!(app.clear_item_drop_target_for_directory(pane_id, &first_dir));
        assert!(app.drop_targets.lease_generation() > directory_generation);
        assert!(app.drop_targets.item().is_none());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn item_drop_target_rejects_source_directory_as_hover_target() {
        let temp = test_dir("item-drop-source-directory-hover");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        let source_paths = vec![source_dir.clone()];

        assert!(set_test_item_drop_target_for_pane(&mut app, pane_id));
        assert!(item_drop_reject_reason(&source_paths, &source_dir).is_some());
        assert!(app.set_dragged_paths_drop_target_for_directory(
            pane_id,
            &source_paths,
            source_dir.clone()
        ));
        assert!(app.drop_targets.item().is_none());

        assert!(item_drop_reject_reason(&source_paths, &target_dir).is_none());
        assert!(app.set_dragged_paths_drop_target_for_directory(
            pane_id,
            &source_paths,
            target_dir.clone()
        ));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn dragged_paths_drop_target_rejects_current_directory_blank_target() {
        let current = test_dir("drop-current-directory-blank");
        std::fs::create_dir_all(&current).unwrap();
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();

        let update = app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            gpui::point(px(24.0), px(24.0)),
            std::slice::from_ref(&current),
        );
        assert!(!update.accepted());
        assert!(app.drop_targets.item().is_none());

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn external_drag_source_paths_are_normalized_for_drop() {
        let temp = test_dir("external-drag-normalized");
        let parent = temp.join("parent");
        let child = parent.join("child.txt");
        let sibling = temp.join("sibling");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(&child, "child").unwrap();
        std::fs::write(&sibling, "sibling").unwrap();
        let app = test_app_with_entries(temp.to_str().unwrap(), &[]);

        assert_eq!(
            app.external_drag_source_paths(&[
                child.clone(),
                parent.clone(),
                child,
                sibling.clone(),
                sibling.clone(),
            ]),
            vec![parent, sibling]
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn dragged_paths_can_add_place_requires_one_new_directory() {
        let current = test_dir("place-add-current");
        let folder = test_dir("place-add-folder");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        let file = current.join("note.txt");
        std::fs::write(&file, "note").unwrap();
        let mut app = test_app_with_entries(current.to_str().unwrap(), &[]);
        app.places = vec![PlaceEntry {
            group: "",
            marker: "F",
            label: "Folder".to_string(),
            path: folder.clone(),
            device_id: None,
            device_mounted: true,
            editable: true,
            removable: true,
            device_ejectable: false,
            device_can_power_off: false,
        }];

        assert!(!app.dragged_paths_can_add_place(std::slice::from_ref(&file)));
        assert!(!app.dragged_paths_can_add_place(&[folder.clone(), current.clone()]));
        assert!(!app.dragged_paths_can_add_place(std::slice::from_ref(&folder)));
        assert!(app.dragged_paths_can_add_place(std::slice::from_ref(&current)));

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn window_drop_target_from_position_uses_directory_item() {
        let temp = test_dir("window-drop-target-directory");
        let target_dir = temp.join("target");
        let place_source = temp.join("place-source");
        let source_file = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::create_dir_all(&place_source).unwrap();
        std::fs::write(&source_file, "drop").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("source.txt"),
            ]),
        );
        app.set_pane_view_mode(pane_id, ViewMode::Details);
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 200.0,
            },
        ));

        let layout = app.layout_projection_for_pane(pane_id).unwrap().layout;
        let item = layout.item_with_required_text_width(0, None).unwrap();
        let view = app.panes.pane(pane_id).unwrap().view.clone();
        let blank_position = gpui::point(px(100.0 + 280.0), px(50.0 + 180.0));
        let update = app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            blank_position,
            std::slice::from_ref(&source_file),
        );
        assert_eq!(update.kind, Some(PathListDropTargetKind::Pane));
        assert!(item_drop_target_matches_pane(
            app.drop_targets.item(),
            pane_id
        ));

        assert!(app.clear_drag_drop_targets());

        let position = gpui::point(
            px(100.0 + item.visual_rect.x - view.scroll_x + 8.0),
            px(50.0 + item.visual_rect.y - view.scroll_y + 8.0),
        );

        let update = app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            position,
            std::slice::from_ref(&source_file),
        );
        assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        assert_raw_grid_marks_directory_drop_target(&mut app, pane_id, &target_dir);

        assert!(app.clear_drag_drop_targets());
        let update = app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            position,
            std::slice::from_ref(&place_source),
        );
        assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        assert_raw_grid_marks_directory_drop_target(&mut app, pane_id, &target_dir);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn drop_operation_menu_records_target_paths_and_position() {
        let temp = test_dir("drop-operation-menu");
        let target_dir = temp.join("target");
        let source = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source, "drop").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        let position = gpui::point(px(120.0), px(64.0));

        assert!(app.set_dragged_paths_drop_target_for_directory(
            pane_id,
            std::slice::from_ref(&source),
            target_dir.clone()
        ));
        app.show_drop_operation_menu(
            pane_id,
            target_dir.clone(),
            vec![source.clone()],
            false,
            position,
        );

        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        let menu = app.context_menu.as_ref().unwrap();
        assert_eq!(menu.pane_id, pane_id);
        assert_eq!(
            menu.position,
            ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            }
        );
        assert_eq!(
            menu.target,
            ContextMenuTarget::DropOperation {
                target_dir,
                paths: vec![source],
                load_target_dir: false,
            }
        );

        app.dismiss_context_menu();
        assert!(app.context_menu.is_none());
        assert!(app.drop_targets.item().is_none());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn blank_press_clears_selection_and_records_pending_rubber_band() {
        let mut app = test_app_with_entries("/tmp/fika-blank-press", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(pane_id, PathBuf::from("/tmp/fika-blank-press/alpha.txt"));

        assert_eq!(app.panes.selected_count(pane_id), Some(1));

        assert!(app.press_rubber_band_from_blank(
            pane_id,
            ViewPoint {
                x: 10_000.0,
                y: 10_000.0
            }
        ));

        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(!app.rubber_band.active_is_for_pane(pane_id));
        assert!(app.rubber_band.pending_start_for_pane(pane_id).is_some());
    }

    #[test]
    fn blank_window_press_uses_viewport_geometry_for_pending_rubber_band() {
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
            app.press_rubber_band_from_window_if_blank(pane_id, gpui::point(px(500.0), px(300.0)))
        );

        assert_eq!(app.panes.selected_count(pane_id), Some(0));
        assert!(!app.rubber_band.active_is_for_pane(pane_id));
        assert_eq!(
            app.rubber_band.pending_start_for_pane(pane_id),
            Some(ViewPoint { x: 400.0, y: 250.0 })
        );
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

        assert!(!app.press_rubber_band_from_window_if_blank(
            pane_id,
            gpui::point(px(500.0), px(300.0)),
        ));

        assert_eq!(app.panes.selected_count(pane_id), Some(1));
        assert!(!app.rubber_band.active_is_for_pane(pane_id));
        assert!(app.rubber_band.pending_start_for_pane(pane_id).is_none());
    }

    #[test]
    fn rubber_band_drag_activates_pending_and_clamps_to_viewport() {
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
            app.press_rubber_band_from_window_if_blank(pane_id, gpui::point(px(120.0), px(70.0)),)
        );

        assert!(!app.move_rubber_band_drag_from_window(pane_id, gpui::point(px(123.0), px(72.0)),));
        assert!(!app.rubber_band.active_is_for_pane(pane_id));
        assert!(app.rubber_band.pending_start_for_pane(pane_id).is_some());

        assert!(
            app.move_rubber_band_drag_from_window(pane_id, gpui::point(px(1000.0), px(900.0)),)
        );

        assert_eq!(
            app.rubber_band.active_current_for_pane(pane_id),
            Some(ViewPoint { x: 300.0, y: 200.0 })
        );
        let view = &app.panes.pane(pane_id).unwrap().view;
        assert_eq!(
            app.rubber_band
                .active_viewport_rect_for_pane(pane_id, view)
                .unwrap(),
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
            !app.press_rubber_band_from_window_if_blank(pane_id, gpui::point(px(500.0), px(300.0)))
        );

        assert_eq!(app.panes.selected_count(pane_id), Some(1));
        assert!(!app.rubber_band.active_is_for_pane(pane_id));
        assert!(app.rubber_band.pending_start_for_pane(pane_id).is_none());
    }

    #[test]
    fn dnd_item_hit_test_uses_viewport_origin_and_scroll_offset() {
        let mut app = test_app_with_entries(
            "/tmp/fika-dnd-hit-test",
            &["alpha.txt", "beta.txt", "gamma.txt"],
        );
        let pane_id = app.panes.focused().unwrap();
        app.set_pane_view_mode(pane_id, ViewMode::Details);
        assert!(app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 200.0,
            }
        ));
        app.panes
            .set_view_scroll(pane_id, 0.0, 12.0, 0.0, 200.0)
            .unwrap();

        let alpha = PathBuf::from("/tmp/fika-dnd-hit-test/alpha.txt");
        let beta = PathBuf::from("/tmp/fika-dnd-hit-test/beta.txt");
        let layout = app.layout_projection_for_pane(pane_id).unwrap().layout;
        let item = layout.item_with_required_text_width(0, None).unwrap();
        let view = app.panes.pane(pane_id).unwrap().view.clone();
        let point_on_alpha = gpui::point(
            px(100.0 + item.visual_rect.x - view.scroll_x + 8.0),
            px(50.0 + item.visual_rect.y - view.scroll_y + 8.0),
        );

        let point = app
            .content_point_from_window(pane_id, point_on_alpha)
            .unwrap();
        assert_eq!(
            app.item_at_content_point(pane_id, point)
                .map(|hit| hit.path),
            Some(alpha)
        );
        assert_ne!(
            app.item_at_content_point(pane_id, point)
                .map(|hit| hit.path),
            Some(beta)
        );
        assert!(
            app.content_point_from_window(pane_id, gpui::point(px(420.0), px(80.0)))
                .and_then(|point| app.item_at_content_point(pane_id, point))
                .is_none()
        );
    }

    fn configure_retained_hit_test_view(app: &mut FikaApp, pane_id: PaneId, view_mode: ViewMode) {
        app.set_pane_view_mode(pane_id, view_mode);
        let _ = app.set_pane_viewport_bounds(pane_id, 480.0, 280.0, 1_000.0, 1_000.0);
        let _ = app.set_pane_viewport_geometry(
            pane_id,
            ViewRect {
                x: 80.0,
                y: 40.0,
                width: 480.0,
                height: 280.0,
            },
        );
    }

    fn retained_hit_test_item_window_point(
        app: &mut FikaApp,
        pane_id: PaneId,
        layout_index: usize,
    ) -> gpui::Point<gpui::Pixels> {
        let geometry = app
            .pane_viewport_geometries
            .get(&pane_id)
            .expect("viewport geometry")
            .window_rect;
        let layout = app.layout_projection_for_pane(pane_id).unwrap().layout;
        let item = layout
            .item_with_required_text_width(layout_index, None)
            .expect("item layout");
        let view = app.panes.pane(pane_id).unwrap().view.clone();
        gpui::point(
            px(geometry.x + item.icon_rect.x + item.icon_rect.width / 2.0 - view.scroll_x),
            px(geometry.y + item.icon_rect.y + item.icon_rect.height / 2.0 - view.scroll_y),
        )
    }

    fn retained_hit_test_blank_window_point(
        app: &mut FikaApp,
        pane_id: PaneId,
    ) -> gpui::Point<gpui::Pixels> {
        let geometry = app
            .pane_viewport_geometries
            .get(&pane_id)
            .expect("viewport geometry")
            .window_rect;
        for (x, y) in [
            (geometry.right() - 8.0, geometry.bottom() - 8.0),
            (geometry.right() - 8.0, geometry.y + geometry.height / 2.0),
            (geometry.x + geometry.width / 2.0, geometry.bottom() - 8.0),
        ] {
            let point = gpui::point(px(x), px(y));
            if app.item_at_window_position(pane_id, point).is_none() {
                return point;
            }
        }
        panic!("expected a blank viewport point");
    }

    #[test]
    fn retained_hit_testing_drives_context_menus_across_view_modes() {
        let temp = test_dir("retained-hit-context");
        let target_dir = temp.join("target");
        let source_file = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source_file, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("source.txt"),
            ]),
        );

        for view_mode in [ViewMode::Compact, ViewMode::Icons, ViewMode::Details] {
            configure_retained_hit_test_view(&mut app, pane_id, view_mode);
            app.clear_selection(pane_id);
            app.dismiss_context_menu();

            let item_point = retained_hit_test_item_window_point(&mut app, pane_id, 0);
            let hit = app
                .item_at_window_position(pane_id, item_point)
                .expect("directory hit");
            assert_eq!(hit.path, target_dir);
            assert!(hit.is_dir);
            assert!(app.show_item_context_menu(pane_id, hit.path, hit.is_dir, item_point));
            assert!(matches!(
                app.context_menu.as_ref().map(|menu| &menu.target),
                Some(ContextMenuTarget::Item {
                    path,
                    is_dir: true,
                    ..
                }) if path == &target_dir
            ));

            app.dismiss_context_menu();
            let blank_point = retained_hit_test_blank_window_point(&mut app, pane_id);
            assert!(app.item_at_window_position(pane_id, blank_point).is_none());
            assert!(app.show_blank_context_menu_if_blank(pane_id, blank_point));
            assert!(matches!(
                app.context_menu.as_ref().map(|menu| &menu.target),
                Some(ContextMenuTarget::Blank { path, .. }) if path == &temp
            ));
        }

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn retained_hit_testing_routes_drop_targets_across_view_modes() {
        let temp = test_dir("retained-hit-drop");
        let target_dir = temp.join("target");
        let source_file = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source_file, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("source.txt"),
            ]),
        );

        for view_mode in [ViewMode::Compact, ViewMode::Icons, ViewMode::Details] {
            configure_retained_hit_test_view(&mut app, pane_id, view_mode);
            assert!(app.clear_drag_drop_targets() || app.drop_targets.item().is_none());

            let item_point = retained_hit_test_item_window_point(&mut app, pane_id, 0);
            let update = app.update_dragged_paths_drop_target_from_window_position(
                pane_id,
                item_point,
                std::slice::from_ref(&source_file),
            );
            assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
            assert!(item_drop_target_matches_directory(
                app.drop_targets.item(),
                pane_id,
                &target_dir
            ));

            assert!(app.clear_drag_drop_targets());
            let blank_point = retained_hit_test_blank_window_point(&mut app, pane_id);
            let update = app.update_dragged_paths_drop_target_from_window_position(
                pane_id,
                blank_point,
                std::slice::from_ref(&source_file),
            );
            assert_eq!(update.kind, Some(PathListDropTargetKind::Pane));
            assert!(item_drop_target_matches_pane(
                app.drop_targets.item(),
                pane_id
            ));
        }

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn source_item_drag_move_updates_drop_target_from_global_window_position() {
        let temp = test_dir("source-item-drag-global-drop");
        let target_dir = temp.join("target");
        let source_file = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source_file, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("source.txt"),
            ]),
        );

        for view_mode in [ViewMode::Compact, ViewMode::Icons, ViewMode::Details] {
            configure_retained_hit_test_view(&mut app, pane_id, view_mode);
            assert!(app.clear_drag_drop_targets() || app.drop_targets.item().is_none());

            let item_point = retained_hit_test_item_window_point(&mut app, pane_id, 0);
            let (target_pane, update) = app
                .update_dragged_paths_drop_target_from_any_window_position(
                    item_point,
                    std::slice::from_ref(&source_file),
                );
            assert_eq!(target_pane, Some(pane_id));
            assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
            assert!(item_drop_target_matches_directory(
                app.drop_targets.item(),
                pane_id,
                &target_dir
            ));
            assert_raw_grid_marks_directory_drop_target(&mut app, pane_id, &target_dir);

            let outside = gpui::point(px(10.0), px(10.0));
            let (target_pane, update) = app
                .update_dragged_paths_drop_target_from_any_window_position(
                    outside,
                    std::slice::from_ref(&source_file),
                );
            assert_eq!(target_pane, None);
            assert_eq!(update.kind, None);
            assert!(update.changed);
            assert!(app.drop_targets.item().is_none());

            assert!(app.set_place_drag_drop_target_for_path(target_dir.clone()));
            let (target_pane, update) = app
                .update_dragged_paths_drop_target_from_any_window_position(
                    outside,
                    std::slice::from_ref(&source_file),
                );
            assert_eq!(target_pane, None);
            assert_eq!(update.kind, None);
            assert!(!update.changed);
            assert!(place_drop_target_matches_place(
                app.drop_targets.place(),
                &target_dir
            ));
            app.clear_drag_drop_targets();

            let payload = ItemDragPayload {
                source_pane: pane_id,
                source_path: source_file.clone(),
                source_selected: false,
            };
            app.begin_item_drag(payload.clone());
            let (target_pane, update, paths) = app
                .update_active_item_drag_drop_target_from_window_position(pane_id, item_point)
                .expect("active item drag update");
            assert_eq!(paths, vec![source_file.clone()]);
            assert_eq!(target_pane, Some(pane_id));
            assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
            assert!(item_drop_target_matches_directory(
                app.drop_targets.item(),
                pane_id,
                &target_dir
            ));
            app.clear_item_drag(&payload);
            assert!(app.active_item_drag.is_none());
            assert!(app.clear_drag_drop_targets());
        }

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn pane_viewport_position_takes_ownership_from_places_during_drag() {
        let temp = test_dir("pane-dnd-owns-place-target");
        let target_dir = temp.join("target");
        let source_file = temp.join("source.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&source_file, "source").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("source.txt"),
            ]),
        );
        configure_retained_hit_test_view(&mut app, pane_id, ViewMode::Icons);

        let pane_point = retained_hit_test_item_window_point(&mut app, pane_id, 0);
        assert!(app.window_position_is_in_pane_viewport(pane_point));
        assert!(app.set_place_drag_drop_target_for_path(target_dir.clone()));
        assert!(place_drop_target_matches_place(
            app.drop_targets.place(),
            &target_dir
        ));

        assert_eq!(
            app.clear_place_drop_target_if_window_position_is_in_pane_viewport(pane_point),
            Some(true)
        );
        assert!(app.drop_targets.place().is_none());
        assert_eq!(
            app.clear_place_drop_target_if_window_position_is_in_pane_viewport(pane_point),
            Some(false)
        );

        assert!(app.set_place_drag_drop_target_for_path(target_dir.clone()));
        let outside_pane = gpui::point(px(10.0), px(10.0));
        assert_eq!(
            app.clear_place_drop_target_if_window_position_is_in_pane_viewport(outside_pane),
            None
        );
        assert!(place_drop_target_matches_place(
            app.drop_targets.place(),
            &target_dir
        ));

        let payload = ItemDragPayload {
            source_pane: pane_id,
            source_path: source_file.clone(),
            source_selected: false,
        };
        app.begin_item_drag(payload.clone());
        assert!(app.set_place_drag_drop_target_for_path(target_dir.clone()));
        assert!(place_drop_target_matches_place(
            app.drop_targets.place(),
            &target_dir
        ));

        let (target_pane, update, paths) = app
            .update_active_item_drag_drop_target_from_window_position(pane_id, pane_point)
            .expect("active item drag update");
        assert_eq!(paths, vec![source_file.clone()]);
        assert_eq!(target_pane, Some(pane_id));
        assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
        assert!(item_drop_target_matches_directory(
            app.drop_targets.item(),
            pane_id,
            &target_dir
        ));
        assert!(app.drop_targets.place().is_none());
        app.clear_item_drag(&payload);
        assert!(app.clear_drag_drop_targets());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn retained_item_view_behavior_matrix_covers_core_paths() {
        let temp = test_dir("retained-behavior-matrix");
        let target_dir = temp.join("target");
        let alpha = temp.join("alpha.txt");
        let external_source = temp.join("external.txt");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&alpha, "alpha").unwrap();
        std::fs::write(&external_source, "external").unwrap();
        let mut app = test_app_with_entries(temp.to_str().unwrap(), &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            temp.clone(),
            Arc::new(vec![
                test_directory_entry("target"),
                test_entry("alpha.txt"),
            ]),
        );
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: temp.clone(),
                device_id: None,
                device_mounted: true,
                editable: false,
                removable: false,
                device_ejectable: false,
                device_can_power_off: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Target".to_string(),
                path: target_dir.clone(),
                device_id: None,
                device_mounted: true,
                editable: true,
                removable: true,
                device_ejectable: false,
                device_can_power_off: false,
            },
        ];

        for view_mode in [ViewMode::Compact, ViewMode::Icons, ViewMode::Details] {
            configure_retained_hit_test_view(&mut app, pane_id, view_mode);
            app.clear_selection(pane_id);
            app.clear_drag_drop_targets();
            app.dismiss_context_menu();
            app.clear_rename_draft_for_pane(pane_id);

            let target_point = retained_hit_test_item_window_point(&mut app, pane_id, 0);
            let target_hit = app
                .item_at_window_position(pane_id, target_point)
                .expect("directory hit");
            assert_eq!(target_hit.path, target_dir);
            assert!(target_hit.is_dir);

            app.select_only(pane_id, target_hit.path.clone());
            assert_eq!(app.panes.selected_count(pane_id), Some(1));
            assert!(app.panes.is_selected(pane_id, &target_dir));

            app.show_item_context_menu(
                pane_id,
                target_hit.path.clone(),
                target_hit.is_dir,
                target_point,
            );
            assert!(matches!(
                app.context_menu.as_ref().map(|menu| &menu.target),
                Some(ContextMenuTarget::Item {
                    path,
                    is_dir: true,
                    selection_count: 1,
                    ..
                }) if path == &target_dir
            ));
            app.dismiss_context_menu();

            assert!(app.start_rename_for_path(pane_id, alpha.clone()));
            assert_eq!(
                app.rename_draft
                    .as_ref()
                    .map(|draft| draft.original_path.clone()),
                Some(alpha.clone())
            );
            app.clear_rename_draft_for_pane(pane_id);

            let update = app.update_dragged_paths_drop_target_from_window_position(
                pane_id,
                target_point,
                std::slice::from_ref(&alpha),
            );
            assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
            assert!(item_drop_target_matches_directory(
                app.drop_targets.item(),
                pane_id,
                &target_dir
            ));

            let payload = ItemDragPayload {
                source_pane: pane_id,
                source_path: alpha.clone(),
                source_selected: false,
            };
            app.begin_item_drag(payload.clone());
            assert_eq!(app.item_drag_source_paths(&payload), vec![alpha.clone()]);
            assert!(app.active_item_drag.is_some());
            app.clear_item_drag(&payload);

            app.clear_drag_drop_targets();
            assert!(app.drop_targets.item().is_none());
            assert!(app.drop_targets.place().is_none());
            let external_paths =
                app.external_drag_source_paths(&[external_source.clone(), external_source.clone()]);
            assert_eq!(external_paths, vec![external_source.clone()]);
            let update = app.update_dragged_paths_drop_target_from_window_position(
                pane_id,
                target_point,
                &external_paths,
            );
            assert_eq!(update.kind, Some(PathListDropTargetKind::Directory));
            assert!(item_drop_target_matches_directory(
                app.drop_targets.item(),
                pane_id,
                &target_dir
            ));
            app.show_drop_operation_menu(
                pane_id,
                target_dir.clone(),
                external_paths,
                false,
                target_point,
            );
            assert!(matches!(
                app.context_menu.as_ref().map(|menu| &menu.target),
                Some(ContextMenuTarget::DropOperation {
                    target_dir: dir,
                    paths,
                    load_target_dir: false,
                }) if dir == &target_dir && paths == &vec![external_source.clone()]
            ));
            app.dismiss_context_menu();

            app.clear_drag_drop_targets();
            assert!(app.drop_targets.item().is_none());
            assert!(app.drop_targets.place().is_none());
            assert!(app.set_place_drag_drop_target_for_path(target_dir.clone()));
            assert!(place_drop_target_matches_place(
                app.drop_targets.place(),
                &target_dir
            ));
            assert!(app.set_dragged_paths_drop_target_for_directory(
                pane_id,
                std::slice::from_ref(&alpha),
                target_dir.clone()
            ));
            assert!(app.drop_targets.place().is_none());
        }

        let _ = std::fs::remove_dir_all(temp);
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
        app.rubber_band.mark_selection_activity_for_pane(pane_id);
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
        assert!(!app.rubber_band.has_selection_activity_for_pane(pane_id));
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
        app.rubber_band.mark_selection_activity_for_pane(pane_id);

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
        assert!(!app.rubber_band.has_selection_activity_for_pane(pane_id));
    }

    #[test]
    fn rubber_band_selection_right_click_outside_selected_visual_clears() {
        let mut app = test_app_with_entries("/tmp/fika-rubber-context-visual", &["alpha.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.select_only(
            pane_id,
            PathBuf::from("/tmp/fika-rubber-context-visual/alpha.txt"),
        );
        app.rubber_band.mark_selection_activity_for_pane(pane_id);
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
        assert!(!app.rubber_band.has_selection_activity_for_pane(pane_id));
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
        pool.update_visible_items([fika_core::ItemId(1), fika_core::ItemId(2)]);
        assert_eq!(pool.slot_by_item_id.len(), 2);

        let slot_for_one = pool.slot_for_item(fika_core::ItemId(1)).unwrap();
        let slot_for_two = pool.slot_for_item(fika_core::ItemId(2)).unwrap();
        pool.update_visible_items([fika_core::ItemId(2), fika_core::ItemId(3)]);

        assert_eq!(pool.slot_for_item(fika_core::ItemId(2)), Some(slot_for_two));
        assert_eq!(pool.slot_for_item(fika_core::ItemId(3)), Some(slot_for_one));
        assert_eq!(pool.slot_by_item_id.len(), 2);
    }

    #[test]
    fn visible_item_slot_pool_treats_duplicate_visible_ids_as_one_widget() {
        let mut pool = VisibleItemSlotPool::default();
        pool.update_visible_items([
            fika_core::ItemId(7),
            fika_core::ItemId(7),
            fika_core::ItemId(7),
        ]);

        assert_eq!(pool.slot_by_item_id.len(), 1);
        assert!(pool.slot_for_item(fika_core::ItemId(7)).is_some());
        assert!(pool.free_slots.is_empty());
    }

    #[test]
    fn visible_item_slot_pool_caps_recycled_slots() {
        let mut pool = VisibleItemSlotPool::default();
        let visible = (1..=150).map(fika_core::ItemId).collect::<Vec<_>>();
        pool.update_visible_items(visible);
        assert_eq!(pool.slot_by_item_id.len(), 150);

        pool.update_visible_items(std::iter::empty::<fika_core::ItemId>());

        assert!(pool.slot_by_item_id.is_empty());
        assert_eq!(pool.free_slots.len(), VisibleItemSlotPool::MAX_FREE_SLOTS);
    }

    #[test]
    fn visible_thumbnail_candidates_queue_once_and_clear_with_pane() {
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-queue", &["image.png"]);
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let path = pane.model.path_for_index(0).unwrap();
        let candidate = ThumbnailCandidate {
            item_id,
            path: path.clone(),
            modified_secs: 42,
            metadata_complete: true,
            mime_type: Some("image/png".to_string()),
            priority: ThumbnailRequestPriority::Visible,
        };

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![candidate.clone()]
        ));
        assert_eq!(app.thumbnail_scheduler.queued_len(), 1);
        assert_eq!(app.thumbnail_scheduler.seen_len(), 1);
        assert!(
            !app.thumbnail_scheduler
                .queue_candidates(pane_id, generation, vec![candidate])
        );
        assert_eq!(app.thumbnail_scheduler.queued_len(), 1);

        let request = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(request.pane_id(), pane_id);
        assert_eq!(request.generation(), generation);
        assert_eq!(request.item_id(), item_id);
        assert_eq!(request.path(), path.as_path());
        assert_eq!(request.modified_secs(), 42);
        assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);

        app.cancel_thumbnail_work_for_pane(pane_id);
        assert!(app.thumbnail_scheduler.is_empty());
        assert_eq!(app.thumbnail_scheduler.seen_len(), 0);
    }

    #[test]
    fn thumbnail_candidates_queue_visible_before_deferred() {
        let mut app =
            test_app_with_entries("/tmp/fika-thumbnail-priority", &["near.png", "visible.png"]);
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let deferred_id = pane.model.entries()[0].id;
        let visible_id = pane.model.entries()[1].id;
        let deferred_path = pane.model.path_for_index(0).unwrap();
        let visible_path = pane.model.path_for_index(1).unwrap();

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![
                ThumbnailCandidate {
                    item_id: deferred_id,
                    path: deferred_path,
                    modified_secs: 42,
                    metadata_complete: true,
                    mime_type: Some("image/png".to_string()),
                    priority: ThumbnailRequestPriority::Deferred,
                },
                ThumbnailCandidate {
                    item_id: visible_id,
                    path: visible_path,
                    modified_secs: 42,
                    metadata_complete: true,
                    mime_type: Some("image/png".to_string()),
                    priority: ThumbnailRequestPriority::Visible,
                },
            ],
        ));

        let first = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(first.item_id(), visible_id);
        assert_eq!(first.priority(), ThumbnailRequestPriority::Visible);
        let second = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(second.item_id(), deferred_id);
        assert_eq!(second.priority(), ThumbnailRequestPriority::Deferred);
    }

    #[test]
    fn visible_thumbnail_candidate_promotes_existing_deferred_request() {
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-promote", &["image.png"]);
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let path = pane.model.path_for_index(0).unwrap();

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![ThumbnailCandidate {
                item_id,
                path: path.clone(),
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Deferred,
            }]
        ));
        assert_eq!(app.thumbnail_scheduler.queued_len(), 1);
        assert_eq!(app.thumbnail_scheduler.seen_len(), 1);

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![ThumbnailCandidate {
                item_id,
                path,
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Visible,
            }]
        ));

        let request = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(request.item_id(), item_id);
        assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);
        assert!(app.thumbnail_scheduler.is_empty());
    }

    #[test]
    fn deferred_thumbnail_candidates_prune_outside_current_resolve_set() {
        let mut app = test_app_with_entries(
            "/tmp/fika-thumbnail-prune-deferred",
            &["keep.png", "stale.png"],
        );
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let keep_id = pane.model.entries()[0].id;
        let stale_id = pane.model.entries()[1].id;
        let keep_path = pane.model.path_for_index(0).unwrap();
        let stale_path = pane.model.path_for_index(1).unwrap();
        let keep = ThumbnailCandidate {
            item_id: keep_id,
            path: keep_path.clone(),
            modified_secs: 42,
            metadata_complete: true,
            mime_type: Some("image/png".to_string()),
            priority: ThumbnailRequestPriority::Deferred,
        };
        let stale = ThumbnailCandidate {
            item_id: stale_id,
            path: stale_path.clone(),
            modified_secs: 42,
            metadata_complete: true,
            mime_type: Some("image/png".to_string()),
            priority: ThumbnailRequestPriority::Deferred,
        };

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![keep.clone(), stale.clone()]
        ));
        assert_eq!(app.thumbnail_scheduler.queued_len(), 2);
        assert_eq!(app.thumbnail_scheduler.seen_len(), 2);

        assert!(
            !app.thumbnail_scheduler
                .queue_candidates(pane_id, generation, vec![keep.clone()])
        );
        assert_eq!(app.thumbnail_scheduler.queued_len(), 1);
        assert_eq!(app.thumbnail_scheduler.seen_len(), 1);
        let remaining = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(remaining.item_id(), keep_id);
        assert!(app.thumbnail_scheduler.pop_next_request().is_none());

        assert!(
            app.thumbnail_scheduler
                .queue_candidates(pane_id, generation, vec![stale])
        );
        let next = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(next.item_id(), stale_id);
    }

    #[test]
    fn active_deferred_thumbnail_candidates_prune_outside_current_resolve_set() {
        let mut app = test_app_with_entries(
            "/tmp/fika-thumbnail-active-cancel",
            &["keep.png", "stale.png"],
        );
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let keep_id = pane.model.entries()[0].id;
        let stale_id = pane.model.entries()[1].id;
        let keep_path = pane.model.path_for_index(0).unwrap();
        let stale_path = pane.model.path_for_index(1).unwrap();
        let keep = ThumbnailCandidate {
            item_id: keep_id,
            path: keep_path.clone(),
            modified_secs: 42,
            metadata_complete: true,
            mime_type: Some("image/png".to_string()),
            priority: ThumbnailRequestPriority::Deferred,
        };
        let stale = ThumbnailCandidate {
            item_id: stale_id,
            path: stale_path.clone(),
            modified_secs: 42,
            metadata_complete: true,
            mime_type: Some("image/png".to_string()),
            priority: ThumbnailRequestPriority::Deferred,
        };

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![keep.clone(), stale.clone()]
        ));
        let active_batch = app
            .thumbnail_scheduler
            .start_probe_batch(THUMBNAIL_PROBE_BATCH_SIZE)
            .unwrap();
        assert_eq!(active_batch.requests.len(), 2);
        assert!(app.thumbnail_scheduler.is_empty());
        assert_eq!(app.thumbnail_scheduler.seen_len(), 2);

        assert!(
            !app.thumbnail_scheduler
                .queue_candidates(pane_id, generation, vec![keep.clone()])
        );

        assert_eq!(app.thumbnail_scheduler.seen_len(), 1);
        assert!(app.thumbnail_scheduler.contains_seen(
            &fika_core::ThumbnailWorkKey::from_candidate(pane_id, generation, &keep)
        ));
        assert!(!app.thumbnail_scheduler.contains_seen(
            &fika_core::ThumbnailWorkKey::from_candidate(pane_id, generation, &stale)
        ));

        assert!(
            app.thumbnail_scheduler
                .queue_candidates(pane_id, generation, vec![stale])
        );
        let requeued = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(requeued.item_id(), stale_id);
    }

    #[test]
    fn visible_thumbnail_candidate_cancels_active_deferred_and_requeues_visible() {
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-active-promote", &["image.png"]);
        let pane_id = app.panes.focused().unwrap();
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let path = pane.model.path_for_index(0).unwrap();

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![ThumbnailCandidate {
                item_id,
                path: path.clone(),
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Deferred,
            }]
        ));
        let active_batch = app
            .thumbnail_scheduler
            .start_probe_batch(THUMBNAIL_PROBE_BATCH_SIZE)
            .unwrap();
        assert_eq!(active_batch.requests.len(), 1);

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![ThumbnailCandidate {
                item_id,
                path,
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Visible,
            }]
        ));

        let requeued = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(requeued.item_id(), item_id);
        assert_eq!(requeued.priority(), ThumbnailRequestPriority::Visible);
    }

    #[test]
    fn visible_thumbnail_candidates_probe_failure_cache_off_snapshot_path() {
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-failure", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/fika-thumbnail-failure"),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("broken.png"),
                name_width_units: 10,
                target_path: None,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let path = pane.model.path_for_index(0).unwrap();
        let cache_root = std::env::temp_dir().join(format!(
            "fika-thumbnail-failure-cache-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&cache_root);
        app.thumbnail_scheduler.set_cache_root(cache_root.clone());
        let uri = fika_core::thumbnail_uri_for_path(&path).unwrap();
        fika_core::record_thumbnail_failure(&cache_root, &uri, 42).unwrap();

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            generation,
            vec![ThumbnailCandidate {
                item_id,
                path: path.clone(),
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Visible,
            }]
        ));
        let request = app.thumbnail_scheduler.pop_next_request().unwrap();
        assert_eq!(request.modified_secs(), 42);
        assert_eq!(app.thumbnail_scheduler.seen_len(), 1);

        let results = fika_core::thumbnail_probe_results_for_requests(
            cache_root.clone(),
            vec![request.clone()],
            fika_core::ThumbnailProbeCancelHandle::from_requests(&[request]),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].thumbnail_path, None);
        assert!(app.finish_thumbnail_probe_results(results));
        assert!(app.panes.pane(pane_id).unwrap().model.entries()[0].thumbnail_failed);

        let _ = std::fs::remove_dir_all(cache_root);
    }

    #[test]
    fn stale_thumbnail_work_is_cancelled_after_navigation() {
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-stale-a", &["image.png"]);
        let pane_id = app.panes.focused().unwrap();
        let old_generation = app.panes.pane(pane_id).unwrap().generation;
        let item_id = app.panes.pane(pane_id).unwrap().model.entries()[0].id;
        let path = app
            .panes
            .pane(pane_id)
            .unwrap()
            .model
            .path_for_index(0)
            .unwrap();

        assert!(app.thumbnail_scheduler.queue_candidates(
            pane_id,
            old_generation,
            vec![ThumbnailCandidate {
                item_id,
                path,
                modified_secs: 42,
                metadata_complete: true,
                mime_type: Some("image/png".to_string()),
                priority: ThumbnailRequestPriority::Visible,
            }]
        ));

        app.panes
            .load(pane_id, PathBuf::from("/tmp/fika-thumbnail-stale-b"))
            .unwrap();
        app.cancel_stale_thumbnail_work_for_pane(pane_id);

        assert!(app.thumbnail_scheduler.is_empty());
        assert_eq!(app.thumbnail_scheduler.seen_len(), 0);
    }

    #[test]
    fn thumbnail_probe_results_update_matching_model_role_only() {
        let path = PathBuf::from("/tmp/fika-thumbnail-result");
        let mut app = test_app_with_entries("/tmp/fika-thumbnail-result", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            path.clone(),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("image.png"),
                name_width_units: 9,
                target_path: None,
                size_bytes: 128,
                modified_secs: Some(42),
                metadata_complete: true,
                trash_original_path: None,
                trash_deletion_time: None,
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
                is_dir: false,
            })]),
        );
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let path = pane.model.path_for_index(0).unwrap();
        let modified_secs = pane.model.entries()[0].effective_modified_secs().unwrap();
        let thumbnail_path = PathBuf::from("/tmp/fika-thumbnail-cache/normal/image.png");

        assert!(
            !app.finish_thumbnail_probe_results(vec![ThumbnailProbeResult {
                pane_id,
                generation: Generation(generation.0 + 1),
                item_id,
                path: path.clone(),
                modified_secs,
                thumbnail_path: Some(PathBuf::from("/tmp/stale.png")),
            }])
        );
        assert!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .thumbnail_path
                .is_none()
        );

        assert!(
            !app.finish_thumbnail_probe_results(vec![ThumbnailProbeResult {
                pane_id,
                generation,
                item_id,
                path: PathBuf::from("/tmp/fika-thumbnail-result/other.png"),
                modified_secs,
                thumbnail_path: Some(PathBuf::from("/tmp/wrong-path.png")),
            }])
        );
        assert!(
            app.finish_thumbnail_probe_results(vec![ThumbnailProbeResult {
                pane_id,
                generation,
                item_id,
                path,
                modified_secs,
                thumbnail_path: Some(thumbnail_path.clone()),
            }])
        );

        assert_eq!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .thumbnail_path
                .as_deref(),
            Some(thumbnail_path.as_path())
        );
    }

    #[test]
    fn metadata_role_results_update_matching_model_role_only() {
        let path = PathBuf::from("/tmp/fika-metadata-result");
        let mut app = test_app_with_entries("/tmp/fika-metadata-result", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            path.clone(),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                target_path: None,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
                mime_magic_checked: false,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let role = fika_core::EntryMetadataRole {
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        };

        assert!(!app.finish_metadata_role_results(vec![MetadataRoleResult {
            pane_id,
            generation: Generation(generation.0 + 1),
            item_id,
            path: path.join("payload"),
            role: Some(role.clone()),
        }]));
        assert_eq!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .effective_mime_type()
                .map(Arc::as_ref),
            Some(fika_core::GENERIC_BINARY_MIME)
        );

        assert!(!app.finish_metadata_role_results(vec![MetadataRoleResult {
            pane_id,
            generation,
            item_id,
            path: path.join("other"),
            role: Some(role.clone()),
        }]));
        assert_eq!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .effective_mime_type()
                .map(Arc::as_ref),
            Some(fika_core::GENERIC_BINARY_MIME)
        );

        assert!(app.finish_metadata_role_results(vec![MetadataRoleResult {
            pane_id,
            generation,
            item_id,
            path: path.join("payload"),
            role: Some(role),
        }]));
        let entry = &app.panes.pane(pane_id).unwrap().model.entries()[0];
        assert!(entry.effective_metadata_complete());
        assert_eq!(entry.effective_size_bytes(), 12);
        assert_eq!(entry.effective_modified_secs(), Some(42));
        assert_eq!(
            entry.effective_mime_type().map(Arc::as_ref),
            Some("text/plain")
        );
    }

    #[test]
    fn pending_generic_binary_icon_snapshot_uses_preliminary_file_icon() {
        let mut app = test_app_with_entries("/tmp/fika-preliminary-icon", &[]);
        let payload = PathBuf::from("/tmp/fika-preliminary-icon/payload");

        let pending = app.icon_snapshot_for_model_item(
            &payload,
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            false,
            48.0,
        );
        let resolved_binary = app.icon_snapshot_for_model_item(
            &payload,
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            true,
            48.0,
        );

        assert_ne!(pending.icon_name.as_ref(), "application-octet-stream");
        assert_eq!(
            resolved_binary.icon_name.as_ref(),
            "application-octet-stream"
        );
    }

    #[test]
    fn metadata_role_result_updates_final_mime_icon_and_keeps_thumbnail() {
        let path = PathBuf::from("/tmp/fika-metadata-final-role");
        let payload = path.join("payload");
        let mut app = test_app_with_entries("/tmp/fika-metadata-final-role", &[]);
        let pane_id = app.panes.focused().unwrap();
        app.panes.pane_mut(pane_id).unwrap().model.replace_listing(
            path.clone(),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                target_path: None,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                trash_original_path: None,
                trash_deletion_time: None,
                mime_type: Some(Arc::from("application/octet-stream")),
                mime_magic_checked: false,
                is_dir: false,
            })]),
        );
        let pane = app.panes.pane(pane_id).unwrap();
        let generation = pane.generation;
        let item_id = pane.model.entries()[0].id;
        let thumbnail_path = PathBuf::from("/tmp/fika-thumbnail-cache/normal/payload.png");
        app.panes
            .pane_mut(pane_id)
            .unwrap()
            .model
            .set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        assert!(app.finish_metadata_role_results(vec![MetadataRoleResult {
            pane_id,
            generation,
            item_id,
            path: payload,
            role: Some(fika_core::EntryMetadataRole {
                size_bytes: 12,
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            }),
        }]));

        assert_eq!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .effective_mime_type()
                .map(Arc::as_ref),
            Some("image/png")
        );
        assert_eq!(
            app.panes.pane(pane_id).unwrap().model.entries()[0]
                .thumbnail_path
                .as_deref(),
            Some(thumbnail_path.as_path())
        );
    }

    fn test_app_with_entries(path: &str, names: &[&str]) -> FikaApp {
        // Clear any operations left over from earlier tests that share the
        // OperationRuntime singleton.
        if let Ok(runtime) = OperationRuntime::shared() {
            for snapshot in runtime.active_operations() {
                runtime.complete_operation(snapshot.id);
            }
        }
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
            trash_has_items: false,
            trash_monitor: TrashEmptinessMonitor::from_known_state(false),
            hidden_places: BTreeSet::new(),
            hidden_place_sections: BTreeSet::new(),
            place_paint_slots: PlacePaintSlotCache::default(),
            place_row_text_shape_cache: PlacesRowTextShapeCache::default(),
            places_sidebar_width: PLACES_SIDEBAR_DEFAULT_WIDTH,
            places_sidebar_visible: true,
            places_layout_autosmoke_original: None,
            app_settings_path: test_dir("settings").join("settings.tsv"),
            app_settings_save_generation: 0,
            app_settings_save_task_running: false,
            user_places_path: test_dir("user-places").join("places.xbel"),
            device_refresh_pending: false,
            next_device_refresh_at: Instant::now(),
            device_monitor_rx: None,
            device_monitor_active: false,
            next_device_monitor_start_at: Instant::now(),
            file_icons: FileIconCache::default(),
            file_icon_resolve_queue: FileIconResolveQueue::default(),
            theme_icon_readiness: ThemeIconImageReadiness::default(),
            mime_applications: MimeApplicationCache::empty(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            item_view_scroll: ItemViewScrollState::default(),
            metadata_role_scheduler: MetadataRoleScheduler::default(),
            thumbnail_scheduler: ThumbnailScheduler::default(),
            visible_work_keys: HashMap::new(),
            pane_viewport_geometries: HashMap::new(),
            pane_split_ratios: HashMap::new(),
            pane_resize_notify_pending: false,
            last_render_viewport_size: None,
            pane_row_width: 0.0,
            visible_item_slots: HashMap::new(),
            item_paint_slots: HashMap::new(),
            visible_item_snapshot_caches: HashMap::new(),
            static_item_text_shape_caches: HashMap::new(),
            details_text_shape_caches: HashMap::new(),
            item_view_perf: ItemViewPerfState::default(),
            hovered_item: RetainedHoveredItem::default(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            active_item_drag: None,
            drop_targets: DropTargetState::default(),
            drop_target_lease_timer_running: false,
            rename_draft: None,
            rename_next_after_operation: None,
            location_draft: None,
            location_edit_metrics: HashMap::new(),
            place_draft: None,
            network_auth_draft: None,
            chooser: None,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: RubberBandController::default(),
            context_menu: None,
            context_menu_tree_hovered: false,
            context_submenu_hide_generation: 0,
            properties_dialog: None,
            trash_conflict_dialog: None,
            application_chooser: None,
            pane_statuses: HashMap::new(),
            background_tasks_expanded: false,
            background_task_history: VecDeque::new(),
            background_task_detail_dialog: None,
        }
    }

    fn test_entry(name: &str) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            trash_original_path: None,
            trash_deletion_time: None,
            mime_type: None,
            mime_magic_checked: true,
            is_dir: false,
        })
    }

    fn test_directory_entry(name: &str) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            trash_original_path: None,
            trash_deletion_time: None,
            mime_type: None,
            mime_magic_checked: true,
            is_dir: true,
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
            metadata_role: None,
            metadata_refresh_pending: false,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from(name),
                name_width_units: name.len() as u16,
                target_path: None,
                size_bytes,
                modified_secs: None,
                metadata_complete: true,
                trash_original_path: None,
                trash_deletion_time: None,
                mime_type: None,
                mime_magic_checked: true,
                is_dir,
            }),
            thumbnail_path: None,
            thumbnail_failed: false,
        }
    }

    fn context_blank_target() -> ContextMenuTarget {
        ContextMenuTarget::Blank {
            path: PathBuf::from("/tmp/fika-blank"),
            trash_view: false,
            trash_has_items: false,
            open_with_apps: Vec::new(),
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
            label: "Place".to_string(),
            path,
            device_id: None,
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
