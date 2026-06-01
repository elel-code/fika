use slint::{
    CloseRequestResponse, ComponentHandle, Image, LogicalSize, ModelRc, Rgba8Pixel,
    SharedPixelBuffer, SharedString, VecModel,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::env;
use std::io;
use std::ops::Range;
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
use app::events::{
    AsyncEvent, DirectoryLoadResult, ExternalEditResult, ExternalFileDrop, FileOpenResult,
    FileOpenSuccess, FileOperationProgress, FileOperationResult, FileUndoResult,
    RecursiveSearchProgress, RecursiveSearchResult,
};
use app::file_clipboard::sync_clipboard_ui;
use app::geometry::{
    ChildPopupInput, HoverBridgeInput, MainGridLayout, MenuMetricsInput, PopupPlacement,
    PopupPoint, PopupRect, SelectionRect, context_menu_metrics, place_drop_geometry,
    point_in_main_pane, virtual_grid_plan,
};
use app::places::{
    add_place, add_place_at_slot, add_place_at_slot_from_external_payload,
    apply_external_file_drop, contains_place_path, is_supported_places_drop_mime,
    open_place_new_window, places_drop_force_gap, remove_place, rename_place, reorder_place_path,
    restore_default_places, sync_places,
};
use app::selection::{
    append_unique_paths, filtered_entries_range, filtered_entry_count, filtered_entry_paths,
    rebuild_visible_entry_index, retained_visible_paths, selection_range_paths_filtered,
    selection_rect_paths_filtered,
};
use app::state::{
    AppState, ChooserChoice as StateChooserChoice, ChooserChoiceItem, ChooserFilter,
    DirectoryViewState, FileUndo,
};
use app::transfer::{
    cancel_queued_operations, entry_at_main_point, format_bytes, main_drop_allowed,
    operation_label, path_label, place_drop_allowed, prepare_current_dir_transfer,
    prepare_entry_transfer, prepare_main_transfer, prepare_place_transfer,
    resolve_transfer_conflict, start_next_operation, start_transfer_operation,
};
use config::args::{Args, Mode};
use config::paths::{expand_user_path, home_dir, normalize_start_dir};
use config::settings::{AppSettings, load_settings, save_settings};
use desktop::{mime_open, open_with, terminal};
use fs::devices::mounted_devices;
use fs::entries::{read_entries_async, to_file_entry};
use fs::places::default_places;
use fs::{file_actions, privilege, search, thumbnails};

slint::include_modules!();

const MAX_THUMBNAIL_CACHE_ENTRIES: usize = 512;
const MAX_THUMBNAIL_FAILURE_ENTRIES: usize = 512;

fn main() -> Result<(), slint::PlatformError> {
    sanitize_locale_for_icu4x();
    let raw_args = env::args().skip(1).collect::<Vec<_>>();

    let async_runtime = build_async_runtime();
    let async_handle = async_runtime.handle().clone();

    let args = Args::parse(raw_args.into_iter());
    let settings = load_settings();
    let start_dir = args.start_dir.clone().unwrap_or_else(|| {
        settings
            .last_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| home_dir()))
    });
    let (async_tx, async_rx) = mpsc::channel();
    let external_drop_ui_weak = Rc::new(RefCell::new(None));
    select_winit_backend_for_external_drops(async_tx.clone(), Rc::clone(&external_drop_ui_weak))?;

    let state = Rc::new(RefCell::new(AppState::new(
        normalize_start_dir(start_dir),
        default_places(),
    )));

    let ui = AppWindow::new()?;
    *external_drop_ui_weak.borrow_mut() = Some(ui.as_weak());
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
    sync_chooser_filter_ui(&ui, &state);
    sync_chooser_choices_ui(&ui, &state);
    ui.set_dark_mode(settings.dark_mode.unwrap_or(true));
    if let Some(sidebar_width_px) = settings.sidebar_width_px {
        ui.set_sidebar_width_px(sidebar_width_px.clamp(220.0, 1200.0));
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
    sync_devices(&ui);
    sync_clipboard_ui(&ui, &state);
    let bridge = AsyncBridge {
        handle: async_handle.clone(),
        tx: async_tx,
        ui_weak: ui.as_weak(),
        directory_watcher: Rc::new(RefCell::new(None)),
        directory_watch_debounce: Arc::new(AtomicU64::new(0)),
    };

    let async_rx = Rc::new(RefCell::new(async_rx));
    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let async_rx = Rc::clone(&async_rx);
        let bridge = bridge.clone();
        ui.on_async_results_ready(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };

            while let Ok(event) = async_rx.borrow_mut().try_recv() {
                apply_async_event(&ui, &state, &bridge, event);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_main_view_changed(move || {
            if let Some(ui) = ui_weak.upgrade() {
                sync_virtual_entries(&ui, &state, &bridge, true);
            }
        });
    }

    load_directory(&ui, &state, &bridge);

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_refresh(move || {
            if let Some(ui) = ui_weak.upgrade() {
                refresh_directory(&ui, &state, &bridge);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_go_home(move || {
            if let Some(ui) = ui_weak.upgrade() {
                navigate_to(&ui, &state, &bridge, home_dir());
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
                navigate_to(&ui, &state, &bridge, PathBuf::from("/"));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_path_submitted(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let requested = expand_user_path(path.as_str());
                if requested.is_dir() {
                    navigate_to(&ui, &state, &bridge, requested);
                } else {
                    ui.set_path_input_text(ui.get_current_path());
                    set_status(&ui, "Path is not a readable directory");
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_open_place(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let requested = expand_user_path(path.as_str());
                if requested.is_dir() {
                    navigate_to(&ui, &state, &bridge, requested);
                } else {
                    set_status(&ui, "Place is not available");
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
        let state = Rc::clone(&state);
        let async_handle = async_handle.clone();
        ui.on_open_terminal_here(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let dir = state.borrow().current_dir.clone();
            set_status(&ui, &format!("Opening terminal in {}...", dir.display()));
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
                    set_status(&ui, &message);
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
        ui.on_chooser_next_filter(move || {
            if let Some(ui) = ui_weak.upgrade() {
                cycle_chooser_filter(&ui, &state, &bridge);
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
        ui.on_select_path(move |path, toggle, range| {
            if let Some(ui) = ui_weak.upgrade() {
                select_path(&ui, &state, path.as_str(), toggle, range);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_select_rect(
            move |x1, y1, x2, y2, rows_per_column, cell_width, row_height, padding, toggle| {
                if let Some(ui) = ui_weak.upgrade() {
                    select_rect(
                        &ui,
                        &state,
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
        ui.on_is_selected(move |path| {
            state
                .borrow()
                .selected_paths
                .iter()
                .any(|selected| selected == path.as_str())
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
        ui.on_add_place(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                add_place(&ui, &state, PathBuf::from(path.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_add_place_at_slot(move |path, slot| {
            if let Some(ui) = ui_weak.upgrade() {
                add_place_at_slot(&ui, &state, PathBuf::from(path.as_str()), slot);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_add_external_place_at_slot(move |payload, slot, mime_type| {
            if let Some(ui) = ui_weak.upgrade() {
                add_place_at_slot_from_external_payload(
                    &ui,
                    &state,
                    payload.as_str(),
                    slot,
                    mime_type.as_str(),
                );
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
        ui.on_remove_place(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                remove_place(&ui, &state, index);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_restore_default_places(move || {
            if let Some(ui) = ui_weak.upgrade() {
                restore_default_places(&ui, &state);
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
        ui.on_prepare_main_transfer(move |source, label, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_main_transfer(&ui, &state, source.as_str(), label.as_str(), x, y)
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_prepare_path_main_transfer(move |source, x, y| {
            ui_weak.upgrade().is_some_and(|ui| {
                prepare_main_transfer(
                    &ui,
                    &state,
                    source.as_str(),
                    path_label(source.as_str()).as_str(),
                    x,
                    y,
                )
            })
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_main_drop_target_path(move |x, y, source| {
            let Some(ui) = ui_weak.upgrade() else {
                return SharedString::new();
            };
            let state = state.borrow();
            entry_at_main_point(&ui, &state, x, y)
                .filter(|entry| entry.is_dir && entry.path.as_str() != source.as_str())
                .map_or_else(SharedString::new, |entry| entry.path)
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_main_drop_allowed(move |x, y, source| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            let source = Path::new(source.as_str());
            main_drop_allowed(&ui, &state, x, y, source)
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
        ui.on_place_drop_target(move |y, force_gap| {
            let Some(ui) = ui_weak.upgrade() else {
                return -1;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
                force_gap,
            )
            .target_index
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_slot(move |y, force_gap| {
            let Some(ui) = ui_weak.upgrade() else {
                return 0;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
                force_gap,
            )
            .slot
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_over_gap(move |y, force_gap| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
                force_gap,
            )
            .over_gap
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_place_drop_over_item(move |y, force_gap| {
            let Some(ui) = ui_weak.upgrade() else {
                return false;
            };
            let state = state.borrow();
            place_drop_geometry(
                y,
                state.places.len(),
                ui.get_places_list_y_px(),
                ui.get_places_row_stride_px(),
                force_gap,
            )
            .over_item
        });
    }

    ui.on_places_drop_supported(|mime_type| is_supported_places_drop_mime(mime_type.as_str()));
    ui.on_places_drop_force_gap(|mime_type| places_drop_force_gap(mime_type.as_str()));

    ui.on_root_menu_left(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap| {
            RootMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
            }
            .popup()
            .x
        },
    );

    ui.on_root_menu_top(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap| {
            RootMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
            }
            .popup()
            .y
        },
    );

    ui.on_anchored_menu_left(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap,
         gap| {
            AnchoredMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
                gap,
            }
            .popup()
            .x
        },
    );

    ui.on_anchored_menu_top(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap,
         gap| {
            AnchoredMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
                gap,
            }
            .popup()
            .y
        },
    );

    ui.on_child_menu_left(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         row_y,
         child_width,
         child_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildMenuGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                row_y,
                child_width,
                child_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .popup()
            .x
        },
    );

    ui.on_child_menu_top(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         row_y,
         child_width,
         child_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildMenuGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                row_y,
                child_width,
                child_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .popup()
            .y
        },
    );

    ui.on_child_bridge_left(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .x
        },
    );

    ui.on_child_bridge_top(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .y
        },
    );

    ui.on_child_bridge_width(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .width
        },
    );

    ui.on_child_bridge_height(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .height
        },
    );

    ui.on_context_menu_height(
        |kind,
         selected_count,
         is_dir,
         default_open_visible,
         add_to_places_visible,
         clipboard_has_paths,
         place_builtin,
         item_height,
         separator_height,
         title_height| {
            context_menu_metrics(MenuMetricsInput {
                kind,
                selected_count,
                is_dir,
                default_open_visible,
                add_to_places_visible,
                clipboard_has_paths,
                place_builtin,
                item_height,
                separator_height,
                title_height,
            })
            .height
        },
    );

    ui.on_context_menu_open_with_row_offset(
        |kind,
         selected_count,
         is_dir,
         default_open_visible,
         add_to_places_visible,
         clipboard_has_paths,
         place_builtin,
         item_height,
         separator_height,
         title_height| {
            context_menu_metrics(MenuMetricsInput {
                kind,
                selected_count,
                is_dir,
                default_open_visible,
                add_to_places_visible,
                clipboard_has_paths,
                place_builtin,
                item_height,
                separator_height,
                title_height,
            })
            .open_with_row_y_offset
        },
    );

    ui.on_context_menu_create_new_row_offset(
        |kind,
         selected_count,
         is_dir,
         default_open_visible,
         add_to_places_visible,
         clipboard_has_paths,
         place_builtin,
         item_height,
         separator_height,
         title_height| {
            context_menu_metrics(MenuMetricsInput {
                kind,
                selected_count,
                is_dir,
                default_open_visible,
                add_to_places_visible,
                clipboard_has_paths,
                place_builtin,
                item_height,
                separator_height,
                title_height,
            })
            .create_new_row_y_offset
        },
    );

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
                    start_privileged_operation(&ui, &bridge, command);
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
                set_status(&ui, "Administrator operation cancelled");
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_commit_external_edit(move || {
            if let Some(ui) = ui_weak.upgrade() {
                start_external_edit_resolution(&ui, &state, &bridge, "Save Back");
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        let bridge = bridge.clone();
        ui.on_discard_external_edit(move || {
            if let Some(ui) = ui_weak.upgrade() {
                start_external_edit_resolution(&ui, &state, &bridge, "Discard");
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
        ui.on_reorder_place_path(move |path, to| {
            if let Some(ui) = ui_weak.upgrade() {
                reorder_place_path(&ui, &state, path.as_str(), to);
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

#[derive(Clone, Copy)]
struct RootMenuGeometry {
    view_width: f32,
    view_height: f32,
    anchor_x: f32,
    anchor_y: f32,
    menu_width: f32,
    menu_height: f32,
    margin: f32,
    pointer_gap: f32,
}

impl RootMenuGeometry {
    fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .root_popup(
            self.anchor_x,
            self.anchor_y,
            self.menu_width,
            self.menu_height,
        )
    }
}

#[derive(Clone, Copy)]
struct AnchoredMenuGeometry {
    view_width: f32,
    view_height: f32,
    anchor_x: f32,
    anchor_y: f32,
    menu_width: f32,
    menu_height: f32,
    margin: f32,
    pointer_gap: f32,
    gap: f32,
}

impl AnchoredMenuGeometry {
    fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .anchored_popup_above(
            self.anchor_x,
            self.anchor_y,
            self.menu_width,
            self.menu_height,
            self.gap,
        )
    }
}

#[derive(Clone, Copy)]
struct ChildMenuGeometry {
    view_width: f32,
    view_height: f32,
    parent_left: f32,
    parent_width: f32,
    row_y: f32,
    child_width: f32,
    child_height: f32,
    margin: f32,
    pointer_gap: f32,
    child_gap: f32,
}

impl ChildMenuGeometry {
    fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .child_popup(ChildPopupInput {
            parent_left: self.parent_left,
            parent_width: self.parent_width,
            row_y: self.row_y,
            child_width: self.child_width,
            child_height: self.child_height,
            child_gap: self.child_gap,
        })
    }
}

#[derive(Clone, Copy)]
struct ChildBridgeGeometry {
    view_width: f32,
    view_height: f32,
    parent_left: f32,
    parent_width: f32,
    child_left: f32,
    child_width: f32,
    row_y: f32,
    child_top: f32,
    row_height: f32,
    title_height: f32,
    margin: f32,
    pointer_gap: f32,
    child_gap: f32,
}

impl ChildBridgeGeometry {
    fn rect(self) -> PopupRect {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .hover_bridge(HoverBridgeInput {
            parent_left: self.parent_left,
            parent_width: self.parent_width,
            child_left: self.child_left,
            child_width: self.child_width,
            row_y: self.row_y,
            child_top: self.child_top,
            row_height: self.row_height,
            title_height: self.title_height,
            child_gap: self.child_gap,
        })
    }
}

fn select_winit_backend_for_external_drops(
    async_tx: mpsc::Sender<AsyncEvent>,
    ui_weak: Rc<RefCell<Option<slint::Weak<AppWindow>>>>,
) -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_custom_application_handler(ExternalDropHandler {
            async_tx,
            ui_weak,
            last_cursor_position: None,
            file_drop_fallback_enabled: winit_file_drop_fallback_enabled_from_env(),
        })
        .select()
}

struct ExternalDropHandler {
    async_tx: mpsc::Sender<AsyncEvent>,
    ui_weak: Rc<RefCell<Option<slint::Weak<AppWindow>>>>,
    last_cursor_position: Option<(f32, f32)>,
    file_drop_fallback_enabled: bool,
}

impl slint::winit_030::CustomApplicationHandler for ExternalDropHandler {
    fn window_event(
        &mut self,
        _event_loop: &slint::winit_030::winit::event_loop::ActiveEventLoop,
        _window_id: slint::winit_030::winit::window::WindowId,
        _winit_window: Option<&slint::winit_030::winit::window::Window>,
        _slint_window: Option<&slint::Window>,
        event: &slint::winit_030::winit::event::WindowEvent,
    ) -> slint::winit_030::EventResult {
        use slint::winit_030::winit::event::{ElementState, MouseButton, WindowEvent};

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_position = Some((position.x as f32, position.y as f32));
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if matches!(button, MouseButton::Back | MouseButton::Forward) => {
                let scale = _winit_window.map_or(1.0, |window| window.scale_factor()) as f32;
                let Some((x, y)) = self.last_cursor_position else {
                    return slint::winit_030::EventResult::Propagate;
                };
                let logical_x = x / scale;
                let logical_y = y / scale;
                let Some(ui_weak) = self.ui_weak.borrow().clone() else {
                    return slint::winit_030::EventResult::Propagate;
                };
                let Some(ui) = ui_weak.upgrade() else {
                    return slint::winit_030::EventResult::Propagate;
                };
                let window_size = ui.window().size().to_logical(ui.window().scale_factor());
                if !point_in_main_pane(
                    ui.get_sidebar_width_px(),
                    window_size.width,
                    window_size.height,
                    logical_x,
                    logical_y,
                ) {
                    debug_log(&format!(
                        "winit side button ignored outside main pane button={button:?} x={logical_x:.1} y={logical_y:.1}"
                    ));
                    return slint::winit_030::EventResult::Propagate;
                }

                let ui_weak_for_event_loop = ui.as_weak();
                let button = *button;
                debug_log(&format!(
                    "winit side button accepted button={button:?} x={logical_x:.1} y={logical_y:.1}"
                ));
                let _ = ui_weak_for_event_loop.upgrade_in_event_loop(move |ui| match button {
                    MouseButton::Back => ui.invoke_go_back(),
                    MouseButton::Forward => ui.invoke_go_forward(),
                    _ => {}
                });
                return slint::winit_030::EventResult::PreventDefault;
            }
            WindowEvent::DroppedFile(path) => {
                if !self.file_drop_fallback_enabled {
                    return slint::winit_030::EventResult::Propagate;
                }
                let scale = _winit_window.map_or(1.0, |window| window.scale_factor()) as f32;
                let (x, y) = self.last_cursor_position.unwrap_or_default();
                let _ = self
                    .async_tx
                    .send(AsyncEvent::ExternalFileDropped(ExternalFileDrop {
                        path: path.clone(),
                        x: x / scale,
                        y: y / scale,
                        source: "winit DroppedFile fallback".to_string(),
                    }));
                if let Some(ui_weak) = self.ui_weak.borrow().clone() {
                    let _ = ui_weak.upgrade_in_event_loop(|ui| ui.invoke_async_results_ready());
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.last_cursor_position = None;
            }
            _ => {}
        }

        slint::winit_030::EventResult::Propagate
    }
}

fn winit_file_drop_fallback_enabled_from_env() -> bool {
    env::var("FIKA_DISABLE_WINIT_DROP_FALLBACK")
        .map(|value| !env_flag_is_truthy(&value))
        .unwrap_or(true)
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn sanitize_locale_for_icu4x() {
    // This runs before Slint creates windows or worker threads. Slint 1.16.1's
    // text stack can ask ICU4X for segmentation models that are not shipped in
    // the selected data set, so force a neutral UTF-8 locale for now.
    unsafe {
        env::set_var("LC_ALL", "C.UTF-8");
        env::set_var("LC_CTYPE", "C.UTF-8");
        env::set_var("LC_MESSAGES", "C.UTF-8");
        env::set_var("LANG", "C.UTF-8");
        env::set_var("LANGUAGE", "C");
    }
}

fn start_privileged_operation(
    ui: &AppWindow,
    bridge: &AsyncBridge,
    command: privilege::PrivilegedCommand,
) {
    set_status(
        ui,
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
    let current_dir = state.borrow().current_dir.clone();
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    save_settings(&AppSettings {
        dark_mode: Some(ui.get_dark_mode()),
        sidebar_width_px: Some(ui.get_sidebar_width_px()),
        icon_zoom_level: Some(ui.get_icon_zoom_level()),
        window_width_px: Some(window_size.width),
        window_height_px: Some(window_size.height),
        last_dir: Some(current_dir),
    });
}

fn remember_current_view_state(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    let current_dir = state.current_dir.clone();
    state.view_state_cache.insert(
        current_dir,
        DirectoryViewState {
            viewport_x: ui.get_main_viewport_x(),
        },
    );
}

fn restore_view_state(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: &Path) {
    let view_state = state
        .borrow()
        .view_state_cache
        .get(path)
        .copied()
        .unwrap_or_default();
    ui.set_main_viewport_x(view_state.viewport_x);
    ui.set_main_viewport_offset(-view_state.viewport_x);
}

fn load_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    load_directory_with_preservation(ui, state, bridge, false);
}

fn refresh_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    load_directory_with_preservation(ui, state, bridge, true);
}

fn load_directory_with_preservation(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    preserve_view: bool,
) {
    sync_devices(ui);
    let (current_dir, generation, cached_entries) = {
        let mut state = state.borrow_mut();
        cancel_active_search(&mut state);
        let generation = state.load_generation.next();
        state.open_generation.next();
        state.search_generation.next();
        state.thumbnail_generation.next();
        state.thumbnail_pending.clear();
        if !preserve_view {
            state.search_query.clear();
            state.search_kind_filter = 0;
            state.search_modified_filter = 0;
            state.search_size_filter = 0;
            state.selected_paths.clear();
            state.selection_anchor = None;
        }
        let current_dir = state.current_dir.clone();
        let cached_entries = state.directory_cache.get(&current_dir).cloned();
        (current_dir, generation, cached_entries)
    };
    debug_log(&format!(
        "load_directory generation={generation} preserve_view={preserve_view} path={} cache_hit={}",
        current_dir.display(),
        cached_entries.is_some()
    ));
    let current_path = current_dir.display().to_string();
    ui.set_current_path(current_path.as_str().into());
    ui.set_path_input_text(current_path.into());
    ui.set_current_name(display_location_name(&current_dir).into());
    ui.set_search_loading(false);
    if !preserve_view {
        restore_view_state(ui, state, &current_dir);
    }
    save_current_settings(ui, state);
    if preserve_view {
        ui.set_directory_loading(false);
        set_status(ui, "Refreshing folder...");
    } else if let Some(cached_entries) = cached_entries {
        {
            let mut state = state.borrow_mut();
            state.entries = cached_entries;
            state.virtual_view.invalidate();
        }
        ui.set_directory_loading(false);
        ui.set_search_query(SharedString::new());
        ui.set_search_kind_filter(0);
        ui.set_search_modified_filter(0);
        ui.set_search_size_filter(0);
        apply_filter(ui, state, bridge, false);
        set_status(ui, "Refreshing cached folder...");
    } else {
        ui.set_directory_loading(true);
        ui.set_search_query(SharedString::new());
        ui.set_search_kind_filter(0);
        ui.set_search_modified_filter(0);
        ui.set_search_size_filter(0);
        update_selection_ui(ui, &[]);
        set_status(ui, "Loading folder...");
    }
    watch_current_directory(&current_dir, generation, bridge);

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = read_entries_async(&current_dir).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                generation,
                path: current_dir,
                preserve_view,
                result,
            }),
        );
    });
}

fn sync_devices(ui: &AppWindow) {
    ui.set_devices(ModelRc::new(Rc::new(VecModel::from(mounted_devices()))));
}

fn watch_current_directory(path: &Path, generation: u64, bridge: &AsyncBridge) {
    use notify::Watcher;

    let watched_path = path.to_path_buf();
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

            let result = read_entries_async(&reload_path).await;
            send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::DirectoryLoaded(DirectoryLoadResult {
                    generation,
                    path: reload_path,
                    preserve_view: true,
                    result,
                }),
            );
        });
    });

    let Ok(mut watcher) = watcher else {
        *bridge.directory_watcher.borrow_mut() = None;
        return;
    };

    if watcher
        .watch(path, notify::RecursiveMode::NonRecursive)
        .is_ok()
    {
        *bridge.directory_watcher.borrow_mut() = Some(watcher);
    } else {
        *bridge.directory_watcher.borrow_mut() = None;
    }
}

fn apply_async_event(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    event: AsyncEvent,
) {
    match event {
        AsyncEvent::DirectoryLoaded(result) => apply_directory_result(ui, state, bridge, result),
        AsyncEvent::FileOpened(result) => apply_file_open_result(ui, state, result),
        AsyncEvent::RecursiveSearchProgress(progress) => {
            apply_recursive_search_progress(ui, state, progress);
        }
        AsyncEvent::RecursiveSearchFinished(result) => {
            apply_recursive_search_result(ui, state, bridge, result);
        }
        AsyncEvent::OpenWithAppsLoaded(result) => {
            open_with::apply_open_with_apps_result(ui, result)
        }
        AsyncEvent::OtherApplicationAppsLoaded(result) => {
            open_with::apply_other_application_apps_result(ui, state, result);
        }
        AsyncEvent::DefaultAppSet(result) => open_with::apply_default_app_set_result(ui, result),
        AsyncEvent::FileActionFinished(result) => {
            let applied = file_actions::apply_file_action_result(ui, state, result);
            if let Some(undo) = applied.undo {
                register_undo(ui, state, undo);
            }
            if let Some(status) = applied.status {
                refresh_directory(ui, state, bridge);
                set_status(ui, &status);
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
        AsyncEvent::PrivilegedOperationFinished(result) => {
            apply_privileged_operation_result(ui, state, bridge, result);
        }
        AsyncEvent::ExternalEditFinished(result) => {
            apply_external_edit_result(ui, state, bridge, result);
        }
        AsyncEvent::ThumbnailLoaded { generation, load } => {
            apply_thumbnail_result(ui, state, bridge, generation, load);
        }
        AsyncEvent::ExternalFileDropped(drop) => apply_external_file_drop(ui, state, drop),
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
        if !state.load_generation.is_current(result.generation) || result.path != state.current_dir
        {
            debug_log(&format!(
                "directory_loaded stale generation={} path={} current={} current_generation_match={}",
                result.generation,
                result.path.display(),
                state.current_dir.display(),
                state.load_generation.is_current(result.generation)
            ));
            return;
        }
    }

    match result.result {
        Ok(entries) => {
            ui.set_directory_loading(false);
            debug_log(&format!(
                "directory_loaded ok generation={} path={} entries={} preserve_view={}",
                result.generation,
                result.path.display(),
                entries.len(),
                result.preserve_view
            ));
            {
                let mut state = state.borrow_mut();
                state.entries = entries.into_iter().map(to_file_entry).collect();
                let cache_entries = state.entries.clone();
                state.virtual_view.invalidate();
                state
                    .directory_cache
                    .insert(result.path.clone(), cache_entries);
                if !result.preserve_view {
                    state.search_query.clear();
                    state.search_kind_filter = 0;
                    state.search_modified_filter = 0;
                    state.search_size_filter = 0;
                    state.selected_paths.clear();
                }
            }
            if !result.preserve_view {
                ui.set_search_query(SharedString::new());
                ui.set_search_kind_filter(0);
                ui.set_search_modified_filter(0);
                ui.set_search_size_filter(0);
            }
            apply_filter(ui, state, bridge, result.preserve_view);
        }
        Err(err) => {
            ui.set_directory_loading(false);
            debug_log(&format!(
                "directory_loaded error generation={} path={} preserve_view={} error={err}",
                result.generation,
                result.path.display(),
                result.preserve_view
            ));
            {
                let mut state = state.borrow_mut();
                state.entries.clear();
                state.visible_entry_indices = None;
                state.virtual_view.invalidate();
                if !result.preserve_view {
                    state.search_query.clear();
                    state.search_kind_filter = 0;
                    state.search_modified_filter = 0;
                    state.search_size_filter = 0;
                    state.selected_paths.clear();
                }
            }
            if !result.preserve_view {
                ui.set_search_query(SharedString::new());
                ui.set_search_kind_filter(0);
                ui.set_search_modified_filter(0);
                ui.set_search_size_filter(0);
            }
            ui.set_entry_count(0);
            ui.set_virtual_start_index(0);
            ui.set_virtual_start_column(0);
            ui.set_virtual_entries(ModelRc::new(Rc::new(VecModel::from(
                Vec::<FileEntry>::new(),
            ))));
            if result.preserve_view {
                retain_visible_selection(ui, state, &[]);
            } else {
                update_selection_ui(ui, &[]);
            }
            set_status(ui, &format!("Cannot read directory: {err}"));
        }
    }
}

fn open_file_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    path: PathBuf,
) {
    let generation = {
        let mut state = state.borrow_mut();
        state.open_generation.next()
    };
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_str().unwrap_or("file"));
    set_status(ui, &format!("Opening {label}..."));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = open_default_with_privilege_fallback(path.clone()).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileOpened(FileOpenResult {
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
        if !state.open_generation.is_current(result.generation) {
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
                register_external_edit(ui, state, session);
                set_status(
                    ui,
                    &format!(
                        "Opened protected scratch copy with default app for {}; auto writeback active{}",
                        success.mime_type, launch_suffix
                    ),
                );
            } else {
                set_status(
                    ui,
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
            set_status(ui, &format!("Cannot open {label}: {err}"));
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
        state.search_query = query.clone();
        state.search_generation.next();
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
        state.search_generation.next();
        let query = state.search_query.clone();
        let progress = state.search_progress;
        if let Some(entries) = state.directory_cache.get(&state.current_dir).cloned() {
            state.entries = entries;
            state.virtual_view.invalidate();
        }
        (query, progress)
    };

    ui.set_search_loading(false);
    apply_filter(ui, state, bridge, true);
    if query.is_empty() {
        set_status(ui, "Recursive search cancelled");
    } else {
        set_status(
            ui,
            &recursive_search_cancelled_status(
                &query,
                progress.directories_scanned,
                progress.matches_found,
            ),
        );
    }
}

fn cancel_active_search(state: &mut AppState) {
    if let Some(cancel) = state.active_search_cancel.take() {
        cancel.store(true, AtomicOrdering::Relaxed);
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
        state.search_kind_filter = kind.clamp(0, 3);
        state.search_modified_filter = modified.clamp(0, 3);
        state.search_size_filter = size.clamp(0, 3);
    }

    apply_filter(ui, state, bridge, true);
    if ui.get_search_loading() {
        let query = state.borrow().search_query.clone();
        set_status(ui, &recursive_search_status(&query));
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
        let generation = state.search_generation.next();
        let cancel = Arc::new(AtomicBool::new(false));
        state.active_search_cancel = Some(cancel.clone());
        state.search_progress = search::SearchProgress::default();
        (state.current_dir.clone(), generation, cancel)
    };

    ui.set_search_loading(true);
    set_status(ui, &recursive_search_status(&query));
    {
        let mut state = state.borrow_mut();
        state.visible_entry_indices = None;
        state.virtual_view.invalidate();
    }
    ui.set_entry_count(0);
    ui.set_virtual_start_index(0);
    ui.set_virtual_start_column(0);
    ui.set_virtual_entries(ModelRc::new(Rc::new(VecModel::from(
        Vec::<FileEntry>::new(),
    ))));
    update_selection_ui(ui, &[]);

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
            .await;
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
        let stale = !state.search_generation.is_current(progress.generation)
            || state.current_dir != progress.root
            || state.search_query != progress.query
            || !ui.get_search_loading();
        if stale {
            return;
        }
    }
    state.borrow_mut().search_progress = progress.progress;

    set_status(
        ui,
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
        let stale = !state.search_generation.is_current(result.generation)
            || state.current_dir != result.root
            || state.search_query != result.query;
        if stale {
            return;
        }
        state.active_search_cancel = None;
    }
    ui.set_search_loading(false);

    match result.result {
        Ok(entries) => {
            let mut entries = entries.into_iter().map(to_file_entry).collect::<Vec<_>>();
            let size_px = thumbnail_size_px(ui);
            {
                let state_ref = state.borrow();
                decorate_entries_with_cached_thumbnails(&state_ref, &mut entries, size_px);
            }
            let total = entries.len();
            {
                let mut state = state.borrow_mut();
                state.entries = entries.clone();
                state.virtual_view.invalidate();
            }
            apply_filter(ui, state, bridge, true);
            let visible = filtered_entry_count(&state.borrow());
            set_status(ui, &recursive_search_finished_status(visible, total));
        }
        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
            set_status(
                ui,
                &format!("Recursive search for '{}' cancelled", result.query),
            );
        }
        Err(err) => set_status(ui, &format!("Recursive search failed: {err}")),
    }
}

fn recursive_search_status(query: &str) -> String {
    format!("Searching recursively for '{query}'...")
}

fn recursive_search_progress_status(
    query: &str,
    directories_scanned: usize,
    matches: usize,
) -> String {
    if directories_scanned == 0 {
        return recursive_search_status(query);
    }

    format!(
        "Searching recursively for '{query}'... {matches} result(s), {directories_scanned} folder(s) scanned"
    )
}

fn recursive_search_finished_status(visible: usize, total: usize) -> String {
    if visible == total {
        format!("{total} recursive search result(s)")
    } else {
        format!("{visible} of {total} recursive search result(s) after filters")
    }
}

fn recursive_search_cancelled_status(
    query: &str,
    directories_scanned: usize,
    matches: usize,
) -> String {
    if directories_scanned == 0 {
        return format!("Recursive search for '{query}' cancelled");
    }

    format!(
        "Recursive search for '{query}' cancelled after {directories_scanned} folder(s); {matches} result(s) discarded"
    )
}

fn apply_file_operation_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: FileOperationResult,
) {
    let source_parent = result.source.parent().map(Path::to_path_buf);
    let result_operation = result.operation.clone();
    let result_source = result.source.clone();
    let (refresh_current_dir, remaining) = {
        let mut state = state.borrow_mut();
        if state.active_operation == Some(result.id) {
            state.active_operation = None;
            state.active_operation_cancel = None;
        }
        state.directory_cache.remove(&result.target_dir);
        if let Some(source_parent) = &source_parent {
            state.directory_cache.remove(source_parent);
        }
        let refresh_current_dir = source_parent
            .as_ref()
            .is_some_and(|source_parent| source_parent == &state.current_dir)
            || state.current_dir == result.target_dir;
        (refresh_current_dir, state.operation_queue.len())
    };

    let mut requested_privilege = false;
    let status_message = match result.result {
        Ok(outcome) => {
            register_file_undo(
                ui,
                state,
                &result_operation,
                &result_source,
                &outcome.destination,
                outcome.overwritten_backup.clone(),
            );
            Some(format!(
                "{} complete: {}",
                operation_finished_label(&result_operation),
                outcome.destination.display()
            ))
        }
        Err(err) if privilege::is_permission_error(&err) => {
            if let Some(command) = result.privileged_command {
                file_actions::request_privileged_action(ui, state, command, &err);
                requested_privilege = true;
                None
            } else {
                Some(format!(
                    "{} failed: {err}",
                    operation_finished_label(&result.operation)
                ))
            }
        }
        Err(err) => Some(format!(
            "{} failed: {err}",
            operation_finished_label(&result.operation)
        )),
    };

    if refresh_current_dir {
        refresh_directory(ui, state, bridge);
    }
    if let Some(status_message) = status_message {
        if remaining == 0 {
            set_status(ui, &status_message);
        } else {
            set_status(ui, &format!("{status_message}; {remaining} queued"));
        }
    } else if requested_privilege && remaining > 0 {
        set_status(
            ui,
            &format!("Administrator privileges required; {remaining} queued"),
        );
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
        set_status(ui, "Nothing to undo");
        return;
    };

    set_status(
        ui,
        &format!("Undoing {}...", operation_finished_label(&undo.operation)),
    );
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let task_undo = undo.clone();
        let result = tokio::task::spawn_blocking(move || match task_undo.operation.as_str() {
            "create-folder" => fs::file_ops::undo_create_folder(&task_undo.destination),
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
    {
        let mut state = state.borrow_mut();
        if let Some(parent) = result.undo.original_source.parent() {
            state.directory_cache.remove(parent);
        }
        if let Some(parent) = result.undo.destination.parent() {
            state.directory_cache.remove(parent);
        }
        for item in &result.undo.items {
            if let Some(parent) = item.original_source.parent() {
                state.directory_cache.remove(parent);
            }
            if let Some(parent) = item.destination.parent() {
                state.directory_cache.remove(parent);
            }
        }
    }

    refresh_directory(ui, state, bridge);
    match result.result {
        Ok(message) => set_status(ui, &format!("Undo complete: {message}")),
        Err(err) => set_status(ui, &format!("Undo failed: {err}")),
    }
}

fn apply_file_operation_progress(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    progress: FileOperationProgress,
) {
    if state.borrow().active_operation != Some(progress.id) {
        return;
    }

    let label = progress
        .source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item");
    if progress.bytes_total == 0 {
        set_status(
            ui,
            &format!("{} {label}...", operation_label(&progress.operation)),
        );
    } else {
        let percent =
            (progress.bytes_done.saturating_mul(100) / progress.bytes_total.max(1)).min(100);
        set_status(
            ui,
            &format!(
                "{} {label}: {percent}% ({}/{})",
                operation_label(&progress.operation),
                format_bytes(progress.bytes_done),
                format_bytes(progress.bytes_total)
            ),
        );
    }
}

fn apply_privileged_operation_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    result: privilege::PrivilegedOperationResult,
) {
    let refresh_current_dir = {
        let mut state = state.borrow_mut();
        for dir in &result.affected_dirs {
            state.directory_cache.remove(dir);
        }
        result
            .affected_dirs
            .iter()
            .any(|dir| dir == &state.current_dir)
    };

    if refresh_current_dir {
        refresh_directory(ui, state, bridge);
    }

    match result.result {
        Ok(message) => set_status(ui, &format!("{} complete: {message}", result.label)),
        Err(err) => set_status(ui, &format!("{} failed: {err}", result.label)),
    }
}

fn register_external_edit(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    session: privilege::ExternalEditSession,
) {
    {
        let mut state = state.borrow_mut();
        state.external_edits.push(session);
    }
    sync_external_edit_ui(ui, state);
}

fn start_external_edit_resolution(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    operation: &str,
) {
    let session = {
        let state = state.borrow();
        state.external_edits.last().cloned()
    };
    let Some(session) = session else {
        ui.set_external_edit_active(false);
        ui.set_external_edit_status(SharedString::new());
        set_status(ui, "No protected edit is pending");
        return;
    };

    set_status(
        ui,
        match operation {
            "Save Back" => "Saving protected edit back...",
            "Discard" => "Discarding protected edit...",
            _ => "Resolving protected edit...",
        },
    );

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    let operation = operation.to_string();
    bridge.handle.spawn(async move {
        let result = if operation == "Save Back" {
            privilege::commit_external_edit_via_dbus(&session).await
        } else {
            privilege::discard_external_edit_via_dbus(&session).await
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ExternalEditFinished(ExternalEditResult {
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
            .retain(|session| session.token != result.session.token);
    }
    sync_external_edit_ui(ui, state);

    match result.result {
        Ok(path) => {
            if result.operation == "Save Back" {
                if let Some(parent) = path.parent()
                    && parent == state.borrow().current_dir
                {
                    refresh_directory(ui, state, bridge);
                }
                set_status(
                    ui,
                    &format!("Protected edit saved back: {}", path.display()),
                );
            } else {
                set_status(ui, &format!("Protected edit discarded: {}", path.display()));
            }
        }
        Err(err) => set_status(ui, &format!("{} failed: {err}", result.operation)),
    }
}

fn sync_external_edit_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    let count = state.external_edits.len();
    ui.set_external_edit_active(count > 0);
    if count == 0 {
        ui.set_external_edit_status(SharedString::new());
    } else if count == 1 {
        let label = state
            .external_edits
            .last()
            .and_then(|session| session.original_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("protected file");
        ui.set_external_edit_status(format!("Protected edit: {label}").into());
    } else {
        ui.set_external_edit_status(format!("{count} protected edits").into());
    }
}

fn operation_finished_label(operation: &str) -> &'static str {
    match operation {
        "move" => "Move",
        "copy" => "Copy",
        "link" => "Link",
        "create-folder" => "Create Folder",
        "rename" => "Rename",
        "trash" => "Move to Trash",
        _ => "Operation",
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

fn sync_virtual_entries(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    schedule_thumbnails: bool,
) {
    sync_virtual_entries_with_count(ui, state, bridge, schedule_thumbnails, None);
}

fn sync_virtual_entries_with_count(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    schedule_thumbnails: bool,
    visible_count_override: Option<usize>,
) {
    let size_px = thumbnail_size_px(ui);
    let layout = MainGridLayout::from_ui(ui);
    let window_size = ui.window().size().to_logical(ui.window().scale_factor());
    let viewport_width = (window_size.width - ui.get_sidebar_width_px() - 8.0).max(1.0);
    let (range, visible_range, virtual_entries) = {
        let mut state_ref = state.borrow_mut();
        let visible_count =
            visible_count_override.unwrap_or_else(|| filtered_entry_count(&state_ref));
        let plan = virtual_grid_plan(
            visible_count,
            layout.rows_per_column,
            ui.get_main_viewport_x(),
            viewport_width,
            layout.cell_width,
            layout.padding,
            2,
        );
        if (plan.viewport_x - ui.get_main_viewport_x()).abs() > f32::EPSILON {
            ui.set_main_viewport_x(plan.viewport_x);
            ui.set_main_viewport_offset(-plan.viewport_x);
        }
        let should_rebuild_model = !schedule_thumbnails
            || state_ref.virtual_view.range != plan.range
            || state_ref.virtual_view.entry_count != visible_count
            || state_ref.virtual_view.rows_per_column != layout.rows_per_column
            || state_ref.virtual_view.cell_width != layout.cell_width
            || state_ref.virtual_view.thumbnail_size_px != size_px;

        ui.set_entry_count(visible_count as i32);

        if !should_rebuild_model {
            return;
        }

        let mut virtual_entries = filtered_entries_range(&state_ref, plan.range.clone());
        decorate_entries_with_cached_thumbnails(&state_ref, &mut virtual_entries, size_px);
        state_ref.virtual_view.range = plan.range.clone();
        state_ref.virtual_view.entry_count = visible_count;
        state_ref.virtual_view.rows_per_column = layout.rows_per_column;
        state_ref.virtual_view.cell_width = layout.cell_width;
        state_ref.virtual_view.thumbnail_size_px = size_px;
        ui.set_virtual_start_index(plan.range.start as i32);
        ui.set_virtual_start_column(plan.start_column as i32);
        (plan.range, plan.visible_range, virtual_entries)
    };
    ui.set_virtual_entries(ModelRc::new(Rc::new(VecModel::from(
        virtual_entries.clone(),
    ))));

    if schedule_thumbnails {
        let thumbnail_entries =
            prioritize_thumbnail_entries(&virtual_entries, range.start, visible_range);
        schedule_visible_thumbnails(ui, state, bridge, &thumbnail_entries, size_px, false);
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
        state_ref.virtual_view.invalidate();
        (
            state_ref.search_query.to_ascii_lowercase(),
            search_filters_active(&state_ref),
            state_ref.entries.len(),
            summary,
        )
    };
    ui.set_entry_count(summary.count as i32);
    sync_virtual_entries_with_count(ui, state, bridge, true, Some(summary.count));
    if preserve_selection {
        let empty_paths = Vec::new();
        let visible_paths = summary.visible_paths.as_ref().unwrap_or(&empty_paths);
        retain_visible_selection(ui, state, visible_paths);
    } else {
        clear_selection(ui, state);
    }

    if query.is_empty() && !filters_active {
        set_status(
            ui,
            &format!("{} folders, {} files", summary.folders, summary.files),
        );
    } else {
        set_status(
            ui,
            &format!(
                "{} of {total} items ({} folders, {} files)",
                summary.count, summary.folders, summary.files
            ),
        );
    }
}

fn search_filters_active(state: &AppState) -> bool {
    state.search_kind_filter != 0
        || state.search_modified_filter != 0
        || state.search_size_filter != 0
}

fn retain_visible_selection(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    visible_paths: &[String],
) {
    let selected_paths = {
        let mut state = state.borrow_mut();
        state.selected_paths = retained_visible_paths(&state.selected_paths, visible_paths);
        if state
            .selection_anchor
            .as_ref()
            .is_some_and(|anchor| !visible_paths.iter().any(|visible| visible == anchor))
        {
            state.selection_anchor = state.selected_paths.last().cloned();
        }
        state.selected_paths.clone()
    };
    update_selection_ui(ui, &selected_paths);
}

fn select_path(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    path: &str,
    toggle: bool,
    range: bool,
) {
    let selected_paths = {
        let mut state = state.borrow_mut();

        if range {
            let anchor = state
                .selection_anchor
                .as_deref()
                .or_else(|| state.selected_paths.last().map(String::as_str))
                .unwrap_or(path);
            let range_paths = selection_range_paths_filtered(&state, anchor, path);
            if toggle {
                append_unique_paths(&mut state.selected_paths, range_paths);
            } else {
                state.selected_paths = range_paths;
            }
        } else if toggle {
            if let Some(index) = state
                .selected_paths
                .iter()
                .position(|selected| selected == path)
            {
                state.selected_paths.remove(index);
            } else {
                state.selected_paths.push(path.to_string());
            }
        } else {
            state.selected_paths.clear();
            state.selected_paths.push(path.to_string());
        }

        if !range {
            state.selection_anchor = Some(path.to_string());
        }
        state.selected_paths.clone()
    };

    update_selection_ui(ui, &selected_paths);
}

fn select_all_visible(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let selected_paths = {
        let state = state.borrow();
        filtered_entry_paths(&state)
    };
    {
        let mut state = state.borrow_mut();
        state.selected_paths = selected_paths.clone();
        state.selection_anchor = selected_paths.last().cloned();
    }
    update_selection_ui(ui, &selected_paths);
}

fn select_rect(ui: &AppWindow, state: &Rc<RefCell<AppState>>, rect: SelectionRect, toggle: bool) {
    let selected_paths = {
        let mut state = state.borrow_mut();
        let selected = selection_rect_paths_filtered(&state, rect);
        if toggle {
            append_unique_paths(&mut state.selected_paths, selected);
        } else {
            state.selected_paths = selected;
        }
        state.selection_anchor = state.selected_paths.last().cloned();
        state.selected_paths.clone()
    };
    update_selection_ui(ui, &selected_paths);
}

fn clear_selection(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    state.selected_paths.clear();
    state.selection_anchor = None;
    drop(state);
    update_selection_ui(ui, &[]);
}

fn decorate_entries_with_cached_thumbnails(
    state: &AppState,
    entries: &mut [FileEntry],
    size_px: u32,
) {
    for entry in entries {
        if entry.is_dir || !thumbnails::is_thumbnail_candidate(Path::new(entry.path.as_str())) {
            continue;
        }
        let Ok(key) = thumbnails::key_for(Path::new(entry.path.as_str()), size_px) else {
            continue;
        };
        if let Some(data) = state.thumbnail_cache.get(&key) {
            entry.thumbnail = image_from_thumbnail(data);
            entry.thumbnail_state = 2;
        } else if state.thumbnail_failures.contains_key(&key) {
            entry.thumbnail_state = 0;
        } else if state.thumbnail_pending.contains_key(entry.path.as_str()) {
            entry.thumbnail_state = 1;
        }
    }
}

fn prioritize_thumbnail_entries(
    entries: &[FileEntry],
    virtual_start_index: usize,
    visible_range: Range<usize>,
) -> Vec<&FileEntry> {
    let visible_start = visible_range
        .start
        .saturating_sub(virtual_start_index)
        .min(entries.len());
    let visible_end = visible_range
        .end
        .saturating_sub(virtual_start_index)
        .min(entries.len())
        .max(visible_start);
    let mut prioritized = Vec::with_capacity(entries.len());
    prioritized.extend(entries[visible_start..visible_end].iter());
    prioritized.extend(entries[..visible_start].iter());
    prioritized.extend(entries[visible_end..].iter());
    prioritized
}

fn path_is_in_virtual_range(state: &AppState, path_text: &str) -> bool {
    let range_start = state.virtual_view.range.start;
    let range_end = state.virtual_view.range.end;
    if range_start >= range_end {
        return false;
    }

    if let Some(indices) = state.visible_entry_indices.as_ref() {
        let start = range_start.min(indices.len());
        let end = range_end.min(indices.len());
        if start >= end {
            return false;
        }

        return indices[start..end]
            .iter()
            .filter_map(|entry_index| state.entries.get(*entry_index))
            .any(|entry| entry.path.as_str() == path_text);
    }

    let start = range_start.min(state.entries.len());
    let end = range_end.min(state.entries.len());
    if start >= end {
        return false;
    }

    state.entries[start..end]
        .iter()
        .any(|entry| entry.path.as_str() == path_text)
}

fn thumbnail_schedule_candidate(
    state: &AppState,
    entry: &FileEntry,
    size_px: u32,
) -> Option<(PathBuf, thumbnails::ThumbnailKey)> {
    if entry.is_dir || entry.thumbnail_state == 2 {
        return None;
    }

    let path = PathBuf::from(entry.path.as_str());
    if !thumbnails::is_thumbnail_candidate(&path) {
        return None;
    }

    let Ok(key) = thumbnails::key_for(&path, size_px) else {
        return None;
    };
    if state.thumbnail_cache.contains_key(&key) || state.thumbnail_failures.contains_key(&key) {
        return None;
    }
    if state.thumbnail_pending.get(entry.path.as_str()) == Some(&key) {
        return None;
    }

    Some((path, key))
}

fn schedule_visible_thumbnails(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    entries: &[&FileEntry],
    size_px: u32,
    announce: bool,
) {
    let (generation, paths) = {
        let mut state = state.borrow_mut();
        let generation = state.thumbnail_generation.current();
        let mut paths = Vec::new();

        for entry in entries.iter().take(96) {
            let Some((path, key)) = thumbnail_schedule_candidate(&state, entry, size_px) else {
                continue;
            };
            state.thumbnail_pending.insert(entry.path.to_string(), key);
            paths.push(path);
        }

        (generation, paths)
    };

    if paths.is_empty() {
        return;
    }

    if announce {
        set_status(ui, "Loading thumbnails...");
    }
    for path in paths {
        let async_tx = bridge.tx.clone();
        let notify_ui = bridge.ui_weak.clone();
        bridge.handle.spawn(async move {
            let fallback_path = path.clone();
            let load = match tokio::task::spawn_blocking(move || {
                thumbnails::load_thumbnail(path, size_px)
            })
            .await
            {
                Ok(load) => load,
                Err(err) => thumbnails::ThumbnailLoad {
                    key: thumbnails::fallback_key(&fallback_path, size_px),
                    path: fallback_path,
                    data: Err(io::Error::other(format!("thumbnail task failed: {err}"))),
                },
            };
            send_async_event(
                async_tx,
                notify_ui,
                AsyncEvent::ThumbnailLoaded { generation, load },
            );
        });
    }
}

fn apply_thumbnail_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    generation: u64,
    load: thumbnails::ThumbnailLoad,
) {
    let path_text = load.path.display().to_string();
    let should_refresh_virtual = {
        let mut state = state.borrow_mut();
        apply_thumbnail_load_to_state(&mut state, generation, &path_text, load)
    };
    if should_refresh_virtual {
        sync_virtual_entries(ui, state, bridge, false);
    }
}

fn apply_thumbnail_load_to_state(
    state: &mut AppState,
    generation: u64,
    path_text: &str,
    load: thumbnails::ThumbnailLoad,
) -> bool {
    if !state.thumbnail_generation.is_current(generation) {
        remove_matching_thumbnail_pending(state, path_text, &load.key);
        return false;
    }

    remove_matching_thumbnail_pending(state, path_text, &load.key);

    match load.data {
        Ok(data) => {
            remove_thumbnail_failure(state, &load.key);
            insert_thumbnail_cache_with_limit(state, load.key, data);
        }
        Err(err) => {
            insert_thumbnail_failure_with_limit(state, load.key, err.to_string());
        }
    }

    path_is_in_virtual_range(state, path_text)
}

fn remove_matching_thumbnail_pending(
    state: &mut AppState,
    path_text: &str,
    key: &thumbnails::ThumbnailKey,
) {
    if state
        .thumbnail_pending
        .get(path_text)
        .is_some_and(|pending_key| pending_key == key)
    {
        state.thumbnail_pending.remove(path_text);
    }
}

fn remove_thumbnail_failure(state: &mut AppState, key: &thumbnails::ThumbnailKey) {
    state.thumbnail_failures.remove(key);
    state.thumbnail_failure_order.retain(|cached| cached != key);
}

fn insert_thumbnail_cache_with_limit(
    state: &mut AppState,
    key: thumbnails::ThumbnailKey,
    data: thumbnails::ThumbnailData,
) {
    state.thumbnail_cache_order.retain(|cached| cached != &key);
    state.thumbnail_cache.insert(key.clone(), data);
    state.thumbnail_cache_order.push_back(key);

    while state.thumbnail_cache_order.len() > MAX_THUMBNAIL_CACHE_ENTRIES {
        if let Some(oldest) = state.thumbnail_cache_order.pop_front() {
            state.thumbnail_cache.remove(&oldest);
        }
    }
}

fn insert_thumbnail_failure_with_limit(
    state: &mut AppState,
    key: thumbnails::ThumbnailKey,
    error: String,
) {
    state
        .thumbnail_failure_order
        .retain(|cached| cached != &key);
    state.thumbnail_failures.insert(key.clone(), error);
    state.thumbnail_failure_order.push_back(key);

    while state.thumbnail_failure_order.len() > MAX_THUMBNAIL_FAILURE_ENTRIES {
        if let Some(oldest) = state.thumbnail_failure_order.pop_front() {
            state.thumbnail_failures.remove(&oldest);
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

fn image_from_thumbnail(data: &thumbnails::ThumbnailData) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(data.width, data.height);
    buffer.make_mut_bytes().copy_from_slice(&data.rgba);
    Image::from_rgba8(buffer)
}

fn update_selection_ui(ui: &AppWindow, selected_paths: &[String]) {
    ui.set_selected_path(
        selected_paths
            .last()
            .map_or_else(SharedString::new, |path| path.as_str().into()),
    );
    ui.set_selected_count(selected_paths.len() as i32);
    ui.set_selected_status(match selected_paths {
        [] => SharedString::new(),
        [path] => format!("1 item selected: {path}").into(),
        paths => format!("{} items selected", paths.len()).into(),
    });
    ui.set_selection_revision(ui.get_selection_revision() + 1);
}

fn navigate_to(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge, path: PathBuf) {
    remember_current_view_state(ui, state);
    {
        let mut state_ref = state.borrow_mut();
        if state_ref.current_dir == path {
            debug_log(&format!(
                "navigate_to same path={} -> refresh",
                path.display()
            ));
            drop(state_ref);
            refresh_directory(ui, state, bridge);
            return;
        }

        let previous = state_ref.current_dir.clone();
        debug_log(&format!(
            "navigate_to from={} to={} back_len_before={} forward_len_before={}",
            previous.display(),
            path.display(),
            state_ref.back_stack.len(),
            state_ref.forward_stack.len()
        ));
        state_ref.back_stack.push(previous);
        state_ref.forward_stack.clear();
        state_ref.current_dir = path;
    }
    load_directory(ui, state, bridge);
}

fn go_parent(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    let parent = state.borrow().current_dir.parent().map(Path::to_path_buf);
    if let Some(parent) = parent {
        debug_log(&format!("go_parent target={}", parent.display()));
        navigate_to(ui, state, bridge, parent);
    } else {
        debug_log("go_parent no parent");
    }
}

fn go_back(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    remember_current_view_state(ui, state);
    {
        let mut state = state.borrow_mut();
        debug_log(&format!(
            "go_back requested current={} back_len={} forward_len={}",
            state.current_dir.display(),
            state.back_stack.len(),
            state.forward_stack.len()
        ));
        let Some(target) = state.back_stack.pop() else {
            debug_log("go_back ignored: empty back stack");
            set_status(ui, "No previous location");
            return;
        };

        let current = state.current_dir.clone();
        debug_log(&format!(
            "go_back accepted target={} previous_current={}",
            target.display(),
            current.display()
        ));
        state.forward_stack.push(current);
        state.current_dir = target;
    }
    load_directory(ui, state, bridge);
}

fn go_forward(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    remember_current_view_state(ui, state);
    {
        let mut state = state.borrow_mut();
        debug_log(&format!(
            "go_forward requested current={} back_len={} forward_len={}",
            state.current_dir.display(),
            state.back_stack.len(),
            state.forward_stack.len()
        ));
        let Some(target) = state.forward_stack.pop() else {
            debug_log("go_forward ignored: empty forward stack");
            set_status(ui, "No next location");
            return;
        };

        let current = state.current_dir.clone();
        debug_log(&format!(
            "go_forward accepted target={} previous_current={}",
            target.display(),
            current.display()
        ));
        state.back_stack.push(current);
        state.current_dir = target;
    }
    load_directory(ui, state, bridge);
}

fn open_path(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: &str, bridge: &AsyncBridge) {
    let (path, is_known_dir) = {
        let state = state.borrow();
        let entry = state
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
        navigate_to(ui, state, bridge, path);
        return;
    }

    if ui.get_chooser_mode() {
        let metadata = chooser_output_metadata(&state.borrow());
        output_chooser_paths_and_exit(vec![path], metadata);
    }

    open_file_async(ui, state, bridge, path);
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
                set_status(ui, "Cannot save: one or more file names are invalid");
            }
            return;
        }

        let Some(path) = safe_child_path(&target_dir, save_name) else {
            set_status(ui, "Cannot save: file name is invalid");
            return;
        };
        output_chooser_paths_and_exit(vec![path], chooser_output_metadata(&state_ref));
    } else if ui.get_chooser_select_directories() {
        output_chooser_paths_and_exit(
            vec![selected_directory_or_current(&state_ref)],
            chooser_output_metadata(&state_ref),
        );
    } else if !state_ref.selected_paths.is_empty() {
        let selected_files = state_ref
            .selected_paths
            .iter()
            .map(PathBuf::from)
            .filter(|path| !path.is_dir())
            .collect::<Vec<_>>();
        if selected_files.is_empty() {
            set_status(ui, "Choose a file, or double-click folders to enter them");
        } else if ui.get_chooser_multiple() {
            output_chooser_paths_and_exit(selected_files, chooser_output_metadata(&state_ref));
        } else {
            output_chooser_paths_and_exit(
                vec![selected_files[0].clone()],
                chooser_output_metadata(&state_ref),
            );
        }
    } else {
        set_status(ui, "Select a file to continue");
    }
}

fn parse_chooser_filter_spec(spec: &str) -> Option<ChooserFilter> {
    let (label, patterns) = spec.split_once('\t').unwrap_or((spec, ""));
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    let patterns = patterns
        .split(';')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(ChooserFilter {
        label: label.to_string(),
        patterns,
    })
}

fn parse_chooser_choice_spec(spec: &str) -> Option<StateChooserChoice> {
    let parts = spec.split('\t').collect::<Vec<_>>();
    let [id, label, selected, items] = parts.as_slice() else {
        return None;
    };
    if id.is_empty() || label.is_empty() {
        return None;
    }

    let items = items
        .split(';')
        .filter_map(|item| {
            let (item_id, item_label) = item.split_once('=')?;
            if item_id.is_empty() || item_label.is_empty() {
                return None;
            }
            Some(ChooserChoiceItem {
                id: item_id.to_string(),
                label: item_label.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    let selected_index = items
        .iter()
        .position(|item| item.id == *selected)
        .unwrap_or_default();

    Some(StateChooserChoice {
        id: (*id).to_string(),
        label: (*label).to_string(),
        items,
        selected_index,
    })
}

fn sync_chooser_filter_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let state = state.borrow();
    ui.set_chooser_filter_count(state.chooser_filters.len() as i32);
    ui.set_chooser_filter_index(state.chooser_filter_index as i32);
    ui.set_chooser_filter_label(
        state
            .chooser_filters
            .get(state.chooser_filter_index)
            .map(|filter| filter.label.as_str())
            .unwrap_or("")
            .into(),
    );
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
}

fn cycle_chooser_filter(ui: &AppWindow, state: &Rc<RefCell<AppState>>, bridge: &AsyncBridge) {
    {
        let mut state_ref = state.borrow_mut();
        if state_ref.chooser_filters.is_empty() {
            return;
        }
        state_ref.chooser_filter_index =
            (state_ref.chooser_filter_index + 1) % state_ref.chooser_filters.len();
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

fn set_chooser_choice_index(state: &mut AppState, choice_index: i32, option_index: i32) -> bool {
    let (Ok(choice_index), Ok(option_index)) =
        (usize::try_from(choice_index), usize::try_from(option_index))
    else {
        return false;
    };
    let Some(choice) = state.chooser_choices.get_mut(choice_index) else {
        return false;
    };
    if option_index >= choice.items.len() {
        return false;
    }
    choice.selected_index = option_index;
    true
}

#[derive(Clone, Debug, Default)]
struct ChooserOutputMetadata {
    filter_index: Option<usize>,
    choices: Vec<(String, String)>,
}

fn chooser_output_metadata(state: &AppState) -> ChooserOutputMetadata {
    ChooserOutputMetadata {
        filter_index: if state.chooser_return_filter && !state.chooser_filters.is_empty() {
            Some(state.chooser_filter_index)
        } else {
            None
        },
        choices: if state.chooser_return_choices {
            state
                .chooser_choices
                .iter()
                .filter_map(|choice| {
                    choice
                        .items
                        .get(choice.selected_index)
                        .map(|item| (choice.id.clone(), item.id.clone()))
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

fn selected_directory_or_current(state: &AppState) -> PathBuf {
    state
        .selected_paths
        .first()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| state.current_dir.clone())
}

fn safe_child_path(parent: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.as_bytes().contains(&0)
    {
        return None;
    }
    Some(parent.join(name))
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

fn set_status(ui: &AppWindow, message: &str) {
    ui.set_status(message.into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::geometry::{MainGridLayout, place_drop_geometry, virtual_entry_range};
    use crate::app::places::normalize_dropped_path;
    use crate::app::selection::{
        filtered_entry_at, filtered_entry_summary, rebuild_visible_entry_index,
        selection_range_paths, selection_rect_paths,
    };
    use crate::app::transfer::transfer_target_rejection;

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
        state.entries = vec![
            test_entry("Alpha.txt", "/tmp/Alpha.txt"),
            test_entry("Beta.txt", "/tmp/Beta.txt"),
            test_entry("notes.md", "/tmp/project-notes.md"),
        ];
        state.search_query = "project".to_string();

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

        state.entries = vec![folder, image, archive];

        state.search_kind_filter = 1;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Images".to_string()]
        );

        state.search_kind_filter = 3;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/photo.png".to_string()]
        );

        state.search_kind_filter = 0;
        state.search_size_filter = 3;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/archive.zip".to_string()]
        );

        state.search_size_filter = 0;
        state.search_modified_filter = 2;
        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Images".to_string(), "/tmp/photo.png".to_string()]
        );
    }

    #[test]
    fn filtered_entries_range_clones_only_requested_filtered_window() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.entries = (0..8)
            .map(|index| test_entry(&format!("item-{index}.txt"), &format!("/tmp/item-{index}")))
            .collect();
        state.search_query = "item".to_string();

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
        state.entries = vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
        ];
        state.search_query = ".txt".to_string();

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
        state.entries = vec![
            folder,
            test_entry("item-file.txt", "/tmp/item-file.txt"),
            test_entry("hidden.log", "/tmp/hidden.log"),
        ];
        state.search_query = "item".to_string();

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
        state.entries = vec![
            test_entry("alpha", "/tmp/alpha"),
            test_entry("beta", "/tmp/beta"),
        ];

        let summary = rebuild_visible_entry_index(&mut state, true);

        assert_eq!(summary.count, 2);
        assert!(state.visible_entry_indices.is_none());
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
        state.entries = vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ];
        state.search_query = ".txt".to_string();

        let summary = rebuild_visible_entry_index(&mut state, false);

        assert_eq!(summary.count, 3);
        assert_eq!(state.visible_entry_indices.as_deref(), Some(&[0, 2, 3][..]));
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
    fn recursive_search_status_keeps_query_visible_during_background_scan() {
        assert_eq!(
            recursive_search_status("report"),
            "Searching recursively for 'report'..."
        );
        assert_eq!(
            recursive_search_progress_status("report", 12, 4),
            "Searching recursively for 'report'... 4 result(s), 12 folder(s) scanned"
        );
        assert_eq!(
            recursive_search_finished_status(4, 4),
            "4 recursive search result(s)"
        );
        assert_eq!(
            recursive_search_finished_status(2, 4),
            "2 of 4 recursive search result(s) after filters"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 12, 4),
            "Recursive search for 'report' cancelled after 12 folder(s); 4 result(s) discarded"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 3, 0),
            "Recursive search for 'report' cancelled after 3 folder(s); 0 result(s) discarded"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 0, 0),
            "Recursive search for 'report' cancelled"
        );
    }

    #[test]
    fn chooser_filter_specs_filter_files_but_keep_folders_visible() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut folder = test_entry("Documents", "/tmp/Documents");
        folder.is_dir = true;
        folder.kind = "Folder".into();
        state.entries = vec![
            folder,
            test_entry("photo.PNG", "/tmp/photo.PNG"),
            test_entry("notes.txt", "/tmp/notes.txt"),
        ];
        state.chooser_filters = vec![parse_chooser_filter_spec("Images\t*.png;*.jpg").unwrap()];

        assert_eq!(
            filtered_entry_paths(&state),
            vec!["/tmp/Documents".to_string(), "/tmp/photo.PNG".to_string()]
        );
    }

    #[test]
    fn chooser_choice_specs_parse_and_emit_selected_metadata() {
        let choice =
            parse_chooser_choice_spec("encoding\tEncoding\tlatin1\tutf8=UTF-8;latin1=Latin-1")
                .unwrap();
        assert_eq!(choice.id, "encoding");
        assert_eq!(choice.label, "Encoding");
        assert_eq!(choice.selected_index, 1);

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.chooser_choices = vec![choice];
        state.chooser_return_choices = true;
        assert!(set_chooser_choice_index(&mut state, 0, 0));
        assert!(!set_chooser_choice_index(&mut state, 0, 9));
        assert!(!set_chooser_choice_index(&mut state, 9, 0));
        assert_eq!(
            chooser_output_metadata(&state).choices,
            vec![("encoding".to_string(), "utf8".to_string())]
        );
    }

    #[test]
    fn thumbnail_cache_evicts_oldest_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        for index in 0..(MAX_THUMBNAIL_CACHE_ENTRIES + 3) {
            insert_thumbnail_cache_with_limit(
                &mut state,
                thumbnails::fallback_key(Path::new(&format!("/tmp/{index}.png")), 64),
                thumbnails::ThumbnailData {
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 0],
                },
            );
        }

        assert_eq!(state.thumbnail_cache.len(), MAX_THUMBNAIL_CACHE_ENTRIES);
        assert_eq!(
            state.thumbnail_cache_order.len(),
            MAX_THUMBNAIL_CACHE_ENTRIES
        );
        assert!(
            !state
                .thumbnail_cache
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/0.png"), 64))
        );
        assert!(
            state
                .thumbnail_cache
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/3.png"), 64))
        );
    }

    #[test]
    fn thumbnail_cache_refreshes_existing_entry_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let first = thumbnails::fallback_key(Path::new("/tmp/first.png"), 64);
        let second = thumbnails::fallback_key(Path::new("/tmp/second.png"), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };

        insert_thumbnail_cache_with_limit(&mut state, first.clone(), data.clone());
        insert_thumbnail_cache_with_limit(&mut state, second.clone(), data.clone());
        insert_thumbnail_cache_with_limit(&mut state, first.clone(), data);

        assert_eq!(state.thumbnail_cache_order.pop_back(), Some(first));
        assert_eq!(state.thumbnail_cache_order.pop_front(), Some(second));
    }

    #[test]
    fn thumbnail_failure_cache_evicts_oldest_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        for index in 0..(MAX_THUMBNAIL_FAILURE_ENTRIES + 3) {
            insert_thumbnail_failure_with_limit(
                &mut state,
                thumbnails::fallback_key(Path::new(&format!("/tmp/{index}.png")), 64),
                "decode failed".to_string(),
            );
        }

        assert_eq!(
            state.thumbnail_failures.len(),
            MAX_THUMBNAIL_FAILURE_ENTRIES
        );
        assert_eq!(
            state.thumbnail_failure_order.len(),
            MAX_THUMBNAIL_FAILURE_ENTRIES
        );
        assert!(
            !state
                .thumbnail_failures
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/0.png"), 64))
        );
        assert!(
            state
                .thumbnail_failures
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/3.png"), 64))
        );
    }

    #[test]
    fn thumbnail_failure_cache_refreshes_existing_entry_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let first = thumbnails::fallback_key(Path::new("/tmp/first.png"), 64);
        let second = thumbnails::fallback_key(Path::new("/tmp/second.png"), 64);

        insert_thumbnail_failure_with_limit(&mut state, first.clone(), "first".to_string());
        insert_thumbnail_failure_with_limit(&mut state, second.clone(), "second".to_string());
        insert_thumbnail_failure_with_limit(&mut state, first.clone(), "first again".to_string());

        assert_eq!(state.thumbnail_failure_order.pop_back(), Some(first));
        assert_eq!(state.thumbnail_failure_order.pop_front(), Some(second));
    }

    #[test]
    fn thumbnail_schedule_skips_failed_key_until_file_changes() {
        let temp_dir =
            std::env::temp_dir().join(format!("fika-thumbnail-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let image_path = temp_dir.join("broken.png");
        std::fs::write(&image_path, b"not an image").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let entry = test_entry("broken.png", image_path.to_str().unwrap());
        let key = thumbnails::key_for(&image_path, 64).unwrap();
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_some());

        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_none());

        remove_thumbnail_failure(&mut state, &key);
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn successful_thumbnail_result_removes_failure_marker() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let key = thumbnails::fallback_key(Path::new("/tmp/photo.png"), 64);

        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());
        assert!(state.thumbnail_failures.contains_key(&key));

        remove_thumbnail_failure(&mut state, &key);
        assert!(!state.thumbnail_failures.contains_key(&key));
        assert!(!state.thumbnail_failure_order.contains(&key));
    }

    #[test]
    fn stale_thumbnail_result_only_clears_matching_pending_key() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let path = "/tmp/photo.png";
        let old_key = thumbnails::fallback_key(Path::new(path), 64);
        let new_key = thumbnails::fallback_key(Path::new(path), 128);

        state
            .thumbnail_pending
            .insert(path.to_string(), new_key.clone());
        remove_matching_thumbnail_pending(&mut state, path, &old_key);
        assert_eq!(state.thumbnail_pending.get(path), Some(&new_key));

        remove_matching_thumbnail_pending(&mut state, path, &new_key);
        assert!(!state.thumbnail_pending.contains_key(path));
    }

    #[test]
    fn thumbnail_success_result_updates_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };

        state.entries = vec![test_entry("photo.png", path)];
        state.virtual_view.range = 0..1;
        state
            .thumbnail_pending
            .insert(path.to_string(), key.clone());
        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                data: Ok(data),
            },
        );

        assert!(should_refresh);
        assert!(!state.thumbnail_pending.contains_key(path));
        assert!(state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.entries[0].thumbnail_state, 0);
    }

    #[test]
    fn thumbnail_failure_result_updates_failure_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);

        let mut entry = test_entry("photo.png", path);
        entry.thumbnail_state = 1;
        state.entries = vec![entry];
        state.virtual_view.range = 0..1;
        state
            .thumbnail_pending
            .insert(path.to_string(), key.clone());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                data: Err(io::Error::other("decode failed")),
            },
        );

        assert!(should_refresh);
        assert!(!state.thumbnail_pending.contains_key(path));
        assert!(state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.entries[0].thumbnail_state, 1);
    }

    #[test]
    fn stale_thumbnail_result_does_not_update_thumbnail_caches() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let stale_generation = state.thumbnail_generation.current();
        state.thumbnail_generation.next();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };
        state
            .thumbnail_pending
            .insert(path.to_string(), key.clone());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            stale_generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                data: Ok(data),
            },
        );

        assert!(!should_refresh);
        assert!(!state.thumbnail_pending.contains_key(path));
        assert!(!state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
    }

    #[test]
    fn virtual_entry_range_keeps_visible_columns_with_overscan() {
        assert_eq!(virtual_entry_range(100, 4, 0.0, 250.0, 100.0, 1), 0..20);
        assert_eq!(virtual_entry_range(100, 4, 350.0, 250.0, 100.0, 1), 8..32);
        assert_eq!(virtual_entry_range(10, 4, 800.0, 250.0, 100.0, 1), 10..10);
    }

    #[test]
    fn thumbnail_priority_schedules_visible_entries_before_overscan() {
        let entries = (8..20)
            .map(|index| test_entry(&format!("item-{index}.png"), &format!("/tmp/{index}.png")))
            .collect::<Vec<_>>();

        let prioritized = prioritize_thumbnail_entries(&entries, 8, 12..16)
            .into_iter()
            .map(|entry| entry.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            prioritized,
            vec![
                "item-12.png".to_string(),
                "item-13.png".to_string(),
                "item-14.png".to_string(),
                "item-15.png".to_string(),
                "item-8.png".to_string(),
                "item-9.png".to_string(),
                "item-10.png".to_string(),
                "item-11.png".to_string(),
                "item-16.png".to_string(),
                "item-17.png".to_string(),
                "item-18.png".to_string(),
                "item-19.png".to_string(),
            ]
        );
    }

    #[test]
    fn virtual_range_path_lookup_uses_identity_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.entries = (0..6)
            .map(|index| test_entry(&format!("item-{index}.png"), &format!("/tmp/{index}.png")))
            .collect();
        state.virtual_view.range = 2..5;

        assert!(path_is_in_virtual_range(&state, "/tmp/2.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/4.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/1.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/5.png"));
    }

    #[test]
    fn virtual_range_path_lookup_uses_filtered_visible_indices() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.entries = vec![
            test_entry("alpha.png", "/tmp/alpha.png"),
            test_entry("skip.log", "/tmp/skip.log"),
            test_entry("beta.png", "/tmp/beta.png"),
            test_entry("gamma.png", "/tmp/gamma.png"),
        ];
        state.search_query = ".png".to_string();
        rebuild_visible_entry_index(&mut state, false);
        state.virtual_view.range = 1..3;

        assert!(path_is_in_virtual_range(&state, "/tmp/beta.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/gamma.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/skip.log"));
    }

    #[test]
    fn virtual_range_path_lookup_rejects_empty_or_stale_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.entries = vec![test_entry("alpha.png", "/tmp/alpha.png")];

        state.virtual_view.range = 0..0;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));

        state.virtual_view.range = 9..12;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
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
        state.entries = vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ];
        state.search_query = ".txt".to_string();

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
        state.entries = vec![
            test_entry("alpha.txt", "/tmp/alpha"),
            test_entry("skip.log", "/tmp/skip"),
            test_entry("beta.txt", "/tmp/beta"),
            test_entry("gamma.txt", "/tmp/gamma"),
        ];
        state.search_query = ".txt".to_string();

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
        state.entries = (0..20)
            .map(|index| test_entry(&format!("entry-{index}"), &format!("/tmp/{index}")))
            .collect();

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
    fn main_grid_layout_maps_points_to_column_first_indices() {
        let layout = MainGridLayout {
            main_x: 328.0,
            main_y: 64.0,
            viewport_x: 0.0,
            rows_per_column: 3,
            cell_width: 100.0,
            row_height: 50.0,
            padding: 10.0,
        };

        assert_eq!(layout.index_at_point(338.0, 74.0), Some(0));
        assert_eq!(layout.index_at_point(338.0, 124.0), Some(1));
        assert_eq!(layout.index_at_point(438.0, 74.0), Some(3));
        assert_eq!(layout.index_at_point(426.5, 74.0), None);
    }

    #[test]
    fn place_drop_geometry_slot_clamps_to_places_range() {
        assert_eq!(place_drop_geometry(90.0, 3, 108.0, 38.0, true).slot, 0);
        assert_eq!(place_drop_geometry(146.0, 3, 108.0, 38.0, true).slot, 1);
        assert_eq!(place_drop_geometry(500.0, 3, 108.0, 38.0, true).slot, 3);
        assert_eq!(place_drop_geometry(222.0, 4, 190.0, 38.0, true).slot, 1);
    }

    #[test]
    fn decodes_file_uri_drop_payload() {
        assert_eq!(
            normalize_dropped_path(PathBuf::from(
                "# comment\n\nfile://localhost/tmp/Hello%20World\nfile:///tmp/Second\n",
            ))
            .to_string_lossy(),
            "/tmp/Hello World"
        );
    }

    #[test]
    fn env_flag_truthy_values_disable_winit_drop_fallback() {
        for value in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert!(env_flag_is_truthy(value));
        }
        for value in ["", "0", "false", "no", "off", "anything-else"] {
            assert!(!env_flag_is_truthy(value));
        }
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
            thumbnail_state: 0,
            thumbnail: Image::default(),
        }
    }
}
