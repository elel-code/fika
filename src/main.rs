use slint::{
    CloseRequestResponse, ComponentHandle, LogicalSize, Model, ModelRc, SharedString, Timer,
    TimerMode, VecModel,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc;
use std::time::Duration;

mod app;
mod config;
mod desktop;
mod fs;
mod support;

use app::async_bridge::{AsyncBridge, build_async_runtime, send_async_event};
use app::chooser::{
    ChooserOutputMetadata, chooser_output_metadata, parse_chooser_choice_spec,
    parse_chooser_filter_spec, safe_child_path, selected_directory_or_current,
    set_chooser_choice_index,
};
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
    AsyncEvent, DeviceActionResult, DeviceMountResult, DevicesLoadedResult, DirectoryLoadResult,
    ExternalEditResult, FileOpenResult, FileOpenSuccess, FileOperationProgress,
    FileOperationResult, FileUndoResult, RecursiveSearchProgress, RecursiveSearchResult,
    VirtualViewResult,
};
use app::file_clipboard::{
    apply_clipboard_load_result, refresh_clipboard_availability_async, sync_clipboard_ui,
};
use app::geometry::{
    MainGridLayout, SelectionRect, active_main_pane_width, clamped_split_pane_ratio,
    inactive_main_pane_width, place_drop_geometry, register_menu_geometry_callbacks,
    virtual_grid_plan,
};
use app::model_update::{update_file_entries_model_selection, update_pane_file_entries_model};
use app::operation_controller::{
    OperationResultDisposition, affected_directory_pane_ids, operation_final_status,
    operation_finished_label,
};
use app::pane::{DirectoryViewState, PaneState, PaneTarget, PreparedDirectoryEntries};
#[cfg(test)]
use app::pane::{PaneEntrySnapshot, PaneHistory};
use app::places::{
    add_place, add_place_at_slot, contains_place_path, open_place_new_window, remove_place,
    rename_place, reorder_place_path, restore_default_places, sync_places,
};
use app::search_ui::{
    cancel_active_search, recursive_search_cancelled_status, recursive_search_finished_status,
    recursive_search_progress_status, recursive_search_status, search_filters_active,
    set_search_filters,
};
use app::selection::{
    annotate_selection_state, append_unique_paths, filtered_entry_count,
    filtered_entry_paths_for_slot, rebuild_visible_entry_index, retained_visible_paths,
    selection_range_paths_filtered_for_slot, selection_rect_paths_filtered_for_slot,
};
use app::split_view::{
    directory_status_text, pane_viewport_x_from_ui, set_pane_viewport_ui,
    set_pane_viewport_ui_if_clamped, sync_focus_navigation_ui, sync_navigation_ui,
    sync_pane_slot_ui, sync_pane_slots_ui, toggle_split_view,
};
use app::state::{AppState, DeviceAction, FileUndo, PaneExternalEdit};
use app::thumbnail_pipeline::{
    apply_thumbnail_load_to_state_for_pane, decorate_entries_with_cached_thumbnails_for_pane,
    prioritize_thumbnail_entries, thumbnail_schedule_batch_for_pane,
};
use app::transfer::{
    cancel_queued_operations, pane_drop_allowed, pane_drop_target_path, place_drop_allowed,
    prepare_current_dir_transfer, prepare_entry_transfer, prepare_pane_transfer,
    prepare_place_transfer, resolve_transfer_conflict, start_next_operation,
    start_transfer_operation,
};
use app::virtual_view::{VirtualViewSnapshotInput, prepare_virtual_view_snapshot_update};
use config::args::{Args, Mode};
use config::paths::{expand_user_path, home_dir, normalize_start_dir};
use config::settings::{AppSettings, load_settings, save_settings};
use desktop::{mime_open, open_with, terminal};
use fs::devices::{
    device_diagnostics_report, eject_device, mount_device, mounted_devices, unmount_device,
};
use fs::entries::read_entries_async;
use fs::places::default_places;
use fs::{file_actions, privilege, search, thumbnails};

slint::include_modules!();

const EXTERNAL_EDIT_SAVE_OPERATION: &str = "Admin Save";
const EXTERNAL_EDIT_DISCARD_OPERATION: &str = "Discard";
const PANE_VIEW_SYNC_COALESCE: Duration = Duration::from_millis(8);
const THUMBNAIL_FLUSH_COALESCE: Duration = Duration::from_millis(16);

struct PaneViewSyncScheduler {
    timer: Timer,
    pending_slots: Rc<RefCell<Vec<i32>>>,
}

impl PaneViewSyncScheduler {
    fn new(ui: slint::Weak<AppWindow>, state: Rc<RefCell<AppState>>, bridge: AsyncBridge) -> Self {
        let timer = Timer::default();
        let pending_slots = Rc::new(RefCell::new(Vec::<i32>::new()));
        let timer_slots = Rc::clone(&pending_slots);

        timer.start(TimerMode::SingleShot, PANE_VIEW_SYNC_COALESCE, move || {
            let Some(ui) = ui.upgrade() else {
                return;
            };
            let slots = {
                let mut pending = timer_slots.borrow_mut();
                pending.sort_unstable();
                pending.dedup();
                pending.drain(..).collect::<Vec<_>>()
            };
            for slot in slots {
                sync_pane_viewport_for_slot(&ui, &state, &bridge, slot);
            }
        });
        timer.stop();

        Self {
            timer,
            pending_slots,
        }
    }

    fn request(&self, slot: i32) {
        let mut pending = self.pending_slots.borrow_mut();
        if !pending.contains(&slot) {
            pending.push(slot);
        }
        drop(pending);
        self.timer.restart();
    }

    fn flush_all(&self) {
        self.timer.stop();
        self.pending_slots.borrow_mut().clear();
    }
}

struct ThumbnailFlushScheduler {
    timer: Timer,
    pending: Rc<RefCell<VecDeque<(u64, u64, thumbnails::ThumbnailLoad)>>>,
}

enum VirtualViewSyncRequest {
    Cached {
        viewport_x: f32,
        viewport_clamped: bool,
    },
    Prepare {
        pane_id: u64,
        generation: u64,
        rows_per_column: usize,
        cell_width: f32,
        input: VirtualViewSnapshotInput,
    },
}

impl ThumbnailFlushScheduler {
    fn new(ui: slint::Weak<AppWindow>, state: Rc<RefCell<AppState>>, bridge: AsyncBridge) -> Self {
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
            flush_thumbnail_results(&ui, &state, &bridge, &timer_pending);
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

    let ui = AppWindow::new()?;

    // ── DndApi bridge ──────────────────────────────────────────────
    // Maps Slint's opaque `data-transfer` ↔ `DropEvent` ↔ our internal drag info.
    {
        use slint::DataTransfer;
        use slint::language::DropEvent;
        use std::rc::Rc;

        #[derive(Clone, Debug)]
        enum FikaDragInfo {
            Place(String),
            Folder(String),
            File(String),
        }

        let dnd_api = ui.global::<DndApi>();

        // ── DragArea.data constructors ──────────────────────────
        dnd_api.on_make_drag_place(|path: SharedString| -> DataTransfer {
            let mut dt = DataTransfer::default();
            dt.set_user_data(Rc::new(FikaDragInfo::Place(path.to_string())));
            dt
        });
        dnd_api.on_make_drag_folder(|path: SharedString| -> DataTransfer {
            let mut dt = DataTransfer::default();
            dt.set_user_data(Rc::new(FikaDragInfo::Folder(path.to_string())));
            dt
        });
        dnd_api.on_make_drag_file(|path: SharedString| -> DataTransfer {
            let mut dt = DataTransfer::default();
            dt.set_user_data(Rc::new(FikaDragInfo::File(path.to_string())));
            dt
        });

        // ── DropEvent inspectors ────────────────────────────────
        dnd_api.on_event_kind(|event: DropEvent| -> DragKind {
            if let Some(rc) = event.data.user_data() {
                match rc.downcast_ref::<FikaDragInfo>() {
                    Some(FikaDragInfo::Place(_)) => return DragKind::Place,
                    Some(FikaDragInfo::Folder(_)) => return DragKind::Folder,
                    Some(FikaDragInfo::File(_)) => return DragKind::File,
                    None => {}
                }
            }
            DragKind::Unsupported
        });

        dnd_api.on_event_path(|event: DropEvent| -> SharedString {
            if let Some(rc) = event.data.user_data()
                && let Some(info) = rc.downcast_ref::<FikaDragInfo>()
            {
                return match info {
                    FikaDragInfo::Place(p) | FikaDragInfo::Folder(p) | FikaDragInfo::File(p) => {
                        SharedString::from(p.as_str())
                    }
                };
            }
            SharedString::new()
        });
    }
    register_pane_routing_callbacks(&ui);
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
        ui.set_icon_zoom_level(icon_zoom_level.clamp(0, 4));
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
        directory_watcher: Rc::new(RefCell::new(None)),
        directory_watch_debounce: Arc::new(AtomicU64::new(0)),
        device_watch_debounce: Arc::new(AtomicU64::new(0)),
    };
    sync_devices(&ui, &state);
    refresh_devices_async(&state, &bridge);
    refresh_clipboard_availability_async(&state, &bridge);
    start_device_monitor(&bridge);
    let pane_view_sync = Rc::new(PaneViewSyncScheduler::new(
        ui.as_weak(),
        Rc::clone(&state),
        bridge.clone(),
    ));
    let thumbnail_flush = Rc::new(ThumbnailFlushScheduler::new(
        ui.as_weak(),
        Rc::clone(&state),
        bridge.clone(),
    ));

    let async_rx = Rc::new(RefCell::new(async_rx));
    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let async_rx = Rc::clone(&async_rx);
        let bridge = bridge.clone();
        let thumbnail_flush = Rc::clone(&thumbnail_flush);
        ui.on_async_results_ready(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };

