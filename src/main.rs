use slint::{
    CloseRequestResponse, ComponentHandle, LogicalSize, Model, ModelRc, SharedString, Timer,
    TimerMode, VecModel,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc;
use std::time::Duration;

mod app;
mod config;
mod desktop;
mod fs;
mod support;

use app::async_bridge::{AsyncBridge, DirectoryReadTracker, build_async_runtime, send_async_event};
use app::chooser::{
    ChooserOutputMetadata, chooser_output_metadata, parse_chooser_choice_spec,
    parse_chooser_filter_spec, safe_child_path, selected_directory_or_current,
    set_chooser_choice_index,
};
use app::context_service_menu;
use app::device_monitor::start_device_monitor;
use app::directory_loading::{
    DirectoryLoadPreparation, directory_entries_match, prepare_directory_load,
    prepare_directory_load_for_target,
};
use app::dnd::{
    MainDndTrace, PlacesDndTrace, SLINT_DROPAREA_BACKEND_SOURCE, dnd_debug_enabled_from_env,
    dnd_main_event_message, dnd_places_event_message, env_flag_is_truthy,
};
use app::events::{
    AsyncEvent, DeviceActionResult, DeviceMountResult, DevicesLoadedResult,
    DirectoryEntriesRemoved, DirectoryLoadResult, EXTERNAL_EDIT_DISCARD_OPERATION,
    EXTERNAL_EDIT_SAVE_OPERATION, ExternalEditResult, FileOpenResult, FileOpenSuccess,
    FileOperationProgress, FileOperationResult, FileUndoResult, LocalSearchIndexResult,
    RecursiveSearchProgress, RecursiveSearchResult, VirtualViewProjection, VirtualViewResult,
};
use app::file_clipboard::{
    apply_clipboard_load_result, apply_clipboard_paste_load_result,
    refresh_clipboard_availability_async, sync_clipboard_ui,
};
use app::file_item_roles_updater::{
    ICON_SIZE_UPDATE_INTERVAL, schedule_thumbnail_roles_for_entries,
    schedule_visible_thumbnail_roles_for_entries, schedule_visible_thumbnail_roles_for_slot,
    thumbnail_size_px,
};
use app::geometry::{
    CompactItemViewLayout, ITEM_VIEW_OVERSCAN_COLUMNS, ItemViewItemBounds, ItemViewLayouter,
    MainItemViewLayout, active_main_pane_width, clamped_split_pane_ratio, inactive_main_pane_width,
    place_drop_geometry, register_menu_geometry_callbacks,
};
use app::item_view::{
    SelectionRect, activate_entry_at_pane_point, cancel_blank_for_slot,
    context_menu_entry_at_pane_point, entry_at_pane_point, item_index_at_pane_point,
    move_blank_for_slot, press_blank_for_slot, press_entry_at_pane_point, release_blank_for_slot,
};
use app::item_view_model::ItemViewModelEntry;
use app::item_view_perf::{self, PerfTimer};
use app::item_view_renderer::{
    ItemViewMediaSource, ItemViewRenderMetrics, ItemViewRenderPlanInput,
    decorate_render_plan_with_metadata,
};
use app::model_update::{
    ItemViewModelUpdateStats, PreparedItemViewSlotProjection,
    item_view_slot_projections_for_entries,
    update_pane_item_view_entries_model_with_slot_projections,
    update_pane_item_view_selection_model,
};
use app::operation_controller::{
    ExternalEditStartDecision, FileUndoRegistrationSummary, FileUndoStartDecision, FileUndoUiState,
    affected_directory_pane_ids, cleanup_file_undo_backup,
};
#[cfg(test)]
use app::pane::PaneHistory;
use app::pane::{
    DirectoryViewState, PaneEntriesRemoved, PaneState, PaneTarget, PreparedDirectoryEntries,
    VirtualViewCache, VirtualViewPrepareRequest,
};
use app::pane_controller::PaneController;
use app::places::{
    add_place, add_place_at_slot, contains_place_path, open_place_new_window, remove_place,
    rename_place, reorder_place_path, restore_default_places, sync_places,
};
use app::search_ui::{
    cancel_active_search_for_slot, recursive_search_cancelled_status,
    recursive_search_finished_status, recursive_search_progress_status, recursive_search_status,
    set_search_filters_for_slot,
};
use app::selection::{
    append_unique_paths, apply_prepared_visible_entry_index_to_pane, filtered_entry_paths_for_slot,
    prepare_visible_entry_index, retained_visible_paths, selection_range_paths_filtered_for_slot,
    selection_rect_paths_filtered_for_slot,
};
#[cfg(test)]
use app::selection::{filtered_entry_count_for_slot, rebuild_visible_entry_index_for_slot};
use app::settings_save::{SettingsSaveScheduler, save_settings_latest};
use app::split_view::{
    directory_status_text, pane_viewport_x_from_ui, set_pane_viewport_ui, sync_focus_navigation_ui,
    sync_navigation_ui, sync_pane_slot_ui, sync_pane_slots_ui, sync_pane_view_ui,
    toggle_split_view,
};
#[cfg(test)]
use app::state::PaneExternalEdit;
use app::state::{AppState, DeviceAction};
use app::thumbnail_pipeline::{
    THUMBNAIL_STATE_LOADED, ThumbnailScheduleEntry, apply_thumbnail_load_to_state_for_pane,
    decorate_entries_with_prepared_thumbnail_keys_for_pane, prepare_thumbnail_keys_for_entries,
};
use app::transfer::{
    cancel_queued_operations, pane_drop_allowed, pane_drop_target_path, place_drop_allowed,
    prepare_current_dir_transfer, prepare_entry_transfer, prepare_pane_transfer,
    prepare_place_transfer, resolve_transfer_conflict, start_next_operation,
    start_transfer_operation,
};
use app::virtual_view::{VirtualViewSnapshotInput, prepare_virtual_view_snapshot_update};
use app::zoom::clamp_zoom_level;
use config::args::{Args, Mode};
use config::paths::{expand_user_path, home_dir, normalize_start_dir};
use config::service_menu_policy::load_service_menu_policy;
use config::settings::{AppSettings, load_settings};
use desktop::{mime_open, open_with};
use fs::devices::{
    device_diagnostics_report, eject_device, mount_device, mounted_devices, unmount_device,
};
use fs::entries::read_entries_async;
use fs::places::default_places;
use fs::{file_actions, privilege, search, thumbnails};

slint::include_modules!();

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewEntry {
    pub(crate) name: SharedString,
    pub(crate) path: SharedString,
    pub(crate) is_dir: bool,
    pub(crate) thumbnail_state: i32,
    pub(crate) media_token: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileEntry {
    pub(crate) name: SharedString,
    pub(crate) path: SharedString,
    pub(crate) group: SharedString,
    pub(crate) location: SharedString,
    pub(crate) kind: SharedString,
    pub(crate) size: SharedString,
    pub(crate) size_bytes: f32,
    pub(crate) modified: SharedString,
    pub(crate) modified_age_days: i32,
    pub(crate) is_dir: bool,
}

const THUMBNAIL_FLUSH_COALESCE: Duration = Duration::from_millis(16);

struct PaneViewSyncScheduler {
    ui: slint::Weak<AppWindow>,
    state: Rc<RefCell<AppState>>,
    bridge: AsyncBridge,
    icon_size_update_pending: Rc<Cell<bool>>,
    syncing: Cell<bool>,
}

impl PaneViewSyncScheduler {
    fn new(
        ui: slint::Weak<AppWindow>,
        state: Rc<RefCell<AppState>>,
        bridge: AsyncBridge,
        icon_size_update_pending: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            ui,
            state,
            bridge,
            icon_size_update_pending,
            syncing: Cell::new(false),
        }
    }

    fn request(&self, slot: i32) {
        let timer = PerfTimer::start();
        if self.syncing.get() {
            item_view_perf::log(format_args!(
                "view_sync_request slot={} reentrant_skip=true",
                slot
            ));
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.syncing.set(true);
        let icon_size_update_pending;
        if self.icon_size_update_pending.get() {
            icon_size_update_pending = true;
            sync_pane_viewport_for_slot_with_thumbnail_scheduling(
                &ui,
                &self.state,
                &self.bridge,
                slot,
                false,
            );
        } else {
            icon_size_update_pending = false;
            sync_pane_viewport_for_slot(&ui, &self.state, &self.bridge, slot);
        }
        self.syncing.set(false);
        item_view_perf::log(format_args!(
            "view_sync_request slot={} reentrant_skip=false icon_size_pending={} sync_ms={:.3}",
            slot,
            icon_size_update_pending,
            timer.elapsed_ms()
        ));
    }

    fn flush_all(&self) {}
}

struct IconSizeUpdateScheduler {
    timer: Timer,
    pending: Rc<Cell<bool>>,
}

impl IconSizeUpdateScheduler {
    fn new(
        ui: slint::Weak<AppWindow>,
        state: Rc<RefCell<AppState>>,
        bridge: AsyncBridge,
        pending: Rc<Cell<bool>>,
    ) -> Self {
        let timer = Timer::default();
        let timer_ui = ui;
        let timer_state = Rc::clone(&state);
        let timer_bridge = bridge;
        let timer_pending = Rc::clone(&pending);

        timer.start(
            TimerMode::SingleShot,
            ICON_SIZE_UPDATE_INTERVAL,
            move || {
                if !timer_pending.replace(false) {
                    return;
                }
                let Some(ui) = timer_ui.upgrade() else {
                    return;
                };
                update_icon_size_for_visible_panes(&ui, &timer_state, &timer_bridge);
            },
        );
        timer.stop();

        Self { timer, pending }
    }

    fn trigger_icon_size_update(&self) {
        self.pending.set(true);
        self.timer.restart();
    }

    fn visible_index_range_updates_enabled(&self) -> bool {
        !self.pending.get()
    }
}

struct PaneLayoutSyncScheduler {
    ui: slint::Weak<AppWindow>,
    state: Rc<RefCell<AppState>>,
    bridge: AsyncBridge,
    pane_view_sync: Rc<PaneViewSyncScheduler>,
    icon_size_update: IconSizeUpdateScheduler,
}

impl PaneLayoutSyncScheduler {
    fn new(
        ui: slint::Weak<AppWindow>,
        state: Rc<RefCell<AppState>>,
        bridge: AsyncBridge,
        pane_view_sync: Rc<PaneViewSyncScheduler>,
        icon_size_update_pending: Rc<Cell<bool>>,
    ) -> Self {
        let icon_size_update = IconSizeUpdateScheduler::new(
            ui.clone(),
            Rc::clone(&state),
            bridge.clone(),
            icon_size_update_pending,
        );

        Self {
            ui,
            state,
            bridge,
            pane_view_sync,
            icon_size_update,
        }
    }

    fn sync_now(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.pane_view_sync.flush_all();
        if self.icon_size_update.visible_index_range_updates_enabled() {
            sync_visible_pane_layouts(&ui, &self.state, &self.bridge);
        } else {
            sync_visible_pane_layouts_with_thumbnail_scheduling(
                &ui,
                &self.state,
                &self.bridge,
                false,
            );
        }
    }

    fn set_icon_zoom_level_now(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let timer = PerfTimer::start();
        let pane_count = ui.get_pane_slots().row_count();
        let zoom_level = ui.get_icon_zoom_level();
        self.pane_view_sync.flush_all();
        apply_visible_pane_zoom_style_options(&ui, &self.state, &self.bridge);
        self.icon_size_update.trigger_icon_size_update();
        item_view_perf::log(format_args!(
            "zoom level={} panes={} layout_ms={:.3} icon_size_timer_pending=true",
            zoom_level,
            pane_count,
            timer.elapsed_ms()
        ));
    }
}

struct ThumbnailFlushScheduler {
    timer: Timer,
    pending: ThumbnailPendingQueue,
}

type ThumbnailPendingLoad = (u64, u64, thumbnails::ThumbnailLoad);
type ThumbnailPendingQueue = Rc<RefCell<VecDeque<ThumbnailPendingLoad>>>;

enum VirtualViewSyncRequest {
    Cached {
        sync: CachedVirtualViewportSync,
        publish_layout: bool,
    },
    Deferred,
    Prepare(VirtualViewPrepareRequest),
}

#[derive(Debug, PartialEq)]
struct CachedVirtualViewportSync {
    viewport_x: f32,
    publish_viewport: bool,
    cached_range: std::ops::Range<usize>,
    visible_range: std::ops::Range<usize>,
    entry_count: usize,
}

impl ThumbnailFlushScheduler {
    fn new(
        ui: slint::Weak<AppWindow>,
        state: Rc<RefCell<AppState>>,
        bridge: AsyncBridge,
        icon_size_update_pending: Rc<Cell<bool>>,
    ) -> Self {
        let timer = Timer::default();
        let pending = Rc::new(RefCell::new(VecDeque::<(
            u64,
            u64,
            thumbnails::ThumbnailLoad,
        )>::new()));
        let timer_pending = Rc::clone(&pending);

        timer.start(TimerMode::SingleShot, THUMBNAIL_FLUSH_COALESCE, move || {
            let Some(ui) = ui.upgrade() else {
                return;
            };
            flush_thumbnail_results(
                &ui,
                &state,
                &bridge,
                &timer_pending,
                icon_size_update_pending.get(),
            );
        });
        timer.stop();

        Self { timer, pending }
    }

    fn push(&self, pane_id: u64, generation: u64, load: thumbnails::ThumbnailLoad) {
        self.pending
            .borrow_mut()
            .push_back((pane_id, generation, load));
        self.timer.restart();
    }
}

enum UiSignal {
    AsyncResultsReady,
    PaneViewChanged(i32),
    PaneLayoutChanged,
    IconZoomLayoutChanged,
    PaneSlotsRefreshRequested,
    PanePathTextChanged(i32, String),
    PanePathFocusChanged(i32, bool),
    PaneSlotRefreshRequested(i32),
    PaneViewportChanged(i32, f32),
    Refresh,
    GoHome,
    GoParent,
    GoRoot,
    PanePathSubmitted(i32, String),
    OpenPlace(String),
    OpenDevice(String, bool),
    UnmountDevice(String, String),
    EjectDevice(String, String),
    OpenSearch,
    OpenSearchForSlot(i32),
    SearchSubmitted(i32, String, bool),
    CancelSearch(i32),
    SearchFiltersChanged(i32, i32, i32, i32),
    SearchCloseRequested(i32),
    OpenPath(String),
    ContextServiceAction(i32),
    PrepareContextServiceSubmenu(String),
    ContextServiceActionEnabled(String, bool),
    ContextServicePolicyModeChanged(i32),
    ChooserAccept(String),
    ChooserSelectFilter(i32),
    ChooserSelectChoice(i32, i32),
    GoBack,
    GoForward,
    PaneGoBack(i32),
    PaneGoForward(i32),
    PaneFocus(i32),
    ToggleSplitView,
    PaneItemViewItemActivated(i32, f32, f32),
    PaneItemViewBlankPressed(i32, f32, f32, bool),
    PaneItemViewBlankReleased(i32, f32, f32),
    PaneItemViewBlankCancelled(i32),
    PaneClearSelection(i32),
    ClearSelection,
    SelectAllVisible,
    AddPlace(String),
    AddPlaceAtSlot(String, i32),
    RenamePlace(i32, String),
    RemovePlace(i32),
    RestoreDefaultPlaces,
    OpenPlaceNewWindow(i32),
    PaneDropTargetChanged(i32, i32),
    TracePlacesDrop {
        phase: String,
        mime_type: String,
        payload: String,
        x: f32,
        y: f32,
        slot: i32,
        target: i32,
        over_gap: bool,
        over_item: bool,
    },
    TraceMainDrop {
        phase: String,
        mime_type: String,
        payload: String,
        x: f32,
        y: f32,
        rejected: bool,
        target_path: String,
    },
    TransferOperation(String, String, String),
    TransferConflictChoice(String),
    PrivilegedPromptAccept,
    PrivilegedPromptDismiss,
    CommitExternalEdit(i32),
    DiscardExternalEdit(i32),
    UndoLastOperation,
    CancelQueuedOperations,
    ReorderPlacePath(String, i32),
    PersistUiState,
    DarkModeChanged,
}

struct UiSignalBus {
    ui: slint::Weak<AppWindow>,
    state: Rc<RefCell<AppState>>,
    bridge: AsyncBridge,
    pane_view_sync: Rc<PaneViewSyncScheduler>,
    pane_layout_sync: Rc<PaneLayoutSyncScheduler>,
    thumbnail_flush: Rc<ThumbnailFlushScheduler>,
    settings_save: Rc<SettingsSaveScheduler>,
    async_rx: Rc<RefCell<mpsc::Receiver<AsyncEvent>>>,
    chooser_save_files: Vec<String>,
    chooser_mode: bool,
}

struct UiSignalBusInput {
    ui: slint::Weak<AppWindow>,
    state: Rc<RefCell<AppState>>,
    bridge: AsyncBridge,
    pane_view_sync: Rc<PaneViewSyncScheduler>,
    pane_layout_sync: Rc<PaneLayoutSyncScheduler>,
    thumbnail_flush: Rc<ThumbnailFlushScheduler>,
    settings_save: Rc<SettingsSaveScheduler>,
    async_rx: Rc<RefCell<mpsc::Receiver<AsyncEvent>>>,
    chooser_save_files: Vec<String>,
    chooser_mode: bool,
}

impl UiSignalBus {
    fn new(input: UiSignalBusInput) -> Self {
        Self {
            ui: input.ui,
            state: input.state,
            bridge: input.bridge,
            pane_view_sync: input.pane_view_sync,
            pane_layout_sync: input.pane_layout_sync,
            thumbnail_flush: input.thumbnail_flush,
            settings_save: input.settings_save,
            async_rx: input.async_rx,
            chooser_save_files: input.chooser_save_files,
            chooser_mode: input.chooser_mode,
        }
    }

    fn emit(&self, signal: UiSignal) {
        match signal {
            UiSignal::AsyncResultsReady => self.drain_async_results(),
            UiSignal::PaneViewChanged(slot) => self.pane_view_sync.request(slot),
            UiSignal::PaneLayoutChanged => self.pane_layout_sync.sync_now(),
            UiSignal::IconZoomLayoutChanged => self.pane_layout_sync.set_icon_zoom_level_now(),
            UiSignal::PaneSlotsRefreshRequested => {
                if let Some(ui) = self.ui.upgrade() {
                    sync_pane_slots_ui(&ui, &self.state);
                }
            }
            UiSignal::PanePathTextChanged(slot, text) => {
                if let Some(pane) = self.state.borrow_mut().panes.pane_mut_for_slot(slot) {
                    pane.path_input_text = text;
                }
                if let Some(ui) = self.ui.upgrade() {
                    sync_pane_slot_ui(&ui, &self.state, slot);
                }
            }
            UiSignal::PanePathFocusChanged(slot, focused) => {
                if let Some(pane) = self.state.borrow_mut().panes.pane_mut_for_slot(slot) {
                    pane.path_focused = focused;
                }
                if let Some(ui) = self.ui.upgrade() {
                    if focused {
                        focus_pane_slot(&ui, &self.state, slot);
                    }
                    sync_pane_slot_ui(&ui, &self.state, slot);
                }
            }
            UiSignal::PaneSlotRefreshRequested(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    sync_pane_slot_ui(&ui, &self.state, slot);
                }
            }
            UiSignal::PaneViewportChanged(slot, viewport_x) => {
                if let Some(pane) = self.state.borrow_mut().panes.pane_mut_for_slot(slot) {
                    pane.view.viewport_x = viewport_x;
                }
            }
            UiSignal::Refresh => {
                if let Some(ui) = self.ui.upgrade() {
                    refresh_focused_directory(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::GoHome => {
                if let Some(ui) = self.ui.upgrade() {
                    navigate_focused_to(&ui, &self.state, &self.bridge, home_dir());
                }
            }
            UiSignal::GoParent => {
                if let Some(ui) = self.ui.upgrade() {
                    go_parent(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::GoRoot => {
                if let Some(ui) = self.ui.upgrade() {
                    navigate_focused_to(&ui, &self.state, &self.bridge, PathBuf::from("/"));
                }
            }
            UiSignal::PanePathSubmitted(slot, path) => {
                if let Some(ui) = self.ui.upgrade() {
                    focus_pane_slot(&ui, &self.state, slot);
                    let requested = expand_user_path(path.as_str());
                    if !requested.is_dir() {
                        reset_pane_path_input_for_slot(&ui, slot);
                        set_status(&ui, &self.state, "Path is not a readable directory");
                    } else {
                        navigate_pane_to_slot(&ui, &self.state, &self.bridge, slot, requested);
                    }
                }
            }
            UiSignal::OpenPlace(path) => {
                if let Some(ui) = self.ui.upgrade() {
                    let slot = focus_current_ui_pane_slot(&ui, &self.state);
                    let requested = expand_user_path(path.as_str());
                    if fs::file_ops::is_trash_files_dir(&requested) {
                        match fs::file_ops::ensure_trash_dirs() {
                            Ok(()) => navigate_pane_to_slot(
                                &ui,
                                &self.state,
                                &self.bridge,
                                slot,
                                requested,
                            ),
                            Err(err) => set_status(
                                &ui,
                                &self.state,
                                &format!("Trash is not available: {err}"),
                            ),
                        }
                    } else if requested.is_dir() {
                        navigate_pane_to_slot(&ui, &self.state, &self.bridge, slot, requested);
                    } else {
                        set_status(&ui, &self.state, "Place is not available");
                    }
                }
            }
            UiSignal::OpenDevice(path, mounted) => {
                if let Some(ui) = self.ui.upgrade() {
                    if !mounted {
                        if register_pending_device_action(&self.state, &path, "mount") {
                            set_status(&ui, &self.state, "Mounting device...");
                            mount_device_async(&self.bridge, path);
                        } else {
                            set_status(&ui, &self.state, "Device action already in progress");
                        }
                        return;
                    }
                    let requested = expand_user_path(path.as_str());
                    if requested.is_dir() {
                        let slot = focus_current_ui_pane_slot(&ui, &self.state);
                        navigate_pane_to_slot(&ui, &self.state, &self.bridge, slot, requested);
                    } else {
                        set_status(&ui, &self.state, "Device is not available");
                    }
                }
            }
            UiSignal::UnmountDevice(device_path, mount_path) => {
                self.start_device_action("unmount", device_path, mount_path);
            }
            UiSignal::EjectDevice(device_path, mount_path) => {
                self.start_device_action("eject", device_path, mount_path);
            }
            UiSignal::OpenSearch => {
                if let Some(ui) = self.ui.upgrade() {
                    open_search(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::OpenSearchForSlot(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    open_search_for_slot(&ui, &self.state, &self.bridge, slot);
                }
            }
            UiSignal::SearchSubmitted(slot, query, recursive) => {
                if let Some(ui) = self.ui.upgrade() {
                    submit_search_for_slot(
                        &ui,
                        &self.state,
                        &self.bridge,
                        slot,
                        query.as_str(),
                        recursive,
                    );
                }
            }
            UiSignal::CancelSearch(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    cancel_recursive_search_for_slot(&ui, &self.state, &self.bridge, slot);
                }
            }
            UiSignal::SearchFiltersChanged(slot, kind, modified, size) => {
                if let Some(ui) = self.ui.upgrade() {
                    update_search_filters_for_slot(
                        &ui,
                        &self.state,
                        &self.bridge,
                        slot,
                        kind,
                        modified,
                        size,
                    );
                }
            }
            UiSignal::SearchCloseRequested(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    close_search_for_slot(&ui, &self.state, &self.bridge, slot);
                }
            }
            UiSignal::OpenPath(path) => {
                if let Some(ui) = self.ui.upgrade() {
                    open_path(&ui, &self.state, path.as_str(), &self.bridge);
                }
            }
            UiSignal::ContextServiceAction(index) => {
                if let Some(ui) = self.ui.upgrade() {
                    context_service_menu::launch_action_async(
                        &ui,
                        &self.state,
                        &self.bridge,
                        index,
                    );
                }
            }
            UiSignal::PrepareContextServiceSubmenu(group) => {
                if let Some(ui) = self.ui.upgrade() {
                    context_service_menu::prepare_submenu_actions(&ui, &self.state, group.as_str());
                }
            }
            UiSignal::ContextServiceActionEnabled(id, enabled) => {
                if let Some(ui) = self.ui.upgrade() {
                    context_service_menu::set_action_enabled(
                        &ui,
                        &self.state,
                        id.as_str(),
                        enabled,
                    );
                }
            }
            UiSignal::ContextServicePolicyModeChanged(mode) => {
                if let Some(ui) = self.ui.upgrade() {
                    context_service_menu::set_policy_mode(&ui, &self.state, mode);
                }
            }
            UiSignal::ChooserAccept(name) => {
                if let Some(ui) = self.ui.upgrade() {
                    chooser_accept(&ui, &self.state, name.as_str(), &self.chooser_save_files);
                }
            }
            UiSignal::ChooserSelectFilter(filter_index) => {
                if let Some(ui) = self.ui.upgrade() {
                    select_chooser_filter(&ui, &self.state, &self.bridge, filter_index);
                }
            }
            UiSignal::ChooserSelectChoice(choice_index, option_index) => {
                if let Some(ui) = self.ui.upgrade() {
                    select_chooser_choice(&ui, &self.state, choice_index, option_index);
                }
            }
            UiSignal::GoBack => {
                if let Some(ui) = self.ui.upgrade() {
                    go_back(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::GoForward => {
                if let Some(ui) = self.ui.upgrade() {
                    go_forward(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::PaneGoBack(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    go_pane_back_slot(&ui, &self.state, &self.bridge, slot);
                }
            }
            UiSignal::PaneGoForward(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    go_pane_forward_slot(&ui, &self.state, &self.bridge, slot);
                }
            }
            UiSignal::PaneFocus(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    focus_pane_slot(&ui, &self.state, slot);
                }
            }
            UiSignal::ToggleSplitView => {
                if let Some(ui) = self.ui.upgrade() {
                    toggle_split_view(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::PaneItemViewItemActivated(slot, x, y) => {
                if let Some(ui) = self.ui.upgrade() {
                    activate_item_view_entry_at_point_for_slot(
                        &ui,
                        &self.state,
                        slot,
                        x,
                        y,
                        &self.bridge,
                    );
                }
            }
            UiSignal::PaneItemViewBlankPressed(slot, x, y, toggle) => {
                press_item_view_blank_for_slot(&self.state, slot, x, y, toggle);
            }
            UiSignal::PaneItemViewBlankReleased(slot, x, y) => {
                if let Some(ui) = self.ui.upgrade() {
                    release_item_view_blank_for_slot(&ui, &self.state, &self.bridge, slot, x, y);
                }
            }
            UiSignal::PaneItemViewBlankCancelled(slot) => {
                cancel_item_view_blank_for_slot(&self.state, slot);
            }
            UiSignal::PaneClearSelection(slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    clear_selection_for_slot(&ui, &self.state, slot);
                }
            }
            UiSignal::ClearSelection => {
                if let Some(ui) = self.ui.upgrade() {
                    clear_selection(&ui, &self.state);
                }
            }
            UiSignal::SelectAllVisible => {
                if let Some(ui) = self.ui.upgrade() {
                    select_all_visible(&ui, &self.state);
                }
            }
            UiSignal::AddPlace(path) => {
                if let Some(ui) = self.ui.upgrade() {
                    add_place(&ui, &self.state, PathBuf::from(path.as_str()));
                    prefetch_sidebar_locations_async(&self.state, &self.bridge);
                }
            }
            UiSignal::AddPlaceAtSlot(path, slot) => {
                if let Some(ui) = self.ui.upgrade() {
                    add_place_at_slot(&ui, &self.state, PathBuf::from(path.as_str()), slot);
                    prefetch_sidebar_locations_async(&self.state, &self.bridge);
                }
            }
            UiSignal::RenamePlace(index, label) => {
                if let Some(ui) = self.ui.upgrade() {
                    rename_place(&ui, &self.state, index, label.as_str());
                }
            }
            UiSignal::RemovePlace(index) => {
                if let Some(ui) = self.ui.upgrade() {
                    remove_place(&ui, &self.state, index);
                    prefetch_sidebar_locations_async(&self.state, &self.bridge);
                }
            }
            UiSignal::RestoreDefaultPlaces => {
                if let Some(ui) = self.ui.upgrade() {
                    restore_default_places(&ui, &self.state);
                    prefetch_sidebar_locations_async(&self.state, &self.bridge);
                }
            }
            UiSignal::OpenPlaceNewWindow(index) => {
                if let Some(ui) = self.ui.upgrade() {
                    open_place_new_window(&ui, &self.state, index);
                }
            }
            UiSignal::PaneDropTargetChanged(slot, slice_index) => {
                if let Some(ui) = self.ui.upgrade() {
                    set_pane_drop_target_slice_index_ui(&ui, &self.state, slot, slice_index);
                }
            }
            UiSignal::TracePlacesDrop {
                phase,
                mime_type,
                payload,
                x,
                y,
                slot,
                target,
                over_gap,
                over_item,
            } => dnd_log_places_event(PlacesDndTrace {
                backend: SLINT_DROPAREA_BACKEND_SOURCE,
                phase: phase.as_str(),
                mime_type: mime_type.as_str(),
                payload: payload.as_str(),
                x,
                y,
                slot,
                target,
                over_gap,
                over_item,
            }),
            UiSignal::TraceMainDrop {
                phase,
                mime_type,
                payload,
                x,
                y,
                rejected,
                target_path,
            } => dnd_log_main_event(MainDndTrace {
                backend: SLINT_DROPAREA_BACKEND_SOURCE,
                phase: phase.as_str(),
                mime_type: mime_type.as_str(),
                payload: payload.as_str(),
                x,
                y,
                rejected,
                target_path: target_path.as_str(),
            }),
            UiSignal::TransferOperation(operation, source, target) => {
                if let Some(ui) = self.ui.upgrade() {
                    start_transfer_operation(
                        &ui,
                        &self.state,
                        &self.bridge,
                        operation.as_str(),
                        source.as_str(),
                        target.as_str(),
                    );
                }
            }
            UiSignal::TransferConflictChoice(decision) => {
                if let Some(ui) = self.ui.upgrade() {
                    resolve_transfer_conflict(&ui, &self.state, &self.bridge, decision.as_str());
                }
            }
            UiSignal::PrivilegedPromptAccept => {
                if let Some(ui) = self.ui.upgrade() {
                    let command = self.state.borrow_mut().pending_privileged_command.take();
                    ui.set_privileged_prompt_open(false);
                    if let Some(command) = command {
                        start_privileged_operation(&ui, &self.state, &self.bridge, command);
                    }
                }
            }
            UiSignal::PrivilegedPromptDismiss => {
                self.state.borrow_mut().pending_privileged_command = None;
                if let Some(ui) = self.ui.upgrade() {
                    ui.set_privileged_prompt_open(false);
                    set_status(&ui, &self.state, "Administrator operation cancelled");
                }
            }
            UiSignal::CommitExternalEdit(slot) => {
                self.start_external_edit_resolution(slot, EXTERNAL_EDIT_SAVE_OPERATION);
            }
            UiSignal::DiscardExternalEdit(slot) => {
                self.start_external_edit_resolution(slot, EXTERNAL_EDIT_DISCARD_OPERATION);
            }
            UiSignal::UndoLastOperation => {
                if let Some(ui) = self.ui.upgrade() {
                    start_file_undo(&ui, &self.state, &self.bridge);
                }
            }
            UiSignal::CancelQueuedOperations => {
                if let Some(ui) = self.ui.upgrade() {
                    cancel_queued_operations(&ui, &self.state);
                }
            }
            UiSignal::ReorderPlacePath(path, to) => {
                if let Some(ui) = self.ui.upgrade() {
                    reorder_place_path(&ui, &self.state, path.as_str(), to);
                    prefetch_sidebar_locations_async(&self.state, &self.bridge);
                }
            }
            UiSignal::PersistUiState => {
                if let Some(ui) = self.ui.upgrade() {
                    self.settings_save
                        .schedule(current_settings(&ui, &self.state));
                }
            }
            UiSignal::DarkModeChanged => {
                if let Some(ui) = self.ui.upgrade() {
                    refresh_visible_pane_tile_frame_rasters(&ui, &self.state);
                }
            }
        }
    }

    fn drain_async_results(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        while let Ok(event) = self.async_rx.borrow_mut().try_recv() {
            apply_async_event(&ui, &self.state, &self.bridge, &self.thumbnail_flush, event);
        }
    }

    fn start_device_action(&self, action: &'static str, device_path: String, mount_path: String) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let mount_path = mounted_device_path(mount_path.as_str());
        if register_pending_device_action(&self.state, &device_path, action) {
            set_status(
                &ui,
                &self.state,
                &format!("{} device...", device_action_label(action)),
            );
            device_action_async(&self.bridge, action, device_path, mount_path);
        } else {
            set_status(&ui, &self.state, "Device action already in progress");
        }
    }

    fn start_external_edit_resolution(&self, slot: i32, operation: &'static str) {
        if let Some(ui) = self.ui.upgrade() {
            start_external_edit_resolution(&ui, &self.state, &self.bridge, slot, operation);
        }
    }

    fn query_clear_focused_search(&self) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| clear_focused_search(&ui, &self.state, &self.bridge))
    }

    fn query_item_view_item_pressed(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        toggle: bool,
        range: bool,
    ) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            press_item_view_entry_at_point_for_slot(
                &ui,
                &self.state,
                &self.bridge,
                slot,
                x,
                y,
                toggle,
                range,
            )
        })
    }

    fn query_item_view_item_context_menu(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        abs_x: f32,
        abs_y: f32,
    ) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            request_item_view_entry_context_menu_at_point_for_slot(
                &ui,
                &self.state,
                &self.bridge,
                ItemViewContextMenuRequest {
                    slot,
                    x,
                    y,
                    abs_x,
                    abs_y,
                },
            )
        })
    }

    fn query_item_view_blank_moved(&self, slot: i32, x: f32, y: f32) -> bool {
        move_item_view_blank_for_slot(&self.state, slot, x, y)
    }

    fn query_is_place(&self, path: &str) -> bool {
        contains_place_path(&self.state.borrow(), path)
    }

    fn query_prepare_place_transfer(
        &self,
        source: &str,
        target_index: i32,
        x: f32,
        y: f32,
    ) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| prepare_place_transfer(&ui, &self.state, source, target_index, x, y))
    }

    fn query_prepare_entry_transfer(
        &self,
        source: &str,
        target_index: i32,
        x: f32,
        y: f32,
    ) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| prepare_entry_transfer(&ui, &self.state, source, target_index, x, y))
    }

    fn query_prepare_current_dir_transfer(
        &self,
        source: &str,
        label: &str,
        x: f32,
        y: f32,
    ) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| prepare_current_dir_transfer(&ui, &self.state, source, label, x, y))
    }

    fn query_pane_prepare_transfer(&self, slot: i32, source: &str, x: f32, y: f32) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| prepare_pane_transfer_for_slot(&ui, &self.state, slot, source, x, y))
    }

    fn query_pane_drop_target_path(&self, slot: i32, x: f32, y: f32, source: &str) -> SharedString {
        let Some(ui) = self.ui.upgrade() else {
            return SharedString::new();
        };
        let state = self.state.borrow();
        pane_drop_target_path_for_slot(&ui, &state, slot, x, y, Path::new(source))
            .map_or_else(SharedString::new, Into::into)
    }

    fn query_pane_drop_target_slice_index(&self, slot: i32, x: f32, y: f32, source: &str) -> i32 {
        let Some(ui) = self.ui.upgrade() else {
            return -1;
        };
        let state = self.state.borrow();
        pane_drop_target_slice_index_for_slot(&ui, &state, slot, x, y, Path::new(source))
    }

    fn query_pane_drop_allowed(&self, slot: i32, x: f32, y: f32, source: &str) -> bool {
        let Some(ui) = self.ui.upgrade() else {
            return false;
        };
        let state = self.state.borrow();
        pane_drop_allowed_for_slot(&ui, &state, slot, x, y, Path::new(source))
    }

    fn query_place_drop_allowed(&self, source: &str, target_index: i32) -> bool {
        let state = self.state.borrow();
        place_drop_allowed(&state, Path::new(source), target_index)
    }

    fn query_place_drop_target(&self, y: f32) -> i32 {
        self.place_drop_geometry_for_y(y)
            .map_or(-1, |geometry| geometry.target_index)
    }

    fn query_place_drop_slot(&self, y: f32) -> i32 {
        self.place_drop_geometry_for_y(y)
            .map_or(0, |geometry| geometry.slot)
    }

    fn query_place_drop_over_gap(&self, y: f32) -> bool {
        self.place_drop_geometry_for_y(y)
            .is_some_and(|geometry| geometry.over_gap)
    }

    fn query_place_drop_over_item(&self, y: f32) -> bool {
        self.place_drop_geometry_for_y(y)
            .is_some_and(|geometry| geometry.over_item)
    }

    fn place_drop_geometry_for_y(&self, y: f32) -> Option<app::geometry::PlaceDropGeometry> {
        let ui = self.ui.upgrade()?;
        let state = self.state.borrow();
        Some(place_drop_geometry(
            y,
            state.places.len(),
            ui.get_places_list_y_px(),
            ui.get_places_row_stride_px(),
        ))
    }

    fn close_requested(&self) -> CloseRequestResponse {
        if let Some(ui) = self.ui.upgrade() {
            self.settings_save
                .save_now(current_settings(&ui, &self.state));
        }
        if self.chooser_mode {
            std::process::exit(support::chooser::CHOOSER_CANCEL_EXIT_CODE);
        }
        CloseRequestResponse::HideWindow
    }

    fn route_focus(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_focus(slot);
        }
    }

    fn route_path_submitted(&self, slot: i32, path: SharedString) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_path_submitted(slot, path);
        }
    }

    fn route_go_back(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_go_back(slot);
        }
    }

    fn route_go_forward(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_go_forward(slot);
        }
    }

    fn route_search_open(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_open(slot);
        }
    }

    fn route_search_submitted(&self, slot: i32, query: SharedString, recursive: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_submitted(slot, query, recursive);
        }
    }

    fn route_cancel_search(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_cancel_search(slot);
        }
    }

    fn route_search_filters_changed(&self, slot: i32, kind: i32, modified: i32, size: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_filters_changed(slot, kind, modified, size);
        }
    }

    fn route_search_filter_menu_requested(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        kind: i32,
        modified: i32,
        size: i32,
        selector: i32,
    ) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_filter_menu_requested(
                slot, x, y, kind, modified, size, selector,
            );
        }
    }

    fn route_search_close_requested(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_close_requested(slot);
        }
    }

    fn route_search_focus_changed(&self, slot: i32, focused: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_search_focus_changed(slot, focused);
        }
    }

    fn route_view_changed(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_view_changed(slot);
        }
    }

    fn route_item_view_item_pressed(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        toggle: bool,
        range: bool,
    ) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            ui.invoke_route_pane_item_view_item_pressed(slot, x, y, toggle, range)
        })
    }

    fn route_item_view_item_activated(&self, slot: i32, x: f32, y: f32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_item_view_item_activated(slot, x, y);
        }
    }

    fn route_item_view_item_context_menu(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        abs_x: f32,
        abs_y: f32,
    ) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            ui.invoke_route_pane_item_view_item_context_menu(slot, x, y, abs_x, abs_y)
        })
    }

    fn route_item_view_blank_pressed(&self, slot: i32, x: f32, y: f32, toggle: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_item_view_blank_pressed(slot, x, y, toggle);
        }
    }

    fn route_item_view_blank_moved(&self, slot: i32, x: f32, y: f32) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.invoke_route_pane_item_view_blank_moved(slot, x, y))
    }

    fn route_item_view_blank_released(&self, slot: i32, x: f32, y: f32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_item_view_blank_released(slot, x, y);
        }
    }

    fn route_item_view_blank_cancelled(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_item_view_blank_cancelled(slot);
        }
    }

    fn route_request_blank_context_menu(&self, slot: i32, x: f32, y: f32) {
        if let Some(ui) = self.ui.upgrade() {
            let service_menu_paths = context_service_menu::blank_paths(&self.state, slot);
            context_service_menu::refresh_actions_async(
                &ui,
                &self.state,
                &self.bridge,
                slot,
                service_menu_paths,
            );
            ui.invoke_route_pane_request_blank_context_menu(slot, x, y);
        }
    }

    fn route_zoom_in(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_zoom_in(slot);
        }
    }

    fn route_zoom_out(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_zoom_out(slot);
        }
    }

    fn route_drop_target_path(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        source: SharedString,
    ) -> SharedString {
        self.ui.upgrade().map_or_else(SharedString::new, |ui| {
            ui.invoke_route_pane_drop_target_path(slot, x, y, source)
        })
    }

    fn route_drop_target_slice_index(
        &self,
        slot: i32,
        x: f32,
        y: f32,
        source: SharedString,
    ) -> i32 {
        self.ui.upgrade().map_or(-1, |ui| {
            ui.invoke_route_pane_drop_target_slice_index(slot, x, y, source)
        })
    }

    fn route_drop_target_changed(&self, slot: i32, slice_index: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_drop_target_changed(slot, slice_index);
        }
    }

    fn route_drop_allowed(&self, slot: i32, x: f32, y: f32, source: SharedString) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.invoke_route_pane_drop_allowed(slot, x, y, source))
    }

    fn route_prepare_transfer(&self, slot: i32, source: SharedString, x: f32, y: f32) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.invoke_route_pane_prepare_transfer(slot, source, x, y))
    }

    fn route_transfer_menu_requested(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_transfer_menu_requested(slot);
        }
    }

    fn route_trace_drop(
        &self,
        action: SharedString,
        kind: SharedString,
        path: SharedString,
        x: f32,
        y: f32,
        rejected: bool,
        target: SharedString,
    ) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_trace_main_drop(action, kind, path, x, y, rejected, target);
        }
    }

    fn route_save_focus_changed(&self, slot: i32, focused: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_save_focus_changed(slot, focused);
        }
    }

    fn route_commit_external_edit(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_commit_external_edit(slot);
        }
    }

    fn route_discard_external_edit(&self, slot: i32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_discard_external_edit(slot);
        }
    }

    fn route_undo_last_operation(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_undo_last_operation();
        }
    }

    fn route_chooser_accept(&self, value: SharedString) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_chooser_accept(value);
        }
    }

    fn route_chooser_filter_requested(&self, slot: i32, x: f32, y: f32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_chooser_filter_requested(slot, x, y);
        }
    }

    fn route_chooser_choice_requested(&self, slot: i32, index: i32, x: f32, y: f32) {
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_route_pane_chooser_choice_requested(slot, index, x, y);
        }
    }
}