            while let Ok(event) = async_rx.borrow_mut().try_recv() {
                apply_async_event(&ui, &state, &bridge, &thumbnail_flush, event);
            }
        });
    }

    {
        let pane_view_sync = Rc::clone(&pane_view_sync);
        ui.on_pane_view_changed(move |slot| {
            pane_view_sync.request(slot);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        let pane_view_sync = Rc::clone(&pane_view_sync);
        ui.on_pane_layout_changed(move || {
            if let Some(ui) = ui_weak.upgrade() {
                pane_view_sync.flush_all();
                sync_visible_pane_layouts(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_slots_refresh_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                sync_pane_slots_ui(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_path_text_changed(move |slot, text| {
            if let Some(pane) = state.borrow_mut().panes.pane_mut_for_slot(slot) {
                pane.path_input_text = text.to_string();
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_pane_slot_ui(&ui, &state, slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_path_focus_changed(move |slot, focused| {
            if let Some(pane) = state.borrow_mut().panes.pane_mut_for_slot(slot) {
                pane.path_focused = focused;
            }
            if let Some(ui) = ui_weak.upgrade() {
                if focused {
                    focus_pane_slot(&ui, &state, slot);
                }
                sync_pane_slot_ui(&ui, &state, slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_slot_refresh_requested(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                sync_pane_slot_ui(&ui, &state, slot);
            }
        });
    }

    {
        let state = Rc::clone(&state);
        ui.on_pane_viewport_changed(move |slot, viewport_x| {
            if let Some(pane) = state.borrow_mut().panes.pane_mut_for_slot(slot) {
                pane.view.viewport_x = viewport_x;
            }
        });
    }

    load_directory(&ui, &state, &bridge);
    sync_navigation_ui(&ui, &state);
    prefetch_sidebar_locations_async(&state, &bridge);

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_refresh(move || {
            if let Some(ui) = ui_weak.upgrade() {
                refresh_focused_directory(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_home(move || {
            if let Some(ui) = ui_weak.upgrade() {
                navigate_focused_to(&ui, &state, &bridge, home_dir());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_parent(move || {
            if let Some(ui) = ui_weak.upgrade() {
                go_parent(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_root(move || {
            if let Some(ui) = ui_weak.upgrade() {
                navigate_focused_to(&ui, &state, &bridge, PathBuf::from("/"));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_pane_path_submitted(move |slot, path| {
            if let Some(ui) = ui_weak.upgrade() {
                focus_pane_slot(&ui, &state, slot);
                let requested = expand_user_path(path.as_str());
                if !requested.is_dir() {
                    reset_pane_path_input_for_slot(&ui, slot);
                    set_status(&ui, &state, "Path is not a readable directory");
                    return;
                }
                navigate_pane_to_slot(&ui, &state, &bridge, slot, requested);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_open_place(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let slot = focus_current_ui_pane_slot(&ui, &state);
                let requested = expand_user_path(path.as_str());
                if fs::file_ops::is_trash_files_dir(&requested) {
                    match fs::file_ops::ensure_trash_dirs() {
                        Ok(()) => navigate_pane_to_slot(&ui, &state, &bridge, slot, requested),
                        Err(err) => {
                            set_status(&ui, &state, &format!("Trash is not available: {err}"))
                        }
                    }
                } else if requested.is_dir() {
                    navigate_pane_to_slot(&ui, &state, &bridge, slot, requested);
                } else {
                    set_status(&ui, &state, "Place is not available");
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_open_device(move |path, mounted| {
            if let Some(ui) = ui_weak.upgrade() {
                if !mounted {
                    let device_path = path.to_string();
                    if register_pending_device_action(&state, &device_path, "mount") {
                        set_status(&ui, &state, "Mounting device...");
                        mount_device_async(&bridge, device_path);
                    } else {
                        set_status(&ui, &state, "Device action already in progress");
                    }
                    return;
                }
                let requested = expand_user_path(path.as_str());
                if requested.is_dir() {
                    let slot = focus_current_ui_pane_slot(&ui, &state);
                    navigate_pane_to_slot(&ui, &state, &bridge, slot, requested);
                } else {
                    set_status(&ui, &state, "Device is not available");
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_unmount_device(move |device_path, mount_path| {
            if let Some(ui) = ui_weak.upgrade() {
                let device_path = device_path.to_string();
                let mount_path = mounted_device_path(mount_path.as_str());
                if register_pending_device_action(&state, &device_path, "unmount") {
                    set_status(&ui, &state, "Unmounting device...");
                    device_action_async(&bridge, "unmount", device_path, mount_path);
                } else {
                    set_status(&ui, &state, "Device action already in progress");
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_eject_device(move |device_path, mount_path| {
            if let Some(ui) = ui_weak.upgrade() {
                let device_path = device_path.to_string();
                let mount_path = mounted_device_path(mount_path.as_str());
                if register_pending_device_action(&state, &device_path, "eject") {
                    set_status(&ui, &state, "Ejecting device...");
                    device_action_async(&bridge, "eject", device_path, mount_path);
                } else {
                    set_status(&ui, &state, "Device action already in progress");
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_search_submitted(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                submit_search(&ui, &state, &bridge, query.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_cancel_search(move || {
            if let Some(ui) = ui_weak.upgrade() {
                cancel_recursive_search(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_search_filters_changed(move |kind, modified, size| {
            if let Some(ui) = ui_weak.upgrade() {
                update_search_filters(&ui, &state, &bridge, kind, modified, size);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_open_path(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                open_path(&ui, &state, path.as_str(), &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let async_handle = async_handle.clone();
        let state = Rc::clone(&state);
        ui.on_open_terminal_here(move |dir| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let dir = PathBuf::from(dir.as_str());
            set_status(
                &ui,
                &state,
                &format!("Opening terminal in {}...", dir.display()),
            );
            let ui_weak = ui.as_weak();
            async_handle.spawn(async move {
                let label = dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| dir.to_str().unwrap_or("folder"))
                    .to_string();
                let result =
                    tokio::task::spawn_blocking(move || terminal::open_terminal_here(&dir))
                        .await
                        .map_err(|err| format!("terminal launch task failed: {err}"))
                        .and_then(|result| result);
                let message = match result {
                    Ok(launch) => match (launch.unit, launch.diagnostic) {
                        (Some(unit), _) => format!("Terminal opened in {label} ({unit})"),
                        (None, Some(diagnostic)) => {
                            format!("Terminal opened in {label}; {diagnostic}")
                        }
                        (None, None) => format!("Terminal opened in {label}"),
                    },
                    Err(err) => format!("Cannot open terminal: {err}"),
                };
                let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                    ui.set_status(message.into());
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let save_files = args.chooser_save_files.clone();
        ui.on_chooser_accept(move |name| {
            if let Some(ui) = ui_weak.upgrade() {
                chooser_accept(&ui, &state, name.as_str(), &save_files);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_chooser_select_filter(move |filter_index| {
            if let Some(ui) = ui_weak.upgrade() {
                select_chooser_filter(&ui, &state, &bridge, filter_index);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_chooser_select_choice(move |choice_index, option_index| {
            if let Some(ui) = ui_weak.upgrade() {
                select_chooser_choice(&ui, &state, choice_index, option_index);
            }
        });
    }

    open_with::register_callbacks(&ui, &state, &bridge);
    app::file_clipboard::register_callbacks(&ui, &state, &bridge);
    file_actions::register_callbacks(&ui, &state, &bridge);

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_back(move || {
            if let Some(ui) = ui_weak.upgrade() {
                go_back(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_forward(move || {
            if let Some(ui) = ui_weak.upgrade() {
                go_forward(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_pane_go_back(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                go_pane_back_slot(&ui, &state, &bridge, slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_pane_go_forward(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                go_pane_forward_slot(&ui, &state, &bridge, slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_focus(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                focus_pane_slot(&ui, &state, slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_toggle_split_view(move || {
            if let Some(ui) = ui_weak.upgrade() {
                toggle_split_view(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_pane_activated(move |slot, path| {
            if let Some(ui) = ui_weak.upgrade() {
                open_path_for_slot(&ui, &state, slot, path.as_str(), &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_request_select(move |slot, path, toggle, range| {
            if let Some(ui) = ui_weak.upgrade() {
                select_path_for_slot(&ui, &state, slot, path.as_str(), toggle, range);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_select_rect(
            move |slot,
                  x1,
                  y1,
                  x2,
                  y2,
                  rows_per_column,
                  cell_width,
                  row_height,
                  padding,
                  toggle| {
                if let Some(ui) = ui_weak.upgrade() {
                    select_rect_for_slot(
                        &ui,
                        &state,
                        slot,
                        SelectionRect {
                            x1,
                            y1,
                            x2,
                            y2,
                            rows_per_column,
                            cell_width,
                            row_height,
                            padding,
                        },
                        toggle,
                    );
                }
            },
        );
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_clear_selection(move || {
            if let Some(ui) = ui_weak.upgrade() {
                clear_selection(&ui, &state);
            }
        });
    }

    {
        let state = Rc::clone(&state);
        ui.on_pane_is_selected(move |slot, path| {
            let state = state.borrow();
            state.panes.pane_for_slot(slot).is_some_and(|pane| {
                pane.selection
                    .paths
                    .iter()
                    .any(|selected| selected == path.as_str())
            })
        });
    }

    {
        let state = Rc::clone(&state);
        ui.on_is_place(move |path| contains_place_path(&state.borrow(), path.as_str()));
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_select_all_visible(move || {
            if let Some(ui) = ui_weak.upgrade() {
                select_all_visible(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_add_place(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                add_place(&ui, &state, PathBuf::from(path.as_str()));
                prefetch_sidebar_locations_async(&state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_add_place_at_slot(move |path, slot| {
            if let Some(ui) = ui_weak.upgrade() {
                add_place_at_slot(&ui, &state, PathBuf::from(path.as_str()), slot);
                prefetch_sidebar_locations_async(&state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_rename_place(move |index, label| {
            if let Some(ui) = ui_weak.upgrade() {
                rename_place(&ui, &state, index, label.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_remove_place(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                remove_place(&ui, &state, index);
                prefetch_sidebar_locations_async(&state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_restore_default_places(move || {
            if let Some(ui) = ui_weak.upgrade() {
                restore_default_places(&ui, &state);
                prefetch_sidebar_locations_async(&state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_open_place_new_window(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                open_place_new_window(&ui, &state, index);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_prepare_place_transfer(move |source, target_index, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_place_transfer(&ui, &state, source.as_str(), target_index, x, y)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_prepare_entry_transfer(move |source, target_index, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_entry_transfer(&ui, &state, source.as_str(), target_index, x, y)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_prepare_current_dir_transfer(move |source, label, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_current_dir_transfer(&ui, &state, source.as_str(), label.as_str(), x, y)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_prepare_transfer(move |slot, source, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_pane_transfer_for_slot(&ui, &state, slot, source.as_str(), x, y)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_drop_target_path(move |slot, x, y, source| {
            let Some(ui) = ui_weak.upgrade() else {
                return SharedString::new();
            };
            let state = state.borrow();
            pane_drop_target_path_for_slot(&ui, &state, slot, x, y, Path::new(source.as_str()))
                .map_or_else(SharedString::new, Into::into)
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_pane_drop_allowed(move |slot, x, y, source| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            pane_drop_allowed_for_slot(&ui, &state, slot, x, y, Path::new(source.as_str()))
        });
    }

    {
        let state = Rc::clone(&state);
        ui.on_place_drop_allowed(move |source, target_index| {
            let state = state.borrow();
            place_drop_allowed(&state, Path::new(source.as_str()), target_index)
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_target(move |y| {
            let Some(ui) = ui_weak.upgrade() else {
                return -1;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
            )
            .target_index
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_slot(move |y| {
            let Some(ui) = ui_weak.upgrade() else {
                return 0;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
            )
            .slot
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_over_gap(move |y| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
            )
            .over_gap
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_over_item(move |y| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
            )
            .over_item
        });
    }

    ui.on_trace_places_drop(
        |phase, mime_type, payload, x, y, slot, target, over_gap, over_item| {
            dnd_log_places_event(PlacesDndTrace {
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
            });
        },
    );
    ui.on_trace_main_drop(|phase, mime_type, payload, x, y, rejected, target_path| {
        dnd_log_main_event(MainDndTrace {
            backend: SLINT_DROPAREA_BACKEND_SOURCE,
            phase: phase.as_str(),
            mime_type: mime_type.as_str(),
            payload: payload.as_str(),
            x,
            y,
            rejected,
            target_path: target_path.as_str(),
        });
    });

    register_menu_geometry_callbacks(&ui);

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        let state = Rc::clone(&state);
        ui.on_transfer_operation(move |operation, source, target| {
            if let Some(ui) = ui_weak.upgrade() {
                start_transfer_operation(
                    &ui,
                    &state,
                    &bridge,
                    operation.as_str(),
                    source.as_str(),
                    target.as_str(),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_transfer_conflict_choice(move |decision| {
            if let Some(ui) = ui_weak.upgrade() {
                resolve_transfer_conflict(&ui, &state, &bridge, decision.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_privileged_prompt_accept(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let command = state.borrow_mut().pending_privileged_command.take();
                ui.set_privileged_prompt_open(false);
                if let Some(command) = command {
                    start_privileged_operation(&ui, &state, &bridge, command);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_privileged_prompt_dismiss(move || {
            state.borrow_mut().pending_privileged_command = None;
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_privileged_prompt_open(false);
                set_status(&ui, &state, "Administrator operation cancelled");
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_commit_external_edit(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                start_external_edit_resolution(
                    &ui,
                    &state,
                    &bridge,
                    slot,
                    EXTERNAL_EDIT_SAVE_OPERATION,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_discard_external_edit(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                start_external_edit_resolution(
                    &ui,
                    &state,
                    &bridge,
                    slot,
                    EXTERNAL_EDIT_DISCARD_OPERATION,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_undo_last_operation(move || {
            if let Some(ui) = ui_weak.upgrade() {
                start_file_undo(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_cancel_queued_operations(move || {
            if let Some(ui) = ui_weak.upgrade() {
                cancel_queued_operations(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_reorder_place_path(move |path, to| {
            if let Some(ui) = ui_weak.upgrade() {
                reorder_place_path(&ui, &state, path.as_str(), to);
                prefetch_sidebar_locations_async(&state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_persist_ui_state(move || {
            if let Some(ui) = ui_weak.upgrade() {
                save_current_settings(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let chooser_mode = matches!(args.mode, Mode::Chooser);
        ui.window().on_close_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                save_current_settings(&ui, &state);
            }
            if chooser_mode {
                std::process::exit(support::chooser::CHOOSER_CANCEL_EXIT_CODE);
            }
            CloseRequestResponse::HideWindow
        });
    }

    ui.run()
}

fn register_pane_routing_callbacks(ui: &AppWindow) {
    let routing = ui.global::<PaneRouting>();

    {
        let ui_weak = ui.as_weak();
        routing.on_focus(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_focus(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_path_submitted(move |slot, path| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_path_submitted(slot, path);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_go_back(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_go_back(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_go_forward(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_go_forward(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_search_submitted(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_search_submitted(query);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_cancel_search(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_cancel_search();
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_search_filters_changed(move |kind, modified, size| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_search_filters_changed(kind, modified, size);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_search_close_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_search_bar_open(false);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_view_changed(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_view_changed(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_activated(move |slot, path| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_activated(slot, path);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_request_select(move |slot, path, toggle, range| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_request_select(slot, path, toggle, range);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_select_rect(
            move |slot,
                  x1,
                  y1,
                  x2,
                  y2,
                  rows_per_column,
                  cell_width,
                  row_height,
                  padding,
                  toggle| {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.invoke_route_pane_select_rect(
                        slot,
                        x1,
                        y1,
                        x2,
                        y2,
                        rows_per_column,
                        cell_width,
                        row_height,
                        padding,
                        toggle,
                    );
                }
            },
        );
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_clear_selection(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_clear_selection(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_request_context_menu(move |slot, path, name, size, modified, is_dir, x, y| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_request_context_menu(
                    slot, path, name, size, modified, is_dir, x, y,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_request_blank_context_menu(move |slot, x, y| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_request_blank_context_menu(slot, x, y);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_zoom_in(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_zoom_in(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_zoom_out(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_zoom_out(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_drop_target_path(move |slot, x, y, source| {
            ui_weak.upgrade().map_or_else(SharedString::new, |ui| {
                ui.invoke_route_pane_drop_target_path(slot, x, y, source)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_drop_allowed(move |slot, x, y, source| {
            ui_weak
                .upgrade()
                .is_some_and(|ui| ui.invoke_route_pane_drop_allowed(slot, x, y, source))
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_prepare_transfer(move |slot, source, x, y| {
            ui_weak
                .upgrade()
                .is_some_and(|ui| ui.invoke_route_pane_prepare_transfer(slot, source, x, y))
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_transfer_menu_requested(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_transfer_menu_requested(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_trace_drop(move |action, kind, path, x, y, rejected, target| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_trace_main_drop(action, kind, path, x, y, rejected, target);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_save_focus_changed(move |slot, focused| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_save_focus_changed(slot, focused);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_commit_external_edit(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_commit_external_edit(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_discard_external_edit(move |slot| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_discard_external_edit(slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_undo_last_operation(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_undo_last_operation();
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_chooser_accept(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_chooser_accept(value);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_chooser_filter_requested(move |slot, x, y| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_chooser_filter_requested(slot, x, y);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        routing.on_chooser_choice_requested(move |slot, index, x, y| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_route_pane_chooser_choice_requested(slot, index, x, y);
            }
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
    let current_dir = state.borrow().panes.focused().current_dir.clone();
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    save_settings(&AppSettings {
        dark_mode: Some(ui.get_dark_mode()),
        sidebar_width_px: Some(ui.get_sidebar_width_px()),
        split_pane_ratio: Some(clamped_split_pane_ratio(ui.get_split_pane_ratio())),
        icon_zoom_level: Some(ui.get_icon_zoom_level()),
        window_width_px: Some(window_size.width),
        window_height_px: Some(window_size.height),
        last_dir: Some(current_dir),
    });
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

fn reset_search_controls(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    ui.set_search_query(SharedString::new());
    ui.set_search_kind_filter(0);
    ui.set_search_modified_filter(0);
    ui.set_search_size_filter(0);
    sync_pane_slots_ui(ui, state);
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
    debug_log(&format!(
        "load_directory slot={slot} pane_id={pane_id} generation={generation} preserve_view={preserve_view} defer_view_restore={defer_view_restore} path={} cache_hit={}",
        current_dir.display(),
        cached_entries.is_some()
    ));
    let target_is_focused = ui.get_focused_pane() == slot;
    if target_is_focused {
        set_current_location_ui(ui, &current_dir);
        ui.set_search_loading(false);
    }
    if !preserve_view && !defer_view_restore {
        restore_pane_view_state(ui, state, slot, &current_dir);
    }
    if let Some(cached_entries) = cached_entries {
        {
            let mut state = state.borrow_mut();
            if let Some(pane) = state.panes.pane_mut_for_target(PaneTarget::Id(pane_id)) {
                pane.search.visible_entries_have_locations = cached_entries.has_locations;
                pane.set_entries(Arc::clone(&cached_entries.entries));
                pane.view.virtual_view.invalidate();
            }
        }
        if target_is_focused {
            reset_search_controls(ui, state);
            ui.set_items_path(current_dir.display().to_string().into());
            ui.set_directory_loading(false);
        }
        set_pane_status(ui, state, slot, "Refreshing cached folder...");
    } else if !preserve_view {
        {
            let mut state = state.borrow_mut();
            if let Some(pane) = state.panes.pane_mut_for_target(PaneTarget::Id(pane_id)) {
                pane.clear_entries();
                pane.search.visible_entries_have_locations = false;
                pane.view.virtual_view.invalidate();
            }
        }
        if target_is_focused {
            ui.set_directory_loading(true);
            reset_search_controls(ui, state);
            update_selection_ui_for_slot(ui, state, slot, &[]);
        }
        set_pane_status(ui, state, slot, "Loading folder...");
    } else {
        if target_is_focused {
            ui.set_directory_loading(false);
        }
        set_pane_status(ui, state, slot, "Refreshing folder...");
    }
    sync_pane_view_for_slot(ui, state, bridge, slot);
    watch_current_directory(&current_dir, pane_id, generation, bridge);

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = read_entries_async(&current_dir)
            .await
            .map(PreparedDirectoryEntries::from_raw_entries);
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                pane_id,
                generation,
                path: current_dir,
                preserve_view,
                defer_view_restore,
                result,
            }),
        );
    });
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
                .map(PreparedDirectoryEntries::from_raw_entries);
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

fn watch_current_directory(path: &Path, pane_id: u64, generation: u64, bridge: &AsyncBridge) {
    use notify::Watcher;

    if fs::file_ops::is_trash_files_dir(path) {
        let _ = fs::file_ops::ensure_trash_dirs();
    }
    let watched_path = path.to_path_buf();
    let watch_paths = directory_watch_paths(path);
    let async_handle = bridge.handle.clone();
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let debounce = Arc::clone(&bridge.directory_watch_debounce);

    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return;
        }

        let serial = debounce.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        let reload_path = watched_path.clone();
        let async_tx = async_tx.clone();
        let notify_ui = notify_ui.clone();
        let debounce = Arc::clone(&debounce);

        async_handle.spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if debounce.load(AtomicOrdering::SeqCst) != serial {
                return;
            }

            let result = read_entries_async(&reload_path)
                .await
                .map(PreparedDirectoryEntries::from_raw_entries);
            send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                    pane_id,
                    generation,
                    path: reload_path,
                    preserve_view: true,
                    defer_view_restore: false,
                    result,
                }),
            );
        });
    });

    let Ok(mut watcher) = watcher else {
        *bridge.directory_watcher.borrow_mut() = None;
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
        *bridge.directory_watcher.borrow_mut() = Some(watcher);
    } else {
        *bridge.directory_watcher.borrow_mut() = None;
    }
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
        AsyncEvent::OpenWithAppsLoaded(result) => {
            open_with::apply_open_with_apps_result(ui, state, result)
        }
        AsyncEvent::OtherApplicationAppsLoaded(result) => {
            open_with::apply_other_application_apps_result(ui, state, result);
        }
        AsyncEvent::DefaultAppSet(result) => {
            open_with::apply_default_app_set_result(ui, state, result)
        }
        AsyncEvent::FileActionFinished(result) => {
            let applied = file_actions::apply_file_action_result(ui, state, result);
            if let Some(undo) = applied.undo {
                register_undo(ui, state, undo);
            }
            if let Some(status) = applied.status {
                let pane_ids =
                    refresh_affected_directories(ui, state, bridge, &applied.affected_dirs);
                set_status_for_panes(ui, state, &pane_ids, &status);
            }
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
        AsyncEvent::VirtualViewPrepared(result) => {
            apply_virtual_view_result(ui, state, bridge, result);
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
                "directory_loaded stale missing-pane pane_id={} generation={} path={}",
                result.pane_id,
                result.generation,
                result.path.display()
            ));
            return;
        };
        if !pane.load_generation.is_current(result.generation) || result.path != pane.current_dir {
            debug_log(&format!(
                "directory_loaded stale pane_id={} generation={} path={} current={} current_generation_match={}",
                result.pane_id,
                result.generation,
                result.path.display(),
                pane.current_dir.display(),
                pane.load_generation.is_current(result.generation)
            ));
            return;
        }
    }

    let Some(slot) = state.borrow().panes.slot_for_id(result.pane_id) else {
        return;
    };
    apply_pane_directory_result(ui, state, bridge, result, slot);
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
                "directory_loaded slot={} ok pane_id={} generation={} path={} entries={} preserve_view={}",
                slot,
                result.pane_id,
                result.generation,
                result.path.display(),
                entries.len(),
                result.preserve_view
            ));
            if result.defer_view_restore {
                restore_pane_view_state(ui, state, slot, &result.path);
            }
            let mut unchanged = false;
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
                    pane.search.visible_entries_have_locations = entries.has_locations;
                    pane.set_entries(Arc::clone(&entries.entries));
                    pane.view.virtual_view.invalidate();
                    if !result.preserve_view {
                        pane.search.reset_all();
                        pane.selection.clear();
                    }
                }
            }
            state
                .borrow_mut()
                .insert_directory_cache(result.path.clone(), entries.clone());
            if unchanged {
                debug_log(&format!(
                    "directory_loaded unchanged slot={slot} generation={} path={}",
                    result.generation,
                    result.path.display()
                ));
            }
            if target_is_focused {
                if !result.preserve_view {
                    reset_search_controls(ui, state);
                }
                ui.set_items_path(result.path.display().to_string().into());
                ui.set_directory_loading(false);
            }
            sync_pane_view_for_slot(ui, state, bridge, slot);
            set_directory_status_from_entries(ui, state, result.pane_id);
        }
        Err(err) => {
            debug_log(&format!(
                "directory_loaded slot={} error pane_id={} generation={} path={} preserve_view={} error={err}",
                slot,
                result.pane_id,
                result.generation,
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
    {
        let state = state.borrow();
        let Some(pane) = state.panes.pane_for_target(PaneTarget::Id(result.pane_id)) else {
            return;
        };
        if !pane.open_generation.is_current(result.generation) {
            return;
        }
    }

    match result.result {
        Ok(success) => {
            let launch_suffix = launch_status_suffix(&success);
            if let Some(unit) = &success.unit {
                state.borrow_mut().launched_units.push(unit.clone());
            }
            if let Some(session) = success.external_edit {
                register_external_edit(ui, state, result.pane_id, session);
                set_status_for_panes(
                    ui,
                    state,
                    &[result.pane_id],
                    &format!(
                        "Opened protected scratch copy with default app for {}; auto writeback active{}",
                        success.mime_type, launch_suffix
                    ),
                );
            } else {
                set_status_for_panes(
                    ui,
                    state,
                    &[result.pane_id],
                    &format!(
                        "Opened with default app for {}{}",
                        success.mime_type, launch_suffix
                    ),
                );
            }
        }
        Err(err) => {
            let label = result
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| result.path.to_str().unwrap_or("file"));
            set_status_for_panes(
                ui,
                state,
                &[result.pane_id],
                &format!("Cannot open {label}: {err}"),
            );
        }
    }
}

fn launch_status_suffix(success: &FileOpenSuccess) -> String {
    if let Some(unit) = &success.unit {
        format!(" ({unit})")
    } else if let Some(diagnostic) = &success.launch_diagnostic {
        format!("; {diagnostic}")
    } else {
        String::new()
    }
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

fn submit_search(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge, query: &str) {
    let query = query.trim().to_string();
    {
        let mut state = state.borrow_mut();
        cancel_active_search(&mut state);
        let pane = state.panes.focused_mut();
        pane.search.query = query.clone();
        pane.search_generation.next();
    }

    if query.is_empty() {
        ui.set_search_loading(false);
        refresh_directory(ui, state, bridge);
        return;
    }

    if ui.get_recursive_search() {
        start_recursive_search(ui, state, bridge, query);
    } else {
        ui.set_search_loading(false);
        apply_filter(ui, state, bridge, false);
    }
}

fn cancel_recursive_search(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let (query, progress) = {
        let mut state = state.borrow_mut();
        cancel_active_search(&mut state);
        state.panes.focused_mut().search_generation.next();
        let query = state.panes.focused().search.query.clone();
        let progress = state.panes.focused().search_progress;
        let current_dir = state.panes.focused().current_dir.clone();
        if let Some(entries) = state.cached_directory_entries(&current_dir) {
            let pane = state.panes.focused_mut();
            pane.search.visible_entries_have_locations = entries.has_locations;
            pane.set_entries(Arc::clone(&entries.entries));
            pane.view.virtual_view.invalidate();
        }
        (query, progress)
    };

    ui.set_search_loading(false);
    apply_filter(ui, state, bridge, true);
    if query.is_empty() {
        set_status(ui, state, "Recursive search cancelled");
    } else {
        set_status(
            ui,
            state,
            &recursive_search_cancelled_status(
                &query,
                progress.directories_scanned,
                progress.matches_found,
            ),
        );
    }
}

fn update_search_filters(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    kind: i32,
    modified: i32,
    size: i32,
) {
    {
        let mut state = state.borrow_mut();
        set_search_filters(&mut state, kind, modified, size);
    }

    apply_filter(ui, state, bridge, true);
    if ui.get_search_loading() {
        let query = state.borrow().panes.focused().search.query.clone();
        set_status(ui, state, &recursive_search_status(&query));
    }
}

fn start_recursive_search(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    query: String,
) {
    let (root, generation, cancel) = {
        let mut state = state.borrow_mut();
        cancel_active_search(&mut state);
        let generation = state.panes.focused_mut().search_generation.next();
        let cancel = Arc::new(AtomicBool::new(false));
        let pane = state.panes.focused_mut();
        pane.search_cancel = Some(cancel.clone());
        pane.search_progress = search::SearchProgress::default();
        (
            state.panes.focused().current_dir.clone(),
            generation,
            cancel,
        )
    };

    ui.set_search_loading(true);
    set_status(ui, state, &recursive_search_status(&query));
    {
        let mut state = state.borrow_mut();
        let pane = state.panes.focused_mut();
        pane.search.visible_entry_indices = None;
        pane.view.virtual_view.invalidate();
    }
    ui.set_entry_count(0);
    update_selection_ui(ui, state, &[]);

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
                        generation,
                        query: progress_query.clone(),
                        root: progress_root.clone(),
                        progress,
                    }),
                );
            })
            .await
            .map(PreparedDirectoryEntries::from_raw_entries);
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::RecursiveSearchFinished(RecursiveSearchResult {
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
    {
        let state = state.borrow();
        let stale = !state
            .panes
            .focused()
            .search_generation
            .is_current(progress.generation)
            || state.panes.focused().current_dir != progress.root
            || state.panes.focused().search.query != progress.query
            || !ui.get_search_loading();
        if stale {
            return;
        }
    }
    state.borrow_mut().panes.focused_mut().search_progress = progress.progress;

    set_status(
        ui,
        state,
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
    {
        let mut state = state.borrow_mut();
        let stale = !state
            .panes
            .focused()
            .search_generation
            .is_current(result.generation)
            || state.panes.focused().current_dir != result.root
            || state.panes.focused().search.query != result.query;
        if stale {
            return;
        }
        state.panes.focused_mut().search_cancel = None;
    }
    ui.set_search_loading(false);

    match result.result {
        Ok(entries) => {
            let total = entries.len();
            {
                let mut state = state.borrow_mut();
                let pane = state.panes.focused_mut();
                pane.search.visible_entries_have_locations = entries.has_locations;
                pane.set_entries(Arc::clone(&entries.entries));
                pane.view.virtual_view.invalidate();
            }
            apply_filter(ui, state, bridge, true);
            let visible = filtered_entry_count(&state.borrow());
            set_status(ui, state, &recursive_search_finished_status(visible, total));
        }
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            set_status(
                ui,
                state,
                &format!("Recursive search for '{}' cancelled", result.query),
            );
        }
        Err(err) => set_status(ui, state, &format!("Recursive search failed: {err}")),
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
    let can_request_privilege = privileged_command.is_some();
    let summary = {
        let mut state = state.borrow_mut();
        state.complete_file_operation(
            id,
            &operation,
            &source,
            &target_dir,
            result,
            can_request_privilege,
        )
    };
    let Some(summary) = summary else {
        return;
    };
    let mut requested_privilege = false;
    let status_message = match summary.disposition {
        OperationResultDisposition::Completed {
            destination,
            overwritten_backup,
            status,
        } => {
            register_file_undo(
                ui,
                state,
                &operation,
                &source,
                &destination,
                overwritten_backup,
            );
            Some(status)
        }
        OperationResultDisposition::RequestPrivilege { error } => {
            if let Some(command) = privileged_command {
                file_actions::request_privileged_action(ui, state, command, &error);
                requested_privilege = true;
                None
            } else {
                Some("Operation failed: missing privileged command".to_string())
            }
        }
        OperationResultDisposition::Failed { status } => Some(status),
    };

    if !summary.refresh_pane_ids.is_empty() {
        refresh_panes(ui, state, bridge, &summary.refresh_pane_ids);
    }
    if let Some(status_message) =
        operation_final_status(status_message, requested_privilege, summary.remaining)
    {
        set_status_for_panes(ui, state, &summary.refresh_pane_ids, &status_message);
    }
    start_next_operation(ui, state, bridge);
}

fn register_file_undo(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    operation: &str,
    original_source: &Path,
    destination: &Path,
    overwritten_backup: Option<PathBuf>,
) {
    if !matches!(operation, "move" | "copy" | "link") {
        return;
    }

    register_undo(
        ui,
        state,
        FileUndo {
            operation: operation.to_string(),
            original_source: original_source.to_path_buf(),
            destination: destination.to_path_buf(),
            overwritten_backup,
            items: Vec::new(),
        },
    );
}

fn register_undo(ui: &AppWindow, state: &Rc<RefCell<AppState>>, undo: FileUndo) {
    replace_file_undo(state, Some(undo));
    sync_undo_ui(ui, state);
}

fn replace_file_undo(state: &Rc<RefCell<AppState>>, undo: Option<FileUndo>) {
    let old_undo = {
        let mut state = state.borrow_mut();
        std::mem::replace(&mut state.last_undo, undo)
    };
    cleanup_file_undo_backup(old_undo);
}

fn cleanup_file_undo_backup(undo: Option<FileUndo>) {
    if let Some(backup) = undo.and_then(|undo| undo.overwritten_backup) {
        let _ = fs::file_ops::cleanup_overwrite_backup(&backup);
    }
}

fn sync_undo_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    ui.set_undo_available(state.last_undo.is_some());
    let label = state
        .last_undo
        .as_ref()
        .map(|undo| format!("Undo {}", operation_finished_label(&undo.operation)))
        .unwrap_or_default();
    ui.set_undo_label(label.into());
}

fn start_file_undo(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let undo = state.borrow_mut().last_undo.take();
    sync_undo_ui(ui, state);
    let Some(undo) = undo else {
        set_status(ui, state, "Nothing to undo");
        return;
    };

    let affected_dirs = file_undo_affected_dirs(&undo);
    let pane_ids = {
        let state = state.borrow();
        affected_directory_pane_ids(&state, affected_dirs.iter().map(|dir| dir.as_path()))
    };
    set_status_for_panes(
        ui,
        state,
        &pane_ids,
        &format!("Undoing {}...", operation_finished_label(&undo.operation)),
    );
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
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
    let affected_dirs = file_undo_affected_dirs(&result.undo);
    let pane_ids = refresh_affected_directories(ui, state, bridge, &affected_dirs);

    match result.result {
        Ok(message) => {
            set_status_for_panes(ui, state, &pane_ids, &format!("Undo complete: {message}"));
        }
        Err(err) => {
            let restored = restore_failed_file_undo(state, result.undo);
            sync_undo_ui(ui, state);
            if restored {
                set_status_for_panes(
                    ui,
                    state,
                    &pane_ids,
                    &format!("Undo failed: {err}; Undo can be retried"),
                );
            } else {
                set_status_for_panes(
                    ui,
                    state,
                    &pane_ids,
                    &format!("Undo failed: {err}; newer Undo is available"),
                );
            }
        }
    }
}

fn file_undo_affected_dirs(undo: &FileUndo) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_unique_parent(&mut dirs, &undo.original_source);
    push_unique_parent(&mut dirs, &undo.destination);
    for item in &undo.items {
        push_unique_parent(&mut dirs, &item.original_source);
        push_unique_parent(&mut dirs, &item.destination);
    }
    dirs
}

fn push_unique_parent(paths: &mut Vec<PathBuf>, path: &Path) {
    if let Some(parent) = path.parent() {
        push_unique_path(paths, parent.to_path_buf());
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn restore_failed_file_undo(state: &Rc<RefCell<AppState>>, undo: FileUndo) -> bool {
    let mut state_ref = state.borrow_mut();
    if state_ref.last_undo.is_none() {
        state_ref.last_undo = Some(undo);
        true
    } else {
        drop(state_ref);
        cleanup_file_undo_backup(Some(undo));
        false
    }
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
    if let Some(update) = state.borrow_mut().file_operation_progress_update(&progress) {
        set_status_for_panes(ui, state, &update.pane_ids, &update.status);
    }
}

fn apply_privileged_operation_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: privilege::PrivilegedOperationResult,
) {
    let pane_ids = refresh_affected_directories(ui, state, bridge, &result.affected_dirs);

    match result.result {
        Ok(message) => set_status_for_panes(
            ui,
            state,
            &pane_ids,
            &format!("{} complete: {message}", result.label),
        ),
        Err(err) => set_status_for_panes(
            ui,
            state,
            &pane_ids,
            &format!("{} failed: {err}", result.label),
        ),
    }
}

fn register_external_edit(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    pane_id: u64,
    session: privilege::ExternalEditSession,
) {
    {
        let mut state = state.borrow_mut();
        state
            .external_edits
            .push(PaneExternalEdit { pane_id, session });
    }
    sync_external_edit_ui(ui, state);
}

fn start_external_edit_resolution(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    operation: &str,
) {
    let pane_id = {
        let state_ref = state.borrow();
        let Some(pane_id) = pane_id_for_slot(&state_ref, slot) else {
            set_status(ui, state, "No split pane target is available");
            return;
        };
        pane_id
    };
    let session = {
        let state = state.borrow();
        state
            .external_edits
            .iter()
            .rev()
            .find(|edit| edit.pane_id == pane_id)
            .map(|edit| edit.session.clone())
    };
    let Some(session) = session else {
        set_status_for_panes(ui, state, &[pane_id], "No admin write-back is pending");
        return;
    };

    set_status_for_panes(
        ui,
        state,
        &[pane_id],
        match operation {
            EXTERNAL_EDIT_SAVE_OPERATION => "Saving admin write-back...",
            EXTERNAL_EDIT_DISCARD_OPERATION => "Discarding admin write-back...",
            _ => "Resolving admin write-back...",
        },
    );

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let operation = operation.to_string();
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
    if result.result.is_ok() {
        let mut state = state.borrow_mut();
        state
            .external_edits
            .retain(|edit| edit.session.token != result.session.token);
    }
    sync_external_edit_ui(ui, state);

    match result.result {
        Ok(path) => {
            if result.operation == EXTERNAL_EDIT_SAVE_OPERATION {
                let affected_dirs = path
                    .parent()
                    .map(|parent| vec![parent.to_path_buf()])
                    .unwrap_or_default();
                let pane_ids = refresh_affected_directories(ui, state, bridge, &affected_dirs);
                let status_pane_ids = if pane_ids.is_empty() {
                    vec![result.pane_id]
                } else {
                    pane_ids
                };
                set_status_for_panes(
                    ui,
                    state,
                    &status_pane_ids,
                    &format!("Admin write-back saved: {}", path.display()),
                );
            } else {
                set_status_for_panes(
                    ui,
                    state,
                    &[result.pane_id],
                    &format!("Admin write-back discarded: {}", path.display()),
                );
            }
        }
        Err(err) => set_status_for_panes(
            ui,
            state,
            &[result.pane_id],
            &format!("{} failed: {err}", result.operation),
        ),
    }
}

fn sync_external_edit_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    sync_pane_slots_ui(ui, state);
}

fn pane_id_for_slot(state: &AppState, slot: i32) -> Option<u64> {
    state.panes.pane_for_slot(slot).map(|pane| pane.id)
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

fn sync_virtual_entries_with_count(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
) {
    let slot = state.borrow().panes.focused_slot();
    sync_virtual_entries_for_slot_with_count(
        ui,
        state,
        bridge,
        slot,
        schedule_thumbnails,
        visible_count_override,
        false,
    );
}

fn sync_virtual_entries_for_slot_with_count(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
    immediate: bool,
) {
    let size_px = thumbnail_size_px(ui);
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let main_width = (window_size.width - ui.get_sidebar_width_px()).max(1.0);
    let viewport_width = pane_slot_width(ui, main_width, slot);
    let mut layout = MainGridLayout::from_ui_for_pane_width(ui, viewport_width);
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
        let requested_viewport_x = pane.view.viewport_x;
        layout.viewport_x = requested_viewport_x;
        if let Some((viewport_x, viewport_clamped)) = cached_virtual_viewport_sync(
            pane,
            &layout,
            requested_viewport_x,
            viewport_width,
            size_px,
            schedule_thumbnails,
            visible_count_override,
            &chooser_patterns,
        ) {
            pane.view.virtual_generation.next();
            Some(VirtualViewSyncRequest::Cached {
                viewport_x,
                viewport_clamped,
            })
        } else {
            let generation = pane.view.virtual_generation.next();
            let query = pane.search.query.to_ascii_lowercase();
            Some(VirtualViewSyncRequest::Prepare {
                pane_id: pane.id,
                generation,
                rows_per_column: layout.rows_per_column,
                cell_width: layout.cell_width,
                input: VirtualViewSnapshotInput {
                    layout,
                    requested_viewport_x,
                    viewport_width,
                    thumbnail_size_px: size_px,
                    schedule_thumbnails,
                    visible_count_override,
                    cache: pane.view.virtual_view.clone(),
                    entries: pane.entry_snapshot(),
                    visible_entry_indices: pane.search.visible_entry_indices.clone(),
                    visible_entries_have_locations: pane.search.visible_entries_have_locations,
                    query,
                    kind_filter: pane.search.kind_filter,
                    modified_filter: pane.search.modified_filter,
                    size_filter: pane.search.size_filter,
                    chooser_patterns,
                },
            })
        }
    }) else {
        return;
    };

    let (pane_id, generation, rows_per_column, cell_width, input) = match request {
        VirtualViewSyncRequest::Cached {
            viewport_x,
            viewport_clamped,
        } => {
            set_pane_viewport_ui_if_clamped(ui, slot, viewport_x, viewport_clamped, state);
            return;
        }
        VirtualViewSyncRequest::Prepare {
            pane_id,
            generation,
            rows_per_column,
            cell_width,
            input,
        } => (pane_id, generation, rows_per_column, cell_width, input),
    };

    if immediate {
        let update = prepare_virtual_view_snapshot_update(input);
        apply_virtual_view_result(
            ui,
            state,
            bridge,
            VirtualViewResult {
                pane_id,
                generation,
                thumbnail_size_px: size_px,
                schedule_thumbnails,
                rows_per_column,
                cell_width,
                update,
            },
        );
        return;
    }

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let Ok(update) =
            tokio::task::spawn_blocking(move || prepare_virtual_view_snapshot_update(input)).await
        else {
            return;
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::VirtualViewPrepared(VirtualViewResult {
                pane_id,
                generation,
                thumbnail_size_px: size_px,
                schedule_thumbnails,
                rows_per_column,
                cell_width,
                update,
            }),
        );
    });
}

fn cached_virtual_viewport_sync(
    pane: &mut PaneState,
    layout: &MainGridLayout,
    requested_viewport_x: f32,
    viewport_width: f32,
    thumbnail_size_px: u32,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
    chooser_patterns: &[String],
) -> Option<(f32, bool)> {
    if !schedule_thumbnails || visible_count_override.is_some() {
        return None;
    }

    let visible_count = if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        indices.len()
    } else if pane.search.query.is_empty()
        && pane.search.kind_filter == 0
        && pane.search.modified_filter == 0
        && pane.search.size_filter == 0
        && chooser_patterns.is_empty()
    {
        pane.entries.len()
    } else {
        return None;
    };

    let plan = virtual_grid_plan(
        visible_count,
        layout.rows_per_column,
        requested_viewport_x,
        viewport_width,
        layout.cell_width,
        layout.padding,
        2,
    );
    if pane.view.virtual_view.entry_count != visible_count
        || pane.view.virtual_view.rows_per_column != plan.rows_per_column
        || pane.view.virtual_view.cell_width != plan.cell_width
        || pane.view.virtual_view.thumbnail_size_px != thumbnail_size_px
        || !virtual_cache_covers_visible_range(&pane.view.virtual_view.range, &plan.visible_range)
    {
        return None;
    }

    pane.view.viewport_x = plan.viewport_x;
    let viewport_clamped = (plan.viewport_x - requested_viewport_x).abs() > f32::EPSILON;
    Some((plan.viewport_x, viewport_clamped))
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
    let update = result.update;
    let slot;
    {
        let mut state_ref = state.borrow_mut();
        slot = match state_ref.panes.slot_for_id(result.pane_id) {
            Some(s) => s,
            None => return,
        };
        let Some(pane) = state_ref.panes.pane_mut_by_id(result.pane_id) else {
            return;
        };
        if !pane.view.virtual_generation.is_current(result.generation) {
            return;
        }
        pane.view.viewport_x = update.viewport_x;
        if update.rebuild_model {
            pane.view.virtual_view.range = update.range.clone();
            pane.view.virtual_view.entry_count = update.entry_count;
            pane.view.virtual_view.rows_per_column = result.rows_per_column;
            pane.view.virtual_view.cell_width = result.cell_width;
            pane.view.virtual_view.thumbnail_size_px = result.thumbnail_size_px;
        }
    }

    set_pane_viewport_ui_if_clamped(ui, slot, update.viewport_x, update.viewport_clamped, state);
    let target_is_focused = state.borrow().panes.focused_slot() == slot;
    if !update.rebuild_model {
        if target_is_focused && ui.get_entry_count() != update.entry_count as i32 {
            ui.set_entry_count(update.entry_count as i32);
            sync_pane_slot_ui(ui, state, slot);
        }
        return;
    }

    let mut entries = update
        .entries
        .into_iter()
        .map(|entry| entry.to_file_entry())
        .collect::<Vec<_>>();
    {
        let state_ref = state.borrow();
        if let Some(pane) = state_ref.panes.pane_by_id(result.pane_id) {
            annotate_selection_state(&mut entries, &pane.selection.paths);
        }
        decorate_entries_with_cached_thumbnails_for_pane(
            &state_ref,
            result.pane_id,
            &mut entries,
            result.thumbnail_size_px,
        );
    }

    if result.schedule_thumbnails {
        let thumbnail_entries =
            prioritize_thumbnail_entries(&entries, update.range.start, update.visible_range);
        schedule_visible_thumbnails(
            ui,
            state,
            bridge,
            result.pane_id,
            &thumbnail_entries,
            result.thumbnail_size_px,
            false,
        );
    }
    set_pane_virtual_entries(
        state,
        slot,
        update.range.start,
        update.start_column,
        entries,
    );
    if target_is_focused {
        ui.set_entry_count(update.entry_count as i32);
    }
    sync_pane_slot_ui(ui, state, slot);
}

fn set_pane_virtual_entries(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    start_index: usize,
    start_column: usize,
    entries: Vec<FileEntry>,
) {
    if let Some(pane) = state.borrow_mut().panes.pane_mut_for_slot(slot) {
        update_pane_file_entries_model(&mut pane.view, start_index, start_column, entries);
    }
}

fn apply_filter(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    preserve_selection: bool,
) {
    let (query, filters_active, total, summary) = {
        let mut state_ref = state.borrow_mut();
        let summary = rebuild_visible_entry_index(&mut state_ref, preserve_selection);
        state_ref.panes.focused_mut().view.virtual_view.invalidate();
        (
            state_ref.panes.focused().search.query.to_ascii_lowercase(),
            search_filters_active(&state_ref),
            state_ref.panes.focused().entries.len(),
            summary,
        )
    };
    sync_virtual_entries_with_count(ui, state, bridge, true, Some(summary.count));
    if preserve_selection {
        let empty_paths = Vec::new();
        let visible_paths = summary.visible_paths.as_ref().unwrap_or(&empty_paths);
        retain_visible_selection(ui, state, visible_paths);
    } else {
        clear_active_selection(ui, state);
    }

    if query.is_empty() && !filters_active {
        set_focused_pane_status(
            ui,
            state,
            &format!("{} folders, {} files", summary.folders, summary.files),
        );
    } else {
        set_focused_pane_status(
            ui,
            state,
            &format!(
                "{} of {total} items ({} folders, {} files)",
                summary.count, summary.folders, summary.files
            ),
        );
    }
}

fn retain_visible_selection(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    visible_paths: &[String],
) {
    let selected_paths = {
        let mut state = state.borrow_mut();
        let pane = state.panes.focused_mut();
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
    update_selection_ui_for_slot(
        ui,
        state,
        state.borrow().panes.focused_slot(),
        &selected_paths,
    );
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

fn clear_selection(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let slot = { state.borrow().panes.focused_slot() };
    let mut state_mut = state.borrow_mut();
    if let Some(pane) = state_mut.panes.pane_mut_for_slot(slot) {
        pane.selection.clear();
    }
    drop(state_mut);
    update_selection_ui_for_slot(ui, state, slot, &[]);
}

fn clear_active_selection(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let mut state_mut = state.borrow_mut();
    state_mut.panes.focused_mut().selection.clear();
    drop(state_mut);
    update_selection_ui_for_slot(ui, state, state.borrow().panes.focused_slot(), &[]);
}

fn schedule_visible_thumbnails(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_id: u64,
    entries: &[&FileEntry],
    size_px: u32,
    announce: bool,
) {
    let (generation, paths) = {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_by_id(pane_id) else {
            return;
        };
        let generation = pane.thumbnail_generation.current();
        let paths = thumbnail_schedule_batch_for_pane(&mut state, pane_id, entries, size_px);

        (generation, paths)
    };

    if paths.is_empty() {
        return;
    }

    if announce {
        set_status(ui, state, "Loading thumbnails...");
    }
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let fallback_paths = paths.clone();
        let loads = match tokio::task::spawn_blocking(move || {
            paths
                .into_iter()
                .map(|path| thumbnails::load_thumbnail(path, size_px))
                .collect::<Vec<_>>()
        })
        .await
        {
            Ok(loads) => loads,
            Err(err) => {
                let message = format!("thumbnail task failed: {err}");
                fallback_paths
                    .into_iter()
                    .map(|path| thumbnails::ThumbnailLoad {
                        key: thumbnails::fallback_key(&path, size_px),
                        path,
                        cache_paths: None,
                        data: Err(io::Error::other(message.clone())),
                    })
                    .collect()
            }
        };
        for load in loads {
            send_async_event(
                async_tx.clone(),
                notify_ui.clone(),
                AsyncEvent::ThumbnailLoaded {
                    pane_id,
                    generation,
                    load,
                },
            );
        }
    });
}

fn flush_thumbnail_results(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pending: &Rc<RefCell<VecDeque<(u64, u64, thumbnails::ThumbnailLoad)>>>,
) {
    let refresh_pane_ids = {
        let mut state = state.borrow_mut();
        let mut refresh_pane_ids = Vec::new();
        let mut pending = pending.borrow_mut();
        while let Some((pane_id, generation, load)) = pending.pop_front() {
            let path_text = load.path.display().to_string();
            if apply_thumbnail_load_to_state_for_pane(
                &mut state, pane_id, generation, &path_text, load,
            ) && !refresh_pane_ids.contains(&pane_id)
            {
                refresh_pane_ids.push(pane_id);
            }
        }
        refresh_pane_ids
    };
    for pane_id in refresh_pane_ids {
        let slot = { state.borrow().panes.slot_for_id(pane_id) };
        if let Some(slot) = slot {
            sync_virtual_entries_for_slot(ui, state, bridge, slot, false);
        }
    }
}

fn thumbnail_size_px(ui: &AppWindow) -> u32 {
    match ui.get_icon_zoom_level() {
        0 => 64,
        1 => 80,
        2 => 104,
        3 => 128,
        _ => 160,
    }
}

fn update_selection_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>, selected_paths: &[String]) {
    let slot = state.borrow().panes.focused_slot();
    update_selection_ui_for_slot(ui, state, slot, selected_paths);
}

fn selection_status_text(selected_paths: &[String]) -> SharedString {
    match selected_paths {
        [] => SharedString::new(),
        [path] => format!("1 item selected: {path}").into(),
        paths => format!("{} items selected", paths.len()).into(),
    }
}

fn sync_visible_pane_layouts(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let slots = ui.get_pane_slots();
    if slots.row_count() == 0 {
        sync_pane_layout_for_slot(ui, state, bridge, 0);
        return;
    }

    for row in 0..slots.row_count() {
        if let Some(pane) = slots.row_data(row) {
            sync_pane_layout_for_slot(ui, state, bridge, pane.slot);
        }
    }
}

fn sync_pane_viewport_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    sync_virtual_entries_for_slot(ui, state, bridge, slot, true);
}

fn sync_pane_layout_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    sync_virtual_entries_for_slot_with_count(ui, state, bridge, slot, true, None, true);
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
        sync_focus_navigation_ui(ui, state);
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
    ui.set_selection_revision(ui.get_selection_revision() + 1);
    sync_pane_slots_ui(ui, state);
}

fn update_virtual_selection_for_slot(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    selected_paths: &[String],
) {
    let model = {
        state
            .borrow()
            .panes
            .pane_for_slot(slot)
            .map(|pane| pane.view.virtual_entries.clone())
    };
    if let Some(model) = model {
        update_file_entries_model_selection(&model, selected_paths);
    }
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
            .unwrap_or(&state.panes.focused())
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
        let entry = pane
            .entries
            .iter()
            .find(|entry| entry.path.as_str() == path);
        let path = entry
            .map(|entry| Cow::Owned(entry.path.to_string()))
            .unwrap_or_else(|| Cow::Borrowed(path));
        (
            PathBuf::from(path.as_ref()),
            entry.map(|entry| entry.is_dir),
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
    apply_filter(ui, state, bridge, true);
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

fn set_focused_pane_status(ui: &AppWindow, state: &Rc<RefCell<AppState>>, message: &str) {
    let slot = state.borrow().panes.focused_slot();
    set_pane_status(ui, state, slot, message);
}

fn set_pane_status(ui: &AppWindow, state: &Rc<RefCell<AppState>>, slot: i32, message: &str) {
    let message: SharedString = message.into();
    if ui.get_focused_pane() == slot {
        ui.set_status(message);
    }
    sync_pane_slots_ui(ui, state);
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
                    .map(|slot| (slot, directory_status_text(pane.entries.iter())))
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
        let state = state.borrow();
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
    let message: SharedString = message.into();
    ui.set_status(message);
    sync_pane_slots_ui(ui, state);
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
    use crate::app::geometry::{place_drop_geometry, virtual_grid_plan};
    use crate::app::operation_controller::transfer_target_rejection;
    use crate::app::selection::{
        filtered_entries_range, filtered_entry_at, filtered_entry_paths, filtered_entry_summary,
        rebuild_visible_entry_index, selection_range_paths, selection_range_paths_filtered,
        selection_rect_paths, selection_rect_paths_filtered,
    };
    use slint::Image;

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

        assert_eq!(filtered_entry_count(&state), 8);
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

        let summary = rebuild_visible_entry_index(&mut state, true);

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

        let summary = rebuild_visible_entry_index(&mut state, false);

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
        assert_eq!(filtered_entry_count(&state), 3);
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

        let summary = rebuild_visible_entry_index(&mut state, false);

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

        let summary = rebuild_visible_entry_index(&mut state, false);

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
        rebuild_visible_entry_index(&mut state, false);

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
    fn virtual_grid_plan_keeps_visible_columns_with_overscan() {
        let at_start = virtual_grid_plan(100, 4, 0.0, 250.0, 100.0, 10.0, 1);
        assert_eq!(at_start.range, 0..16);
        assert_eq!(at_start.visible_range, 0..12);

        let middle = virtual_grid_plan(100, 4, 350.0, 250.0, 100.0, 10.0, 1);
        assert_eq!(middle.range, 8..28);
        assert_eq!(middle.visible_range, 12..24);

        let clamped = virtual_grid_plan(10, 4, 800.0, 250.0, 100.0, 10.0, 1);
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
                rows_per_column: 2,
                cell_width: 100.0,
                row_height: 100.0,
                padding: 10.0,
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
                rows_per_column: 2,
                cell_width: 100.0,
                row_height: 100.0,
                padding: 10.0,
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
                x1: 210.0,
                y1: 0.0,
                x2: 309.0,
                y2: 205.0,
                rows_per_column: 2,
                cell_width: 100.0,
                row_height: 100.0,
                padding: 10.0,
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
    fn failed_file_undo_is_restored_when_no_newer_undo_exists() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/tmp"),
            Vec::new(),
        )));
        let undo = test_undo("copy", "/tmp/source.txt", "/tmp/target/source.txt");

        assert!(restore_failed_file_undo(&state, undo.clone()));

        let state = state.borrow();
        let restored = state.last_undo.as_ref().unwrap();
        assert_eq!(restored.operation, undo.operation);
        assert_eq!(restored.original_source, undo.original_source);
        assert_eq!(restored.destination, undo.destination);
    }

    #[test]
    fn failed_file_undo_does_not_replace_newer_undo() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/tmp"),
            Vec::new(),
        )));
        let newer = test_undo("move", "/tmp/new-source.txt", "/tmp/new-target.txt");
        state.borrow_mut().last_undo = Some(newer.clone());
        let failed = test_undo("copy", "/tmp/source.txt", "/tmp/target/source.txt");

        assert!(!restore_failed_file_undo(&state, failed));

        let state = state.borrow();
        let retained = state.last_undo.as_ref().unwrap();
        assert_eq!(retained.operation, newer.operation);
        assert_eq!(retained.original_source, newer.original_source);
        assert_eq!(retained.destination, newer.destination);
    }

    #[test]
    fn file_undo_affected_dirs_are_deduplicated_in_operation_order() {
        let mut undo = test_undo("copy", "/tmp/source/one.txt", "/tmp/target/one.txt");
        undo.items = vec![
            crate::app::state::FileUndoItem {
                original_source: PathBuf::from("/tmp/source/two.txt"),
                destination: PathBuf::from("/tmp/target/two.txt"),
            },
            crate::app::state::FileUndoItem {
                original_source: PathBuf::from("/tmp/other/three.txt"),
                destination: PathBuf::from("/tmp/target/three.txt"),
            },
        ];

        assert_eq!(
            file_undo_affected_dirs(&undo),
            vec![
                PathBuf::from("/tmp/source"),
                PathBuf::from("/tmp/target"),
                PathBuf::from("/tmp/other"),
            ]
        );
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
                && body.contains("sync_focus_navigation_ui(ui, state);")
                && !body.contains("sync_navigation_ui(ui, state);"),
            "clicking inside the already focused pane must not rebuild pane surfaces, and focus changes should skip left-pane rewrites"
        );
    }

    #[test]
    fn pane_slot_sync_updates_existing_rows_when_slot_shape_is_unchanged() {
        let source = include_str!("app/split_view.rs");
        let body = source
            .split_once("pub(crate) fn sync_pane_slots_ui(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn sync_pane_slot_ui("))
            .map(|(body, _)| body)
            .expect("sync_pane_slots_ui body should be present");

        assert!(
            body.contains("let current = ui.get_pane_slots();")
                && body.contains("let same_slots = current.row_count() == slots.len()")
                && body.contains(".is_some_and(|current| current.slot == slot.slot)")
                && body.contains("if current.row_data(row).as_ref() != Some(&slot)")
                && body.contains("current.set_row_data(row, slot);")
                && body
                    .contains("ui.set_pane_slots(ModelRc::new(Rc::new(VecModel::from(slots))));"),
            "pane data refresh should update stable rows instead of replacing the pane model while gestures are active"
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

        assert!(
            body.contains("if current_slot.slot == slot")
                && body.contains("let next = pane_slot_data(ui, slot, &state_ref);")
                && body.contains("if current_slot != next")
                && body.contains("current.set_row_data(row, next);")
                && body.contains("sync_pane_slots_ui(ui, state);"),
            "viewport-only pane refreshes should update the affected pane row and fall back to full sync only when the row is missing"
        );
    }

    #[test]
    fn pane_view_changes_are_coalesced_before_virtual_slice_sync() {
        let source = include_str!("main.rs");
        let main_body = source
            .split_once("fn main()")
            .and_then(|(_, rest)| rest.split_once("load_directory(&ui, &state, &bridge);"))
            .map(|(body, _)| body)
            .expect("main setup body should be present");
        let scheduler_body = source
            .split_once("struct PaneViewSyncScheduler")
            .and_then(|(_, rest)| rest.split_once("struct ThumbnailFlushScheduler"))
            .map(|(body, _)| body)
            .expect("pane view scheduler body should be present");

        assert!(
            source.contains("const PANE_VIEW_SYNC_COALESCE: Duration = Duration::from_millis(8);")
                && main_body.contains("let pane_view_sync = Rc::new(PaneViewSyncScheduler::new(")
                && main_body.contains("ui.on_pane_view_changed(move |slot|")
                && main_body.contains("pane_view_sync.request(slot);")
                && !main_body.contains("sync_pane_view_for_slot(&ui, &state, &bridge, slot);"),
            "pane view-changed callbacks should enqueue viewport sync instead of doing full virtual refresh immediately"
        );
        assert!(
            scheduler_body.contains("timer.start(TimerMode::SingleShot, PANE_VIEW_SYNC_COALESCE")
                && scheduler_body.contains("pending.sort_unstable();")
                && scheduler_body.contains("pending.dedup();")
                && scheduler_body
                    .contains("sync_pane_viewport_for_slot(&ui, &state, &bridge, slot);"),
            "pane view scheduler should collapse repeated slot events into one viewport sync"
        );
    }

    #[test]
    fn layout_changes_rebuild_the_visible_slice_immediately() {
        let source = include_str!("main.rs");
        let main_body = source
            .split_once("ui.on_pane_layout_changed(move ||")
            .and_then(|(_, rest)| rest.split_once("ui.on_pane_slots_refresh_requested"))
            .map(|(body, _)| body)
            .expect("pane layout callback body should be present");
        let viewport_body = source
            .split_once("fn sync_pane_viewport_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_layout_for_slot("))
            .map(|(body, _)| body)
            .expect("viewport-only pane sync body should be present");
        let layout_body = source
            .split_once("fn sync_pane_layout_for_slot(")
            .and_then(|(_, rest)| rest.split_once("fn sync_pane_view_for_slot("))
            .map(|(body, _)| body)
            .expect("pane layout sync body should be present");

        assert!(
            main_body.contains("pane_view_sync.flush_all();")
                && main_body.contains("sync_visible_pane_layouts(&ui, &state, &bridge);")
                && !main_body.contains("sync_pane_view_for_slot"),
            "layout changes should flush pending scroll work and rebuild visible pane slices immediately"
        );
        assert!(
            viewport_body.contains("sync_virtual_entries_for_slot(ui, state, bridge, slot, true);")
                && !viewport_body.contains("sync_pane_slot_preview")
                && !viewport_body.contains("sync_virtual_entries(ui, state, bridge, true);")
                && !viewport_body.contains("filtered_entry_count_for_slot")
                && !viewport_body.contains("return;"),
            "pane layout/scroll sync should update the target slot through the shared virtual slice path"
        );
        assert!(
            layout_body.contains(
                "sync_virtual_entries_for_slot_with_count(ui, state, bridge, slot, true, None, true);"
            ),
            "layout changes must synchronously clamp/rebuild the visible slice before Slint reuses old virtual coordinates"
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
            .and_then(|(_, rest)| rest.split_once("fn thumbnail_size_px("))
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
                && flush_body
                    .contains("sync_virtual_entries_for_slot(ui, state, bridge, slot, false);")
                && !flush_body.contains("sync_virtual_entries(ui, state, bridge, false);"),
            "thumbnail flush should apply pending loads and refresh each affected pane slot once"
        );
    }

    #[test]
    fn file_operation_completion_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_file_operation_result(")
            .and_then(|(_, rest)| rest.split_once("fn register_file_undo("))
            .map(|(body, _)| body)
            .expect("apply_file_operation_result body should be present");

        assert!(
            body.contains(
                "set_status_for_panes(ui, state, &summary.refresh_pane_ids, &status_message);"
            ),
            "file operation completion status should write to the panes affected by the operation"
        );
        assert!(
            !body.contains("set_status(ui, state, &status_message);"),
            "file operation completion status must not jump to whichever pane is focused when the async result returns"
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
            !body.contains("set_status(ui, state, &update.status);"),
            "file operation progress status must not jump to whichever pane is focused while progress events arrive"
        );
    }

    #[test]
    fn file_undo_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let start_body = source
            .split_once("fn start_file_undo(")
            .and_then(|(_, rest)| rest.split_once("fn apply_file_undo_result("))
            .map(|(body, _)| body)
            .expect("start_file_undo body should be present");
        let result_body = source
            .split_once("fn apply_file_undo_result(")
            .and_then(|(_, rest)| rest.split_once("fn file_undo_affected_dirs("))
            .map(|(body, _)| body)
            .expect("apply_file_undo_result body should be present");

        assert!(
            start_body.contains("let affected_dirs = file_undo_affected_dirs(&undo);")
                && start_body.contains("let pane_ids = {")
                && start_body.contains("affected_directory_pane_ids(&state, affected_dirs.iter().map(|dir| dir.as_path()))")
                && start_body.contains("set_status_for_panes("),
            "file undo start status should write to panes affected by the undo"
        );
        assert!(
            result_body.contains(
                "let pane_ids = refresh_affected_directories(ui, state, bridge, &affected_dirs);"
            ) && result_body.matches("set_status_for_panes(").count() == 3,
            "file undo result status should use the same affected-pane route as its refresh"
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
            .and_then(|(_, rest)| rest.split_once("fn launch_status_suffix("))
            .map(|(body, _)| body)
            .expect("apply_file_open_result body should be present");

        assert!(
            start_body.contains(
                "set_status_for_panes(ui, state, &[pane_id], &format!(\"Opening {label}...\"));"
            ),
            "file-open start status should write to the pane that requested the open"
        );
        assert!(
            result_body.matches("set_status_for_panes(").count() == 3
                && result_body.matches("&[result.pane_id]").count() == 3
                && result_body
                    .contains("register_external_edit(ui, state, result.pane_id, session);"),
            "file-open success, protected external-edit registration, and failure status should use the requesting pane id"
        );
        assert!(
            !result_body.contains("set_status(\n                    ui,\n                    &format!(\n                        \"Opened with default app")
                && !result_body.contains("set_status(ui, state, &format!(\"Cannot open {label}: {err}\"));"),
            "file-open result status must not jump to whichever pane is focused when the async result returns"
        );
    }

    #[test]
    fn privileged_operation_status_uses_affected_pane_route() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn apply_privileged_operation_result(")
            .and_then(|(_, rest)| rest.split_once("fn register_external_edit("))
            .map(|(body, _)| body)
            .expect("apply_privileged_operation_result body should be present");

        assert!(
            body.contains(
                "let pane_ids = refresh_affected_directories(ui, state, bridge, &result.affected_dirs);"
            ) && body.matches("set_status_for_panes(").count() == 2,
            "privileged operation result status should use the same affected-pane route as its refresh"
        );
        assert!(
            !body.contains(
                "set_status(ui, state, &format!(\"{} complete: {message}\", result.label))"
            ) && !body
                .contains("set_status(ui, state, &format!(\"{} failed: {err}\", result.label))"),
            "privileged operation result status must not jump to whichever pane is focused when the helper returns"
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
            body.contains("let affected_dirs = path")
                && body.contains("let pane_ids = refresh_affected_directories(ui, state, bridge, &affected_dirs);")
                && body.contains("let status_pane_ids = if pane_ids.is_empty()")
                && body.contains("vec![result.pane_id]")
                && body.contains("set_status_for_panes(\n                    ui,\n                    state,\n                    &status_pane_ids,"),
            "admin write-back save status should write to the pane whose directory was refreshed"
        );
        assert!(
            !body.contains(
                "set_status(ui, state, &format!(\"Admin write-back saved: {}\", path.display()))"
            ),
            "admin write-back save status must not jump to whichever pane is focused when the helper returns"
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
            .and_then(|(_, rest)| rest.split_once("fn pane_id_for_slot("))
            .map(|(body, _)| body)
            .expect("sync_external_edit_ui body should be present");

        assert!(
            start_body.contains("pane_id_for_slot(&state_ref, slot)")
                && start_body.contains(".find(|edit| edit.pane_id == pane_id)")
                && start_body.contains("ExternalEditResult {\n                pane_id,"),
            "admin write-back resolution should select the pending session owned by the clicked pane"
        );
        assert!(
            !start_body.contains("state.external_edits.last().cloned()")
                && !start_body.contains("ui.set_external_edit_active")
                && !start_body.contains("ui.set_external_edit_status"),
            "admin write-back resolution must not fall back to the last global session or root-level pending state"
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
            PreparedDirectoryEntries::new(vec![PaneEntrySnapshot::from_entry(&test_entry(
                "a", "/tmp/a",
            ))]),
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

    fn test_undo(operation: &str, original_source: &str, destination: &str) -> FileUndo {
        FileUndo {
            operation: operation.to_string(),
            original_source: PathBuf::from(original_source),
            destination: PathBuf::from(destination),
            overwritten_backup: None,
            items: Vec::new(),
        }
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
            selected: false,
            thumbnail_state: 0,
            thumbnail: Image::default(),
        }
    }
}