fn device_action_label(action: &str) -> &'static str {
    match action {
        "unmount" => "Unmounting",
        "eject" => "Ejecting",
        _ => "Updating",
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let raw_args = env::args().skip(1).collect::<Vec<_>>();
    let args = Args::parse(raw_args.into_iter());

    if matches!(args.mode, Mode::DeviceDiagnostics) {
        print!("{}", device_diagnostics_report());
        return Ok(());
    }

    let async_runtime = build_async_runtime();
    let async_handle = async_runtime.handle().clone();
    let settings = load_settings();
    let start_dir = args.start_dir.clone().unwrap_or_else(|| {
        settings
            .last_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| home_dir()))
    });
    let (async_tx, async_rx) = mpsc::channel();

    let state = Rc::new(RefCell::new(AppState::new(
        normalize_start_dir(start_dir),
        default_places(),
    )));
    state.borrow_mut().service_menu_policy = load_service_menu_policy();

    let ui = AppWindow::new()?;

    // ── DndApi bridge ──────────────────────────────────────────────
    // Maps Slint's opaque `data-transfer` ↔ `DropEvent` ↔ our internal drag info.
    {
        use slint::DataTransfer;
        use slint::language::DropEvent;
        use std::rc::Rc;

        #[derive(Clone, Debug)]
        enum FikaDragInfo {
            Pending(i32),
            Place(String),
            Folder(String),
            File(String),
        }

        fn drag_transfer(info: FikaDragInfo) -> DataTransfer {
            let mut dt = DataTransfer::default();
            dt.set_user_data(Rc::new(info));
            dt
        }

        fn pending_drag_info(state: &AppState, slot: i32) -> Option<FikaDragInfo> {
            let source = state.panes.pane_for_slot(slot)?.view.input.drag_source()?;
            Some(if source.is_dir() {
                FikaDragInfo::Folder(source.path().to_string())
            } else {
                FikaDragInfo::File(source.path().to_string())
            })
        }

        fn drag_label(path: &str) -> SharedString {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(path)
                .into()
        }

        let dnd_api = ui.global::<DndApi>();

        // ── DragArea.data constructors ──────────────────────────
        dnd_api.on_make_drag_place(|path: SharedString| -> DataTransfer {
            drag_transfer(FikaDragInfo::Place(path.to_string()))
        });
        dnd_api.on_make_drag_folder(|path: SharedString| -> DataTransfer {
            drag_transfer(FikaDragInfo::Folder(path.to_string()))
        });
        dnd_api.on_make_drag_file(|path: SharedString| -> DataTransfer {
            drag_transfer(FikaDragInfo::File(path.to_string()))
        });
        {
            let ui_weak = ui.as_weak();
            let state = Rc::clone(&state);
            dnd_api.on_make_drag_at(move |slot, x, y| -> DataTransfer {
                if x <= -2.0 || y <= -2.0 {
                    return DataTransfer::default();
                }
                if x < 0.0 || y < 0.0 {
                    return drag_transfer(FikaDragInfo::Pending(slot));
                }
                let Some(ui) = ui_weak.upgrade() else {
                    return DataTransfer::default();
                };
                let state_ref = state.borrow();
                let Some(entry) = entry_at_pane_point(&ui, &state_ref, slot, x, y) else {
                    return DataTransfer::default();
                };
                let path = entry.path.to_string();
                if entry.is_dir {
                    drag_transfer(FikaDragInfo::Folder(path))
                } else {
                    drag_transfer(FikaDragInfo::File(path))
                }
            });
        }

        // ── DropEvent inspectors ────────────────────────────────
        {
            let state = Rc::clone(&state);
            dnd_api.on_event_kind(move |event: DropEvent| -> DragKind {
                if let Some(rc) = event.data.user_data() {
                    match rc.downcast_ref::<FikaDragInfo>() {
                        Some(FikaDragInfo::Pending(slot)) => {
                            let state_ref = state.borrow();
                            return match pending_drag_info(&state_ref, *slot) {
                                Some(FikaDragInfo::Folder(_)) => DragKind::Folder,
                                Some(FikaDragInfo::File(_)) => DragKind::File,
                                _ => DragKind::Unsupported,
                            };
                        }
                        Some(FikaDragInfo::Place(_)) => return DragKind::Place,
                        Some(FikaDragInfo::Folder(_)) => return DragKind::Folder,
                        Some(FikaDragInfo::File(_)) => return DragKind::File,
                        None => {}
                    }
                }
                DragKind::Unsupported
            });
        }

        {
            let state = Rc::clone(&state);
            dnd_api.on_event_path(move |event: DropEvent| -> SharedString {
                if let Some(rc) = event.data.user_data()
                    && let Some(info) = rc.downcast_ref::<FikaDragInfo>()
                {
                    return match info {
                        FikaDragInfo::Pending(slot) => {
                            let state_ref = state.borrow();
                            match pending_drag_info(&state_ref, *slot) {
                                Some(
                                    FikaDragInfo::Place(p)
                                    | FikaDragInfo::Folder(p)
                                    | FikaDragInfo::File(p),
                                ) => SharedString::from(p.as_str()),
                                _ => SharedString::new(),
                            }
                        }
                        FikaDragInfo::Place(p)
                        | FikaDragInfo::Folder(p)
                        | FikaDragInfo::File(p) => SharedString::from(p.as_str()),
                    };
                }
                SharedString::new()
            });
        }

        {
            let state = Rc::clone(&state);
            dnd_api.on_event_label(move |event: DropEvent| -> SharedString {
                if let Some(rc) = event.data.user_data()
                    && let Some(info) = rc.downcast_ref::<FikaDragInfo>()
                {
                    return match info {
                        FikaDragInfo::Pending(slot) => {
                            let state_ref = state.borrow();
                            match pending_drag_info(&state_ref, *slot) {
                                Some(
                                    FikaDragInfo::Place(p)
                                    | FikaDragInfo::Folder(p)
                                    | FikaDragInfo::File(p),
                                ) => drag_label(p.as_str()),
                                _ => SharedString::new(),
                            }
                        }
                        FikaDragInfo::Place(p)
                        | FikaDragInfo::Folder(p)
                        | FikaDragInfo::File(p) => drag_label(p.as_str()),
                    };
                }
                SharedString::new()
            });
        }
    }
    ui.set_chooser_mode(matches!(args.mode, Mode::Chooser));
    ui.set_chooser_select_directories(args.chooser_select_directories);
    ui.set_chooser_multiple(args.chooser_multiple);
    if let Some(title) = &args.chooser_title {
        ui.set_chooser_title(title.as_str().into());
    }
    if let Some(label) = &args.chooser_accept_label {
        ui.set_chooser_accept_label(label.as_str().into());
    }
    ui.set_chooser_save_mode(
        args.chooser_save_name.is_some() || !args.chooser_save_files.is_empty(),
    );
    if let Some(name) = &args.chooser_save_name {
        ui.set_chooser_save_name(name.as_str().into());
    }
    {
        let mut state_ref = state.borrow_mut();
        state_ref.chooser_filters = args
            .chooser_filters
            .iter()
            .filter_map(|spec| parse_chooser_filter_spec(spec))
            .collect();
        state_ref.chooser_filter_index = args
            .chooser_filter_index
            .min(state_ref.chooser_filters.len().saturating_sub(1));
        state_ref.chooser_return_filter = args.chooser_return_filter;
        state_ref.chooser_choices = args
            .chooser_choices
            .iter()
            .filter_map(|spec| parse_chooser_choice_spec(spec))
            .collect();
        state_ref.chooser_return_choices = args.chooser_return_choices;
        state_ref.chooser_parent_window = args.chooser_parent_window.clone();
    }
    if matches!(args.mode, Mode::Chooser) {
        log_chooser_parent_window(args.chooser_parent_window.as_deref());
    }
    sync_chooser_filter_ui(&ui, &state);
    sync_chooser_choices_ui(&ui, &state);
    ui.set_dark_mode(settings.dark_mode.unwrap_or(true));
    if let Some(sidebar_width_px) = settings.sidebar_width_px {
        ui.set_sidebar_width_px(sidebar_width_px.clamp(220.0, 1200.0));
    }
    if let Some(split_pane_ratio) = settings.split_pane_ratio {
        ui.set_split_pane_ratio(clamped_split_pane_ratio(split_pane_ratio));
    }
    if let Some(icon_zoom_level) = settings.icon_zoom_level {
        ui.set_icon_zoom_level(clamp_zoom_level(icon_zoom_level));
    }
    if let (Some(width), Some(height)) = (settings.window_width_px, settings.window_height_px) {
        ui.window().set_size(LogicalSize::new(
            width.clamp(780.0, 3200.0),
            height.clamp(460.0, 2200.0),
        ));
    }
    sync_places(&ui, &state.borrow().places);
    sync_clipboard_ui(&ui, &state);
    let bridge = AsyncBridge {
        handle: async_handle.clone(),
        tx: async_tx,
        ui_weak: ui.as_weak(),
        directory_watchers: Rc::new(RefCell::new(HashMap::new())),
        directory_read_trackers: Rc::new(RefCell::new(HashMap::new())),
        device_watch_debounce: Arc::new(AtomicU64::new(0)),
    };
    sync_devices(&ui, &state);
    refresh_devices_async(&state, &bridge);
    refresh_clipboard_availability_async(&state, &bridge);
    start_device_monitor(&bridge);
    let icon_size_update_pending = Rc::new(Cell::new(false));
    let pane_view_sync = Rc::new(PaneViewSyncScheduler::new(
        ui.as_weak(),
        Rc::clone(&state),
        bridge.clone(),
        Rc::clone(&icon_size_update_pending),
    ));
    let pane_layout_sync = Rc::new(PaneLayoutSyncScheduler::new(
        ui.as_weak(),
        Rc::clone(&state),
        bridge.clone(),
        Rc::clone(&pane_view_sync),
        Rc::clone(&icon_size_update_pending),
    ));
    let thumbnail_flush = Rc::new(ThumbnailFlushScheduler::new(
        ui.as_weak(),
        Rc::clone(&state),
        bridge.clone(),
        Rc::clone(&icon_size_update_pending),
    ));
    let settings_save = Rc::new(SettingsSaveScheduler::new(async_handle.clone()));

    let async_rx = Rc::new(RefCell::new(async_rx));
    let ui_signals = Rc::new(UiSignalBus::new(UiSignalBusInput {
        ui: ui.as_weak(),
        state: Rc::clone(&state),
        bridge: bridge.clone(),
        pane_view_sync: Rc::clone(&pane_view_sync),
        pane_layout_sync: Rc::clone(&pane_layout_sync),
        thumbnail_flush: Rc::clone(&thumbnail_flush),
        settings_save: Rc::clone(&settings_save),
        async_rx: Rc::clone(&async_rx),
        chooser_save_files: args.chooser_save_files.clone(),
        chooser_mode: matches!(args.mode, Mode::Chooser),
    }));

    register_ui_signal_callbacks(&ui, Rc::clone(&ui_signals));
    register_pane_routing_callbacks(&ui, Rc::clone(&ui_signals));
    register_menu_geometry_callbacks(&ui);
    open_with::register_callbacks(&ui, &state, &bridge);
    app::file_clipboard::register_callbacks(&ui, &state, &bridge);
    file_actions::register_callbacks(&ui, &state, &bridge);

    clear_pane_models_ui(&ui);
    load_directory(&ui, &state, &bridge);
    prefetch_sidebar_locations_async(&state, &bridge);

    ui.run()
}

fn register_ui_signal_callbacks(ui: &AppWindow, bus: Rc<UiSignalBus>) {
    {
        let bus = Rc::clone(&bus);
        ui.on_async_results_ready(move || bus.emit(UiSignal::AsyncResultsReady));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_view_changed(move |slot| bus.emit(UiSignal::PaneViewChanged(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_layout_changed(move || bus.emit(UiSignal::PaneLayoutChanged));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_icon_zoom_layout_changed(move || bus.emit(UiSignal::IconZoomLayoutChanged));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_slots_refresh_requested(move || {
            bus.emit(UiSignal::PaneSlotsRefreshRequested);
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_path_text_changed(move |slot, text| {
            bus.emit(UiSignal::PanePathTextChanged(slot, text.to_string()));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_path_focus_changed(move |slot, focused| {
            bus.emit(UiSignal::PanePathFocusChanged(slot, focused));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_slot_refresh_requested(move |slot| {
            bus.emit(UiSignal::PaneSlotRefreshRequested(slot));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_viewport_changed(move |slot, viewport_x| {
            bus.emit(UiSignal::PaneViewportChanged(slot, viewport_x));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_refresh(move || bus.emit(UiSignal::Refresh));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_go_home(move || bus.emit(UiSignal::GoHome));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_go_parent(move || bus.emit(UiSignal::GoParent));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_go_root(move || bus.emit(UiSignal::GoRoot));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_path_submitted(move |slot, path| {
            bus.emit(UiSignal::PanePathSubmitted(slot, path.to_string()));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_place(move |path| bus.emit(UiSignal::OpenPlace(path.to_string())));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_device(move |path, mounted| {
            bus.emit(UiSignal::OpenDevice(path.to_string(), mounted));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_unmount_device(move |device_path, mount_path| {
            bus.emit(UiSignal::UnmountDevice(
                device_path.to_string(),
                mount_path.to_string(),
            ));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_eject_device(move |device_path, mount_path| {
            bus.emit(UiSignal::EjectDevice(
                device_path.to_string(),
                mount_path.to_string(),
            ));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_search(move || bus.emit(UiSignal::OpenSearch));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_search_for_slot(move |slot| bus.emit(UiSignal::OpenSearchForSlot(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_clear_focused_search(move || bus.query_clear_focused_search());
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_search_submitted(move |slot, query, recursive| {
            bus.emit(UiSignal::SearchSubmitted(
                slot,
                query.to_string(),
                recursive,
            ));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_cancel_search(move |slot| bus.emit(UiSignal::CancelSearch(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_search_filters_changed(move |slot, kind, modified, size| {
            bus.emit(UiSignal::SearchFiltersChanged(slot, kind, modified, size));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_search_close_requested(move |slot| {
            bus.emit(UiSignal::SearchCloseRequested(slot));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_path(move |path| bus.emit(UiSignal::OpenPath(path.to_string())));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_context_service_action(move |index| {
            bus.emit(UiSignal::ContextServiceAction(index));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_prepare_context_service_submenu(move |group| {
            bus.emit(UiSignal::PrepareContextServiceSubmenu(group.to_string()));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_context_service_action_enabled(move |id, enabled| {
            bus.emit(UiSignal::ContextServiceActionEnabled(
                id.to_string(),
                enabled,
            ));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_context_service_policy_mode_changed(move |mode| {
            bus.emit(UiSignal::ContextServicePolicyModeChanged(mode));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_chooser_accept(move |name| bus.emit(UiSignal::ChooserAccept(name.to_string())));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_chooser_select_filter(move |filter_index| {
            bus.emit(UiSignal::ChooserSelectFilter(filter_index));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_chooser_select_choice(move |choice_index, option_index| {
            bus.emit(UiSignal::ChooserSelectChoice(choice_index, option_index));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_go_back(move || bus.emit(UiSignal::GoBack));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_go_forward(move || bus.emit(UiSignal::GoForward));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_go_back(move |slot| bus.emit(UiSignal::PaneGoBack(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_go_forward(move |slot| bus.emit(UiSignal::PaneGoForward(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_focus(move |slot| bus.emit(UiSignal::PaneFocus(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_toggle_split_view(move || bus.emit(UiSignal::ToggleSplitView));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_item_pressed(move |slot, x, y, toggle, range| {
            bus.query_item_view_item_pressed(slot, x, y, toggle, range)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_item_activated(move |slot, x, y| {
            bus.emit(UiSignal::PaneItemViewItemActivated(slot, x, y));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_item_context_menu(move |slot, x, y, abs_x, abs_y| {
            bus.query_item_view_item_context_menu(slot, x, y, abs_x, abs_y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_blank_pressed(move |slot, x, y, toggle| {
            bus.emit(UiSignal::PaneItemViewBlankPressed(slot, x, y, toggle));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_blank_moved(move |slot, x, y| {
            bus.query_item_view_blank_moved(slot, x, y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_blank_released(move |slot, x, y| {
            bus.emit(UiSignal::PaneItemViewBlankReleased(slot, x, y));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_item_view_blank_cancelled(move |slot| {
            bus.emit(UiSignal::PaneItemViewBlankCancelled(slot));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_clear_selection(move |slot| bus.emit(UiSignal::PaneClearSelection(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_clear_selection(move || bus.emit(UiSignal::ClearSelection));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_is_place(move |path| bus.query_is_place(path.as_str()));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_select_all_visible(move || bus.emit(UiSignal::SelectAllVisible));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_add_place(move |path| bus.emit(UiSignal::AddPlace(path.to_string())));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_add_place_at_slot(move |path, slot| {
            bus.emit(UiSignal::AddPlaceAtSlot(path.to_string(), slot));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_rename_place(move |index, label| {
            bus.emit(UiSignal::RenamePlace(index, label.to_string()));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_remove_place(move |index| bus.emit(UiSignal::RemovePlace(index)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_restore_default_places(move || bus.emit(UiSignal::RestoreDefaultPlaces));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_open_place_new_window(move |index| {
            bus.emit(UiSignal::OpenPlaceNewWindow(index));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_prepare_place_transfer(move |source, target_index, x, y| {
            bus.query_prepare_place_transfer(source.as_str(), target_index, x, y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_prepare_entry_transfer(move |source, target_index, x, y| {
            bus.query_prepare_entry_transfer(source.as_str(), target_index, x, y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_prepare_current_dir_transfer(move |source, label, x, y| {
            bus.query_prepare_current_dir_transfer(source.as_str(), label.as_str(), x, y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_prepare_transfer(move |slot, source, x, y| {
            bus.query_pane_prepare_transfer(slot, source.as_str(), x, y)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_drop_target_path(move |slot, x, y, source| {
            bus.query_pane_drop_target_path(slot, x, y, source.as_str())
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_drop_target_slice_index(move |slot, x, y, source| {
            bus.query_pane_drop_target_slice_index(slot, x, y, source.as_str())
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_drop_target_changed(move |slot, slice_index| {
            bus.emit(UiSignal::PaneDropTargetChanged(slot, slice_index));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_pane_drop_allowed(move |slot, x, y, source| {
            bus.query_pane_drop_allowed(slot, x, y, source.as_str())
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_place_drop_allowed(move |source, target_index| {
            bus.query_place_drop_allowed(source.as_str(), target_index)
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_place_drop_target(move |y| bus.query_place_drop_target(y));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_place_drop_slot(move |y| bus.query_place_drop_slot(y));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_place_drop_over_gap(move |y| bus.query_place_drop_over_gap(y));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_place_drop_over_item(move |y| bus.query_place_drop_over_item(y));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_trace_places_drop(
            move |phase, mime_type, payload, x, y, slot, target, over_gap, over_item| {
                bus.emit(UiSignal::TracePlacesDrop {
                    phase: phase.to_string(),
                    mime_type: mime_type.to_string(),
                    payload: payload.to_string(),
                    x,
                    y,
                    slot,
                    target,
                    over_gap,
                    over_item,
                });
            },
        );
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_trace_main_drop(
            move |phase, mime_type, payload, x, y, rejected, target_path| {
                bus.emit(UiSignal::TraceMainDrop {
                    phase: phase.to_string(),
                    mime_type: mime_type.to_string(),
                    payload: payload.to_string(),
                    x,
                    y,
                    rejected,
                    target_path: target_path.to_string(),
                });
            },
        );
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_transfer_operation(move |operation, source, target| {
            bus.emit(UiSignal::TransferOperation(
                operation.to_string(),
                source.to_string(),
                target.to_string(),
            ));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_transfer_conflict_choice(move |decision| {
            bus.emit(UiSignal::TransferConflictChoice(decision.to_string()));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_privileged_prompt_accept(move || bus.emit(UiSignal::PrivilegedPromptAccept));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_privileged_prompt_dismiss(move || bus.emit(UiSignal::PrivilegedPromptDismiss));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_commit_external_edit(move |slot| bus.emit(UiSignal::CommitExternalEdit(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_discard_external_edit(move |slot| bus.emit(UiSignal::DiscardExternalEdit(slot)));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_undo_last_operation(move || bus.emit(UiSignal::UndoLastOperation));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_cancel_queued_operations(move || bus.emit(UiSignal::CancelQueuedOperations));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_reorder_place_path(move |path, to| {
            bus.emit(UiSignal::ReorderPlacePath(path.to_string(), to));
        });
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_persist_ui_state(move || bus.emit(UiSignal::PersistUiState));
    }
    {
        let bus = Rc::clone(&bus);
        ui.on_dark_mode_changed(move || bus.emit(UiSignal::DarkModeChanged));
    }
    {
        let bus = Rc::clone(&bus);
        ui.window()
            .on_close_requested(move || bus.close_requested());
    }
}

fn register_pane_routing_callbacks(ui: &AppWindow, bus: Rc<UiSignalBus>) {
    let routing = ui.global::<PaneRouting>();

    {
        let bus = Rc::clone(&bus);
        routing.on_focus(move |slot| {
            bus.route_focus(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_path_submitted(move |slot, path| {
            bus.route_path_submitted(slot, path);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_go_back(move |slot| {
            bus.route_go_back(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_go_forward(move |slot| {
            bus.route_go_forward(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_open(move |slot| {
            bus.route_search_open(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_submitted(move |slot, query, recursive| {
            bus.route_search_submitted(slot, query, recursive);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_cancel_search(move |slot| {
            bus.route_cancel_search(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_filters_changed(move |slot, kind, modified, size| {
            bus.route_search_filters_changed(slot, kind, modified, size);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_filter_menu_requested(
            move |slot, x, y, kind, modified, size, selector| {
                bus.route_search_filter_menu_requested(slot, x, y, kind, modified, size, selector);
            },
        );
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_close_requested(move |slot| {
            bus.route_search_close_requested(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_search_focus_changed(move |slot, focused| {
            bus.route_search_focus_changed(slot, focused);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_view_changed(move |slot| {
            bus.route_view_changed(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_item_pressed(move |slot, x, y, toggle, range| {
            bus.route_item_view_item_pressed(slot, x, y, toggle, range)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_item_activated(move |slot, x, y| {
            bus.route_item_view_item_activated(slot, x, y);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_item_context_menu(move |slot, x, y, abs_x, abs_y| {
            bus.route_item_view_item_context_menu(slot, x, y, abs_x, abs_y)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_blank_pressed(move |slot, x, y, toggle| {
            bus.route_item_view_blank_pressed(slot, x, y, toggle);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_blank_moved(move |slot, x, y| {
            bus.route_item_view_blank_moved(slot, x, y)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_blank_released(move |slot, x, y| {
            bus.route_item_view_blank_released(slot, x, y);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_item_view_blank_cancelled(move |slot| {
            bus.route_item_view_blank_cancelled(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_request_blank_context_menu(move |slot, x, y| {
            bus.route_request_blank_context_menu(slot, x, y);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_zoom_in(move |slot| {
            bus.route_zoom_in(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_zoom_out(move |slot| {
            bus.route_zoom_out(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_drop_target_path(move |slot, x, y, source| {
            bus.route_drop_target_path(slot, x, y, source)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_drop_target_slice_index(move |slot, x, y, source| {
            bus.route_drop_target_slice_index(slot, x, y, source)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_drop_target_changed(move |slot, slice_index| {
            bus.route_drop_target_changed(slot, slice_index);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing
            .on_drop_allowed(move |slot, x, y, source| bus.route_drop_allowed(slot, x, y, source));
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_prepare_transfer(move |slot, source, x, y| {
            bus.route_prepare_transfer(slot, source, x, y)
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_transfer_menu_requested(move |slot| {
            bus.route_transfer_menu_requested(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_trace_drop(move |action, kind, path, x, y, rejected, target| {
            bus.route_trace_drop(action, kind, path, x, y, rejected, target);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_save_focus_changed(move |slot, focused| {
            bus.route_save_focus_changed(slot, focused);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_commit_external_edit(move |slot| {
            bus.route_commit_external_edit(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_discard_external_edit(move |slot| {
            bus.route_discard_external_edit(slot);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_undo_last_operation(move || {
            bus.route_undo_last_operation();
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_chooser_accept(move |value| {
            bus.route_chooser_accept(value);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_chooser_filter_requested(move |slot, x, y| {
            bus.route_chooser_filter_requested(slot, x, y);
        });
    }

    {
        let bus = Rc::clone(&bus);
        routing.on_chooser_choice_requested(move |slot, index, x, y| {
            bus.route_chooser_choice_requested(slot, index, x, y);
        });
    }
}

fn log_chooser_parent_window(parent_window: Option<&str>) {
    static DEBUG_PORTAL: OnceLock<bool> = OnceLock::new();
    if !*DEBUG_PORTAL
        .get_or_init(|| env::var("FIKA_DEBUG_PORTAL").is_ok_and(|value| env_flag_is_truthy(&value)))
    {
        return;
    }
    eprintln!("{}", chooser_parent_window_log_message(parent_window));
}

fn chooser_parent_window_log_message(parent_window: Option<&str>) -> String {
    let (parent_binding, parent_binding_reason) = chooser_parent_window_binding(parent_window);
    format!(
        "[fika chooser] parent_window received={} handle={} parent_binding={} parent_binding_reason={} native_transient=false",
        parent_window.is_some(),
        parent_window.unwrap_or(""),
        parent_binding,
        parent_binding_reason,
    )
}

fn chooser_parent_window_binding(parent_window: Option<&str>) -> (&'static str, &'static str) {
    if parent_window.is_some() {
        ("metadata-only", "slint-parent-token-binding-unavailable")
    } else {
        ("none", "no-parent-window")
    }
}

fn start_privileged_operation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    command: privilege::PrivilegedCommand,
) {
    set_status(
        ui,
        state,
        &format!(
            "Requesting administrator privileges for {}...",
            command.label()
        ),
    );
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = privilege::run_via_dbus(command).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::PrivilegedOperationFinished(result),
        );
    });
}

fn save_current_settings(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    save_settings_latest(current_settings(ui, state));
}

fn current_settings(ui: &AppWindow, state: &Rc<RefCell<AppState>>) -> AppSettings {
    let current_dir = state.borrow().panes.focused().current_dir.clone();
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    AppSettings {
        dark_mode: Some(ui.get_dark_mode()),
        sidebar_width_px: Some(ui.get_sidebar_width_px()),
        split_pane_ratio: Some(clamped_split_pane_ratio(ui.get_split_pane_ratio())),
        icon_zoom_level: Some(ui.get_icon_zoom_level()),
        window_width_px: Some(window_size.width),
        window_height_px: Some(window_size.height),
        last_dir: Some(current_dir),
    }
}

pub(crate) fn remember_pane_view_state(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    let viewport_x = pane_viewport_x_from_ui(ui, slot, state);
    let mut state = state.borrow_mut();
    let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
        return;
    };
    let current_dir = pane.current_dir.clone();
    pane.view.viewport_x = viewport_x;
    pane.view
        .insert_state_cache(current_dir, DirectoryViewState { viewport_x });
}

fn restore_pane_view_state(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32, path: &Path) {
    let view_state = {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        let view_state = pane.view.cached_state(path).unwrap_or_default();
        pane.view.viewport_x = view_state.viewport_x;
        view_state
    };
    set_pane_viewport_ui(ui, slot, view_state.viewport_x, state);
}

fn set_current_location_ui(ui: &AppWindow, path: &Path) {
    let current_path = path.display().to_string();
    let in_trash = fs::file_ops::is_in_trash_files_dir(path);
    ui.set_current_path(current_path.as_str().into());
    ui.set_current_name(display_location_name(path).into());
    ui.set_current_in_trash(in_trash);
}

fn clear_pane_models_ui(ui: &AppWindow) {
    ui.set_pane_slots(ModelRc::new(Rc::new(VecModel::from(
        Vec::<PaneSlotData>::new(),
    ))));
    ui.set_pane_surfaces(ModelRc::new(Rc::new(VecModel::from(
        Vec::<PaneSurfaceData>::new(),
    ))));
}

fn load_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    load_directory_with_preservation(ui, state, bridge, false);
}

fn refresh_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    load_directory_with_preservation(ui, state, bridge, true);
}

fn refresh_panes(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_ids: &[u64],
) {
    for pane_id in pane_ids {
        refresh_pane_by_id(ui, state, bridge, *pane_id);
    }
}

fn refresh_affected_directories(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    affected_dirs: &[PathBuf],
) -> Vec<u64> {
    let pane_ids = {
        let mut state = state.borrow_mut();
        for dir in affected_dirs {
            state.remove_directory_cache(dir);
        }
        affected_directory_pane_ids(&state, affected_dirs.iter().map(|dir| dir.as_path()))
    };
    if !pane_ids.is_empty() {
        refresh_panes(ui, state, bridge, &pane_ids);
    }
    pane_ids
}

fn refresh_pane_by_id(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_id: u64,
) {
    let Some(preparation) = ({
        let mut state = state.borrow_mut();
        prepare_directory_load_for_target(&mut state, PaneTarget::Id(pane_id), true)
    }) else {
        return;
    };
    load_prepared_pane_directory(ui, state, bridge, preparation, true);
}

fn load_prepared_pane_directory(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    preparation: DirectoryLoadPreparation,
    preserve_view: bool,
) {
    let DirectoryLoadPreparation {
        pane_id,
        current_dir,
        generation,
        cached_entries,
        defer_view_restore,
    } = preparation;
    let Some(slot) = state.borrow().panes.slot_for_id(pane_id) else {
        return;
    };
    let read_tracker = directory_read_tracker_for_pane(pane_id, bridge);
    let Some(request) = begin_directory_read_request(&read_tracker, generation) else {
        debug_log(&format!(
            "load_directory skipped stale request pane_id={pane_id} generation={generation} path={}",
            current_dir.display()
        ));
        return;
    };
    debug_log(&format!(
        "load_directory slot={slot} pane_id={pane_id} generation={generation} request={request} preserve_view={preserve_view} defer_view_restore={defer_view_restore} path={} cache_hit={}",
        current_dir.display(),
        cached_entries.is_some()
    ));
    let target_is_focused = ui.get_focused_pane() == slot;
    if target_is_focused {
        set_current_location_ui(ui, &current_dir);
    }
    if !preserve_view && !defer_view_restore {
        restore_pane_view_state(ui, state, slot, &current_dir);
    }
    let sync_virtual_view = if let Some(cached_entries) = cached_entries {
        {
            let mut state = state.borrow_mut();
            if let Some(pane) = state.panes.pane_mut_for_target(PaneTarget::Id(pane_id)) {
                pane.set_entries_with_summary(
                    cached_entries.entries.clone(),
                    cached_entries.summary.clone(),
                );
            }
        }
        if target_is_focused {
            ui.set_items_path(current_dir.display().to_string().into());
            ui.set_directory_loading(false);
        }
        sync_pane_slot_ui(ui, state, slot);
        set_pane_status(ui, state, slot, "Refreshing cached folder...");
        true
    } else if !preserve_view {
        {
            let mut state = state.borrow_mut();
            if let Some(pane) = state.panes.pane_mut_for_target(PaneTarget::Id(pane_id)) {
                pane.clear_entries();
                pane.search.visible_entries_have_locations = false;
            }
        }
        if target_is_focused {
            ui.set_directory_loading(true);
            update_selection_ui_for_slot(ui, state, slot, &[]);
        }
        sync_pane_slot_ui(ui, state, slot);
        set_pane_status(ui, state, slot, "Loading folder...");
        if pane_view_row_exists(ui, slot) {
            sync_pane_view_ui(ui, state, slot);
        }
        false
    } else {
        if target_is_focused {
            ui.set_directory_loading(false);
        }
        set_pane_status(ui, state, slot, "Refreshing folder...");
        true
    };
    if sync_virtual_view {
        sync_pane_view_for_slot(ui, state, bridge, slot);
    }
    watch_current_directory(&current_dir, pane_id, generation, bridge);

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = read_entries_async(&current_dir)
            .await
            .map(PreparedDirectoryEntries::new);
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                pane_id,
                generation,
                request,
                path: current_dir,
                preserve_view,
                defer_view_restore,
                result,
            }),
        );
    });
}

fn directory_read_tracker_for_pane(
    pane_id: u64,
    bridge: &AsyncBridge,
) -> Arc<Mutex<DirectoryReadTracker>> {
    bridge
        .directory_read_trackers
        .borrow_mut()
        .entry(pane_id)
        .or_insert_with(|| Arc::new(Mutex::new(DirectoryReadTracker::default())))
        .clone()
}

fn begin_directory_read_request(
    tracker: &Arc<Mutex<DirectoryReadTracker>>,
    generation: u64,
) -> Option<u64> {
    tracker.lock().ok()?.begin_request(generation)
}

fn directory_read_result_is_current(
    pane_id: u64,
    generation: u64,
    request: u64,
    bridge: &AsyncBridge,
) -> bool {
    let tracker = bridge
        .directory_read_trackers
        .borrow()
        .get(&pane_id)
        .cloned();
    tracker
        .and_then(|tracker| {
            tracker
                .lock()
                .ok()
                .map(|tracker| tracker.is_current(generation, request))
        })
        .unwrap_or(false)
}

fn prefetch_sidebar_locations_async(state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let paths = {
        let mut state = state.borrow_mut();
        sidebar_prefetch_paths(&mut state)
    };
    for path in paths {
        let async_tx = bridge.tx.clone();
        let notify_ui = bridge.ui_weak.clone();
        bridge.handle.spawn(async move {
            if fs::file_ops::is_trash_files_dir(&path) {
                let _ = fs::file_ops::ensure_trash_dirs();
            }
            let result = read_entries_async(&path)
                .await
                .map(PreparedDirectoryEntries::new);
            send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::DirectoryPrefetched { path, result },
            );
        });
    }
}

fn sidebar_prefetch_paths(state: &mut AppState) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let candidates = state
        .places
        .iter()
        .filter_map(|place| {
            let text = place.path.as_str();
            (!text.is_empty()).then(|| expand_user_path(text))
        })
        .chain(state.devices.iter().filter_map(|device| {
            let text = device.path.as_str();
            (device.mounted && !text.is_empty()).then(|| PathBuf::from(text))
        }))
        .collect::<Vec<_>>();

    for path in candidates {
        push_sidebar_prefetch_path(state, &mut paths, path);
    }
    paths
}

fn push_sidebar_prefetch_path(state: &mut AppState, paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path == state.panes.focused().current_dir
        || state.directory_cache.contains_key(&path)
        || state.directory_prefetch_pending.contains(&path)
        || paths.iter().any(|existing| existing == &path)
    {
        return;
    }
    state.directory_prefetch_pending.insert(path.clone());
    paths.push(path);
}

fn load_directory_with_preservation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    preserve_view: bool,
) {
    let preparation = {
        let mut state = state.borrow_mut();
        prepare_directory_load(&mut state, preserve_view)
    };
    save_current_settings(ui, state);
    load_prepared_pane_directory(ui, state, bridge, preparation, preserve_view);
}

fn sync_devices(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    ui.set_devices(ModelRc::new(Rc::new(VecModel::from(devices_with_status(
        state.devices.clone(),
        &state.pending_device_actions,
        &state.device_errors,
    )))));
}

fn refresh_devices_async(state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let generation = state.borrow_mut().device_generation.next();
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let devices = tokio::task::spawn_blocking(mounted_devices)
            .await
            .unwrap_or_default();
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DevicesLoaded(DevicesLoadedResult {
                generation,
                devices,
            }),
        );
    });
}

fn devices_with_status(
    mut devices: Vec<DeviceEntry>,
    pending_actions: &[DeviceAction],
    errors: &std::collections::HashMap<String, String>,
) -> Vec<DeviceEntry> {
    for device in &mut devices {
        if let Some(pending) = pending_actions
            .iter()
            .find(|pending| pending.device_path == device.device_path.as_str())
        {
            device.pending_action = pending.action.as_str().into();
        }
        if let Some(error) = errors.get(device.device_path.as_str()) {
            device.error = error.as_str().into();
        }
    }
    devices
}

fn mount_device_async(bridge: &AsyncBridge, device_path: String) {
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let task_device_path = device_path.clone();
        let result = tokio::task::spawn_blocking(move || mount_device(&task_device_path))
            .await
            .unwrap_or_else(|err| Err(format!("mount task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DeviceMountFinished(DeviceMountResult {
                device_path,
                result,
            }),
        );
    });
}

fn register_pending_device_action(
    state: &Rc<RefCell<AppState>>,
    device_path: &str,
    action: &str,
) -> bool {
    let mut state = state.borrow_mut();
    if state
        .pending_device_actions
        .iter()
        .any(|pending| pending.device_path == device_path)
    {
        return false;
    }

    state.pending_device_actions.push(DeviceAction {
        device_path: device_path.to_string(),
        action: action.to_string(),
    });
    true
}

fn clear_pending_device_action(state: &Rc<RefCell<AppState>>, device_path: &str, action: &str) {
    state
        .borrow_mut()
        .pending_device_actions
        .retain(|pending| pending.device_path != device_path || pending.action.as_str() != action);
}

fn set_device_error(state: &Rc<RefCell<AppState>>, device_path: &str, error: &str) {
    state
        .borrow_mut()
        .device_errors
        .insert(device_path.to_string(), error.to_string());
}

fn clear_device_error(state: &Rc<RefCell<AppState>>, device_path: &str) {
    state.borrow_mut().device_errors.remove(device_path);
}

fn mounted_device_path(path: &str) -> Option<PathBuf> {
    let path = expand_user_path(path);
    (path.is_dir() && path != Path::new("/")).then_some(path)
}

fn device_action_async(
    bridge: &AsyncBridge,
    action: &'static str,
    device_path: String,
    mount_path: Option<PathBuf>,
) {
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let task_device_path = device_path.clone();
        let result = tokio::task::spawn_blocking(move || match action {
            "unmount" => unmount_device(&task_device_path),
            "eject" => eject_device(&task_device_path),
            _ => Err(format!("unknown device action: {action}")),
        })
        .await
        .unwrap_or_else(|err| Err(format!("{action} task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DeviceActionFinished(DeviceActionResult {
                action: action.to_string(),
                device_path,
                mount_path,
                result,
            }),
        );
    });
}

pub(crate) fn watch_current_directory(
    path: &Path,
    pane_id: u64,
    generation: u64,
    bridge: &AsyncBridge,
) {
    use notify::Watcher;

    if fs::file_ops::is_trash_files_dir(path) {
        let _ = fs::file_ops::ensure_trash_dirs();
    }
    let watched_path = path.to_path_buf();
    let watch_paths = directory_watch_paths(path);
    let async_handle = bridge.handle.clone();
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let debounce = Arc::new(AtomicU64::new(0));
    let read_tracker = directory_read_tracker_for_pane(pane_id, bridge);

    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if !directory_watch_event_should_reload(&event) {
            return;
        }

        let removed_paths = directory_watch_removed_paths(&event, &watched_path);
        if !removed_paths.is_empty() {
            debug_log(&format!(
                "directory watcher removed pane_id={pane_id} generation={generation} path={} removed={}",
                watched_path.display(),
                removed_paths.len()
            ));
            send_async_event(
                async_tx.clone(),
                notify_ui.clone(),
                AsyncEvent::DirectoryEntriesRemoved(DirectoryEntriesRemoved {
                    pane_id,
                    generation,
                    path: watched_path.clone(),
                    removed_paths,
                }),
            );
        }

        let serial = debounce.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        let reload_path = watched_path.clone();
        let async_tx = async_tx.clone();
        let notify_ui = notify_ui.clone();
        let debounce = Arc::clone(&debounce);
        let read_tracker = Arc::clone(&read_tracker);

        async_handle.spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if debounce.load(AtomicOrdering::SeqCst) != serial {
                return;
            }
            let Some(request) = begin_directory_read_request(&read_tracker, generation) else {
                return;
            };

            let result = read_entries_async(&reload_path)
                .await
                .map(PreparedDirectoryEntries::new);
            send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                    pane_id,
                    generation,
                    request,
                    path: reload_path,
                    preserve_view: true,
                    defer_view_restore: false,
                    result,
                }),
            );
        });
    });

    let Ok(mut watcher) = watcher else {
        bridge.directory_watchers.borrow_mut().remove(&pane_id);
        return;
    };

    let mut watched_any = false;
    for watch_path in watch_paths {
        match watcher.watch(&watch_path, notify::RecursiveMode::NonRecursive) {
            Ok(()) => {
                watched_any = true;
            }
            Err(err) => {
                debug_log(&format!(
                    "directory watcher skipped path={} error={err}",
                    watch_path.display()
                ));
            }
        }
    }

    if watched_any {
        bridge
            .directory_watchers
            .borrow_mut()
            .insert(pane_id, watcher);
    } else {
        bridge.directory_watchers.borrow_mut().remove(&pane_id);
    }
}

fn directory_watch_event_should_reload(event: &notify::Event) -> bool {
    use notify::event::{AccessKind, AccessMode};

    if event.need_rescan() {
        return true;
    }

    match &event.kind {
        notify::EventKind::Access(AccessKind::Close(
            AccessMode::Write | AccessMode::Any | AccessMode::Other,
        )) => true,
        notify::EventKind::Access(_) => false,
        _ => true,
    }
}

fn directory_watch_removed_paths(event: &notify::Event, watched_path: &Path) -> Vec<PathBuf> {
    use notify::event::{ModifyKind, RenameMode};

    let paths = match &event.kind {
        notify::EventKind::Remove(_) => event.paths.iter().collect::<Vec<_>>(),
        notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            event.paths.iter().collect::<Vec<_>>()
        }
        notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            event.paths.iter().take(1).collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    paths
        .into_iter()
        .filter(|path| {
            path.as_path() != watched_path
                && path.parent().is_some_and(|parent| parent == watched_path)
        })
        .cloned()
        .collect()
}

pub(crate) fn unwatch_directory_for_pane(pane_id: u64, bridge: &AsyncBridge) {
    bridge.directory_watchers.borrow_mut().remove(&pane_id);
    bridge.directory_read_trackers.borrow_mut().remove(&pane_id);
}

fn directory_watch_paths(path: &Path) -> Vec<PathBuf> {
    if fs::file_ops::is_trash_files_dir(path) {
        vec![
            fs::file_ops::trash_files_dir(),
            fs::file_ops::trash_info_dir(),
        ]
    } else {
        vec![path.to_path_buf()]
    }
}

fn apply_async_event(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    thumbnail_flush: &ThumbnailFlushScheduler,
    event: AsyncEvent,
) {
    match event {
        AsyncEvent::DirectoryLoaded(result) => apply_directory_result(ui, state, bridge, result),
        AsyncEvent::DirectoryEntriesRemoved(result) => {
            apply_directory_entries_removed(ui, state, bridge, result);
        }
        AsyncEvent::DirectoryPrefetched { path, result } => {
            apply_directory_prefetch_result(state, path, result);
        }
        AsyncEvent::FileOpened(result) => apply_file_open_result(ui, state, result),
        AsyncEvent::RecursiveSearchProgress(progress) => {
            apply_recursive_search_progress(ui, state, progress);
        }
        AsyncEvent::RecursiveSearchFinished(result) => {
            apply_recursive_search_result(ui, state, bridge, result);
        }
        AsyncEvent::LocalSearchIndexPrepared(result) => {
            apply_local_search_index_result(ui, state, bridge, result);
        }
        AsyncEvent::LocalSearchIndexPrepareFailed {
            pane_id,
            generation,
        } => {
            apply_local_search_index_prepare_failure(state, pane_id, generation);
        }
        AsyncEvent::OpenWithAppsLoaded(result) => {
            open_with::apply_open_with_apps_result(ui, state, result)
        }
        AsyncEvent::OtherApplicationAppsLoaded(result) => {
            open_with::apply_other_application_apps_result(ui, state, result);
        }
        AsyncEvent::DefaultAppSet(result) => {
            open_with::apply_default_app_set_result(ui, state, result)
        }
        AsyncEvent::ServiceMenuActionsLoaded(result) => {
            context_service_menu::apply_actions_result(ui, state, result);
        }
        AsyncEvent::ServiceMenuActionFinished(result) => {
            context_service_menu::apply_launch_result(ui, state, result);
        }
        AsyncEvent::FileActionFinished(result) => {
            apply_file_action_result(ui, state, bridge, result);
        }
        AsyncEvent::FileOperationProgress(progress) => {
            apply_file_operation_progress(ui, state, progress);
        }
        AsyncEvent::FileOperationFinished(result) => {
            apply_file_operation_result(ui, state, bridge, result);
        }
        AsyncEvent::FileUndoFinished(result) => {
            apply_file_undo_result(ui, state, bridge, result);
        }
        AsyncEvent::DeviceMountFinished(result) => {
            apply_device_mount_result(ui, state, bridge, result);
        }
        AsyncEvent::DeviceActionFinished(result) => {
            apply_device_action_result(ui, state, bridge, result);
        }
        AsyncEvent::DevicesChanged => {
            refresh_devices_async(state, bridge);
        }
        AsyncEvent::DevicesLoaded(result) => {
            apply_devices_loaded_result(ui, state, bridge, result);
        }
        AsyncEvent::ClipboardLoaded(result) => {
            apply_clipboard_load_result(ui, state, result);
        }
        AsyncEvent::ClipboardPasteLoaded(result) => {
            apply_clipboard_paste_load_result(ui, state, bridge, result);
        }
        AsyncEvent::VirtualViewPrepared(result) => {
            apply_virtual_view_result(ui, state, bridge, result);
        }
        AsyncEvent::VirtualViewPrepareFailed {
            pane_id,
            generation,
        } => {
            apply_virtual_view_prepare_failure(state, bridge, pane_id, generation);
        }
        AsyncEvent::PrivilegedOperationFinished(result) => {
            apply_privileged_operation_result(ui, state, bridge, result);
        }
        AsyncEvent::ExternalEditFinished(result) => {
            apply_external_edit_result(ui, state, bridge, result);
        }
        AsyncEvent::ThumbnailLoaded {
            pane_id,
            generation,
            load,
        } => thumbnail_flush.push(pane_id, generation, load),
    }
}

fn apply_directory_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DirectoryLoadResult,
) {
    {
        let state = state.borrow();
        let Some(pane) = state.panes.pane_for_target(PaneTarget::Id(result.pane_id)) else {
            debug_log(&format!(
                "directory_loaded stale missing-pane pane_id={} generation={} request={} path={}",
                result.pane_id,
                result.generation,
                result.request,
                result.path.display()
            ));
            return;
        };
        if !pane.load_generation.is_current(result.generation) || result.path != pane.current_dir {
            debug_log(&format!(
                "directory_loaded stale pane_id={} generation={} request={} path={} current={} current_generation_match={}",
                result.pane_id,
                result.generation,
                result.request,
                result.path.display(),
                pane.current_dir.display(),
                pane.load_generation.is_current(result.generation)
            ));
            return;
        }
    }
    if !directory_read_result_is_current(result.pane_id, result.generation, result.request, bridge)
    {
        debug_log(&format!(
            "directory_loaded stale request pane_id={} generation={} request={} path={}",
            result.pane_id,
            result.generation,
            result.request,
            result.path.display()
        ));
        return;
    }

    let Some(slot) = state.borrow().panes.slot_for_id(result.pane_id) else {
        return;
    };
    apply_pane_directory_result(ui, state, bridge, result, slot);
}

fn apply_directory_entries_removed(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DirectoryEntriesRemoved,
) {
    let removed_paths = result
        .removed_paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<HashSet<_>>();
    if removed_paths.is_empty() {
        return;
    }

    let Some((slot, selected_paths, removed)) = ({
        let mut state_ref = state.borrow_mut();
        state_ref.remove_directory_cache(&result.path);
        let Some(slot) = state_ref.panes.slot_for_id(result.pane_id) else {
            debug_log(&format!(
                "directory_removed stale missing-pane pane_id={} generation={} path={}",
                result.pane_id,
                result.generation,
                result.path.display()
            ));
            return;
        };
        let Some(pane) = state_ref.panes.pane_mut_by_id(result.pane_id) else {
            return;
        };
        if !pane.load_generation.is_current(result.generation) || pane.current_dir != result.path {
            debug_log(&format!(
                "directory_removed stale pane_id={} generation={} path={} current={} current_generation_match={}",
                result.pane_id,
                result.generation,
                result.path.display(),
                pane.current_dir.display(),
                pane.load_generation.is_current(result.generation)
            ));
            return;
        }
        let removed = pane.remove_entries_by_paths(&removed_paths);
        Some((slot, pane.selection.paths.clone(), removed))
    }) else {
        return;
    };

    let Some(removed) = removed else {
        return;
    };

    let selected_path = selected_paths
        .last()
        .map_or_else(SharedString::new, |path| path.as_str().into());
    let selected_count = selected_paths.len() as i32;
    let selected_status = selection_status_text(&selected_paths);
    if ui.get_focused_pane() == slot {
        ui.set_selected_path(selected_path);
        ui.set_selected_count(selected_count);
        ui.set_selected_status(selected_status);
    }

    debug_log(&format!(
        "directory_removed applied slot={slot} pane_id={} generation={} path={} removed={}",
        result.pane_id,
        result.generation,
        result.path.display(),
        removed_paths.len()
    ));
    sync_pane_slot_ui(ui, state, slot);
    if !apply_removed_entries_relayout_for_slot(ui, state, bridge, slot, &removed) {
        clear_pane_rendered_virtual_slice(ui, state, slot);
        sync_pane_view_for_slot(ui, state, bridge, slot);
    }
    set_directory_status_from_entries(ui, state, result.pane_id);
}

#[cfg(test)]
fn directory_removed_path_set(
    current_entries: &app::pane::PaneEntryModel,
    incoming_entries: &PreparedDirectoryEntries,
) -> HashSet<String> {
    let incoming_paths = incoming_entries
        .entries
        .iter()
        .map(|entry| entry.model_path().to_string())
        .collect::<HashSet<_>>();

    current_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.model_path();
            (!incoming_paths.contains(path)).then(|| path.to_string())
        })
        .collect()
}

#[derive(Debug, Default)]
struct DirectoryReloadDiff {
    removed_paths: HashSet<String>,
    inserted_width_ranges: Vec<(usize, Vec<f32>)>,
    retained_order_preserved: bool,
    retained_widths_unchanged: bool,
}

impl DirectoryReloadDiff {
    fn supports_index_delta_relayout(&self) -> bool {
        self.retained_order_preserved && self.retained_widths_unchanged
    }
}

#[derive(Debug, Default)]
struct DirectoryReloadRelayout {
    removed: Option<PaneEntriesRemoved>,
    inserted_width_ranges: Vec<(usize, Vec<f32>)>,
}

fn directory_reload_diff(
    current_entries: &app::pane::PaneEntryModel,
    incoming_entries: &PreparedDirectoryEntries,
) -> DirectoryReloadDiff {
    let current_paths = current_entries
        .iter()
        .map(|entry| entry.model_path().to_string())
        .collect::<HashSet<_>>();
    let incoming_paths = incoming_entries
        .entries
        .iter()
        .map(|entry| entry.model_path().to_string())
        .collect::<HashSet<_>>();
    let removed_paths = current_paths
        .iter()
        .filter(|path| !incoming_paths.contains(path.as_str()))
        .cloned()
        .collect::<HashSet<_>>();

    let mut inserted_width_ranges = Vec::<(usize, Vec<f32>)>::new();
    for (index, entry) in incoming_entries.entries.iter().enumerate() {
        if current_paths.contains(entry.model_path()) {
            continue;
        }
        if let Some((range_start, width_units)) = inserted_width_ranges.last_mut()
            && range_start.saturating_add(width_units.len()) == index
        {
            width_units.push(entry.model_name_width_units());
            continue;
        }
        inserted_width_ranges.push((index, vec![entry.model_name_width_units()]));
    }

    let retained_current_paths = current_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.model_path();
            incoming_paths.contains(path).then(|| path.to_string())
        })
        .collect::<Vec<_>>();
    let retained_incoming_paths = incoming_entries
        .entries
        .iter()
        .filter_map(|entry| {
            let path = entry.model_path();
            current_paths.contains(path).then(|| path.to_string())
        })
        .collect::<Vec<_>>();
    let retained_order_preserved = retained_current_paths == retained_incoming_paths;

    let current_widths = current_entries
        .iter()
        .map(|entry| {
            (
                entry.model_path().to_string(),
                entry.model_name_width_units(),
            )
        })
        .collect::<HashMap<_, _>>();
    let retained_widths_unchanged = incoming_entries.entries.iter().all(|entry| {
        current_widths
            .get(entry.model_path())
            .is_none_or(|width| (*width - entry.model_name_width_units()).abs() <= f32::EPSILON)
    });

    DirectoryReloadDiff {
        removed_paths,
        inserted_width_ranges,
        retained_order_preserved,
        retained_widths_unchanged,
    }
}

fn clear_pane_rendered_virtual_slice(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    {
        let mut state_ref = state.borrow_mut();
        let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) else {
            return;
        };
        pane.view.clear_rendered_virtual_slice();
    }
    sync_pane_view_ui(ui, state, slot);
}

fn apply_reload_delta_relayout_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    delta: &DirectoryReloadRelayout,
) -> bool {
    let Some(request) = reload_delta_relayout_request_for_slot(ui, state, slot, delta, true) else {
        return false;
    };
    let update = prepare_virtual_view_snapshot_update(*request.input.clone());
    let projection = prepare_virtual_view_projection(
        &update,
        request.cell_width,
        request.render_metrics,
        request.show_location,
        request.thumbnail_size_px,
        request.schedule_thumbnails,
    );
    apply_virtual_view_result(
        ui,
        state,
        bridge,
        VirtualViewResult {
            pane_id: request.pane_id,
            generation: request.generation,
            thumbnail_size_px: request.thumbnail_size_px,
            schedule_thumbnails: request.schedule_thumbnails,
            schedule_visible_thumbnail_roles_after_apply: request
                .schedule_visible_thumbnail_roles_after_apply,
            update,
            projection,
        },
    );
    true
}

fn apply_removed_entries_relayout_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    removed: &PaneEntriesRemoved,
) -> bool {
    if removed.visible_ranges.is_empty() {
        return true;
    }
    apply_reload_delta_relayout_for_slot(
        ui,
        state,
        bridge,
        slot,
        &DirectoryReloadRelayout {
            removed: Some(removed.clone()),
            inserted_width_ranges: Vec::new(),
        },
    )
}

fn reload_delta_relayout_request_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    delta: &DirectoryReloadRelayout,
    schedule_thumbnails: bool,
) -> Option<VirtualViewPrepareRequest> {
    let size_px = thumbnail_size_px(ui);
    let zoom_level = ui.get_icon_zoom_level();
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let main_width = (window_size.width - ui.get_sidebar_width_px()).max(1.0);
    let viewport_width = pane_slot_width(ui, main_width, slot);
    let (search_panel_visible, text_line_count) = {
        let state_ref = state.borrow();
        state_ref
            .panes
            .pane_for_slot(slot)
            .map(|pane| {
                (
                    pane.search.panel_visible(),
                    pane.item_view_text_line_count(),
                )
            })
            .unwrap_or((false, 1))
    };
    let mut layout = MainItemViewLayout::from_ui_for_pane_width_with_text_lines(
        ui,
        viewport_width,
        search_panel_visible,
        text_line_count,
    );
    let render_metrics =
        ItemViewRenderMetrics::from_zoom_level_with_text_line_count(zoom_level, text_line_count);

    let mut state_ref = state.borrow_mut();
    let chooser_patterns = state_ref
        .chooser_filters
        .get(state_ref.chooser_filter_index)
        .map(|filter| filter.patterns.clone())
        .unwrap_or_default();
    let pane = state_ref.panes.pane_mut_for_slot(slot)?;
    if pane.search.index_pending {
        return None;
    }
    let cached_layout = pane.view.virtual_view.layout.as_ref()?.as_compact();
    if !main_layout_matches_compact_layout(&layout, cached_layout) {
        return None;
    }

    let mut delta_layout = cached_layout.clone();
    if let Some(removed) = delta.removed.as_ref()
        && !removed.visible_ranges.is_empty()
    {
        delta_layout = delta_layout.without_item_ranges(&removed.visible_ranges);
    }
    if !delta.inserted_width_ranges.is_empty() {
        delta_layout = delta_layout.with_inserted_item_width_ranges(&delta.inserted_width_ranges);
    }
    let visible_count = pane_visible_entry_count_for_virtual_cache(pane, &chooser_patterns)?;
    if delta_layout.entry_count != visible_count {
        return None;
    }
    layout.viewport_x = pane.view.viewport_x;
    let requested_viewport_x = pane.view.viewport_x;
    let generation = pane.view.virtual_generation.next();
    let pane_id = pane.id;
    let show_location = pane.show_item_locations();
    pane.view.virtual_view.range = 0..0;
    pane.view
        .virtual_view
        .update_layout_signature_arc(Arc::new(delta_layout.into()), size_px);
    pane.view.clear_pending_virtual_prepare();

    Some(VirtualViewPrepareRequest {
        pane_id,
        generation,
        thumbnail_size_px: size_px,
        schedule_thumbnails,
        schedule_visible_thumbnail_roles_after_apply: false,
        cell_width: layout.cell_width,
        render_metrics,
        show_location,
        input: Box::new(VirtualViewSnapshotInput {
            layout,
            requested_viewport_x,
            range_hint: None,
            thumbnail_size_px: size_px,
            schedule_thumbnails,
            force_rebuild_model: true,
            visible_count_override: None,
            cache: pane.view.virtual_view.clone(),
            entries: pane.entry_model(),
            visible_entry_indices: pane.search.visible_entry_indices.clone(),
            visible_entries_have_locations: pane.search.visible_entries_have_locations,
            visible_location_groups: pane.search.visible_location_groups.clone(),
            query: pane.search.query.to_ascii_lowercase(),
            kind_filter: pane.search.kind_filter,
            modified_filter: pane.search.modified_filter,
            size_filter: pane.search.size_filter,
            chooser_patterns,
        }),
    })
}

fn apply_pane_directory_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DirectoryLoadResult,
    slot: i32,
) {
    let target_is_focused = ui.get_focused_pane() == slot;
    match result.result {
        Ok(entries) => {
            debug_log(&format!(
                "directory_loaded slot={} ok pane_id={} generation={} request={} path={} entries={} preserve_view={}",
                slot,
                result.pane_id,
                result.generation,
                result.request,
                result.path.display(),
                entries.len(),
                result.preserve_view
            ));
            if result.defer_view_restore {
                restore_pane_view_state(ui, state, slot, &result.path);
            }
            let mut unchanged = false;
            let mut selected_paths_after_model_change = None;
            let mut reload_relayout = None;
            let mut rendered_slice_cleared = false;
            {
                let mut state = state.borrow_mut();
                let Some(pane) = state
                    .panes
                    .pane_mut_for_target(PaneTarget::Id(result.pane_id))
                else {
                    return;
                };
                if directory_entries_match(&pane.entries, &entries) {
                    unchanged = true;
                } else {
                    if result.preserve_view {
                        let diff = directory_reload_diff(&pane.entries, &entries);
                        if diff.supports_index_delta_relayout() {
                            let removed = if diff.removed_paths.is_empty() {
                                None
                            } else {
                                pane.remove_entries_by_paths(&diff.removed_paths)
                            };
                            pane.set_entries_with_summary_preserving_rendered(
                                entries.entries.clone(),
                                entries.summary.clone(),
                            );
                            reload_relayout = Some(DirectoryReloadRelayout {
                                removed,
                                inserted_width_ranges: diff.inserted_width_ranges,
                            });
                        } else {
                            if !diff.removed_paths.is_empty() {
                                pane.apply_removed_paths_cleanup(&diff.removed_paths);
                            }
                            pane.set_entries_with_summary(
                                entries.entries.clone(),
                                entries.summary.clone(),
                            );
                            rendered_slice_cleared = true;
                        }
                    } else {
                        pane.set_entries_with_summary(
                            entries.entries.clone(),
                            entries.summary.clone(),
                        );
                    }
                    if !result.preserve_view {
                        pane.search.reset_all();
                        pane.selection.clear();
                    }
                    selected_paths_after_model_change = Some(pane.selection.paths.clone());
                }
            }
            state
                .borrow_mut()
                .insert_directory_cache(result.path.clone(), entries.clone());
            if unchanged {
                debug_log(&format!(
                    "directory_loaded unchanged slot={slot} generation={} request={} path={}",
                    result.generation,
                    result.request,
                    result.path.display()
                ));
            }
            if target_is_focused {
                ui.set_items_path(result.path.display().to_string().into());
                ui.set_directory_loading(false);
            }
            if let Some(selected_paths) = selected_paths_after_model_change {
                update_selection_ui_for_slot(ui, state, slot, &selected_paths);
            }
            if rendered_slice_cleared {
                sync_pane_view_ui(ui, state, slot);
            }
            if let Some(delta) = reload_relayout.as_ref()
                && !apply_reload_delta_relayout_for_slot(ui, state, bridge, slot, delta)
            {
                clear_pane_rendered_virtual_slice(ui, state, slot);
            }
            sync_pane_view_for_slot(ui, state, bridge, slot);
            set_directory_status_from_entries(ui, state, result.pane_id);
        }
        Err(err) => {
            debug_log(&format!(
                "directory_loaded slot={} error pane_id={} generation={} request={} path={} preserve_view={} error={err}",
                slot,
                result.pane_id,
                result.generation,
                result.request,
                result.path.display(),
                result.preserve_view
            ));
            if target_is_focused {
                ui.set_directory_loading(false);
            }
            sync_pane_view_for_slot(ui, state, bridge, slot);
            set_pane_status(ui, state, slot, &format!("Cannot read directory: {err}"));
        }
    }
}

fn apply_directory_prefetch_result(
    state: &Rc<RefCell<AppState>>,
    path: PathBuf,
    result: io::Result<PreparedDirectoryEntries>,
) {
    let mut state = state.borrow_mut();
    state.directory_prefetch_pending.remove(&path);
    match result {
        Ok(entries) => {
            if state.panes.focused().current_dir == path {
                debug_log(&format!(
                    "directory_prefetched ignored current path={}",
                    path.display()
                ));
                return;
            }
            state.insert_directory_cache(path.clone(), entries);
            debug_log(&format!(
                "directory_prefetched cached path={}",
                path.display()
            ));
        }
        Err(err) => {
            debug_log(&format!(
                "directory_prefetched skipped path={} error={err}",
                path.display()
            ));
        }
    }
}

fn open_file_for_target_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    target: PaneTarget,
    path: PathBuf,
) {
    let Some((pane_id, generation)) = ({
        let mut state = state.borrow_mut();
        state
            .panes
            .pane_mut_for_target(target)
            .map(|pane| (pane.id, pane.open_generation.next()))
    }) else {
        set_status(ui, state, "No split pane target is available");
        return;
    };
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_str().unwrap_or("file"));
    set_status_for_panes(ui, state, &[pane_id], &format!("Opening {label}..."));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = open_default_with_privilege_fallback(path.clone()).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileOpened(FileOpenResult {
                pane_id,
                generation,
                path,
                result,
            }),
        );
    });
}

fn apply_file_open_result(ui: &AppWindow, state: &Rc<RefCell<AppState>>, result: FileOpenResult) {
    let Some(summary) = ({
        let mut state = state.borrow_mut();
        state.complete_file_open(result)
    }) else {
        return;
    };

    if summary.external_edit_changed {
        sync_external_edit_ui(ui, state);
    }
    set_status_for_panes(ui, state, &[summary.pane_id], &summary.status);
}

async fn open_default_with_privilege_fallback(path: PathBuf) -> Result<FileOpenSuccess, String> {
    let open_path = path.clone();
    let direct = match tokio::task::spawn_blocking(move || {
        mime_open::open_file_with_default_app(&open_path)
    })
    .await
    {
        Ok(result) => result,
        Err(err) => return Err(format!("file open task failed: {err}")),
    };

    match direct {
        Ok(launch) => Ok(FileOpenSuccess {
            mime_type: launch.mime_type,
            unit: launch.unit,
            launch_diagnostic: launch.launch_diagnostic,
            external_edit: None,
        }),
        Err(err) if privilege::is_permission_error(&err) => {
            let mut session = privilege::prepare_external_edit_via_dbus(path).await?;
            let scratch_path = session.scratch_path.clone();
            let launch = match tokio::task::spawn_blocking(move || {
                mime_open::open_file_with_default_app(&scratch_path)
            })
            .await
            {
                Ok(result) => result?,
                Err(err) => return Err(format!("file open task failed: {err}")),
            };
            session.unit = launch.unit.clone();
            if let Err(err) = privilege::associate_external_edit_unit_via_dbus(&session).await {
                eprintln!("[fika launch] cannot associate protected edit with systemd unit: {err}");
            }
            Ok(FileOpenSuccess {
                mime_type: launch.mime_type,
                unit: launch.unit,
                launch_diagnostic: launch.launch_diagnostic,
                external_edit: Some(session),
            })
        }
        Err(err) => Err(err),
    }
}

fn open_search(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let slot = focus_current_ui_pane_slot(ui, state);
    open_search_for_slot(ui, state, bridge, slot);
}

fn open_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        pane.search.bar_open = true;
        pane.search.request_focus();
        pane.view.invalidate_virtual_view();
    }
    sync_pane_slot_ui(ui, state, slot);
    sync_pane_view_for_slot(ui, state, bridge, slot);
}

fn clear_focused_search(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) -> bool {
    let slot = { state.borrow().panes.focused_slot() };
    let query_is_non_empty = {
        let state = state.borrow();
        state
            .panes
            .pane_for_slot(slot)
            .is_some_and(|pane| pane.search.panel_visible() && !pane.search.query.is_empty())
    };
    if query_is_non_empty {
        clear_search_query_for_slot(ui, state, bridge, slot)
    } else {
        clear_search_for_slot(ui, state, bridge, slot, true)
    }
}

fn close_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    clear_search_for_slot(ui, state, bridge, slot, true);
}

fn submit_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    query: &str,
    recursive: bool,
) {
    let query = query.trim().to_string();
    let restore_entries = {
        let mut state = state.borrow_mut();
        cancel_active_search_for_slot(&mut state, slot);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        let restore_entries = query.is_empty()
            && (pane.search.recursive || pane.search.visible_entries_have_locations);
        pane.search.query = query.clone();
        pane.search.recursive = recursive && !query.is_empty();
        pane.search.loading = false;
        pane.search.bar_open = true;
        pane.search_generation.next();
        pane.view.invalidate_virtual_view();
        restore_entries.then(|| pane.current_dir.clone())
    };

    if let Some(current_dir) = restore_entries {
        if !restore_cached_directory_entries_for_slot(state, slot, &current_dir) {
            load_current_directory_for_slot(ui, state, bridge, slot, true);
            return;
        }
    }

    if query.is_empty() {
        apply_filter_for_slot(ui, state, bridge, slot, true);
    } else if recursive {
        start_recursive_search_for_slot(ui, state, bridge, slot, query);
    } else {
        apply_filter_for_slot(ui, state, bridge, slot, false);
    }
}

fn clear_search_query_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) -> bool {
    let restore_entries = {
        let mut state = state.borrow_mut();
        cancel_active_search_for_slot(&mut state, slot);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return false;
        };
        if pane.search.query.is_empty() {
            return false;
        }
        let restore_entries = pane.search.recursive || pane.search.visible_entries_have_locations;
        pane.search.query.clear();
        pane.search.recursive = false;
        pane.search.loading = false;
        pane.search.bar_open = true;
        pane.search.request_query_sync();
        pane.search_generation.next();
        pane.view.invalidate_virtual_view();
        restore_entries.then(|| pane.current_dir.clone())
    };

    if let Some(current_dir) = restore_entries {
        if !restore_cached_directory_entries_for_slot(state, slot, &current_dir) {
            load_current_directory_for_slot(ui, state, bridge, slot, true);
            return true;
        }
    }

    apply_filter_for_slot(ui, state, bridge, slot, true);
    true
}

fn clear_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    close_bar: bool,
) -> bool {
    let (was_visible, restore_entries, current_dir) = {
        let mut state = state.borrow_mut();
        cancel_active_search_for_slot(&mut state, slot);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return false;
        };
        let was_visible = pane.search.panel_visible();
        if !was_visible {
            return false;
        }
        let restore_entries = pane.search.recursive || pane.search.visible_entries_have_locations;
        let current_dir = pane.current_dir.clone();
        pane.search.reset_all();
        if !close_bar {
            pane.search.bar_open = true;
        }
        pane.search_generation.next();
        pane.view.invalidate_virtual_view();
        (was_visible, restore_entries, current_dir)
    };

    if restore_entries && !restore_cached_directory_entries_for_slot(state, slot, &current_dir) {
        load_current_directory_for_slot(ui, state, bridge, slot, true);
        return was_visible;
    }

    apply_filter_for_slot(ui, state, bridge, slot, true);
    was_visible
}

fn restore_cached_directory_entries_for_slot(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    current_dir: &Path,
) -> bool {
    let Some(entries) = ({ state.borrow_mut().cached_directory_entries(current_dir) }) else {
        return false;
    };
    let mut state = state.borrow_mut();
    let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
        return false;
    };
    pane.set_entries_with_summary(entries.entries.clone(), entries.summary.clone());
    true
}

fn cancel_recursive_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    let (query, progress, current_dir) = {
        let mut state = state.borrow_mut();
        cancel_active_search_for_slot(&mut state, slot);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        pane.search_generation.next();
        pane.search.loading = false;
        pane.search.recursive = false;
        pane.view.invalidate_virtual_view();
        (
            pane.search.query.clone(),
            pane.search_progress,
            pane.current_dir.clone(),
        )
    };

    restore_cached_directory_entries_for_slot(state, slot, &current_dir);
    apply_filter_for_slot(ui, state, bridge, slot, true);
    if query.is_empty() {
        set_pane_status(ui, state, slot, "Recursive search cancelled");
    } else {
        set_pane_status(
            ui,
            state,
            slot,
            &recursive_search_cancelled_status(
                &query,
                progress.directories_scanned,
                progress.matches_found,
            ),
        );
    }
}

fn update_search_filters_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    kind: i32,
    modified: i32,
    size: i32,
) {
    {
        let mut state = state.borrow_mut();
        set_search_filters_for_slot(&mut state, slot, kind, modified, size);
        if let Some(pane) = state.panes.pane_mut_for_slot(slot) {
            pane.search.bar_open = true;
        }
    }

    apply_filter_for_slot(ui, state, bridge, slot, true);
    let loading_query = {
        let state = state.borrow();
        state
            .panes
            .pane_for_slot(slot)
            .filter(|pane| pane.search.loading)
            .map(|pane| pane.search.query.clone())
    };
    if let Some(query) = loading_query {
        set_pane_status(ui, state, slot, &recursive_search_status(&query));
    }
}

fn start_recursive_search_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    query: String,
) {
    let (pane_id, root, generation, cancel) = {
        let mut state = state.borrow_mut();
        cancel_active_search_for_slot(&mut state, slot);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        let generation = pane.search_generation.next();
        pane.search_index_generation.next();
        let cancel = Arc::new(AtomicBool::new(false));
        pane.search_cancel = Some(cancel.clone());
        pane.search_progress = search::SearchProgress::default();
        pane.search.loading = true;
        pane.search.index_pending = false;
        pane.search.recursive = true;
        pane.search.visible_entry_indices = Some(Arc::from([]));
        pane.search.visible_entries_have_locations = false;
        pane.search.visible_location_groups = None;
        pane.selection.clear();
        pane.view.invalidate_virtual_view();
        (pane.id, pane.current_dir.clone(), generation, cancel)
    };

    set_pane_status(ui, state, slot, &recursive_search_status(&query));
    sync_virtual_entries_for_slot_with_count(ui, state, bridge, slot, true, Some(0), true, true);
    update_selection_ui_for_slot(ui, state, slot, &[]);

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let progress_tx = async_tx.clone();
        let progress_ui = notify_ui.clone();
        let progress_root = root.clone();
        let progress_query = query.clone();
        let result =
            search::search_recursive_with_progress(&root, &query, cancel, move |progress| {
                send_async_event(
                    progress_tx.clone(),
                    progress_ui.clone(),
                    AsyncEvent::RecursiveSearchProgress(RecursiveSearchProgress {
                        pane_id,
                        generation,
                        query: progress_query.clone(),
                        root: progress_root.clone(),
                        progress,
                    }),
                );
            })
            .await
            .map(PreparedDirectoryEntries::new);
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::RecursiveSearchFinished(RecursiveSearchResult {
                pane_id,
                generation,
                query,
                root,
                result,
            }),
        );
    });
}

fn apply_recursive_search_progress(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    progress: RecursiveSearchProgress,
) {
    let slot = {
        let mut state = state.borrow_mut();
        let Some(slot) = state.panes.slot_for_id(progress.pane_id) else {
            return;
        };
        let Some(pane) = state.panes.pane_mut_by_id(progress.pane_id) else {
            return;
        };
        let stale = !pane.search_generation.is_current(progress.generation)
            || pane.current_dir != progress.root
            || pane.search.query != progress.query
            || !pane.search.loading;
        if stale {
            return;
        }
        pane.search_progress = progress.progress;
        slot
    };

    set_pane_status(
        ui,
        state,
        slot,
        &recursive_search_progress_status(
            &progress.query,
            progress.progress.directories_scanned,
            progress.progress.matches_found,
        ),
    );
}

fn apply_recursive_search_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: RecursiveSearchResult,
) {
    let Some(slot) = ({
        let mut state = state.borrow_mut();
        let Some(slot) = state.panes.slot_for_id(result.pane_id) else {
            return;
        };
        let Some(pane) = state.panes.pane_mut_by_id(result.pane_id) else {
            return;
        };
        let stale = !pane.search_generation.is_current(result.generation)
            || pane.current_dir != result.root
            || pane.search.query != result.query;
        if stale {
            return;
        }
        pane.search_cancel = None;
        pane.search.loading = false;
        pane.view.invalidate_virtual_view();
        Some(slot)
    }) else {
        return;
    };

    match result.result {
        Ok(entries) => {
            {
                let mut state = state.borrow_mut();
                let Some(pane) = state.panes.pane_mut_by_id(result.pane_id) else {
                    return;
                };
                pane.set_entries_with_summary(entries.entries.clone(), entries.summary.clone());
            }
            apply_filter_for_slot(ui, state, bridge, slot, true);
        }
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            set_pane_status(
                ui,
                state,
                slot,
                &format!("Recursive search for '{}' cancelled", result.query),
            );
        }
        Err(err) => {
            set_pane_status(ui, state, slot, &format!("Recursive search failed: {err}"));
        }
    }
}

fn apply_file_operation_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: FileOperationResult,
) {
    let FileOperationResult {
        id,
        operation,
        source,
        target_dir,
        privileged_command,
        result,
    } = result;
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_file_operation(
            id,
            &operation,
            &source,
            &target_dir,
            result,
            privileged_command,
        )
    };
    let Some(summary) = summary else {
        return;
    };

    if let Some(registration) = summary.undo_registration {
        apply_undo_registration(ui, registration);
    }
    if let Some(request) = summary.privileged_request {
        let command = request.command;
        let reason = request.reason;
        file_actions::request_privileged_action(ui, state, command, &reason);
    }

    if !summary.refresh_pane_ids.is_empty() {
        refresh_panes(ui, state, bridge, &summary.refresh_pane_ids);
    }
    if let Some(status) = summary.status {
        set_status_for_panes(ui, state, &summary.refresh_pane_ids, &status);
    }
    start_next_operation(ui, state, bridge);
}

fn apply_file_action_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: file_actions::FileActionResult,
) {
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_file_action(result)
    };

    if let Some(registration) = summary.undo_registration {
        apply_undo_registration(ui, registration);
    }
    if let Some(request) = summary.privileged_request {
        let command = request.command;
        let reason = request.reason;
        file_actions::request_privileged_action(ui, state, command, &reason);
    }
    if let Some(status) = summary.status {
        let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);
        set_status_for_panes(ui, state, &pane_ids, &status);
    }
}

fn apply_undo_registration(ui: &AppWindow, summary: FileUndoRegistrationSummary) {
    cleanup_file_undo_backup(summary.cleanup_backup);
    apply_undo_ui(ui, &summary.undo_ui);
}

fn apply_undo_ui(ui: &AppWindow, undo_ui: &FileUndoUiState) {
    ui.set_undo_available(undo_ui.available);
    ui.set_undo_label(undo_ui.label.clone().into());
}

fn start_file_undo(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let decision = {
        let mut state = state.borrow_mut();
        state.take_file_undo_start()
    };
    let summary = match decision {
        FileUndoStartDecision::Started(summary) => summary,
        FileUndoStartDecision::Empty { status, undo_ui } => {
            apply_undo_ui(ui, &undo_ui);
            set_status(ui, state, &status);
            return;
        }
    };

    apply_undo_ui(ui, &summary.undo_ui);
    set_status_for_panes(ui, state, &summary.pane_ids, &summary.status);
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let undo = summary.undo;
    bridge.handle.spawn(async move {
        let task_undo = undo.clone();
        let result = tokio::task::spawn_blocking(move || match task_undo.operation.as_str() {
            "create-folder" => fs::file_ops::undo_create_folder(&task_undo.destination),
            "create-file" => fs::file_ops::undo_create_file(&task_undo.destination),
            "rename" => {
                fs::file_ops::undo_rename(&task_undo.original_source, &task_undo.destination)
            }
            "trash" => fs::file_ops::undo_trash(
                &task_undo
                    .items
                    .iter()
                    .map(|item| (item.original_source.clone(), item.destination.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ if let Some(backup) = task_undo.overwritten_backup.as_deref() => {
                fs::file_ops::undo_transfer_with_backup(
                    &task_undo.operation,
                    &task_undo.original_source,
                    &task_undo.destination,
                    Some(backup),
                )
            }
            _ => fs::file_ops::undo_transfer(
                &task_undo.operation,
                &task_undo.original_source,
                &task_undo.destination,
            ),
        })
        .await
        .unwrap_or_else(|err| Err(format!("undo task failed: {err}")));

        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileUndoFinished(FileUndoResult { undo, result }),
        );
    });
}

fn apply_file_undo_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: FileUndoResult,
) {
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_file_undo(result.undo, result.result)
    };
    cleanup_file_undo_backup(summary.cleanup_backup);
    if let Some(undo_ui) = &summary.undo_ui {
        apply_undo_ui(ui, undo_ui);
    }
    let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);
    set_status_for_panes(ui, state, &pane_ids, &summary.status);
}

fn apply_device_mount_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DeviceMountResult,
) {
    clear_pending_device_action(state, &result.device_path, "mount");
    match result.result {
        Ok(mount_point) if mount_point.is_dir() => {
            clear_device_error(state, &result.device_path);
            sync_devices(ui, state);
            refresh_devices_async(state, bridge);
            set_status(ui, state, &format!("Mounted {}", result.device_path));
            navigate_to(ui, state, bridge, mount_point);
        }
        Ok(mount_point) => {
            clear_device_error(state, &result.device_path);
            sync_devices(ui, state);
            refresh_devices_async(state, bridge);
            set_status(
                ui,
                state,
                &format!(
                    "Mounted {}, but mount point is not readable: {}",
                    result.device_path,
                    mount_point.display()
                ),
            );
        }
        Err(err) => {
            let status = format!("Cannot mount {}: {err}", result.device_path);
            set_device_error(state, &result.device_path, &status);
            sync_devices(ui, state);
            refresh_devices_async(state, bridge);
            set_status(ui, state, &status);
        }
    }
}

fn apply_device_action_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DeviceActionResult,
) {
    clear_pending_device_action(state, &result.device_path, &result.action);
    match result.result {
        Ok(()) => {
            clear_device_error(state, &result.device_path);
            sync_devices(ui, state);
            refresh_devices_async(state, bridge);
            if let Some(mount_path) = &result.mount_path
                && move_current_directory_home_if_inside_mount(state, mount_path)
            {
                sync_navigation_ui(ui, state);
                load_directory(ui, state, bridge);
            } else {
                sync_navigation_ui(ui, state);
                refresh_directory(ui, state, bridge);
            }
            set_status(
                ui,
                state,
                &format!(
                    "{} complete: {}",
                    title_case_action(&result.action),
                    result.device_path
                ),
            );
        }
        Err(err) => {
            let status = format!("Cannot {} {}: {err}", result.action, result.device_path);
            set_device_error(state, &result.device_path, &status);
            sync_devices(ui, state);
            refresh_devices_async(state, bridge);
            set_status(ui, state, &status);
        }
    }
}

fn apply_devices_loaded_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: DevicesLoadedResult,
) {
    {
        let mut state = state.borrow_mut();
        if !state.device_generation.is_current(result.generation) {
            return;
        }
        state.devices = result.devices;
    }
    sync_devices(ui, state);
    prefetch_sidebar_locations_async(state, bridge);
}

fn move_current_directory_home_if_inside_mount(
    state: &Rc<RefCell<AppState>>,
    mount_path: &Path,
) -> bool {
    let mut state = state.borrow_mut();
    state.panes.prune_mount_path(mount_path, home_dir())
}

fn title_case_action(action: &str) -> Cow<'static, str> {
    match action {
        "unmount" => Cow::Borrowed("Unmount"),
        "eject" => Cow::Borrowed("Eject"),
        other => Cow::Owned(other.to_string()),
    }
}

fn apply_file_operation_progress(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    progress: FileOperationProgress,
) {
    let update = {
        let mut state = state.borrow_mut();
        state.file_operation_progress_update(&progress)
    };

    if let Some(update) = update {
        set_status_for_panes(ui, state, &update.pane_ids, &update.status);
    }
}

fn apply_privileged_operation_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: privilege::PrivilegedOperationResult,
) {
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_privileged_operation(result)
    };
    let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);
    set_status_for_panes(ui, state, &pane_ids, &summary.status);
}

fn start_external_edit_resolution(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    operation: &str,
) {
    let decision = {
        let state = state.borrow();
        state.start_external_edit_resolution(slot, operation)
    };
    let summary = match decision {
        ExternalEditStartDecision::MissingPane { status } => {
            set_status(ui, state, &status);
            return;
        }
        ExternalEditStartDecision::MissingPending { pane_id, status } => {
            set_status_for_panes(ui, state, &[pane_id], &status);
            return;
        }
        ExternalEditStartDecision::Started(summary) => summary,
    };

    set_status_for_panes(ui, state, &[summary.pane_id], &summary.status);

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let pane_id = summary.pane_id;
    let operation = summary.operation;
    let session = summary.session;
    bridge.handle.spawn(async move {
        let result = if operation == EXTERNAL_EDIT_SAVE_OPERATION {
            privilege::commit_external_edit_via_dbus(&session).await
        } else {
            privilege::discard_external_edit_via_dbus(&session).await
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ExternalEditFinished(ExternalEditResult {
                pane_id,
                operation,
                session,
                result,
            }),
        );
    });
}

fn apply_external_edit_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: ExternalEditResult,
) {
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_external_edit(result)
    };

    if summary.pending_changed {
        sync_external_edit_ui(ui, state);
    }
    let refreshed_pane_ids =
        refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);
    let status_pane_ids = summary.status_pane_ids(&refreshed_pane_ids);
    set_status_for_panes(ui, state, &status_pane_ids, &summary.status);
}

fn sync_external_edit_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    sync_pane_slots_ui(ui, state);
}

#[cfg(test)]
fn external_edit_status_for_pane(edits: &[PaneExternalEdit], pane_id: u64) -> String {
    let mut pane_edits = edits.iter().filter(|edit| edit.pane_id == pane_id);
    let Some(first) = pane_edits.next() else {
        return String::new();
    };
    let extra_count = pane_edits.count();
    if extra_count == 0 {
        let label = first
            .session
            .original_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("protected file");
        format!("Admin write-back: {label}")
    } else {
        format!("{} admin write-backs pending", extra_count + 1)
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

pub(crate) fn sync_virtual_entries_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
) {
    sync_virtual_entries_for_slot_with_count(
        ui,
        state,
        bridge,
        slot,
        schedule_thumbnails,
        None,
        false,
        false,
    );
}

fn pane_slot_width(ui: &AppWindow, main_width: f32, slot: i32) -> f32 {
    if !ui.get_split_view_open() || slot <= 0 {
        return active_main_pane_width(
            main_width,
            ui.get_split_view_open(),
            ui.get_split_pane_ratio(),
        );
    }
    inactive_main_pane_width(main_width, true, ui.get_split_pane_ratio())
}

fn sync_virtual_entries_for_slot_with_count(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
    immediate: bool,
    publish_layout_on_cache: bool,
) {
    sync_virtual_entries_for_slot_with_count_and_cache_policy(
        ui,
        state,
        bridge,
        slot,
        schedule_thumbnails,
        visible_count_override,
        immediate,
        publish_layout_on_cache,
        false,
        false,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn sync_virtual_entries_for_slot_with_count_and_cache_policy(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
    immediate: bool,
    publish_layout_on_cache: bool,
    force_uncached_prepare: bool,
    force_rebuild_model: bool,
    schedule_visible_thumbnail_roles_after_apply: bool,
) {
    let sync_timer = PerfTimer::start();
    let size_px = thumbnail_size_px(ui);
    let zoom_level = ui.get_icon_zoom_level();
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let main_width = (window_size.width - ui.get_sidebar_width_px()).max(1.0);
    let viewport_width = pane_slot_width(ui, main_width, slot);
    let (search_panel_visible, text_line_count) = {
        let state_ref = state.borrow();
        state_ref
            .panes
            .pane_for_slot(slot)
            .map(|pane| {
                (
                    pane.search.panel_visible(),
                    pane.item_view_text_line_count(),
                )
            })
            .unwrap_or((false, 1))
    };
    let mut layout = MainItemViewLayout::from_ui_for_pane_width_with_text_lines(
        ui,
        viewport_width,
        search_panel_visible,
        text_line_count,
    );
    let render_metrics =
        ItemViewRenderMetrics::from_zoom_level_with_text_line_count(zoom_level, text_line_count);
    let Some(request) = ({
        let mut state_ref = state.borrow_mut();
        let chooser_patterns = state_ref
            .chooser_filters
            .get(state_ref.chooser_filter_index)
            .map(|filter| filter.patterns.clone())
            .unwrap_or_default();
        let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) else {
            return;
        };
        if pane.search.index_pending {
            item_view_perf::log(format_args!(
                "virtual_sync slot={} search_index_pending=true sync_ms={:.3}",
                slot,
                sync_timer.elapsed_ms()
            ));
            return;
        }
        let requested_viewport_x = pane.view.viewport_x;
        let show_location = pane.show_item_locations();
        layout.viewport_x = requested_viewport_x;
        if !pane.view.has_renderable_virtual_entries() {
            pane.view.virtual_view.invalidate();
        }
        if !force_uncached_prepare
            && !force_rebuild_model
            && let Some(sync) = cached_virtual_viewport_sync(
                pane,
                &layout,
                requested_viewport_x,
                size_px,
                schedule_thumbnails,
                visible_count_override,
                &chooser_patterns,
            )
        {
            pane.view.virtual_generation.next();
            pane.view.clear_pending_virtual_prepare();
            Some(VirtualViewSyncRequest::Cached {
                sync,
                publish_layout: publish_layout_on_cache,
            })
        } else {
            let generation = pane.view.virtual_generation.next();
            let query = pane.search.query.to_ascii_lowercase();
            let request = VirtualViewPrepareRequest {
                pane_id: pane.id,
                generation,
                thumbnail_size_px: size_px,
                schedule_thumbnails,
                schedule_visible_thumbnail_roles_after_apply,
                cell_width: layout.cell_width,
                render_metrics,
                show_location,
                input: Box::new(VirtualViewSnapshotInput {
                    layout,
                    requested_viewport_x,
                    range_hint: None,
                    thumbnail_size_px: size_px,
                    schedule_thumbnails,
                    force_rebuild_model,
                    visible_count_override,
                    cache: if force_uncached_prepare {
                        VirtualViewCache::default()
                    } else {
                        pane.view.virtual_view.clone()
                    },
                    entries: pane.entry_model(),
                    visible_entry_indices: pane.search.visible_entry_indices.clone(),
                    visible_entries_have_locations: pane.search.visible_entries_have_locations,
                    visible_location_groups: pane.search.visible_location_groups.clone(),
                    query,
                    kind_filter: pane.search.kind_filter,
                    modified_filter: pane.search.modified_filter,
                    size_filter: pane.search.size_filter,
                    chooser_patterns,
                }),
            };
            if pane.view.has_virtual_prepare_in_flight() {
                pane.view.defer_virtual_prepare(request);
                Some(VirtualViewSyncRequest::Deferred)
            } else {
                pane.view.mark_virtual_prepare_started(generation);
                Some(VirtualViewSyncRequest::Prepare(request))
            }
        }
    }) else {
        return;
    };

    let request = match request {
        VirtualViewSyncRequest::Cached {
            sync,
            publish_layout,
        } => {
            if publish_layout {
                sync_pane_view_ui(ui, state, slot);
            } else if sync.publish_viewport {
                set_pane_viewport_ui(ui, slot, sync.viewport_x, state);
            }
            item_view_perf::log(format_args!(
                "scroll slot={} cached=true viewport={:.0} range={}..{} visible={}..{} entry_count={} publish_viewport={} publish_layout={} sync_ms={:.3} model_writes=0",
                slot,
                sync.viewport_x,
                sync.cached_range.start,
                sync.cached_range.end,
                sync.visible_range.start,
                sync.visible_range.end,
                sync.entry_count,
                sync.publish_viewport,
                publish_layout,
                sync_timer.elapsed_ms()
            ));
            if schedule_visible_thumbnail_roles_after_apply {
                schedule_visible_thumbnail_roles_for_slot(ui, state, bridge, slot);
            }
            return;
        }
        VirtualViewSyncRequest::Deferred => {
            item_view_perf::log(format_args!(
                "virtual_sync slot={} deferred=true immediate={} sync_ms={:.3}",
                slot,
                immediate,
                sync_timer.elapsed_ms()
            ));
            return;
        }
        VirtualViewSyncRequest::Prepare(request) => request,
    };

    start_virtual_view_prepare(bridge, request);
}

fn start_virtual_view_prepare(bridge: &AsyncBridge, request: VirtualViewPrepareRequest) {
    let VirtualViewPrepareRequest {
        pane_id,
        generation,
        thumbnail_size_px,
        schedule_thumbnails,
        schedule_visible_thumbnail_roles_after_apply,
        cell_width,
        render_metrics,
        show_location,
        input,
    } = request;
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        match tokio::task::spawn_blocking(move || {
            let prepare_timer = PerfTimer::start();
            let update = prepare_virtual_view_snapshot_update(*input);
            let prepare_ms = prepare_timer.elapsed_ms();
            let projection_timer = PerfTimer::start();
            let projection = prepare_virtual_view_projection(
                &update,
                cell_width,
                render_metrics,
                show_location,
                thumbnail_size_px,
                schedule_thumbnails,
            );
            (update, projection, prepare_ms, projection_timer.elapsed_ms())
        })
        .await
        {
            Ok((update, projection, prepare_ms, projection_ms)) => {
                item_view_perf::log(format_args!(
                    "prepare pane_id={} generation={} immediate=false rebuild={} range={}..{} visible={}..{} prepare_ms={:.3} projection_ms={:.3}",
                    pane_id,
                    generation,
                    update.rebuild_model,
                    update.range.start,
                    update.range.end,
                    update.visible_range.start,
                    update.visible_range.end,
                    prepare_ms,
                    projection_ms
                ));
                send_async_event(
                    async_tx,
                    notify_ui,
                    AsyncEvent::VirtualViewPrepared(VirtualViewResult {
                        pane_id,
                        generation,
                        thumbnail_size_px,
                        schedule_thumbnails,
                        schedule_visible_thumbnail_roles_after_apply,
                        update,
                        projection,
                    }),
                )
            }
            Err(_) => send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::VirtualViewPrepareFailed {
                    pane_id,
                    generation,
                },
            ),
        }
    });
}

fn prepare_virtual_view_projection(
    update: &app::virtual_view::VirtualViewSnapshotUpdate,
    cell_width: f32,
    render_metrics: ItemViewRenderMetrics,
    show_location: bool,
    thumbnail_size_px: u32,
    schedule_thumbnails: bool,
) -> Option<VirtualViewProjection> {
    if !update.rebuild_model {
        return None;
    }

    let metadata_sources = if show_location {
        update
            .entries
            .iter()
            .map(ItemViewModelEntry::model_metadata_source)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut entries = update
        .entries
        .iter()
        .map(ItemViewModelEntry::model_to_item_view_entry)
        .collect::<Vec<_>>();
    let metadata_entries = decorate_render_plan_with_metadata(
        &mut entries,
        ItemViewRenderPlanInput {
            cell_width,
            render_metrics,
            show_location,
        },
        &metadata_sources,
    );
    let bounds_entries =
        item_view_bounds_entries(update.layout.as_ref(), update.range.start, entries.len());
    let metadata_rows = metadata_entries.len();
    let slot_projections = item_view_slot_projections_for_entries(
        update.range.start,
        &entries,
        &bounds_entries,
        metadata_entries,
    );
    let thumbnail_keys = if schedule_thumbnails {
        prepare_thumbnail_keys_for_entries(&entries, thumbnail_size_px)
    } else {
        Vec::new()
    };

    Some(VirtualViewProjection {
        entries,
        bounds_entries,
        slot_projections,
        thumbnail_keys,
        metadata_rows,
    })
}

#[allow(clippy::too_many_arguments)]
fn cached_virtual_viewport_sync(
    pane: &mut PaneState,
    layout: &MainItemViewLayout,
    requested_viewport_x: f32,
    thumbnail_size_px: u32,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
    chooser_patterns: &[String],
) -> Option<CachedVirtualViewportSync> {
    if !schedule_thumbnails || visible_count_override.is_some() {
        return None;
    }

    let compact_item_view = pane.view.virtual_view.layout.as_ref()?.as_compact();
    let current_entry_count = pane_visible_entry_count_for_virtual_cache(pane, chooser_patterns)?;
    if compact_item_view.entry_count != current_entry_count {
        return None;
    }
    if !main_layout_matches_compact_layout(layout, compact_item_view)
        || pane.view.virtual_view.thumbnail_size_px != thumbnail_size_px
    {
        return None;
    }
    let plan = compact_item_view.virtual_plan(requested_viewport_x, ITEM_VIEW_OVERSCAN_COLUMNS);
    if !virtual_cache_covers_visible_range(&pane.view.virtual_view.range, &plan.visible_range) {
        return None;
    }

    pane.view.viewport_x = plan.viewport_x;
    Some(CachedVirtualViewportSync {
        viewport_x: plan.viewport_x,
        publish_viewport: (plan.viewport_x - requested_viewport_x).abs() > f32::EPSILON,
        cached_range: pane.view.virtual_view.range.clone(),
        visible_range: plan.visible_range,
        entry_count: current_entry_count,
    })
}

fn pane_visible_entry_count_for_virtual_cache(
    pane: &PaneState,
    chooser_patterns: &[String],
) -> Option<usize> {
    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return Some(indices.len());
    }

    (pane.search.query.is_empty()
        && pane.search.kind_filter == 0
        && pane.search.modified_filter == 0
        && pane.search.size_filter == 0
        && chooser_patterns.is_empty())
    .then_some(pane.entries.len())
}

fn main_layout_matches_compact_layout(
    layout: &MainItemViewLayout,
    compact_item_view: &CompactItemViewLayout,
) -> bool {
    layout.rows_per_column == compact_item_view.rows_per_column
        && same_layout_metric(layout.viewport_width, compact_item_view.viewport_width)
        && same_layout_metric(layout.cell_width, compact_item_view.cell_width)
        && same_layout_metric(layout.row_height, compact_item_view.row_height)
        && same_layout_metric(layout.padding, compact_item_view.padding)
}

fn same_layout_metric(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.5
}

fn virtual_cache_covers_visible_range(
    cached_range: &std::ops::Range<usize>,
    visible_range: &std::ops::Range<usize>,
) -> bool {
    if visible_range.is_empty() {
        return cached_range.is_empty();
    }

    cached_range.start <= visible_range.start && cached_range.end >= visible_range.end
}

fn apply_virtual_view_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: VirtualViewResult,
) {
    let apply_timer = PerfTimer::start();
    let pane_id = result.pane_id;
    let generation = result.generation;
    let thumbnail_size_px = result.thumbnail_size_px;
    let schedule_thumbnails = result.schedule_thumbnails;
    let schedule_visible_thumbnail_roles_after_apply =
        result.schedule_visible_thumbnail_roles_after_apply;
    let projection = result.projection;
    let update = result.update;
    let slot;
    let follow_up_request;
    let result_is_current;
    {
        let mut state_ref = state.borrow_mut();
        slot = match state_ref.panes.slot_for_id(pane_id) {
            Some(s) => s,
            None => return,
        };
        let Some(pane) = state_ref.panes.pane_mut_by_id(pane_id) else {
            return;
        };
        follow_up_request = pane.view.finish_virtual_prepare(generation);
        result_is_current = pane.view.virtual_generation.is_current(generation);
        if result_is_current {
            pane.view.viewport_x = update.viewport_x;
            if update.rebuild_model {
                pane.view.virtual_view.range = update.range.clone();
                pane.view
                    .virtual_view
                    .update_layout_signature_arc(Arc::clone(&update.layout), thumbnail_size_px);
            }
        }
    }

    if let Some(request) = follow_up_request {
        start_virtual_view_prepare(bridge, request);
    }
    if !result_is_current {
        debug_log(&format!(
            "virtual_view_result stale pane_id={} generation={}",
            pane_id, generation
        ));
        item_view_perf::log(format_args!(
            "virtual_apply pane_id={} generation={} stale=true apply_ms={:.3}",
            pane_id,
            generation,
            apply_timer.elapsed_ms()
        ));
        return;
    }

    let target_is_focused = state.borrow().panes.focused_slot() == slot;
    if !update.rebuild_model {
        if update.viewport_clamped {
            set_pane_viewport_ui(ui, slot, update.viewport_x, state);
        }
        if target_is_focused && ui.get_entry_count() != update.entry_count as i32 {
            ui.set_entry_count(update.entry_count as i32);
            sync_pane_view_ui(ui, state, slot);
        }
        item_view_perf::log(format_args!(
            "virtual_apply slot={} pane_id={} generation={} rebuild=false clamped={} entry_count={} apply_ms={:.3}",
            slot,
            pane_id,
            generation,
            update.viewport_clamped,
            update.entry_count,
            apply_timer.elapsed_ms()
        ));
        return;
    }

    let Some(VirtualViewProjection {
        mut entries,
        bounds_entries,
        slot_projections,
        thumbnail_keys,
        metadata_rows,
    }) = projection
    else {
        item_view_perf::log(format_args!(
            "virtual_apply slot={} pane_id={} generation={} missing_projection=true apply_ms={:.3}",
            slot,
            pane_id,
            generation,
            apply_timer.elapsed_ms()
        ));
        return;
    };
    let (selected_paths, media_entries) = {
        let mut state_ref = state.borrow_mut();
        let selected_paths = state_ref
            .panes
            .pane_by_id(pane_id)
            .map(|pane| pane.selection.paths.clone())
            .unwrap_or_default();
        let media_entries = if schedule_thumbnails {
            decorate_entries_with_prepared_thumbnail_keys_for_pane(
                &mut state_ref,
                pane_id,
                &mut entries,
                &thumbnail_keys,
            )
        } else if let Some(pane) = state_ref.panes.pane_by_id(pane_id) {
            preserve_current_thumbnail_roles_for_deferred_icon_size_update(
                pane,
                update.range.start,
                &mut entries,
            )
        } else {
            Vec::new()
        };
        if state_ref.panes.pane_by_id(pane_id).is_none() {
            return;
        }
        (selected_paths, media_entries)
    };
    let thumbnail_schedule_entries = if schedule_thumbnails {
        entries
            .iter()
            .enumerate()
            .map(|(row, entry)| {
                ThumbnailScheduleEntry::from_entry_with_prepared_key(entry, thumbnail_keys.get(row))
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if schedule_thumbnails {
        let maximum_visible_items = update
            .visible_range
            .end
            .saturating_sub(update.visible_range.start)
            .max(1);
        if schedule_visible_thumbnail_roles_after_apply {
            schedule_visible_thumbnail_roles_for_entries(
                state,
                bridge,
                pane_id,
                &thumbnail_schedule_entries,
                update.range.start,
                update.visible_range.clone(),
                maximum_visible_items,
                thumbnail_size_px,
            );
        } else {
            schedule_thumbnail_roles_for_entries(
                state,
                bridge,
                pane_id,
                &thumbnail_schedule_entries,
                update.range.start,
                update.visible_range.clone(),
                maximum_visible_items,
                thumbnail_size_px,
            );
        }
    }
    let model_timer = PerfTimer::start();
    let model_stats = set_pane_virtual_entries(
        state,
        slot,
        update.range.start,
        entries,
        bounds_entries,
        slot_projections,
        media_entries,
        metadata_rows,
        &selected_paths,
        !schedule_thumbnails,
    );
    let model_ms = model_timer.elapsed_ms();
    if target_is_focused {
        ui.set_entry_count(update.entry_count as i32);
    }
    debug_log(&format!(
        "virtual_view_result applied pane_id={} generation={} range={}..{} entries={} entry_count={}",
        pane_id,
        generation,
        update.range.start,
        update.range.end,
        update.range.end.saturating_sub(update.range.start),
        update.entry_count
    ));
    let sync_ui_timer = PerfTimer::start();
    sync_pane_view_ui(ui, state, slot);
    let sync_ui_ms = sync_ui_timer.elapsed_ms();
    if let Some(stats) = model_stats {
        let slot_stats = stats.slot;
        item_view_perf::log(format_args!(
            "virtual_apply slot={} pane_id={} generation={} rebuild=true schedule_thumbnails={} range={}..{} visible={}..{} entries={} bounds={} media={} metadata={} active_slots={} inactive_slots={} reused_slots={} extended_slots={} patched={} content_patched={} geometry_patched={} thumbnail_patched={} thumbnail_reuse={} thumbnail_replace={} set_row_data={} model_extend={} model_rebuild={} raster_bumped={} model_ms={:.3} sync_ui_ms={:.3} apply_ms={:.3}",
            slot,
            pane_id,
            generation,
            schedule_thumbnails,
            update.range.start,
            update.range.end,
            update.visible_range.start,
            update.visible_range.end,
            stats.entry_rows,
            stats.bounds_rows,
            stats.media_rows,
            stats.metadata_rows,
            slot_stats.active_rows,
            slot_stats.inactive_rows,
            slot_stats.reused_slots,
            slot_stats.extended_slots,
            slot_stats.patched_rows,
            slot_stats.content_patched_rows,
            slot_stats.geometry_patched_rows,
            slot_stats.thumbnail_patched_rows,
            slot_stats.thumbnail_image_reused,
            slot_stats.thumbnail_image_replaced,
            slot_stats.set_row_data,
            slot_stats.model_extend_rows,
            slot_stats.model_rebuilt_rows,
            stats.raster_revision_bumped,
            model_ms,
            sync_ui_ms,
            apply_timer.elapsed_ms()
        ));
    }
}

fn preserve_current_thumbnail_roles_for_deferred_icon_size_update(
    pane: &PaneState,
    range_start: usize,
    entries: &mut [ItemViewEntry],
) -> Vec<ItemViewMediaSource> {
    let mut media_entries = Vec::new();
    let current_start = pane.view.virtual_start_index;
    for (row, entry) in entries.iter_mut().enumerate() {
        let absolute_index = range_start.saturating_add(row);
        let Some(current_row) = absolute_index.checked_sub(current_start) else {
            continue;
        };
        let Some(token) = pane.view.virtual_entry_tokens.get(current_row) else {
            continue;
        };
        if token.path() == entry.path.as_str() {
            entry.thumbnail_state = token.thumbnail_state();
            entry.media_token = token.media_token();
            if token.thumbnail_state() == THUMBNAIL_STATE_LOADED
                && let Some(media) =
                    thumbnail_media_for_token(pane, current_row as i32, token.media_token())
            {
                media_entries.push(ItemViewMediaSource {
                    slice_index: row as i32,
                    media,
                });
            }
        }
    }
    media_entries
}

fn thumbnail_media_for_token(
    pane: &PaneState,
    current_slice_index: i32,
    media_token: i32,
) -> Option<slint::Image> {
    let current_absolute_index = usize::try_from(current_slice_index)
        .ok()
        .map(|slice_index| pane.view.virtual_start_index.saturating_add(slice_index) as i32);
    pane.view
        .virtual_slot_entries
        .iter()
        .zip(pane.view.virtual_slot_tokens.iter())
        .find(|(slot, token)| {
            slot.active
                && slot.has_thumbnail
                && token.thumbnail_token() == media_token
                && current_absolute_index.is_some_and(|absolute_index| {
                    token
                        .absolute_index()
                        .is_some_and(|slot_index| slot_index == absolute_index)
                })
        })
        .or_else(|| {
            pane.view
                .virtual_slot_entries
                .iter()
                .zip(pane.view.virtual_slot_tokens.iter())
                .find(|(slot, token)| {
                    slot.active && slot.has_thumbnail && token.thumbnail_token() == media_token
                })
        })
        .map(|(slot, _)| slot.thumbnail.clone())
}

fn apply_virtual_view_prepare_failure(
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_id: u64,
    generation: u64,
) {
    let follow_up_request = {
        let mut state_ref = state.borrow_mut();
        let Some(pane) = state_ref.panes.pane_mut_by_id(pane_id) else {
            return;
        };
        pane.view.finish_virtual_prepare(generation)
    };
    if let Some(request) = follow_up_request {
        start_virtual_view_prepare(bridge, request);
    }
    debug_log(&format!(
        "virtual_view_prepare_failed pane_id={pane_id} generation={generation}"
    ));
}

fn set_pane_virtual_entries(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    start_index: usize,
    entries: Vec<ItemViewEntry>,
    bounds_entries: Vec<ItemViewItemBounds>,
    slot_projections: Vec<PreparedItemViewSlotProjection>,
    media_entries: Vec<ItemViewMediaSource>,
    metadata_rows: usize,
    selected_paths: &[String],
    defer_raster_update: bool,
) -> Option<ItemViewModelUpdateStats> {
    state
        .borrow_mut()
        .panes
        .pane_mut_for_slot(slot)
        .map(|pane| {
            pane.view.set_raster_updates_deferred(defer_raster_update);
            update_pane_item_view_entries_model_with_slot_projections(
                &mut pane.view,
                start_index,
                entries,
                bounds_entries,
                slot_projections,
                media_entries,
                metadata_rows,
                selected_paths,
            )
        })
}

fn item_view_bounds_entries(
    layout: &impl ItemViewLayouter,
    start_index: usize,
    count: usize,
) -> Vec<ItemViewItemBounds> {
    layout.bounds_for_range(start_index, count)
}

fn apply_filter_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    preserve_selection: bool,
) {
    let Some((pane_id, generation, total, entries, search, chooser_patterns)) = ({
        let mut state_ref = state.borrow_mut();
        let chooser_patterns = state_ref
            .chooser_filters
            .get(state_ref.chooser_filter_index)
            .map(|filter| filter.patterns.clone())
            .unwrap_or_default();
        let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) else {
            return;
        };
        let generation = pane.search_index_generation.next();
        pane.search.index_pending = true;
        pane.view.invalidate_virtual_view();
        Some((
            pane.id,
            generation,
            pane.entries.len(),
            pane.entry_model(),
            pane.search.clone(),
            chooser_patterns,
        ))
    }) else {
        return;
    };
    sync_pane_slot_ui(ui, state, slot);
    start_local_search_index_prepare(
        bridge,
        pane_id,
        generation,
        total,
        entries,
        search,
        chooser_patterns,
        preserve_selection,
    );
}

#[allow(clippy::too_many_arguments)]
fn start_local_search_index_prepare(
    bridge: &AsyncBridge,
    pane_id: u64,
    generation: u64,
    total: usize,
    entries: app::pane::PaneEntryModel,
    search: app::pane::PaneSearch,
    chooser_patterns: Vec<String>,
    preserve_selection: bool,
) {
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        match tokio::task::spawn_blocking(move || {
            prepare_visible_entry_index(entries, search, chooser_patterns, preserve_selection)
        })
        .await
        {
            Ok(result) => send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::LocalSearchIndexPrepared(LocalSearchIndexResult {
                    pane_id,
                    generation,
                    total,
                    preserve_selection,
                    result,
                }),
            ),
            Err(_) => send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::LocalSearchIndexPrepareFailed {
                    pane_id,
                    generation,
                },
            ),
        }
    });
}

fn apply_local_search_index_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: LocalSearchIndexResult,
) {
    let LocalSearchIndexResult {
        pane_id,
        generation,
        total,
        preserve_selection,
        result,
    } = result;
    let summary = result.summary.clone();
    let visible_paths = summary.visible_paths.clone().unwrap_or_default();
    let Some((slot, query, filters_active, recursive_loading, recursive_results)) = ({
        let mut state_ref = state.borrow_mut();
        let Some(slot) = state_ref.panes.slot_for_id(pane_id) else {
            return;
        };
        let Some(pane) = state_ref.panes.pane_mut_by_id(pane_id) else {
            return;
        };
        if !pane.search_index_generation.is_current(generation) {
            return;
        }
        pane.search.index_pending = false;
        apply_prepared_visible_entry_index_to_pane(pane, result);
        pane.view.virtual_view.invalidate();
        Some((
            slot,
            pane.search.query.clone(),
            pane.search.filters_active(),
            pane.search.recursive && pane.search.loading && !pane.search.query.is_empty(),
            pane.search.recursive && !pane.search.loading && !pane.search.query.is_empty(),
        ))
    }) else {
        return;
    };
    sync_virtual_entries_for_slot_with_count_and_cache_policy(
        ui,
        state,
        bridge,
        slot,
        true,
        Some(summary.count),
        false,
        false,
        true,
        false,
        false,
    );
    if preserve_selection {
        retain_visible_selection_for_slot(ui, state, slot, &visible_paths);
    } else {
        clear_selection_for_slot(ui, state, slot);
    }

    if recursive_loading {
        return;
    }

    if recursive_results {
        set_pane_status(
            ui,
            state,
            slot,
            &recursive_search_finished_status(summary.count, total),
        );
    } else if query.is_empty() && !filters_active {
        set_pane_status(
            ui,
            state,
            slot,
            &format!("{} folders, {} files", summary.folders, summary.files),
        );
    } else {
        set_pane_status(
            ui,
            state,
            slot,
            &format!(
                "{} of {total} items ({} folders, {} files)",
                summary.count, summary.folders, summary.files
            ),
        );
    }
}

fn apply_local_search_index_prepare_failure(
    state: &Rc<RefCell<AppState>>,
    pane_id: u64,
    generation: u64,
) {
    let mut state_ref = state.borrow_mut();
    let Some(pane) = state_ref.panes.pane_mut_by_id(pane_id) else {
        return;
    };
    if pane.search_index_generation.is_current(generation) {
        pane.search.index_pending = false;
    }
}

fn retain_visible_selection_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    visible_paths: &[String],
) {
    let selected_paths = {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        pane.selection.paths = retained_visible_paths(&pane.selection.paths, visible_paths);
        if pane
            .selection
            .anchor
            .as_ref()
            .is_some_and(|anchor| !visible_paths.iter().any(|visible| visible == anchor))
        {
            pane.selection.anchor = pane.selection.paths.last().cloned();
        }
        pane.selection.paths.clone()
    };
    update_selection_ui_for_slot(ui, state, slot, &selected_paths);
}

fn select_path_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    path: &str,
    toggle: bool,
    range: bool,
) {
    let selected_paths = {
        let mut state = state.borrow_mut();

        if range {
            let Some(pane) = state.panes.pane_for_slot(slot) else {
                return;
            };
            let anchor = pane
                .selection
                .anchor
                .as_deref()
                .or_else(|| pane.selection.paths.last().map(String::as_str))
                .unwrap_or(path);
            let range_paths = selection_range_paths_filtered_for_slot(&state, slot, anchor, path);
            let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
                return;
            };
            if toggle {
                append_unique_paths(&mut pane.selection.paths, range_paths);
            } else {
                pane.selection.paths = range_paths;
            }
        } else if toggle {
            let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
                return;
            };
            if let Some(index) = pane
                .selection
                .paths
                .iter()
                .position(|selected| selected == path)
            {
                pane.selection.paths.remove(index);
            } else {
                pane.selection.paths.push(path.to_string());
            }
        } else {
            let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
                return;
            };
            pane.selection.paths.clear();
            pane.selection.paths.push(path.to_string());
        }

        if !range {
            let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
                return;
            };
            pane.selection.anchor = Some(path.to_string());
        }
        state
            .panes
            .pane_for_slot(slot)
            .map(|pane| pane.selection.paths.clone())
            .unwrap_or_default()
    };

    update_selection_ui_for_slot(ui, state, slot, &selected_paths);
}

fn press_item_view_entry_at_point_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    x: f32,
    y: f32,
    toggle: bool,
    range: bool,
) -> bool {
    let action = {
        let mut state_ref = state.borrow_mut();
        let Some(action) = press_entry_at_pane_point(ui, &mut state_ref, slot, x, y, toggle, range)
        else {
            return false;
        };
        action
    };
    PaneController::new(ui, state, bridge).apply_item_view_controller_action(slot, action);
    true
}

fn activate_item_view_entry_at_point_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    x: f32,
    y: f32,
    bridge: &AsyncBridge,
) {
    let action = {
        let state_ref = state.borrow();
        let Some(action) = activate_entry_at_pane_point(ui, &state_ref, slot, x, y) else {
            return;
        };
        action
    };
    PaneController::new(ui, state, bridge).apply_item_view_controller_action(slot, action);
}

#[derive(Clone, Copy)]
struct ItemViewContextMenuRequest {
    slot: i32,
    x: f32,
    y: f32,
    abs_x: f32,
    abs_y: f32,
}

fn request_item_view_entry_context_menu_at_point_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    request: ItemViewContextMenuRequest,
) -> bool {
    let action = {
        let state_ref = state.borrow();
        let Some(action) = context_menu_entry_at_pane_point(
            ui,
            &state_ref,
            request.slot,
            request.x,
            request.y,
            request.abs_x,
            request.abs_y,
        ) else {
            return false;
        };
        action
    };
    PaneController::new(ui, state, bridge).apply_item_view_controller_action(request.slot, action);
    true
}

fn select_all_visible(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let slot = { state.borrow().panes.focused_slot() };
    let selected_paths = {
        let state = state.borrow();
        filtered_entry_paths_for_slot(&state, slot)
    };
    {
        let mut state = state.borrow_mut();
        if let Some(pane) = state.panes.pane_mut_for_slot(slot) {
            pane.selection.paths = selected_paths.clone();
            pane.selection.anchor = selected_paths.last().cloned();
        }
    }
    update_selection_ui_for_slot(ui, state, slot, &selected_paths);
}

fn select_rect_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    rect: SelectionRect,
    toggle: bool,
) {
    let selected_paths = {
        let mut state = state.borrow_mut();
        let selected = selection_rect_paths_filtered_for_slot(&state, slot, rect);
        let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
            return;
        };
        if toggle {
            append_unique_paths(&mut pane.selection.paths, selected);
        } else {
            pane.selection.paths = selected;
        }
        pane.selection.anchor = pane.selection.paths.last().cloned();
        pane.selection.paths.clone()
    };
    update_selection_ui_for_slot(ui, state, slot, &selected_paths);
}

fn press_item_view_blank_for_slot(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    x: f32,
    y: f32,
    toggle: bool,
) {
    let mut state = state.borrow_mut();
    press_blank_for_slot(&mut state, slot, x, y, toggle);
}

fn move_item_view_blank_for_slot(state: &Rc<RefCell<AppState>>, slot: i32, x: f32, y: f32) -> bool {
    let mut state = state.borrow_mut();
    move_blank_for_slot(&mut state, slot, x, y)
}

fn release_item_view_blank_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    x: f32,
    y: f32,
) {
    let action = {
        let mut state = state.borrow_mut();
        let Some(action) = release_blank_for_slot(&mut state, slot, x, y) else {
            return;
        };
        action
    };

    PaneController::new(ui, state, bridge).apply_item_view_controller_action(slot, action);
}

fn cancel_item_view_blank_for_slot(state: &Rc<RefCell<AppState>>, slot: i32) {
    let mut state = state.borrow_mut();
    cancel_blank_for_slot(&mut state, slot);
}

fn clear_selection_for_slot(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    let mut state_mut = state.borrow_mut();
    if let Some(pane) = state_mut.panes.pane_mut_for_slot(slot) {
        pane.selection.clear();
    }
    drop(state_mut);
    update_selection_ui_for_slot(ui, state, slot, &[]);
}

fn clear_selection(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let slot = { state.borrow().panes.focused_slot() };
    clear_selection_for_slot(ui, state, slot);
}

fn flush_thumbnail_results(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pending: &ThumbnailPendingQueue,
    icon_size_update_pending: bool,
) {
    let timer = PerfTimer::start();
    let (refresh_pane_ids, batch_size, applied_count) = {
        let mut state = state.borrow_mut();
        let mut refresh_pane_ids = Vec::new();
        let mut pending = pending.borrow_mut();
        let mut batch_size = 0;
        let mut applied_count = 0;
        while let Some((pane_id, generation, load)) = pending.pop_front() {
            batch_size += 1;
            let path_text = load.path.display().to_string();
            if apply_thumbnail_load_to_state_for_pane(
                &mut state, pane_id, generation, &path_text, load,
            ) {
                applied_count += 1;
                if !refresh_pane_ids.contains(&pane_id) {
                    refresh_pane_ids.push(pane_id);
                }
            }
        }
        (refresh_pane_ids, batch_size, applied_count)
    };
    let refresh_count = refresh_pane_ids.len();
    let mut visible_syncs = 0;
    if !icon_size_update_pending {
        for pane_id in refresh_pane_ids {
            let slot = { state.borrow().panes.slot_for_id(pane_id) };
            if let Some(slot) = slot {
                visible_syncs += 1;
                sync_virtual_entries_for_slot(ui, state, bridge, slot, false);
            }
        }
    }
    item_view_perf::log(format_args!(
        "thumbnail_flush batch={} applied={} affected_panes={} visible_syncs={} gated_by_icon_size={} flush_ms={:.3}",
        batch_size,
        applied_count,
        refresh_count,
        visible_syncs,
        icon_size_update_pending,
        timer.elapsed_ms()
    ));
}

fn refresh_visible_pane_tile_frame_rasters(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let slots = ui.get_pane_slots();
    if slots.row_count() == 0 {
        sync_pane_view_ui(ui, state, 0);
        return;
    }

    for row in 0..slots.row_count() {
        if let Some(pane) = slots.row_data(row) {
            sync_pane_view_ui(ui, state, pane.slot);
        }
    }
}

fn selection_status_text(selected_paths: &[String]) -> SharedString {
    match selected_paths {
        [] => SharedString::new(),
        [path] => format!("1 item selected: {path}").into(),
        paths => format!("{} items selected", paths.len()).into(),
    }
}

fn sync_visible_pane_layouts(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    sync_visible_pane_layouts_with_thumbnail_scheduling(ui, state, bridge, true);
}

fn sync_visible_pane_layouts_with_thumbnail_scheduling(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    schedule_thumbnails: bool,
) {
    let slots = ui.get_pane_slots();
    if slots.row_count() == 0 {
        return;
    }

    for row in 0..slots.row_count() {
        if let Some(pane) = slots.row_data(row) {
            sync_pane_layout_for_slot_with_thumbnail_scheduling(
                ui,
                state,
                bridge,
                pane.slot,
                schedule_thumbnails,
            );
        }
    }
}

fn apply_visible_pane_zoom_style_options(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let slots = ui.get_pane_slots();
    if slots.row_count() == 0 {
        return;
    }

    for row in 0..slots.row_count() {
        if let Some(pane) = slots.row_data(row) {
            apply_pane_zoom_style_option_for_slot(ui, state, bridge, pane.slot);
        }
    }
}

fn apply_pane_zoom_style_option_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    item_view_perf::log(format_args!(
        "zoom slot={} level={} schedule_thumbnails=false immediate=true",
        slot,
        ui.get_icon_zoom_level()
    ));
    sync_virtual_entries_for_slot_with_count(ui, state, bridge, slot, false, None, true, true);
}

fn update_icon_size_for_visible_panes(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    refresh_icon_size_models_for_visible_panes(ui, state, bridge);
}

fn refresh_icon_size_models_for_visible_panes(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    let slots = ui.get_pane_slots();
    if slots.row_count() == 0 {
        refresh_icon_size_model_for_slot(ui, state, bridge, 0);
        return;
    }

    for row in 0..slots.row_count() {
        if let Some(pane) = slots.row_data(row) {
            refresh_icon_size_model_for_slot(ui, state, bridge, pane.slot);
        }
    }
}

fn refresh_icon_size_model_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    sync_virtual_entries_for_slot_with_count_and_cache_policy(
        ui, state, bridge, slot, true, None, false, true, false, true, true,
    );
}

fn sync_pane_viewport_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    sync_pane_viewport_for_slot_with_thumbnail_scheduling(ui, state, bridge, slot, true);
}

fn sync_pane_viewport_for_slot_with_thumbnail_scheduling(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
) {
    sync_virtual_entries_for_slot_with_count(
        ui,
        state,
        bridge,
        slot,
        schedule_thumbnails,
        None,
        true,
        false,
    );
}

fn sync_pane_layout_for_slot_with_thumbnail_scheduling(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
) {
    sync_virtual_entries_for_slot_with_count(
        ui,
        state,
        bridge,
        slot,
        schedule_thumbnails,
        None,
        true,
        true,
    );
}

fn sync_pane_view_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    sync_virtual_entries_for_slot(ui, state, bridge, slot, true);
}

fn focus_pane_slot(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32) {
    let previous_slot = { state.borrow().panes.focused_slot() };
    let focused = {
        let mut state = state.borrow_mut();
        state.panes.focus_slot(slot)
    };
    if focused && previous_slot != slot {
        sync_focus_navigation_ui(ui, state, previous_slot);
    }
}

fn focus_current_ui_pane_slot(ui: &AppWindow, state: &Rc<RefCell<AppState>>) -> i32 {
    let requested_slot = if ui.get_split_view_open() {
        ui.get_focused_pane()
    } else {
        0
    };
    focus_pane_slot(ui, state, requested_slot);
    state.borrow().panes.focused_slot()
}

fn reset_pane_path_input_for_slot(_ui: &AppWindow, _slot: i32) {
    // path input sync happens via pane_path_text_changed callback now
}

fn prepare_pane_transfer_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    source: &str,
    x: f32,
    y: f32,
) -> bool {
    prepare_pane_transfer(ui, state, slot, source, x, y)
}

fn pane_drop_target_path_for_slot(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> Option<String> {
    pane_drop_target_path(ui, state, slot, x, y, source)
}

fn pane_drop_target_slice_index_for_slot(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> i32 {
    let Some(target_path) = pane_drop_target_path_for_slot(ui, state, slot, x, y, source) else {
        return -1;
    };
    let Some(global_index) = item_index_at_pane_point(ui, state, slot, x, y) else {
        return -1;
    };
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return -1;
    };
    if global_index < pane.view.virtual_start_index {
        return -1;
    }
    let slice_index = global_index - pane.view.virtual_start_index;
    let Some(entry) = pane.view.virtual_entries.get(slice_index) else {
        return -1;
    };
    if entry.path.as_str() != target_path {
        return -1;
    }
    slice_index as i32
}

fn set_pane_drop_target_slice_index_ui(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    slice_index: i32,
) {
    let changed = {
        let mut state = state.borrow_mut();
        state
            .panes
            .pane_mut_for_slot(slot)
            .is_some_and(|pane| pane.view.set_drop_target_slice_index(slice_index))
    };
    if changed {
        sync_pane_view_ui(ui, state, slot);
    }
}

fn pane_drop_allowed_for_slot(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
    source: &Path,
) -> bool {
    pane_drop_allowed(ui, state, slot, x, y, source)
}

fn update_selection_ui_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    selected_paths: &[String],
) {
    let selected_path = selected_paths
        .last()
        .map_or_else(SharedString::new, |path| path.as_str().into());
    let selected_count = selected_paths.len() as i32;
    let selected_status = selection_status_text(selected_paths);

    update_virtual_selection_for_slot(state, slot, selected_paths);
    sync_pane_slot_ui(ui, state, slot);

    let selected_slot_is_focused = ui.get_focused_pane() == slot;
    if selected_slot_is_focused {
        ui.set_selected_path(selected_path);
        ui.set_selected_count(selected_count);
        ui.set_selected_status(selected_status);
    }
    if pane_view_row_exists(ui, slot) {
        sync_pane_view_ui(ui, state, slot);
    }
}

fn pane_view_row_exists(ui: &AppWindow, slot: i32) -> bool {
    let current = ui.get_pane_surfaces();
    (0..current.row_count()).any(|row| {
        current
            .row_data(row)
            .is_some_and(|current| current.slot == slot)
    })
}

fn update_virtual_selection_for_slot(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    selected_paths: &[String],
) {
    let Some(_) = ({
        let mut state_ref = state.borrow_mut();
        state_ref
            .panes
            .pane_mut_for_slot(slot)
            .map(|pane| update_pane_item_view_selection_model(&mut pane.view, selected_paths))
    }) else {
        return;
    };
}

fn navigate_pane_to_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    path: PathBuf,
) {
    remember_pane_view_state(ui, state, slot);
    let same_path = {
        let mut state_ref = state.borrow_mut();
        let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) else {
            drop(state_ref);
            sync_navigation_ui(ui, state);
            set_status(ui, state, "No pane target is available");
            return;
        };

        if pane.current_dir == path {
            true
        } else {
            debug_log(&format!(
                "navigate_pane slot={slot} from={} to={} back_len_before={} forward_len_before={}",
                pane.current_dir.display(),
                path.display(),
                pane.history.back_len(),
                pane.history.forward_len()
            ));
            let previous = pane.current_dir.clone();
            let nav = pane.history.navigate_from(previous, path.clone());
            pane.current_dir = nav.target;
            false
        }
    };

    sync_navigation_ui(ui, state);
    if same_path {
        debug_log(&format!(
            "navigate_pane slot={slot} same path={} -> refresh",
            path.display()
        ));
        load_current_directory_for_slot(ui, state, bridge, slot, true);
    } else {
        load_current_directory_for_slot(ui, state, bridge, slot, false);
    }
}

fn go_pane_back_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    go_pane_history_slot(ui, state, bridge, slot, HistoryDirection::Back);
}

fn go_pane_forward_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    go_pane_history_slot(ui, state, bridge, slot, HistoryDirection::Forward);
}

fn open_path_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    path: &str,
    bridge: &AsyncBridge,
) {
    open_path_for_slot_impl(ui, state, slot, path, bridge);
}

fn navigate_to(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge, path: PathBuf) {
    navigate_pane_to_slot(ui, state, bridge, 0, path);
}

fn navigate_focused_to(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    path: PathBuf,
) {
    let slot = { state.borrow().panes.focused_slot() };
    navigate_pane_to_slot(ui, state, bridge, slot, path);
}

fn refresh_focused_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let slot = { state.borrow().panes.focused_slot() };
    load_current_directory_for_slot(ui, state, bridge, slot, true);
}

fn load_current_directory_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    preserve_view: bool,
) {
    let Some(preparation) = ({
        let mut state = state.borrow_mut();
        prepare_directory_load_for_target(&mut state, PaneTarget::Slot(slot), preserve_view)
    }) else {
        sync_navigation_ui(ui, state);
        set_status(ui, state, "No pane target is available");
        return;
    };
    load_prepared_pane_directory(ui, state, bridge, preparation, preserve_view);
}

fn go_parent(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let current_dir = {
        let state = state.borrow();
        state
            .panes
            .pane_for_target(PaneTarget::Focused)
            .unwrap_or(state.panes.focused())
            .current_dir
            .clone()
    };
    let parent = current_dir.parent().map(Path::to_path_buf);
    if let Some(parent) = parent {
        debug_log(&format!("go_parent target={}", parent.display()));
        navigate_focused_to(ui, state, bridge, parent);
    } else {
        debug_log("go_parent no parent");
    }
}

fn go_back(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let slot = { state.borrow().panes.focused_slot() };
    go_pane_back_slot(ui, state, bridge, slot);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryDirection {
    Back,
    Forward,
}

fn go_pane_history_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    direction: HistoryDirection,
) {
    remember_pane_view_state(ui, state, slot);
    {
        let mut state_ref = state.borrow_mut();
        let Some(pane) = state_ref.panes.pane_mut_for_slot(slot) else {
            drop(state_ref);
            sync_navigation_ui(ui, state);
            set_status(ui, state, "No pane target is available");
            return;
        };
        let action = match direction {
            HistoryDirection::Back => "go_back",
            HistoryDirection::Forward => "go_forward",
        };
        debug_log(&format!(
            "{action} requested slot={slot} current={} back_len={} forward_len={}",
            pane.current_dir.display(),
            pane.history.back_len(),
            pane.history.forward_len()
        ));
        let previous = pane.current_dir.clone();
        let nav = match direction {
            HistoryDirection::Back => pane.history.go_back_from(previous),
            HistoryDirection::Forward => pane.history.go_forward_from(previous),
        };
        let Some(nav) = nav else {
            debug_log(&format!("{action} ignored slot={slot}: empty stack"));
            drop(state_ref);
            sync_navigation_ui(ui, state);
            match direction {
                HistoryDirection::Back => set_status(ui, state, "No previous location"),
                HistoryDirection::Forward => set_status(ui, state, "No next location"),
            }
            return;
        };
        pane.current_dir = nav.target.clone();

        debug_log(&format!(
            "{action} accepted slot={slot} target={} previous_current={}",
            nav.target.display(),
            nav.previous.display()
        ));
    }
    sync_navigation_ui(ui, state);
    load_current_directory_for_slot(ui, state, bridge, slot, false);
}

fn go_forward(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let slot = { state.borrow().panes.focused_slot() };
    go_pane_forward_slot(ui, state, bridge, slot);
}

fn open_path(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: &str, bridge: &AsyncBridge) {
    let slot = { state.borrow().panes.focused_slot() };
    open_path_for_slot_impl(ui, state, slot, path, bridge);
}

fn open_path_for_slot_impl(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    path: &str,
    bridge: &AsyncBridge,
) {
    let (path, is_known_dir) = {
        let state_ref = state.borrow();
        let Some(pane) = state_ref.panes.pane_for_slot(slot) else {
            set_status(ui, state, "No pane target is available");
            return;
        };
        let entry = pane.entries.iter().find(|entry| entry.model_path() == path);
        let path = entry
            .map(|entry| Cow::Owned(entry.model_path_string()))
            .unwrap_or_else(|| Cow::Borrowed(path));
        (
            PathBuf::from(path.as_ref()),
            entry.map(ItemViewModelEntry::model_is_dir),
        )
    };

    let is_dir = is_known_dir.unwrap_or_else(|| path.is_dir());
    if is_dir {
        navigate_pane_to_slot(ui, state, bridge, slot, path);
        return;
    }

    if ui.get_chooser_mode() {
        let metadata = chooser_output_metadata(&state.borrow());
        output_chooser_paths_and_exit(vec![path], metadata);
    }

    open_file_for_target_async(ui, state, bridge, PaneTarget::Slot(slot), path);
}

fn chooser_accept(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    save_name: &str,
    save_files: &[String],
) {
    if !ui.get_chooser_mode() {
        return;
    }

    let state_ref = state.borrow();
    if ui.get_chooser_save_mode() {
        let target_dir = selected_directory_or_current(&state_ref);
        if !save_files.is_empty() {
            let paths = save_files
                .iter()
                .filter_map(|name| safe_child_path(&target_dir, name))
                .collect::<Vec<_>>();
            if paths.len() == save_files.len() {
                output_chooser_paths_and_exit(paths, chooser_output_metadata(&state_ref));
            } else {
                set_status(ui, state, "Cannot save: one or more file names are invalid");
            }
            return;
        }

        let Some(path) = safe_child_path(&target_dir, save_name) else {
            set_status(ui, state, "Cannot save: file name is invalid");
            return;
        };
        output_chooser_paths_and_exit(vec![path], chooser_output_metadata(&state_ref));
    } else if ui.get_chooser_select_directories() {
        output_chooser_paths_and_exit(
            vec![selected_directory_or_current(&state_ref)],
            chooser_output_metadata(&state_ref),
        );
    } else if !state_ref.panes.focused().selection.paths.is_empty() {
        let selected_files = state_ref
            .panes
            .focused()
            .selection
            .paths
            .iter()
            .map(PathBuf::from)
            .filter(|path| !path.is_dir())
            .collect::<Vec<_>>();
        if selected_files.is_empty() {
            set_status(
                ui,
                state,
                "Choose a file, or double-click folders to enter them",
            );
        } else if ui.get_chooser_multiple() {
            output_chooser_paths_and_exit(selected_files, chooser_output_metadata(&state_ref));
        } else {
            output_chooser_paths_and_exit(
                vec![selected_files[0].clone()],
                chooser_output_metadata(&state_ref),
            );
        }
    } else {
        set_status(ui, state, "Select a file to continue");
    }
}

fn sync_chooser_filter_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state_ref = state.borrow();
    ui.set_chooser_filter_count(state_ref.chooser_filters.len() as i32);
    ui.set_chooser_filter_index(state_ref.chooser_filter_index as i32);
    ui.set_chooser_filter_label(
        state_ref
            .chooser_filters
            .get(state_ref.chooser_filter_index)
            .map(|filter| filter.label.as_str())
            .unwrap_or("")
            .into(),
    );
    ui.set_chooser_filter_options(ModelRc::new(Rc::new(VecModel::from(
        state_ref
            .chooser_filters
            .iter()
            .map(|filter| ChooserChoiceOption {
                label: filter.label.as_str().into(),
            })
            .collect::<Vec<_>>(),
    ))));
    sync_pane_slots_ui(ui, state);
}

fn sync_chooser_choices_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let choices = state
        .borrow()
        .chooser_choices
        .iter()
        .map(|choice| {
            let selected_label = choice
                .items
                .get(choice.selected_index)
                .map(|item| item.label.as_str())
                .unwrap_or("");
            ChooserChoice {
                label: choice.label.as_str().into(),
                selected_label: selected_label.into(),
                selected_index: choice.selected_index as i32,
                options: ModelRc::new(Rc::new(VecModel::from(
                    choice
                        .items
                        .iter()
                        .map(|item| ChooserChoiceOption {
                            label: item.label.as_str().into(),
                        })
                        .collect::<Vec<_>>(),
                ))),
            }
        })
        .collect::<Vec<_>>();
    ui.set_chooser_choices(ModelRc::new(Rc::new(VecModel::from(choices))));
    sync_pane_slots_ui(ui, state);
}

fn select_chooser_filter(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    filter_index: i32,
) {
    {
        let mut state_ref = state.borrow_mut();
        let Ok(filter_index) = usize::try_from(filter_index) else {
            return;
        };
        if filter_index >= state_ref.chooser_filters.len() {
            return;
        }
        state_ref.chooser_filter_index = filter_index;
    }
    sync_chooser_filter_ui(ui, state);
    let slot = { state.borrow().panes.focused_slot() };
    apply_filter_for_slot(ui, state, bridge, slot, true);
}

fn select_chooser_choice(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    choice_index: i32,
    option_index: i32,
) {
    {
        let mut state_ref = state.borrow_mut();
        if !set_chooser_choice_index(&mut state_ref, choice_index, option_index) {
            return;
        }
    }
    sync_chooser_choices_ui(ui, state);
}

fn output_chooser_paths_and_exit(paths: Vec<PathBuf>, metadata: ChooserOutputMetadata) -> ! {
    if let Some(filter_index) = metadata.filter_index {
        println!("FIKA_CHOOSER_FILTER\t{filter_index}");
    }
    for (id, selected) in metadata.choices {
        println!("FIKA_CHOOSER_CHOICE\t{id}\t{selected}");
    }
    for path in paths {
        match path.canonicalize() {
            Ok(path) => println!("{}", path.display()),
            Err(_) => println!("{}", path.display()),
        }
    }
    std::process::exit(0);
}

fn set_pane_status(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32, message: &str) {
    let Some(target_is_focused) = ({
        let Ok(mut state) = state.try_borrow_mut() else {
            ui.set_status(SharedString::from(message));
            return;
        };
        set_pane_status_state(&mut state, slot, message)
    }) else {
        return;
    };

    if target_is_focused {
        ui.set_status(SharedString::from(message));
    }
    sync_pane_slot_ui(ui, state, slot);
}

fn set_pane_status_state(state: &mut AppState, slot: i32, message: &str) -> Option<bool> {
    let focused = state.panes.focused_slot() == slot;
    let pane = state.panes.pane_mut_for_slot(slot)?;
    pane.status = message.to_string();
    Some(focused)
}

fn set_directory_status_from_entries(ui: &AppWindow, state: &Rc<RefCell<AppState>>, pane_id: u64) {
    let status = {
        let state = state.borrow();
        state
            .panes
            .pane_for_target(PaneTarget::Id(pane_id))
            .and_then(|pane| {
                state
                    .panes
                    .slot_for_id(pane_id)
                    .map(|slot| (slot, directory_status_text(&pane.entry_summary)))
            })
    };
    if let Some((slot, status)) = status {
        set_pane_status(ui, state, slot, &status);
    }
}

fn set_status_for_panes(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    pane_ids: &[u64],
    message: &str,
) {
    let target_slots = {
        let Ok(state) = state.try_borrow() else {
            ui.set_status(SharedString::from(message));
            return;
        };
        pane_status_target_slots(&state, pane_ids)
    };

    if target_slots.is_empty() {
        set_status(ui, state, message);
        return;
    }
    for slot in target_slots {
        set_pane_status(ui, state, slot, message);
    }
}

fn pane_status_target_slots(state: &AppState, pane_ids: &[u64]) -> Vec<i32> {
    if pane_ids.is_empty() {
        return Vec::new();
    }

    state
        .panes
        .iter()
        .filter_map(|(slot, pane)| pane_ids.contains(&pane.id).then_some(slot))
        .collect()
}

fn set_status(ui: &AppWindow, state: &Rc<RefCell<AppState>>, message: &str) {
    let slot = {
        let Ok(mut state) = state.try_borrow_mut() else {
            ui.set_status(SharedString::from(message));
            return;
        };
        set_focused_status_state(&mut state, message)
    };

    ui.set_status(SharedString::from(message));
    sync_pane_slot_ui(ui, state, slot);
}

fn set_focused_status_state(state: &mut AppState, message: &str) -> i32 {
    let slot = state.panes.focused_slot();
    if let Some(pane) = state.panes.pane_mut_for_slot(slot) {
        pane.status = message.to_string();
    }
    slot
}

fn debug_log(message: &str) {
    static DEBUG_NAV: OnceLock<bool> = OnceLock::new();
    if *DEBUG_NAV.get_or_init(|| {
        env::var("FIKA_DEBUG_NAV").is_ok_and(|value| {
            !matches!(value.as_str(), "" | "0" | "false" | "FALSE" | "off" | "OFF")
        })
    }) {
        eprintln!("[fika nav] {message}");
    }
}

fn dnd_log_places_event(trace: PlacesDndTrace<'_>) {
    if !dnd_debug_enabled() {
        return;
    }

    eprintln!("{}", dnd_places_event_message(&trace));
}

fn dnd_log_main_event(trace: MainDndTrace<'_>) {
    if !dnd_debug_enabled() {
        return;
    }

    eprintln!("{}", dnd_main_event_message(&trace));
}

fn dnd_debug_enabled() -> bool {
    static DEBUG_DND: OnceLock<bool> = OnceLock::new();
    *DEBUG_DND.get_or_init(dnd_debug_enabled_from_env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::geometry::{
        CompactItemViewLayout, ItemViewLayoutEngine, compact_item_view_layout, place_drop_geometry,
    };
    use crate::app::operation_controller::transfer_target_rejection;
    use crate::app::selection::{
        filtered_entries_range, filtered_entry_at, filtered_entry_paths, filtered_entry_summary,
        selection_range_paths, selection_range_paths_filtered, selection_rect_paths,
        selection_rect_paths_filtered,
    };

    fn compact_test_layout(
        viewport_width: f32,
        entry_count: usize,
        rows_per_column: usize,
        cell_width: f32,
        row_height: f32,
        padding: f32,
    ) -> CompactItemViewLayout {
        let names = (0..entry_count)
            .map(|index| format!("item-{index}"))
            .collect::<Vec<_>>();
        compact_item_view_layout(
            viewport_width,
            names.iter().map(String::as_str),
            rows_per_column,
            cell_width,
            row_height,
            padding,
            0.0,
            1.0,
            0.0,
            1.0,
        )
    }

    fn selection_test_layout(names: &[&str]) -> CompactItemViewLayout {
        compact_item_view_layout(
            300.0,
            names.iter().copied(),
            2,
            100.0,
            100.0,
            10.0,
            0.0,
            1.0,
            0.0,
            1.0,
        )
    }

    fn selection_test_engine(names: &[&str]) -> Arc<ItemViewLayoutEngine> {
        Arc::new(ItemViewLayoutEngine::from(selection_test_layout(names)))
    }

    #[test]
    fn cached_virtual_viewport_rejects_stale_empty_layout_after_directory_switch() {
        let stale_empty_layout = compact_item_view_layout(
            480.0,
            std::iter::empty::<&str>(),
            4,
            100.0,
            90.0,
            10.0,
            0.0,
            1.0,
            0.0,
            1.0,
        );
        let main_layout = MainItemViewLayout {
            viewport_x: 0.0,
            viewport_width: stale_empty_layout.viewport_width,
            rows_per_column: stale_empty_layout.rows_per_column,
            cell_width: stale_empty_layout.cell_width,
            row_height: stale_empty_layout.row_height,
            padding: stale_empty_layout.padding,
            item_padding: 0.0,
            media_width: 1.0,
            media_text_gap: 0.0,
            title_font_size: 1.0,
        };
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        pane.view.virtual_view.layout = Some(Arc::new(stale_empty_layout.into()));
        pane.view.virtual_view.range = 0..0;
        pane.view.virtual_view.thumbnail_size_px = 64;

        let entries = PreparedDirectoryEntries::new(vec![test_entry("new", "/tmp/new")]);
        pane.set_entries_with_summary(entries.entries.clone(), entries.summary.clone());

        assert_eq!(
            cached_virtual_viewport_sync(&mut pane, &main_layout, 0.0, 64, true, None, &[]),
            None,
            "directory switches must not reuse an empty virtual layout for a newly loaded non-empty directory"
        );
    }

    #[test]
    fn drops_selection_paths_that_are_no_longer_visible() {
        let selected = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/missing".to_string(),
        ];
        let visible = vec!["/tmp/two".to_string(), "/tmp/one".to_string()];

        assert_eq!(
            retained_visible_paths(&selected, &visible),
            vec!["/tmp/one".to_string(), "/tmp/two".to_string()]
        );
    }

    #[test]
    fn filtered_entry_paths_returns_only_visible_matches() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("Alpha.txt", "/tmp/Alpha.txt"),
            test_entry("Beta.txt", "/tmp/Beta.txt"),
            test_entry("notes.md", "/tmp/project-notes.md"),
        ]);
        state.panes.focused_mut().search.query = "project".to_string();

        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/project-notes.md".to_string()]
        );
    }

    #[test]
    fn filtered_entries_apply_kind_modified_and_size_filters() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut folder = test_entry("Images", "/tmp/Images");
        folder.is_dir = true;
        folder.kind = "Folder".into();
        folder.size = "-".into();
        folder.size_bytes = 0.0;

        let mut image = test_entry("photo.png", "/tmp/photo.png");
        image.size_bytes = 512_000.0;
        image.modified_age_days = 0;

        let mut archive = test_entry("archive.zip", "/tmp/archive.zip");
        archive.size_bytes = 150_000_000.0;
        archive.modified_age_days = 20;

        state
            .panes
            .focused_mut()
            .set_file_entries(vec![folder, image, archive]);

        state.panes.focused_mut().search.kind_filter = 1;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Images".to_string()]
        );

        state.panes.focused_mut().search.kind_filter = 3;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/photo.png".to_string()]
        );

        state.panes.focused_mut().search.kind_filter = 0;
        state.panes.focused_mut().search.size_filter = 3;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/archive.zip".to_string()]
        );

        state.panes.focused_mut().search.size_filter = 0;
        state.panes.focused_mut().search.modified_filter = 2;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Images".to_string(), "/tmp/photo.png".to_string()]
        );
    }

    #[test]
    fn filtered_entries_range_clones_only_requested_filtered_window() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(
            (0..8)
                .map(|index| {
                    test_entry(&format!("item-{index}.txt"), &format!("/tmp/item-{index}"))
                })
                .collect(),
        );
        state.panes.focused_mut().search.query = "item".to_string();

        assert_eq!(filtered_entry_count_for_slot(&state, 0), 8);
        assert_eq!(
            filtered_entries_range(&state, 2..5)
                .into_iter()
                .map(|entry| entry.name.to_string())
                .collect::<Vec<_>>(),
            vec![
                "item-2.txt".to_string(),
                "item-3.txt".to_string(),
                "item-4.txt".to_string()
            ]
        );
    }

    #[test]
    fn filtered_entry_at_clones_only_requested_visible_item() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
        ]);
        state.panes.focused_mut().search.query = ".txt".to_string();

        assert_eq!(
            filtered_entry_at(&state, 1)
                .map(|entry| entry.path.to_string())
                .as_deref(),
            Some("/tmp/beta")
        );
        assert!(filtered_entry_at(&state, 2).is_none());
    }

    #[test]
    fn filtered_entry_summary_counts_without_cloning_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut folder = test_entry("item-folder", "/tmp/item-folder");
        folder.is_dir = true;
        state.panes.focused_mut().set_file_entries(vec![
            folder,
            test_entry("item-file.txt", "/tmp/item-file.txt"),
            test_entry("hidden.log", "/tmp/hidden.log"),
        ]);
        state.panes.focused_mut().search.query = "item".to_string();

        let summary = filtered_entry_summary(&state, true);

        assert_eq!(summary.count, 2);
        assert_eq!(summary.folders, 1);
        assert_eq!(summary.files, 1);
        assert_eq!(
            summary.visible_paths,
            Some(vec![
                "/tmp/item-folder".to_string(),
                "/tmp/item-file.txt".to_string()
            ])
        );
    }

    #[test]
    fn visible_entry_index_uses_identity_fast_path_without_filters() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha", "/tmp/alpha"),
            test_entry("beta", "/tmp/beta"),
        ]);

        let summary = rebuild_visible_entry_index_for_slot(&mut state, 0, true);

        assert_eq!(summary.count, 2);
        assert!(state.panes.focused().search.visible_entry_indices.is_none());
        assert_eq!(
            filtered_entries_range(&state, 1..2)
                .into_iter()
                .map(|entry| entry.path.to_string())
                .collect::<Vec<_>>(),
            vec!["/tmp/beta".to_string()]
        );
    }

    #[test]
    fn visible_entry_index_drives_virtual_range_without_rescanning_filters() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ]);
        state.panes.focused_mut().search.query = ".txt".to_string();

        let summary = rebuild_visible_entry_index_for_slot(&mut state, 0, false);

        assert_eq!(summary.count, 3);
        assert_eq!(
            state
                .panes
                .focused()
                .search
                .visible_entry_indices
                .as_deref(),
            Some(&[0, 2, 3][..])
        );
        assert_eq!(filtered_entry_count_for_slot(&state, 0), 3);
        assert_eq!(
            filtered_entry_at(&state, 1)
                .map(|entry| entry.path.to_string())
                .as_deref(),
            Some("/tmp/beta")
        );
        assert_eq!(
            filtered_entries_range(&state, 1..3)
                .into_iter()
                .map(|entry| entry.path.to_string())
                .collect::<Vec<_>>(),
            vec!["/tmp/beta".to_string(), "/tmp/gamma".to_string()]
        );
    }

    #[test]
    fn pending_visible_entry_index_keeps_committed_view_without_rescanning_query() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
        ]);
        state.panes.focused_mut().search.query = ".txt".to_string();
        state.panes.focused_mut().search.index_pending = true;

        assert_eq!(filtered_entry_count_for_slot(&state, 0), 3);
        assert_eq!(
            filtered_entry_paths(&state),
            vec![
                "/tmp/alpha".to_string(),
                "/tmp/skip".to_string(),
                "/tmp/beta".to_string()
            ]
        );
        assert_eq!(
            filtered_entries_range(&state, 1..3)
                .into_iter()
                .map(|entry| entry.path.to_string())
                .collect::<Vec<_>>(),
            vec!["/tmp/skip".to_string(), "/tmp/beta".to_string()]
        );
        assert_eq!(
            filtered_entry_at(&state, 1)
                .map(|entry| entry.path.to_string())
                .as_deref(),
            Some("/tmp/skip")
        );
    }

    #[test]
    fn visible_location_group_flag_tracks_only_visible_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut hidden = test_entry("hidden.log", "/tmp/docs/hidden.log");
        hidden.location = "docs".into();
        let visible = test_entry("visible.txt", "/tmp/visible.txt");
        state
            .panes
            .focused_mut()
            .set_file_entries(vec![hidden, visible]);
        state.panes.focused_mut().search.query = ".txt".to_string();

        let summary = rebuild_visible_entry_index_for_slot(&mut state, 0, false);

        assert_eq!(summary.count, 1);
        assert!(!summary.has_locations);
        assert!(!state.panes.focused().search.visible_entries_have_locations);
        assert_eq!(
            filtered_entries_range(&state, 0..1)
                .into_iter()
                .map(|entry| (entry.group.to_string(), entry.path.to_string()))
                .collect::<Vec<_>>(),
            vec![("".to_string(), "/tmp/visible.txt".to_string())]
        );
    }

    #[test]
    fn recursive_search_groups_are_recomputed_after_filters_hide_first_match() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut old_file = test_entry("old.txt", "/tmp/docs/old.txt");
        old_file.location = "docs".into();
        old_file.modified_age_days = 20;
        let mut visible_file = test_entry("visible.txt", "/tmp/docs/visible.txt");
        visible_file.location = "docs".into();
        visible_file.modified_age_days = 0;
        state
            .panes
            .focused_mut()
            .set_file_entries(vec![old_file, visible_file]);
        state.panes.focused_mut().search.modified_filter = 1;

        let summary = rebuild_visible_entry_index_for_slot(&mut state, 0, false);

        assert_eq!(summary.count, 1);
        assert_eq!(
            filtered_entries_range(&state, 0..1)
                .into_iter()
                .map(|entry| (entry.group.to_string(), entry.path.to_string()))
                .collect::<Vec<_>>(),
            vec![("docs".to_string(), "/tmp/docs/visible.txt".to_string())]
        );
    }

    #[test]
    fn recursive_search_groups_are_not_repeated_inside_same_visible_location() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut first = test_entry("first.txt", "/tmp/docs/first.txt");
        first.location = "docs".into();
        let mut second = test_entry("second.txt", "/tmp/docs/second.txt");
        second.location = "docs".into();
        let mut third = test_entry("third.txt", "/tmp/docs/third.txt");
        third.location = "docs".into();
        state
            .panes
            .focused_mut()
            .set_file_entries(vec![first, second, third]);
        rebuild_visible_entry_index_for_slot(&mut state, 0, false);

        assert_eq!(
            filtered_entries_range(&state, 1..3)
                .into_iter()
                .map(|entry| (entry.group.to_string(), entry.path.to_string()))
                .collect::<Vec<_>>(),
            vec![
                ("".to_string(), "/tmp/docs/second.txt".to_string()),
                ("".to_string(), "/tmp/docs/third.txt".to_string())
            ]
        );
    }

    #[test]
    fn chooser_filter_specs_filter_files_but_keep_folders_visible() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut folder = test_entry("Documents", "/tmp/Documents");
        folder.is_dir = true;
        folder.kind = "Folder".into();
        state.panes.focused_mut().set_file_entries(vec![
            folder,
            test_entry("photo.PNG", "/tmp/photo.PNG"),
            test_entry("notes.txt", "/tmp/notes.txt"),
        ]);
        state.chooser_filters = vec![parse_chooser_filter_spec("Images\t*.png;*.jpg").unwrap()];

        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Documents".to_string(), "/tmp/photo.PNG".to_string()]
        );
    }

    #[test]
    fn chooser_parent_window_log_reports_native_transient_status() {
        assert_eq!(
            chooser_parent_window_log_message(Some("wayland:1_42")),
            "[fika chooser] parent_window received=true handle=wayland:1_42 parent_binding=metadata-only parent_binding_reason=slint-parent-token-binding-unavailable native_transient=false"
        );
        assert_eq!(
            chooser_parent_window_log_message(None),
            "[fika chooser] parent_window received=false handle= parent_binding=none parent_binding_reason=no-parent-window native_transient=false"
        );
    }

    #[test]
    fn compact_item_view_layout_keeps_visible_columns_with_overscan() {
        let compact_layout = compact_test_layout(250.0, 100, 4, 100.0, 100.0, 10.0);
        let at_start = compact_layout.virtual_plan(0.0, 1);
        assert_eq!(at_start.range, 0..16);
        assert_eq!(at_start.visible_range, 0..12);

        let middle = compact_layout.virtual_plan(350.0, 1);
        assert_eq!(middle.range, 8..28);
        assert_eq!(middle.visible_range, 12..24);

        let clamped = compact_test_layout(250.0, 10, 4, 100.0, 100.0, 10.0).virtual_plan(800.0, 1);
        assert_eq!(clamped.range, 0..10);
        assert_eq!(clamped.visible_range, 0..10);
    }

    #[test]
    fn selection_range_uses_visible_order() {
        let visible = vec![
            "/tmp/a".to_string(),
            "/tmp/b".to_string(),
            "/tmp/c".to_string(),
            "/tmp/d".to_string(),
        ];

        assert_eq!(
            selection_range_paths(&visible, "/tmp/d", "/tmp/b"),
            vec![
                "/tmp/b".to_string(),
                "/tmp/c".to_string(),
                "/tmp/d".to_string()
            ]
        );
    }

    #[test]
    fn filtered_selection_range_scans_only_visible_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ]);
        state.panes.focused_mut().search.query = ".txt".to_string();

        assert_eq!(
            selection_range_paths_filtered(&state, "/tmp/gamma", "/tmp/alpha"),
            vec![
                "/tmp/alpha".to_string(),
                "/tmp/beta".to_string(),
                "/tmp/gamma".to_string()
            ]
        );
        assert_eq!(
            selection_range_paths_filtered(&state, "/tmp/missing", "/tmp/beta"),
            vec!["/tmp/beta".to_string()]
        );
    }

    #[test]
    fn append_unique_paths_preserves_existing_selection_order() {
        let mut selected = vec!["/tmp/a".to_string(), "/tmp/c".to_string()];
        append_unique_paths(
            &mut selected,
            vec!["/tmp/b".to_string(), "/tmp/c".to_string()],
        );

        assert_eq!(
            selected,
            vec![
                "/tmp/a".to_string(),
                "/tmp/c".to_string(),
                "/tmp/b".to_string()
            ]
        );
    }

    #[test]
    fn selection_rect_uses_column_first_geometry() {
        let entries = vec![
            test_entry("a", "/tmp/a"),
            test_entry("b", "/tmp/b"),
            test_entry("c", "/tmp/c"),
            test_entry("d", "/tmp/d"),
        ];
        let selected = selection_rect_paths(
            &entries,
            SelectionRect {
                x1: 0.0,
                y1: 0.0,
                x2: 109.0,
                y2: 205.0,
                layout: selection_test_engine(&["a", "b", "c", "d"]),
            },
        );

        assert_eq!(selected, vec!["/tmp/a".to_string(), "/tmp/b".to_string()]);
    }

    #[test]
    fn filtered_selection_rect_scans_visible_order_without_cloning_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ]);
        state.panes.focused_mut().search.query = ".txt".to_string();

        let selected = selection_rect_paths_filtered(
            &state,
            SelectionRect {
                x1: 0.0,
                y1: 0.0,
                x2: 109.0,
                y2: 205.0,
                layout: selection_test_engine(&["alpha.txt", "beta.txt", "gamma.txt"]),
            },
        );

        assert_eq!(
            selected,
            vec!["/tmp/alpha".to_string(), "/tmp/beta".to_string()]
        );
    }

    #[test]
    fn filtered_selection_rect_limits_scan_to_intersecting_columns() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(
            (0..20)
                .map(|index| test_entry(&format!("entry-{index}"), &format!("/tmp/{index}")))
                .collect(),
        );

        let selected = selection_rect_paths_filtered(
            &state,
            SelectionRect {
                x1: 244.0,
                y1: 0.0,
                x2: 325.0,
                y2: 205.0,
                layout: selection_test_engine(
                    &(0..20)
                        .map(|index| format!("entry-{index}"))
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                ),
            },
        );

        assert_eq!(selected, vec!["/tmp/4".to_string(), "/tmp/5".to_string()]);
    }

    #[test]
    fn place_drop_geometry_slot_clamps_to_places_range() {
        assert_eq!(place_drop_geometry(90.0, 3, 108.0, 38.0).slot, 0);
        assert_eq!(place_drop_geometry(146.0, 3, 108.0, 38.0).slot, 1);
        assert_eq!(place_drop_geometry(500.0, 3, 108.0, 38.0).slot, 3);
        assert_eq!(place_drop_geometry(222.0, 4, 190.0, 38.0).slot, 1);
    }

    #[test]
    fn rejects_transfer_targets_that_are_self_or_descendant() {
        assert_eq!(
            transfer_target_rejection(Path::new("/tmp/project"), Path::new("/tmp/project")),
            Some("Cannot drop an item onto itself")
        );
        assert_eq!(
            transfer_target_rejection(Path::new("/tmp/project"), Path::new("/tmp/project/src")),
            Some("Cannot drop a folder into itself")
        );
        assert_eq!(
            transfer_target_rejection(Path::new("/tmp/project"), Path::new("/tmp/project-copy")),
            None
        );
    }

    #[test]
    fn device_action_pending_guard_blocks_duplicate_device_actions() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/tmp"),
            Vec::new(),
        )));

        assert!(register_pending_device_action(&state, "/dev/sdb1", "mount"));
        assert!(!register_pending_device_action(
            &state,
            "/dev/sdb1",
            "unmount"
        ));
        assert!(register_pending_device_action(&state, "/dev/sdc1", "mount"));

        clear_pending_device_action(&state, "/dev/sdb1", "mount");

        assert!(register_pending_device_action(
            &state,
            "/dev/sdb1",
            "unmount"
        ));
    }

    #[test]
    fn successful_unmount_moves_current_mount_path_home_and_prunes_history() {
        let mount_path = PathBuf::from("/run/media/yk/USB");
        let state = Rc::new(RefCell::new(AppState::new(
            mount_path.join("project"),
            Vec::new(),
        )));
        {
            let mut state = state.borrow_mut();
            state.panes.focused_mut().history = PaneHistory::from_stacks(
                vec![PathBuf::from("/tmp"), mount_path.join("old")],
                vec![
                    mount_path.join("future"),
                    PathBuf::from("/run/media/yk/USB-sibling"),
                ],
            );
            assert!(state.panes.open_pane(mount_path.join("other")));
            let inactive = state.panes.pane_mut_for_slot(1).expect("inactive pane");
            inactive.history = PaneHistory::from_stacks(
                vec![mount_path.join("other-old"), PathBuf::from("/tmp/keep")],
                vec![mount_path.join("other-future")],
            );
        }

        assert!(move_current_directory_home_if_inside_mount(
            &state,
            &mount_path
        ));

        let state = state.borrow();
        assert_eq!(state.panes.focused().current_dir, home_dir());
        assert_eq!(
            state.panes.focused().history.back_paths(),
            &[PathBuf::from("/tmp")]
        );
        assert_eq!(
            state.panes.focused().history.forward_paths(),
            &[PathBuf::from("/run/media/yk/USB-sibling")]
        );
        let inactive = state.panes.pane_for_slot(1).expect("inactive pane");
        assert_eq!(inactive.current_dir, home_dir());
        assert_eq!(inactive.history.back_paths(), &[PathBuf::from("/tmp/keep")]);
        assert!(inactive.history.forward_paths().is_empty());
    }

    #[test]
    fn successful_unmount_keeps_unrelated_current_path() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/run/media/yk/USB-sibling"),
            Vec::new(),
        )));

        assert!(!move_current_directory_home_if_inside_mount(
            &state,
            Path::new("/run/media/yk/USB")
        ));

        assert_eq!(
            state.borrow().panes.focused().current_dir,
            PathBuf::from("/run/media/yk/USB-sibling")
        );
    }

    #[test]
    fn devices_with_status_marks_matching_pending_action_and_error() {
        let devices = vec![
            DeviceEntry {
                label: "USB".into(),
                path: "/run/media/yk/USB".into(),
                device_path: "/dev/sdb1".into(),
                kind: "removable-media".into(),
                marker: "U".into(),
                mounted: true,
                can_mount: false,
                can_unmount: true,
                can_eject: true,
                pending_action: "".into(),
                error: "".into(),
            },
            DeviceEntry {
                label: "Other".into(),
                path: "/dev/sdc1".into(),
                device_path: "/dev/sdc1".into(),
                kind: "removable-media".into(),
                marker: "O".into(),
                mounted: false,
                can_mount: true,
                can_unmount: false,
                can_eject: false,
                pending_action: "".into(),
                error: "".into(),
            },
        ];
        let pending_actions = vec![DeviceAction {
            device_path: "/dev/sdc1".to_string(),
            action: "mount".to_string(),
        }];
        let errors = std::collections::HashMap::from([(
            "/dev/sdb1".to_string(),
            "Cannot unmount /dev/sdb1: device is busy".to_string(),
        )]);

        let devices = devices_with_status(devices, &pending_actions, &errors);

        assert_eq!(devices[0].error, "Cannot unmount /dev/sdb1: device is busy");
        assert_eq!(devices[0].pending_action, "");
        assert_eq!(devices[1].error, "");
        assert_eq!(devices[1].pending_action, "mount");
    }

    #[test]
    fn pane_status_target_slots_route_to_affected_panes() {
        let mut state = AppState::new(PathBuf::from("/tmp/slot-0"), Vec::new());
        let slot_0_id = state.panes.focused().id;
        assert!(state.panes.open_pane(PathBuf::from("/tmp/slot-1")));
        let slot_1_id = state.panes.pane_for_slot(1).expect("slot 1 pane").id;

        assert_eq!(pane_status_target_slots(&state, &[slot_1_id]), vec![1]);
        assert_eq!(
            pane_status_target_slots(&state, &[slot_0_id, slot_1_id]),
            vec![0, 1]
        );
        assert!(pane_status_target_slots(&state, &[]).is_empty());
        assert!(pane_status_target_slots(&state, &[99]).is_empty());
    }

    #[test]
    fn pane_status_state_updates_only_target_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/slot-0"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/slot-1")));

        assert_eq!(
            set_pane_status_state(&mut state, 1, "Right pane busy"),
            Some(false)
        );

        assert_eq!(state.panes.pane_for_slot(0).expect("slot 0").status, "");
        assert_eq!(
            state.panes.pane_for_slot(1).expect("slot 1").status,
            "Right pane busy"
        );

        assert_eq!(
            set_pane_status_state(&mut state, 0, "Left pane ready"),
            Some(true)
        );
        assert_eq!(
            state.panes.pane_for_slot(0).expect("slot 0").status,
            "Left pane ready"
        );
        assert_eq!(
            state.panes.pane_for_slot(1).expect("slot 1").status,
            "Right pane busy"
        );
        assert_eq!(set_pane_status_state(&mut state, 99, "Missing"), None);
    }

    #[test]
    fn focused_status_state_updates_only_focused_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/slot-0"), Vec::new());
        assert!(state.panes.open_pane(PathBuf::from("/tmp/slot-1")));
        assert!(state.panes.focus_slot(1));

        assert_eq!(set_focused_status_state(&mut state, "Focused pane"), 1);

        assert_eq!(state.panes.pane_for_slot(0).expect("slot 0").status, "");
        assert_eq!(
            state.panes.pane_for_slot(1).expect("slot 1").status,
            "Focused pane"
        );
    }

    #[test]
    fn refresh_panes_releases_slot_lookup_borrow_before_refreshing() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn refresh_panes(")
            .and_then(|(_, rest)| rest.split_once("fn refresh_affected_directories("))
            .map(|(body, _)| body)
            .expect("refresh_panes body should be present");

        assert!(
            body.contains("for pane_id in pane_ids {")
                && body.contains("refresh_pane_by_id(ui, state, bridge, *pane_id);")
                && !body.contains("state.borrow().panes.slot_for_id(*pane_id)"),
            "refresh_panes should dispatch by pane id without holding a slot lookup borrow"
        );
    }

    #[test]
    fn selection_ui_update_does_not_keep_slot_lookup_borrow_alive() {
        let source = include_str!("main.rs");
        let nested_slot_lookup = concat!(
            "update_selection_ui_for_slot(ui, state, ",
            "state.borrow().panes.focused_slot()"
        );

        assert!(
            !source.contains(nested_slot_lookup),
            "focused slot lookup must be stored before update_selection_ui_for_slot so the RefCell borrow ends before selection sync mutably borrows state"
        );
    }

    #[test]
    fn blank_drag_sentinel_returns_empty_transfer_before_pending_drag_probe() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("dnd_api.on_make_drag_at(move |slot, x, y| -> DataTransfer {")
            .and_then(|(_, rest)| rest.split_once("let Some(ui) = ui_weak.upgrade() else"))
            .map(|(body, _)| body)
            .expect("make-drag-at body should be present");

        assert!(
            body.contains("if x <= -2.0 || y <= -2.0 {\n                    return DataTransfer::default();\n                }")
                && body.contains("if x < 0.0 || y < 0.0 {\n                    return drag_transfer(FikaDragInfo::Pending(slot));\n                }"),
            "blank-area rectangle selection should suppress DragArea with an empty transfer while preserving the pending press probe for item drags"
        );
    }

    #[test]
    fn pending_drag_probe_resolves_from_pane_local_press_source() {
        let source = include_str!("main.rs");
        let dnd_block = source
            .split_once("enum FikaDragInfo {")
            .and_then(|(_, rest)| rest.split_once("// ── DropEvent inspectors"))
            .map(|(body, _)| body)
            .expect("DndApi setup should define drag info before inspectors");
        let press_body = source
            .split_once("fn press_item_view_entry_at_point_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn activate_item_view_entry_at_point_for_slot("))
            .map(|(body, _)| body)
            .expect("item-view press handler should be present");

        let controller_action = press_body
            .find("press_entry_at_pane_point(ui, &mut state_ref, slot, x, y, toggle, range)")
            .expect("item press hit-test and input update should route through the pane-local item-view controller");
        let action_execution = press_body
            .find("PaneController::new(ui, state, bridge).apply_item_view_controller_action(slot, action);")
            .expect("item press should execute the controller action after recording press state");

        assert!(dnd_block.contains("Pending(i32)"));
        assert!(
            dnd_block.contains(
                "fn pending_drag_info(state: &AppState, slot: i32) -> Option<FikaDragInfo>"
            )
        );
        assert!(dnd_block.contains("pane_for_slot(slot)?.view.input.drag_source()?"));
        assert!(dnd_block.contains("return drag_transfer(FikaDragInfo::Pending(slot));"));
        assert!(source.contains("Some(FikaDragInfo::Pending(slot)) => {"));
        assert!(source.contains("return match pending_drag_info(&state_ref, *slot)"));
        assert!(source.contains("FikaDragInfo::Pending(slot) => {"));
        assert!(source.contains("match pending_drag_info(&state_ref, *slot)"));
        assert!(
            controller_action < action_execution,
            "item press must record pane-local drag source inside the controller before action execution can refresh selection UI"
        );
        assert!(
            !press_body.contains(".set_drag_source("),
            "main.rs should execute item-view controller actions instead of mutating the input state internals directly"
        );
    }

    #[test]
    fn item_activation_routes_through_controller_action() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn activate_item_view_entry_at_point_for_slot(")
            .and_then(|(_, rest)| {
                rest.split_once("#[derive(Clone, Copy)]\nstruct ItemViewContextMenuRequest")
            })
            .map(|(body, _)| body)
            .expect("item activation handler should be present");

        assert!(
            body.contains("activate_entry_at_pane_point(ui, &state_ref, slot, x, y)")
                && body.contains("PaneController::new(ui, state, bridge).apply_item_view_controller_action(slot, action);")
                && !body.contains("item_view_entry_at_point_for_slot(ui, state, slot, x, y)")
                && !body
                    .contains("open_path_for_slot(ui, state, slot, entry.path.as_str(), bridge);"),
            "item activation should route hit-test through item_view.rs and leave PaneController to execute the returned controller action"
        );
    }

    #[test]
    fn pane_focus_refresh_does_not_rebuild_the_current_pane_surface() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn focus_pane_slot(")
            .and_then(|(_, rest)| rest.split_once("fn focus_current_ui_pane_slot("))
            .map(|(body, _)| body)
            .expect("focus_pane_slot body should be present");

        assert!(
            body.contains("let previous_slot = { state.borrow().panes.focused_slot() };")
                && body.contains("if focused && previous_slot != slot {")
                && body.contains("sync_focus_navigation_ui(ui, state, previous_slot);")
                && !body.contains("sync_navigation_ui(ui, state);"),
            "clicking inside the already focused pane must not rebuild pane surfaces, and focus changes should skip left-pane rewrites"
        );

        let split_view = include_str!("app/split_view.rs");
        let focus_sync_body = split_view
            .split_once("pub(crate) fn sync_focus_navigation_ui(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn toggle_split_view("))
            .map(|(body, _)| body)
            .expect("sync_focus_navigation_ui body should be present");

        assert!(
            focus_sync_body.contains("sync_focused_ui(")
                && focus_sync_body.contains("sync_pane_slot_ui(ui, state, previous_slot);")
                && focus_sync_body.contains("sync_pane_view_ui(ui, state, previous_slot);")
                && focus_sync_body.contains("sync_pane_slot_ui(ui, state, focused_slot);")
                && focus_sync_body.contains("sync_pane_view_ui(ui, state, focused_slot);")
                && !focus_sync_body.contains("sync_pane_slots_ui(ui, state);"),
            "pure pane focus changes should update only the old/new pane chrome and hot view rows instead of running a full pane-slots sync"
        );
    }

    #[test]
    fn pane_slot_and_view_sync_update_existing_rows_when_slot_shape_is_unchanged() {
        let source = include_str!("app/split_view.rs");
        let body = source
            .split_once("pub(crate) fn sync_pane_slots_ui(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_slot_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_slots_ui body should be present");
        let slots_model_body = body
            .split_once("fn sync_pane_slots_model(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_view_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_slots_model body should be present");
        let surfaces_model_body = source
            .split_once("fn sync_pane_surfaces_model(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_surface_pane_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_surfaces_model body should be present");

        assert!(
            body.contains("let visible_slots = visible_pane_slots(ui);")
                && body.contains("let (slots, surfaces) = {")
                && body.contains("let mut slots = Vec::with_capacity(visible_slots.len());")
                && !body.contains("let mut views = Vec::with_capacity(visible_slots.len());")
                && body.contains("let mut surfaces = Vec::with_capacity(visible_slots.len());")
                && body.contains("for slot in visible_slots.iter().copied()")
                && body.contains("let pane = pane_slot_data(ui, slot, &state_ref);")
                && body.contains("let view = pane_view_data(ui, slot, &state_ref);")
                && body.contains(
                    "PaneSurfaceData {\n                slot,\n                pane: pane.clone(),\n                view,\n            }"
                )
                && !body.contains("sync_pane_views_model(ui, views);")
                && body.contains("sync_pane_slots_model(ui, slots);")
                && body.contains("sync_pane_surfaces_model(ui, surfaces);")
                && source.contains("fn pane_view_data_needs_row_update(")
                && source.contains("fn pane_view_lightweight_fields_match(")
                && !source.contains("fn sync_pane_views_model(")
                && !source.contains("get_pane_views")
                && !source.contains("set_pane_views")
                && !source.contains("current_view != next")
                && !source.contains("current_surface.view != view")
                && !body.contains("sync_pane_entries_ui(ui, entries);")
                && !body.contains("sync_pane_media_ui(ui, media);")
                && !body.contains("sync_pane_metadata_ui(ui, metadata);")
                && slots_model_body.contains("let current = ui.get_pane_slots();")
                && slots_model_body.contains("let same_slots = current.row_count() == slots.len()")
                && slots_model_body.contains(".is_some_and(|current| current.slot == slot.slot)")
                && slots_model_body.contains("if current.row_data(row).as_ref() != Some(&slot)")
                && slots_model_body.contains("current.set_row_data(row, slot);")
                && slots_model_body
                    .contains("ui.set_pane_slots(ModelRc::new(Rc::new(VecModel::from(slots))));")
                && !slots_model_body.contains("state.borrow()")
                && surfaces_model_body.contains("let current = ui.get_pane_surfaces();")
                && surfaces_model_body
                    .contains("let same_slots = current.row_count() == surfaces.len()")
                && surfaces_model_body
                    .contains(".is_some_and(|current| current.slot == surface.slot)")
                && surfaces_model_body
                    .contains("if let Some(current_surface) = current.row_data(row)")
                && surfaces_model_body.contains("current_surface.pane != surface.pane")
                && surfaces_model_body.contains("pane_view_data_needs_row_update(")
                && surfaces_model_body.contains("current.set_row_data(row, surface);")
                && !surfaces_model_body
                    .contains("current.row_data(row).as_ref() != Some(&surface)")
                && surfaces_model_body.contains(
                    "ui.set_pane_surfaces(ModelRc::new(Rc::new(VecModel::from(surfaces))));"
                )
                && !surfaces_model_body.contains("state.borrow()"),
            "pane chrome and hot view data refresh should snapshot state before updating Slint models"
        );
    }

    #[test]
    fn pane_view_rows_carry_pane_local_item_models_without_slot_sidecars() {
        let models = include_str!("../ui/models.slint");
        let app = include_str!("../ui/app.slint");
        let split_view = include_str!("app/split_view.rs");
        let pane_view_data = models
            .split_once("export struct PaneViewData")
            .and_then(|(_, rest)| rest.split_once("export struct PaneSlotData"))
            .map(|(body, _)| body)
            .expect("PaneViewData should be declared before PaneSlotData");
        let surface_body = app
            .split_once("component PaneSlotSurface inherits Rectangle")
            .and_then(|(_, rest)| rest.split_once("export component AppWindow"))
            .map(|(body, _)| body)
            .expect("PaneSlotSurface body should be present");
        let view_data_body = split_view
            .split_once("fn pane_view_data(")
            .and_then(|(_, rest)| rest.split_once("fn pane_slot_item_view_render_geometry("))
            .map(|(body, _)| body)
            .expect("pane_view_data body should be present");
        let legacy_media_entry = concat!("ItemView", "MediaEntry");
        let legacy_pane_view_media_field = format!("media: [{legacy_media_entry}]");
        let legacy_surface_media_binding = concat!("media: root.view.", "media;");
        let legacy_slot_binding = concat!("media: pane_slot_", "media(slot, state)");
        let legacy_thumbnail_entry = concat!("ItemView", "ThumbnailEntry");
        let legacy_thumbnail_struct = format!("export struct {legacy_thumbnail_entry}");
        let slot_entry = models
            .split_once("export struct ItemViewSlotEntry")
            .and_then(|(_, rest)| rest.split_once("export struct PlaceEntry"))
            .map(|(body, _)| body)
            .expect("models.slint should define ItemViewSlotEntry before PlaceEntry");

        assert!(
            !pane_view_data.contains("entries: [ItemViewEntry]")
                && !pane_view_data.contains("bounds:")
                && !pane_view_data.contains("paint:")
                && !pane_view_data.contains("thumbnails:")
                && pane_view_data.contains("item_view_slots: [ItemViewSlotEntry]")
                && pane_view_data.contains("item_view_raster_layer: image")
                && pane_view_data.contains("item_view_raster_width: float")
                && pane_view_data.contains("item_view_raster_height: float")
                && !pane_view_data.contains("highlights:")
                && !pane_view_data.contains(&legacy_pane_view_media_field)
                && !pane_view_data.contains("metadata:")
                && !app.contains("ItemViewBounds")
                && !app.contains("thumbnails: root.view.thumbnails;")
                && !models.contains("ItemViewBounds")
                && !models.contains("export struct ItemViewMetadataEntry")
                && !models.contains(&legacy_thumbnail_struct)
                && slot_entry.contains("has_thumbnail: bool")
                && slot_entry.contains("thumbnail: image")
                && !slot_entry.contains("absolute_index")
                && !slot_entry.contains("path: string")
                && !slot_entry.contains("thumbnail_token")
                && slot_entry.contains("has_metadata_group: bool")
                && slot_entry.contains("metadata_group: string")
                && slot_entry.contains("has_metadata_location: bool")
                && slot_entry.contains("metadata_location: string")
                && slot_entry.contains("metadata_text_x: float")
                && slot_entry.contains("metadata_text_width: float")
                && slot_entry.contains("metadata_group_y: float")
                && slot_entry.contains("metadata_location_y: float")
                && slot_entry.contains("metadata_line_height: float")
                && slot_entry.contains("metadata_font_size: float")
                && !app.contains("pane_slot_0_entries")
                && !app.contains("pane_slot_1_entries")
                && !app.contains("pane_slot_0_bounds")
                && !app.contains("pane_slot_1_bounds")
                && !app.contains("pane_slot_0_media")
                && !app.contains("pane_slot_1_media")
                && !app.contains("pane_slot_0_metadata")
                && !app.contains("pane_slot_1_metadata")
                && !split_view.contains("fn sync_pane_entries_ui(")
                && !split_view.contains("fn sync_pane_media_ui(")
                && !split_view.contains("fn sync_pane_metadata_ui(")
                && !surface_body.contains("entries: root.view.entries;")
                && !surface_body.contains("bounds: root.view.bounds;")
                && surface_body.contains("item-view-slots: root.view.item_view_slots;")
                && surface_body
                    .contains("item-view-raster-layer: root.view.item_view_raster_layer;")
                && surface_body
                    .contains("item-view-raster-width: root.view.item_view_raster_width;")
                && surface_body
                    .contains("item-view-raster-height: root.view.item_view_raster_height;")
                && !surface_body.contains("highlights: root.view.highlights;")
                && !surface_body.contains(legacy_surface_media_binding)
                && !surface_body.contains("metadata: root.view.metadata;")
                && !view_data_body.contains("entries: pane_slot_entries(slot, state)")
                && !view_data_body.contains("bounds: pane_slot_bounds(slot, state)")
                && view_data_body
                    .contains("item_view_slots: pane_slot_item_view_slots(slot, state)")
                && view_data_body.contains("item_view_raster_layer")
                && !view_data_body.contains("pane_slot_highlights(slot, state)")
                && !view_data_body.contains(legacy_slot_binding)
                && !view_data_body.contains("metadata: pane_slot_metadata(slot, state)"),
            "visible paint, tile raster, slotized thumbnails, and metadata should be pane-local slot data on PaneViewData instead of fixed slot sidecars, while business entries and bounds stay Rust-side"
        );
    }

    #[test]
    fn pane_slot_sync_can_update_one_row_without_refreshing_all_slots() {
        let source = include_str!("app/split_view.rs");
        let body = source
            .split_once("pub(crate) fn sync_pane_slot_ui(")
            .and_then(|(_, rest)| rest.split_once("fn visible_pane_slots("))
            .map(|(body, _)| body)
            .expect("sync_pane_slot_ui body should be present");
        let surface_pane_body = source
            .split_once("fn sync_pane_surface_pane_ui(")
            .and_then(|(_, rest)| rest.split_once("fn replace_pane_surfaces_model_with_view("))
            .map(|(body, _)| body)
            .expect("sync_pane_surface_pane_ui body should be present");
        let view_body = source
            .split_once("pub(crate) fn sync_pane_view_ui(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_slot_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_view_ui body should be present");

        assert!(
            body.contains("if current_slot.slot == slot")
                && body.contains(
                    "let next = {\n                let state_ref = state.borrow();\n                pane_slot_data(ui, slot, &state_ref)\n            };"
                )
                && body.contains("if current_slot != next")
                && body.contains("current.set_row_data(row, next.clone());")
                && body.contains("sync_pane_surface_pane_ui(ui, state, slot, next);")
                && !body.contains("pane_view_data(")
                && !body.contains("sync_pane_slots_ui(ui, state);"),
            "single-pane refreshes should update the affected pane row without rebuilding item-view data"
        );
        assert!(
            surface_pane_body.contains("current_surface.pane = pane;")
                && !surface_pane_body.contains("pane_view_data(")
                && view_body.contains("let current = ui.get_pane_surfaces();")
                && view_body.contains("let current_view = current_surface.view.clone();")
                && view_body.contains("pane_view_data_with_visual_reuse(")
                && view_body.contains("current_surface.view = next;")
                && !view_body.contains("get_pane_views")
                && !view_body.contains("pane_slot_data("),
            "PaneSurfaceData pane/view patches should not rebuild unrelated surface data on the UI thread"
        );
    }

    #[test]
    fn pane_viewport_sync_updates_only_the_hot_view_row_field() {
        let source = include_str!("app/split_view.rs");
        let setter_body = source
            .split_once("pub(crate) fn set_pane_viewport_ui(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_view_viewport_ui("))
            .map(|(body, _)| body)
            .expect("set_pane_viewport_ui body should be present");
        let viewport_body = source
            .split_once("fn sync_pane_view_viewport_ui(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_slots_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_view_viewport_ui body should be present");

        assert!(
            setter_body.contains("pane.view.viewport_x = viewport_x;")
                && setter_body.contains("sync_pane_view_viewport_ui(ui, state, slot, viewport_x);")
                && !setter_body.contains("sync_pane_slot_ui(ui, state, slot);"),
            "viewport writes should not rebuild full PaneSlotData just to publish viewport_x"
        );
        assert!(
            viewport_body.contains("let current = ui.get_pane_surfaces();")
                && viewport_body.contains("let Some(mut current_surface) = current.row_data(row)")
                && viewport_body.contains("if current_surface.slot == slot")
                && viewport_body.contains("current_surface.view.viewport_x = viewport_x;")
                && viewport_body.contains("current.set_row_data(row, current_surface);")
                && viewport_body.contains("sync_pane_view_ui(ui, state, slot);")
                && !viewport_body.contains("get_pane_views")
                && !viewport_body.contains("pane_slot_data(ui"),
            "viewport-only row sync should patch only viewport fields on PaneSurfaceData and use hot view sync as a missing-row fallback"
        );
    }

    #[test]
    fn virtual_view_sync_updates_hot_view_rows_without_rebuilding_pane_chrome() {
        let source = include_str!("main.rs");
        let sync_body = source
            .split_once("fn sync_virtual_entries_for_slot_with_count(")
            .and_then(|(_, rest)| rest.split_once("fn start_virtual_view_prepare("))
            .map(|(body, _)| body)
            .expect("virtual entry sync body should be present");
        let result_body = source
            .split_once("fn apply_virtual_view_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_virtual_view_prepare_failure("))
            .map(|(body, _)| body)
            .expect("virtual view result body should be present");
        let production_body = source
            .split_once("#[cfg(test)]")
            .map(|(body, _)| body)
            .expect("main production body should be present");

        assert!(
            sync_body.contains("VirtualViewSyncRequest::Cached {\n            sync,\n            publish_layout,")
                && sync_body.contains("if sync.publish_viewport {\n                set_pane_viewport_ui(ui, slot, sync.viewport_x, state);")
                && result_body.contains("if update.viewport_clamped {\n            set_pane_viewport_ui(ui, slot, update.viewport_x, state);")
                && result_body.contains("sync_pane_view_ui(ui, state, slot);")
                && !result_body.contains("sync_pane_slot_ui(ui, state, slot);")
                && !production_body.contains("set_pane_viewport_ui_if_clamped")
                && !production_body.contains("sync_pane_slot_viewport_ui"),
            "cached viewport sync should only publish clamp corrections, while virtual model rebuilds update PaneViewData instead of pane chrome rows"
        );
    }

    #[test]
    fn removed_entries_use_dolphin_style_model_delta_relayout() {
        let source = include_str!("main.rs");
        let pane_source = include_str!("app/pane.rs");
        let geometry_source = include_str!("app/geometry.rs");
        let removed_body = source
            .split_once("fn apply_directory_entries_removed(")
            .and_then(|(_, rest)| rest.split_once("fn directory_removed_path_set("))
            .map(|(body, _)| body)
            .expect("directory removed body should be present");
        let relayout_body = source
            .split_once("fn apply_removed_entries_relayout_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn reload_delta_relayout_request_for_slot("))
            .map(|(body, _)| body)
            .expect("removed relayout body should be present");
        let reload_apply_body = source
            .split_once("fn apply_reload_delta_relayout_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn apply_removed_entries_relayout_for_slot("))
            .map(|(body, _)| body)
            .expect("reload delta apply body should be present");
        let request_body = source
            .split_once("fn reload_delta_relayout_request_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn apply_pane_directory_result("))
            .map(|(body, _)| body)
            .expect("reload delta relayout request body should be present");
        let load_body = source
            .split_once("fn apply_pane_directory_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_directory_prefetch_result("))
            .map(|(body, _)| body)
            .expect("directory load result body should be present");

        assert!(
            removed_body.contains("pane.remove_entries_by_paths(&removed_paths)")
                && removed_body.contains("apply_removed_entries_relayout_for_slot(")
                && removed_body.contains("clear_pane_rendered_virtual_slice(")
                && !removed_body.contains("hide_rendered_paths")
                && !removed_body
                    .contains("sync_pane_view_ui(ui, state, slot);\n    sync_pane_view_for_slot"),
            "watcher deletes should update the model delta first, then relayout the visible slice instead of publishing inactive holes"
        );
        assert!(
            relayout_body.contains("DirectoryReloadRelayout")
                && relayout_body.contains("apply_reload_delta_relayout_for_slot(")
                && source.contains("fn apply_reload_delta_relayout_for_slot(")
                && reload_apply_body
                    .contains("prepare_virtual_view_snapshot_update(*request.input.clone())")
                && reload_apply_body.contains("prepare_virtual_view_projection(")
                && reload_apply_body.contains("apply_virtual_view_result("),
            "removed-entry relayout should reuse the unified reload-delta projection/apply pipeline"
        );
        assert!(
            request_body.contains("cached_layout.clone()")
                && request_body
                    .contains("delta_layout.without_item_ranges(&removed.visible_ranges)")
                && request_body.contains(
                    "delta_layout.with_inserted_item_width_ranges(&delta.inserted_width_ranges)"
                )
                && request_body.contains("delta_layout.entry_count != visible_count")
                && request_body.contains("force_rebuild_model: true")
                && request_body.contains("pane.view.clear_pending_virtual_prepare();"),
            "reload delta relayout should fold Dolphin-style removed and inserted ranges into the cached compact layout before rebuilding slots"
        );
        assert!(
            load_body.contains("let diff = directory_reload_diff(&pane.entries, &entries);")
                && load_body.contains("diff.supports_index_delta_relayout()")
                && load_body.contains("pane.remove_entries_by_paths(&diff.removed_paths)")
                && load_body.contains("pane.set_entries_with_summary_preserving_rendered(")
                && load_body.contains("reload_relayout = Some(DirectoryReloadRelayout")
                && load_body.contains("pane.apply_removed_paths_cleanup(&diff.removed_paths);")
                && load_body.contains("pane.set_entries_with_summary(")
                && load_body.contains("rendered_slice_cleared = true;")
                && load_body.contains("apply_reload_delta_relayout_for_slot(")
                && load_body.contains("clear_pane_rendered_virtual_slice(")
                && !load_body.contains("apply_removed_paths_side_effects"),
            "reload diffs should use Dolphin-style index deltas when safe and clear stale rendered rows for mixed/reordered reloads"
        );
        assert!(
            pane_source.contains("struct PaneEntriesRemoved")
                && pane_source.contains("fn remove_model_ranges_from_visible_indices(")
                && pane_source.contains("fn clear_removed_drop_target(")
                && !pane_source.contains("fn hide_rendered_paths(")
                && geometry_source
                    .contains("fn without_item_ranges(&self, ranges: &[Range<usize>]) -> Self")
                && geometry_source.contains("fn with_inserted_item_width_ranges("),
            "pane and geometry layers should expose inserted/removed ranges instead of path-based rendered slot hiding"
        );
    }

    #[test]
    fn filter_refresh_preserves_current_view_until_uncached_prepare_commits() {
        let source = include_str!("main.rs");
        let filter_body = source
            .split_once("fn apply_filter_for_slot(")
            .and_then(|(_, rest)| rest.split_once("#[allow(clippy::too_many_arguments)]"))
            .map(|(body, _)| body)
            .expect("filter body should be present");
        let prepare_body = source
            .split_once("fn start_local_search_index_prepare(")
            .and_then(|(_, rest)| rest.split_once("fn apply_local_search_index_result("))
            .map(|(body, _)| body)
            .expect("local search prepare body should be present");
        let apply_body = source
            .split_once("fn apply_local_search_index_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_local_search_index_prepare_failure("))
            .map(|(body, _)| body)
            .expect("local search apply body should be present");
        let sync_body = source
            .split_once("fn sync_virtual_entries_for_slot_with_count_and_cache_policy(")
            .and_then(|(_, rest)| rest.split_once("fn start_virtual_view_prepare("))
            .map(|(body, _)| body)
            .expect("virtual entry sync body should be present");

        assert!(
            filter_body.contains("pane.search_index_generation.next()")
                && filter_body.contains("pane.search.index_pending = true;")
                && filter_body.contains("start_local_search_index_prepare(")
                && !filter_body.contains("rebuild_visible_entry_index_for_slot")
                && !filter_body
                    .contains("sync_virtual_entries_for_slot_with_count_and_cache_policy(")
                && prepare_body.contains("tokio::task::spawn_blocking")
                && prepare_body.contains("prepare_visible_entry_index(")
                && prepare_body.contains("AsyncEvent::LocalSearchIndexPrepared(")
                && apply_body.contains("pane.search_index_generation.is_current(generation)")
                && apply_body.contains("pane.search.index_pending = false;")
                && apply_body.contains("apply_prepared_visible_entry_index_to_pane(")
                && apply_body.contains("pane.view.virtual_view.invalidate();")
                && apply_body
                    .contains("sync_virtual_entries_for_slot_with_count_and_cache_policy(")
                && apply_body.contains("Some(summary.count),")
                && apply_body.contains("true,\n        false,\n        false,\n    );")
                && sync_body.contains("if pane.search.index_pending")
                && sync_body.contains("force_uncached_prepare: bool")
                && sync_body.contains("force_rebuild_model: bool")
                && sync_body.contains("if !force_uncached_prepare")
                && sync_body.contains("&& !force_rebuild_model")
                && sync_body.contains("VirtualViewCache::default()"),
            "filter/search refresh should build the visible index off the UI thread, keep the old view alive while pending, force an uncached virtual prepare for the new filter model, and commit atomically when the current result returns"
        );
    }

    #[test]
    fn pane_slot_sync_releases_state_borrow_before_model_updates() {
        let source = include_str!("app/split_view.rs");
        let slots_model_body = source
            .split_once("fn sync_pane_slots_model(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_view_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_slots_model body should be present");
        let surfaces_model_body = source
            .split_once("fn sync_pane_surfaces_model(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_surface_pane_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_surfaces_model body should be present");
        let slot_body = source
            .split_once("pub(crate) fn sync_pane_slot_ui(")
            .and_then(|(_, rest)| rest.split_once("fn visible_pane_slots("))
            .map(|(body, _)| body)
            .expect("sync_pane_slot_ui body should be present");
        let (_, slot_update_body) = slot_body
            .split_once("};\n            if current_slot != next")
            .expect("sync_pane_slot_ui should close the state borrow before touching the model");

        assert!(
            !slots_model_body.contains("state.borrow()")
                && slots_model_body.contains("current.set_row_data(row, slot);")
                && slots_model_body
                    .contains("ui.set_pane_slots(ModelRc::new(Rc::new(VecModel::from(slots))));")
                && !source.contains("fn sync_pane_views_model(")
                && !source.contains("get_pane_views")
                && !source.contains("set_pane_views")
                && !surfaces_model_body.contains("state.borrow()")
                && surfaces_model_body.contains("current.set_row_data(row, surface);")
                && surfaces_model_body.contains(
                    "ui.set_pane_surfaces(ModelRc::new(Rc::new(VecModel::from(surfaces))));"
                ),
            "full pane sync must release AppState borrow before Slint model setters"
        );
        assert!(
            !slot_update_body.contains("state.borrow()")
                && slot_update_body.contains("current.set_row_data(row, next.clone());"),
            "single-row pane sync must release AppState borrow before Slint model setters"
        );
    }

    #[test]
    fn pane_status_updates_write_state_before_syncing_ui() {
        let source = include_str!("main.rs");
        let pane_status_body = source
            .split_once("fn set_pane_status(")
            .and_then(|(_, rest)| rest.split_once("fn set_pane_status_state("))
            .map(|(body, _)| body)
            .expect("set_pane_status body should be present");
        let focused_status_body = source
            .split_once("fn set_status(")
            .and_then(|(_, rest)| rest.split_once("fn set_focused_status_state("))
            .map(|(body, _)| body)
            .expect("set_status body should be present");

        assert!(
            pane_status_body.contains("state.try_borrow_mut()")
                && pane_status_body.contains("ui.set_status(SharedString::from(message));")
                && pane_status_body.contains("set_pane_status_state(&mut state, slot, message)")
                && pane_status_body.contains("sync_pane_slot_ui(ui, state, slot);")
                && !pane_status_body.contains("sync_pane_slots_ui(ui, state);")
                && focused_status_body.contains("state.try_borrow_mut()")
                && focused_status_body.contains("ui.set_status(SharedString::from(message));")
                && focused_status_body.contains("set_focused_status_state(&mut state, message)")
                && focused_status_body.contains("sync_pane_slot_ui(ui, state, slot);")
                && !focused_status_body.contains("sync_pane_slots_ui(ui, state);"),
            "status updates should avoid RefCell panics, mutate pane-local state first, and refresh only the affected pane row"
        );
    }

    #[test]
    fn pane_slot_sync_legacy_shape_is_not_used() {
        let source = include_str!("app/split_view.rs");
        let body = source
            .split_once("pub(crate) fn sync_pane_slot_ui(")
            .and_then(|(_, rest)| rest.split_once("fn visible_pane_slots("))
            .map(|(body, _)| body)
            .expect("sync_pane_slot_ui body should be present");

        assert!(
            !body.contains(
                "let state_ref = state.borrow();\n    let current = ui.get_pane_slots();"
            ) && !body.contains("let next = pane_slot_data(ui, slot, &state_ref);"),
            "single-row pane sync must not keep AppState borrowed across Slint model reads/writes"
        );
    }

    #[test]
    fn pane_view_changes_rebuild_the_visible_slice_synchronously() {
        let source = include_str!("main.rs");
        let main_body = source
            .split_once("fn main()")
            .and_then(|(_, rest)| rest.split_once("load_directory(&ui, &state, &bridge);"))
            .map(|(body, _)| body)
            .expect("main setup body should be present");
        let callback_body = source
            .split_once("fn register_ui_signal_callbacks(")
            .and_then(|(_, rest)| rest.split_once("fn register_pane_routing_callbacks("))
            .map(|(body, _)| body)
            .expect("signal callback registration body should be present");
        let emit_body = source
            .split_once("fn emit(&self, signal: UiSignal)")
            .and_then(|(_, rest)| rest.split_once("fn drain_async_results"))
            .map(|(body, _)| body)
            .expect("signal bus emit body should be present");
        let scheduler_body = source
            .split_once("struct PaneViewSyncScheduler")
            .and_then(|(_, rest)| rest.split_once("struct IconSizeUpdateScheduler"))
            .map(|(body, _)| body)
            .expect("pane view scheduler body should be present");

        assert!(
            main_body.contains("let ui_signals = Rc::new(UiSignalBus::new(UiSignalBusInput {")
                && main_body.contains("register_ui_signal_callbacks(&ui, Rc::clone(&ui_signals));")
                && callback_body.contains(
                    "ui.on_pane_view_changed(move |slot| bus.emit(UiSignal::PaneViewChanged(slot)));"
                )
                && emit_body
                    .contains("UiSignal::PaneViewChanged(slot) => self.pane_view_sync.request(slot),")
                && !callback_body.contains("pane_view_sync.request(slot);")
                && !main_body.contains("sync_pane_view_for_slot(&ui, &state, &bridge, slot);"),
            "pane view-changed callbacks should emit a bus signal, with synchronous scheduler dispatch centralized in UiSignalBus"
        );
        assert!(
            scheduler_body.contains("syncing: Cell<bool>")
                && scheduler_body.contains("if self.syncing.get()")
                && scheduler_body.contains("icon_size_update_pending: Rc<Cell<bool>>")
                && scheduler_body.contains("if self.icon_size_update_pending.get()")
                && scheduler_body
                    .contains("sync_pane_viewport_for_slot_with_thumbnail_scheduling(")
                && scheduler_body.contains("false,")
                && scheduler_body
                    .contains("sync_pane_viewport_for_slot(&ui, &self.state, &self.bridge, slot);")
                && !scheduler_body.contains("TimerMode::SingleShot")
                && !scheduler_body.contains("pending_slots"),
            "pane view scheduler should synchronously rebuild the current visible slice while deferring roles updates during Dolphin's icon-size timer"
        );
    }

    #[test]
    fn layout_changes_rebuild_the_visible_slice_immediately() {
        let source = include_str!("main.rs");
        let callback_body = source
            .split_once("fn register_ui_signal_callbacks(")
            .and_then(|(_, rest)| rest.split_once("fn register_pane_routing_callbacks("))
            .map(|(body, _)| body)
            .expect("signal callback registration body should be present");
        let emit_body = source
            .split_once("fn emit(&self, signal: UiSignal)")
            .and_then(|(_, rest)| rest.split_once("fn drain_async_results"))
            .map(|(body, _)| body)
            .expect("signal bus emit body should be present");
        let viewport_body = source
            .split_once("fn sync_pane_viewport_for_slot(")
            .and_then(|(_, rest)| {
                rest.split_once("fn sync_pane_layout_for_slot_with_thumbnail_scheduling(")
            })
            .map(|(body, _)| body)
            .expect("viewport-only pane sync body should be present");
        let visible_layout_body = source
            .split_once("fn sync_visible_pane_layouts(")
            .and_then(|(_, rest)| {
                rest.split_once("fn sync_visible_pane_layouts_with_thumbnail_scheduling(")
            })
            .map(|(body, _)| body)
            .expect("visible pane layout body should be present");
        let visible_layout_with_thumbnail_body = source
            .split_once("fn sync_visible_pane_layouts_with_thumbnail_scheduling(")
            .and_then(|(_, rest)| rest.split_once("fn apply_visible_pane_zoom_style_options("))
            .map(|(body, _)| body)
            .expect("visible pane layout with thumbnails body should be present");
        let layout_with_thumbnail_body = source
            .split_once("fn sync_pane_layout_for_slot_with_thumbnail_scheduling(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_view_for_slot("))
            .map(|(body, _)| body)
            .expect("pane layout thumbnail sync body should be present");
        let layout_scheduler_body = source
            .split_once("struct PaneLayoutSyncScheduler")
            .and_then(|(_, rest)| rest.split_once("struct ThumbnailFlushScheduler"))
            .map(|(body, _)| body)
            .expect("pane layout scheduler body should be present");

        assert!(
            callback_body.contains(
                "ui.on_pane_layout_changed(move || bus.emit(UiSignal::PaneLayoutChanged));"
            ) && emit_body
                .contains("UiSignal::PaneLayoutChanged => self.pane_layout_sync.sync_now(),")
                && !callback_body.contains("pane_layout_sync.sync_now();")
                && !callback_body.contains("sync_pane_view_for_slot"),
            "ordinary layout changes should enter the signal bus before the immediate layout scheduler path runs"
        );
        assert!(
            layout_scheduler_body.contains("fn sync_now(&self)")
                && layout_scheduler_body.contains("self.pane_view_sync.flush_all();")
                && layout_scheduler_body
                    .contains("if self.icon_size_update.visible_index_range_updates_enabled()")
                && layout_scheduler_body
                    .contains("sync_visible_pane_layouts(&ui, &self.state, &self.bridge);")
                && layout_scheduler_body
                    .contains("sync_visible_pane_layouts_with_thumbnail_scheduling(")
                && layout_scheduler_body.contains("false,"),
            "ordinary layout changes should rebuild visible pane slices immediately and defer roles updates while Dolphin's icon-size timer is pending"
        );
        assert!(
            viewport_body.contains("sync_pane_viewport_for_slot_with_thumbnail_scheduling")
                && viewport_body.contains("true);")
                && !viewport_body.contains("sync_pane_slot_preview")
                && !viewport_body.contains("sync_virtual_entries(ui, state, bridge, true);")
                && !viewport_body.contains("filtered_entry_count_for_slot")
                && !viewport_body.contains("return;"),
            "pane layout/scroll sync should update the target slot through the shared virtual slice path"
        );
        assert!(
            visible_layout_body.contains(
                "sync_visible_pane_layouts_with_thumbnail_scheduling(ui, state, bridge, true);"
            ) && visible_layout_with_thumbnail_body.contains(
                "if slots.row_count() == 0 {\n        return;\n    }"
            ) && !visible_layout_with_thumbnail_body.contains(
                "sync_pane_layout_for_slot_with_thumbnail_scheduling(\n            ui,\n            state,\n            bridge,\n            0,"
            ),
            "ordinary layout changes should keep thumbnail scheduling enabled without creating the initial pane surface before entries are loaded"
        );
        assert!(
            layout_with_thumbnail_body.contains("schedule_thumbnails,")
                && layout_with_thumbnail_body.contains("true,\n        true,"),
            "layout changes must synchronously clamp/rebuild the visible slice before Slint reuses old virtual coordinates"
        );
    }

    #[test]
    fn icon_zoom_layout_is_latest_only_and_thumbnail_updates_are_coalesced() {
        let source = include_str!("main.rs");
        let production_source = source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .expect("main.rs should contain tests after production code");
        let app = include_str!("../ui/app.slint");
        let roles_updater_source = include_str!("app/file_item_roles_updater.rs");
        let main_body = source
            .split_once("fn main()")
            .and_then(|(_, rest)| {
                rest.split_once("let async_rx = Rc::new(RefCell::new(async_rx));")
            })
            .map(|(body, _)| body)
            .expect("pane layout scheduler setup should be present");
        let callback_body = source
            .split_once("fn register_ui_signal_callbacks(")
            .and_then(|(_, rest)| rest.split_once("fn register_pane_routing_callbacks("))
            .map(|(body, _)| body)
            .expect("signal callback registration body should be present");
        let emit_body = source
            .split_once("fn emit(&self, signal: UiSignal)")
            .and_then(|(_, rest)| rest.split_once("fn drain_async_results"))
            .map(|(body, _)| body)
            .expect("signal bus emit body should be present");
        let pane_view_scheduler_body = source
            .split_once("struct PaneViewSyncScheduler")
            .and_then(|(_, rest)| rest.split_once("struct IconSizeUpdateScheduler"))
            .map(|(body, _)| body)
            .expect("pane view scheduler body should be present");
        let icon_size_scheduler_body = source
            .split_once("struct IconSizeUpdateScheduler")
            .and_then(|(_, rest)| rest.split_once("struct PaneLayoutSyncScheduler"))
            .map(|(body, _)| body)
            .expect("icon size update scheduler body should be present");
        let scheduler_body = source
            .split_once("struct PaneLayoutSyncScheduler")
            .and_then(|(_, rest)| rest.split_once("struct ThumbnailFlushScheduler"))
            .map(|(body, _)| body)
            .expect("pane layout scheduler body should be present");
        let icon_zoom_body = source
            .split_once("fn apply_pane_zoom_style_option_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn update_icon_size_for_visible_panes("))
            .map(|(body, _)| body)
            .expect("icon zoom layout body should be present");
        let visible_icon_zoom_body = source
            .split_once("fn apply_visible_pane_zoom_style_options(")
            .and_then(|(_, rest)| rest.split_once("fn apply_pane_zoom_style_option_for_slot("))
            .map(|(body, _)| body)
            .expect("visible icon zoom layout body should be present");
        let update_icon_size_body = source
            .split_once("fn update_icon_size_for_visible_panes(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_viewport_for_slot("))
            .map(|(body, _)| body)
            .expect("icon size update body should be present");
        let apply_virtual_view_result_body = source
            .split_once("fn apply_virtual_view_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_virtual_view_prepare_failure("))
            .map(|(body, _)| body)
            .expect("virtual view result body should be present");
        let virtual_sync_body = source
            .split_once("fn sync_virtual_entries_for_slot_with_count_and_cache_policy(")
            .and_then(|(_, rest)| rest.split_once("fn start_virtual_view_prepare("))
            .map(|(body, _)| body)
            .expect("virtual sync body should be present");
        let start_prepare_body = source
            .split_once("fn start_virtual_view_prepare(")
            .and_then(|(_, rest)| rest.split_once("fn prepare_virtual_view_projection("))
            .map(|(body, _)| body)
            .expect("virtual prepare body should be present");
        let prepare_projection_body = source
            .split_once("fn prepare_virtual_view_projection(")
            .and_then(|(_, rest)| rest.split_once("#[allow(clippy::too_many_arguments)]"))
            .map(|(body, _)| body)
            .expect("virtual projection body should be present");
        let removed_zoom_range_hint_function = ["fn ", "icon_zoom_range_hint("].concat();
        let removed_zoom_width_function =
            ["zoom", "_range", "_visible", "_name", "_width", "_units"].concat();
        let removed_zoom_width_vec = ["visible", "_name", "_width", "_units"].concat();

        assert!(
            roles_updater_source.contains(
                "pub(crate) const ICON_SIZE_UPDATE_INTERVAL: Duration = Duration::from_millis(300);"
            ) && app.contains("callback icon_zoom_layout_changed();")
                && app.contains(
                    "changed icon_zoom_level => {\n        root.icon_zoom_layout_changed();\n    }"
                )
                && !app.contains(
                    "changed icon_zoom_level => {\n        root.pane_layout_changed();\n    }"
                ),
            "icon zoom should use a dedicated callback instead of the ordinary immediate layout callback"
        );
        assert!(
            main_body.contains("let icon_size_update_pending = Rc::new(Cell::new(false));")
                && main_body.contains("Rc::clone(&icon_size_update_pending)")
                && main_body
                    .matches("Rc::clone(&icon_size_update_pending)")
                    .count()
                    >= 2
                && callback_body.contains(
                    "ui.on_icon_zoom_layout_changed(move || bus.emit(UiSignal::IconZoomLayoutChanged));"
                )
                && emit_body.contains(
                    "UiSignal::IconZoomLayoutChanged => self.pane_layout_sync.set_icon_zoom_level_now(),"
                )
                && !callback_body.contains("pane_layout_sync.set_icon_zoom_level_now();"),
            "icon zoom callbacks should emit a bus signal; the bus dispatch should synchronously request the latest layout"
        );
        assert!(
            icon_size_scheduler_body.contains("TimerMode::SingleShot")
                && icon_size_scheduler_body.contains("ICON_SIZE_UPDATE_INTERVAL")
                && icon_size_scheduler_body.contains("fn trigger_icon_size_update(&self)")
                && icon_size_scheduler_body.contains("self.pending.set(true);")
                && icon_size_scheduler_body.contains("self.timer.restart();")
                && icon_size_scheduler_body
                    .contains("fn visible_index_range_updates_enabled(&self)")
                && icon_size_scheduler_body.contains("!self.pending.get()")
                && icon_size_scheduler_body.contains("timer_pending.replace(false)")
                && icon_size_scheduler_body.contains(
                    "update_icon_size_for_visible_panes(&ui, &timer_state, &timer_bridge);"
                )
                && !icon_size_scheduler_body.contains("refresh_visible_pane_tile_frame_rasters")
                && !icon_size_scheduler_body.contains("pending_icon_zoom_rasters"),
            "icon-size changes should use Dolphin's triggerIconSizeUpdate/updateIconSize split"
        );
        assert!(
            pane_view_scheduler_body.contains("icon_size_update_pending: Rc<Cell<bool>>")
                && pane_view_scheduler_body.contains("if self.icon_size_update_pending.get()")
                && pane_view_scheduler_body
                    .contains("sync_pane_viewport_for_slot_with_thumbnail_scheduling(")
                && pane_view_scheduler_body.contains("false,")
                && pane_view_scheduler_body
                    .contains("sync_pane_viewport_for_slot(&ui, &self.state, &self.bridge, slot);"),
            "viewport range changes should not run roles updates while Dolphin's icon-size timer is pending"
        );
        assert!(
            scheduler_body.contains("fn set_icon_zoom_level_now(&self)")
                && scheduler_body.contains(
                    "apply_visible_pane_zoom_style_options(&ui, &self.state, &self.bridge);"
                )
                && scheduler_body.contains("self.icon_size_update.trigger_icon_size_update();")
                && scheduler_body.contains("fn sync_now(&self)")
                && scheduler_body.contains("visible_index_range_updates_enabled()")
                && scheduler_body
                    .contains("sync_visible_pane_layouts(&ui, &self.state, &self.bridge);")
                && scheduler_body.contains("sync_visible_pane_layouts_with_thumbnail_scheduling(")
                && scheduler_body.contains("false,")
                && !scheduler_body.contains("self.timer.stop();")
                && !scheduler_body.contains("pending_icon_zoom_thumbnails"),
            "zoom should request layout immediately but keep roles updater paused until the icon-size timer fires"
        );
        assert!(
            icon_zoom_body.contains("sync_virtual_entries_for_slot_with_count(")
                && icon_zoom_body.contains(
                    "sync_virtual_entries_for_slot_with_count(ui, state, bridge, slot, false, None, true, true);"
                )
                && update_icon_size_body
                    .contains("refresh_icon_size_models_for_visible_panes(ui, state, bridge);")
                && !update_icon_size_body.contains("schedule_thumbnail_roles_for_visible_panes")
                && update_icon_size_body.contains(
                    "ui, state, bridge, slot, true, None, false, true, false, true, true,"
                )
                && apply_virtual_view_result_body
                    .contains("let media_entries = if schedule_thumbnails")
                && apply_virtual_view_result_body
                    .contains("decorate_entries_with_prepared_thumbnail_keys_for_pane(")
                && apply_virtual_view_result_body
                    .contains("preserve_current_thumbnail_roles_for_deferred_icon_size_update(")
                && apply_virtual_view_result_body.contains("if schedule_thumbnails")
                && apply_virtual_view_result_body
                    .contains("if schedule_visible_thumbnail_roles_after_apply")
                && apply_virtual_view_result_body
                    .contains("ThumbnailScheduleEntry::from_entry_with_prepared_key(")
                && apply_virtual_view_result_body
                    .contains("schedule_visible_thumbnail_roles_for_entries(")
                && !apply_virtual_view_result_body
                    .contains("schedule_visible_thumbnail_roles_for_slot(ui, state, bridge, slot);")
                && !apply_virtual_view_result_body
                    .contains("decorate_entries_with_cached_thumbnails_for_pane(")
                && apply_virtual_view_result_body.contains("missing_projection=true")
                && !apply_virtual_view_result_body.contains("prepare_virtual_view_projection(")
                && apply_virtual_view_result_body.contains("slot_projections,")
                && apply_virtual_view_result_body.contains("thumbnail_keys,")
                && apply_virtual_view_result_body.contains("metadata_rows,")
                && !apply_virtual_view_result_body.contains("item_view_slot_projections_for_entries(")
                && virtual_sync_body.contains("pane.view.defer_virtual_prepare(request);")
                && virtual_sync_body.contains("pane.view.mark_virtual_prepare_started(generation);")
                && !virtual_sync_body.contains("prepare_virtual_view_snapshot_update(*input)")
                && start_prepare_body.contains("tokio::task::spawn_blocking")
                && start_prepare_body.contains("prepare_virtual_view_snapshot_update(*input)")
                && start_prepare_body.contains("prepare_virtual_view_projection(")
                && start_prepare_body.contains("projection_ms")
                && prepare_projection_body.contains("item_view_slot_projections_for_entries(")
                && prepare_projection_body.contains("prepare_thumbnail_keys_for_entries(")
                && visible_icon_zoom_body
                    .contains("if slots.row_count() == 0 {\n        return;\n    }")
                && !visible_icon_zoom_body
                    .contains("apply_pane_zoom_style_option_for_slot(ui, state, bridge, 0);")
                && !icon_zoom_body.contains("sync_pane_layout_for_slot_with_thumbnail_scheduling")
                && !production_source.contains(&removed_zoom_width_function)
                && !production_source.contains(&removed_zoom_width_vec)
                && !production_source.contains(&removed_zoom_range_hint_function)
                && !production_source.contains("try_relayout_cached_pane_icon_zoom_layout")
                && !production_source.contains("prepare_pane_icon_zoom_layout_for_slot")
                && !production_source.contains("relayout_pane_item_view_entries_model")
                && !production_source.contains("cached_zoom_relayout_range")
                && !production_source.contains("zoom_item_view_layout_engine")
                && !production_source.contains("ICON_ZOOM_LAYOUT_PREWARM_ENTRY_LIMIT")
                && !production_source.contains("ICON_ZOOM_SYNC_UNCACHED_RELAYOUT_ENTRY_LIMIT")
                && !production_source.contains("schedule_zoom_layout_prewarm")
                && !production_source.contains("zoom_layout_prewarm_levels")
                && !production_source.contains("prepare_virtual_view_layout_prewarm")
                && !production_source.contains("VirtualViewLayoutsPrewarmed")
                && !production_source.contains("layout_history")
                && !production_source.contains("sync_pane_view_ui_defer_raster")
                && !production_source.contains("ICON_ZOOM_RASTER_COALESCE"),
            "icon zoom should keep Dolphin's immediate request boundary, but heavy snapshot/projection work must stay off the UI thread and thumbnail/preview roles must stay coalesced through the 300ms updater timer"
        );
    }

    #[test]
    fn persist_ui_state_does_not_rebuild_virtual_views() {
        let source = include_str!("main.rs");
        let callback_body = source
            .split_once("fn register_ui_signal_callbacks(")
            .and_then(|(_, rest)| rest.split_once("fn register_pane_routing_callbacks("))
            .map(|(body, _)| body)
            .expect("signal callback registration body should be present");
        let persist_arm = source
            .split_once("UiSignal::PersistUiState => {")
            .and_then(|(_, rest)| rest.split_once("UiSignal::DarkModeChanged"))
            .map(|(body, _)| body)
            .expect("persist ui signal arm should be present");

        assert!(
            callback_body
                .contains("ui.on_persist_ui_state(move || bus.emit(UiSignal::PersistUiState));")
                && persist_arm.contains("self.settings_save")
                && persist_arm.contains(".schedule(current_settings(&ui, &self.state));")
                && !callback_body.contains("settings_save.schedule")
                && !persist_arm.contains("save_current_settings")
                && !persist_arm.contains("save_settings")
                && !persist_arm.contains("sync_visible_pane_layouts")
                && !persist_arm.contains("sync_pane_layout_for_slot")
                && !persist_arm.contains("invalidate_virtual_view")
                && !persist_arm.contains("bridge"),
            "interactive settings persistence should emit through the bus and schedule a coalesced save without blocking zoom/layout on virtual refresh or disk writes"
        );
    }

    #[test]
    fn dark_mode_toggle_refreshes_tile_frame_rasters() {
        let source = include_str!("main.rs");
        let app = include_str!("../ui/app.slint");
        let dark_toggle_body = app
            .split_once("dark_toggled => {")
            .and_then(|(_, rest)| rest.split_once("}"))
            .map(|(body, _)| body)
            .expect("TopBar dark_toggled handler should be present");
        let callback_body = source
            .split_once("ui.on_dark_mode_changed(move ||")
            .and_then(|(_, rest)| rest.split_once("ui.window().on_close_requested"))
            .map(|(body, _)| body)
            .expect("dark mode changed handler should be present");
        let dark_signal_arm = source
            .split_once("UiSignal::DarkModeChanged => {")
            .and_then(|(_, rest)| rest.split_once("fn drain_async_results"))
            .map(|(body, _)| body)
            .expect("dark mode signal arm should be present");
        let refresh_body = source
            .split_once("fn refresh_visible_pane_tile_frame_rasters(")
            .and_then(|(_, rest)| rest.split_once("fn selection_status_text("))
            .map(|(body, _)| body)
            .expect("tile frame raster refresh body should be present");

        assert!(
            app.contains("callback dark_mode_changed();")
                && dark_toggle_body.contains("root.dark_mode = !root.dark_mode;")
                && dark_toggle_body.contains("root.dark_mode_changed();")
                && dark_toggle_body.contains("root.persist_ui_state();"),
            "theme toggles should notify Rust before saving settings so tile frame rasters can match the new theme"
        );
        assert!(
            callback_body.contains("bus.emit(UiSignal::DarkModeChanged)")
                && dark_signal_arm
                    .contains("refresh_visible_pane_tile_frame_rasters(&ui, &self.state);")
                && refresh_body.contains("sync_pane_view_ui(ui, state, 0);")
                && refresh_body.contains("sync_pane_view_ui(ui, state, pane.slot);")
                && !refresh_body.contains("sync_pane_layout_for_slot")
                && !refresh_body.contains("sync_virtual_entries_for_slot"),
            "dark-mode signal dispatch should regenerate visible tile frame images without rebuilding the directory snapshot"
        );
    }

    #[test]
    fn interactive_settings_saves_are_coalesced_off_the_ui_thread() {
        let main_source = include_str!("main.rs");
        let production_main_source = main_source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .expect("main.rs should contain tests after production code");
        let scheduler_source = include_str!("app/settings_save.rs");
        let scheduler_body = scheduler_source
            .split_once("pub(crate) struct SettingsSaveScheduler")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn save_settings_latest"))
            .map(|(body, _)| body)
            .expect("settings save scheduler body should be present");
        let close_body = main_source
            .split_once("fn close_requested(&self) -> CloseRequestResponse")
            .and_then(|(_, rest)| rest.split_once("fn route_focus"))
            .map(|(body, _)| body)
            .expect("signal bus close-request body should be present");
        let callback_body = main_source
            .split_once("fn register_ui_signal_callbacks(")
            .and_then(|(_, rest)| rest.split_once("fn register_pane_routing_callbacks("))
            .map(|(body, _)| body)
            .expect("signal callback registration body should be present");

        assert!(
            production_main_source
                .contains("use app::settings_save::{SettingsSaveScheduler, save_settings_latest};")
                && !production_main_source.contains("const SETTINGS_SAVE_COALESCE")
                && scheduler_source.contains(
                    "const SETTINGS_SAVE_COALESCE: Duration = Duration::from_millis(120);"
                )
                && scheduler_body.contains("TimerMode::SingleShot")
                && scheduler_body.contains(
                    "*self.pending.borrow_mut() = Some((generation, settings));"
                )
                && scheduler_body.contains("self.timer.restart();")
                && scheduler_body.contains("spawn_settings_save(async_handle.clone(), generation, settings);")
                && scheduler_body.contains("save_settings_latest(settings);")
                && scheduler_source.contains("tokio::task::spawn_blocking(move || save_settings_if_latest(generation, settings))")
                && callback_body.contains(".on_close_requested(move || bus.close_requested());")
                && close_body.contains("self.settings_save")
                && close_body.contains(".save_now(current_settings(&ui, &self.state));"),
            "zoom/sidebar persistence should coalesce latest settings and write them in the background, while close keeps a synchronous final save"
        );
    }

    #[test]
    fn thumbnail_results_are_batched_before_virtual_refresh() {
        let source = include_str!("main.rs");
        let async_body = source
            .split_once("fn apply_async_event(")
            .and_then(|(_, rest)| rest.split_once("fn apply_directory_result("))
            .map(|(body, _)| body)
            .expect("apply_async_event body should be present");
        let flush_body = source
            .split_once("fn flush_thumbnail_results(")
            .and_then(|(_, rest)| rest.split_once("fn refresh_visible_pane_tile_frame_rasters("))
            .map(|(body, _)| body)
            .expect("thumbnail flush body should be present");

        assert!(
            source.contains("const THUMBNAIL_FLUSH_COALESCE: Duration = Duration::from_millis(16);")
                && source.contains("struct ThumbnailFlushScheduler")
                && async_body.contains(
                    "AsyncEvent::ThumbnailLoaded {\n            pane_id,\n            generation,\n            load,\n        } => thumbnail_flush.push(pane_id, generation, load)"
                )
                && !async_body.contains("sync_virtual_entries(ui, state, bridge, false);"),
            "thumbnail async results should be queued instead of refreshing the virtual model per image"
        );
        assert!(
            flush_body
                .contains("while let Some((pane_id, generation, load)) = pending.pop_front()")
                && flush_body.contains("refresh_pane_ids.push(pane_id);")
                && flush_body.contains("icon_size_update_pending: bool")
                && flush_body.contains("if !icon_size_update_pending")
                && flush_body.contains("visible_syncs += 1;")
                && flush_body
                    .contains("sync_virtual_entries_for_slot(ui, state, bridge, slot, false);")
                && !flush_body.contains("sync_virtual_entries(ui, state, bridge, false);"),
            "thumbnail flush should apply pending loads, refresh affected pane slots once when zoom is idle, and gate UI sync while Dolphin's icon-size timer is pending"
        );
    }

    #[test]
    fn delayed_thumbnail_roles_updater_uses_row_tokens_without_slint_image_clones() {
        let source = include_str!("main.rs");
        let production_source = source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .expect("main.rs should contain tests after production code");
        let roles_updater_source = include_str!("app/file_item_roles_updater.rs");
        let schedule_body = roles_updater_source
            .split_once("pub(crate) fn schedule_visible_thumbnail_roles_for_slot(")
            .and_then(|(_, rest)| {
                rest.split_once("fn schedule_thumbnail_roles_for_slot_with_scope(")
            })
            .map(|(body, _)| body)
            .expect("visible thumbnail roles scheduling body should be present");
        let slot_schedule_body = roles_updater_source
            .split_once("fn schedule_thumbnail_roles_for_slot_with_scope(")
            .and_then(|(_, rest)| rest.split_once("#[allow(clippy::too_many_arguments)]"))
            .map(|(body, _)| body)
            .expect("slot thumbnail roles scheduling body should be present");

        assert!(
            production_source.contains("schedule_thumbnail_roles_for_entries(")
                && !production_source.contains("fn schedule_visible_thumbnails(")
                && slot_schedule_body.contains(".virtual_entry_tokens")
                && slot_schedule_body.contains(".map(ThumbnailScheduleEntry::from_row_token)")
                && schedule_body.contains("ThumbnailScheduleScope::VisibleOnly")
                && roles_updater_source.contains("ThumbnailScheduleScope::VisibleAndReadAhead")
                && roles_updater_source.contains("visible_indexes_to_resolve_for_slice(")
                && roles_updater_source.contains("indexes_to_resolve_for_slice(")
                && roles_updater_source.contains("READ_AHEAD_PAGES")
                && roles_updater_source.contains("RESOLVE_ALL_ITEMS_LIMIT")
                && !roles_updater_source.contains("prioritize_thumbnail_entries(")
                && !slot_schedule_body.contains(".virtual_entries")
                && !slot_schedule_body.contains("row_data(")
                && roles_updater_source.contains("match scope")
                && roles_updater_source.contains("match entry.is_dir()"),
            "coalesced zoom thumbnail/preview roles should live in the Dolphin-style roles updater and use Rust row tokens instead of touching the Slint row model"
        );
    }

    #[test]
    fn file_operation_completion_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_file_operation_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_file_action_result("))
            .map(|(body, _)| body)
            .expect("apply_file_operation_result body should be present");

        assert!(
            body.contains("state.complete_file_operation(")
                && body.contains("apply_undo_registration(ui, registration);")
                && body.contains("let command = request.command;")
                && body.contains("let reason = request.reason;")
                && body.contains(
                    "file_actions::request_privileged_action(ui, state, command, &reason);"
                )
                && body.contains("refresh_panes(ui, state, bridge, &summary.refresh_pane_ids);")
                && body.contains(
                    "set_status_for_panes(ui, state, &summary.refresh_pane_ids, &status);"
                ),
            "file operation completion status should write to the panes affected by the operation"
        );
        assert!(
            !body.contains("OperationResultDisposition")
                && !body.contains("register_transfer_undo(")
                && !body.contains("operation_final_status(")
                && !body.contains("FileUndo {")
                && !body.contains("matches!(operation, \"move\" | \"copy\" | \"link\")"),
            "file operation completion should delegate Undo, privilege, and status decisions to the controller"
        );
        assert!(
            !body.contains("set_status(ui, state, &status);"),
            "file operation completion status must not jump to whichever pane is focused when the async result returns"
        );
    }

    #[test]
    fn file_action_completion_state_is_controller_owned() {
        let source = include_str!("main.rs");
        let async_body = source
            .split_once("fn apply_async_event(")
            .and_then(|(_, rest)| rest.split_once("fn apply_directory_result("))
            .map(|(body, _)| body)
            .expect("apply_async_event body should be present");
        let result_body = source
            .split_once("fn apply_file_action_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_undo_registration("))
            .map(|(body, _)| body)
            .expect("apply_file_action_result body should be present");
        let file_actions_source = include_str!("fs/file_actions.rs");

        assert!(
            async_body.contains(
                "AsyncEvent::FileActionFinished(result) => {\n            apply_file_action_result(ui, state, bridge, result);\n        }"
            ) && !async_body.contains("file_actions::apply_file_action_result("),
            "async dispatch should route FileActionFinished through the local UI applier only"
        );
        assert!(
            result_body.contains("state.complete_file_action(result)")
                && result_body.contains("apply_undo_registration(ui, registration);")
                && result_body.contains("let command = request.command;")
                && result_body.contains("let reason = request.reason;")
                && result_body
                    .contains("file_actions::request_privileged_action(ui, state, command, &reason);")
                && result_body.contains(
                    "let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);"
                )
                && result_body.contains("set_status_for_panes(ui, state, &pane_ids, &status);"),
            "file action completion should consume the controller summary after releasing AppState borrow"
        );
        assert!(
            !result_body.contains("register_file_undo(")
                && !result_body.contains("FileUndo {")
                && !result_body.contains("format!(\"{action} complete:")
                && !result_body.contains("format!(\"{action} failed:"),
            "main.rs must not rebuild file action Undo/status decisions"
        );
        assert!(
            !file_actions_source.contains("FileActionApplyResult")
                && !file_actions_source.contains("fn file_action_apply_result(")
                && !file_actions_source.contains("pub(crate) fn apply_file_action_result("),
            "file_actions.rs should not keep a second action-result application path"
        );
    }

    #[test]
    fn file_operation_progress_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_file_operation_progress(")
            .and_then(|(_, rest)| rest.split_once("fn apply_privileged_operation_result("))
            .map(|(body, _)| body)
            .expect("apply_file_operation_progress body should be present");

        assert!(
            body.contains("set_status_for_panes(ui, state, &update.pane_ids, &update.status);"),
            "file operation progress status should write to the panes captured when the operation started"
        );
        assert!(
            body.contains(
                "let update = {\n        let mut state = state.borrow_mut();\n        state.file_operation_progress_update(&progress)\n    };"
            ) && !body.contains(
                "if let Some(update) = state.borrow_mut().file_operation_progress_update(&progress)"
            ),
            "file operation progress must release the mutable AppState borrow before updating pane status"
        );
        assert!(
            !body.contains("set_status(ui, state, &update.status);"),
            "file operation progress status must not jump to whichever pane is focused while progress events arrive"
        );
    }

    #[test]
    fn file_undo_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let production_source = source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .unwrap_or(source);
        let start_body = source
            .split_once("fn start_file_undo(")
            .and_then(|(_, rest)| rest.split_once("fn apply_file_undo_result("))
            .map(|(body, _)| body)
            .expect("start_file_undo body should be present");
        let result_body = source
            .split_once("fn apply_file_undo_result(")
            .and_then(|(_, rest)| rest.split_once("fn apply_device_mount_result("))
            .map(|(body, _)| body)
            .expect("apply_file_undo_result body should be present");

        assert!(
            start_body.contains("state.take_file_undo_start()")
                && start_body.contains("apply_undo_ui(ui, &summary.undo_ui);")
                && start_body.contains("FileUndoStartDecision::Started(summary) => summary")
                && start_body.contains("FileUndoStartDecision::Empty { status, undo_ui }")
                && start_body.contains("apply_undo_ui(ui, &undo_ui);")
                && start_body.contains(
                    "set_status_for_panes(ui, state, &summary.pane_ids, &summary.status);"
                ),
            "file undo start status should use the controller summary after releasing AppState borrow"
        );
        assert!(
            result_body.contains("state.complete_file_undo(result.undo, result.result)")
                && result_body.contains("cleanup_file_undo_backup(summary.cleanup_backup);")
                && result_body.contains("if let Some(undo_ui) = &summary.undo_ui {")
                && result_body.contains("apply_undo_ui(ui, undo_ui);")
                && result_body.contains(
                    "let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);"
                )
                && result_body
                    .contains("set_status_for_panes(ui, state, &pane_ids, &summary.status);")
                && result_body.matches("set_status_for_panes(").count() == 1,
            "file undo result status should use the controller completion summary after releasing AppState borrow"
        );
        assert!(
            !production_source.contains("fn file_undo_affected_dirs(")
                && !production_source.contains("fn restore_failed_file_undo(")
                && !production_source.contains("fn cleanup_file_undo_backup("),
            "file undo state decisions should live in operation_controller.rs, not main.rs"
        );
        assert!(
            !start_body.contains("last_undo.take()")
                && !start_body.contains("file_undo_affected_dirs(&undo)")
                && !start_body.contains("affected_directory_pane_ids(")
                && !start_body.contains("operation_finished_label(&undo.operation)"),
            "file undo start should not re-derive controller-owned state in main.rs"
        );
        assert!(
            !result_body.contains("file_undo_affected_dirs(")
                && !result_body.contains("restore_failed_file_undo(")
                && !result_body.contains("format!(\"Undo complete:")
                && !result_body.contains("format!(\"Undo failed:"),
            "file undo completion should not re-derive controller-owned state or status copy in main.rs"
        );
        assert!(
            !production_source.contains("fn sync_undo_ui(")
                && !production_source.contains("state.last_undo.is_some()")
                && !production_source.contains("operation_finished_label(&undo.operation)")
                && !production_source.contains("state.replace_file_undo("),
            "main.rs should apply controller-provided Undo UI state instead of reading last_undo directly"
        );
        assert!(
            !start_body.contains("set_status(\n        ui,\n        &format!(\"Undoing {}...\"")
                && !result_body.contains("set_status(ui, state, &format!(\"Undo complete: {message}\"))")
                && !result_body.contains(
                    "set_status(ui, state, &format!(\"Undo failed: {err}; Undo can be retried\"))"
                )
                && !result_body.contains(
                    "set_status(ui, state, &format!(\"Undo failed: {err}; newer Undo is available\"))"
                ),
            "file undo status must not jump to whichever pane is focused while the undo runs"
        );
    }

    #[test]
    fn file_open_status_uses_requesting_pane_route() {
        let source = include_str!("main.rs");
        let start_body = source
            .split_once("fn open_file_for_target_async(")
            .and_then(|(_, rest)| rest.split_once("fn apply_file_open_result("))
            .map(|(body, _)| body)
            .expect("open_file_for_target_async body should be present");
        let result_body = source
            .split_once("fn apply_file_open_result(")
            .and_then(|(_, rest)| rest.split_once("async fn open_default_with_privilege_fallback("))
            .map(|(body, _)| body)
            .expect("apply_file_open_result body should be present");

        assert!(
            start_body.contains(
                "set_status_for_panes(ui, state, &[pane_id], &format!(\"Opening {label}...\"));"
            ),
            "file-open start status should write to the pane that requested the open"
        );
        assert!(
            result_body.contains("state.complete_file_open(result)")
                && result_body.contains(
                    "if summary.external_edit_changed {\n        sync_external_edit_ui(ui, state);\n    }"
                )
                && result_body
                    .contains("set_status_for_panes(ui, state, &[summary.pane_id], &summary.status);")
                && result_body.matches("set_status_for_panes(").count() == 1,
            "file-open result status should consume the controller summary after releasing AppState borrow"
        );
        assert!(
            !result_body.contains("register_external_edit")
                && !result_body.contains("PaneTarget::Id(result.pane_id)")
                && !result_body.contains("result.result")
                && !result_body.contains("launch_status_suffix")
                && !result_body.contains(".launched_units")
                && !result_body.contains("success.external_edit")
                && !result_body.contains("format!(\"Opened with default app")
                && !result_body.contains("format!(\"Cannot open {label}: {err}\""),
            "file-open result status must not rebuild stale checks, launch bookkeeping, external-edit registration, or status copy in main.rs"
        );
    }

    #[test]
    fn context_service_menu_actions_are_pane_routed_and_model_backed() {
        let source = include_str!("main.rs");
        let production_source = source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .expect("main.rs should contain tests after production code");
        let controller_source = include_str!("app/pane_controller.rs");
        let item_route = source
            .split_once("fn request_item_view_entry_context_menu_at_point_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn select_all_visible("))
            .map(|(body, _)| body)
            .expect("item context menu routing body should be present");
        let blank_route_callback = source
            .split_once("routing.on_request_blank_context_menu")
            .and_then(|(_, rest)| rest.split_once("routing.on_zoom_in"))
            .map(|(body, _)| body)
            .expect("blank context menu routing body should be present");
        let blank_route = source
            .split_once("fn route_request_blank_context_menu")
            .and_then(|(_, rest)| rest.split_once("fn route_zoom_in"))
            .map(|(body, _)| body)
            .expect("blank context menu signal route body should be present");

        assert!(
            item_route.contains(
                "context_menu_entry_at_pane_point(\n            ui,\n            &state_ref,\n            request.slot,"
            )
                && item_route.contains("PaneController::new(ui, state, bridge)")
                && item_route.contains(".apply_item_view_controller_action(request.slot, action);")
                && controller_source.contains("pub(crate) struct PaneController")
                && controller_source
                    .contains("pub(crate) fn apply_item_view_controller_action(")
                && controller_source.contains("ItemViewControllerAction::RequestContextMenu")
                && controller_source.contains(
                    "select_path_for_slot(self.ui, self.state, slot, path, false, false);"
                )
                && controller_source.contains(
                    "context_service_menu::item_paths(self.state, slot, entry.path.as_str())"
                )
                && controller_source.contains("context_service_menu::refresh_actions_async(")
                && controller_source.contains("self.ui.invoke_route_pane_request_context_menu(")
                && !production_source.contains("fn apply_item_view_controller_action("),
            "item service menu discovery should be driven by PaneController for the pane slot that opened the context menu"
        );
        assert!(
            blank_route_callback.contains("bus.route_request_blank_context_menu(slot, x, y);")
                && blank_route.contains("context_service_menu::blank_paths(&self.state, slot)")
                && blank_route.contains("context_service_menu::refresh_actions_async(")
                && blank_route.contains("&ui,")
                && blank_route.contains("&self.state,")
                && blank_route.contains("&self.bridge,")
                && blank_route.contains("slot,\n                service_menu_paths,"),
            "blank-area service menu discovery should enter the signal bus route and use the pane slot that opened the context menu"
        );
    }

    #[test]
    fn privileged_operation_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_privileged_operation_result(")
            .and_then(|(_, rest)| rest.split_once("fn start_external_edit_resolution("))
            .map(|(body, _)| body)
            .expect("apply_privileged_operation_result body should be present");

        assert!(
            body.contains("state.complete_privileged_operation(result)")
                && body.contains(
                    "let pane_ids = refresh_affected_directories(ui, state, bridge, &summary.affected_dirs);"
                )
                && body.contains("set_status_for_panes(ui, state, &pane_ids, &summary.status);")
                && body.matches("set_status_for_panes(").count() == 1,
            "privileged operation result status should consume the controller summary after releasing AppState borrow"
        );
        assert!(
            !body.contains(
                "set_status(ui, state, &format!(\"{} complete: {message}\", result.label))"
            ) && !body
                .contains("set_status(ui, state, &format!(\"{} failed: {err}\", result.label))")
                && !body.contains("format!(\"{} complete: {message}\", result.label)")
                && !body.contains("format!(\"{} failed: {err}\", result.label)")
                && !body.contains("match result.result"),
            "privileged operation result status must not jump to the focused pane or rebuild success/failure copy in main.rs"
        );
    }

    #[test]
    fn admin_writeback_save_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_external_edit_result(")
            .and_then(|(_, rest)| rest.split_once("fn sync_external_edit_ui("))
            .map(|(body, _)| body)
            .expect("apply_external_edit_result body should be present");

        assert!(
            body.contains("state.complete_external_edit(result)")
                && body.contains(
                    "if summary.pending_changed {\n        sync_external_edit_ui(ui, state);\n    }"
                )
                && body.contains(
                    "refresh_affected_directories(ui, state, bridge, &summary.affected_dirs)"
                )
                && body.contains(
                    "let status_pane_ids = summary.status_pane_ids(&refreshed_pane_ids);"
                )
                && body.contains(
                    "set_status_for_panes(ui, state, &status_pane_ids, &summary.status);"
                )
                && body.matches("set_status_for_panes(").count() == 1,
            "admin write-back result status should consume the controller summary after releasing AppState borrow"
        );
        assert!(
            !body.contains(
                "set_status(ui, state, &format!(\"Admin write-back saved: {}\", path.display()))"
            ) && !body.contains(".external_edits\n            .retain")
                && !body.contains("match result.result")
                && !body.contains("format!(\"Admin write-back saved:")
                && !body.contains("format!(\"Admin write-back discarded:")
                && !body.contains("format!(\"{} failed: {err}\", result.operation)"),
            "admin write-back result status must not jump to focus or rebuild pending cleanup/status copy in main.rs"
        );
    }

    #[test]
    fn admin_writeback_pending_state_is_pane_local() {
        let edits = vec![
            test_pane_external_edit(7, "/etc/one.conf", "one"),
            test_pane_external_edit(11, "/etc/two.conf", "two"),
            test_pane_external_edit(11, "/etc/three.conf", "three"),
        ];

        assert_eq!(
            external_edit_status_for_pane(&edits, 7),
            "Admin write-back: one.conf"
        );
        assert_eq!(
            external_edit_status_for_pane(&edits, 11),
            "2 admin write-backs pending"
        );
        assert_eq!(external_edit_status_for_pane(&edits, 99), "");
    }

    fn test_pane_external_edit(pane_id: u64, original_path: &str, token: &str) -> PaneExternalEdit {
        PaneExternalEdit {
            pane_id,
            session: privilege::ExternalEditSession {
                original_path: PathBuf::from(original_path),
                scratch_path: PathBuf::from(format!("/tmp/{token}")),
                token: token.to_string(),
                unit: None,
            },
        }
    }

    #[test]
    fn admin_writeback_resolution_uses_pane_slot_and_pane_id() {
        let source = include_str!("main.rs");
        let start_body = source
            .split_once("fn start_external_edit_resolution(")
            .and_then(|(_, rest)| rest.split_once("fn apply_external_edit_result("))
            .map(|(body, _)| body)
            .expect("start_external_edit_resolution body should be present");
        let sync_body = source
            .split_once("fn sync_external_edit_ui(")
            .and_then(|(_, rest)| rest.split_once("fn external_edit_status_for_pane("))
            .map(|(body, _)| body)
            .expect("sync_external_edit_ui body should be present");

        assert!(
            start_body.contains("state.start_external_edit_resolution(slot, operation)")
                && start_body.contains("ExternalEditStartDecision::MissingPane { status }")
                && start_body
                    .contains("ExternalEditStartDecision::MissingPending { pane_id, status }")
                && start_body.contains("ExternalEditStartDecision::Started(summary) => summary")
                && start_body.contains(
                    "set_status_for_panes(ui, state, &[summary.pane_id], &summary.status);"
                )
                && start_body.contains("let pane_id = summary.pane_id;")
                && start_body.contains("let operation = summary.operation;")
                && start_body.contains("let session = summary.session;"),
            "admin write-back resolution should consume the controller start decision and then dispatch the returned session"
        );
        assert!(
            !start_body.contains("pane_id_for_slot")
                && !start_body.contains(".external_edits")
                && !start_body.contains("ui.set_external_edit_active")
                && !start_body.contains("ui.set_external_edit_status")
                && !start_body.contains("Saving admin write-back")
                && !start_body.contains("Discarding admin write-back")
                && !start_body.contains("No admin write-back is pending"),
            "admin write-back resolution must not rebuild pane lookup, pending-session selection, or start status copy in main.rs"
        );
        assert!(
            sync_body.contains("sync_pane_slots_ui(ui, state);"),
            "admin write-back UI should sync pane-local pending state via sync_pane_slots_ui"
        );
    }

    #[test]
    fn sidebar_prefetch_paths_skip_current_cached_empty_duplicates_and_unmounted_devices() {
        let mut state = AppState::new(
            PathBuf::from("/tmp/current"),
            vec![
                crate::fs::places::place_entry("Current", PathBuf::from("/tmp/current"), "C"),
                crate::fs::places::place_entry("Cached", PathBuf::from("/tmp/cached"), "A"),
                crate::fs::places::place_entry("Target", PathBuf::from("/tmp/target"), "T"),
                crate::fs::places::place_entry("Target Again", PathBuf::from("/tmp/target"), "T"),
                PlaceEntry {
                    label: "Empty".into(),
                    path: "".into(),
                    marker: "E".into(),
                    is_builtin: false,
                },
            ],
        );
        state.devices = vec![
            DeviceEntry {
                label: "USB".into(),
                path: "/run/media/yk/USB".into(),
                device_path: "/dev/sdb1".into(),
                kind: "removable-media".into(),
                marker: "U".into(),
                mounted: true,
                can_mount: false,
                can_unmount: true,
                can_eject: true,
                pending_action: "".into(),
                error: "".into(),
            },
            DeviceEntry {
                label: "Duplicate".into(),
                path: "/tmp/target".into(),
                device_path: "/dev/sdb2".into(),
                kind: "removable-media".into(),
                marker: "D".into(),
                mounted: true,
                can_mount: false,
                can_unmount: true,
                can_eject: false,
                pending_action: "".into(),
                error: "".into(),
            },
            DeviceEntry {
                label: "Unmounted".into(),
                path: "/dev/sdc1".into(),
                device_path: "/dev/sdc1".into(),
                kind: "removable-media".into(),
                marker: "U".into(),
                mounted: false,
                can_mount: true,
                can_unmount: false,
                can_eject: true,
                pending_action: "".into(),
                error: "".into(),
            },
        ];
        state.insert_directory_cache(
            PathBuf::from("/tmp/cached"),
            PreparedDirectoryEntries::new(vec![test_entry("a", "/tmp/a")]),
        );

        let expected = vec![
            PathBuf::from("/tmp/target"),
            PathBuf::from("/run/media/yk/USB"),
        ];
        assert_eq!(sidebar_prefetch_paths(&mut state), expected);
        assert!(state.directory_prefetch_pending.contains(&expected[0]));
        assert!(state.directory_prefetch_pending.contains(&expected[1]));
        assert!(sidebar_prefetch_paths(&mut state).is_empty());

        let state = Rc::new(RefCell::new(state));
        apply_directory_prefetch_result(
            &state,
            expected[0].clone(),
            Ok(PreparedDirectoryEntries::default()),
        );
        assert!(
            !state
                .borrow()
                .directory_prefetch_pending
                .contains(&expected[0])
        );

        apply_directory_prefetch_result(
            &state,
            expected[1].clone(),
            Err(io::Error::other("prefetch failed")),
        );
        assert!(
            !state
                .borrow()
                .directory_prefetch_pending
                .contains(&expected[1])
        );
    }

    #[test]
    fn directory_watch_paths_are_single_for_regular_directories() {
        assert_eq!(
            directory_watch_paths(Path::new("/tmp/project")),
            vec![PathBuf::from("/tmp/project")]
        );
    }

    #[test]
    fn directory_watch_paths_include_trash_metadata_directory() {
        let trash_files = fs::file_ops::trash_files_dir();
        let trash_info = fs::file_ops::trash_info_dir();

        assert_eq!(
            directory_watch_paths(&trash_files),
            vec![trash_files, trash_info]
        );
    }

    #[test]
    fn directory_watch_event_filter_keeps_write_close_and_rescan() {
        use notify::event::{
            AccessKind, AccessMode, CreateKind, DataChange, Flag, ModifyKind, RemoveKind,
        };

        assert!(directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Access(AccessKind::Close(AccessMode::Write))
        )));
        assert!(directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Access(AccessKind::Close(AccessMode::Any))
        )));
        assert!(directory_watch_event_should_reload(
            &notify::Event::new(notify::EventKind::Other).set_flag(Flag::Rescan)
        ));
        assert!(directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Create(CreateKind::File)
        )));
        assert!(directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Modify(ModifyKind::Data(DataChange::Any))
        )));
        assert!(directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Remove(RemoveKind::File)
        )));

        assert!(!directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Access(AccessKind::Read)
        )));
        assert!(!directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Access(AccessKind::Open(AccessMode::Read))
        )));
        assert!(!directory_watch_event_should_reload(&notify::Event::new(
            notify::EventKind::Access(AccessKind::Close(AccessMode::Read))
        )));
    }

    #[test]
    fn directory_watch_removed_paths_extracts_deletes_and_rename_sources() {
        use notify::event::{ModifyKind, RemoveKind, RenameMode};

        let watched = Path::new("/tmp/project");
        let deleted = notify::Event::new(notify::EventKind::Remove(RemoveKind::File))
            .add_path(PathBuf::from("/tmp/project/old.txt"));
        assert_eq!(
            directory_watch_removed_paths(&deleted, watched),
            vec![PathBuf::from("/tmp/project/old.txt")]
        );

        let renamed_from = notify::Event::new(notify::EventKind::Modify(ModifyKind::Name(
            RenameMode::From,
        )))
        .add_path(PathBuf::from("/tmp/project/before.txt"));
        assert_eq!(
            directory_watch_removed_paths(&renamed_from, watched),
            vec![PathBuf::from("/tmp/project/before.txt")]
        );

        let renamed_both = notify::Event::new(notify::EventKind::Modify(ModifyKind::Name(
            RenameMode::Both,
        )))
        .add_path(PathBuf::from("/tmp/project/before.txt"))
        .add_path(PathBuf::from("/tmp/project/after.txt"));
        assert_eq!(
            directory_watch_removed_paths(&renamed_both, watched),
            vec![PathBuf::from("/tmp/project/before.txt")]
        );

        let current_dir_removed = notify::Event::new(notify::EventKind::Remove(RemoveKind::Folder))
            .add_path(PathBuf::from("/tmp/project"));
        assert!(directory_watch_removed_paths(&current_dir_removed, watched).is_empty());
    }

    #[test]
    fn directory_removed_path_set_detects_paths_missing_from_reload() {
        let current = PreparedDirectoryEntries::new(vec![
            test_entry("keep.txt", "/tmp/keep.txt"),
            test_entry("remove.txt", "/tmp/remove.txt"),
            test_entry("other.txt", "/tmp/other.txt"),
        ]);
        let incoming = PreparedDirectoryEntries::new(vec![
            test_entry("keep.txt", "/tmp/keep.txt"),
            test_entry("other.txt", "/tmp/other.txt"),
        ]);

        assert_eq!(
            directory_removed_path_set(&current.entries, &incoming),
            HashSet::from(["/tmp/remove.txt".to_string()])
        );
    }

    #[test]
    fn directory_reload_diff_classifies_dolphin_index_deltas() {
        let current = PreparedDirectoryEntries::new(vec![
            test_entry("alpha.txt", "/tmp/alpha.txt"),
            test_entry("charlie.txt", "/tmp/charlie.txt"),
            test_entry("delta.txt", "/tmp/delta.txt"),
        ]);
        let inserted = PreparedDirectoryEntries::new(vec![
            test_entry("alpha.txt", "/tmp/alpha.txt"),
            test_entry("bravo.txt", "/tmp/bravo.txt"),
            test_entry("charlie.txt", "/tmp/charlie.txt"),
            test_entry("delta.txt", "/tmp/delta.txt"),
            test_entry("echo.txt", "/tmp/echo.txt"),
        ]);

        let diff = directory_reload_diff(&current.entries, &inserted);

        assert!(diff.removed_paths.is_empty());
        assert_eq!(
            diff.inserted_width_ranges
                .iter()
                .map(|(index, widths)| (*index, widths.len()))
                .collect::<Vec<_>>(),
            vec![(1, 1), (4, 1)]
        );
        assert!(diff.supports_index_delta_relayout());

        let renamed = PreparedDirectoryEntries::new(vec![
            test_entry("alpha.txt", "/tmp/alpha.txt"),
            test_entry("old-name.txt", "/tmp/old-name.txt"),
            test_entry("delta.txt", "/tmp/delta.txt"),
        ]);
        let diff = directory_reload_diff(&inserted.entries, &renamed);

        assert_eq!(
            diff.removed_paths,
            HashSet::from([
                "/tmp/bravo.txt".to_string(),
                "/tmp/charlie.txt".to_string(),
                "/tmp/echo.txt".to_string(),
            ])
        );
        assert_eq!(
            diff.inserted_width_ranges
                .iter()
                .map(|(index, widths)| (*index, widths.len()))
                .collect::<Vec<_>>(),
            vec![(1, 1)]
        );
        assert!(diff.supports_index_delta_relayout());
    }

    #[test]
    fn directory_reload_diff_rejects_reordered_or_resized_retained_entries() {
        let current = PreparedDirectoryEntries::new(vec![
            test_entry("alpha.txt", "/tmp/alpha.txt"),
            test_entry("bravo.txt", "/tmp/bravo.txt"),
            test_entry("charlie.txt", "/tmp/charlie.txt"),
        ]);
        let reordered = PreparedDirectoryEntries::new(vec![
            test_entry("bravo.txt", "/tmp/bravo.txt"),
            test_entry("alpha.txt", "/tmp/alpha.txt"),
            test_entry("charlie.txt", "/tmp/charlie.txt"),
        ]);

        let diff = directory_reload_diff(&current.entries, &reordered);

        assert!(!diff.retained_order_preserved);
        assert!(!diff.supports_index_delta_relayout());

        let resized = PreparedDirectoryEntries::new(vec![
            test_entry(
                "alpha-with-a-much-longer-visible-name.txt",
                "/tmp/alpha.txt",
            ),
            test_entry("bravo.txt", "/tmp/bravo.txt"),
            test_entry("charlie.txt", "/tmp/charlie.txt"),
        ]);

        let diff = directory_reload_diff(&current.entries, &resized);

        assert!(diff.retained_order_preserved);
        assert!(!diff.retained_widths_unchanged);
        assert!(!diff.supports_index_delta_relayout());
    }

    #[test]
    fn directory_watchers_are_pane_scoped_and_split_lifecycle_owned() {
        let async_bridge = include_str!("app/async_bridge.rs");
        let main = include_str!("main.rs");
        let split_view = include_str!("app/split_view.rs");
        let watch_body = main
            .split_once("pub(crate) fn watch_current_directory(")
            .and_then(|(_, rest)| rest.split_once("fn directory_watch_paths("))
            .map(|(body, _)| body)
            .expect("watch_current_directory body should be present");
        let split_toggle_body = split_view
            .split_once("pub(crate) fn toggle_split_view(")
            .and_then(|(_, rest)| rest.split_once("#[derive(Debug)]"))
            .map(|(body, _)| body)
            .expect("toggle_split_view body should be present");

        assert!(
            async_bridge.contains(
                "directory_watchers: Rc<RefCell<HashMap<u64, notify::RecommendedWatcher>>>"
            ) && main.contains("directory_watchers: Rc::new(RefCell::new(HashMap::new()))")
                && async_bridge.contains("directory_read_trackers:")
                && async_bridge.contains("Arc<Mutex<DirectoryReadTracker>>")
                && main.contains("directory_read_trackers: Rc::new(RefCell::new(HashMap::new()))")
                && !async_bridge.contains("directory_watcher:")
                && !async_bridge.contains("directory_watch_debounce"),
            "directory watchers and read trackers should be keyed by stable pane id, not stored as one global watcher"
        );
        assert!(
            watch_body.contains("let debounce = Arc::new(AtomicU64::new(0));")
                && watch_body.contains("directory_watch_event_should_reload(&event)")
                && watch_body.contains("begin_directory_read_request(&read_tracker, generation)")
                && watch_body.contains(".insert(pane_id, watcher);")
                && watch_body.matches("remove(&pane_id)").count() >= 2
                && !watch_body.contains("bridge.directory_watch_debounce"),
            "each pane watcher should own debounce state, use the precise event filter, and sequence directory reads"
        );
        assert!(
            main.contains(
                "directory_read_result_is_current(result.pane_id, result.generation, result.request, bridge)"
            ) && main.contains("bridge.directory_read_trackers.borrow_mut().remove(&pane_id);"),
            "directory results should reject stale same-generation reads and closed panes should drop tracker state"
        );
        assert!(
            split_toggle_body.contains("crate::unwatch_directory_for_pane(pane_id, bridge);")
                && split_toggle_body.contains("pane.load_generation.current()")
                && split_toggle_body.contains(
                    "crate::watch_current_directory(&current_dir, pane_id, generation, bridge);"
                ),
            "split view open/close should keep pane-scoped directory watchers in sync with pane lifetime"
        );
    }

    fn test_entry(name: &str, path: &str) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: path.into(),
            group: String::new().into(),
            location: String::new().into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
        }
    }
}
