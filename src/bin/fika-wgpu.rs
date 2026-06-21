use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use cosmic_text::{
    Align, Attrs, Buffer, Color as TextColor, Cursor, Family, FontSystem, Metrics, Shaping,
    SwashCache, Wrap,
};
use fika_core::{
    CompactLayout, CompactLayoutOptions, DesktopLaunchPlan, DeviceInfo, Entry, FileClipboardRole,
    FileTransferMode, Generation, IconsLayout, IconsLayoutOptions, ItemId, MimeApplication,
    MimeApplicationCache, NETWORK_ROOT_LABEL, NameFilter, OpenWithLaunchResult, ServiceMenuAction,
    ServiceMenuLaunchResult, ServiceMenuPriority, ServiceMenuTarget, ThumbnailRequest,
    ThumbnailRequestPriority, ThumbnailerRegistry, TransferTaskResult, TrashViewOperation,
    TrashViewOperationResult, UserPlace, ViewPoint, ViewRect, ViewSize, complete_location_input,
    decode_file_clipboard_text, default_thumbnail_cache_root, default_user_places_path,
    encode_file_clipboard_text, file_ops, format_modified_secs, format_size,
    generate_thumbnail_with_external_thumbnailer_registry, home_dir, is_network_path,
    launch_with_systemd_user, load_place_order, load_user_places, mime_magic_resolution_required,
    network_root_path, network_uri_from_path, paste_text_result,
    place_order_path_for_user_places_path, read_entries_sync, read_gio_devices,
    resolve_location_input, save_place_order, save_user_places, service_menu_target_label,
    thumbnail_request_may_have_preview, transfer_paths_result, trash_view_operation_result,
};
use gio::prelude::FileExt;
use winit::application::ApplicationHandler;
use winit::cursor::{Cursor as WinitCursor, CursorIcon};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

#[path = "fika_wgpu/clipboard.rs"]
mod wgpu_clipboard;
#[path = "fika_wgpu/location.rs"]
mod wgpu_location;
#[path = "fika_wgpu/metrics.rs"]
mod wgpu_metrics;
#[path = "fika_wgpu/options.rs"]
mod wgpu_options;
#[path = "fika_wgpu/pane.rs"]
mod wgpu_pane;
#[path = "fika_wgpu/pane_layout.rs"]
mod wgpu_pane_layout;
#[path = "fika_wgpu/selection.rs"]
mod wgpu_selection;

use wgpu_clipboard::ShellClipboard;
use wgpu_location::{LocationDraft, PathHistory, normalized_text_cursor};
use wgpu_metrics::*;
use wgpu_options::{ShellViewMode, parse_start_options};
use wgpu_pane::{
    ShellPaneGeometry, ShellPaneKind, ShellPaneProjection, ShellPaneScrollMetrics,
    ShellPaneSplitMetrics, ShellPaneState, ShellPaneView, ShellPaneVisibleItem,
    ShellVisibleItemSlotPool, ShellVisibleItemSlotStats,
};
use wgpu_pane_layout::{DetailsLayout, ShellCompactLayout, ShellLayout, navigation_target};
use wgpu_selection::{
    NavigationAction, RubberBand, RubberBandMode, SelectionClick, ShellSelection,
};

fn main() -> Result<(), Box<dyn Error>> {
    let Some(options) = parse_start_options()? else {
        return Ok(());
    };
    let scene = ShellScene::load(options.path, options.view_mode)?;

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = FikaWgpuApp::new(scene, options.auto_cycle_views);
    event_loop.run_app(app)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ScrollbarDragTarget {
    Content {
        pane: ShellPaneKind,
        axis: ContentScrollbarAxis,
    },
    Places,
    PlacesResize,
    SplitPaneResize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDrag {
    target: ScrollbarDragTarget,
    grab_offset: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathNavigationAction {
    Back,
    Forward,
    Parent,
}

impl PathNavigationAction {
    fn reason(self) -> &'static str {
        match self {
            Self::Back => "history-back",
            Self::Forward => "history-forward",
            Self::Parent => "parent-directory",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZoomAction {
    In,
    Out,
    Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectionCommand {
    SelectAll,
    Clear,
}

impl SelectionCommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::SelectAll => "select-all",
            Self::Clear => "clear",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FilterCommand {
    Activate,
    Insert(String),
    Backspace,
    ClearAndDeactivate,
    Deactivate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocationCommand {
    Activate,
    Insert(String),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    Cancel,
    Commit,
    Complete,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CreateCommand {
    Insert(String),
    Backspace,
    Cancel,
    Commit,
    SetKind(CreateEntryKind),
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RenameCommand {
    Insert(String),
    Backspace,
    Cancel,
    Commit,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OpenWithCommand {
    Insert(String),
    Backspace,
    Cancel,
    Commit,
    MoveUp,
    MoveDown,
    Ignore,
}

fn window_title(scene: &ShellScene) -> String {
    if let Some(split_pane) = scene.split_pane.as_ref() {
        format!(
            "Fika wgpu shell [{}] - {} | {}",
            scene.primary_pane.view_mode.as_str(),
            scene.primary_pane.path.display(),
            split_pane.path.display()
        )
    } else {
        format!(
            "Fika wgpu shell [{}] - {}",
            scene.primary_pane.view_mode.as_str(),
            scene.primary_pane.path.display()
        )
    }
}

struct FikaWgpuApp {
    scene: ShellScene,
    mime_applications: MimeApplicationCache,
    modifiers: Modifiers,
    // Drop order matters: renderer and clipboard borrow display/window handles,
    // so they must be dropped before the window.
    renderer: Option<WgpuState>,
    clipboard: Option<ShellClipboard>,
    window: Option<Box<dyn Window>>,
    cursor_icon: CursorIcon,
    pending_redraw_frames: u8,
    auto_cycle_views: bool,
    next_auto_cycle: Instant,
}

impl FikaWgpuApp {
    fn new(scene: ShellScene, auto_cycle_views: bool) -> Self {
        Self {
            scene,
            mime_applications: MimeApplicationCache::load(),
            modifiers: Modifiers::default(),
            renderer: None,
            clipboard: None,
            window: None,
            cursor_icon: CursorIcon::Default,
            pending_redraw_frames: 0,
            auto_cycle_views,
            next_auto_cycle: Instant::now() + AUTO_CYCLE_INTERVAL,
        }
    }

    fn set_window_cursor(&mut self, cursor_icon: CursorIcon) {
        if self.cursor_icon == cursor_icon {
            return;
        }
        self.cursor_icon = cursor_icon;
        if let Some(window) = self.window.as_ref() {
            window.set_cursor(WinitCursor::Icon(cursor_icon));
        }
    }

    fn update_window_cursor_for_scene(&mut self, size: PhysicalSize<u32>) {
        self.set_window_cursor(self.scene.cursor_icon(size));
    }
}

impl ApplicationHandler for FikaWgpuApp {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title(window_title(&self.scene))
            .with_surface_size(PhysicalSize::new(1100, 720));

        let window = match event_loop.create_window(attrs) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("[fika-wgpu] window create failed: {error}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match WgpuState::new(window.as_ref()) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("[fika-wgpu] renderer init failed: {error}");
                event_loop.exit();
                return;
            }
        };
        let clipboard = match ShellClipboard::from_window(window.as_ref()) {
            Ok(Some(clipboard)) => {
                eprintln!(
                    "[fika-wgpu] clipboard-ready backend={}",
                    clipboard.backend()
                );
                Some(clipboard)
            }
            Ok(None) => {
                eprintln!("[fika-wgpu] clipboard-unavailable backend=unsupported");
                None
            }
            Err(error) => {
                eprintln!("[fika-wgpu] clipboard-unavailable error={error}");
                None
            }
        };

        self.scene
            .set_scale_factor(window.scale_factor() as f32, renderer.size);

        eprintln!(
            "[fika-wgpu] shell-ready size={}x{} scale={:.2}",
            renderer.size.width,
            renderer.size.height,
            window.scale_factor()
        );

        self.scene.clamp_scroll(renderer.size);
        self.renderer = Some(renderer);
        self.clipboard = clipboard;
        self.window = Some(window);

        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.auto_cycle_views && Instant::now() >= self.next_auto_cycle {
            self.next_auto_cycle = Instant::now() + AUTO_CYCLE_INTERVAL;
            if let Some(renderer) = self.renderer.as_ref() {
                let next = self.scene.primary_pane.view_mode.next();
                if self.scene.set_view_mode(next, renderer.size) {
                    self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
                    if let Some(window) = self.window.as_ref() {
                        window.set_title(&window_title(&self.scene));
                        window.request_redraw();
                    }
                    self.render_now(event_loop, "auto-cycle", true);
                }
            }
        }

        let needs_redraw = self.renderer.as_ref().is_some_and(|renderer| {
            renderer.frame_count == 0
                || renderer.rendered_view_switches != self.scene.view_switches
                || self.pending_redraw_frames > 0
        });

        if needs_redraw && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }

        if needs_redraw {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if self.auto_cycle_views {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_auto_cycle));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.renderer = None;
                self.clipboard = None;
                self.window = None;
                event_loop.exit();
            }
            WindowEvent::SurfaceResized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                    self.scene.clamp_scroll(renderer.size);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(renderer), Some(window)) =
                    (self.renderer.as_mut(), self.window.as_ref())
                {
                    renderer.resize(window.surface_size());
                    self.scene
                        .set_scale_factor(window.scale_factor() as f32, renderer.size);
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                let shortcut =
                    self.modifiers.state().control_key() || self.modifiers.state().meta_key();
                if self.scene.is_open_with_chooser_open() {
                    match open_with_command_for_key_event(&event, shortcut) {
                        OpenWithCommand::Commit => self.commit_open_with_chooser(),
                        OpenWithCommand::Ignore => {}
                        command => {
                            if self.scene.apply_open_with_command(command)
                                && let Some(window) = self.window.as_ref()
                            {
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }
                if self.scene.is_trash_conflict_dialog_open() {
                    if escape_requested_for_key_event(&event) {
                        if self.scene.close_trash_conflict_dialog()
                            && let Some(window) = self.window.as_ref()
                        {
                            window.request_redraw();
                        }
                    }
                    return;
                }
                if self.scene.is_rename_dialog_open() {
                    match rename_command_for_key_event(&event, shortcut) {
                        RenameCommand::Commit => self.commit_rename_dialog(event_loop),
                        RenameCommand::Ignore => {}
                        command => {
                            if self.scene.apply_rename_command(command)
                                && let Some(window) = self.window.as_ref()
                            {
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }
                if self.scene.is_create_dialog_open() {
                    match create_command_for_key_event(&event, shortcut) {
                        CreateCommand::Commit => self.commit_create_dialog(event_loop),
                        CreateCommand::Ignore => {}
                        command => {
                            if self.scene.apply_create_command(command, renderer.size)
                                && let Some(window) = self.window.as_ref()
                            {
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }
                if self.scene.is_properties_overlay_open() && escape_requested_for_key_event(&event)
                {
                    if self.scene.close_properties_overlay()
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if self.scene.is_context_menu_open() && escape_requested_for_key_event(&event) {
                    if self.scene.close_context_menu()
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if let Some(command) = location_command_for_key_event(
                    &event,
                    shortcut,
                    self.scene.is_location_editing(),
                ) {
                    if command == LocationCommand::Commit {
                        self.commit_location_draft(event_loop);
                    } else if self.scene.apply_location_command(command, renderer.size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if let Some(command) =
                    filter_command_for_key_event(&event, shortcut, self.scene.filter_active)
                {
                    if self.scene.apply_filter_command(command, renderer.size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if let Some(view_mode) = view_mode_for_key_event(&event, shortcut) {
                    if self.scene.set_view_mode(view_mode, renderer.size) {
                        self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
                        if let Some(window) = self.window.as_ref() {
                            window.set_title(&window_title(&self.scene));
                            window.request_redraw();
                        }
                        self.render_now(event_loop, "switch-immediate", true);
                    }
                    return;
                }
                if shortcut && let Some(zoom_action) = zoom_action_for_key_event(&event) {
                    if self.scene.zoom(zoom_action, renderer.size) {
                        self.present_scene_change(event_loop, "zoom");
                    }
                    return;
                }
                if let Some(command) = selection_command_for_key_event(&event, shortcut) {
                    if self.scene.apply_selection_command(command)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if reload_requested_for_key_event(&event, shortcut) {
                    self.reload_scene_path(event_loop);
                    return;
                }
                if hidden_toggle_requested_for_key_event(&event, shortcut) {
                    if self.scene.toggle_hidden_visibility(renderer.size) {
                        self.present_scene_change(event_loop, "toggle-hidden");
                    }
                    return;
                }
                if is_activation_key(&event.logical_key) {
                    if let Some(path) = self.scene.selected_directory_path() {
                        self.load_scene_path(event_loop, path, "activate-directory");
                    }
                    return;
                }
                if let Some(action) = path_navigation_action_for_key(
                    &event.logical_key,
                    self.modifiers.state().alt_key(),
                ) {
                    self.perform_path_navigation(event_loop, action);
                    return;
                }
                let Some(action) = navigation_action_for_key(&event.logical_key) else {
                    return;
                };
                let extend = self.modifiers.state().shift_key();
                if self.scene.navigate(action, extend, renderer.size)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                let size = renderer.size;
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                if self.scene.is_open_with_chooser_open() {
                    self.set_window_cursor(CursorIcon::Default);
                    return;
                }
                if self.scene.is_rename_dialog_open() {
                    self.set_window_cursor(CursorIcon::Default);
                    return;
                }
                if self.scene.is_create_dialog_open() {
                    self.set_window_cursor(CursorIcon::Default);
                    return;
                }
                let changed = self.scene.set_pointer(point, size);
                self.update_window_cursor_for_scene(size);
                if changed && let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerLeft { .. } => {
                self.set_window_cursor(CursorIcon::Default);
                if self.scene.clear_pointer()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                let size = renderer.size;
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                let Some(mouse_button) = button.mouse_button() else {
                    return;
                };
                if self.scene.is_open_with_chooser_open() {
                    if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                        match self
                            .scene
                            .open_with_chooser_click_at_screen_point(point, size)
                        {
                            OpenWithChooserClick::Outside | OpenWithChooserClick::Cancel => {
                                if self.scene.close_open_with_chooser()
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            OpenWithChooserClick::Open => self.commit_open_with_chooser(),
                            OpenWithChooserClick::Row(row) => {
                                if self.scene.select_open_with_filtered_row(row)
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            OpenWithChooserClick::Inside => {}
                        }
                    }
                    return;
                }
                if self.scene.is_trash_conflict_dialog_open() {
                    if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                        match self
                            .scene
                            .trash_conflict_dialog_click_at_screen_point(point, size)
                        {
                            TrashConflictDialogClick::Outside
                            | TrashConflictDialogClick::Cancel => {
                                if self.scene.close_trash_conflict_dialog()
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            TrashConflictDialogClick::Replace => {
                                self.replace_trash_restore_conflicts(event_loop)
                            }
                            TrashConflictDialogClick::Inside => {}
                        }
                    }
                    return;
                }
                if self.scene.is_rename_dialog_open() {
                    if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                        match self.scene.rename_dialog_click_at_screen_point(point, size) {
                            RenameDialogClick::Outside | RenameDialogClick::Cancel => {
                                if self.scene.close_rename_dialog()
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            RenameDialogClick::Commit => self.commit_rename_dialog(event_loop),
                            RenameDialogClick::Inside => {}
                        }
                    }
                    return;
                }
                if self.scene.is_create_dialog_open() {
                    if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                        match self.scene.create_dialog_click_at_screen_point(point, size) {
                            CreateDialogClick::Outside | CreateDialogClick::Cancel => {
                                if self.scene.close_create_dialog()
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            CreateDialogClick::Commit => self.commit_create_dialog(event_loop),
                            CreateDialogClick::Kind(kind) => {
                                if self
                                    .scene
                                    .apply_create_command(CreateCommand::SetKind(kind), size)
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            CreateDialogClick::Inside => {}
                        }
                    }
                    return;
                }
                if self.scene.is_properties_overlay_open() {
                    if state == ElementState::Pressed
                        && mouse_button == MouseButton::Left
                        && self.scene.close_properties_overlay_if_outside(point, size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if mouse_button == MouseButton::Right {
                    if state == ElementState::Pressed
                        && self.scene.open_context_menu_with_cache(
                            point,
                            size,
                            &self.mime_applications,
                        )
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if mouse_button != MouseButton::Left {
                    return;
                }
                let path_bar_hit = state == ElementState::Pressed
                    && self.scene.path_bar_contains_screen_point(point, size);
                let location_blur_changed = state == ElementState::Pressed
                    && !path_bar_hit
                    && self.scene.close_location_draft_if_outside(point, size);
                if state == ElementState::Released && self.scene.is_scrollbar_dragging() {
                    let changed = self.scene.end_scrollbar_drag(point, size);
                    self.update_window_cursor_for_scene(size);
                    if changed && let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed && self.scene.is_context_menu_open() {
                    let action = self
                        .scene
                        .activate_or_close_context_menu_command(point, size);
                    if let Some(action) = action {
                        self.perform_context_menu_action(event_loop, action);
                    } else if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(changed) = self.scene.toggle_places_at_screen_point(point, size)
                {
                    self.update_window_cursor_for_scene(size);
                    if (changed || location_blur_changed)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(changed) = self.scene.begin_scrollbar_drag(point, size)
                {
                    self.update_window_cursor_for_scene(size);
                    if (changed || location_blur_changed)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if path_bar_hit {
                    if self
                        .scene
                        .apply_location_command(LocationCommand::Activate, size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(action) = self
                        .scene
                        .path_navigation_action_at_screen_point(point, size)
                {
                    self.perform_path_navigation(event_loop, action);
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(view_mode) = self.scene.view_mode_at_screen_point(point, size)
                {
                    if self.scene.set_view_mode(view_mode, size) {
                        self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
                        if let Some(window) = self.window.as_ref() {
                            window.set_title(&window_title(&self.scene));
                            window.request_redraw();
                        }
                        self.render_now(event_loop, "mode-click", true);
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(path) = self.scene.place_activation_for_primary_press(point, size)
                {
                    self.load_scene_path(event_loop, path, "place-open");
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(path) = self.scene.directory_activation_for_primary_press(
                        point,
                        size,
                        Instant::now(),
                    )
                {
                    self.load_scene_path(event_loop, path, "double-click-directory");
                    return;
                }
                let changed = match state {
                    ElementState::Pressed => {
                        let selection = SelectionClick {
                            point,
                            extend: self.modifiers.state().shift_key(),
                            toggle: self.modifiers.state().control_key()
                                || self.modifiers.state().meta_key(),
                        };
                        self.scene.begin_primary_pointer(selection, size)
                    }
                    ElementState::Released => self.scene.end_primary_pointer(point, size),
                };
                self.update_window_cursor_for_scene(size);
                if (changed || location_blur_changed)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                let shortcut =
                    self.modifiers.state().control_key() || self.modifiers.state().meta_key();
                let delta_y = scroll_delta_y(delta, self.scene.ui_scale());
                if shortcut {
                    if let Some(zoom_action) = zoom_action_for_scroll_delta(delta_y)
                        && self.scene.zoom(zoom_action, renderer.size)
                    {
                        self.present_scene_change(event_loop, "wheel-zoom");
                    }
                    return;
                }
                if self.scene.scroll_by(delta_y, renderer.size)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let force_log = self.pending_redraw_frames > 0;
                let reason = if force_log { "switch-redraw" } else { "redraw" };
                self.render_now(event_loop, reason, force_log);
            }
            _ => {}
        }
    }
}

impl FikaWgpuApp {
    fn perform_context_menu_action(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        command: ShellContextMenuCommand,
    ) {
        let action = match command {
            ShellContextMenuCommand::Builtin(action) => action,
            ShellContextMenuCommand::CreateEntry(kind) => {
                if self.scene.open_create_dialog_from_context_with_kind(kind)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
                return;
            }
            ShellContextMenuCommand::RunServiceMenuAction { action_id } => {
                self.run_context_service_menu_action(action_id);
                return;
            }
            ShellContextMenuCommand::OpenWithApplication { desktop_id } => {
                self.open_context_target_with_application(desktop_id);
                return;
            }
            ShellContextMenuCommand::OpenSubmenu(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        match action {
            ShellContextMenuAction::Open => {
                if let Some(path) = self.scene.context_target_directory_path() {
                    self.load_scene_path(event_loop, path, "context-open");
                } else {
                    match self.scene.open_context_target_file_with_default_app() {
                        Ok(true) => {
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                        }
                        Ok(false) => {
                            eprintln!("[fika-wgpu] context-action-pending action=open target=none");
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                        }
                        Err(error) => {
                            eprintln!("[fika-wgpu] open-error {error}");
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                        }
                    }
                }
            }
            ShellContextMenuAction::OpenWith => {
                if self
                    .scene
                    .open_open_with_chooser_from_context(&self.mime_applications)
                {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            ShellContextMenuAction::Refresh => self.reload_scene_path(event_loop),
            ShellContextMenuAction::ToggleHiddenFiles => {
                let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                    return;
                };
                if self.scene.toggle_hidden_visibility(size) {
                    self.present_scene_change(event_loop, "context-toggle-hidden");
                }
            }
            ShellContextMenuAction::SplitPane | ShellContextMenuAction::OpenInNewPane => {
                self.open_context_target_in_split_pane(event_loop, action.as_str());
            }
            ShellContextMenuAction::SelectAll => {
                let _ = self
                    .scene
                    .apply_selection_command(SelectionCommand::SelectAll);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::Properties => {
                if self.scene.open_properties_overlay_from_context()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::CreateNew => {
                if self.scene.open_create_dialog_from_context()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::Rename => {
                if self.scene.open_rename_dialog_from_context()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::AddToPlaces => self.add_context_target_to_places(event_loop),
            ShellContextMenuAction::RemovePlace => self.remove_context_place(event_loop),
            ShellContextMenuAction::RestoreFromTrash
            | ShellContextMenuAction::DeletePermanently
            | ShellContextMenuAction::EmptyTrash => {
                self.perform_trash_view_context_action(event_loop, action)
            }
            ShellContextMenuAction::MoveToTrash => self.move_context_target_to_trash(event_loop),
            ShellContextMenuAction::Copy | ShellContextMenuAction::Cut => {
                match self.scene.context_target_file_clipboard_request(action) {
                    Ok(Some(request)) => {
                        if let Some(clipboard) = self.clipboard.as_ref() {
                            clipboard.store_text(&request.text);
                            self.scene.record_file_clipboard_export(&request);
                        } else {
                            eprintln!(
                                "[fika-wgpu] clipboard-export-error role={} paths={} error=clipboard-unavailable",
                                file_clipboard_role_as_str(request.role),
                                request.paths.len()
                            );
                        }
                    }
                    Ok(None) => eprintln!(
                        "[fika-wgpu] clipboard-export-error role={} target=none",
                        action.as_str()
                    ),
                    Err(error) => eprintln!(
                        "[fika-wgpu] clipboard-export-error role={} {error}",
                        action.as_str()
                    ),
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::CopyLocation => {
                match self.scene.context_target_copy_location_request() {
                    Some(request) => {
                        if let Some(clipboard) = self.clipboard.as_ref() {
                            clipboard.store_text(&request.text);
                            self.scene.record_copy_location(&request);
                        } else {
                            eprintln!(
                                "[fika-wgpu] copy-location-error path={} error=clipboard-unavailable",
                                request.path.display()
                            );
                        }
                    }
                    None => eprintln!("[fika-wgpu] copy-location-error target=none"),
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::MountDevice
            | ShellContextMenuAction::UnmountDevice
            | ShellContextMenuAction::EjectDevice
            | ShellContextMenuAction::SafelyRemoveDevice => {
                match self.scene.context_target_device_action(action) {
                    Some(request) => eprintln!(
                        "[fika-wgpu] device-action-pending action={} id={:?} label={:?}",
                        action.as_str(),
                        request.id,
                        request.label
                    ),
                    None => eprintln!(
                        "[fika-wgpu] device-action-error action={} target=none",
                        action.as_str()
                    ),
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            ShellContextMenuAction::Paste => self.paste_from_clipboard(event_loop),
        }
    }

    fn run_context_service_menu_action(&mut self, action_id: String) {
        let request = match self
            .scene
            .service_menu_launch_request(&self.mime_applications, &action_id)
        {
            Ok(request) => request,
            Err(error) => {
                eprintln!("[fika-wgpu] service-menu-error action={action_id:?} {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        let paths = request.paths.clone();
        let app_name = request.app_name.clone();
        std::thread::spawn(move || {
            let result = pollster::block_on(launch_with_systemd_user(request.plan));
            let status = ServiceMenuLaunchResult {
                pane_id: WGPU_SHELL_PANE_ID,
                target_label: service_menu_target_label(&paths),
                app_name,
                result,
            }
            .status_message();
            eprintln!("[fika-wgpu] service-menu-finished {status}");
        });
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn open_context_target_with_application(&mut self, desktop_id: String) {
        let request = match self
            .scene
            .open_with_launch_request_for_context_application(&self.mime_applications, &desktop_id)
        {
            Ok(request) => request,
            Err(error) => {
                eprintln!("[fika-wgpu] open-with-error app={desktop_id:?} {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        let path = request.path.clone();
        let app_name = request.app_name.clone();
        std::thread::spawn(move || {
            let result = pollster::block_on(launch_with_systemd_user(request.plan));
            let status = OpenWithLaunchResult {
                pane_id: WGPU_SHELL_PANE_ID,
                path,
                app_name,
                result,
            }
            .status_message();
            eprintln!("[fika-wgpu] open-with-finished {status}");
        });
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn open_context_target_in_split_pane(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        reason: &'static str,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.open_split_pane_from_context(size) {
            Ok(true) => self.present_scene_change(event_loop, reason),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] split-pane-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn perform_trash_view_context_action(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.perform_trash_view_context_action(action, size) {
            Ok(result) if result.success_count > 0 => {
                self.present_scene_change(event_loop, action.as_str())
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!(
                    "[fika-wgpu] trash-view-error action={} {error}",
                    action.as_str()
                );
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn replace_trash_restore_conflicts(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.replace_trash_restore_conflicts(size) {
            Ok(result) if result.success_count > 0 => {
                self.present_scene_change(event_loop, "replace-trash-conflicts")
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] trash-conflict-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn add_context_target_to_places(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .add_context_target_to_places(&default_user_places_path(), size)
        {
            Ok(true) => self.present_scene_change(event_loop, "add-place"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] add-place-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn remove_context_place(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .remove_context_place(&default_user_places_path(), size)
        {
            Ok(true) => self.present_scene_change(event_loop, "remove-place"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] remove-place-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn move_context_target_to_trash(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.move_context_target_to_trash(size) {
            Ok(result) if result.changed() => {
                self.present_scene_change(event_loop, "move-to-trash")
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] trash-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn paste_from_clipboard(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let Some(clipboard) = self.clipboard.as_ref() else {
            eprintln!("[fika-wgpu] paste-error error=clipboard-unavailable");
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        };
        let text = match clipboard.load_text() {
            Ok(text) => text,
            Err(error) => {
                eprintln!("[fika-wgpu] paste-error load={error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        match self.scene.paste_clipboard_text_from_context(&text, size) {
            Ok(result) if result.changed() => {
                if result.clear_clipboard && result.failure_count == 0 {
                    clipboard.store_text("");
                }
                self.present_scene_change(event_loop, "paste");
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] paste-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn commit_rename_dialog(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let request = match self.scene.rename_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_rename_dialog_error(error)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
                return;
            }
        };

        if let Err(error) = rename_entry_on_disk(&request) {
            if self.scene.set_rename_dialog_error(error)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }

        self.scene.close_rename_dialog_after_success(&request);
        match self.scene.reload_current_path(size) {
            Ok(_) => {
                self.scene.select_entry_by_name(&request.name, size);
                self.present_scene_change(event_loop, "rename");
            }
            Err(error) => {
                eprintln!("[fika-wgpu] rename-reload-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn commit_create_dialog(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let request = match self.scene.create_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_create_dialog_error(error)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
                return;
            }
        };

        if let Err(error) = create_entry_on_disk(&request) {
            if self.scene.set_create_dialog_error(error)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }

        self.scene.close_create_dialog_after_success(&request);
        match self.scene.reload_current_path(size) {
            Ok(_) => {
                self.scene.select_entry_by_name(&request.name, size);
                self.present_scene_change(event_loop, "create-new");
            }
            Err(error) => {
                eprintln!("[fika-wgpu] create-new-reload-error {error}");
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn commit_open_with_chooser(&mut self) {
        let request = match self.scene.open_with_launch_request(&self.mime_applications) {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_open_with_chooser_error(error)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
                return;
            }
        };

        self.scene.close_open_with_chooser_after_success(&request);
        let path = request.path.clone();
        let app_name = request.app_name.clone();
        std::thread::spawn(move || {
            let result = pollster::block_on(launch_with_systemd_user(request.plan));
            let status = OpenWithLaunchResult {
                pane_id: WGPU_SHELL_PANE_ID,
                path,
                app_name,
                result,
            }
            .status_message();
            eprintln!("[fika-wgpu] open-with-finished {status}");
        });

        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn perform_path_navigation(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: PathNavigationAction,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let result = match action {
            PathNavigationAction::Back => self.scene.go_history_back(size),
            PathNavigationAction::Forward => self.scene.go_history_forward(size),
            PathNavigationAction::Parent => self.scene.go_parent_directory(size),
        };
        match result {
            Ok(true) => self.present_scene_change(event_loop, action.reason()),
            Ok(false) => {}
            Err(error) => eprintln!("[fika-wgpu] navigation-error {error}"),
        }
    }

    fn reload_scene_path(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.reload_current_path(size) {
            Ok(true) => self.present_scene_change(event_loop, "reload-directory"),
            Ok(false) => {}
            Err(error) => eprintln!("[fika-wgpu] reload-error {error}"),
        }
    }

    fn commit_location_draft(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let input = self.scene.location_draft_value().unwrap_or("").to_string();
        let Some(path) = self.scene.resolved_location_draft() else {
            eprintln!("[fika-wgpu] location-error input={input:?} error=empty");
            return;
        };
        let closed = self.scene.close_location_draft(size);
        match self.scene.load_path(path, size) {
            Ok(true) => self.present_scene_change(event_loop, "location-commit"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                eprintln!("[fika-wgpu] location-error input={input:?} error={error}");
                if closed && let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn load_scene_path(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        path: PathBuf,
        reason: &'static str,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.load_path(path, size) {
            Ok(true) => self.present_scene_change(event_loop, reason),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => eprintln!("[fika-wgpu] navigation-error {error}"),
        }
    }

    fn present_scene_change(&mut self, event_loop: &dyn ActiveEventLoop, reason: &'static str) {
        self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
            window.request_redraw();
        }
        self.render_now(event_loop, reason, true);
    }

    fn render_now(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        reason: &'static str,
        force_log: bool,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        window.pre_present_notify();
        if renderer.render(
            window.as_ref(),
            event_loop,
            &mut self.scene,
            reason,
            force_log,
        ) && self.pending_redraw_frames > 0
        {
            self.pending_redraw_frames -= 1;
        }
    }
}

fn scroll_delta_y(delta: MouseScrollDelta, scale_factor: f32) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => -y * SCROLL_LINE_PX * scale_factor,
        MouseScrollDelta::PixelDelta(position) => -position.y as f32,
    }
}

fn zoom_action_for_scroll_delta(delta_y: f32) -> Option<ZoomAction> {
    if delta_y < -f32::EPSILON {
        Some(ZoomAction::In)
    } else if delta_y > f32::EPSILON {
        Some(ZoomAction::Out)
    } else {
        None
    }
}

fn is_activation_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Enter))
}

fn escape_requested_for_key_event(event: &KeyEvent) -> bool {
    matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(event.logical_key, Key::Named(NamedKey::Escape))
        || matches!(event.key_without_modifiers, Key::Named(NamedKey::Escape))
}

fn path_navigation_action_for_key(key: &Key, alt: bool) -> Option<PathNavigationAction> {
    if matches!(key, Key::Named(NamedKey::Backspace)) {
        return Some(PathNavigationAction::Parent);
    }
    if !alt {
        return None;
    }
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(PathNavigationAction::Back),
        Key::Named(NamedKey::ArrowRight) => Some(PathNavigationAction::Forward),
        Key::Named(NamedKey::ArrowUp) => Some(PathNavigationAction::Parent),
        _ => None,
    }
}

fn zoom_action_for_key_event(event: &KeyEvent) -> Option<ZoomAction> {
    zoom_action_for_key(&event.key_without_modifiers)
        .or_else(|| zoom_action_for_key(&event.logical_key))
}

fn zoom_action_for_key(key: &Key) -> Option<ZoomAction> {
    match key {
        Key::Character(value) if value.as_str() == "+" || value.as_str() == "=" => {
            Some(ZoomAction::In)
        }
        Key::Character(value) if value.as_str() == "-" || value.as_str() == "_" => {
            Some(ZoomAction::Out)
        }
        Key::Character(value) if value.as_str() == "0" => Some(ZoomAction::Reset),
        _ => None,
    }
}

fn selection_command_for_key_event(event: &KeyEvent, shortcut: bool) -> Option<SelectionCommand> {
    selection_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn selection_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<SelectionCommand> {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(SelectionCommand::Clear);
    }
    if !shortcut {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyA))
        || key_character_eq(logical_key, "a")
        || key_character_eq(key_without_modifiers, "a")
    {
        return Some(SelectionCommand::SelectAll);
    }
    None
}

fn reload_requested_for_key_event(event: &KeyEvent, shortcut: bool) -> bool {
    reload_requested_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn reload_requested_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> bool {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::F5))
        || matches!(logical_key, Key::Named(NamedKey::F5))
        || matches!(key_without_modifiers, Key::Named(NamedKey::F5))
    {
        return true;
    }
    shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyR))
            || key_character_eq(logical_key, "r")
            || key_character_eq(key_without_modifiers, "r"))
}

fn hidden_toggle_requested_for_key_event(event: &KeyEvent, shortcut: bool) -> bool {
    hidden_toggle_requested_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn hidden_toggle_requested_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> bool {
    shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyH))
            || key_character_eq(logical_key, "h")
            || key_character_eq(key_without_modifiers, "h"))
}

fn location_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
    location_active: bool,
) -> Option<LocationCommand> {
    location_command_for_key_parts(
        shortcut,
        location_active,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn location_command_for_key_parts(
    shortcut: bool,
    location_active: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<LocationCommand> {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::F6))
        || matches!(logical_key, Key::Named(NamedKey::F6))
        || matches!(key_without_modifiers, Key::Named(NamedKey::F6))
        || (shortcut
            && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyL))
                || matches!(physical_key, PhysicalKey::Code(KeyCode::KeyD))
                || key_character_eq(logical_key, "l")
                || key_character_eq(key_without_modifiers, "l")
                || key_character_eq(logical_key, "d")
                || key_character_eq(key_without_modifiers, "d")))
    {
        return Some(LocationCommand::Activate);
    }
    if !location_active {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(LocationCommand::Cancel);
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return Some(LocationCommand::Commit);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Tab))
        || matches!(logical_key, Key::Named(NamedKey::Tab))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Tab))
    {
        return Some(LocationCommand::Complete);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return Some(LocationCommand::Backspace);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Delete))
        || matches!(logical_key, Key::Named(NamedKey::Delete))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Delete))
    {
        return Some(LocationCommand::Delete);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowLeft))
        || matches!(logical_key, Key::Named(NamedKey::ArrowLeft))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowLeft))
    {
        return Some(LocationCommand::MoveLeft);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowRight))
        || matches!(logical_key, Key::Named(NamedKey::ArrowRight))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowRight))
    {
        return Some(LocationCommand::MoveRight);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Home))
        || matches!(logical_key, Key::Named(NamedKey::Home))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Home))
    {
        return Some(LocationCommand::MoveHome);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::End))
        || matches!(logical_key, Key::Named(NamedKey::End))
        || matches!(key_without_modifiers, Key::Named(NamedKey::End))
    {
        return Some(LocationCommand::MoveEnd);
    }
    if shortcut {
        return Some(LocationCommand::Ignore);
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            Some(LocationCommand::Insert(value.to_string()))
        }
        _ => Some(LocationCommand::Ignore),
    }
}

fn filter_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
    filter_active: bool,
) -> Option<FilterCommand> {
    filter_command_for_key_parts(
        shortcut,
        filter_active,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn filter_command_for_key_parts(
    shortcut: bool,
    filter_active: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<FilterCommand> {
    if shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyF))
            || key_character_eq(logical_key, "f")
            || key_character_eq(key_without_modifiers, "f"))
    {
        return Some(FilterCommand::Activate);
    }
    if !filter_active {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(FilterCommand::ClearAndDeactivate);
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return Some(FilterCommand::Deactivate);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return Some(FilterCommand::Backspace);
    }
    if shortcut {
        return None;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            Some(FilterCommand::Insert(value.to_string()))
        }
        _ => None,
    }
}

fn create_command_for_key_event(event: &KeyEvent, shortcut: bool) -> CreateCommand {
    create_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn create_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> CreateCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return CreateCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return CreateCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return CreateCommand::Backspace;
    }
    if shortcut {
        return CreateCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            CreateCommand::Insert(value.to_string())
        }
        _ => CreateCommand::Ignore,
    }
}

fn rename_command_for_key_event(event: &KeyEvent, shortcut: bool) -> RenameCommand {
    rename_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn rename_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> RenameCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return RenameCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return RenameCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return RenameCommand::Backspace;
    }
    if shortcut {
        return RenameCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            RenameCommand::Insert(value.to_string())
        }
        _ => RenameCommand::Ignore,
    }
}

fn open_with_command_for_key_event(event: &KeyEvent, shortcut: bool) -> OpenWithCommand {
    open_with_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn open_with_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> OpenWithCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return OpenWithCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return OpenWithCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowUp))
        || matches!(logical_key, Key::Named(NamedKey::ArrowUp))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowUp))
    {
        return OpenWithCommand::MoveUp;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowDown))
        || matches!(logical_key, Key::Named(NamedKey::ArrowDown))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowDown))
    {
        return OpenWithCommand::MoveDown;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return OpenWithCommand::Backspace;
    }
    if shortcut {
        return OpenWithCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            OpenWithCommand::Insert(value.to_string())
        }
        _ => OpenWithCommand::Ignore,
    }
}

fn key_character_eq(key: &Key, expected: &str) -> bool {
    matches!(key, Key::Character(value) if value.as_str().eq_ignore_ascii_case(expected))
}

fn navigation_action_for_key(key: &Key) -> Option<NavigationAction> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(NavigationAction::Left),
        Key::Named(NamedKey::ArrowRight) => Some(NavigationAction::Right),
        Key::Named(NamedKey::ArrowUp) => Some(NavigationAction::Up),
        Key::Named(NamedKey::ArrowDown) => Some(NavigationAction::Down),
        Key::Named(NamedKey::Home) => Some(NavigationAction::Home),
        Key::Named(NamedKey::End) => Some(NavigationAction::End),
        Key::Named(NamedKey::PageUp) => Some(NavigationAction::PageUp),
        Key::Named(NamedKey::PageDown) => Some(NavigationAction::PageDown),
        _ => None,
    }
}

fn view_mode_for_key_event(event: &KeyEvent, shortcut: bool) -> Option<ShellViewMode> {
    view_mode_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

fn view_mode_for_key_parts(
    _shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<ShellViewMode> {
    let function_key_mode = match physical_key {
        PhysicalKey::Code(KeyCode::F1) => Some(ShellViewMode::Icons),
        PhysicalKey::Code(KeyCode::F2) => Some(ShellViewMode::Compact),
        PhysicalKey::Code(KeyCode::F3) => Some(ShellViewMode::Details),
        _ => function_key_view_mode(logical_key),
    };
    if function_key_mode.is_some() {
        return function_key_mode;
    }

    physical_digit_view_mode(physical_key)
        .or_else(|| character_digit_view_mode(key_without_modifiers))
        .or_else(|| character_digit_view_mode(logical_key))
}

fn physical_digit_view_mode(physical_key: &PhysicalKey) -> Option<ShellViewMode> {
    match physical_key {
        PhysicalKey::Code(KeyCode::Digit1 | KeyCode::Numpad1) => Some(ShellViewMode::Icons),
        PhysicalKey::Code(KeyCode::Digit2 | KeyCode::Numpad2) => Some(ShellViewMode::Compact),
        PhysicalKey::Code(KeyCode::Digit3 | KeyCode::Numpad3) => Some(ShellViewMode::Details),
        _ => None,
    }
}

fn character_digit_view_mode(key: &Key) -> Option<ShellViewMode> {
    match key {
        Key::Character(value) if value.as_str() == "1" => Some(ShellViewMode::Icons),
        Key::Character(value) if value.as_str() == "2" => Some(ShellViewMode::Compact),
        Key::Character(value) if value.as_str() == "3" => Some(ShellViewMode::Details),
        _ => None,
    }
}

fn function_key_view_mode(key: &Key) -> Option<ShellViewMode> {
    match key {
        Key::Named(NamedKey::F1) => Some(ShellViewMode::Icons),
        Key::Named(NamedKey::F2) => Some(ShellViewMode::Compact),
        Key::Named(NamedKey::F3) => Some(ShellViewMode::Details),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct PrimaryClick {
    index: usize,
    point: ViewPoint,
    time: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellPlace {
    group: &'static str,
    marker: &'static str,
    label: String,
    path: PathBuf,
    device: Option<ShellDevicePlace>,
    network: bool,
    trash: bool,
    root: bool,
    editable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellDevicePlace {
    id: String,
    mounted: bool,
    ejectable: bool,
    can_power_off: bool,
}

impl ShellPlace {
    fn new(
        group: &'static str,
        marker: &'static str,
        label: impl Into<String>,
        path: PathBuf,
        editable: bool,
    ) -> Self {
        let trash = file_ops::is_trash_files_dir(&path);
        let network = is_network_path(&path);
        let root = path == PathBuf::from("/");
        Self {
            group,
            marker,
            label: label.into(),
            path,
            device: None,
            network,
            trash,
            root,
            editable,
        }
    }

    fn with_device(mut self, device: ShellDevicePlace) -> Self {
        self.device = Some(device);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellContextTarget {
    Item {
        index: usize,
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
    },
    Blank {
        path: PathBuf,
    },
    Place {
        index: usize,
        label: String,
        path: PathBuf,
        group: &'static str,
        device: Option<ShellDevicePlace>,
        network: bool,
        trash: bool,
        root: bool,
        editable: bool,
    },
}

impl ShellContextTarget {
    fn kind(&self) -> &'static str {
        match self {
            Self::Item { .. } => "item",
            Self::Blank { .. } => "blank",
            Self::Place { .. } => "place",
        }
    }

    fn log_path(&self) -> &Path {
        match self {
            Self::Item { path, .. } | Self::Blank { path, .. } | Self::Place { path, .. } => path,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellContextMenuCommand {
    Builtin(ShellContextMenuAction),
    CreateEntry(CreateEntryKind),
    RunServiceMenuAction { action_id: String },
    OpenWithApplication { desktop_id: String },
    OpenSubmenu(ShellContextSubmenu),
}

impl ShellContextMenuCommand {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin(action) => action.as_str(),
            Self::CreateEntry(kind) => kind.as_str(),
            Self::RunServiceMenuAction { .. } => "run-service-menu-action",
            Self::OpenWithApplication { .. } => "open-with-application",
            Self::OpenSubmenu(submenu) => submenu.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellContextSubmenu {
    CreateNew,
    OpenWith,
    ServiceMenu,
    ServiceMenuGroup(usize),
}

impl ShellContextSubmenu {
    fn as_str(self) -> &'static str {
        match self {
            Self::CreateNew => "submenu-create-new",
            Self::OpenWith => "submenu-open-with",
            Self::ServiceMenu => "submenu-service-menu",
            Self::ServiceMenuGroup(_) => "submenu-service-menu-group",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellContextMenuItem {
    command: ShellContextMenuCommand,
    label: String,
    separator_before: bool,
    submenu: Option<ShellContextSubmenu>,
    icon: ShellContextMenuIcon,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellContextMenuIcon {
    Builtin(ShellContextMenuAction),
    Service(Option<String>),
    Application(Option<String>),
}

impl ShellContextMenuItem {
    fn builtin(action: ShellContextMenuAction) -> Self {
        Self {
            command: ShellContextMenuCommand::Builtin(action),
            label: action.label().to_string(),
            separator_before: false,
            submenu: None,
            icon: ShellContextMenuIcon::Builtin(action),
        }
    }

    fn builtin_submenu(
        action: ShellContextMenuAction,
        label: impl Into<String>,
        submenu: ShellContextSubmenu,
    ) -> Self {
        Self {
            command: ShellContextMenuCommand::OpenSubmenu(submenu),
            label: label.into(),
            separator_before: false,
            submenu: Some(submenu),
            icon: ShellContextMenuIcon::Builtin(action),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellContextMenuAction {
    Open,
    OpenWith,
    OpenInNewPane,
    SplitPane,
    Copy,
    Cut,
    CopyLocation,
    Rename,
    MoveToTrash,
    RestoreFromTrash,
    DeletePermanently,
    EmptyTrash,
    AddToPlaces,
    CreateNew,
    Paste,
    SelectAll,
    ToggleHiddenFiles,
    Refresh,
    Properties,
    RemovePlace,
    MountDevice,
    UnmountDevice,
    EjectDevice,
    SafelyRemoveDevice,
}

impl ShellContextMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::OpenWith => "Open With",
            Self::OpenInNewPane => "Open in New Pane",
            Self::SplitPane => "Split View",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::CopyLocation => "Copy Location",
            Self::Rename => "Rename",
            Self::MoveToTrash => "Move to Trash",
            Self::RestoreFromTrash => "Restore to Former Location",
            Self::DeletePermanently => "Delete Permanently",
            Self::EmptyTrash => "Empty Trash",
            Self::AddToPlaces => "Add to Places",
            Self::CreateNew => "Create New",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
            Self::ToggleHiddenFiles => "Show Hidden Files",
            Self::Refresh => "Refresh",
            Self::Properties => "Properties",
            Self::RemovePlace => "Remove",
            Self::MountDevice => "Mount",
            Self::UnmountDevice => "Unmount",
            Self::EjectDevice => "Eject",
            Self::SafelyRemoveDevice => "Safely Remove",
        }
    }

    fn label_for_hidden_state(self, show_hidden: bool) -> &'static str {
        match (self, show_hidden) {
            (Self::ToggleHiddenFiles, true) => "Hide Hidden Files",
            (Self::ToggleHiddenFiles, false) => "Show Hidden Files",
            _ => self.label(),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenWith => "open-with",
            Self::OpenInNewPane => "open-in-new-pane",
            Self::SplitPane => "split-pane",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::CopyLocation => "copy-location",
            Self::Rename => "rename",
            Self::MoveToTrash => "move-to-trash",
            Self::RestoreFromTrash => "restore-from-trash",
            Self::DeletePermanently => "delete-permanently",
            Self::EmptyTrash => "empty-trash",
            Self::AddToPlaces => "add-to-places",
            Self::CreateNew => "create-new",
            Self::Paste => "paste",
            Self::SelectAll => "select-all",
            Self::ToggleHiddenFiles => "toggle-hidden-files",
            Self::Refresh => "refresh",
            Self::Properties => "properties",
            Self::RemovePlace => "remove-place",
            Self::MountDevice => "mount-device",
            Self::UnmountDevice => "unmount-device",
            Self::EjectDevice => "eject-device",
            Self::SafelyRemoveDevice => "safely-remove-device",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuGlyph {
    Open,
    OpenWith,
    Pane,
    Hidden,
    Copy,
    Cut,
    Location,
    Rename,
    Trash,
    Restore,
    Delete,
    Place,
    Create,
    Paste,
    Select,
    Refresh,
    Properties,
    Remove,
}

fn context_menu_icon_style(
    action: ShellContextMenuAction,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match action {
        ShellContextMenuAction::Open => (
            ContextMenuGlyph::Open,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::OpenWith => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::OpenInNewPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::SplitPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Copy => (
            ContextMenuGlyph::Copy,
            [0.145, 0.388, 0.922, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Cut => (
            ContextMenuGlyph::Cut,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::CopyLocation => (
            ContextMenuGlyph::Location,
            [0.200, 0.255, 0.333, 1.0],
            [0.910, 0.933, 0.969, 1.0],
        ),
        ShellContextMenuAction::Rename => (
            ContextMenuGlyph::Rename,
            [0.427, 0.157, 0.851, 1.0],
            [0.949, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::MoveToTrash | ShellContextMenuAction::EmptyTrash => (
            ContextMenuGlyph::Trash,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::RestoreFromTrash => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::DeletePermanently => (
            ContextMenuGlyph::Delete,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::AddToPlaces => (
            ContextMenuGlyph::Place,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::CreateNew => (
            ContextMenuGlyph::Create,
            [0.059, 0.298, 0.506, 1.0],
            [0.906, 0.945, 0.984, 1.0],
        ),
        ShellContextMenuAction::Paste => (
            ContextMenuGlyph::Paste,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::SelectAll => (
            ContextMenuGlyph::Select,
            [0.122, 0.310, 0.749, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::ToggleHiddenFiles => (
            ContextMenuGlyph::Hidden,
            [0.294, 0.318, 0.357, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::Refresh => (
            ContextMenuGlyph::Refresh,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::Properties => (
            ContextMenuGlyph::Properties,
            [0.216, 0.255, 0.318, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::RemovePlace => (
            ContextMenuGlyph::Remove,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::MountDevice => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::UnmountDevice
        | ShellContextMenuAction::EjectDevice
        | ShellContextMenuAction::SafelyRemoveDevice => (
            ContextMenuGlyph::Open,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
    }
}

fn context_menu_item_label(item: &ShellContextMenuItem, show_hidden: bool) -> String {
    match item.command {
        ShellContextMenuCommand::Builtin(action) => {
            action.label_for_hidden_state(show_hidden).to_string()
        }
        _ => item.label.clone(),
    }
}

fn context_menu_item_icon_style(
    item: &ShellContextMenuItem,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match &item.icon {
        ShellContextMenuIcon::Builtin(action) => context_menu_icon_style(*action),
        ShellContextMenuIcon::Service(_) => (
            ContextMenuGlyph::Properties,
            [0.216, 0.255, 0.318, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuIcon::Application(_) => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
    }
}

fn context_menu_named_icon_request(
    item: &ShellContextMenuItem,
) -> Option<(&str, NamedIconFallback)> {
    match &item.icon {
        ShellContextMenuIcon::Service(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Service))
        }
        ShellContextMenuIcon::Application(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Application))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ShellContextMenu {
    target: ShellContextTarget,
    position: ViewPoint,
    open_with_apps: Vec<MimeApplication>,
    service_actions: Vec<ServiceMenuAction>,
    hovered_row: Option<usize>,
    active_submenu: Option<ShellContextSubmenu>,
    hovered_submenu_row: Option<usize>,
}

impl ShellContextMenu {
    #[cfg(test)]
    fn new(target: ShellContextTarget, position: ViewPoint) -> Self {
        Self {
            target,
            position,
            open_with_apps: Vec::new(),
            service_actions: Vec::new(),
            hovered_row: None,
            active_submenu: None,
            hovered_submenu_row: None,
        }
    }

    fn with_dynamic(
        target: ShellContextTarget,
        position: ViewPoint,
        open_with_apps: Vec<MimeApplication>,
        service_actions: Vec<ServiceMenuAction>,
    ) -> Self {
        Self {
            target,
            position,
            open_with_apps,
            service_actions,
            hovered_row: None,
            active_submenu: None,
            hovered_submenu_row: None,
        }
    }
}

fn context_menu_builtin_actions(target: &ShellContextTarget) -> Vec<ShellContextMenuAction> {
    const ITEM_FILE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::OpenWith,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::Cut,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Rename,
        ShellContextMenuAction::MoveToTrash,
        ShellContextMenuAction::Properties,
    ];
    const ITEM_DIR_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::Cut,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Rename,
        ShellContextMenuAction::MoveToTrash,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_ITEM_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::RestoreFromTrash,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::DeletePermanently,
        ShellContextMenuAction::Properties,
    ];
    const BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::CreateNew,
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::Paste,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::ToggleHiddenFiles,
        ShellContextMenuAction::SplitPane,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::EmptyTrash,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::EmptyTrash,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const EDITABLE_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::RemovePlace,
        ShellContextMenuAction::Properties,
    ];
    match target {
        ShellContextTarget::Item { path, .. } if file_ops::is_in_trash_files_dir(path) => {
            TRASH_ITEM_ACTIONS.to_vec()
        }
        ShellContextTarget::Item { is_dir: true, .. } => ITEM_DIR_ACTIONS.to_vec(),
        ShellContextTarget::Item { .. } => ITEM_FILE_ACTIONS.to_vec(),
        ShellContextTarget::Blank { path, .. } if file_ops::is_trash_files_dir(path) => {
            TRASH_BLANK_ACTIONS.to_vec()
        }
        ShellContextTarget::Blank { .. } => BLANK_ACTIONS.to_vec(),
        ShellContextTarget::Place { trash: true, .. } => TRASH_PLACE_ACTIONS.to_vec(),
        ShellContextTarget::Place {
            device: Some(device),
            ..
        } => {
            let mut actions = Vec::new();
            if device.mounted {
                actions.extend([
                    ShellContextMenuAction::Open,
                    ShellContextMenuAction::OpenInNewPane,
                    ShellContextMenuAction::CopyLocation,
                    ShellContextMenuAction::UnmountDevice,
                ]);
            } else {
                actions.push(ShellContextMenuAction::MountDevice);
            }
            if device.ejectable {
                actions.push(ShellContextMenuAction::EjectDevice);
            }
            if device.can_power_off {
                actions.push(ShellContextMenuAction::SafelyRemoveDevice);
            }
            actions.push(ShellContextMenuAction::Properties);
            actions
        }
        ShellContextTarget::Place { editable: true, .. } => EDITABLE_PLACE_ACTIONS.to_vec(),
        ShellContextTarget::Place { .. } => PLACE_ACTIONS.to_vec(),
    }
}

fn context_menu_items(menu: &ShellContextMenu) -> Vec<ShellContextMenuItem> {
    context_menu_items_for_target(&menu.target, &menu.service_actions)
}

fn context_menu_items_for_target(
    target: &ShellContextTarget,
    service_actions: &[ServiceMenuAction],
) -> Vec<ShellContextMenuItem> {
    let mut items = context_menu_builtin_actions(target)
        .iter()
        .copied()
        .map(|action| {
            let mut item = match action {
                ShellContextMenuAction::OpenWith => ShellContextMenuItem::builtin_submenu(
                    action,
                    action.label(),
                    ShellContextSubmenu::OpenWith,
                ),
                ShellContextMenuAction::CreateNew => ShellContextMenuItem::builtin_submenu(
                    action,
                    action.label(),
                    ShellContextSubmenu::CreateNew,
                ),
                _ => ShellContextMenuItem::builtin(action),
            };
            item.separator_before = context_menu_separator_before_builtin(target, action);
            item
        })
        .collect::<Vec<_>>();

    if !service_actions.is_empty() {
        let insert_at = items
            .iter()
            .position(|item| {
                matches!(
                    item.command,
                    ShellContextMenuCommand::Builtin(ShellContextMenuAction::Copy)
                        | ShellContextMenuCommand::Builtin(ShellContextMenuAction::SelectAll)
                )
            })
            .unwrap_or(items.len());
        let mut service_items = service_menu_root_items(service_actions);
        if service_menu_has_more_actions(service_actions) {
            let mut more = ShellContextMenuItem::builtin_submenu(
                ShellContextMenuAction::Properties,
                "More Actions",
                ShellContextSubmenu::ServiceMenu,
            );
            more.icon = ShellContextMenuIcon::Service(None);
            more.separator_before = service_items.is_empty();
            service_items.push(more);
        }
        if !service_items.is_empty() {
            if let Some(first) = service_items.first_mut() {
                first.separator_before = true;
            }
            items.splice(insert_at..insert_at, service_items);
        }
    }

    items
}

#[cfg(test)]
fn context_menu_actions(target: &ShellContextTarget) -> Vec<ShellContextMenuAction> {
    context_menu_items_for_target(target, &[])
        .into_iter()
        .filter_map(|item| match (&item.command, &item.icon) {
            (ShellContextMenuCommand::Builtin(action), _) => Some(*action),
            (_, ShellContextMenuIcon::Builtin(action)) => Some(*action),
            _ => None,
        })
        .collect()
}

fn context_submenu_actions(
    submenu: ShellContextSubmenu,
    menu: &ShellContextMenu,
) -> Vec<ShellContextMenuItem> {
    match submenu {
        ShellContextSubmenu::CreateNew => vec![
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry(CreateEntryKind::Folder),
                label: "Folder".to_string(),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry(CreateEntryKind::File),
                label: "Text File".to_string(),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
        ],
        ShellContextSubmenu::OpenWith => {
            let apps = menu.open_with_apps.as_slice();
            if apps.is_empty() {
                return vec![ShellContextMenuItem {
                    command: ShellContextMenuCommand::Builtin(ShellContextMenuAction::OpenWith),
                    label: "Other Application...".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::OpenWith),
                }];
            }
            let mut items = apps
                .iter()
                .take(12)
                .map(|app| ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenWithApplication {
                        desktop_id: app.id.clone(),
                    },
                    label: if app.is_default {
                        format!("{} (default)", app.name)
                    } else {
                        app.name.clone()
                    },
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Application(app.icon.clone()),
                })
                .collect::<Vec<_>>();
            items.push(ShellContextMenuItem {
                command: ShellContextMenuCommand::Builtin(ShellContextMenuAction::OpenWith),
                label: "Other Application...".to_string(),
                separator_before: !items.is_empty(),
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::OpenWith),
            });
            items
        }
        ShellContextSubmenu::ServiceMenu => {
            let mut items = service_menu_more_items(&menu.service_actions);
            if items.is_empty() {
                items.push(ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ServiceMenu),
                    label: "No Actions".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Service(None),
                });
            }
            items
        }
        ShellContextSubmenu::ServiceMenuGroup(group_index) => {
            let mut items = service_menu_group_items(&menu.service_actions, group_index);
            if items.is_empty() {
                items.push(ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenSubmenu(
                        ShellContextSubmenu::ServiceMenuGroup(group_index),
                    ),
                    label: "No Actions".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Service(None),
                });
            }
            items
        }
    }
}

fn context_menu_separator_before_builtin(
    target: &ShellContextTarget,
    action: ShellContextMenuAction,
) -> bool {
    let Some(row) = context_menu_builtin_actions(target)
        .iter()
        .position(|candidate| *candidate == action)
    else {
        return false;
    };
    context_menu_separator_before(target, row)
}

fn context_menu_separator_before(target: &ShellContextTarget, row: usize) -> bool {
    let Some(action) = context_menu_builtin_actions(target).get(row).copied() else {
        return false;
    };
    match target {
        ShellContextTarget::Item { path, .. } if file_ops::is_in_trash_files_dir(path) => {
            action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Item { .. } => {
            action == ShellContextMenuAction::Copy
                || action == ShellContextMenuAction::Rename
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Blank { path, .. } if file_ops::is_trash_files_dir(path) => {
            action == ShellContextMenuAction::SelectAll
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Blank { .. } => {
            action == ShellContextMenuAction::Paste
                || action == ShellContextMenuAction::SelectAll
                || action == ShellContextMenuAction::ToggleHiddenFiles
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Place {
            device: Some(_), ..
        } => matches!(
            action,
            ShellContextMenuAction::MountDevice
                | ShellContextMenuAction::UnmountDevice
                | ShellContextMenuAction::Properties
        ),
        ShellContextTarget::Place { .. } => action == ShellContextMenuAction::Properties,
    }
}

fn service_menu_root_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
    actions
        .iter()
        .filter(|action| service_menu_action_promoted(action, actions.len()))
        .map(service_menu_action_item)
        .collect()
}

fn service_menu_has_more_actions(actions: &[ServiceMenuAction]) -> bool {
    actions
        .iter()
        .any(|action| !service_menu_action_promoted(action, actions.len()))
}

fn service_menu_more_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
    let more = actions
        .iter()
        .filter(|action| !service_menu_action_promoted(action, actions.len()))
        .collect::<Vec<_>>();
    let (ungrouped, groups) = service_menu_partition_grouped_actions(more);
    let mut items = ungrouped
        .into_iter()
        .map(service_menu_action_item)
        .collect::<Vec<_>>();
    for (group_index, (label, _)) in groups.iter().enumerate() {
        let mut item = ShellContextMenuItem::builtin_submenu(
            ShellContextMenuAction::Properties,
            label.clone(),
            ShellContextSubmenu::ServiceMenuGroup(group_index),
        );
        item.command = ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ServiceMenuGroup(
            group_index,
        ));
        item.icon = ShellContextMenuIcon::Service(None);
        item.separator_before = !items.is_empty() && group_index == 0;
        items.push(item);
    }
    items
}

fn service_menu_group_items(
    actions: &[ServiceMenuAction],
    group_index: usize,
) -> Vec<ShellContextMenuItem> {
    let more = actions
        .iter()
        .filter(|action| !service_menu_action_promoted(action, actions.len()))
        .collect::<Vec<_>>();
    let (_, groups) = service_menu_partition_grouped_actions(more);
    groups
        .into_iter()
        .nth(group_index)
        .map(|(_, group_actions)| {
            group_actions
                .into_iter()
                .map(service_menu_action_item)
                .collect()
        })
        .unwrap_or_default()
}

fn service_menu_partition_grouped_actions<'a>(
    actions: Vec<&'a ServiceMenuAction>,
) -> (
    Vec<&'a ServiceMenuAction>,
    Vec<(String, Vec<&'a ServiceMenuAction>)>,
) {
    let mut grouped: Vec<(String, Vec<&ServiceMenuAction>)> = Vec::new();
    let ungrouped = actions
        .iter()
        .copied()
        .filter(|action| action.submenu.is_none())
        .collect::<Vec<_>>();
    for action in actions
        .into_iter()
        .filter(|action| action.submenu.is_some())
    {
        let group = action.submenu.as_deref().unwrap_or_default().to_string();
        if let Some((_, group_actions)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(&group))
        {
            group_actions.push(action);
        } else {
            grouped.push((group, vec![action]));
        }
    }
    (ungrouped, grouped)
}

fn service_menu_action_promoted(action: &ServiceMenuAction, action_count: usize) -> bool {
    if action.priority == ServiceMenuPriority::TopLevel {
        return true;
    }
    if action.submenu.is_some() {
        return false;
    }
    if action_count <= 4 {
        return true;
    }
    let label = action.label.to_ascii_lowercase();
    [
        "compress", "extract", "archive", "terminal", "send to", "copy to", "move to",
    ]
    .iter()
    .any(|keyword| label.contains(keyword))
}

fn service_menu_action_item(action: &ServiceMenuAction) -> ShellContextMenuItem {
    ShellContextMenuItem {
        command: ShellContextMenuCommand::RunServiceMenuAction {
            action_id: action.id.clone(),
        },
        label: action.label.clone(),
        separator_before: false,
        submenu: None,
        icon: ShellContextMenuIcon::Service(action.icon.clone()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellPropertyRow {
    label: &'static str,
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellPropertiesOverlay {
    title: String,
    rows: Vec<ShellPropertyRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreateEntryKind {
    Folder,
    File,
}

impl CreateEntryKind {
    fn label(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::File => "File",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
        }
    }

    fn default_name(self) -> &'static str {
        match self {
            Self::Folder => "New Folder",
            Self::File => "New File",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellCreateDialog {
    parent: PathBuf,
    kind: CreateEntryKind,
    name: String,
    error: Option<String>,
    replace_on_insert: bool,
}

impl ShellCreateDialog {
    fn new(parent: PathBuf, kind: CreateEntryKind) -> Self {
        let name = unique_child_name(&parent, kind.default_name());
        Self {
            parent,
            kind,
            name,
            error: None,
            replace_on_insert: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CreateEntryRequest {
    parent: PathBuf,
    path: PathBuf,
    kind: CreateEntryKind,
    name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreateDialogClick {
    Outside,
    Inside,
    Cancel,
    Commit,
    Kind(CreateEntryKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellRenameDialog {
    source: PathBuf,
    parent: PathBuf,
    original_name: String,
    name: String,
    is_dir: bool,
    error: Option<String>,
    replace_on_insert: bool,
}

impl ShellRenameDialog {
    fn new(source: PathBuf, is_dir: bool) -> Option<Self> {
        let parent = source.parent()?.to_path_buf();
        let original_name = source.file_name()?.to_string_lossy().to_string();
        Some(Self {
            source,
            parent,
            name: original_name.clone(),
            original_name,
            is_dir,
            error: None,
            replace_on_insert: true,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenameEntryRequest {
    source: PathBuf,
    target: PathBuf,
    original_name: String,
    name: String,
    is_dir: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellOpenWithChooser {
    path: PathBuf,
    mime_type: Option<Arc<str>>,
    applications: Vec<MimeApplication>,
    query: String,
    selected_index: usize,
    scroll_row: usize,
    error: Option<String>,
}

impl ShellOpenWithChooser {
    fn new(path: PathBuf, mime_type: Option<Arc<str>>, applications: Vec<MimeApplication>) -> Self {
        let mut chooser = Self {
            path,
            mime_type,
            selected_index: applications
                .iter()
                .position(|application| application.is_default)
                .unwrap_or(0),
            applications,
            query: String::new(),
            scroll_row: 0,
            error: None,
        };
        chooser.ensure_selected_visible();
        chooser
    }

    fn filtered_indexes(&self) -> Vec<usize> {
        open_with_filtered_application_indexes(&self.applications, &self.query)
    }

    fn filtered_count(&self) -> usize {
        self.filtered_indexes().len()
    }

    fn visible_filtered_indexes(&self) -> Vec<usize> {
        let indexes = self.filtered_indexes();
        indexes
            .into_iter()
            .skip(self.scroll_row)
            .take(OPEN_WITH_CHOOSER_MAX_ROWS)
            .collect()
    }

    fn selected_application(&self) -> Option<&MimeApplication> {
        let indexes = self.filtered_indexes();
        let selected = self.selected_index.min(indexes.len().saturating_sub(1));
        let app_index = *indexes.get(selected)?;
        self.applications.get(app_index)
    }

    fn apply_command(&mut self, command: OpenWithCommand) -> bool {
        let old = self.clone();
        match command {
            OpenWithCommand::Insert(value) => {
                self.query.push_str(&value);
                self.selected_index = 0;
                self.scroll_row = 0;
                self.error = None;
            }
            OpenWithCommand::Backspace => {
                self.query.pop();
                self.selected_index = 0;
                self.scroll_row = 0;
                self.error = None;
            }
            OpenWithCommand::Cancel => return false,
            OpenWithCommand::MoveUp => self.move_selection(-1),
            OpenWithCommand::MoveDown => self.move_selection(1),
            OpenWithCommand::Commit | OpenWithCommand::Ignore => return false,
        }
        self.ensure_selected_visible();
        old != *self
    }

    fn select_filtered_row(&mut self, row: usize) -> bool {
        let count = self.filtered_count();
        if count == 0 {
            return false;
        }
        let old_selected = self.selected_index;
        let old_scroll = self.scroll_row;
        self.selected_index = row.min(count - 1);
        self.error = None;
        self.ensure_selected_visible();
        old_selected != self.selected_index || old_scroll != self.scroll_row
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.filtered_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        let current = self.selected_index.min(count - 1);
        self.selected_index = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            (current + delta as usize).min(count - 1)
        };
    }

    fn ensure_selected_visible(&mut self) {
        let count = self.filtered_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        self.selected_index = self.selected_index.min(count - 1);
        if self.selected_index < self.scroll_row {
            self.scroll_row = self.selected_index;
        } else if self.selected_index >= self.scroll_row + OPEN_WITH_CHOOSER_MAX_ROWS {
            self.scroll_row = self.selected_index + 1 - OPEN_WITH_CHOOSER_MAX_ROWS;
        }
        self.scroll_row = self
            .scroll_row
            .min(count.saturating_sub(OPEN_WITH_CHOOSER_MAX_ROWS));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenWithLaunchRequest {
    path: PathBuf,
    app_name: String,
    plan: DesktopLaunchPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServiceMenuLaunchRequest {
    paths: Vec<PathBuf>,
    app_name: String,
    plan: DesktopLaunchPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenWithChooserClick {
    Outside,
    Inside,
    Cancel,
    Open,
    Row(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenFileRequest {
    path: PathBuf,
    uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CopyLocationRequest {
    path: PathBuf,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeviceActionRequest {
    id: String,
    label: String,
    action: ShellContextMenuAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileClipboardExportRequest {
    role: FileClipboardRole,
    paths: Vec<PathBuf>,
    text: String,
}

#[derive(Clone, Debug)]
struct ShellPasteResult {
    mode: FileTransferMode,
    success_count: usize,
    failure_count: usize,
    clear_clipboard: bool,
}

impl ShellPasteResult {
    fn from_transfer(result: &TransferTaskResult) -> Self {
        Self {
            mode: result.mode,
            success_count: result.success_count,
            failure_count: result.failure_count,
            clear_clipboard: result.clear_clipboard,
        }
    }

    fn changed(&self) -> bool {
        self.success_count > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellTrashResult {
    success_count: usize,
    failure_count: usize,
    trash_pairs: Vec<(PathBuf, PathBuf)>,
}

impl ShellTrashResult {
    fn changed(&self) -> bool {
        self.success_count > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenameDialogClick {
    Outside,
    Inside,
    Cancel,
    Commit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellTrashConflictDialog {
    conflicts: Vec<file_ops::TrashRestoreConflict>,
}

impl ShellTrashConflictDialog {
    fn new(conflicts: Vec<file_ops::TrashRestoreConflict>) -> Option<Self> {
        (!conflicts.is_empty()).then_some(Self { conflicts })
    }

    fn first_conflict(&self) -> Option<&file_ops::TrashRestoreConflict> {
        self.conflicts.first()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrashConflictDialogClick {
    Outside,
    Inside,
    Cancel,
    Replace,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellDropTarget {
    PaneItem {
        pane: ShellPaneKind,
        index: usize,
        path: PathBuf,
        is_dir: bool,
    },
    PaneBlank {
        pane: ShellPaneKind,
        path: PathBuf,
    },
    Place {
        index: usize,
        path: PathBuf,
    },
    PlacesBlank,
}

#[derive(Clone, Debug, PartialEq)]
struct ShellInternalDrag {
    source_pane: ShellPaneKind,
    source_index: usize,
    paths: Vec<PathBuf>,
    start: ViewPoint,
    current: ViewPoint,
    active: bool,
}

impl ShellInternalDrag {
    fn new(
        source_pane: ShellPaneKind,
        source_index: usize,
        paths: Vec<PathBuf>,
        start: ViewPoint,
    ) -> Self {
        Self {
            source_pane,
            source_index,
            paths,
            start,
            current: start,
            active: false,
        }
    }

    fn update(&mut self, current: ViewPoint) -> bool {
        let old_current = self.current;
        let old_active = self.active;
        self.current = current;
        if !self.active && point_distance(self.start, current) >= RUBBER_BAND_START_THRESHOLD {
            self.active = true;
        }
        old_current != self.current || old_active != self.active
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellDropOperationRequest {
    sources: Vec<PathBuf>,
    target_dir: PathBuf,
    target: ShellDropTarget,
    mode: FileTransferMode,
}

impl ShellDropTarget {
    fn kind(&self) -> &'static str {
        match self {
            Self::PaneItem { .. } => "pane-item",
            Self::PaneBlank { .. } => "pane-blank",
            Self::Place { .. } => "place",
            Self::PlacesBlank => "places-blank",
        }
    }
}

struct ShellScene {
    primary_pane: ShellPaneState,
    places: Vec<ShellPlace>,
    location_draft: Option<LocationDraft>,
    filter_active: bool,
    filter_pattern: String,
    show_hidden: bool,
    zoom_step: i32,
    places_visible: bool,
    places_width: f32,
    places_scroll_y: f32,
    scrollbar_drag: Option<ScrollbarDrag>,
    pointer: Option<ViewPoint>,
    hovered_index: Option<usize>,
    hovered_place: Option<usize>,
    last_primary_click: Option<PrimaryClick>,
    history: PathHistory,
    selection: ShellSelection,
    context_target: Option<ShellContextTarget>,
    context_menu: Option<ShellContextMenu>,
    properties_overlay: Option<ShellPropertiesOverlay>,
    create_dialog: Option<ShellCreateDialog>,
    rename_dialog: Option<ShellRenameDialog>,
    open_with_chooser: Option<ShellOpenWithChooser>,
    trash_conflict_dialog: Option<ShellTrashConflictDialog>,
    split_pane: Option<ShellPaneState>,
    split_pane_left_fraction: f32,
    primary_visible_slots: ShellVisibleItemSlotPool,
    split_visible_slots: ShellVisibleItemSlotPool,
    visible_slot_stats: ShellVisibleItemSlotStats,
    internal_drag: Option<ShellInternalDrag>,
    dnd_hover_target: Option<ShellDropTarget>,
    pending_drop_request: Option<ShellDropOperationRequest>,
    rubber_band: Option<RubberBand>,
    scale_factor: f32,
    hit_tests: u64,
    selection_changes: u64,
    context_target_changes: u64,
    context_menu_actions: u64,
    properties_changes: u64,
    create_changes: u64,
    rename_changes: u64,
    open_with_changes: u64,
    open_changes: u64,
    copy_location_changes: u64,
    file_clipboard_changes: u64,
    paste_changes: u64,
    trash_changes: u64,
    places_changes: u64,
    places_resize_changes: u64,
    places_scroll_changes: u64,
    keyboard_navigation: u64,
    rubber_band_updates: u64,
    view_switches: u64,
    path_changes: u64,
    directory_reloads: u64,
    location_changes: u64,
    filter_changes: u64,
    hidden_changes: u64,
    zoom_changes: u64,
    split_pane_changes: u64,
    dnd_hover_changes: u64,
    dnd_drop_requests: u64,
}

impl ShellScene {
    fn load(path: PathBuf, view_mode: ShellViewMode) -> Result<Self, String> {
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        eprintln!(
            "[fika-wgpu] path={} entries={} dirs={} files={} load={}us",
            path.display(),
            entries.len(),
            dir_count,
            entries.len().saturating_sub(dir_count),
            elapsed.as_micros()
        );
        if !preview.is_empty() {
            eprintln!("[fika-wgpu] first-entries={preview}");
        }

        let primary_pane = ShellPaneState::from_entries(path, view_mode, entries, false, "");
        let places = build_shell_places();
        eprintln!("[fika-wgpu] places entries={}", places.len());

        Ok(Self {
            primary_pane,
            places,
            location_draft: None,
            filter_active: false,
            filter_pattern: String::new(),
            show_hidden: false,
            zoom_step: 0,
            places_visible: true,
            places_width: PLACES_SIDEBAR_WIDTH,
            places_scroll_y: 0.0,
            scrollbar_drag: None,
            pointer: None,
            hovered_index: None,
            hovered_place: None,
            last_primary_click: None,
            history: PathHistory::default(),
            selection: ShellSelection::default(),
            context_target: None,
            context_menu: None,
            properties_overlay: None,
            create_dialog: None,
            rename_dialog: None,
            open_with_chooser: None,
            trash_conflict_dialog: None,
            split_pane: None,
            split_pane_left_fraction: 0.5,
            primary_visible_slots: ShellVisibleItemSlotPool::default(),
            split_visible_slots: ShellVisibleItemSlotPool::default(),
            visible_slot_stats: ShellVisibleItemSlotStats::default(),
            internal_drag: None,
            dnd_hover_target: None,
            pending_drop_request: None,
            rubber_band: None,
            scale_factor: 1.0,
            hit_tests: 0,
            selection_changes: 0,
            context_target_changes: 0,
            context_menu_actions: 0,
            properties_changes: 0,
            create_changes: 0,
            rename_changes: 0,
            open_with_changes: 0,
            open_changes: 0,
            copy_location_changes: 0,
            file_clipboard_changes: 0,
            paste_changes: 0,
            trash_changes: 0,
            places_changes: 0,
            places_resize_changes: 0,
            places_scroll_changes: 0,
            keyboard_navigation: 0,
            rubber_band_updates: 0,
            view_switches: 0,
            path_changes: 0,
            directory_reloads: 0,
            location_changes: 0,
            filter_changes: 0,
            hidden_changes: 0,
            zoom_changes: 0,
            split_pane_changes: 0,
            dnd_hover_changes: 0,
            dnd_drop_requests: 0,
        })
    }

    fn load_path(&mut self, path: PathBuf, size: PhysicalSize<u32>) -> Result<bool, String> {
        if path == self.primary_pane.path {
            return Ok(false);
        }
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        let previous_path = self.primary_pane.path.clone();
        self.history.push_back(previous_path);
        self.history.clear_forward();
        self.apply_loaded_path(path, entries, dir_count, size);

        self.log_loaded_path(dir_count, &preview, elapsed);
        Ok(true)
    }

    fn reload_current_path(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let load_start = Instant::now();
        let entries = read_entries_sync(&self.primary_pane.path).map_err(|error| {
            format!(
                "read directory {}: {error}",
                self.primary_pane.path.display()
            )
        })?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        let remapped_selection = self.selection_for_reloaded_entries(&entries);
        let previous_selection = self.selection.clone();

        self.primary_pane.entries = entries;
        self.primary_pane.dir_count = dir_count;
        self.selection = remapped_selection;
        self.rebuild_filtered_indexes();
        let pruned_selection = self
            .selection
            .retain_indexes(&self.primary_pane.filtered_indexes);
        let selection_changed = previous_selection != self.selection;
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.last_primary_click = None;
        self.directory_reloads += 1;
        if selection_changed || pruned_selection {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        self.log_reloaded_path(dir_count, &preview, elapsed, selection_changed);
        Ok(true)
    }

    fn go_parent_directory(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let Some(path) = self.parent_directory_path() else {
            return Ok(false);
        };
        self.load_path(path, size)
    }

    fn go_history_back(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let Some(path) = self.history.back.last().cloned() else {
            return Ok(false);
        };
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        let current_path = self.primary_pane.path.clone();
        self.history.back.pop();
        self.history.push_forward(current_path);
        self.apply_loaded_path(path, entries, dir_count, size);
        self.log_loaded_path(dir_count, &preview, elapsed);
        Ok(true)
    }

    fn go_history_forward(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let Some(path) = self.history.forward.last().cloned() else {
            return Ok(false);
        };
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let preview = entries
            .iter()
            .take(8)
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");

        let current_path = self.primary_pane.path.clone();
        self.history.forward.pop();
        self.history.push_back(current_path);
        self.apply_loaded_path(path, entries, dir_count, size);
        self.log_loaded_path(dir_count, &preview, elapsed);
        Ok(true)
    }

    fn apply_loaded_path(
        &mut self,
        path: PathBuf,
        entries: Vec<Entry>,
        _dir_count: usize,
        size: PhysicalSize<u32>,
    ) {
        self.primary_pane = ShellPaneState::from_entries(
            path,
            self.primary_pane.view_mode,
            entries,
            self.show_hidden,
            &self.filter_pattern,
        );
        self.primary_visible_slots.clear();
        self.primary_pane.scroll_x = 0.0;
        self.primary_pane.scroll_y = 0.0;
        self.location_draft = None;
        self.selection = ShellSelection::default();
        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.internal_drag = None;
        self.pending_drop_request = None;
        self.clear_dnd_hover_target();
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.last_primary_click = None;
        self.path_changes += 1;
        self.clamp_scroll(size);
    }

    fn log_loaded_path(&self, dir_count: usize, preview: &str, elapsed: Duration) {
        eprintln!(
            "[fika-wgpu] path={} entries={} dirs={} files={} load={}us changes={}",
            self.primary_pane.path.display(),
            self.primary_pane.entries.len(),
            dir_count,
            self.primary_pane.entries.len().saturating_sub(dir_count),
            elapsed.as_micros(),
            self.path_changes
        );
        if !preview.is_empty() {
            eprintln!("[fika-wgpu] first-entries={preview}");
        }
    }

    fn log_reloaded_path(
        &self,
        dir_count: usize,
        preview: &str,
        elapsed: Duration,
        selection_changed: bool,
    ) {
        eprintln!(
            "[fika-wgpu] reload path={} entries={} dirs={} files={} load={}us reloads={} selected={} selection_changed={}",
            self.primary_pane.path.display(),
            self.primary_pane.entries.len(),
            dir_count,
            self.primary_pane.entries.len().saturating_sub(dir_count),
            elapsed.as_micros(),
            self.directory_reloads,
            self.selection.len(),
            selection_changed as u8
        );
        if !preview.is_empty() {
            eprintln!("[fika-wgpu] first-entries={preview}");
        }
    }

    fn set_view_mode(&mut self, view_mode: ShellViewMode, size: PhysicalSize<u32>) -> bool {
        if self.primary_pane.view_mode == view_mode {
            return false;
        }
        for kind in [ShellPaneKind::Primary, ShellPaneKind::Split] {
            if let Some(pane) = self.pane_state_mut(kind) {
                pane.view_mode = view_mode;
                pane.scroll_x = 0.0;
                pane.scroll_y = 0.0;
            }
        }
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.view_switches += 1;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] view-mode={} switches={} scroll_x={:.1} scroll_y={:.1}",
            self.primary_pane.view_mode.as_str(),
            self.view_switches,
            self.primary_pane.scroll_x,
            self.primary_pane.scroll_y
        );
        true
    }

    fn zoom(&mut self, action: ZoomAction, size: PhysicalSize<u32>) -> bool {
        let next_step = match action {
            ZoomAction::In => self.zoom_step + 1,
            ZoomAction::Out => self.zoom_step - 1,
            ZoomAction::Reset => 0,
        }
        .clamp(ZOOM_STEP_MIN, ZOOM_STEP_MAX);

        if next_step == self.zoom_step {
            return false;
        }

        self.zoom_step = next_step;
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.zoom_changes += 1;
        if let Some(index) = self.selection.focus_or_first_selected() {
            self.ensure_index_visible(index, size);
        } else {
            self.clamp_scroll(size);
        }
        eprintln!(
            "[fika-wgpu] zoom step={} percent={} changes={} scroll_x={:.1} scroll_y={:.1}",
            self.zoom_step,
            self.zoom_percent(),
            self.zoom_changes,
            self.primary_pane.scroll_x,
            self.primary_pane.scroll_y
        );
        true
    }

    fn apply_selection_command(&mut self, command: SelectionCommand) -> bool {
        let rubber_band_changed = self.rubber_band.take().is_some();
        let selection_changed = match command {
            SelectionCommand::SelectAll => self
                .selection
                .select_indexes(&self.primary_pane.filtered_indexes),
            SelectionCommand::Clear => self.selection.clear(),
        };
        if selection_changed {
            self.selection_changes += 1;
        }
        if selection_changed || rubber_band_changed {
            eprintln!(
                "[fika-wgpu] selection command={} selected={} changes={}",
                command.as_str(),
                self.selection.len(),
                self.selection_changes
            );
        }
        selection_changed || rubber_band_changed
    }

    fn is_location_editing(&self) -> bool {
        self.location_draft.is_some()
    }

    fn location_draft_value(&self) -> Option<&str> {
        self.location_draft
            .as_ref()
            .map(|draft| draft.value.as_str())
    }

    fn resolved_location_draft(&self) -> Option<PathBuf> {
        let value = self.location_draft_value()?;
        resolve_location_input(&self.primary_pane.path, value)
    }

    fn close_location_draft(&mut self, size: PhysicalSize<u32>) -> bool {
        if self.location_draft.take().is_none() {
            return false;
        }
        self.location_changes += 1;
        self.rubber_band = None;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] location active=0 value=\"\" changes={}",
            self.location_changes
        );
        true
    }

    fn close_location_draft_if_outside(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        if !self.is_location_editing() || self.path_bar_contains_screen_point(point, size) {
            return false;
        }
        self.close_location_draft(size)
    }

    fn apply_location_command(
        &mut self,
        command: LocationCommand,
        size: PhysicalSize<u32>,
    ) -> bool {
        let old_draft = self.location_draft.clone();
        let old_filter_active = self.filter_active;

        match command {
            LocationCommand::Activate => {
                self.location_draft = Some(LocationDraft::new(
                    self.primary_pane.path.display().to_string(),
                ));
                self.filter_active = false;
            }
            LocationCommand::Insert(value) => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.insert(&value);
            }
            LocationCommand::Backspace => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.backspace();
            }
            LocationCommand::Delete => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.delete();
            }
            LocationCommand::MoveLeft => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.move_left();
            }
            LocationCommand::MoveRight => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.move_right();
            }
            LocationCommand::MoveHome => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.move_home();
            }
            LocationCommand::MoveEnd => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                draft.move_end();
            }
            LocationCommand::Cancel => {
                self.location_draft = None;
            }
            LocationCommand::Complete => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                let Some(completed) =
                    complete_location_input(&self.primary_pane.path, &draft.value)
                else {
                    return false;
                };
                draft.set_completed(completed);
            }
            LocationCommand::Commit | LocationCommand::Ignore => return false,
        }

        let changed = old_draft != self.location_draft || old_filter_active != self.filter_active;
        if !changed {
            return false;
        }

        self.location_changes += 1;
        self.rubber_band = None;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] location active={} value={:?} changes={}",
            self.location_draft.is_some() as u8,
            self.location_draft_value().unwrap_or(""),
            self.location_changes
        );
        true
    }

    fn apply_filter_command(&mut self, command: FilterCommand, size: PhysicalSize<u32>) -> bool {
        let old_active = self.filter_active;
        let old_pattern = self.filter_pattern.clone();

        match command {
            FilterCommand::Activate => {
                self.filter_active = true;
            }
            FilterCommand::Insert(value) => {
                self.filter_active = true;
                self.filter_pattern.push_str(&value);
            }
            FilterCommand::Backspace => {
                self.filter_active = true;
                self.filter_pattern.pop();
            }
            FilterCommand::ClearAndDeactivate => {
                self.filter_active = false;
                self.filter_pattern.clear();
            }
            FilterCommand::Deactivate => {
                self.filter_active = false;
            }
        }

        let filter_changed = old_active != self.filter_active || old_pattern != self.filter_pattern;
        if !filter_changed {
            return false;
        }

        self.filter_changes += 1;
        self.rubber_band = None;
        self.rebuild_filtered_indexes();
        let selection_changed = self
            .selection
            .retain_indexes(&self.primary_pane.filtered_indexes);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] filter active={} pattern={:?} matches={} changes={} selection_changed={}",
            self.filter_active as u8,
            self.filter_pattern,
            self.primary_pane.filtered_indexes.len(),
            self.filter_changes,
            selection_changed as u8
        );
        true
    }

    fn toggle_hidden_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        self.show_hidden = !self.show_hidden;
        self.hidden_changes += 1;
        self.rubber_band = None;
        self.rebuild_filtered_indexes();
        if let Some(split_pane) = self.split_pane.as_mut() {
            split_pane.rebuild_filtered_indexes(self.show_hidden);
        }
        let selection_changed = self
            .selection
            .retain_indexes(&self.primary_pane.filtered_indexes);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] hidden show={} visible={} changes={} selection_changed={}",
            self.show_hidden as u8,
            self.filtered_entry_count(),
            self.hidden_changes,
            selection_changed as u8
        );
        true
    }

    fn open_split_pane_from_context(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let path = self
            .context_target_split_pane_path()
            .unwrap_or_else(|| self.primary_pane.path.clone());
        self.open_split_pane(path, size)
    }

    fn open_split_pane(&mut self, path: PathBuf, size: PhysicalSize<u32>) -> Result<bool, String> {
        let mut split_pane =
            ShellPaneState::load(path, self.primary_pane.view_mode, self.show_hidden)?;
        split_pane.scroll_x = 0.0;
        split_pane.scroll_y = 0.0;
        self.split_pane = Some(split_pane);
        self.split_visible_slots.clear();
        self.split_pane_left_fraction = 0.5;
        self.split_pane_changes += 1;
        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.open_with_chooser = None;
        self.trash_conflict_dialog = None;
        self.internal_drag = None;
        self.pending_drop_request = None;
        self.clear_dnd_hover_target();
        self.rubber_band = None;
        self.scrollbar_drag = None;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] split-pane open=1 changes={} left={} right={}",
            self.split_pane_changes,
            self.primary_pane.path.display(),
            self.split_pane
                .as_ref()
                .map(|pane| pane.path.display().to_string())
                .unwrap_or_default()
        );
        Ok(true)
    }

    fn context_target_split_pane_path(&self) -> Option<PathBuf> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item { path, is_dir, .. } if *is_dir => Some(path.clone()),
            ShellContextTarget::Item { path, .. } => path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .or_else(|| Some(self.primary_pane.path.clone())),
            ShellContextTarget::Blank { path, .. } | ShellContextTarget::Place { path, .. } => {
                Some(path.clone())
            }
        }
    }

    fn rebuild_filtered_indexes(&mut self) {
        self.primary_pane
            .rebuild_filtered_indexes_with_pattern(self.show_hidden, &self.filter_pattern);
    }

    fn filtered_entry_count(&self) -> usize {
        self.primary_pane.filtered_indexes.len()
    }

    fn set_scale_factor(&mut self, scale_factor: f32, size: PhysicalSize<u32>) -> bool {
        let next = normalized_scale_factor(scale_factor);
        if (self.scale_factor - next).abs() <= 0.01 {
            self.scale_factor = next;
            self.clamp_scroll(size);
            return false;
        }

        let old_ui_scale = self.ui_scale();
        self.scale_factor = next;
        let next_ui_scale = self.ui_scale();
        if old_ui_scale > f32::EPSILON {
            let ratio = next_ui_scale / old_ui_scale;
            self.primary_pane.scroll_x *= ratio;
            self.primary_pane.scroll_y *= ratio;
            self.places_scroll_y *= ratio;
        }
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] scale-factor={:.2} ui_scale={:.2} scroll_x={:.1} scroll_y={:.1}",
            self.scale_factor,
            self.ui_scale(),
            self.primary_pane.scroll_x,
            self.primary_pane.scroll_y
        );
        true
    }

    fn ui_scale(&self) -> f32 {
        normalized_scale_factor(self.scale_factor).max(1.0)
    }

    fn scale_metric(&self, value: f32) -> f32 {
        (value * self.ui_scale()).round().max(1.0)
    }

    fn zoomed_metric(&self, value: f32, min: f32, max: f32) -> f32 {
        let scale = self.ui_scale();
        (value * self.zoom_factor() * scale)
            .round()
            .clamp(min * scale, max * scale)
    }

    fn text_line_height(&self) -> f32 {
        self.scale_metric(TEXT_LINE_HEIGHT)
    }

    fn small_text_line_height(&self) -> f32 {
        self.scale_metric(14.0)
    }

    fn app_toolbar_height(&self) -> f32 {
        self.scale_metric(APP_TOOLBAR_HEIGHT)
    }

    fn app_toolbar_y(&self) -> f32 {
        0.0
    }

    fn pane_margin(&self) -> f32 {
        self.scale_metric(PANE_MARGIN)
    }

    fn pane_top_y(&self) -> f32 {
        self.app_toolbar_height() + self.pane_margin()
    }

    fn top_bar_height(&self) -> f32 {
        self.scale_metric(TOP_BAR_HEIGHT)
    }

    fn status_bar_height(&self) -> f32 {
        self.scale_metric(STATUS_BAR_HEIGHT)
    }

    fn details_header_height(&self) -> f32 {
        self.scale_metric(DETAILS_HEADER_HEIGHT)
    }

    fn details_name_width(&self) -> f32 {
        self.scale_metric(DETAILS_NAME_WIDTH)
    }

    fn details_size_width(&self) -> f32 {
        self.scale_metric(DETAILS_SIZE_WIDTH)
    }

    fn details_modified_width(&self) -> f32 {
        self.scale_metric(DETAILS_MODIFIED_WIDTH)
    }

    fn model_index_for_layout_index(&self, layout_index: usize) -> Option<usize> {
        self.primary_pane
            .filtered_indexes
            .get(layout_index)
            .copied()
    }

    fn layout_index_for_model_index(&self, model_index: usize) -> Option<usize> {
        self.primary_pane
            .filtered_indexes
            .binary_search(&model_index)
            .ok()
    }

    fn selection_for_reloaded_entries(&self, entries: &[Entry]) -> ShellSelection {
        if self.selection.selected.is_empty() {
            return ShellSelection::default();
        }

        let selected_names = self
            .selection
            .selected
            .iter()
            .filter_map(|index| self.primary_pane.entries.get(*index))
            .map(|entry| entry.name.to_string())
            .collect::<BTreeSet<_>>();
        let anchor_name = self
            .selection
            .anchor
            .and_then(|index| self.primary_pane.entries.get(index))
            .map(|entry| entry.name.to_string());
        let focus_name = self
            .selection
            .focus
            .and_then(|index| self.primary_pane.entries.get(index))
            .map(|entry| entry.name.to_string());

        let selected = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                selected_names
                    .contains(entry.name.as_ref())
                    .then_some(index)
            })
            .collect::<BTreeSet<_>>();
        if selected.is_empty() {
            return ShellSelection::default();
        }

        let anchor = anchor_name
            .and_then(|name| entry_index_by_name(entries, &name))
            .filter(|index| selected.contains(index))
            .or_else(|| selected.iter().next().copied());
        let focus = focus_name
            .and_then(|name| entry_index_by_name(entries, &name))
            .filter(|index| selected.contains(index))
            .or_else(|| selected.iter().next_back().copied());

        ShellSelection {
            selected,
            anchor,
            focus,
        }
    }

    fn view_mode_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellViewMode> {
        let _ = (point, size);
        None
    }

    fn path_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let pane = self.pane_rect(size);
        let margin = self.scale_metric(8.0);
        let path_x = pane.x + margin;
        let available_width = (pane.right() - path_x - margin).max(0.0);
        let rect = ViewRect {
            x: path_x,
            y: self.pane_top_y() + self.scale_metric(4.0),
            width: available_width,
            height: self.scale_metric(28.0),
        };
        (rect.width > self.scale_metric(24.0)).then_some(rect)
    }

    fn path_bar_contains_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.path_bar_rect(size)
            .is_some_and(|rect| rect.contains(point))
    }

    fn path_navigation_action_at_screen_point(
        &self,
        point: ViewPoint,
        _size: PhysicalSize<u32>,
    ) -> Option<PathNavigationAction> {
        let _ = point;
        None
    }

    fn app_toolbar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: 0.0,
            y: self.app_toolbar_y(),
            width: size.width.max(1) as f32,
            height: self.app_toolbar_height(),
        }
    }

    fn places_toggle_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let toolbar = self.app_toolbar_rect(size);
        ViewRect {
            x: self.scale_metric(8.0),
            y: toolbar.y + self.scale_metric(8.0),
            width: self.scale_metric(28.0),
            height: self
                .scale_metric(28.0)
                .min((toolbar.height - self.scale_metric(8.0)).max(1.0)),
        }
    }

    fn toggle_places_at_screen_point(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<bool> {
        self.places_toggle_rect(size)
            .contains(point)
            .then(|| self.toggle_places_visibility(size))
    }

    fn toggle_places_visibility(&mut self, size: PhysicalSize<u32>) -> bool {
        self.places_visible = !self.places_visible;
        self.places_changes += 1;
        self.scrollbar_drag = None;
        self.rubber_band = None;
        self.hovered_place = None;
        self.last_primary_click = None;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] places visible={} width={:.1} changes={}",
            self.places_visible as u8,
            self.places_sidebar_width(size),
            self.places_changes
        );
        true
    }

    fn places_resize_handle_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        if !self.places_visible {
            return None;
        }
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return None;
        }
        let handle_width = self
            .scale_metric(PLACES_RESIZE_HANDLE_WIDTH)
            .max(self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH));
        let splitter_cover = self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH + 2.0);
        Some(ViewRect {
            x: (sidebar.right() - handle_width).max(sidebar.x),
            y: sidebar.y,
            width: (handle_width + splitter_cover).min(sidebar.width + splitter_cover),
            height: sidebar.height,
        })
    }

    fn split_pane_resize_handle_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let divider = self.split_pane_metrics(size)?.divider;
        let handle_width = self
            .scale_metric(SPLIT_PANE_RESIZE_HANDLE_WIDTH)
            .max(divider.width);
        Some(ViewRect {
            x: divider.x + (divider.width - handle_width) / 2.0,
            y: divider.y,
            width: handle_width,
            height: divider.height,
        })
    }

    fn cursor_icon(&self, size: PhysicalSize<u32>) -> CursorIcon {
        if self.scrollbar_drag.is_some_and(|drag| {
            matches!(
                drag.target,
                ScrollbarDragTarget::PlacesResize | ScrollbarDragTarget::SplitPaneResize
            )
        }) {
            return CursorIcon::ColResize;
        }
        if self.scrollbar_drag.is_some() {
            return CursorIcon::Default;
        }
        let Some(point) = self.pointer else {
            return CursorIcon::Default;
        };
        if self
            .places_scrollbar_rects(size)
            .is_some_and(|(track, _)| track.contains(point))
        {
            return CursorIcon::Default;
        }
        if self
            .places_resize_handle_rect(size)
            .is_some_and(|rect| rect.contains(point))
            || self
                .split_pane_resize_handle_rect(size)
                .is_some_and(|rect| rect.contains(point))
        {
            CursorIcon::ColResize
        } else if self.path_bar_contains_screen_point(point, size) {
            CursorIcon::Text
        } else {
            CursorIcon::Default
        }
    }

    fn place_activation_for_primary_press(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<PathBuf> {
        let index = self.place_index_at_screen_point(point, size)?;
        self.pointer = Some(point);
        let hover_changed = self.set_hovered_place(Some(index));
        let item_hover_changed = self.set_hovered_index(None);
        self.rubber_band = None;
        self.internal_drag = None;
        self.last_primary_click = None;
        self.context_target = None;
        self.context_menu = None;
        let place = self.places.get(index)?;
        if place.device.as_ref().is_some_and(|device| !device.mounted) {
            self.places_changes += 1;
            eprintln!(
                "[fika-wgpu] place-open index={} label={:?} mounted=0 path={} changes={}",
                index,
                place.label,
                place.path.display(),
                self.places_changes
            );
            return None;
        }
        self.places_changes += 1;
        eprintln!(
            "[fika-wgpu] place-open index={} label={:?} path={} hover_changed={} item_hover_changed={} changes={}",
            index,
            place.label,
            place.path.display(),
            hover_changed as u8,
            item_hover_changed as u8,
            self.places_changes
        );
        Some(place.path.clone())
    }

    fn place_index_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        if !self.places_sidebar_rect(size).contains(point) {
            return None;
        }
        self.place_row_rects(size)
            .into_iter()
            .find_map(|(index, rect)| rect.contains(point).then_some(index))
    }

    fn place_row_rects(&self, size: PhysicalSize<u32>) -> Vec<(usize, ViewRect)> {
        let panel = self.places_panel_rect(size);
        if panel.width <= 0.0 || panel.height <= 0.0 {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(self.places.len());
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let mut y = panel.y + top_padding + title_height - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                y += section_height;
            }
            let rect = ViewRect {
                x: panel.x + padding_x,
                y,
                width: (panel.width - padding_x * 2.0).max(1.0),
                height: row_height,
            };
            if rect.y < panel.bottom() && rect.bottom() > panel.y {
                rows.push((index, rect));
            }
            y += row_height + row_gap;
            previous_group = Some(place.group);
        }
        rows
    }

    fn places_sidebar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let width = self.places_sidebar_width(size);
        let y = self.pane_top_y();
        let height = (size.height as f32 - y - self.pane_margin()).max(1.0);
        ViewRect {
            x: 0.0,
            y,
            width,
            height,
        }
    }

    fn places_panel_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return ViewRect {
                x: sidebar.x,
                y: sidebar.y,
                width: 0.0,
                height: 0.0,
            };
        }
        let margin_x = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_X)
            .min(sidebar.width / 3.0);
        let margin_bottom = self
            .scale_metric(PLACES_SIDEBAR_PANEL_MARGIN_BOTTOM)
            .min(sidebar.height / 3.0);
        let y = sidebar.y;
        ViewRect {
            x: sidebar.x + margin_x,
            y,
            width: (sidebar.width - margin_x * 2.0).max(1.0),
            height: (sidebar.bottom() - y - margin_bottom).max(1.0),
        }
    }

    fn places_content_height(&self) -> f32 {
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        if self.places.is_empty() {
            return top_padding * 2.0 + title_height;
        }

        let mut height = top_padding + title_height;
        let mut previous_group = None;
        for place in &self.places {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                height += section_height;
            }
            height += row_height + row_gap;
            previous_group = Some(place.group);
        }
        height - row_gap + top_padding
    }

    fn max_places_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        let panel = self.places_panel_rect(size);
        (self.places_content_height() - panel.height).max(0.0)
    }

    #[cfg(test)]
    fn places_scrollbar_thumb_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        self.places_scrollbar_rects(size).map(|(_, thumb)| thumb)
    }

    fn places_scrollbar_rects(&self, size: PhysicalSize<u32>) -> Option<(ViewRect, ViewRect)> {
        let panel = self.places_panel_rect(size);
        let max_scroll = self.max_places_scroll_y(size);
        if panel.width <= 0.0 || panel.height <= 0.0 || max_scroll <= f32::EPSILON {
            return None;
        }

        let scrollbar_margin = self.scale_metric(PLACES_SCROLLBAR_MARGIN);
        let scrollbar_width = self.scale_metric(PLACES_SCROLLBAR_WIDTH);
        let min_thumb_height = self.scale_metric(PLACES_SCROLLBAR_MIN_THUMB_HEIGHT);
        let track_height = (panel.height - scrollbar_margin * 2.0).max(1.0);
        let content_height = self.places_content_height().max(panel.height);
        let thumb_height = (panel.height / content_height * track_height)
            .clamp(min_thumb_height.min(track_height), track_height);
        let travel = (track_height - thumb_height).max(0.0);
        let scroll_ratio = if max_scroll <= f32::EPSILON {
            0.0
        } else {
            (self.places_scroll_y / max_scroll).clamp(0.0, 1.0)
        };
        let track = ViewRect {
            x: panel.right() - scrollbar_margin - scrollbar_width,
            y: panel.y + scrollbar_margin,
            width: scrollbar_width,
            height: track_height,
        };
        let thumb = ViewRect {
            x: track.x,
            y: panel.y + scrollbar_margin + travel * scroll_ratio,
            width: scrollbar_width,
            height: thumb_height,
        };
        Some((track, thumb))
    }

    fn context_target_for_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextTarget> {
        if let Some(index) = self.place_index_at_screen_point(point, size) {
            let place = self.places.get(index)?;
            return Some(ShellContextTarget::Place {
                index,
                label: place.label.clone(),
                path: place.path.clone(),
                group: place.group,
                device: place.device.clone(),
                network: place.network,
                trash: place.trash,
                root: place.root,
                editable: place.editable,
            });
        }
        if !self.content_screen_rect(size).contains(point) {
            return None;
        }
        if let Some(index) = self.hit_test_screen_point(point, size) {
            let entry = self.primary_pane.entries.get(index)?;
            let selection_count = if self.selection.contains(index) {
                self.selection.len().max(1)
            } else {
                1
            };
            return Some(ShellContextTarget::Item {
                index,
                path: self.entry_path_for_index(index)?,
                is_dir: entry.is_dir,
                selection_count,
            });
        }
        Some(ShellContextTarget::Blank {
            path: self.primary_pane.path.clone(),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn drop_target_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellDropTarget> {
        if let Some(index) = self.place_index_at_screen_point(point, size) {
            let place = self.places.get(index)?;
            return Some(ShellDropTarget::Place {
                index,
                path: place.path.clone(),
            });
        }
        if self.places_visible && self.places_sidebar_rect(size).contains(point) {
            return Some(ShellDropTarget::PlacesBlank);
        }

        for geometry in self.pane_geometries(size) {
            if !geometry.content.contains(point) {
                continue;
            }
            let pane = self.pane_view(geometry.kind)?;
            if let Some(index) = self.pane_hit_test_screen_point(pane, geometry, point) {
                let entry = pane.entries.get(index)?;
                return Some(ShellDropTarget::PaneItem {
                    pane: geometry.kind,
                    index,
                    path: self.entry_path_for_pane_view(pane, index)?,
                    is_dir: entry.is_dir,
                });
            }
            return Some(ShellDropTarget::PaneBlank {
                pane: geometry.kind,
                path: pane.path.to_path_buf(),
            });
        }

        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn update_dnd_hover_target(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let next = self.drop_target_at_screen_point(point, size);
        let changed = self.dnd_hover_target != next;
        if changed {
            self.dnd_hover_target = next;
            self.dnd_hover_changes += 1;
        }
        changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn clear_dnd_hover_target(&mut self) -> bool {
        let changed = self.dnd_hover_target.take().is_some();
        if changed {
            self.dnd_hover_changes += 1;
        }
        changed
    }

    fn primary_drag_paths_for_index(&self, index: usize) -> Vec<PathBuf> {
        if self.selection.contains(index) {
            let paths = self
                .selection
                .selected
                .iter()
                .filter_map(|index| self.entry_path_for_index(*index))
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                return paths;
            }
        }
        self.entry_path_for_index(index).into_iter().collect()
    }

    fn begin_internal_drag_for_primary_item(&mut self, index: usize, point: ViewPoint) -> bool {
        let paths = self.primary_drag_paths_for_index(index);
        if paths.is_empty() {
            self.internal_drag = None;
            return false;
        }
        self.internal_drag = Some(ShellInternalDrag::new(
            ShellPaneKind::Primary,
            index,
            paths,
            point,
        ));
        true
    }

    fn update_internal_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(drag) = self.internal_drag.as_mut() else {
            return false;
        };
        let drag_changed = drag.update(point);
        if !drag.active {
            return drag_changed;
        }
        let hover_changed = self.update_dnd_hover_target(point, size);
        drag_changed || hover_changed
    }

    fn finish_internal_drag(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellDropOperationRequest> {
        let drag = self.internal_drag.take()?;
        if !drag.active {
            let _ = self.clear_dnd_hover_target();
            return None;
        }
        let Some(target) = self.drop_target_at_screen_point(point, size) else {
            let _ = self.clear_dnd_hover_target();
            return None;
        };
        let Some(target_dir) = self.target_dir_for_drop_target(&target) else {
            let _ = self.clear_dnd_hover_target();
            return None;
        };
        if drag.paths.iter().any(|source| source == &target_dir) {
            let _ = self.clear_dnd_hover_target();
            return None;
        }
        let request = ShellDropOperationRequest {
            sources: drag.paths,
            target_dir,
            target,
            mode: FileTransferMode::Copy,
        };
        self.pending_drop_request = Some(request.clone());
        self.dnd_drop_requests += 1;
        let _ = self.clear_dnd_hover_target();
        eprintln!(
            "[fika-wgpu] dnd-drop-request sources={} target={} mode={} requests={}",
            request.sources.len(),
            request.target_dir.display(),
            request.mode.operation(),
            self.dnd_drop_requests
        );
        Some(request)
    }

    fn target_dir_for_drop_target(&self, target: &ShellDropTarget) -> Option<PathBuf> {
        match target {
            ShellDropTarget::PaneItem { path, is_dir, .. } if *is_dir => Some(path.clone()),
            ShellDropTarget::PaneBlank { path, .. } | ShellDropTarget::Place { path, .. } => {
                Some(path.clone())
            }
            ShellDropTarget::PaneItem { .. } | ShellDropTarget::PlacesBlank => None,
        }
    }

    fn open_context_target(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        let target = self.context_target_for_screen_point(point, size);
        let old_target = self.context_target.clone();
        let old_rubber_band_active = self.rubber_band.as_ref().is_some_and(|band| band.active);
        let rubber_band_cleared = self.rubber_band.take().is_some();
        let hover = target.as_ref().and_then(|target| match target {
            ShellContextTarget::Item { index, .. } => Some(*index),
            ShellContextTarget::Blank { .. } | ShellContextTarget::Place { .. } => None,
        });
        let hover_changed = self.set_hovered_index(hover);
        let place_hover = target.as_ref().and_then(|target| match target {
            ShellContextTarget::Place { index, .. } => Some(*index),
            ShellContextTarget::Item { .. } | ShellContextTarget::Blank { .. } => None,
        });
        let place_hover_changed = self.set_hovered_place(place_hover);

        let mut selection_changed = false;
        if let Some(ShellContextTarget::Item { index, .. }) = target.as_ref() {
            selection_changed = if self.selection.contains(*index) {
                self.selection.focus_selected(*index)
            } else {
                self.selection.apply_click(Some(*index), false, false)
            };
            if selection_changed {
                self.selection_changes += 1;
            }
        }

        let target_changed = old_target != target;
        self.context_target = target;
        if target_changed {
            self.context_target_changes += 1;
            self.log_context_target();
        }

        target_changed
            || hover_changed
            || place_hover_changed
            || selection_changed
            || rubber_band_cleared
            || old_rubber_band_active
    }

    fn is_context_menu_open(&self) -> bool {
        self.context_menu.is_some()
    }

    #[cfg(test)]
    fn open_context_menu(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.open_context_menu_with_cache(point, size, &MimeApplicationCache::empty())
    }

    fn open_context_menu_with_cache(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        cache: &MimeApplicationCache,
    ) -> bool {
        let changed = self.open_context_target(point, size);
        let old_menu = self.context_menu.clone();
        self.context_menu = self.context_target.clone().map(|target| {
            let (open_with_apps, service_actions) = self.context_menu_dynamic_data(&target, cache);
            ShellContextMenu::with_dynamic(target, point, open_with_apps, service_actions)
        });
        let menu_changed = old_menu != self.context_menu;
        if menu_changed {
            eprintln!(
                "[fika-wgpu] context-menu open={} target={} actions={}",
                self.context_menu.is_some() as u8,
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                self.context_menu
                    .as_ref()
                    .map(|menu| context_menu_items(menu).len())
                    .unwrap_or(0)
            );
        }
        changed || menu_changed
    }

    fn context_menu_dynamic_data(
        &self,
        target: &ShellContextTarget,
        cache: &MimeApplicationCache,
    ) -> (Vec<MimeApplication>, Vec<ServiceMenuAction>) {
        match target {
            ShellContextTarget::Item {
                index,
                path,
                is_dir,
                ..
            } => {
                let mime_type = self
                    .primary_pane
                    .entries
                    .get(*index)
                    .and_then(|entry| entry.mime_type.as_deref());
                let open_with_apps = if *is_dir || file_ops::is_in_trash_files_dir(path) {
                    Vec::new()
                } else {
                    open_with_applications_for_mime(cache, mime_type)
                };
                let service_actions = if file_ops::is_in_trash_files_dir(path) {
                    Vec::new()
                } else {
                    cache.service_actions_for_targets(
                        &self.service_menu_targets_for_context_item(*index, *is_dir, mime_type),
                    )
                };
                (open_with_apps, service_actions)
            }
            ShellContextTarget::Blank { path } => {
                let open_with_apps = if file_ops::is_trash_files_dir(&path) {
                    Vec::new()
                } else {
                    open_with_applications_for_mime(cache, Some("inode/directory"))
                };
                let service_actions = if file_ops::is_trash_files_dir(&path) {
                    Vec::new()
                } else {
                    cache.service_actions_for_targets(&[ServiceMenuTarget::new(
                        Some("inode/directory"),
                        true,
                    )])
                };
                (open_with_apps, service_actions)
            }
            ShellContextTarget::Place { .. } => (Vec::new(), Vec::new()),
        }
    }

    fn service_menu_targets_for_context_item(
        &self,
        index: usize,
        is_dir: bool,
        mime_type: Option<&str>,
    ) -> Vec<ServiceMenuTarget> {
        if self.selection.contains(index) {
            let targets = self
                .selection
                .selected
                .iter()
                .filter_map(|selected| self.primary_pane.entries.get(*selected))
                .map(|entry| {
                    ServiceMenuTarget::new(
                        entry
                            .mime_type
                            .as_deref()
                            .or_else(|| entry.is_dir.then_some("inode/directory")),
                        entry.is_dir,
                    )
                })
                .collect::<Vec<_>>();
            if !targets.is_empty() {
                return targets;
            }
        }
        vec![ServiceMenuTarget::new(
            mime_type.or_else(|| is_dir.then_some("inode/directory")),
            is_dir,
        )]
    }

    fn close_context_menu(&mut self) -> bool {
        if self.context_menu.take().is_none() {
            return false;
        }
        eprintln!("[fika-wgpu] context-menu open=0");
        true
    }

    fn activate_or_close_context_menu_command(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuCommand> {
        let action = self.context_menu_command_at_screen_point(point, size);
        let menu_was_open = self.context_menu.take().is_some();
        if let Some(action) = action {
            self.context_menu_actions += 1;
            eprintln!(
                "[fika-wgpu] context-menu action={} target={} actions={}",
                action.as_str(),
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                self.context_menu_actions
            );
            return Some(action);
        } else if menu_was_open {
            eprintln!("[fika-wgpu] context-menu open=0");
        }
        None
    }

    #[cfg(test)]
    fn activate_or_close_context_menu(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        self.activate_or_close_context_menu_command(point, size)
            .and_then(|command| match command {
                ShellContextMenuCommand::Builtin(action) => Some(action),
                _ => None,
            })
    }

    fn context_menu_command_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuCommand> {
        let menu = self.context_menu.as_ref()?;
        if let Some(submenu) = menu.active_submenu
            && let Some(row) =
                context_submenu_row_at_screen_point(menu, submenu, point, size, self.ui_scale())
        {
            return context_submenu_actions(submenu, menu)
                .get(row)
                .map(|item| item.command.clone());
        }
        let row = context_menu_row_at_screen_point(menu, point, size, self.ui_scale())?;
        context_menu_items(menu)
            .get(row)
            .map(|item| item.command.clone())
    }

    #[cfg(test)]
    fn context_menu_action_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        self.context_menu_command_at_screen_point(point, size)
            .and_then(|command| match command {
                ShellContextMenuCommand::Builtin(action) => Some(action),
                _ => None,
            })
    }

    fn update_context_menu_hover(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(snapshot) = self.context_menu.clone() else {
            return false;
        };
        let scale = self.ui_scale();
        let hovered_submenu_row = snapshot.active_submenu.and_then(|submenu| {
            context_submenu_row_at_screen_point(&snapshot, submenu, point, size, scale)
        });
        let hovered_row = if hovered_submenu_row.is_some()
            && context_menu_submenu_rect(&snapshot, size, scale)
                .is_some_and(|rect| rect.contains(point))
        {
            snapshot.hovered_row
        } else {
            context_menu_row_at_screen_point(&snapshot, point, size, scale)
        };
        let root_items = context_menu_items(&snapshot);
        let hovered_row = hovered_row.filter(|row| *row < root_items.len());
        let active_submenu = hovered_row
            .and_then(|row| root_items.get(row))
            .and_then(|item| item.submenu)
            .or_else(|| {
                hovered_submenu_row
                    .is_some()
                    .then_some(snapshot.active_submenu)
                    .flatten()
            });
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let changed = menu.hovered_row != hovered_row
            || menu.hovered_submenu_row != hovered_submenu_row
            || menu.active_submenu != active_submenu;
        menu.hovered_row = hovered_row;
        menu.hovered_submenu_row = hovered_submenu_row;
        menu.active_submenu = active_submenu;
        changed
    }

    fn log_context_target(&self) {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                index,
                path,
                is_dir,
                selection_count,
                ..
            }) => eprintln!(
                "[fika-wgpu] context-target kind=item index={} dir={} selection={} path={} changes={}",
                index,
                *is_dir as u8,
                selection_count,
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Blank { path, .. }) => eprintln!(
                "[fika-wgpu] context-target kind=blank path={} changes={}",
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Place {
                index,
                label,
                path,
                device,
                network,
                trash,
                root,
                editable,
                ..
            }) => eprintln!(
                "[fika-wgpu] context-target kind=place index={} label={:?} device={} mounted={} ejectable={} poweroff={} network={} trash={} root={} editable={} path={} changes={}",
                index,
                label,
                device.is_some() as u8,
                device.as_ref().is_none_or(|device| device.mounted) as u8,
                device.as_ref().is_some_and(|device| device.ejectable) as u8,
                device.as_ref().is_some_and(|device| device.can_power_off) as u8,
                *network as u8,
                *trash as u8,
                *root as u8,
                *editable as u8,
                path.display(),
                self.context_target_changes
            ),
            None => eprintln!(
                "[fika-wgpu] context-target kind=none changes={}",
                self.context_target_changes
            ),
        }
    }

    fn selected_directory_path(&self) -> Option<PathBuf> {
        self.selection
            .focus_or_first_selected()
            .and_then(|index| self.directory_path_for_index(index))
    }

    fn context_target_directory_path(&self) -> Option<PathBuf> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item { index, is_dir, .. } if *is_dir => {
                self.directory_path_for_index(*index)
            }
            ShellContextTarget::Place { path, device, .. }
                if device.as_ref().is_none_or(|device| device.mounted) =>
            {
                Some(path.clone())
            }
            _ => None,
        }
    }

    fn context_target_open_file_request(&self) -> Option<OpenFileRequest> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item {
                path,
                is_dir: false,
                ..
            } => Some(OpenFileRequest {
                path: path.clone(),
                uri: launch_uri_for_path(path),
            }),
            _ => None,
        }
    }

    fn open_context_target_file_with_default_app(&mut self) -> Result<bool, String> {
        let Some(request) = self.context_target_open_file_request() else {
            return Ok(false);
        };
        launch_file_with_default_app(&request)?;
        self.open_changes += 1;
        eprintln!(
            "[fika-wgpu] open path={} uri={} changes={}",
            request.path.display(),
            request.uri,
            self.open_changes
        );
        Ok(true)
    }

    fn open_open_with_chooser_from_context(&mut self, cache: &MimeApplicationCache) -> bool {
        let chooser = match self.open_with_chooser_for_context(cache) {
            Ok(chooser) => chooser,
            Err(error) => {
                eprintln!("[fika-wgpu] open-with-error {error}");
                return false;
            }
        };
        let changed = self.open_with_chooser.as_ref() != Some(&chooser);
        self.open_with_chooser = Some(chooser);
        self.context_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.trash_conflict_dialog = None;
        self.rubber_band = None;
        if changed {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
        }
        changed
    }

    fn open_with_chooser_for_context(
        &self,
        cache: &MimeApplicationCache,
    ) -> Result<ShellOpenWithChooser, String> {
        let Some(ShellContextTarget::Item {
            index,
            path,
            is_dir: false,
            ..
        }) = self.context_target.as_ref()
        else {
            return Err(format!(
                "target={} is not a file item",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            ));
        };
        if file_ops::is_in_trash_files_dir(path) {
            return Err("Open With is not available inside Trash".to_string());
        }
        let entry = self
            .primary_pane
            .entries
            .get(*index)
            .ok_or_else(|| format!("entry index {index} is out of range"))?;
        let applications =
            open_with_applications_for_mime(cache, entry.mime_type.as_ref().map(|mime| &**mime));
        if applications.is_empty() {
            return Err("no desktop applications found".to_string());
        }
        Ok(ShellOpenWithChooser::new(
            path.clone(),
            entry.mime_type.clone(),
            applications,
        ))
    }

    fn is_open_with_chooser_open(&self) -> bool {
        self.open_with_chooser.is_some()
    }

    fn apply_open_with_command(&mut self, command: OpenWithCommand) -> bool {
        if command == OpenWithCommand::Cancel {
            return self.close_open_with_chooser();
        }
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.apply_command(command) {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else {
            false
        }
    }

    fn select_open_with_filtered_row(&mut self, row: usize) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            return false;
        };
        if chooser.select_filtered_row(row) {
            self.open_with_changes += 1;
            self.log_open_with_chooser_state();
            true
        } else {
            false
        }
    }

    fn open_with_launch_request(
        &self,
        cache: &MimeApplicationCache,
    ) -> Result<OpenWithLaunchRequest, String> {
        let chooser = self
            .open_with_chooser
            .as_ref()
            .ok_or_else(|| "Open With chooser is not open".to_string())?;
        let selected = chooser
            .selected_application()
            .ok_or_else(|| "no application is selected".to_string())?;
        let app = cache
            .application(&selected.id)
            .ok_or_else(|| format!("application not found: {}", selected.id))?;
        let plan = app
            .launch_plan(std::slice::from_ref(&chooser.path))
            .ok_or_else(|| format!("{} did not produce a launch command", app.name))?;
        Ok(OpenWithLaunchRequest {
            path: chooser.path.clone(),
            app_name: plan.app_name.clone(),
            plan,
        })
    }

    fn open_with_launch_request_for_context_application(
        &self,
        cache: &MimeApplicationCache,
        desktop_id: &str,
    ) -> Result<OpenWithLaunchRequest, String> {
        let path = match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                path,
                is_dir: false,
                ..
            }) => path.clone(),
            Some(ShellContextTarget::Blank { path, .. }) => path.clone(),
            _ => return Err("Open With application requires a file or folder target".to_string()),
        };
        let app = cache
            .application(desktop_id)
            .ok_or_else(|| format!("application not found: {desktop_id}"))?;
        let plan = app
            .launch_plan(std::slice::from_ref(&path))
            .ok_or_else(|| format!("{} did not produce a launch command", app.name))?;
        Ok(OpenWithLaunchRequest {
            path,
            app_name: plan.app_name.clone(),
            plan,
        })
    }

    fn service_menu_launch_request(
        &self,
        cache: &MimeApplicationCache,
        action_id: &str,
    ) -> Result<ServiceMenuLaunchRequest, String> {
        let paths = self
            .context_target_service_menu_paths()?
            .ok_or_else(|| "no service menu target paths".to_string())?;
        let plan = cache
            .service_action_launch_plan(action_id, &paths)
            .ok_or_else(|| format!("service action not found or unsupported: {action_id}"))?;
        Ok(ServiceMenuLaunchRequest {
            paths,
            app_name: plan.app_name.clone(),
            plan,
        })
    }

    fn context_target_service_menu_paths(&self) -> Result<Option<Vec<PathBuf>>, String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item { .. }) => self.context_target_item_paths(),
            Some(ShellContextTarget::Blank { path, .. }) => Ok(Some(vec![path.clone()])),
            Some(ShellContextTarget::Place { path, .. }) => Ok(Some(vec![path.clone()])),
            None => Ok(None),
        }
    }

    fn set_open_with_chooser_error(&mut self, error: String) -> bool {
        let Some(chooser) = self.open_with_chooser.as_mut() else {
            eprintln!("[fika-wgpu] open-with-error {error}");
            return false;
        };
        if chooser.error.as_ref() == Some(&error) {
            return false;
        }
        chooser.error = Some(error);
        self.open_with_changes += 1;
        self.log_open_with_chooser_state();
        true
    }

    fn close_open_with_chooser(&mut self) -> bool {
        if self.open_with_chooser.take().is_none() {
            return false;
        }
        self.open_with_changes += 1;
        eprintln!(
            "[fika-wgpu] open-with open=0 changes={}",
            self.open_with_changes
        );
        true
    }

    fn close_open_with_chooser_after_success(&mut self, request: &OpenWithLaunchRequest) -> bool {
        if self.open_with_chooser.take().is_none() {
            return false;
        }
        self.open_with_changes += 1;
        eprintln!(
            "[fika-wgpu] open-with path={} app={:?} changes={}",
            request.path.display(),
            request.app_name,
            self.open_with_changes
        );
        true
    }

    fn open_with_chooser_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> OpenWithChooserClick {
        let Some(chooser) = self.open_with_chooser.as_ref() else {
            return OpenWithChooserClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = open_with_chooser_rect_scaled(chooser, size, scale);
        if !rect.contains(point) {
            return OpenWithChooserClick::Outside;
        }
        if open_with_chooser_cancel_button_rect_scaled(rect, scale).contains(point) {
            return OpenWithChooserClick::Cancel;
        }
        if open_with_chooser_open_button_rect_scaled(rect, scale).contains(point) {
            return OpenWithChooserClick::Open;
        }
        let list = open_with_chooser_list_rect_scaled(rect, chooser, scale);
        if list.contains(point) {
            let visible_row = ((point.y - list.y)
                / scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale))
            .floor() as usize;
            let row = chooser.scroll_row + visible_row;
            if row < chooser.filtered_count() {
                return OpenWithChooserClick::Row(row);
            }
        }
        OpenWithChooserClick::Inside
    }

    fn log_open_with_chooser_state(&self) {
        match self.open_with_chooser.as_ref() {
            Some(chooser) => eprintln!(
                "[fika-wgpu] open-with open=1 path={} mime={} apps={} filtered={} selected={} query={:?} error={:?} changes={}",
                chooser.path.display(),
                chooser.mime_type.as_deref().unwrap_or("unknown"),
                chooser.applications.len(),
                chooser.filtered_count(),
                chooser.selected_index,
                chooser.query,
                chooser.error,
                self.open_with_changes
            ),
            None => eprintln!(
                "[fika-wgpu] open-with open=0 changes={}",
                self.open_with_changes
            ),
        }
    }

    fn context_target_copy_location_request(&self) -> Option<CopyLocationRequest> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item { path, .. } | ShellContextTarget::Place { path, .. } => {
                Some(CopyLocationRequest {
                    path: path.clone(),
                    text: copy_location_text_for_path(path),
                })
            }
            ShellContextTarget::Blank { .. } => None,
        }
    }

    fn context_target_device_action(
        &self,
        action: ShellContextMenuAction,
    ) -> Option<DeviceActionRequest> {
        if !matches!(
            action,
            ShellContextMenuAction::MountDevice
                | ShellContextMenuAction::UnmountDevice
                | ShellContextMenuAction::EjectDevice
                | ShellContextMenuAction::SafelyRemoveDevice
        ) {
            return None;
        }
        let ShellContextTarget::Place {
            label,
            device: Some(device),
            ..
        } = self.context_target.as_ref()?
        else {
            return None;
        };
        Some(DeviceActionRequest {
            id: device.id.clone(),
            label: label.clone(),
            action,
        })
    }

    fn record_copy_location(&mut self, request: &CopyLocationRequest) {
        self.copy_location_changes += 1;
        eprintln!(
            "[fika-wgpu] copy-location path={} text={:?} changes={}",
            request.path.display(),
            request.text,
            self.copy_location_changes
        );
    }

    fn context_target_add_place_candidate(&self) -> Result<(String, PathBuf), String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                path, is_dir: true, ..
            })
            | Some(ShellContextTarget::Blank { path, .. }) => {
                Ok((default_shell_place_label(path), path.clone()))
            }
            Some(ShellContextTarget::Item { is_dir: false, .. }) => {
                Err("only directories can be added to Places".to_string())
            }
            Some(ShellContextTarget::Place { .. }) => {
                Err("place context targets cannot be added to Places".to_string())
            }
            None => Err("no context target to add to Places".to_string()),
        }
    }

    fn add_context_target_to_places(
        &mut self,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let (label, path) = self.context_target_add_place_candidate()?;
        if self.places.iter().any(|place| place.path == path) {
            eprintln!(
                "[fika-wgpu] add-place label={:?} path={} added=0 duplicate=1 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            return Ok(false);
        }
        if !add_user_place_at_path(user_places_path, &path, label.clone())? {
            eprintln!(
                "[fika-wgpu] add-place label={:?} path={} added=0 duplicate=1 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            return Ok(false);
        }

        self.places = rebuild_shell_places_for_user_path(user_places_path);
        save_shell_primary_place_order(user_places_path, &self.places)?;
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        eprintln!(
            "[fika-wgpu] add-place label={:?} path={} added=1 places={} changes={}",
            label,
            path.display(),
            self.places.len(),
            self.places_changes
        );
        Ok(true)
    }

    fn remove_context_place(
        &mut self,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        let Some(ShellContextTarget::Place {
            label,
            path,
            editable,
            ..
        }) = self.context_target.as_ref()
        else {
            return Err("no place context target to remove".to_string());
        };
        if !editable {
            return Err(format!("place {label:?} is not removable"));
        }
        let label = label.clone();
        let path = path.clone();
        if !remove_user_place_at_path(user_places_path, &path)? {
            eprintln!(
                "[fika-wgpu] remove-place label={:?} path={} removed=0 changes={}",
                label,
                path.display(),
                self.places_changes
            );
            return Ok(false);
        }

        self.places = rebuild_shell_places_for_user_path(user_places_path);
        self.clamp_places_scroll(size);
        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.places_changes += 1;
        self.refresh_hover(size);
        eprintln!(
            "[fika-wgpu] remove-place label={:?} path={} removed=1 places={} changes={}",
            label,
            path.display(),
            self.places.len(),
            self.places_changes
        );
        Ok(true)
    }

    fn context_target_file_clipboard_request(
        &self,
        action: ShellContextMenuAction,
    ) -> Result<Option<FileClipboardExportRequest>, String> {
        let role = match action {
            ShellContextMenuAction::Copy => FileClipboardRole::Copy,
            ShellContextMenuAction::Cut => FileClipboardRole::Cut,
            _ => return Ok(None),
        };
        let Some(paths) = self.context_target_item_paths()? else {
            return Ok(None);
        };
        if role == FileClipboardRole::Cut && paths.iter().any(|path| is_network_path(path)) {
            return Err("remote cut is not available yet".to_string());
        }
        let text = encode_file_clipboard_text(role, &paths);
        Ok(Some(FileClipboardExportRequest { role, paths, text }))
    }

    fn record_file_clipboard_export(&mut self, request: &FileClipboardExportRequest) {
        self.file_clipboard_changes += 1;
        eprintln!(
            "[fika-wgpu] clipboard-export role={} paths={} bytes={} changes={}",
            file_clipboard_role_as_str(request.role),
            request.paths.len(),
            request.text.len(),
            self.file_clipboard_changes
        );
    }

    fn context_target_paste_directory(&self) -> Option<PathBuf> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Blank { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    fn paste_clipboard_text_from_context(
        &mut self,
        clipboard_text: &str,
        size: PhysicalSize<u32>,
    ) -> Result<ShellPasteResult, String> {
        let target_dir = self
            .context_target_paste_directory()
            .unwrap_or_else(|| self.primary_pane.path.clone());
        if is_network_path(&target_dir) {
            return Err("remote paste target is not available yet".to_string());
        }
        if clipboard_text.trim().is_empty() {
            return Err("clipboard is empty".to_string());
        }

        let transfer = if let Some(payload) = decode_file_clipboard_text(clipboard_text) {
            if payload.paths.iter().any(|path| is_network_path(path)) {
                return Err("remote paste source is not available yet".to_string());
            }
            let mode = match payload.role {
                FileClipboardRole::Copy => FileTransferMode::Copy,
                FileClipboardRole::Cut => FileTransferMode::Move,
            };
            transfer_paths_result(
                WGPU_SHELL_PANE_ID,
                target_dir.clone(),
                mode,
                payload.paths,
                "Paste",
                payload.role == FileClipboardRole::Cut,
                None,
            )
        } else {
            paste_text_result(WGPU_SHELL_PANE_ID, target_dir.clone(), clipboard_text)
        };

        let result = ShellPasteResult::from_transfer(&transfer);
        self.paste_changes += 1;
        eprintln!(
            "[fika-wgpu] paste mode={} target={} success={} failure={} clear_clipboard={} changes={}",
            result.mode.label(),
            target_dir.display(),
            result.success_count,
            result.failure_count,
            result.clear_clipboard as u8,
            self.paste_changes
        );

        if result.changed() {
            self.context_target = None;
            self.context_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            self.reload_current_path(size)?;
        }
        Ok(result)
    }

    fn context_target_item_paths(&self) -> Result<Option<Vec<PathBuf>>, String> {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                index,
                path,
                selection_count,
                ..
            }) => {
                if *selection_count > 1 && self.selection.contains(*index) {
                    let paths = self
                        .selection
                        .selected
                        .iter()
                        .filter_map(|index| self.entry_path_for_index(*index))
                        .collect::<Vec<_>>();
                    if paths.is_empty() {
                        return Err("selected context target no longer exists".to_string());
                    }
                    Ok(Some(paths))
                } else {
                    Ok(Some(vec![path.clone()]))
                }
            }
            Some(ShellContextTarget::Blank { .. })
            | Some(ShellContextTarget::Place { .. })
            | None => Ok(None),
        }
    }

    fn context_target_trash_paths(&self) -> Result<Vec<PathBuf>, String> {
        self.context_target_item_paths()?
            .ok_or_else(|| "no item context target to move to trash".to_string())
    }

    fn context_target_trash_view_operation(
        &self,
        action: ShellContextMenuAction,
    ) -> Result<(TrashViewOperation, Vec<PathBuf>), String> {
        match action {
            ShellContextMenuAction::RestoreFromTrash => Ok((
                TrashViewOperation::Restore {
                    conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
                },
                self.context_target_trash_view_item_paths()?,
            )),
            ShellContextMenuAction::DeletePermanently => Ok((
                TrashViewOperation::DeletePermanently,
                self.context_target_trash_view_item_paths()?,
            )),
            ShellContextMenuAction::EmptyTrash => {
                if self.context_target_can_empty_trash() {
                    Ok((TrashViewOperation::Empty, Vec::new()))
                } else {
                    Err("Empty Trash is only available from Trash".to_string())
                }
            }
            _ => Err(format!(
                "action {} is not a Trash view action",
                action.as_str()
            )),
        }
    }

    fn context_target_trash_view_item_paths(&self) -> Result<Vec<PathBuf>, String> {
        let paths = self
            .context_target_item_paths()?
            .ok_or_else(|| "no Trash item context target".to_string())?;
        if paths.is_empty() {
            return Err("no Trash item context target".to_string());
        }
        if paths.iter().any(|path| {
            file_ops::is_trash_files_dir(path) || !file_ops::is_in_trash_files_dir(path)
        }) {
            return Err("Trash item action is only available for items inside Trash".to_string());
        }
        Ok(paths)
    }

    fn context_target_can_empty_trash(&self) -> bool {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Blank { path, .. }) => file_ops::is_trash_files_dir(path),
            Some(ShellContextTarget::Place { trash, .. }) => *trash,
            _ => false,
        }
    }

    fn perform_trash_view_context_action(
        &mut self,
        action: ShellContextMenuAction,
        size: PhysicalSize<u32>,
    ) -> Result<TrashViewOperationResult, String> {
        let (operation, paths) = self.context_target_trash_view_operation(action)?;
        let result = trash_view_operation_result(WGPU_SHELL_PANE_ID, operation, paths);
        self.apply_trash_view_result(action.as_str(), &result, size)?;
        Ok(result)
    }

    fn replace_trash_restore_conflicts(
        &mut self,
        size: PhysicalSize<u32>,
    ) -> Result<TrashViewOperationResult, String> {
        let Some(dialog) = self.trash_conflict_dialog.take() else {
            return Err("no Trash restore conflicts to replace".to_string());
        };
        let paths = dialog
            .conflicts
            .into_iter()
            .map(|conflict| conflict.trash_path)
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err("no Trash restore conflicts to replace".to_string());
        }
        let result = trash_view_operation_result(
            WGPU_SHELL_PANE_ID,
            TrashViewOperation::Restore {
                conflict_policy: file_ops::TrashRestoreConflictPolicy::Replace,
            },
            paths,
        );
        self.apply_trash_view_result("replace-trash-conflicts", &result, size)?;
        Ok(result)
    }

    fn apply_trash_view_result(
        &mut self,
        action: &str,
        result: &TrashViewOperationResult,
        size: PhysicalSize<u32>,
    ) -> Result<(), String> {
        self.trash_changes += 1;
        eprintln!(
            "[fika-wgpu] trash-view action={} success={} failure={} conflicts={} changes={}",
            action,
            result.success_count,
            result.failure_count,
            result.restore_conflicts.len(),
            self.trash_changes
        );
        for conflict in &result.restore_conflicts {
            eprintln!(
                "[fika-wgpu] trash-restore-conflict original={} trash={}",
                conflict.original_path.display(),
                conflict.trash_path.display()
            );
        }

        if let Some(dialog) = ShellTrashConflictDialog::new(result.restore_conflicts.clone()) {
            self.trash_conflict_dialog = Some(dialog);
            self.context_target = None;
            self.context_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            eprintln!(
                "[fika-wgpu] trash-conflict open=1 conflicts={} changes={}",
                result.restore_conflicts.len(),
                self.trash_changes
            );
        }

        if result.success_count > 0 {
            self.context_target = None;
            self.context_menu = None;
            self.properties_overlay = None;
            self.create_dialog = None;
            self.rename_dialog = None;
            self.rubber_band = None;
            self.selection.clear();
            self.reload_current_path(size)?;
        }
        Ok(())
    }

    fn move_context_target_to_trash(
        &mut self,
        size: PhysicalSize<u32>,
    ) -> Result<ShellTrashResult, String> {
        let paths = self.context_target_trash_paths()?;
        if paths.iter().any(|path| is_network_path(path)) {
            return Err("remote trash is not available yet".to_string());
        }

        let summary = file_ops::trash_paths(&paths);
        let result = ShellTrashResult {
            success_count: summary.successes.len(),
            failure_count: summary.failures.len(),
            trash_pairs: summary
                .successes
                .iter()
                .map(|record| (record.original_path.clone(), record.trash_path.clone()))
                .collect(),
        };
        self.trash_changes += 1;
        eprintln!(
            "[fika-wgpu] trash paths={} success={} failure={} changes={}",
            paths.len(),
            result.success_count,
            result.failure_count,
            self.trash_changes
        );
        for failure in &summary.failures {
            eprintln!("[fika-wgpu] trash-failure {failure}");
        }

        if !result.changed() {
            return Ok(result);
        }

        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        self.reload_current_path(size)?;
        Ok(result)
    }

    fn is_trash_conflict_dialog_open(&self) -> bool {
        self.trash_conflict_dialog.is_some()
    }

    fn close_trash_conflict_dialog(&mut self) -> bool {
        if self.trash_conflict_dialog.take().is_none() {
            return false;
        }
        self.trash_changes += 1;
        eprintln!(
            "[fika-wgpu] trash-conflict open=0 changes={}",
            self.trash_changes
        );
        true
    }

    fn trash_conflict_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> TrashConflictDialogClick {
        let Some(dialog) = self.trash_conflict_dialog.as_ref() else {
            return TrashConflictDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = trash_conflict_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return TrashConflictDialogClick::Outside;
        }
        if trash_conflict_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return TrashConflictDialogClick::Cancel;
        }
        if trash_conflict_dialog_replace_button_rect_scaled(rect, scale).contains(point) {
            return TrashConflictDialogClick::Replace;
        }
        TrashConflictDialogClick::Inside
    }

    fn is_properties_overlay_open(&self) -> bool {
        self.properties_overlay.is_some()
    }

    fn open_properties_overlay_from_context(&mut self) -> bool {
        let Some(overlay) = self.properties_overlay_for_context_target() else {
            eprintln!("[fika-wgpu] properties-error target=none");
            return false;
        };
        let changed = self.properties_overlay.as_ref() != Some(&overlay);
        self.properties_overlay = Some(overlay);
        if changed {
            self.properties_changes += 1;
            if let Some(overlay) = self.properties_overlay.as_ref() {
                eprintln!(
                    "[fika-wgpu] properties open=1 title={:?} rows={} changes={}",
                    overlay.title,
                    overlay.rows.len(),
                    self.properties_changes
                );
            }
        }
        changed
    }

    fn close_properties_overlay(&mut self) -> bool {
        if self.properties_overlay.take().is_none() {
            return false;
        }
        self.properties_changes += 1;
        eprintln!(
            "[fika-wgpu] properties open=0 changes={}",
            self.properties_changes
        );
        true
    }

    fn close_properties_overlay_if_outside(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(overlay) = self.properties_overlay.as_ref() else {
            return false;
        };
        if properties_overlay_rect_scaled(overlay, size, self.ui_scale()).contains(point) {
            return false;
        }
        self.close_properties_overlay()
    }

    fn is_create_dialog_open(&self) -> bool {
        self.create_dialog.is_some()
    }

    fn open_create_dialog_from_context(&mut self) -> bool {
        self.open_create_dialog_from_context_with_kind(CreateEntryKind::Folder)
    }

    fn open_create_dialog_from_context_with_kind(&mut self, kind: CreateEntryKind) -> bool {
        let Some(ShellContextTarget::Blank { path, .. }) = self.context_target.as_ref() else {
            eprintln!(
                "[fika-wgpu] create-new-error target={}",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            );
            return false;
        };
        let dialog = ShellCreateDialog::new(path.clone(), kind);
        let changed = self.create_dialog.as_ref() != Some(&dialog);
        self.create_dialog = Some(dialog);
        self.properties_overlay = None;
        self.rubber_band = None;
        if changed {
            self.create_changes += 1;
            if let Some(dialog) = self.create_dialog.as_ref() {
                eprintln!(
                    "[fika-wgpu] create-new open=1 kind={} parent={} name={:?} changes={}",
                    dialog.kind.as_str(),
                    dialog.parent.display(),
                    dialog.name,
                    self.create_changes
                );
            }
        }
        changed
    }

    fn apply_create_command(&mut self, command: CreateCommand, _size: PhysicalSize<u32>) -> bool {
        let old_dialog = self.create_dialog.clone();
        match command {
            CreateCommand::Insert(value) => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                }
                dialog.name.push_str(&value);
                dialog.error = None;
            }
            CreateCommand::Backspace => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                } else {
                    dialog.name.pop();
                }
                dialog.error = None;
            }
            CreateCommand::Cancel => {
                return self.close_create_dialog();
            }
            CreateCommand::SetKind(kind) => {
                let Some(dialog) = self.create_dialog.as_mut() else {
                    return false;
                };
                if dialog.kind == kind {
                    return false;
                }
                dialog.kind = kind;
                dialog.name = unique_child_name(&dialog.parent, kind.default_name());
                dialog.error = None;
                dialog.replace_on_insert = true;
            }
            CreateCommand::Commit | CreateCommand::Ignore => return false,
        }

        let changed = old_dialog != self.create_dialog;
        if changed {
            self.create_changes += 1;
            self.log_create_dialog_state();
        }
        changed
    }

    fn create_entry_request(&self) -> Result<CreateEntryRequest, String> {
        let dialog = self
            .create_dialog
            .as_ref()
            .ok_or_else(|| "create dialog is not open".to_string())?;
        let name = dialog.name.trim();
        validate_create_name(name)?;
        let path = dialog.parent.join(name);
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        Ok(CreateEntryRequest {
            parent: dialog.parent.clone(),
            path,
            kind: dialog.kind,
            name: name.to_string(),
        })
    }

    fn set_create_dialog_error(&mut self, error: String) -> bool {
        let Some(dialog) = self.create_dialog.as_mut() else {
            eprintln!("[fika-wgpu] create-new-error {error}");
            return false;
        };
        if dialog.error.as_ref() == Some(&error) {
            return false;
        }
        dialog.error = Some(error);
        dialog.replace_on_insert = false;
        self.create_changes += 1;
        self.log_create_dialog_state();
        true
    }

    fn close_create_dialog(&mut self) -> bool {
        if self.create_dialog.take().is_none() {
            return false;
        }
        self.create_changes += 1;
        eprintln!(
            "[fika-wgpu] create-new open=0 changes={}",
            self.create_changes
        );
        true
    }

    fn close_create_dialog_after_success(&mut self, request: &CreateEntryRequest) -> bool {
        if self.create_dialog.take().is_none() {
            return false;
        }
        self.create_changes += 1;
        eprintln!(
            "[fika-wgpu] create-new created kind={} path={} changes={}",
            request.kind.as_str(),
            request.path.display(),
            self.create_changes
        );
        true
    }

    fn create_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> CreateDialogClick {
        let Some(dialog) = self.create_dialog.as_ref() else {
            return CreateDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = create_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return CreateDialogClick::Outside;
        }
        for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
            if create_kind_button_rect_scaled(rect, kind, scale).contains(point) {
                return CreateDialogClick::Kind(kind);
            }
        }
        if create_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return CreateDialogClick::Cancel;
        }
        if create_dialog_commit_button_rect_scaled(rect, scale).contains(point) {
            return CreateDialogClick::Commit;
        }
        CreateDialogClick::Inside
    }

    fn select_entry_by_name(&mut self, name: &str, size: PhysicalSize<u32>) -> bool {
        let Some(index) = entry_index_by_name(&self.primary_pane.entries, name) else {
            return false;
        };
        if self
            .primary_pane
            .filtered_indexes
            .binary_search(&index)
            .is_err()
        {
            return false;
        }
        let changed = self.selection.apply_navigation(index, false);
        if changed {
            self.selection_changes += 1;
        }
        self.ensure_index_visible(index, size);
        changed
    }

    fn log_create_dialog_state(&self) {
        match self.create_dialog.as_ref() {
            Some(dialog) => eprintln!(
                "[fika-wgpu] create-new open=1 kind={} parent={} name={:?} error={:?} changes={}",
                dialog.kind.as_str(),
                dialog.parent.display(),
                dialog.name,
                dialog.error,
                self.create_changes
            ),
            None => eprintln!(
                "[fika-wgpu] create-new open=0 changes={}",
                self.create_changes
            ),
        }
    }

    fn is_rename_dialog_open(&self) -> bool {
        self.rename_dialog.is_some()
    }

    fn open_rename_dialog_from_context(&mut self) -> bool {
        let Some(ShellContextTarget::Item { path, is_dir, .. }) = self.context_target.as_ref()
        else {
            eprintln!(
                "[fika-wgpu] rename-error target={}",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            );
            return false;
        };
        let Some(dialog) = ShellRenameDialog::new(path.clone(), *is_dir) else {
            eprintln!(
                "[fika-wgpu] rename-error path={} error=no-file-name",
                path.display()
            );
            return false;
        };
        let changed = self.rename_dialog.as_ref() != Some(&dialog);
        self.rename_dialog = Some(dialog);
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rubber_band = None;
        if changed {
            self.rename_changes += 1;
            if let Some(dialog) = self.rename_dialog.as_ref() {
                eprintln!(
                    "[fika-wgpu] rename open=1 source={} name={:?} dir={} changes={}",
                    dialog.source.display(),
                    dialog.name,
                    dialog.is_dir as u8,
                    self.rename_changes
                );
            }
        }
        changed
    }

    fn apply_rename_command(&mut self, command: RenameCommand) -> bool {
        let old_dialog = self.rename_dialog.clone();
        match command {
            RenameCommand::Insert(value) => {
                let Some(dialog) = self.rename_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                }
                dialog.name.push_str(&value);
                dialog.error = None;
            }
            RenameCommand::Backspace => {
                let Some(dialog) = self.rename_dialog.as_mut() else {
                    return false;
                };
                if dialog.replace_on_insert {
                    dialog.name.clear();
                    dialog.replace_on_insert = false;
                } else {
                    dialog.name.pop();
                }
                dialog.error = None;
            }
            RenameCommand::Cancel => {
                return self.close_rename_dialog();
            }
            RenameCommand::Commit | RenameCommand::Ignore => return false,
        }

        let changed = old_dialog != self.rename_dialog;
        if changed {
            self.rename_changes += 1;
            self.log_rename_dialog_state();
        }
        changed
    }

    fn rename_entry_request(&self) -> Result<RenameEntryRequest, String> {
        let dialog = self
            .rename_dialog
            .as_ref()
            .ok_or_else(|| "rename dialog is not open".to_string())?;
        let name = dialog.name.trim();
        validate_create_name(name)?;
        if name == dialog.original_name {
            return Err("name is unchanged".to_string());
        }
        let target = dialog.parent.join(name);
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        Ok(RenameEntryRequest {
            source: dialog.source.clone(),
            target,
            original_name: dialog.original_name.clone(),
            name: name.to_string(),
            is_dir: dialog.is_dir,
        })
    }

    fn set_rename_dialog_error(&mut self, error: String) -> bool {
        let Some(dialog) = self.rename_dialog.as_mut() else {
            eprintln!("[fika-wgpu] rename-error {error}");
            return false;
        };
        if dialog.error.as_ref() == Some(&error) {
            return false;
        }
        dialog.error = Some(error);
        dialog.replace_on_insert = false;
        self.rename_changes += 1;
        self.log_rename_dialog_state();
        true
    }

    fn close_rename_dialog(&mut self) -> bool {
        if self.rename_dialog.take().is_none() {
            return false;
        }
        self.rename_changes += 1;
        eprintln!("[fika-wgpu] rename open=0 changes={}", self.rename_changes);
        true
    }

    fn close_rename_dialog_after_success(&mut self, request: &RenameEntryRequest) -> bool {
        if self.rename_dialog.take().is_none() {
            return false;
        }
        self.rename_changes += 1;
        eprintln!(
            "[fika-wgpu] rename source={} target={} dir={} changes={}",
            request.source.display(),
            request.target.display(),
            request.is_dir as u8,
            self.rename_changes
        );
        true
    }

    fn rename_dialog_click_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> RenameDialogClick {
        let Some(dialog) = self.rename_dialog.as_ref() else {
            return RenameDialogClick::Outside;
        };
        let scale = self.ui_scale();
        let rect = rename_dialog_rect_scaled(dialog, size, scale);
        if !rect.contains(point) {
            return RenameDialogClick::Outside;
        }
        if rename_dialog_cancel_button_rect_scaled(rect, scale).contains(point) {
            return RenameDialogClick::Cancel;
        }
        if rename_dialog_commit_button_rect_scaled(rect, scale).contains(point) {
            return RenameDialogClick::Commit;
        }
        RenameDialogClick::Inside
    }

    fn log_rename_dialog_state(&self) {
        match self.rename_dialog.as_ref() {
            Some(dialog) => eprintln!(
                "[fika-wgpu] rename open=1 source={} name={:?} error={:?} changes={}",
                dialog.source.display(),
                dialog.name,
                dialog.error,
                self.rename_changes
            ),
            None => eprintln!("[fika-wgpu] rename open=0 changes={}", self.rename_changes),
        }
    }

    fn properties_overlay_for_context_target(&self) -> Option<ShellPropertiesOverlay> {
        match self.context_target.as_ref()? {
            ShellContextTarget::Item {
                index,
                path,
                is_dir,
                selection_count,
                ..
            } => {
                let entry = self.primary_pane.entries.get(*index)?;
                let title_name = entry.name.as_ref().to_string();
                let location = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map(|parent| parent.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                let mut rows = vec![
                    property_row("Name", title_name.clone()),
                    property_row("Type", if *is_dir { "Folder" } else { "File" }.to_string()),
                    property_row("Location", location),
                    property_row(
                        "Size",
                        if *is_dir {
                            "-".to_string()
                        } else {
                            format_size(entry.size_bytes)
                        },
                    ),
                    property_row("Modified", format_modified_secs(entry.modified_secs)),
                    property_row("Path", path.display().to_string()),
                ];
                if *selection_count > 1 {
                    rows.push(property_row(
                        "Selection",
                        format!("{selection_count} items"),
                    ));
                }
                if let Some(mime) = entry.mime_type.as_ref() {
                    rows.push(property_row("MIME", mime.to_string()));
                }
                Some(ShellPropertiesOverlay {
                    title: format!("Properties - {title_name}"),
                    rows,
                })
            }
            ShellContextTarget::Blank { path, .. } => Some(ShellPropertiesOverlay {
                title: format!("Properties - {}", path.display()),
                rows: vec![
                    property_row("Name", path_name_or_display(path)),
                    property_row("Type", "Folder".to_string()),
                    property_row("Entries", self.primary_pane.entries.len().to_string()),
                    property_row("Folders", self.primary_pane.dir_count.to_string()),
                    property_row(
                        "Files",
                        self.primary_pane
                            .entries
                            .len()
                            .saturating_sub(self.primary_pane.dir_count)
                            .to_string(),
                    ),
                    property_row("Path", path.display().to_string()),
                ],
            }),
            ShellContextTarget::Place {
                label,
                path,
                group,
                network,
                trash,
                root,
                editable,
                ..
            } => Some(ShellPropertiesOverlay {
                title: format!("Properties - {label}"),
                rows: vec![
                    property_row("Name", label.clone()),
                    property_row("Type", "Place".to_string()),
                    property_row(
                        "Section",
                        if group.is_empty() {
                            "Places".to_string()
                        } else {
                            (*group).to_string()
                        },
                    ),
                    property_row("Path", path.display().to_string()),
                    property_row("Network", yes_no(*network)),
                    property_row("Trash", yes_no(*trash)),
                    property_row("Root", yes_no(*root)),
                    property_row("Editable", yes_no(*editable)),
                ],
            }),
        }
    }

    fn parent_directory_path(&self) -> Option<PathBuf> {
        self.primary_pane
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }

    fn directory_path_for_index(&self, index: usize) -> Option<PathBuf> {
        let entry = self.primary_pane.entries.get(index)?;
        entry
            .is_dir
            .then(|| self.entry_path_for_index(index))
            .flatten()
    }

    fn entry_path_for_index(&self, index: usize) -> Option<PathBuf> {
        self.entry_path_for_pane_view(self.primary_pane_view(), index)
    }

    fn entry_path_for_pane_view(&self, pane: ShellPaneView<'_>, index: usize) -> Option<PathBuf> {
        let entry = pane.entries.get(index)?;
        Some(
            entry
                .target_path
                .clone()
                .unwrap_or_else(|| pane.path.join(entry.name.as_ref())),
        )
    }

    fn pane_state(&self, kind: ShellPaneKind) -> Option<&ShellPaneState> {
        match kind {
            ShellPaneKind::Primary => Some(&self.primary_pane),
            ShellPaneKind::Split => self.split_pane.as_ref(),
        }
    }

    fn pane_state_mut(&mut self, kind: ShellPaneKind) -> Option<&mut ShellPaneState> {
        match kind {
            ShellPaneKind::Primary => Some(&mut self.primary_pane),
            ShellPaneKind::Split => self.split_pane.as_mut(),
        }
    }

    fn directory_activation_for_primary_press(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        now: Instant,
    ) -> Option<PathBuf> {
        let Some(index) = self.hit_test_screen_point(point, size) else {
            self.last_primary_click = None;
            return None;
        };

        let double_click = self.last_primary_click.is_some_and(|click| {
            click.index == index
                && now.duration_since(click.time) <= DOUBLE_CLICK_MAX_INTERVAL
                && point_distance(click.point, point) <= DOUBLE_CLICK_MAX_DISTANCE
        });
        self.last_primary_click = Some(PrimaryClick {
            index,
            point,
            time: now,
        });

        double_click
            .then(|| self.directory_path_for_index(index))
            .flatten()
    }

    fn primary_pane_view(&self) -> ShellPaneView<'_> {
        ShellPaneView::from_state(&self.primary_pane)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_view(&self, kind: ShellPaneKind) -> Option<ShellPaneView<'_>> {
        self.pane_state(kind).map(ShellPaneView::from_state)
    }

    fn primary_pane_geometry(&self, size: PhysicalSize<u32>) -> ShellPaneGeometry {
        let pane = self.pane_rect(size);
        ShellPaneGeometry {
            kind: ShellPaneKind::Primary,
            pane,
            top_bar: ViewRect {
                x: pane.x,
                y: pane.y,
                width: pane.width,
                height: self.top_bar_height(),
            },
            content: self.content_screen_rect(size),
            status_bar: self.status_bar_rect(size),
        }
    }

    fn split_pane_geometry(&self, size: PhysicalSize<u32>) -> Option<ShellPaneGeometry> {
        let split_pane = self.split_pane.as_ref()?;
        let metrics = self.split_pane_metrics(size)?;
        let pane = metrics.right_pane;
        let status_height = self.status_bar_height().min(pane.height);
        let status_bar = ViewRect {
            x: pane.x,
            y: pane.bottom() - status_height,
            width: pane.width,
            height: status_height,
        };
        let content_y = pane.y
            + self.top_bar_height()
            + if split_pane.view_mode == ShellViewMode::Details {
                self.details_header_height()
            } else {
                0.0
            };
        Some(ShellPaneGeometry {
            kind: ShellPaneKind::Split,
            pane,
            top_bar: ViewRect {
                x: pane.x,
                y: pane.y,
                width: pane.width,
                height: self.top_bar_height(),
            },
            content: ViewRect {
                x: pane.x,
                y: content_y,
                width: pane.width,
                height: (status_bar.y - content_y).max(1.0),
            },
            status_bar,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_geometries(&self, size: PhysicalSize<u32>) -> Vec<ShellPaneGeometry> {
        let mut geometries = Vec::with_capacity(2);
        geometries.push(self.primary_pane_geometry(size));
        if let Some(geometry) = self.split_pane_geometry(size) {
            geometries.push(geometry);
        }
        geometries
    }

    fn primary_pane_projection(&self, size: PhysicalSize<u32>) -> ShellPaneProjection<'_> {
        self.pane_projection_from_geometry(
            self.primary_pane_view(),
            self.primary_pane_geometry(size),
            &self.primary_visible_slots,
        )
    }

    fn pane_projection(
        &self,
        kind: ShellPaneKind,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneProjection<'_>> {
        let view = self.pane_view(kind)?;
        let geometry = match kind {
            ShellPaneKind::Primary => self.primary_pane_geometry(size),
            ShellPaneKind::Split => self.split_pane_geometry(size)?,
        };
        let slots = match kind {
            ShellPaneKind::Primary => &self.primary_visible_slots,
            ShellPaneKind::Split => &self.split_visible_slots,
        };
        Some(self.pane_projection_from_geometry(view, geometry, slots))
    }

    fn pane_projection_from_geometry<'a>(
        &self,
        view: ShellPaneView<'a>,
        geometry: ShellPaneGeometry,
        slots: &ShellVisibleItemSlotPool,
    ) -> ShellPaneProjection<'a> {
        let layout = self.pane_layout(view, geometry.content.width, geometry.content.height);
        let visible_items = layout
            .visible_items()
            .into_iter()
            .map(|layout| {
                let slot_id = view
                    .filtered_indexes
                    .get(layout.model_index)
                    .and_then(|entry_index| self.entry_path_for_pane_view(view, *entry_index))
                    .and_then(|path| slots.slot_for_path(&path))
                    .unwrap_or_default();
                ShellPaneVisibleItem { layout, slot_id }
            })
            .collect();
        let scroll_metrics = ShellPaneScrollMetrics::new(layout.content_size(), geometry.content);
        ShellPaneProjection {
            view,
            geometry,
            visible_items,
            scroll_metrics,
        }
    }

    fn primary_pane_scroll_metrics(&self, size: PhysicalSize<u32>) -> ShellPaneScrollMetrics {
        let geometry = self.primary_pane_geometry(size);
        let layout = self.pane_layout(
            self.primary_pane_view(),
            geometry.content.width,
            geometry.content.height,
        );
        ShellPaneScrollMetrics::new(layout.content_size(), geometry.content)
    }

    fn visible_paths_for_pane_projection(
        &self,
        view: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
    ) -> Vec<PathBuf> {
        let layout = self.pane_layout(view, geometry.content.width, geometry.content.height);
        layout
            .visible_items()
            .into_iter()
            .filter_map(|item| {
                view.filtered_indexes
                    .get(item.model_index)
                    .and_then(|entry_index| self.entry_path_for_pane_view(view, *entry_index))
            })
            .collect()
    }

    fn update_visible_slot_pools(&mut self, size: PhysicalSize<u32>) -> ShellVisibleItemSlotStats {
        let primary_paths = self.visible_paths_for_pane_projection(
            self.primary_pane_view(),
            self.primary_pane_geometry(size),
        );
        let primary_stats = self
            .primary_visible_slots
            .update_visible_items(primary_paths);
        let split_stats = if let Some(split_view) = self.pane_view(ShellPaneKind::Split)
            && let Some(split_geometry) = self.split_pane_geometry(size)
        {
            let split_paths = self.visible_paths_for_pane_projection(split_view, split_geometry);
            self.split_visible_slots.update_visible_items(split_paths)
        } else {
            self.split_visible_slots.clear();
            ShellVisibleItemSlotStats::default()
        };
        let stats = primary_stats.merged(split_stats);
        self.visible_slot_stats = stats;
        stats
    }

    fn layout(&self, size: PhysicalSize<u32>) -> ShellLayout {
        self.pane_layout(
            self.primary_pane_view(),
            self.content_width(size),
            self.viewport_height(size),
        )
    }

    fn pane_layout(
        &self,
        pane: ShellPaneView<'_>,
        content_width: f32,
        viewport_height: f32,
    ) -> ShellLayout {
        let item_count = pane.filtered_entry_count();
        match pane.view_mode {
            ShellViewMode::Icons => {
                let mut options = self.icons_options_for_viewport(content_width, viewport_height);
                options.scroll_x = pane.scroll_x;
                options.scroll_y = pane.scroll_y;
                ShellLayout::Icons(IconsLayout::new(item_count, options))
            }
            ShellViewMode::Compact => {
                let mut options = self.compact_options_for_viewport(content_width, viewport_height);
                options.scroll_x = pane.scroll_x;
                ShellLayout::Compact(self.pane_compact_layout(pane, options))
            }
            ShellViewMode::Details => ShellLayout::Details(DetailsLayout::new(
                item_count,
                content_width,
                viewport_height,
                pane.scroll_y,
                self.details_row_height(),
                self.details_icon_size(),
                self.ui_scale(),
                self.details_name_width(),
                self.details_size_width(),
                self.details_modified_width(),
                self.text_line_height(),
            )),
        }
    }

    fn pane_compact_layout(
        &self,
        pane: ShellPaneView<'_>,
        options: CompactLayoutOptions,
    ) -> ShellCompactLayout {
        let item_count = pane.filtered_entry_count();
        let rows_per_column = CompactLayout::rows_per_column_for_options(options);
        let column_count = item_count.div_ceil(rows_per_column);
        let mut text_widths = Vec::with_capacity(item_count);
        let mut column_widths = vec![options.item_width; column_count];
        for layout_index in 0..item_count {
            let Some(entry_index) = pane.filtered_indexes.get(layout_index).copied() else {
                text_widths.push(0.0);
                continue;
            };
            let Some(entry) = pane.entries.get(entry_index) else {
                text_widths.push(0.0);
                continue;
            };
            let text_width = compact_entry_text_width(entry, self.ui_scale() * self.zoom_factor());
            text_widths.push(text_width);
            let column = layout_index / rows_per_column;
            if let Some(width) = column_widths.get_mut(column) {
                *width = width.max(required_compact_item_width(options, text_width));
            }
        }
        let layout = CompactLayout::new_with_column_widths(item_count, options, column_widths);
        ShellCompactLayout::new(layout, text_widths)
    }

    #[cfg(test)]
    fn icons_options(&self, size: PhysicalSize<u32>) -> IconsLayoutOptions {
        let mut options =
            self.icons_options_for_viewport(self.content_width(size), self.viewport_height(size));
        options.scroll_x = self.primary_pane.scroll_x;
        options.scroll_y = self.primary_pane.scroll_y;
        options
    }

    fn icons_options_for_viewport(
        &self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> IconsLayoutOptions {
        IconsLayoutOptions {
            viewport_width,
            viewport_height,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding: self.zoomed_metric(8.0, 6.0, 14.0),
            gap: self.zoomed_metric(12.0, 8.0, 22.0),
            item_width: self.zoomed_metric(ICONS_ITEM_WIDTH, 82.0, 188.0),
            item_height: self.zoomed_metric(ICONS_ITEM_HEIGHT, 76.0, 172.0),
            icon_size: self.zoomed_metric(ICONS_ICON_SIZE, 28.0, 92.0),
            text_height: self.zoomed_metric(TEXT_LINE_HEIGHT, 16.0, 30.0),
        }
    }

    #[cfg(test)]
    fn compact_options(&self, size: PhysicalSize<u32>) -> CompactLayoutOptions {
        let mut options =
            self.compact_options_for_viewport(self.content_width(size), self.viewport_height(size));
        options.scroll_x = self.primary_pane.scroll_x;
        options
    }

    fn compact_options_for_viewport(
        &self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> CompactLayoutOptions {
        let padding = self.zoomed_metric(6.0, 4.0, 10.0);
        let side_padding = self.zoomed_metric(8.0, 6.0, 14.0);
        let gap = self.zoomed_metric(8.0, 6.0, 14.0);
        let text_gap = self.zoomed_metric(8.0, 6.0, 14.0);
        let icon_size = self.zoomed_metric(COMPACT_ICON_SIZE, 20.0, 56.0);
        let min_text_width = self.zoomed_metric(COMPACT_MIN_TEXT_WIDTH, 16.0, 48.0);
        CompactLayoutOptions {
            viewport_width,
            viewport_height,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding,
            side_padding,
            gap,
            text_gap,
            item_width: (padding * 2.0 + icon_size + text_gap + min_text_width).round(),
            item_height: self.zoomed_metric(COMPACT_ITEM_HEIGHT, 34.0, 72.0),
            icon_size,
            text_height: self.zoomed_metric(TEXT_LINE_HEIGHT, 16.0, 26.0),
        }
    }

    fn zoom_factor(&self) -> f32 {
        (1.0 + self.zoom_step as f32 * ZOOM_STEP_SCALE).clamp(0.64, 1.64)
    }

    fn zoom_percent(&self) -> i32 {
        (self.zoom_factor() * 100.0).round() as i32
    }

    fn details_row_height(&self) -> f32 {
        self.zoomed_metric(DETAILS_ROW_HEIGHT, 22.0, 44.0)
    }

    fn details_icon_size(&self) -> f32 {
        self.zoomed_metric(DETAILS_ICON_SIZE, 16.0, 34.0)
    }

    fn build_frame(
        &mut self,
        size: PhysicalSize<u32>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        overlay_text: &mut TextFrameBuilder<'_>,
    ) -> SceneFrame {
        let layout_start = Instant::now();
        self.update_visible_slot_pools(size);
        let mut vertices = Vec::with_capacity(64);
        let mut overlay_vertices = Vec::with_capacity(32);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let pane = self.pane_rect(size);
        let top_bar_height = self.top_bar_height();
        let status_bar = self.status_bar_rect(size);
        let primary_projection = self.primary_pane_projection(size);
        let content_clip = primary_projection.geometry.content;

        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            view_mode_surface_color(self.primary_pane.view_mode),
            size,
        );
        self.push_app_toolbar(&mut vertices, size);
        push_rect(&mut vertices, pane, [1.000, 1.000, 1.000, 1.0], size);
        push_rect(
            &mut vertices,
            ViewRect {
                x: pane.x,
                y: pane.y,
                width: pane.width,
                height: top_bar_height,
            },
            chrome_color(),
            size,
        );
        if let Some(path_rect) = self.path_bar_rect(size) {
            let location_active = self.is_location_editing();
            let location_clip = ViewRect {
                x: pane.x,
                y: pane.y,
                width: self.pane_width(size),
                height: top_bar_height,
            };
            let path_label = self
                .location_draft
                .as_ref()
                .map(|draft| draft.value.clone())
                .unwrap_or_else(|| self.primary_pane.path.display().to_string());
            let path_cursor = self.location_draft.as_ref().map(|draft| draft.cursor);
            self.push_location_bar(
                &mut vertices,
                text,
                size,
                path_rect,
                location_clip,
                &path_label,
                location_active,
                path_cursor,
            );
        }
        self.push_places_sidebar(&mut vertices, text, size);
        let pane_body = self.pane_body_rect(size);
        push_rect(
            &mut vertices,
            pane_body,
            view_mode_content_color(self.primary_pane.view_mode),
            size,
        );
        self.push_filter_bar(&mut vertices, text, size);
        if self.primary_pane.view_mode == ShellViewMode::Details {
            self.push_details_header(&mut vertices, text, size);
        }

        let content_size = primary_projection.scroll_metrics.content_size;
        let first_item_rect = primary_projection
            .visible_items
            .first()
            .map(|item| item.layout.item_rect);
        let visible_items = primary_projection.visible_items.len();
        let thumbnail_candidates = self
            .thumbnail_candidate_count_for_projection(&primary_projection)
            + self
                .pane_projection(ShellPaneKind::Split, size)
                .as_ref()
                .map(|projection| self.thumbnail_candidate_count_for_projection(projection))
                .unwrap_or(0);
        for item in primary_projection.visible_items.iter().copied() {
            self.push_pane_item(&mut vertices, text, icons, &primary_projection, item, size);
        }
        self.push_rubber_band(&mut vertices, content_clip, size);
        let content_scrollbar_visible =
            self.push_content_scrollbar_for_projection(&mut vertices, &primary_projection, size);
        self.push_status_bar(&mut vertices, text, size, visible_items, status_bar);
        self.push_pane_borders(&mut vertices, size);
        self.push_split_pane(&mut vertices, text, icons, size);
        self.queue_thumbnail_read_ahead_for_projection(&primary_projection, icons);
        if let Some(split_projection) = self.pane_projection(ShellPaneKind::Split, size) {
            self.queue_thumbnail_read_ahead_for_projection(&split_projection, icons);
        }
        self.push_context_menu_overlay(&mut overlay_vertices, overlay_text, icons, size);
        self.push_properties_overlay(&mut overlay_vertices, overlay_text, size);
        self.push_create_dialog_overlay(&mut overlay_vertices, overlay_text, size);
        self.push_rename_dialog_overlay(&mut overlay_vertices, overlay_text, size);
        self.push_open_with_chooser_overlay(&mut overlay_vertices, overlay_text, size);
        self.push_trash_conflict_dialog_overlay(&mut overlay_vertices, overlay_text, size);

        SceneFrame {
            layout_us: layout_start.elapsed().as_micros(),
            visible_items,
            thumbnail_candidates,
            quad_count: (vertices.len() + overlay_vertices.len()) / 6,
            content_size,
            content_scrollbar_visible,
            first_item_rect,
            vertices,
            overlay_vertices,
            text_stats: TextFrameStats::default(),
            icon_stats: IconFrameStats::default(),
        }
    }

    fn push_location_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        rect: ViewRect,
        clip: ViewRect,
        label: &str,
        active: bool,
        cursor: Option<usize>,
    ) {
        let radius = self.scale_metric(7.0);
        let border_color = if active {
            [0.184, 0.435, 0.929, 1.0]
        } else {
            [0.784, 0.808, 0.839, 1.0]
        };
        push_clipped_rounded_rect(vertices, rect, clip, radius, border_color, size);
        if let Some(inner) = inset_rect(rect, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner,
                clip,
                (radius - self.scale_metric(1.0)).max(1.0),
                [1.000, 1.000, 1.000, 1.0],
                size,
            );
        }

        let icon_size = self
            .scale_metric(18.0)
            .min((rect.height - self.scale_metric(8.0)).max(1.0));
        let icon_rect = ViewRect {
            x: rect.x + self.scale_metric(8.0),
            y: rect.y + (rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_location_bar_icon(vertices, icon_rect, clip, active, self.ui_scale(), size);
        let separator_x = icon_rect.right() + self.scale_metric(8.0);
        push_clipped_rect(
            vertices,
            ViewRect {
                x: separator_x,
                y: rect.y + self.scale_metric(7.0),
                width: self.scale_metric(1.0),
                height: (rect.height - self.scale_metric(14.0)).max(1.0),
            },
            clip,
            [0.835, 0.851, 0.875, 1.0],
            size,
        );
        let text_x = separator_x + self.scale_metric(9.0);
        let text_rect = ViewRect {
            x: text_x,
            y: rect.y + (rect.height - self.text_line_height()) / 2.0,
            width: (rect.right() - text_x - self.scale_metric(8.0)).max(1.0),
            height: self.text_line_height(),
        };
        let cursor_x = cursor.map(|cursor| {
            text.measure_label_cursor_x(
                label,
                text_rect,
                cursor,
                LabelAlignment::Start,
                LabelWrap::None,
            )
        });
        text.push_label_aligned_no_wrap(
            label,
            text_rect,
            clip,
            TextColor::rgb(36, 41, 47),
            LabelAlignment::Start,
        );
        if active {
            let caret_width = self.scale_metric(1.25);
            let caret_height = self
                .scale_metric(17.0)
                .min((rect.height - self.scale_metric(10.0)).max(1.0));
            let caret_x = (text_rect.x + cursor_x.unwrap_or(0.0)).clamp(
                text_rect.x,
                (text_rect.right() - caret_width).max(text_rect.x),
            );
            push_clipped_rounded_rect(
                vertices,
                ViewRect {
                    x: caret_x,
                    y: rect.y + (rect.height - caret_height) / 2.0,
                    width: caret_width,
                    height: caret_height,
                },
                clip,
                caret_width / 2.0,
                [0.122, 0.310, 0.749, 1.0],
                size,
            );
        }
    }

    fn push_pane_item(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        item: ShellPaneVisibleItem,
        size: PhysicalSize<u32>,
    ) {
        let layout = item.layout;
        let _slot_id = item.slot_id;
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(layout.model_index)
            .copied()
        else {
            return;
        };
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return;
        };
        let content_clip = projection.geometry.content;
        let item_rect = pane_content_rect_to_screen(layout.item_rect, projection);
        let visual_rect = pane_content_rect_to_screen(layout.visual_rect, projection);
        let icon_rect = pane_content_rect_to_screen(layout.icon_rect, projection);
        let text_rect = pane_content_rect_to_screen(layout.text_rect, projection);
        let primary = projection.geometry.kind == ShellPaneKind::Primary;
        let selected = primary && self.selection.contains(entry_index);
        let hovered = primary && self.hovered_index == Some(entry_index);

        if projection.view.view_mode == ShellViewMode::Details {
            push_clipped_rect(
                vertices,
                item_rect,
                content_clip,
                details_row_background_color(selected, hovered, entry_index),
                size,
            );
            if selected {
                push_clipped_rect_outline(
                    vertices,
                    item_rect,
                    content_clip,
                    1.0,
                    [0.38, 0.64, 0.92, 0.92],
                    size,
                );
            }
        } else if selected {
            let radius = self.scale_metric(7.0);
            push_clipped_rounded_rect(
                vertices,
                visual_rect,
                content_clip,
                radius,
                [0.38, 0.64, 0.92, 0.95],
                size,
            );
            if let Some(inner) = inset_rect(visual_rect, self.scale_metric(1.0)) {
                push_clipped_rounded_rect(
                    vertices,
                    inner,
                    content_clip,
                    (radius - self.scale_metric(1.0)).max(1.0),
                    item_background_color(selected, hovered),
                    size,
                );
            }
        } else if hovered {
            push_clipped_rounded_rect(
                vertices,
                visual_rect,
                content_clip,
                self.scale_metric(7.0),
                item_background_color(selected, hovered),
                size,
            );
        }

        if !icons.push_thumbnail_or_icon(projection.view.path, entry, icon_rect, content_clip) {
            push_fallback_icon(vertices, entry, icon_rect, content_clip, size);
        }

        let text_color = if selected {
            TextColor::rgb(15, 23, 42)
        } else if projection.view.view_mode != ShellViewMode::Details && entry.is_dir {
            TextColor::rgb(31, 79, 191)
        } else {
            TextColor::rgb(36, 41, 47)
        };
        if projection.view.view_mode == ShellViewMode::Compact {
            text.push_label_aligned(
                entry.name.as_ref(),
                text_rect,
                content_clip,
                text_color,
                LabelAlignment::Start,
            );
        } else {
            text.push_label(entry.name.as_ref(), text_rect, content_clip, text_color);
        }

        if projection.view.view_mode == ShellViewMode::Details {
            let text_height = self.text_line_height();
            let metadata_y = item_rect.y + (item_rect.height - text_height).max(0.0) / 2.0;
            text.push_label(
                &details_size_label(entry),
                ViewRect {
                    x: content_clip.x + self.details_name_width() + self.scale_metric(8.0)
                        - projection.view.scroll_x,
                    y: metadata_y,
                    width: self.details_size_width() - self.scale_metric(16.0),
                    height: text_height,
                },
                content_clip,
                TextColor::rgb(89, 99, 110),
            );
            text.push_label(
                &format_modified_secs(entry.modified_secs),
                ViewRect {
                    x: content_clip.x
                        + self.details_name_width()
                        + self.details_size_width()
                        + self.scale_metric(8.0)
                        - projection.view.scroll_x,
                    y: metadata_y,
                    width: self.details_modified_width() - self.scale_metric(16.0),
                    height: text_height,
                },
                content_clip,
                TextColor::rgb(89, 99, 110),
            );
        }

        if selected || hovered {
            let marker_size = self.scale_metric(5.0);
            let index_marker = ViewRect {
                x: item_rect.x + self.scale_metric(7.0),
                y: item_rect.y + self.scale_metric(7.0),
                width: marker_size,
                height: marker_size,
            };
            push_clipped_rect(
                vertices,
                index_marker,
                content_clip,
                if entry.is_dir {
                    [0.55, 0.80, 0.54, 1.0]
                } else {
                    [0.42, 0.62, 0.84, 1.0]
                },
                size,
            );
        }
    }

    fn thumbnail_candidate_count_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> usize {
        projection
            .visible_items
            .iter()
            .filter(|item| {
                projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                    .and_then(|entry_index| {
                        self.thumbnail_candidate_for_pane_entry(projection.view, entry_index)
                    })
                    .is_some()
            })
            .count()
    }

    fn queue_thumbnail_read_ahead_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
        icons: &mut IconFrameBuilder<'_>,
    ) {
        let Some(visible_range) = visible_layout_range_for_projection(projection) else {
            return;
        };
        let size_px = self.thumbnail_read_ahead_size_px(projection.view.view_mode);
        if size_px < 32 {
            return;
        }
        let item_count = projection.view.filtered_entry_count();
        for layout_index in shell_dolphin_read_ahead_indexes(
            visible_range,
            item_count,
            projection.visible_items.len(),
        )
        .into_iter()
        .take(THUMBNAIL_READ_AHEAD_QUEUE_BUDGET_PER_FRAME)
        {
            let Some(entry_index) = projection.view.filtered_indexes.get(layout_index).copied()
            else {
                continue;
            };
            if let Some(candidate) =
                self.thumbnail_candidate_for_pane_entry(projection.view, entry_index)
            {
                icons.queue_thumbnail_read_ahead(candidate, size_px);
            }
        }
    }

    fn thumbnail_read_ahead_size_px(&self, view_mode: ShellViewMode) -> u16 {
        let icon_size = match view_mode {
            ShellViewMode::Icons => self.zoomed_metric(ICONS_ICON_SIZE, 28.0, 92.0),
            ShellViewMode::Compact => self.zoomed_metric(COMPACT_ICON_SIZE, 20.0, 56.0),
            ShellViewMode::Details => self.details_icon_size(),
        };
        icon_cache_size(icon_size)
    }

    fn thumbnail_candidate_for_pane_entry(
        &self,
        view: ShellPaneView<'_>,
        entry_index: usize,
    ) -> Option<ShellThumbnailCandidate> {
        let entry = view.entries.get(entry_index)?;
        if entry.is_dir || !entry.metadata_complete {
            return None;
        }
        let modified_secs = entry.modified_secs?;
        let path = self.entry_path_for_pane_view(view, entry_index)?;
        if is_network_path(&path)
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.size_bytes,
                entry.mime_type.as_deref(),
                entry.mime_magic_checked,
            )
            || !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref())
        {
            return None;
        }
        Some(ShellThumbnailCandidate {
            path,
            modified_secs,
            mime_type: entry
                .mime_type
                .as_deref()
                .map(std::borrow::ToOwned::to_owned),
        })
    }

    fn push_split_pane(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(projection) = self.pane_projection(ShellPaneKind::Split, size) else {
            return;
        };
        let Some(metrics) = self.split_pane_metrics(size) else {
            return;
        };
        let split_view = projection.view;
        let pane = projection.geometry.pane;
        let status_bar = projection.geometry.status_bar;

        push_rect(vertices, metrics.divider, [0.784, 0.808, 0.839, 1.0], size);
        push_rect(vertices, pane, [1.000, 1.000, 1.000, 1.0], size);
        push_rect(vertices, projection.geometry.top_bar, chrome_color(), size);
        let margin = self.scale_metric(8.0);
        let path_rect = ViewRect {
            x: pane.x + margin,
            y: pane.y + self.scale_metric(4.0),
            width: (pane.width - margin * 2.0).max(1.0),
            height: self.scale_metric(28.0),
        };
        self.push_location_bar(
            vertices,
            text,
            size,
            path_rect,
            projection.geometry.top_bar,
            &split_view.path.display().to_string(),
            false,
            None,
        );
        push_rect(
            vertices,
            ViewRect {
                x: pane.x,
                y: projection.geometry.top_bar.bottom(),
                width: pane.width,
                height: (status_bar.y - projection.geometry.top_bar.bottom()).max(1.0),
            },
            view_mode_content_color(split_view.view_mode),
            size,
        );
        if split_view.view_mode == ShellViewMode::Details {
            self.push_split_details_header(vertices, text, pane, size);
        }

        for item in projection.visible_items.iter().copied() {
            self.push_pane_item(vertices, text, icons, &projection, item, size);
        }
        let _ = self.push_content_scrollbar_for_projection(vertices, &projection, size);
        self.push_split_status_bar(
            vertices,
            text,
            split_view,
            status_bar,
            projection.visible_items.len(),
            size,
        );
    }

    fn push_split_details_header(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        pane: ViewRect,
        size: PhysicalSize<u32>,
    ) {
        let header = ViewRect {
            x: pane.x,
            y: pane.y + self.top_bar_height(),
            width: pane.width,
            height: self.details_header_height(),
        };
        push_rect(vertices, header, [0.953, 0.961, 0.973, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: header.x,
                y: header.bottom() - 1.0,
                width: header.width,
                height: 1.0,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );
        for (label, x, width) in [
            (
                "Name",
                self.scale_metric(34.0),
                self.details_name_width() - self.scale_metric(42.0),
            ),
            (
                "Size",
                self.details_name_width() + self.scale_metric(8.0),
                self.details_size_width() - self.scale_metric(16.0),
            ),
            (
                "Modified",
                self.details_name_width() + self.details_size_width() + self.scale_metric(8.0),
                self.details_modified_width() - self.scale_metric(16.0),
            ),
        ] {
            text.push_label(
                label,
                ViewRect {
                    x: header.x + x,
                    y: header.y + self.scale_metric(6.0),
                    width: width.max(1.0),
                    height: self.text_line_height(),
                },
                header,
                TextColor::rgb(89, 99, 110),
            );
        }
    }

    fn push_split_status_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        pane: ShellPaneView<'_>,
        rect: ViewRect,
        visible_items: usize,
        size: PhysicalSize<u32>,
    ) {
        push_rect(vertices, rect, [1.000, 1.000, 1.000, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: 1.0,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );
        let status = format!(
            "{} entries ({} dirs, {} files) | {} visible | {}",
            pane.entries.len(),
            pane.dir_count,
            pane.entries.len().saturating_sub(pane.dir_count),
            visible_items,
            pane.view_mode.label()
        );
        text.push_label(
            &status,
            ViewRect {
                x: rect.x + self.scale_metric(12.0),
                y: rect.y + self.scale_metric(5.0),
                width: (rect.width - self.scale_metric(24.0)).max(1.0),
                height: self.text_line_height(),
            },
            rect,
            TextColor::rgb(89, 99, 110),
        );
    }

    fn push_app_toolbar(&self, vertices: &mut Vec<QuadVertex>, size: PhysicalSize<u32>) {
        let toolbar = self.app_toolbar_rect(size);
        push_rect(
            vertices,
            toolbar,
            view_mode_surface_color(self.primary_pane.view_mode),
            size,
        );

        let button = self.places_toggle_rect(size);
        let border_color = if self.places_visible {
            [0.184, 0.435, 0.929, 1.0]
        } else {
            [0.694, 0.729, 0.776, 1.0]
        };
        let fill_color = if self.places_visible {
            [0.918, 0.945, 1.000, 1.0]
        } else {
            [0.984, 0.986, 0.990, 1.0]
        };
        let icon_color = if self.places_visible {
            [0.122, 0.310, 0.749, 1.0]
        } else {
            [0.420, 0.466, 0.545, 1.0]
        };
        push_clipped_rounded_rect(
            vertices,
            button,
            toolbar,
            self.scale_metric(6.0),
            border_color,
            size,
        );
        if let Some(inner) = inset_rect(button, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner,
                toolbar,
                self.scale_metric(5.0),
                fill_color,
                size,
            );
        }

        let icon = ViewRect {
            x: button.x + (button.width - self.scale_metric(18.0)) / 2.0,
            y: button.y + (button.height - self.scale_metric(18.0)) / 2.0,
            width: self.scale_metric(18.0),
            height: self.scale_metric(18.0),
        };
        let rail = self.scale_metric(2.0);
        push_clipped_rect(
            vertices,
            ViewRect {
                x: icon.x + self.scale_metric(2.0),
                y: icon.y + self.scale_metric(2.0),
                width: rail,
                height: icon.height - self.scale_metric(4.0),
            },
            toolbar,
            icon_color,
            size,
        );
        push_clipped_rect_outline(
            vertices,
            ViewRect {
                x: icon.x + self.scale_metric(1.0),
                y: icon.y + self.scale_metric(3.0),
                width: icon.width - self.scale_metric(2.0),
                height: icon.height - self.scale_metric(6.0),
            },
            toolbar,
            self.scale_metric(1.0),
            icon_color,
            size,
        );
    }

    fn push_places_sidebar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return;
        }
        let panel = self.places_panel_rect(size);
        let panel_radius = self.scale_metric(12.0);
        push_clipped_rounded_rect(
            vertices,
            panel,
            sidebar,
            panel_radius,
            [0.784, 0.808, 0.839, 1.0],
            size,
        );
        if let Some(inner_panel) = inset_rect(panel, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner_panel,
                sidebar,
                (panel_radius - self.scale_metric(1.0)).max(1.0),
                sidebar_color(),
                size,
            );
        }
        push_rect(
            vertices,
            ViewRect {
                x: sidebar.right(),
                y: sidebar.y,
                width: self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH),
                height: sidebar.height,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );

        let active_place = active_shell_place_index(&self.places, &self.primary_pane.path);
        let top_padding = self.scale_metric(PLACES_SIDEBAR_TOP_PADDING);
        let title_height = self.scale_metric(PLACES_TITLE_HEIGHT);
        let padding_x = self.scale_metric(PLACES_SIDEBAR_PADDING_X);
        let section_height = self.scale_metric(PLACES_SECTION_HEIGHT);
        let row_height = self.scale_metric(PLACES_ROW_HEIGHT);
        let row_gap = self.scale_metric(PLACES_ROW_GAP);
        let icon_size = self.scale_metric(PLACES_ICON_SIZE);
        let text_height = self.text_line_height();
        let small_text_height = self.small_text_line_height();
        text.push_label_aligned(
            "Places",
            ViewRect {
                x: panel.x + padding_x + self.scale_metric(8.0),
                y: panel.y + top_padding,
                width: (panel.width - padding_x * 2.0 - self.scale_metric(16.0)).max(1.0),
                height: text_height,
            },
            panel,
            TextColor::rgb(36, 41, 47),
            LabelAlignment::Start,
        );

        let mut y = panel.y + top_padding + title_height - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                let section = ViewRect {
                    x: panel.x + padding_x + self.scale_metric(8.0),
                    y: y + self.scale_metric(4.0),
                    width: (panel.width - padding_x * 2.0 - self.scale_metric(16.0)).max(1.0),
                    height: small_text_height,
                };
                if section.y < panel.bottom() && section.bottom() > panel.y {
                    text.push_label_aligned(
                        place.group,
                        section,
                        panel,
                        TextColor::rgb(107, 114, 128),
                        LabelAlignment::Start,
                    );
                }
                y += section_height;
            }

            let row = ViewRect {
                x: panel.x + padding_x,
                y,
                width: (panel.width - padding_x * 2.0).max(1.0),
                height: row_height,
            };
            if row.y < panel.bottom() && row.bottom() > panel.y {
                let active = active_place == Some(index);
                let hovered = self.hovered_place == Some(index);
                if active {
                    push_clipped_rounded_rect(
                        vertices,
                        row,
                        panel,
                        self.scale_metric(8.0),
                        [0.749, 0.859, 0.996, 1.0],
                        size,
                    );
                    if let Some(inner_row) = inset_rect(row, self.scale_metric(1.0)) {
                        push_clipped_rounded_rect(
                            vertices,
                            inner_row,
                            panel,
                            self.scale_metric(7.0),
                            place_row_background_color(active, hovered),
                            size,
                        );
                    }
                } else if hovered {
                    push_clipped_rounded_rect(
                        vertices,
                        row,
                        panel,
                        self.scale_metric(8.0),
                        place_row_background_color(active, hovered),
                        size,
                    );
                }
                let icon = ViewRect {
                    x: row.x + self.scale_metric(8.0),
                    y: row.y + (row.height - icon_size) / 2.0,
                    width: icon_size,
                    height: icon_size,
                };
                push_place_icon(vertices, icon, panel, place, active, self.ui_scale(), size);
                text.push_label_aligned(
                    &place.label,
                    ViewRect {
                        x: icon.right() + self.scale_metric(8.0),
                        y: row.y + (row.height - text_height) / 2.0,
                        width: (row.right() - icon.right() - self.scale_metric(16.0)).max(1.0),
                        height: text_height,
                    },
                    panel,
                    if active {
                        TextColor::rgb(31, 79, 191)
                    } else {
                        TextColor::rgb(36, 41, 47)
                    },
                    LabelAlignment::Start,
                );
                if place.trash && file_ops::trash_has_items() {
                    let dot_size = self.scale_metric(7.0);
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: row.right() - self.scale_metric(8.0) - dot_size,
                            y: row.y + (row.height - dot_size) / 2.0,
                            width: dot_size,
                            height: dot_size,
                        },
                        panel,
                        dot_size / 2.0,
                        [0.184, 0.435, 0.929, 1.0],
                        size,
                    );
                }
            }

            y += row_height + row_gap;
            previous_group = Some(place.group);
        }

        if let Some((track, thumb)) = self.places_scrollbar_rects(size) {
            push_scrollbar(vertices, track, thumb, panel, size);
        }
    }

    fn push_details_header(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let x = self.content_origin_x(size);
        let width = self.content_width(size);
        let y = self.details_header_y();
        let header = ViewRect {
            x,
            y,
            width,
            height: self.details_header_height(),
        };
        push_rect(vertices, header, [0.953, 0.961, 0.973, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x,
                y: header.bottom() - 1.0,
                width,
                height: 1.0,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );
        let name_width = self.details_name_width();
        let size_width = self.details_size_width();
        let modified_width = self.details_modified_width();
        let columns = [
            (
                "Name",
                self.scale_metric(34.0),
                name_width - self.scale_metric(42.0),
            ),
            (
                "Size",
                name_width + self.scale_metric(8.0),
                size_width - self.scale_metric(16.0),
            ),
            (
                "Modified",
                name_width + size_width + self.scale_metric(8.0),
                modified_width - self.scale_metric(16.0),
            ),
        ];
        for (label, x, width) in columns {
            text.push_label(
                label,
                ViewRect {
                    x: header.x + x,
                    y: header.y + self.scale_metric(6.0),
                    width: width.max(1.0),
                    height: self.text_line_height(),
                },
                header,
                TextColor::rgb(89, 99, 110),
            );
        }
    }

    fn push_filter_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(rect) = self.filter_bar_rect(size) else {
            return;
        };
        push_rect(vertices, rect, [0.973, 0.976, 0.984, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.bottom() - 1.0,
                width: rect.width,
                height: 1.0,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );
        text.push_label(
            "Filter:",
            ViewRect {
                x: rect.x + self.scale_metric(12.0),
                y: rect.y + self.scale_metric(6.0),
                width: self.scale_metric(54.0),
                height: self.text_line_height(),
            },
            rect,
            TextColor::rgb(89, 99, 110),
        );
        let pattern = if self.filter_pattern.is_empty() {
            ""
        } else {
            self.filter_pattern.as_str()
        };
        text.push_label(
            pattern,
            ViewRect {
                x: rect.x + self.scale_metric(66.0),
                y: rect.y + self.scale_metric(6.0),
                width: (rect.width - self.scale_metric(78.0)).max(1.0),
                height: self.text_line_height(),
            },
            rect,
            TextColor::rgb(36, 41, 47),
        );
    }

    fn push_rubber_band(
        &self,
        vertices: &mut Vec<QuadVertex>,
        content_clip: ViewRect,
        size: PhysicalSize<u32>,
    ) {
        let Some(rect) = self.rubber_band.as_ref().and_then(RubberBand::active_rect) else {
            return;
        };
        let rect = self.content_to_screen(rect, size);
        push_clipped_rect(vertices, rect, content_clip, [0.28, 0.58, 0.92, 0.18], size);
        push_clipped_rect_outline(
            vertices,
            rect,
            content_clip,
            1.5,
            [0.45, 0.72, 0.98, 0.92],
            size,
        );
    }

    fn push_status_bar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
        visible_items: usize,
        rect: ViewRect,
    ) {
        if rect.height <= 0.0 {
            return;
        }
        push_rect(vertices, rect, [1.000, 1.000, 1.000, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: 1.0,
            },
            [0.784, 0.808, 0.839, 1.0],
            size,
        );

        let mut status = format!(
            "{} entries ({} dirs, {} files) | {} selected | {} visible | {} | {}%",
            self.primary_pane.entries.len(),
            self.primary_pane.dir_count,
            self.primary_pane
                .entries
                .len()
                .saturating_sub(self.primary_pane.dir_count),
            self.selection.len(),
            visible_items,
            self.primary_pane.view_mode.label(),
            self.zoom_percent()
        );
        if self.show_hidden {
            status.push_str(" | hidden");
        }
        if let Some(value) = self.location_draft_value() {
            status.push_str(&format!(" | location {:?}", value));
        }
        if let Some(dialog) = self.create_dialog.as_ref() {
            status.push_str(&format!(
                " | create {} {:?}",
                dialog.kind.as_str(),
                dialog.name
            ));
        }
        if let Some(dialog) = self.rename_dialog.as_ref() {
            status.push_str(&format!(" | rename {:?}", dialog.name));
        }
        if let Some(chooser) = self.open_with_chooser.as_ref() {
            status.push_str(&format!(
                " | open-with {} apps {:?}",
                chooser.filtered_count(),
                chooser.query
            ));
        }
        if let Some(dialog) = self.trash_conflict_dialog.as_ref() {
            status.push_str(&format!(" | trash conflicts {}", dialog.conflicts.len()));
        }
        if let Some(target) = self.context_target.as_ref() {
            status.push_str(&format!(
                " | context {} {}",
                target.kind(),
                target.log_path().display()
            ));
        }
        if self.filter_active || !self.filter_pattern.is_empty() {
            status.push_str(&format!(
                " | filter {:?} ({})",
                self.filter_pattern,
                self.filtered_entry_count()
            ));
        }
        text.push_label(
            &status,
            ViewRect {
                x: rect.x + self.scale_metric(12.0),
                y: rect.y + self.scale_metric(5.0),
                width: (rect.width - self.scale_metric(24.0)).max(1.0),
                height: self.text_line_height(),
            },
            rect,
            TextColor::rgb(89, 99, 110),
        );
    }

    fn push_pane_borders(&self, vertices: &mut Vec<QuadVertex>, size: PhysicalSize<u32>) {
        let body = self.pane_body_rect(size);
        push_rect(
            vertices,
            ViewRect {
                x: body.x,
                y: body.y,
                width: body.width,
                height: 1.0,
            },
            [0.875, 0.890, 0.910, 1.0],
            size,
        );
    }

    fn push_content_scrollbar_for_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some((track, thumb)) = self.content_scrollbar_rects_for_projection(projection) else {
            return false;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_scrollbar(vertices, track, thumb, screen, size);
        true
    }

    fn push_context_menu_item_icon(
        &self,
        vertices: &mut Vec<QuadVertex>,
        icons: &mut IconFrameBuilder<'_>,
        item: &ShellContextMenuItem,
        icon: ViewRect,
        clip: ViewRect,
        scale: f32,
        size: PhysicalSize<u32>,
    ) {
        if let Some((icon_name, fallback)) = context_menu_named_icon_request(item)
            && icons.push_named_theme_icon(icon_name, fallback, icon, clip, IconDrawLayer::Overlay)
        {
            return;
        }
        let (glyph, icon_fg, icon_bg) = context_menu_item_icon_style(item);
        push_context_menu_icon(vertices, icon, clip, glyph, icon_fg, icon_bg, scale, size);
    }

    fn push_context_menu_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let scale = self.ui_scale();
        let rect = context_menu_rect_scaled(menu, size, scale);
        let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
        let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
        let row_padding_x = scaled_context_menu_metric(8.0, scale);
        let gap = scaled_context_menu_metric(8.0, scale);
        let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
        let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_context_menu_shadow(vertices, rect, clip, scale, size);
        push_clipped_rounded_rect(
            vertices,
            rect,
            clip,
            self.scale_metric(6.0),
            [1.000, 1.000, 1.000, 1.0],
            size,
        );

        let items = context_menu_items(menu);
        for (row, item) in items.iter().enumerate() {
            let row_rect = ViewRect {
                x: rect.x,
                y: rect.y + padding_y + row as f32 * row_height,
                width: rect.width,
                height: row_height,
            };
            if menu.hovered_row == Some(row) {
                push_rect(vertices, row_rect, [0.918, 0.945, 1.000, 1.0], size);
            }
            if context_menu_separator_before(&menu.target, row) {
                push_clipped_rect(
                    vertices,
                    ViewRect {
                        x: rect.x + row_padding_x,
                        y: row_rect.y,
                        width: (rect.width - row_padding_x * 2.0).max(1.0),
                        height: scale.round().max(1.0),
                    },
                    rect,
                    [0.898, 0.906, 0.922, 1.0],
                    size,
                );
            }
            let icon = ViewRect {
                x: row_rect.x + row_padding_x,
                y: row_rect.y + (row_rect.height - icon_size) / 2.0,
                width: icon_size,
                height: icon_size,
            };
            self.push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
            let text_x = icon.right() + gap;
            text.push_label_aligned(
                context_menu_item_label(item, self.show_hidden).as_str(),
                ViewRect {
                    x: text_x,
                    y: row_rect.y + (row_rect.height - text_height) / 2.0,
                    width: (row_rect.right()
                        - text_x
                        - row_padding_x
                        - if item.submenu.is_some() { gap } else { 0.0 })
                    .max(1.0),
                    height: text_height,
                },
                rect,
                if menu.hovered_row == Some(row) {
                    TextColor::rgb(31, 79, 191)
                } else {
                    TextColor::rgb(36, 41, 47)
                },
                LabelAlignment::Start,
            );
            if item.submenu.is_some() {
                text.push_label_aligned(
                    ">",
                    ViewRect {
                        x: row_rect.right() - row_padding_x - gap,
                        y: row_rect.y + (row_rect.height - text_height) / 2.0,
                        width: gap,
                        height: text_height,
                    },
                    rect,
                    TextColor::rgb(89, 99, 110),
                    LabelAlignment::Center,
                );
            }
        }
        push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.784, 0.808, 0.839, 1.0], size);
        self.push_context_submenu_overlay(vertices, text, icons, menu, size);
    }

    fn push_context_submenu_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        menu: &ShellContextMenu,
        size: PhysicalSize<u32>,
    ) {
        let Some(submenu) = menu.active_submenu else {
            return;
        };
        let scale = self.ui_scale();
        let Some(rect) = context_menu_submenu_rect(menu, size, scale) else {
            return;
        };
        let items = context_submenu_actions(submenu, menu);
        let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
        let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
        let row_padding_x = scaled_context_menu_metric(8.0, scale);
        let gap = scaled_context_menu_metric(8.0, scale);
        let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
        let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_context_menu_shadow(vertices, rect, clip, scale, size);
        push_clipped_rounded_rect(
            vertices,
            rect,
            clip,
            self.scale_metric(6.0),
            [1.000, 1.000, 1.000, 1.0],
            size,
        );
        for (row, item) in items.iter().enumerate() {
            let row_rect = ViewRect {
                x: rect.x,
                y: rect.y + padding_y + row as f32 * row_height,
                width: rect.width,
                height: row_height,
            };
            if menu.hovered_submenu_row == Some(row) {
                push_rect(vertices, row_rect, [0.918, 0.945, 1.000, 1.0], size);
            }
            if item.separator_before {
                push_clipped_rect(
                    vertices,
                    ViewRect {
                        x: rect.x + row_padding_x,
                        y: row_rect.y,
                        width: (rect.width - row_padding_x * 2.0).max(1.0),
                        height: scale.round().max(1.0),
                    },
                    rect,
                    [0.898, 0.906, 0.922, 1.0],
                    size,
                );
            }
            let icon = ViewRect {
                x: row_rect.x + row_padding_x,
                y: row_rect.y + (row_rect.height - icon_size) / 2.0,
                width: icon_size,
                height: icon_size,
            };
            self.push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
            let text_x = icon.right() + gap;
            text.push_label_aligned(
                context_menu_item_label(item, self.show_hidden).as_str(),
                ViewRect {
                    x: text_x,
                    y: row_rect.y + (row_rect.height - text_height) / 2.0,
                    width: (row_rect.right() - text_x - row_padding_x).max(1.0),
                    height: text_height,
                },
                rect,
                if menu.hovered_submenu_row == Some(row) {
                    TextColor::rgb(31, 79, 191)
                } else {
                    TextColor::rgb(36, 41, 47)
                },
                LabelAlignment::Start,
            );
        }
        push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.784, 0.808, 0.839, 1.0], size);
    }

    fn push_properties_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(overlay) = self.properties_overlay.as_ref() else {
            return;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, screen, [0.0, 0.0, 0.0, 0.40], size);
        let scale = self.ui_scale();
        let rect = properties_overlay_rect_scaled(overlay, size, scale);
        let title_height = scaled_dialog_metric(PROPERTIES_TITLE_HEIGHT, scale);
        let row_height = scaled_dialog_metric(PROPERTIES_ROW_HEIGHT, scale);
        let margin = scaled_dialog_metric(16.0, scale);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: title_height,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            &overlay.title,
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(12.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        let rows_y = rect.y + title_height + scaled_dialog_metric(10.0, scale);
        for (index, row) in overlay.rows.iter().enumerate() {
            let y = rows_y + index as f32 * row_height;
            text.push_label(
                row.label,
                ViewRect {
                    x: rect.x + margin,
                    y,
                    width: scaled_dialog_metric(92.0, scale),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(164, 176, 188),
            );
            text.push_label(
                &row.value,
                ViewRect {
                    x: rect.x + scaled_dialog_metric(116.0, scale),
                    y,
                    width: (rect.width - scaled_dialog_metric(132.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(222, 230, 238),
            );
        }
    }

    fn push_create_dialog_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(dialog) = self.create_dialog.as_ref() else {
            return;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, screen, [0.0, 0.0, 0.0, 0.44], size);
        let scale = self.ui_scale();
        let rect = create_dialog_rect_scaled(dialog, size, scale);
        let title_height = scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale);
        let margin = scaled_dialog_metric(16.0, scale);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: title_height,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            "Create New",
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(12.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
            let button = create_kind_button_rect_scaled(rect, kind, scale);
            let active = dialog.kind == kind;
            push_rect(
                vertices,
                button,
                if active {
                    view_mode_badge_color(self.primary_pane.view_mode)
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                kind.label(),
                ViewRect {
                    x: button.x + scaled_dialog_metric(10.0, scale),
                    y: button.y + scaled_dialog_metric(4.0, scale),
                    width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                if active {
                    TextColor::rgb(246, 250, 252)
                } else {
                    TextColor::rgb(196, 207, 218)
                },
            );
        }

        let input = create_dialog_input_rect_scaled(rect, scale);
        push_rect(vertices, input, [0.078, 0.086, 0.096, 1.0], size);
        push_clipped_rect_outline(vertices, input, rect, 1.0, [0.26, 0.31, 0.36, 1.0], size);
        let draft = format!("{}|", dialog.name);
        text.push_label(
            &draft,
            ViewRect {
                x: input.x + scaled_dialog_metric(10.0, scale),
                y: input.y + scaled_dialog_metric(7.0, scale),
                width: (input.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            input,
            TextColor::rgb(230, 236, 241),
        );

        if let Some(error) = dialog.error.as_ref() {
            text.push_label(
                error,
                ViewRect {
                    x: rect.x + margin,
                    y: input.bottom() + scaled_dialog_metric(8.0, scale),
                    width: (rect.width - margin * 2.0).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 132, 122),
            );
        }

        let cancel = create_dialog_cancel_button_rect_scaled(rect, scale);
        let commit = create_dialog_commit_button_rect_scaled(rect, scale);
        for (label, button, active) in [("Cancel", cancel, false), ("Create", commit, true)] {
            push_rect(
                vertices,
                button,
                if active {
                    [0.22, 0.42, 0.62, 1.0]
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                label,
                ViewRect {
                    x: button.x + scaled_dialog_metric(10.0, scale),
                    y: button.y + scaled_dialog_metric(4.0, scale),
                    width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 244, 249),
            );
        }
    }

    fn push_rename_dialog_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(dialog) = self.rename_dialog.as_ref() else {
            return;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, screen, [0.0, 0.0, 0.0, 0.44], size);
        let scale = self.ui_scale();
        let rect = rename_dialog_rect_scaled(dialog, size, scale);
        let title_height = scaled_dialog_metric(RENAME_DIALOG_TITLE_HEIGHT, scale);
        let margin = scaled_dialog_metric(16.0, scale);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: title_height,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            if dialog.is_dir {
                "Rename Folder"
            } else {
                "Rename File"
            },
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(12.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        let input = rename_dialog_input_rect_scaled(rect, scale);
        push_rect(vertices, input, [0.078, 0.086, 0.096, 1.0], size);
        push_clipped_rect_outline(vertices, input, rect, 1.0, [0.26, 0.31, 0.36, 1.0], size);
        let draft = format!("{}|", dialog.name);
        text.push_label(
            &draft,
            ViewRect {
                x: input.x + scaled_dialog_metric(10.0, scale),
                y: input.y + scaled_dialog_metric(7.0, scale),
                width: (input.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            input,
            TextColor::rgb(230, 236, 241),
        );

        if let Some(error) = dialog.error.as_ref() {
            text.push_label(
                error,
                ViewRect {
                    x: rect.x + margin,
                    y: input.bottom() + scaled_dialog_metric(8.0, scale),
                    width: (rect.width - margin * 2.0).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 132, 122),
            );
        }

        let cancel = rename_dialog_cancel_button_rect_scaled(rect, scale);
        let commit = rename_dialog_commit_button_rect_scaled(rect, scale);
        for (label, button, active) in [("Cancel", cancel, false), ("Rename", commit, true)] {
            push_rect(
                vertices,
                button,
                if active {
                    [0.22, 0.42, 0.62, 1.0]
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                label,
                ViewRect {
                    x: button.x + scaled_dialog_metric(10.0, scale),
                    y: button.y + scaled_dialog_metric(4.0, scale),
                    width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 244, 249),
            );
        }
    }

    fn push_open_with_chooser_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(chooser) = self.open_with_chooser.as_ref() else {
            return;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, screen, [0.0, 0.0, 0.0, 0.46], size);
        let scale = self.ui_scale();
        let rect = open_with_chooser_rect_scaled(chooser, size, scale);
        let title_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale);
        let margin = scaled_dialog_metric(16.0, scale);
        let row_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: title_height,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            &format!("Open With - {}", path_name_or_display(&chooser.path)),
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(8.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );
        text.push_label(
            chooser.mime_type.as_deref().unwrap_or("unknown MIME"),
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(25.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(14.0, scale),
            },
            rect,
            TextColor::rgb(162, 176, 188),
        );

        let query = open_with_chooser_query_rect_scaled(rect, scale);
        push_rect(vertices, query, [0.078, 0.086, 0.096, 1.0], size);
        push_clipped_rect_outline(vertices, query, rect, 1.0, [0.26, 0.31, 0.36, 1.0], size);
        let query_label = if chooser.query.is_empty() {
            "Search applications|".to_string()
        } else {
            format!("{}|", chooser.query)
        };
        text.push_label(
            &query_label,
            ViewRect {
                x: query.x + scaled_dialog_metric(10.0, scale),
                y: query.y + scaled_dialog_metric(7.0, scale),
                width: (query.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            query,
            if chooser.query.is_empty() {
                TextColor::rgb(132, 146, 158)
            } else {
                TextColor::rgb(230, 236, 241)
            },
        );

        let list = open_with_chooser_list_rect_scaled(rect, chooser, scale);
        push_rect(vertices, list, [0.090, 0.100, 0.112, 1.0], size);
        push_clipped_rect_outline(vertices, list, rect, 1.0, [0.23, 0.27, 0.31, 1.0], size);
        let visible = chooser.visible_filtered_indexes();
        if visible.is_empty() {
            text.push_label(
                "No matching applications",
                ViewRect {
                    x: list.x + scaled_dialog_metric(12.0, scale),
                    y: list.y + scaled_dialog_metric(10.0, scale),
                    width: (list.width - scaled_dialog_metric(24.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                list,
                TextColor::rgb(178, 188, 198),
            );
        } else {
            for (visible_row, app_index) in visible.iter().copied().enumerate() {
                let row = chooser.scroll_row + visible_row;
                let Some(application) = chooser.applications.get(app_index) else {
                    continue;
                };
                let row_rect = ViewRect {
                    x: list.x,
                    y: list.y + visible_row as f32 * row_height,
                    width: list.width,
                    height: row_height,
                };
                let selected = row == chooser.selected_index;
                push_rect(
                    vertices,
                    row_rect,
                    if selected {
                        [0.19, 0.33, 0.50, 0.90]
                    } else if application.is_default {
                        [0.15, 0.18, 0.16, 0.88]
                    } else if visible_row % 2 == 1 {
                        [0.105, 0.114, 0.126, 0.76]
                    } else {
                        [0.095, 0.104, 0.116, 0.72]
                    },
                    size,
                );
                let marker = ViewRect {
                    x: row_rect.x + scaled_dialog_metric(10.0, scale),
                    y: row_rect.y + scaled_dialog_metric(11.0, scale),
                    width: scaled_dialog_metric(16.0, scale),
                    height: scaled_dialog_metric(16.0, scale),
                };
                push_rect(
                    vertices,
                    marker,
                    if application.is_default {
                        [0.42, 0.58, 0.34, 1.0]
                    } else {
                        [0.38, 0.48, 0.58, 1.0]
                    },
                    size,
                );
                let name = if application.is_default {
                    format!("{} (default)", application.name)
                } else {
                    application.name.clone()
                };
                text.push_label(
                    &name,
                    ViewRect {
                        x: row_rect.x + scaled_dialog_metric(36.0, scale),
                        y: row_rect.y + scaled_dialog_metric(4.0, scale),
                        width: (row_rect.width - scaled_dialog_metric(48.0, scale)).max(1.0),
                        height: scaled_dialog_metric(18.0, scale),
                    },
                    row_rect,
                    if selected {
                        TextColor::rgb(246, 250, 252)
                    } else {
                        TextColor::rgb(222, 230, 238)
                    },
                );
                text.push_label(
                    &application.id,
                    ViewRect {
                        x: row_rect.x + scaled_dialog_metric(36.0, scale),
                        y: row_rect.y + scaled_dialog_metric(20.0, scale),
                        width: (row_rect.width - scaled_dialog_metric(48.0, scale)).max(1.0),
                        height: scaled_dialog_metric(14.0, scale),
                    },
                    row_rect,
                    if selected {
                        TextColor::rgb(198, 214, 228)
                    } else {
                        TextColor::rgb(146, 160, 174)
                    },
                );
            }
        }

        if chooser.filtered_count() > OPEN_WITH_CHOOSER_MAX_ROWS {
            let end = (chooser.scroll_row + visible.len()).min(chooser.filtered_count());
            text.push_label(
                &format!(
                    "{}-{} of {}",
                    chooser.scroll_row + 1,
                    end,
                    chooser.filtered_count()
                ),
                ViewRect {
                    x: rect.x + margin,
                    y: list.bottom() + scaled_dialog_metric(5.0, scale),
                    width: scaled_dialog_metric(120.0, scale),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(146, 160, 174),
            );
        }

        if let Some(error) = chooser.error.as_ref() {
            text.push_label(
                error,
                ViewRect {
                    x: rect.x + margin,
                    y: list.bottom() + scaled_dialog_metric(5.0, scale),
                    width: (rect.width - margin * 2.0).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 132, 122),
            );
        }

        let cancel = open_with_chooser_cancel_button_rect_scaled(rect, scale);
        let open = open_with_chooser_open_button_rect_scaled(rect, scale);
        for (label, button, active) in [("Cancel", cancel, false), ("Open", open, true)] {
            push_rect(
                vertices,
                button,
                if active {
                    [0.22, 0.42, 0.62, 1.0]
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                label,
                ViewRect {
                    x: button.x + scaled_dialog_metric(10.0, scale),
                    y: button.y + scaled_dialog_metric(4.0, scale),
                    width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(238, 244, 249),
            );
        }
    }

    fn push_trash_conflict_dialog_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(dialog) = self.trash_conflict_dialog.as_ref() else {
            return;
        };
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, screen, [0.0, 0.0, 0.0, 0.48], size);
        let scale = self.ui_scale();
        let rect = trash_conflict_dialog_rect_scaled(dialog, size, scale);
        let title_height = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_TITLE_HEIGHT, scale);
        let margin = scaled_dialog_metric(16.0, scale);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.42, 0.36, 0.25, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: title_height,
            },
            [0.180, 0.150, 0.105, 1.0],
            size,
        );
        text.push_label(
            "Restore Conflict",
            ViewRect {
                x: rect.x + margin,
                y: rect.y + scaled_dialog_metric(12.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(248, 242, 232),
        );

        let count = dialog.conflicts.len();
        text.push_label(
            &format!("{count} item(s) already exist at the original location."),
            ViewRect {
                x: rect.x + margin,
                y: rect.y + title_height + scaled_dialog_metric(18.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            TextColor::rgb(226, 218, 206),
        );
        if let Some(conflict) = dialog.first_conflict() {
            text.push_label(
                &format!("Original: {}", conflict.original_path.display()),
                ViewRect {
                    x: rect.x + margin,
                    y: rect.y + title_height + scaled_dialog_metric(48.0, scale),
                    width: (rect.width - margin * 2.0).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(192, 202, 212),
            );
            text.push_label(
                &format!("Trash: {}", conflict.trash_path.display()),
                ViewRect {
                    x: rect.x + margin,
                    y: rect.y + title_height + scaled_dialog_metric(76.0, scale),
                    width: (rect.width - margin * 2.0).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(192, 202, 212),
            );
        }

        let cancel = trash_conflict_dialog_cancel_button_rect_scaled(rect, scale);
        let replace = trash_conflict_dialog_replace_button_rect_scaled(rect, scale);
        for (label, button, active) in [("Cancel", cancel, false), ("Replace", replace, true)] {
            push_rect(
                vertices,
                button,
                if active {
                    [0.58, 0.36, 0.18, 1.0]
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                label,
                ViewRect {
                    x: button.x + scaled_dialog_metric(10.0, scale),
                    y: button.y + scaled_dialog_metric(4.0, scale),
                    width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                rect,
                TextColor::rgb(248, 244, 238),
            );
        }
    }

    fn content_to_screen(&self, rect: ViewRect, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: rect.x - self.primary_pane.scroll_x + self.content_origin_x(size),
            y: rect.y - self.primary_pane.scroll_y + self.content_origin_y(),
            width: rect.width,
            height: rect.height,
        }
    }

    fn content_origin_x(&self, size: PhysicalSize<u32>) -> f32 {
        let sidebar_width = self.places_sidebar_width(size);
        if sidebar_width <= 0.0 {
            0.0
        } else {
            sidebar_width
                + self.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH)
                + self.scale_metric(PLACES_TO_PANE_GAP)
        }
    }

    fn content_origin_y(&self) -> f32 {
        self.details_header_y()
            + if self.primary_pane.view_mode == ShellViewMode::Details {
                self.details_header_height()
            } else {
                0.0
            }
    }

    fn details_header_y(&self) -> f32 {
        self.pane_top_y() + self.top_bar_height() + self.filter_bar_height()
    }

    fn filter_bar_height(&self) -> f32 {
        if self.filter_active || !self.filter_pattern.is_empty() {
            self.scale_metric(FILTER_BAR_HEIGHT)
        } else {
            0.0
        }
    }

    fn filter_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let height = self.filter_bar_height();
        (height > 0.0).then(|| ViewRect {
            x: self.content_origin_x(size),
            y: self.pane_top_y() + self.top_bar_height(),
            width: self.content_width(size),
            height,
        })
    }

    fn content_width(&self, size: PhysicalSize<u32>) -> f32 {
        let reserved = if self.content_scrollbar_axis() == ContentScrollbarAxis::Vertical {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        (self.pane_width(size) - reserved).max(1.0)
    }

    fn pane_width(&self, size: PhysicalSize<u32>) -> f32 {
        self.split_pane_metrics(size)
            .map(|metrics| metrics.left_width)
            .unwrap_or_else(|| (size.width as f32 - self.content_origin_x(size)).max(1.0))
    }

    fn pane_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let y = self.pane_top_y();
        let bottom_margin = self.pane_margin();
        ViewRect {
            x: self.content_origin_x(size),
            y,
            width: self.pane_width(size),
            height: (size.height.max(1) as f32 - y - bottom_margin).max(1.0),
        }
    }

    fn pane_body_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let pane = self.pane_rect(size);
        let status_bar = self.status_bar_rect(size);
        let y = pane.y + self.top_bar_height();
        ViewRect {
            x: pane.x,
            y,
            width: pane.width,
            height: (status_bar.y - y).max(1.0),
        }
    }

    fn viewport_height(&self, size: PhysicalSize<u32>) -> f32 {
        let reserved = if self.content_scrollbar_axis() == ContentScrollbarAxis::Horizontal {
            self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT)
        } else {
            0.0
        };
        (self.status_bar_rect(size).y - self.content_origin_y() - reserved).max(1.0)
    }

    fn places_sidebar_width(&self, size: PhysicalSize<u32>) -> f32 {
        if !self.places_visible {
            return 0.0;
        }
        let (min_width, max_width) = self.places_sidebar_width_bounds(size);
        if max_width <= f32::EPSILON {
            return 0.0;
        }
        self.scale_metric(self.places_width)
            .clamp(min_width, max_width)
    }

    fn places_sidebar_width_bounds(&self, size: PhysicalSize<u32>) -> (f32, f32) {
        let width = size.width.max(1) as f32;
        let reserve = self.scale_metric(PLACES_SIDEBAR_RIGHT_RESERVE);
        let max_for_window = (width - reserve).max(0.0);
        let min_width = self
            .scale_metric(PLACES_SIDEBAR_MIN_WIDTH)
            .min(max_for_window);
        let responsive_width = (width * PLACES_SIDEBAR_MAX_WIDTH_RATIO).max(min_width);
        let max_width = responsive_width.min(max_for_window).max(0.0);
        (min_width.min(max_width), max_width)
    }

    fn set_places_sidebar_width_px(&mut self, desired_width: f32, size: PhysicalSize<u32>) -> bool {
        if !self.places_visible {
            return false;
        }
        let (min_width, max_width) = self.places_sidebar_width_bounds(size);
        if max_width <= f32::EPSILON {
            return false;
        }
        let next_width = desired_width.clamp(min_width, max_width);
        let old_width = self.places_sidebar_width(size);
        if (old_width - next_width).abs() <= 0.5 {
            return false;
        }
        self.places_width = next_width / self.ui_scale().max(f32::EPSILON);
        self.places_resize_changes += 1;
        eprintln!(
            "[fika-wgpu] places-resize width={:.1} min={:.1} max={:.1} changes={}",
            next_width, min_width, max_width, self.places_resize_changes
        );
        true
    }

    fn split_pane_metrics(&self, size: PhysicalSize<u32>) -> Option<ShellPaneSplitMetrics> {
        self.split_pane.as_ref()?;
        let origin_x = self.content_origin_x(size);
        let total_width = (size.width.max(1) as f32 - origin_x).max(1.0);
        let divider_width = self.scale_metric(SPLIT_PANE_DIVIDER_WIDTH);
        let (available_width, min_width, max_left_width) =
            self.split_pane_width_bounds_for_total(total_width, divider_width);
        let left_width = (available_width * self.split_pane_left_fraction)
            .clamp(min_width, max_left_width)
            .round()
            .max(1.0);
        let divider = ViewRect {
            x: origin_x + left_width,
            y: self.pane_top_y(),
            width: divider_width,
            height: (size.height.max(1) as f32 - self.pane_top_y() - self.pane_margin()).max(1.0),
        };
        let right_x = divider.right();
        let right_pane = ViewRect {
            x: right_x,
            y: divider.y,
            width: (size.width.max(1) as f32 - right_x).max(1.0),
            height: divider.height,
        };
        Some(ShellPaneSplitMetrics {
            divider,
            right_pane,
            left_width,
        })
    }

    fn split_pane_width_bounds(&self, size: PhysicalSize<u32>) -> Option<(f32, f32, f32)> {
        self.split_pane.as_ref()?;
        let origin_x = self.content_origin_x(size);
        let total_width = (size.width.max(1) as f32 - origin_x).max(1.0);
        let divider_width = self.scale_metric(SPLIT_PANE_DIVIDER_WIDTH);
        Some(self.split_pane_width_bounds_for_total(total_width, divider_width))
    }

    fn split_pane_width_bounds_for_total(
        &self,
        total_width: f32,
        divider_width: f32,
    ) -> (f32, f32, f32) {
        let available_width = (total_width - divider_width).max(1.0);
        let min_width = self
            .scale_metric(SPLIT_PANE_MIN_WIDTH)
            .min((available_width / 2.0).max(1.0));
        let max_left_width = (available_width - min_width).max(min_width);
        (available_width, min_width, max_left_width)
    }

    fn set_split_pane_left_width_px(
        &mut self,
        desired_left_width: f32,
        size: PhysicalSize<u32>,
    ) -> bool {
        if self.split_pane.is_none() {
            return false;
        }
        let Some((available_width, min_width, max_left_width)) = self.split_pane_width_bounds(size)
        else {
            return false;
        };
        let next_width = desired_left_width.clamp(min_width, max_left_width);
        let Some(old_width) = self
            .split_pane_metrics(size)
            .map(|metrics| metrics.left_width)
        else {
            return false;
        };
        if (old_width - next_width).abs() <= 0.5 {
            return false;
        }
        self.split_pane_left_fraction = (next_width / available_width).clamp(0.0, 1.0);
        self.split_pane_changes += 1;
        eprintln!(
            "[fika-wgpu] split-pane-resize left_width={:.1} fraction={:.3} changes={}",
            next_width, self.split_pane_left_fraction, self.split_pane_changes
        );
        true
    }

    fn content_scrollbar_axis(&self) -> ContentScrollbarAxis {
        scrollbar_axis_for_view_mode(self.primary_pane.view_mode)
    }

    fn pane_content_scrollbar_axis(&self, kind: ShellPaneKind) -> Option<ContentScrollbarAxis> {
        self.pane_view(kind)
            .map(|pane| scrollbar_axis_for_view_mode(pane.view_mode))
    }

    fn content_screen_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: self.content_origin_x(size),
            y: self.content_origin_y(),
            width: self.content_width(size),
            height: self.viewport_height(size),
        }
    }

    fn status_bar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let height = size.height.max(1) as f32;
        let bar_height = self.status_bar_height().min(height);
        let x = self.content_origin_x(size);
        let pane = self.pane_rect(size);
        ViewRect {
            x,
            y: pane.bottom() - bar_height,
            width: self.pane_width(size),
            height: bar_height,
        }
    }

    fn clamp_scroll(&mut self, size: PhysicalSize<u32>) {
        self.clamp_pane_scroll(ShellPaneKind::Primary, size);
        self.clamp_pane_scroll(ShellPaneKind::Split, size);
        self.clamp_places_scroll(size);
        self.refresh_hover(size);
    }

    fn scroll_by(&mut self, delta_y: f32, size: PhysicalSize<u32>) -> bool {
        if self
            .pointer
            .is_some_and(|point| self.places_sidebar_rect(size).contains(point))
        {
            return self.scroll_places_by(delta_y, size);
        }

        let pane = self
            .pointer
            .and_then(|point| self.pane_kind_at_screen_point(point, size))
            .unwrap_or(ShellPaneKind::Primary);
        let scrolled = self.scroll_pane_by(pane, delta_y, size);
        let hover_changed = self.refresh_hover(size);
        scrolled || hover_changed
    }

    fn pane_kind_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneKind> {
        self.pane_geometries(size)
            .into_iter()
            .find(|geometry| geometry.content.contains(point))
            .map(|geometry| geometry.kind)
    }

    fn scroll_pane_by(
        &mut self,
        kind: ShellPaneKind,
        delta_y: f32,
        size: PhysicalSize<u32>,
    ) -> bool {
        let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
            return false;
        };
        let Some(metrics) = self.pane_scroll_metrics(kind, size) else {
            return false;
        };
        let (old_x, old_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        match axis {
            ContentScrollbarAxis::Horizontal => {
                self.set_pane_scroll_offset(
                    kind,
                    (old_x + delta_y).clamp(0.0, metrics.max_scroll_x),
                    0.0,
                );
            }
            ContentScrollbarAxis::Vertical => {
                self.set_pane_scroll_offset(
                    kind,
                    0.0,
                    (old_y + delta_y).clamp(0.0, metrics.max_scroll_y),
                );
            }
        }
        let (new_x, new_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        (new_x - old_x).abs() > f32::EPSILON || (new_y - old_y).abs() > f32::EPSILON
    }

    fn pane_scroll_metrics(
        &self,
        kind: ShellPaneKind,
        size: PhysicalSize<u32>,
    ) -> Option<ShellPaneScrollMetrics> {
        match kind {
            ShellPaneKind::Primary => Some(self.primary_pane_scroll_metrics(size)),
            ShellPaneKind::Split => {
                let geometry = self.split_pane_geometry(size)?;
                let view = self.pane_view(ShellPaneKind::Split)?;
                let layout =
                    self.pane_layout(view, geometry.content.width, geometry.content.height);
                Some(ShellPaneScrollMetrics::new(
                    layout.content_size(),
                    geometry.content,
                ))
            }
        }
    }

    fn pane_scroll_offset(&self, kind: ShellPaneKind) -> Option<(f32, f32)> {
        self.pane_state(kind)
            .map(|pane| (pane.scroll_x, pane.scroll_y))
    }

    fn set_pane_scroll_offset(&mut self, kind: ShellPaneKind, scroll_x: f32, scroll_y: f32) {
        if let Some(pane) = self.pane_state_mut(kind) {
            pane.scroll_x = scroll_x;
            pane.scroll_y = scroll_y;
        }
    }

    fn clamp_pane_scroll(&mut self, kind: ShellPaneKind, size: PhysicalSize<u32>) {
        let Some(metrics) = self.pane_scroll_metrics(kind, size) else {
            return;
        };
        let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
            return;
        };
        let (scroll_x, scroll_y) = self.pane_scroll_offset(kind).unwrap_or((0.0, 0.0));
        match axis {
            ContentScrollbarAxis::Horizontal => {
                self.set_pane_scroll_offset(kind, scroll_x.clamp(0.0, metrics.max_scroll_x), 0.0);
            }
            ContentScrollbarAxis::Vertical => {
                self.set_pane_scroll_offset(kind, 0.0, scroll_y.clamp(0.0, metrics.max_scroll_y));
            }
        }
    }

    fn clamp_places_scroll(&mut self, size: PhysicalSize<u32>) {
        self.places_scroll_y = self
            .places_scroll_y
            .clamp(0.0, self.max_places_scroll_y(size));
    }

    fn scroll_places_by(&mut self, delta_y: f32, size: PhysicalSize<u32>) -> bool {
        let old_y = self.places_scroll_y;
        self.places_scroll_y =
            (self.places_scroll_y + delta_y).clamp(0.0, self.max_places_scroll_y(size));
        let scrolled = (self.places_scroll_y - old_y).abs() > f32::EPSILON;
        if scrolled {
            self.places_scroll_changes += 1;
        }
        let hover_changed = self.refresh_hover(size);
        scrolled || hover_changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn max_scroll_x(&self, size: PhysicalSize<u32>) -> f32 {
        self.primary_pane_scroll_metrics(size).max_scroll_x
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn max_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        self.primary_pane_scroll_metrics(size).max_scroll_y
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn content_scrollbar_rects(&self, size: PhysicalSize<u32>) -> Option<(ViewRect, ViewRect)> {
        self.pane_content_scrollbar_rects(ShellPaneKind::Primary, size)
    }

    fn pane_content_scrollbar_rects(
        &self,
        kind: ShellPaneKind,
        size: PhysicalSize<u32>,
    ) -> Option<(ViewRect, ViewRect)> {
        let projection = self.pane_projection(kind, size)?;
        self.content_scrollbar_rects_for_projection(&projection)
    }

    fn content_scrollbar_rects_for_projection(
        &self,
        projection: &ShellPaneProjection<'_>,
    ) -> Option<(ViewRect, ViewRect)> {
        let metrics = projection.scroll_metrics;
        match scrollbar_axis_for_view_mode(projection.view.view_mode) {
            ContentScrollbarAxis::Vertical => {
                let max_scroll = metrics.max_scroll_y;
                if max_scroll <= f32::EPSILON {
                    return None;
                }
                let viewport_extent = metrics.viewport_height;
                let slot = ViewRect {
                    x: projection.geometry.content.right(),
                    y: projection.geometry.content.y,
                    width: self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT).min(
                        (projection.geometry.pane.right() - projection.geometry.content.x).max(1.0),
                    ),
                    height: viewport_extent,
                };
                let track = inset_content_scrollbar_slot(slot, self.ui_scale())?;
                let content_extent = metrics.content_size.height;
                let min_thumb_size = self.scale_metric(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
                let thumb_extent = (track.height * (viewport_extent / content_extent))
                    .clamp(min_thumb_size.min(track.height), track.height);
                if thumb_extent >= track.height {
                    return None;
                }
                let travel = (track.height - thumb_extent).max(0.0);
                let thumb_y =
                    track.y + (projection.view.scroll_y / max_scroll).clamp(0.0, 1.0) * travel;
                Some((
                    track,
                    ViewRect {
                        x: track.x,
                        y: thumb_y,
                        width: track.width,
                        height: thumb_extent,
                    },
                ))
            }
            ContentScrollbarAxis::Horizontal => {
                let max_scroll = metrics.max_scroll_x;
                if max_scroll <= f32::EPSILON {
                    return None;
                }
                let viewport_extent = metrics.viewport_width;
                let slot = ViewRect {
                    x: projection.geometry.content.x,
                    y: projection.geometry.content.bottom(),
                    width: viewport_extent,
                    height: self.scale_metric(CONTENT_SCROLLBAR_RESERVED_EXTENT).min(
                        (projection.geometry.status_bar.y - projection.geometry.content.y).max(1.0),
                    ),
                };
                let track = inset_content_scrollbar_slot(slot, self.ui_scale())?;
                let content_extent = metrics.content_size.width;
                let min_thumb_size = self.scale_metric(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
                let thumb_extent = (track.width * (viewport_extent / content_extent))
                    .clamp(min_thumb_size.min(track.width), track.width);
                if thumb_extent >= track.width {
                    return None;
                }
                let travel = (track.width - thumb_extent).max(0.0);
                let thumb_x =
                    track.x + (projection.view.scroll_x / max_scroll).clamp(0.0, 1.0) * travel;
                Some((
                    track,
                    ViewRect {
                        x: thumb_x,
                        y: track.y,
                        width: thumb_extent,
                        height: track.height,
                    },
                ))
            }
        }
    }

    fn begin_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<bool> {
        if let Some((track, thumb)) = self.places_scrollbar_rects(size)
            && track.contains(point)
        {
            let grab_offset = if thumb.contains(point) {
                point.y - thumb.y
            } else {
                thumb.height / 2.0
            };
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::Places,
                grab_offset,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some(handle) = self.places_resize_handle_rect(size)
            && handle.contains(point)
        {
            let sidebar = self.places_sidebar_rect(size);
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::PlacesResize,
                grab_offset: point.x - sidebar.right(),
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some(handle) = self.split_pane_resize_handle_rect(size)
            && handle.contains(point)
            && let Some(metrics) = self.split_pane_metrics(size)
        {
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::SplitPaneResize,
                grab_offset: point.x - metrics.divider.x,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        if let Some((pane, axis, _track, thumb)) = self.content_scrollbar_hit_at_point(point, size)
        {
            let grab_offset = match axis {
                ContentScrollbarAxis::Vertical => {
                    if thumb.contains(point) {
                        point.y - thumb.y
                    } else {
                        thumb.height / 2.0
                    }
                }
                ContentScrollbarAxis::Horizontal => {
                    if thumb.contains(point) {
                        point.x - thumb.x
                    } else {
                        thumb.width / 2.0
                    }
                }
            };
            self.scrollbar_drag = Some(ScrollbarDrag {
                target: ScrollbarDragTarget::Content { pane, axis },
                grab_offset,
            });
            self.pointer = Some(point);
            return Some(self.update_scrollbar_drag(point, size));
        }

        None
    }

    fn content_scrollbar_hit_at_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<(ShellPaneKind, ContentScrollbarAxis, ViewRect, ViewRect)> {
        for kind in [ShellPaneKind::Primary, ShellPaneKind::Split] {
            let Some(axis) = self.pane_content_scrollbar_axis(kind) else {
                continue;
            };
            let Some((track, thumb)) = self.pane_content_scrollbar_rects(kind, size) else {
                continue;
            };
            if track.contains(point) {
                return Some((kind, axis, track, thumb));
            }
        }
        None
    }

    fn update_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let Some(drag) = self.scrollbar_drag else {
            return false;
        };
        let old_x = self.primary_pane.scroll_x;
        let old_y = self.primary_pane.scroll_y;
        let old_split_scroll = self
            .split_pane
            .as_ref()
            .map(|pane| (pane.scroll_x, pane.scroll_y));
        let old_places_y = self.places_scroll_y;
        let old_places_width = self.places_sidebar_width(size);
        let old_split_left_width = self
            .split_pane_metrics(size)
            .map(|metrics| metrics.left_width);

        match drag.target {
            ScrollbarDragTarget::PlacesResize => {
                let desired_width = point.x - drag.grab_offset;
                self.set_places_sidebar_width_px(desired_width, size);
            }
            ScrollbarDragTarget::SplitPaneResize => {
                let desired_left_width = point.x - self.content_origin_x(size) - drag.grab_offset;
                self.set_split_pane_left_width_px(desired_left_width, size);
            }
            ScrollbarDragTarget::Places => {
                if let Some((track, thumb)) = self.places_scrollbar_rects(size) {
                    self.places_scroll_y = scrollbar_scroll_from_pointer(
                        point.y,
                        drag.grab_offset,
                        track.y,
                        track.height,
                        thumb.height,
                        self.max_places_scroll_y(size),
                    );
                }
            }
            ScrollbarDragTarget::Content {
                pane,
                axis: ContentScrollbarAxis::Vertical,
            } => {
                if let Some((track, thumb)) = self.pane_content_scrollbar_rects(pane, size) {
                    let next_y = scrollbar_scroll_from_pointer(
                        point.y,
                        drag.grab_offset,
                        track.y,
                        track.height,
                        thumb.height,
                        self.pane_scroll_metrics(pane, size)
                            .map(|metrics| metrics.max_scroll_y)
                            .unwrap_or(0.0),
                    );
                    self.set_pane_scroll_offset(pane, 0.0, next_y);
                }
            }
            ScrollbarDragTarget::Content {
                pane,
                axis: ContentScrollbarAxis::Horizontal,
            } => {
                if let Some((track, thumb)) = self.pane_content_scrollbar_rects(pane, size) {
                    let next_x = scrollbar_scroll_from_pointer(
                        point.x,
                        drag.grab_offset,
                        track.x,
                        track.width,
                        thumb.width,
                        self.pane_scroll_metrics(pane, size)
                            .map(|metrics| metrics.max_scroll_x)
                            .unwrap_or(0.0),
                    );
                    self.set_pane_scroll_offset(pane, next_x, 0.0);
                }
            }
        }

        self.clamp_scroll(size);
        let content_changed = (self.primary_pane.scroll_x - old_x).abs() > f32::EPSILON
            || (self.primary_pane.scroll_y - old_y).abs() > f32::EPSILON;
        let split_content_changed = old_split_scroll
            .zip(
                self.split_pane
                    .as_ref()
                    .map(|pane| (pane.scroll_x, pane.scroll_y)),
            )
            .is_some_and(|((old_x, old_y), (new_x, new_y))| {
                (old_x - new_x).abs() > f32::EPSILON || (old_y - new_y).abs() > f32::EPSILON
            });
        let places_changed = (self.places_scroll_y - old_places_y).abs() > f32::EPSILON;
        let places_resized =
            (self.places_sidebar_width(size) - old_places_width).abs() > f32::EPSILON;
        let split_resized = old_split_left_width
            .zip(
                self.split_pane_metrics(size)
                    .map(|metrics| metrics.left_width),
            )
            .is_some_and(|(old_width, new_width)| (old_width - new_width).abs() > f32::EPSILON);
        if places_changed {
            self.places_scroll_changes += 1;
        }
        let hover_changed = self.refresh_hover(size);
        content_changed
            || split_content_changed
            || places_changed
            || places_resized
            || split_resized
            || hover_changed
    }

    fn end_scrollbar_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        if self.scrollbar_drag.is_none() {
            return false;
        }
        self.pointer = Some(point);
        let changed = self.update_scrollbar_drag(point, size);
        self.scrollbar_drag = None;
        changed
    }

    fn is_scrollbar_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }

    fn set_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if self.scrollbar_drag.is_some() {
            return self.update_scrollbar_drag(point, size);
        }
        if self.context_menu.is_some() {
            return self.update_context_menu_hover(point, size);
        }
        if self.internal_drag.is_some() {
            return self.update_internal_drag(point, size);
        }
        if self.rubber_band.is_some() {
            return self.update_rubber_band(point, size);
        }
        self.refresh_hover(size)
    }

    fn clear_pointer(&mut self) -> bool {
        self.pointer = None;
        let changed = self.hovered_index.take().is_some()
            || self.hovered_place.take().is_some()
            || self.internal_drag.take().is_some()
            || self.clear_dnd_hover_target();
        if changed {
            self.hit_tests += 1;
        }
        changed
    }

    fn begin_primary_pointer(&mut self, click: SelectionClick, size: PhysicalSize<u32>) -> bool {
        self.rubber_band = None;
        self.pointer = Some(click.point);
        let hit = self.hit_test_screen_point(click.point, size);
        let hover_changed = self.set_hovered_index(hit);
        if let Some(index) = hit {
            let drag_started = !click.extend
                && !click.toggle
                && self.begin_internal_drag_for_primary_item(index, click.point);
            let selection_changed = self.selection.apply_click(hit, click.extend, click.toggle);
            if selection_changed {
                self.selection_changes += 1;
            }
            return hover_changed || selection_changed || drag_started;
        }
        self.internal_drag = None;
        if !self.content_screen_rect(size).contains(click.point) {
            return hover_changed;
        }

        let Some(start) = screen_to_content_point(
            click.point,
            self.scroll_offset(),
            self.content_screen_rect(size),
        ) else {
            return hover_changed;
        };
        self.rubber_band = Some(RubberBand::new(
            start,
            RubberBandMode::from_modifiers(click.extend, click.toggle),
            self.selection.clone(),
        ));
        let selection_changed = self.selection.apply_click(hit, click.extend, click.toggle);
        if selection_changed {
            self.selection_changes += 1;
        }
        hover_changed || selection_changed
    }

    fn end_primary_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if let Some(drag) = self.internal_drag.as_ref() {
            let was_active = drag.active;
            let request_created = self.finish_internal_drag(point, size).is_some();
            let hover_changed = self.refresh_hover(size);
            return was_active || request_created || hover_changed;
        }
        let band_was_active = self.rubber_band.as_ref().is_some_and(|band| band.active);
        let changed = if self.rubber_band.is_some() {
            self.update_rubber_band(point, size)
        } else {
            self.refresh_hover(size)
        };
        if self.rubber_band.take().is_some() {
            return changed || band_was_active;
        }
        changed
    }

    fn update_rubber_band(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let current = clamped_screen_to_content_point(
            point,
            self.scroll_offset(),
            self.content_screen_rect(size),
        );
        let Some(band) = self.rubber_band.as_mut() else {
            return self.refresh_hover(size);
        };
        let old_active_rect = band.active_rect();
        band.update(current);
        let active_rect = band.active_rect();
        let mode = band.mode;
        let base_selection = band.base_selection.clone();
        let rect_changed = old_active_rect != active_rect;
        let active_rect = active_rect.filter(|rect| rect.width > 0.0 && rect.height > 0.0);

        let hover_changed = self.refresh_hover(size);
        let Some(rect) = active_rect else {
            return hover_changed || rect_changed;
        };

        let indexes = self.rubber_band_indexes(rect, size);
        let selection_changed = self
            .selection
            .apply_rubber_band(&base_selection, &indexes, mode);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.rubber_band_updates += 1;
        hover_changed || rect_changed || selection_changed
    }

    fn rubber_band_indexes(&self, rect: ViewRect, size: PhysicalSize<u32>) -> Vec<usize> {
        let layout = self.layout(size);
        layout
            .indexes_intersecting(rect)
            .iter()
            .filter_map(|layout_index| {
                layout
                    .item(*layout_index)
                    .is_some_and(|item| item.visual_rect.intersects(rect))
                    .then(|| self.model_index_for_layout_index(*layout_index))
                    .flatten()
            })
            .collect()
    }

    fn navigate(
        &mut self,
        action: NavigationAction,
        extend: bool,
        size: PhysicalSize<u32>,
    ) -> bool {
        if self.filtered_entry_count() == 0 {
            return false;
        }

        let old_scroll_y = self.primary_pane.scroll_y;
        let old_hovered = self.hovered_index;
        let old_hovered_place = self.hovered_place;
        let current = self
            .selection
            .focus_or_first_selected()
            .and_then(|index| self.layout_index_for_model_index(index))
            .unwrap_or(0);
        let layout = self.layout(size);
        let Some(target_layout_index) =
            navigation_target(action, current, self.filtered_entry_count(), &layout)
        else {
            return false;
        };
        let Some(target) = self.model_index_for_layout_index(target_layout_index) else {
            return false;
        };

        let selection_changed = self.selection.apply_navigation(target, extend);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.keyboard_navigation += 1;
        self.ensure_index_visible(target, size);
        self.hovered_place = self
            .pointer
            .and_then(|point| self.place_index_at_screen_point(point, size));
        self.hovered_index = self
            .pointer
            .filter(|_| self.hovered_place.is_none())
            .and_then(|point| self.hit_test_screen_point(point, size));

        selection_changed
            || (self.primary_pane.scroll_y - old_scroll_y).abs() > f32::EPSILON
            || self.hovered_index != old_hovered
            || self.hovered_place != old_hovered_place
    }

    fn refresh_hover(&mut self, size: PhysicalSize<u32>) -> bool {
        let place_hit = self
            .pointer
            .and_then(|point| self.place_index_at_screen_point(point, size));
        let item_hit = if place_hit.is_none() {
            self.pointer
                .and_then(|point| self.hit_test_screen_point(point, size))
        } else {
            None
        };
        self.hit_tests += 1;
        let changed = self.hovered_place != place_hit || self.hovered_index != item_hit;
        self.hovered_place = place_hit;
        self.hovered_index = item_hit;
        changed
    }

    fn set_hovered_index(&mut self, hovered_index: Option<usize>) -> bool {
        self.hit_tests += 1;
        let changed = self.hovered_index != hovered_index;
        self.hovered_index = hovered_index;
        changed
    }

    fn set_hovered_place(&mut self, hovered_place: Option<usize>) -> bool {
        self.hit_tests += 1;
        let changed = self.hovered_place != hovered_place;
        self.hovered_place = hovered_place;
        changed
    }

    fn hit_test_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<usize> {
        self.pane_hit_test_screen_point(
            self.primary_pane_view(),
            self.primary_pane_geometry(size),
            point,
        )
    }

    fn pane_hit_test_screen_point(
        &self,
        pane: ShellPaneView<'_>,
        geometry: ShellPaneGeometry,
        point: ViewPoint,
    ) -> Option<usize> {
        if !geometry.content.contains(point) {
            return None;
        }
        let content_point = screen_to_content_point(
            point,
            ViewPoint {
                x: pane.scroll_x,
                y: pane.scroll_y,
            },
            geometry.content,
        )?;
        let layout = self.pane_layout(pane, geometry.content.width, geometry.content.height);
        let layout_index = layout.hit_test_content_point(content_point)?;
        let item = layout.item(layout_index)?;
        item.visual_rect
            .contains(content_point)
            .then(|| pane.filtered_indexes.get(layout_index).copied())
            .flatten()
    }

    fn ensure_index_visible(&mut self, index: usize, size: PhysicalSize<u32>) {
        let layout = self.layout(size);
        let Some(layout_index) = self.layout_index_for_model_index(index) else {
            return;
        };
        let Some(item) = layout.item(layout_index) else {
            return;
        };
        let viewport_h = self.viewport_height(size);
        let padding = 8.0;
        match self.primary_pane.view_mode {
            ShellViewMode::Compact => {
                if item.visual_rect.x < self.primary_pane.scroll_x + padding {
                    self.primary_pane.scroll_x = (item.visual_rect.x - padding).max(0.0);
                } else if item.visual_rect.right()
                    > self.primary_pane.scroll_x + self.content_width(size) - padding
                {
                    self.primary_pane.scroll_x =
                        item.visual_rect.right() - self.content_width(size) + padding;
                }
            }
            ShellViewMode::Icons | ShellViewMode::Details => {
                if item.visual_rect.y < self.primary_pane.scroll_y + padding {
                    self.primary_pane.scroll_y = (item.visual_rect.y - padding).max(0.0);
                } else if item.visual_rect.bottom()
                    > self.primary_pane.scroll_y + viewport_h - padding
                {
                    self.primary_pane.scroll_y = item.visual_rect.bottom() - viewport_h + padding;
                }
            }
        }
        self.clamp_scroll(size);
    }

    fn scroll_offset(&self) -> ViewPoint {
        ViewPoint {
            x: self.primary_pane.scroll_x,
            y: self.primary_pane.scroll_y,
        }
    }
}

struct SceneFrame {
    vertices: Vec<QuadVertex>,
    overlay_vertices: Vec<QuadVertex>,
    visible_items: usize,
    thumbnail_candidates: usize,
    quad_count: usize,
    content_size: ViewSize,
    content_scrollbar_visible: bool,
    first_item_rect: Option<ViewRect>,
    layout_us: u128,
    text_stats: TextFrameStats,
    icon_stats: IconFrameStats,
}

struct WgpuState {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    quad_renderer: QuadRenderer,
    overlay_quad_renderer: QuadRenderer,
    icon_renderer: IconRenderer,
    text_renderer: TextRenderer,
    overlay_text_renderer: TextRenderer,
    frame_count: u64,
    last_log: Instant,
    rendered_view_switches: u64,
}

impl WgpuState {
    fn new(window: &dyn Window) -> Result<Self, String> {
        let size = nonzero_size(window.surface_size());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });

        // The app stores and drops the renderer before the window. This mirrors
        // winit's own example strategy for handle-owning render resources while
        // keeping the Phase 0 spike free of a larger ownership wrapper.
        let window: &'static dyn Window = unsafe { std::mem::transmute(window) };
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("create surface: {error}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("request adapter: {error}"))?;

        let adapter_info = adapter.get_info();
        eprintln!(
            "[fika-wgpu] adapter name={:?} backend={:?}",
            adapter_info.name, adapter_info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fika-wgpu-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("request device: {error}"))?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "surface has no supported formats".to_string())?;
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .unwrap_or_else(|| capabilities.present_modes[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        eprintln!(
            "[fika-wgpu] surface-format={format:?} srgb={}",
            format.is_srgb() as u8
        );

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let quad_renderer = QuadRenderer::new(&device, config.format);
        let overlay_quad_renderer = QuadRenderer::new(&device, config.format);
        let icon_renderer = IconRenderer::new(&device, config.format);
        let text_renderer = TextRenderer::new(&device, config.format);
        let overlay_text_renderer = TextRenderer::new(&device, config.format);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            size,
            quad_renderer,
            overlay_quad_renderer,
            icon_renderer,
            text_renderer,
            overlay_text_renderer,
            frame_count: 0,
            last_log: Instant::now(),
            rendered_view_switches: 0,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.configure_surface(size, false);
    }

    fn force_reconfigure(&mut self, size: PhysicalSize<u32>) {
        self.configure_surface(size, true);
    }

    fn configure_surface(&mut self, size: PhysicalSize<u32>, force: bool) {
        let size = nonzero_size(size);
        if self.size == size && !force {
            return;
        }

        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        eprintln!(
            "[fika-wgpu] {} width={} height={}",
            if force { "reconfigure" } else { "resize" },
            size.width,
            size.height
        );
    }

    fn render(
        &mut self,
        window: &dyn Window,
        event_loop: &dyn ActiveEventLoop,
        scene: &mut ShellScene,
        reason: &'static str,
        force_log: bool,
    ) -> bool {
        let frame_start = Instant::now();
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.force_reconfigure(window.surface_size());
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                if force_log {
                    eprintln!(
                        "[fika-wgpu] frame-retry reason={} view={} surface=reconfigure",
                        reason,
                        scene.primary_pane.view_mode.as_str()
                    );
                }
                self.force_reconfigure(window.surface_size());
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        if force_log {
                            eprintln!(
                                "[fika-wgpu] frame-skip reason={} view={} surface=reconfigure-pending",
                                reason,
                                scene.primary_pane.view_mode.as_str()
                            );
                        }
                        window.request_redraw();
                        return false;
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded => {
                        if force_log {
                            eprintln!(
                                "[fika-wgpu] frame-skip reason={} view={} surface=not-ready",
                                reason,
                                scene.primary_pane.view_mode.as_str()
                            );
                        }
                        return false;
                    }
                    wgpu::CurrentSurfaceTexture::Validation => {
                        eprintln!("[fika-wgpu] surface validation error");
                        event_loop.exit();
                        return false;
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                if force_log {
                    eprintln!(
                        "[fika-wgpu] frame-skip reason={} view={} surface=not-ready",
                        reason,
                        scene.primary_pane.view_mode.as_str()
                    );
                }
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("[fika-wgpu] surface validation error");
                event_loop.exit();
                return false;
            }
        };

        let scene_frame = prepare_scene_frame(
            &mut self.text_renderer,
            &mut self.overlay_text_renderer,
            &mut self.icon_renderer,
            &self.device,
            &self.queue,
            scene,
            self.size,
        );
        let icon_work_pending = scene_frame.icon_stats.deferred > 0
            || scene_frame.icon_stats.raster_deferred > 0
            || scene_frame.icon_stats.thumbnail_deferred > 0
            || self.icon_renderer.resolver.has_pending()
            || self.icon_renderer.thumbnails.has_pending();
        if icon_work_pending {
            window.request_redraw();
        }
        self.quad_renderer
            .upload(&self.device, &self.queue, &scene_frame.vertices);
        self.overlay_quad_renderer
            .upload(&self.device, &self.queue, &scene_frame.overlay_vertices);

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fika-wgpu-frame"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fika-wgpu-frame-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(view_mode_clear_color(
                            scene.primary_pane.view_mode,
                        )),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.quad_renderer.draw(&mut pass);
            self.icon_renderer.draw(&mut pass);
            self.text_renderer.draw(&mut pass);
            self.overlay_quad_renderer.draw(&mut pass);
            self.icon_renderer.draw_overlay(&mut pass);
            self.overlay_text_renderer.draw(&mut pass);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        let view_switch_rendered = self.rendered_view_switches != scene.view_switches;
        self.frame_count += 1;
        if self.frame_count == 1
            || view_switch_rendered
            || force_log
            || self.last_log.elapsed() >= Duration::from_secs(1)
        {
            eprintln!(
                "[fika-wgpu] frame={} reason={} view={} scale={:.2} zoom={} zoom_changes={} path={} entries={} filtered={} show_hidden={} hidden_changes={} location_active={} location_changes={} filter_active={} filter_changes={} places={} places_visible={} places_width={:.1} place_hover={} places_changes={} places_resize_changes={} places_scroll_y={:.1} places_scroll_changes={} split_pane={} split_changes={} split_path={} content_scrollbar={} visible={} thumbnails={} slots={}/{} slot_reused={} slot_recycled={} slot_allocated={} selected={} hover={} dnd_hover={} dnd_hover_changes={} dnd_drop_requests={} context={} context_menu={} context_changes={} context_actions={} properties={} properties_changes={} create_dialog={} create_changes={} rename_dialog={} rename_changes={} open_with={} open_with_changes={} open_changes={} copy_location_changes={} file_clipboard_changes={} paste_changes={} trash_changes={} rubber_band={} hit_tests={} selection_changes={} keyboard_nav={} rubber_band_updates={} view_switches={} path_changes={} reloads={} quads={} layout_content={:.1}x{:.1} first_item={:.1},{:.1},{:.1},{:.1} icons={} icon_quads={} icon_fallbacks={} icon_deferred={} icon_raster_deferred={} thumb_loaded={} thumb_quads={} thumb_deferred={} thumb_read_ahead={} thumb_ready={}/{}b icon_cache={}/{} entries={} bytes={} icon_atlas={}x{}:{}b icon_resolve={}us icon_raster={}us text_labels={} text_quads={} text_cache={}/{} entries={} bytes={} batches={} scroll_x={:.1} scroll_y={:.1} layout={}us text_raster={}us text_atlas={}x{}:{}b render={}us",
                self.frame_count,
                reason,
                scene.primary_pane.view_mode.as_str(),
                scene.ui_scale(),
                scene.zoom_percent(),
                scene.zoom_changes,
                scene.primary_pane.path.display(),
                scene.primary_pane.entries.len(),
                scene.filtered_entry_count(),
                scene.show_hidden as u8,
                scene.hidden_changes,
                scene.is_location_editing() as u8,
                scene.location_changes,
                scene.filter_active as u8,
                scene.filter_changes,
                scene.places.len(),
                scene.places_visible as u8,
                scene.places_sidebar_width(self.size),
                scene.hovered_place.map(|index| index as i64).unwrap_or(-1),
                scene.places_changes,
                scene.places_resize_changes,
                scene.places_scroll_y,
                scene.places_scroll_changes,
                scene.split_pane.is_some() as u8,
                scene.split_pane_changes,
                scene
                    .split_pane
                    .as_ref()
                    .map(|pane| pane.path.display().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                scene_frame.content_scrollbar_visible as u8,
                scene_frame.visible_items,
                scene_frame.thumbnail_candidates,
                scene.visible_slot_stats.active,
                scene.visible_slot_stats.free,
                scene.visible_slot_stats.reused,
                scene.visible_slot_stats.recycled,
                scene.visible_slot_stats.allocated,
                scene.selection.len(),
                scene.hovered_index.map(|index| index as i64).unwrap_or(-1),
                scene
                    .dnd_hover_target
                    .as_ref()
                    .map(ShellDropTarget::kind)
                    .unwrap_or("none"),
                scene.dnd_hover_changes,
                scene.dnd_drop_requests,
                scene
                    .context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none"),
                scene.context_menu.is_some() as u8,
                scene.context_target_changes,
                scene.context_menu_actions,
                scene.properties_overlay.is_some() as u8,
                scene.properties_changes,
                scene.create_dialog.is_some() as u8,
                scene.create_changes,
                scene.rename_dialog.is_some() as u8,
                scene.rename_changes,
                scene.open_with_chooser.is_some() as u8,
                scene.open_with_changes,
                scene.open_changes,
                scene.copy_location_changes,
                scene.file_clipboard_changes,
                scene.paste_changes,
                scene.trash_changes,
                scene.rubber_band.as_ref().is_some_and(|band| band.active) as u8,
                scene.hit_tests,
                scene.selection_changes,
                scene.keyboard_navigation,
                scene.rubber_band_updates,
                scene.view_switches,
                scene.path_changes,
                scene.directory_reloads,
                scene_frame.quad_count,
                scene_frame.content_size.width,
                scene_frame.content_size.height,
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.x)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.y)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.width)
                    .unwrap_or(-1.0),
                scene_frame
                    .first_item_rect
                    .map(|rect| rect.height)
                    .unwrap_or(-1.0),
                scene_frame.icon_stats.icons,
                scene_frame.icon_stats.quads,
                scene_frame.icon_stats.fallbacks,
                scene_frame.icon_stats.deferred,
                scene_frame.icon_stats.raster_deferred,
                scene_frame.icon_stats.thumbnails,
                scene_frame.icon_stats.thumbnail_quads,
                scene_frame.icon_stats.thumbnail_deferred,
                scene_frame.icon_stats.thumbnail_read_ahead_queued,
                scene_frame.icon_stats.thumbnail_ready_entries,
                scene_frame.icon_stats.thumbnail_ready_bytes,
                scene_frame.icon_stats.cache_hits,
                scene_frame.icon_stats.cache_misses,
                scene_frame.icon_stats.cache_entries,
                scene_frame.icon_stats.cache_bytes,
                scene_frame.icon_stats.atlas_width,
                scene_frame.icon_stats.atlas_height,
                scene_frame.icon_stats.atlas_bytes,
                scene_frame.icon_stats.resolve_us,
                scene_frame.icon_stats.raster_us,
                scene_frame.text_stats.labels,
                scene_frame.text_stats.quads,
                scene_frame.text_stats.cache_hits,
                scene_frame.text_stats.cache_misses,
                scene_frame.text_stats.cache_entries,
                scene_frame.text_stats.cache_bytes,
                self.quad_renderer.batch_count()
                    + self.overlay_quad_renderer.batch_count()
                    + self.icon_renderer.batch_count()
                    + self.text_renderer.batch_count()
                    + self.overlay_text_renderer.batch_count(),
                scene.primary_pane.scroll_x,
                scene.primary_pane.scroll_y,
                scene_frame.layout_us,
                scene_frame.text_stats.raster_us,
                scene_frame.text_stats.atlas_width,
                scene_frame.text_stats.atlas_height,
                scene_frame.text_stats.atlas_bytes,
                frame_start.elapsed().as_micros()
            );
            self.last_log = Instant::now();
        }
        self.rendered_view_switches = scene.view_switches;
        true
    }
}

impl Drop for WgpuState {
    fn drop(&mut self) {
        let _ = self.instance.poll_all(false);
    }
}

struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
}

impl QuadRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-quad-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-quad-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[QuadVertex::layout()],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = 6;
        let vertex_buffer = create_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[QuadVertex]) {
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = create_vertex_buffer(device, self.vertex_capacity);
        }

        self.vertex_count = vertices.len();
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct IconFrameStats {
    icons: usize,
    quads: usize,
    fallbacks: usize,
    deferred: usize,
    thumbnails: usize,
    thumbnail_quads: usize,
    thumbnail_deferred: usize,
    thumbnail_read_ahead_queued: usize,
    thumbnail_ready_entries: usize,
    thumbnail_ready_bytes: usize,
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    raster_deferred: usize,
    cache_entries: usize,
    cache_bytes: usize,
    resolve_us: u128,
    raster_us: u128,
}

struct IconFrame {
    vertices: Vec<TextVertex>,
    overlay_vertices: Vec<TextVertex>,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    stats: IconFrameStats,
}

#[derive(Clone, Debug)]
struct IconDraw {
    screen: ViewRect,
    atlas: AtlasRect,
    source: ViewRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IconDrawLayer {
    Content,
    Overlay,
}

#[derive(Clone, Debug)]
struct IconRaster {
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconRasterCacheKey {
    path: PathBuf,
    size_px: u16,
    stamp: Option<u64>,
}

impl IconRasterCacheKey {
    fn icon(path: PathBuf, size_px: u16) -> Self {
        Self {
            path,
            size_px,
            stamp: None,
        }
    }

    fn thumbnail(path: PathBuf, size_px: u16, modified_secs: u64) -> Self {
        Self {
            path,
            size_px,
            stamp: Some(modified_secs),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailProbeCacheKey {
    path: PathBuf,
    modified_secs: u64,
}

impl ThumbnailProbeCacheKey {
    fn new(path: PathBuf, modified_secs: u64) -> Self {
        Self {
            path,
            modified_secs,
        }
    }

    fn from_raster_key(key: &IconRasterCacheKey) -> Option<Self> {
        Some(Self::new(key.path.clone(), key.stamp?))
    }
}

#[derive(Clone, Debug)]
struct CachedIconRaster {
    raster: IconRaster,
    bytes: usize,
    last_used_frame: u64,
}

#[derive(Debug)]
struct IconRasterCache {
    entries: HashMap<IconRasterCacheKey, CachedIconRaster>,
    frame: u64,
    bytes: usize,
    max_bytes: usize,
}

impl IconRasterCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            bytes: 0,
            max_bytes,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &IconRasterCacheKey) -> Option<IconRaster> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(entry.raster.clone())
    }

    fn contains(&self, key: &IconRasterCacheKey) -> bool {
        self.entries.contains_key(key)
    }

    fn insert(&mut self, key: IconRasterCacheKey, raster: IconRaster) -> IconRaster {
        let bytes = raster.pixels.len();
        if let Some(old) = self.entries.insert(
            key.clone(),
            CachedIconRaster {
                raster: raster.clone(),
                bytes,
                last_used_frame: self.frame,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.evict_if_needed(&key);
        raster
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn evict_if_needed(&mut self, protected: &IconRasterCacheKey) {
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ThumbnailRasterRequest {
    key: IconRasterCacheKey,
    mime_type: Option<String>,
    priority: ThumbnailRequestPriority,
}

#[derive(Clone, Debug)]
struct ThumbnailRasterResult {
    key: IconRasterCacheKey,
    raster: Option<IconRaster>,
}

#[derive(Clone, Debug)]
enum ThumbnailResolveState {
    Ready(IconRaster),
    Pending,
    Failed,
}

#[derive(Clone, Debug)]
struct ThumbnailReadyEntry {
    raster: IconRaster,
    bytes: usize,
    last_used_frame: u64,
}

struct ThumbnailRasterResolver {
    ready: HashMap<IconRasterCacheKey, ThumbnailReadyEntry>,
    failed: HashSet<ThumbnailProbeCacheKey>,
    pending: HashMap<IconRasterCacheKey, ThumbnailRequestPriority>,
    ready_frame: u64,
    ready_bytes: usize,
    ready_max_bytes: usize,
    request_tx: Option<Sender<ThumbnailRasterRequest>>,
    result_rx: Receiver<ThumbnailRasterResult>,
}

impl ThumbnailRasterResolver {
    fn new() -> Self {
        Self::with_cache_root(default_thumbnail_cache_root())
    }

    fn with_cache_root(cache_root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ThumbnailRasterRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ThumbnailRasterResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-thumbnail-raster".to_string())
            .spawn(move || thumbnail_raster_worker(cache_root, request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::new(),
            ready_frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx,
            result_rx,
        }
    }

    fn resolve(
        &mut self,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<String>,
        size_px: u16,
    ) -> ThumbnailResolveState {
        self.drain_results();
        let key = IconRasterCacheKey::thumbnail(path.to_path_buf(), size_px, modified_secs);
        let failure_key = ThumbnailProbeCacheKey::new(path.to_path_buf(), modified_secs);
        if let Some(entry) = self.ready.remove(&key) {
            self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            return ThumbnailResolveState::Ready(entry.raster);
        }
        if self.failed.contains(&failure_key) {
            return ThumbnailResolveState::Failed;
        }
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => return ThumbnailResolveState::Pending,
            Some(ThumbnailRequestPriority::Deferred) | None => {}
        }
        if self.send_request(
            key,
            mime_type,
            ThumbnailRequestPriority::Visible,
            failure_key,
        ) {
            ThumbnailResolveState::Pending
        } else {
            ThumbnailResolveState::Failed
        }
    }

    fn queue_deferred(
        &mut self,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<String>,
        size_px: u16,
    ) -> bool {
        self.drain_results();
        let key = IconRasterCacheKey::thumbnail(path.to_path_buf(), size_px, modified_secs);
        let failure_key = ThumbnailProbeCacheKey::new(path.to_path_buf(), modified_secs);
        if self.ready.contains_key(&key)
            || self.failed.contains(&failure_key)
            || self.pending.contains_key(&key)
        {
            return false;
        }
        self.send_request(
            key,
            mime_type,
            ThumbnailRequestPriority::Deferred,
            failure_key,
        )
    }

    fn send_request(
        &mut self,
        key: IconRasterCacheKey,
        mime_type: Option<String>,
        priority: ThumbnailRequestPriority,
        failure_key: ThumbnailProbeCacheKey,
    ) -> bool {
        let Some(tx) = self.request_tx.as_ref() else {
            self.failed.insert(failure_key);
            return false;
        };
        if tx
            .send(ThumbnailRasterRequest {
                key: key.clone(),
                mime_type,
                priority,
            })
            .is_err()
        {
            self.failed.insert(failure_key);
            return false;
        }
        self.pending.insert(key, priority);
        true
    }

    fn drain_results(&mut self) -> usize {
        let mut changed = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&result.key);
            if let Some(raster) = result.raster {
                self.insert_ready(result.key, raster);
            } else if let Some(key) = ThumbnailProbeCacheKey::from_raster_key(&result.key) {
                self.failed.insert(key);
            }
            changed += 1;
        }
        changed
    }

    fn insert_ready(&mut self, key: IconRasterCacheKey, raster: IconRaster) {
        let bytes = raster.pixels.len();
        self.ready_frame = self.ready_frame.wrapping_add(1);
        if let Some(old) = self.ready.insert(
            key.clone(),
            ThumbnailReadyEntry {
                raster,
                bytes,
                last_used_frame: self.ready_frame,
            },
        ) {
            self.ready_bytes = self.ready_bytes.saturating_sub(old.bytes);
        }
        self.ready_bytes += bytes;
        self.evict_ready_if_needed(&key);
    }

    fn evict_ready_if_needed(&mut self, protected: &IconRasterCacheKey) {
        while self.ready_bytes > self.ready_max_bytes && self.ready.len() > 1 {
            let Some(victim) = self
                .ready
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.ready.remove(&victim) {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    fn ready_len(&self) -> usize {
        self.ready.len()
    }

    fn ready_bytes(&self) -> usize {
        self.ready_bytes
    }

    fn has_pending(&mut self) -> bool {
        self.drain_results();
        !self.pending.is_empty()
    }
}

fn thumbnail_raster_worker(
    cache_root: PathBuf,
    request_rx: Receiver<ThumbnailRasterRequest>,
    result_tx: Sender<ThumbnailRasterResult>,
) {
    let thumbnailers = ThumbnailerRegistry::shared_system();
    let mut visible = VecDeque::new();
    let mut deferred = VecDeque::new();
    let mut queued = HashMap::new();
    while let Some(request) =
        thumbnail_worker_next_request(&request_rx, &mut visible, &mut deferred, &mut queued)
    {
        let raster = thumbnail_request_from_raster_request(&request)
            .and_then(|thumbnail_request| {
                generate_thumbnail_with_external_thumbnailer_registry(
                    &cache_root,
                    &thumbnail_request,
                    thumbnailers,
                )
                .ok()
                .flatten()
            })
            .and_then(|thumbnail| rasterize_icon(thumbnail.path(), request.key.size_px as u32));
        if result_tx
            .send(ThumbnailRasterResult {
                key: request.key,
                raster,
            })
            .is_err()
        {
            break;
        }
    }
}

fn thumbnail_worker_next_request(
    request_rx: &Receiver<ThumbnailRasterRequest>,
    visible: &mut VecDeque<ThumbnailRasterRequest>,
    deferred: &mut VecDeque<ThumbnailRasterRequest>,
    queued: &mut HashMap<IconRasterCacheKey, ThumbnailRequestPriority>,
) -> Option<ThumbnailRasterRequest> {
    loop {
        while let Ok(request) = request_rx.try_recv() {
            thumbnail_worker_queue_request(request, visible, deferred, queued);
        }

        if let Some(request) = visible.pop_front().or_else(|| deferred.pop_front()) {
            queued.remove(&request.key);
            return Some(request);
        }

        match request_rx.recv() {
            Ok(request) => thumbnail_worker_queue_request(request, visible, deferred, queued),
            Err(_) => return None,
        }
    }
}

fn thumbnail_worker_queue_request(
    request: ThumbnailRasterRequest,
    visible: &mut VecDeque<ThumbnailRasterRequest>,
    deferred: &mut VecDeque<ThumbnailRasterRequest>,
    queued: &mut HashMap<IconRasterCacheKey, ThumbnailRequestPriority>,
) {
    let key = request.key.clone();
    match queued.get(&key).copied() {
        Some(ThumbnailRequestPriority::Visible) => {}
        Some(ThumbnailRequestPriority::Deferred)
            if request.priority == ThumbnailRequestPriority::Visible =>
        {
            deferred.retain(|queued| queued.key != key);
            queued.insert(key, ThumbnailRequestPriority::Visible);
            visible.push_back(request);
        }
        Some(ThumbnailRequestPriority::Deferred) => {}
        None => {
            let priority = request.priority;
            queued.insert(key, priority);
            match priority {
                ThumbnailRequestPriority::Visible => visible.push_back(request),
                ThumbnailRequestPriority::Deferred => deferred.push_back(request),
            }
        }
    }
}

fn thumbnail_request_from_raster_request(
    request: &ThumbnailRasterRequest,
) -> Option<ThumbnailRequest> {
    ThumbnailRequest::from_entry_metadata_with_mime(
        WGPU_SHELL_PANE_ID,
        Generation(0),
        ItemId(0),
        request.key.path.clone(),
        request.key.stamp?,
        request.mime_type.clone(),
        request.priority,
    )
}

fn entry_path_for_thumbnail(directory: &Path, entry: &Entry) -> PathBuf {
    entry
        .target_path
        .clone()
        .unwrap_or_else(|| directory.join(entry.name.as_ref()))
}

#[derive(Clone, Debug)]
struct ShellThumbnailCandidate {
    path: PathBuf,
    modified_secs: u64,
    mime_type: Option<String>,
}

struct IconFrameBuilder<'a> {
    resolver: &'a mut FileIconResolver,
    thumbnails: &'a mut ThumbnailRasterResolver,
    raster_cache: &'a mut IconRasterCache,
    surface_size: PhysicalSize<u32>,
    pixels: Vec<u8>,
    draws: Vec<IconDraw>,
    overlay_draws: Vec<IconDraw>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    icons: usize,
    fallbacks: usize,
    thumbnails_loaded: usize,
    thumbnail_quads: usize,
    thumbnail_deferred: usize,
    thumbnail_read_ahead_queued: usize,
    cache_hits: usize,
    cache_misses: usize,
    deferred: usize,
    raster_deferred: usize,
    raster_miss_budget: usize,
    resolve_us: u128,
    raster_us: u128,
}

impl<'a> IconFrameBuilder<'a> {
    fn new(
        resolver: &'a mut FileIconResolver,
        thumbnails: &'a mut ThumbnailRasterResolver,
        raster_cache: &'a mut IconRasterCache,
        surface_size: PhysicalSize<u32>,
    ) -> Self {
        Self {
            resolver,
            thumbnails,
            raster_cache,
            surface_size,
            pixels: vec![0; (ICON_ATLAS_WIDTH * 4) as usize],
            draws: Vec::with_capacity(64),
            overlay_draws: Vec::with_capacity(16),
            width: ICON_ATLAS_WIDTH,
            height: 1,
            cursor_x: ICON_PADDING,
            cursor_y: ICON_PADDING,
            row_height: 0,
            icons: 0,
            fallbacks: 0,
            thumbnails_loaded: 0,
            thumbnail_quads: 0,
            thumbnail_deferred: 0,
            thumbnail_read_ahead_queued: 0,
            cache_hits: 0,
            cache_misses: 0,
            deferred: 0,
            raster_deferred: 0,
            raster_miss_budget: ICON_RASTER_MISS_BUDGET_PER_FRAME,
            resolve_us: 0,
            raster_us: 0,
        }
    }

    fn push_icon(
        &mut self,
        directory: &Path,
        entry: &Entry,
        rect: ViewRect,
        clip: ViewRect,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            self.fallbacks += 1;
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };

        self.icons += 1;
        let resolve_start = Instant::now();
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0);
        let Some(snapshot) = self.resolver.resolve_entry(directory, entry, icon_size) else {
            self.resolve_us += resolve_start.elapsed().as_micros();
            self.deferred += 1;
            self.fallbacks += 1;
            return false;
        };
        self.resolve_us += resolve_start.elapsed().as_micros();

        let Some(path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        let size_px = icon_cache_size(icon_size);
        let key = IconRasterCacheKey::icon(path, size_px);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            if self.raster_miss_budget == 0 {
                self.raster_deferred += 1;
                self.fallbacks += 1;
                return false;
            }
            self.raster_miss_budget -= 1;
            let raster_start = Instant::now();
            let Some(raster) = rasterize_icon(&key.path, size_px as u32) else {
                self.raster_us += raster_start.elapsed().as_micros();
                self.fallbacks += 1;
                return false;
            };
            self.raster_us += raster_start.elapsed().as_micros();
            self.raster_cache.insert(key, raster)
        };

        self.copy_raster_to_atlas(raster, rect, screen, IconDrawLayer::Content);
        true
    }

    fn push_thumbnail_or_icon(
        &mut self,
        directory: &Path,
        entry: &Entry,
        rect: ViewRect,
        clip: ViewRect,
    ) -> bool {
        if self.push_thumbnail(directory, entry, rect, clip) {
            return true;
        }
        self.push_icon(directory, entry, rect, clip)
    }

    fn push_thumbnail(
        &mut self,
        directory: &Path,
        entry: &Entry,
        rect: ViewRect,
        clip: ViewRect,
    ) -> bool {
        if entry.is_dir || rect.width.max(rect.height) < 32.0 {
            return false;
        }
        let path = entry_path_for_thumbnail(directory, entry);
        let Some(modified_secs) = entry.modified_secs else {
            return false;
        };
        if !entry.metadata_complete
            || is_network_path(&path)
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.size_bytes,
                entry.mime_type.as_deref(),
                entry.mime_magic_checked,
            )
            || !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref())
        {
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        let size_px = icon_cache_size(rect.width.max(rect.height).clamp(16.0, 256.0));
        let key = IconRasterCacheKey::thumbnail(path.clone(), size_px, modified_secs);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else {
            match self.thumbnails.resolve(
                &path,
                modified_secs,
                entry
                    .mime_type
                    .as_deref()
                    .map(std::borrow::ToOwned::to_owned),
                size_px,
            ) {
                ThumbnailResolveState::Ready(raster) => {
                    self.cache_misses += 1;
                    self.thumbnails_loaded += 1;
                    self.raster_cache.insert(key, raster)
                }
                ThumbnailResolveState::Pending => {
                    self.thumbnail_deferred += 1;
                    return false;
                }
                ThumbnailResolveState::Failed => return false,
            }
        };
        self.copy_raster_to_atlas(raster, rect, screen, IconDrawLayer::Content);
        self.thumbnail_quads += 1;
        true
    }

    fn push_named_theme_icon(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            self.fallbacks += 1;
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        self.icons += 1;
        let resolve_start = Instant::now();
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0);
        let Some(snapshot) = self.resolver.resolve_named(icon_name, fallback, icon_size) else {
            self.resolve_us += resolve_start.elapsed().as_micros();
            self.deferred += 1;
            self.fallbacks += 1;
            return false;
        };
        self.resolve_us += resolve_start.elapsed().as_micros();

        let Some(path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        let size_px = icon_cache_size(icon_size);
        let key = IconRasterCacheKey::icon(path, size_px);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            if self.raster_miss_budget == 0 {
                self.raster_deferred += 1;
                self.fallbacks += 1;
                return false;
            }
            self.raster_miss_budget -= 1;
            let raster_start = Instant::now();
            let Some(raster) = rasterize_icon(&key.path, size_px as u32) else {
                self.raster_us += raster_start.elapsed().as_micros();
                self.fallbacks += 1;
                return false;
            };
            self.raster_us += raster_start.elapsed().as_micros();
            self.raster_cache.insert(key, raster)
        };

        self.copy_raster_to_atlas(raster, rect, screen, layer);
        true
    }

    fn queue_thumbnail_read_ahead(&mut self, candidate: ShellThumbnailCandidate, size_px: u16) {
        let key =
            IconRasterCacheKey::thumbnail(candidate.path.clone(), size_px, candidate.modified_secs);
        if self.raster_cache.contains(&key) {
            return;
        }
        if self.thumbnails.queue_deferred(
            &candidate.path,
            candidate.modified_secs,
            candidate.mime_type,
            size_px,
        ) {
            self.thumbnail_read_ahead_queued += 1;
        }
    }

    fn copy_raster_to_atlas(
        &mut self,
        raster: IconRaster,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
    ) {
        let atlas = self.allocate(raster.width, raster.height);
        self.copy_icon_pixels(atlas, &raster);

        let scale_x = raster.width as f32 / rect.width.max(1.0);
        let scale_y = raster.height as f32 / rect.height.max(1.0);
        let source = ViewRect {
            x: (screen.x - rect.x).max(0.0) * scale_x,
            y: (screen.y - rect.y).max(0.0) * scale_y,
            width: screen.width * scale_x,
            height: screen.height * scale_y,
        };
        let draw = IconDraw {
            screen,
            atlas,
            source,
        };
        match layer {
            IconDrawLayer::Content => self.draws.push(draw),
            IconDrawLayer::Overlay => self.overlay_draws.push(draw),
        }
    }

    fn finish(self) -> IconFrame {
        let height = self.height.max(1);
        let vertices = icon_draw_vertices(&self.draws, self.width, height, self.surface_size);
        let overlay_vertices =
            icon_draw_vertices(&self.overlay_draws, self.width, height, self.surface_size);
        let atlas_bytes = (self.width * height * 4) as usize;
        let cache_entries = self.raster_cache.len();
        let cache_bytes = self.raster_cache.bytes();
        let thumbnail_ready_entries = self.thumbnails.ready_len();
        let thumbnail_ready_bytes = self.thumbnails.ready_bytes();
        IconFrame {
            vertices,
            overlay_vertices,
            pixels: self.pixels,
            width: self.width,
            height,
            stats: IconFrameStats {
                icons: self.icons,
                quads: self.draws.len() + self.overlay_draws.len(),
                fallbacks: self.fallbacks,
                deferred: self.deferred,
                thumbnails: self.thumbnails_loaded,
                thumbnail_quads: self.thumbnail_quads,
                thumbnail_deferred: self.thumbnail_deferred,
                thumbnail_read_ahead_queued: self.thumbnail_read_ahead_queued,
                thumbnail_ready_entries,
                thumbnail_ready_bytes,
                atlas_width: self.width,
                atlas_height: height,
                atlas_bytes,
                cache_hits: self.cache_hits,
                cache_misses: self.cache_misses,
                raster_deferred: self.raster_deferred,
                cache_entries,
                cache_bytes,
                resolve_us: self.resolve_us,
                raster_us: self.raster_us,
            },
        }
    }

    fn allocate(&mut self, icon_width: u32, icon_height: u32) -> AtlasRect {
        if self.cursor_x + icon_width + ICON_PADDING > self.width {
            self.cursor_x = ICON_PADDING;
            self.cursor_y += self.row_height.max(1);
            self.row_height = 0;
        }

        let x = self.cursor_x;
        let y = self.cursor_y;
        self.cursor_x += icon_width + ICON_PADDING;
        self.row_height = self.row_height.max(icon_height + ICON_PADDING);
        self.ensure_height(y + icon_height + ICON_PADDING);

        AtlasRect {
            x: x as f32,
            y: y as f32,
            width: icon_width as f32,
            height: icon_height as f32,
        }
    }

    fn ensure_height(&mut self, needed_height: u32) {
        if needed_height <= self.height {
            return;
        }
        self.height = needed_height.next_power_of_two();
        self.pixels
            .resize((self.width * self.height * 4) as usize, 0);
    }

    fn copy_icon_pixels(&mut self, atlas: AtlasRect, raster: &IconRaster) {
        let atlas_x = atlas.x as u32;
        let atlas_y = atlas.y as u32;
        for row in 0..raster.height {
            let src_start = (row * raster.width * 4) as usize;
            let src_end = src_start + (raster.width * 4) as usize;
            let dst_start = (((atlas_y + row) * self.width + atlas_x) * 4) as usize;
            let dst_end = dst_start + (raster.width * 4) as usize;
            self.pixels[dst_start..dst_end].copy_from_slice(&raster.pixels[src_start..src_end]);
        }
    }
}

fn icon_draw_vertices(
    draws: &[IconDraw],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(draws.len() * 6);
    for draw in draws {
        push_textured_rect(
            &mut vertices,
            draw.screen,
            AtlasRect {
                x: draw.atlas.x + draw.source.x,
                y: draw.atlas.y + draw.source.y,
                width: draw.source.width,
                height: draw.source.height,
            },
            atlas_width,
            atlas_height,
            surface_size,
        );
    }
    vertices
}

struct IconRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    texture_width: u32,
    texture_height: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    overlay_vertex_start: usize,
    overlay_vertex_count: usize,
    resolver: FileIconResolver,
    thumbnails: ThumbnailRasterResolver,
    raster_cache: IconRasterCache,
}

impl IconRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-icon-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-wgpu-icon-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture = create_icon_texture(device, 1, 1);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_icon_bind_group(device, &bind_group_layout, &texture_view, &sampler);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-icon-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-icon-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-icon-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TextVertex::layout()],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            texture_width: 1,
            texture_height: 1,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            overlay_vertex_start: 0,
            overlay_vertex_count: 0,
            resolver: FileIconResolver::new(),
            thumbnails: ThumbnailRasterResolver::new(),
            raster_cache: IconRasterCache::new(ICON_CACHE_MAX_BYTES),
        }
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &IconFrame) {
        if frame.width != self.texture_width || frame.height != self.texture_height {
            self.texture = create_icon_texture(device, frame.width, frame.height);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bind_group = create_icon_bind_group(
                device,
                &self.bind_group_layout,
                &self.texture_view,
                &self.sampler,
            );
            self.texture_width = frame.width;
            self.texture_height = frame.height;
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );

        let total_vertices = frame.vertices.len() + frame.overlay_vertices.len();
        if total_vertices > self.vertex_capacity {
            self.vertex_capacity = total_vertices.next_power_of_two();
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
        }
        self.vertex_count = frame.vertices.len();
        self.overlay_vertex_start = frame.vertices.len();
        self.overlay_vertex_count = frame.overlay_vertices.len();
        if total_vertices > 0 {
            let mut vertices = Vec::with_capacity(total_vertices);
            vertices.extend_from_slice(&frame.vertices);
            vertices.extend_from_slice(&frame.overlay_vertices);
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    fn draw_overlay<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.overlay_vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        let start = self.overlay_vertex_start as u32;
        let end = start + self.overlay_vertex_count as u32;
        pass.draw(start..end, 0..1);
    }

    fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0) + usize::from(self.overlay_vertex_count > 0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TextFrameStats {
    labels: usize,
    quads: usize,
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    cache_entries: usize,
    cache_bytes: usize,
    raster_us: u128,
}

impl TextFrameStats {
    fn merged(self, other: Self) -> Self {
        Self {
            labels: self.labels + other.labels,
            quads: self.quads + other.quads,
            atlas_width: self.atlas_width.max(other.atlas_width),
            atlas_height: self.atlas_height.max(other.atlas_height),
            atlas_bytes: self.atlas_bytes + other.atlas_bytes,
            cache_hits: self.cache_hits + other.cache_hits,
            cache_misses: self.cache_misses + other.cache_misses,
            cache_entries: self.cache_entries + other.cache_entries,
            cache_bytes: self.cache_bytes + other.cache_bytes,
            raster_us: self.raster_us + other.raster_us,
        }
    }
}

struct TextFrame {
    vertices: Vec<TextVertex>,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    stats: TextFrameStats,
}

#[derive(Clone, Copy, Debug)]
struct AtlasRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct TextDraw {
    screen: ViewRect,
    atlas: AtlasRect,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LabelAlignment {
    Start,
    Center,
}

impl LabelAlignment {
    fn cosmic_align(self) -> Align {
        match self {
            Self::Start => Align::Left,
            Self::Center => Align::Center,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LabelWrap {
    None,
    WordOrGlyph,
}

impl LabelWrap {
    const fn cosmic_wrap(self) -> Wrap {
        match self {
            Self::None => Wrap::None,
            Self::WordOrGlyph => Wrap::WordOrGlyph,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LabelCacheKey {
    text: String,
    width: u32,
    height: u32,
    color: TextColor,
    alignment: LabelAlignment,
    wrap: LabelWrap,
}

#[derive(Clone, Debug)]
struct CachedLabel {
    pixels: Arc<[u8]>,
    bytes: usize,
    last_used_frame: u64,
}

#[derive(Debug)]
struct LabelRasterCache {
    entries: HashMap<LabelCacheKey, CachedLabel>,
    frame: u64,
    bytes: usize,
    max_bytes: usize,
}

impl LabelRasterCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            bytes: 0,
            max_bytes,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &LabelCacheKey) -> Option<Arc<[u8]>> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(Arc::clone(&entry.pixels))
    }

    fn insert(&mut self, key: LabelCacheKey, pixels: Vec<u8>) -> Arc<[u8]> {
        let bytes = pixels.len();
        let pixels = Arc::<[u8]>::from(pixels);
        if let Some(old) = self.entries.insert(
            key.clone(),
            CachedLabel {
                pixels: Arc::clone(&pixels),
                bytes,
                last_used_frame: self.frame,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.evict_if_needed(&key);
        pixels
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn evict_if_needed(&mut self, protected: &LabelCacheKey) {
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

struct TextFrameBuilder<'a> {
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    text_buffer: &'a mut Buffer,
    label_cache: &'a mut LabelRasterCache,
    surface_size: PhysicalSize<u32>,
    max_font_size: f32,
    max_line_height: f32,
    pixels: Vec<u8>,
    draws: Vec<TextDraw>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    labels: usize,
    cache_hits: usize,
    cache_misses: usize,
    raster_us: u128,
}

impl<'a> TextFrameBuilder<'a> {
    fn new(
        font_system: &'a mut FontSystem,
        swash_cache: &'a mut SwashCache,
        text_buffer: &'a mut Buffer,
        label_cache: &'a mut LabelRasterCache,
        surface_size: PhysicalSize<u32>,
        text_scale_factor: f32,
    ) -> Self {
        let max_line_height = (TEXT_LINE_HEIGHT * text_scale_factor).round().max(1.0);
        let max_font_size = (TEXT_FONT_SIZE * max_line_height / TEXT_LINE_HEIGHT).max(1.0);
        text_buffer.set_metrics(Metrics::new(max_font_size, max_line_height));
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        Self {
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            surface_size,
            max_font_size,
            max_line_height,
            pixels: vec![0; (TEXT_ATLAS_WIDTH * 4) as usize],
            draws: Vec::with_capacity(64),
            width: TEXT_ATLAS_WIDTH,
            height: 1,
            cursor_x: TEXT_PADDING,
            cursor_y: TEXT_PADDING,
            row_height: 0,
            labels: 0,
            cache_hits: 0,
            cache_misses: 0,
            raster_us: 0,
        }
    }

    fn push_label(&mut self, label: &str, rect: ViewRect, clip: ViewRect, color: TextColor) {
        self.push_label_aligned(label, rect, clip, color, LabelAlignment::Center);
    }

    fn push_label_aligned(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) {
        self.push_label_aligned_wrapped(
            label,
            rect,
            clip,
            color,
            alignment,
            LabelWrap::WordOrGlyph,
        );
    }

    fn push_label_aligned_no_wrap(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) {
        self.push_label_aligned_wrapped(label, rect, clip, color, alignment, LabelWrap::None);
    }

    fn push_label_aligned_wrapped(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) {
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return;
        };

        let max_label_width = self.width.saturating_sub(TEXT_PADDING * 2).max(1);
        let label_width = (rect.width.ceil().max(1.0) as u32).min(max_label_width);
        let label_height = rect.height.ceil().max(1.0) as u32;
        let key = LabelCacheKey {
            text: label.to_string(),
            width: label_width,
            height: label_height,
            color,
            alignment,
            wrap,
        };
        let label_pixels = if let Some(pixels) = self.label_cache.get(&key) {
            self.cache_hits += 1;
            pixels
        } else {
            self.cache_misses += 1;
            let raster_start = Instant::now();
            let label_pixels =
                self.rasterize_label(label, label_width, label_height, color, alignment, wrap);
            self.raster_us += raster_start.elapsed().as_micros();
            self.label_cache.insert(key, label_pixels)
        };

        let atlas = self.allocate(label_width, label_height);
        self.copy_label_pixels(atlas, label_width, label_height, label_pixels.as_ref());

        let scale_x = label_width as f32 / rect.width.max(1.0);
        let scale_y = label_height as f32 / rect.height.max(1.0);
        let atlas = AtlasRect {
            x: atlas.x + (screen.x - rect.x).max(0.0) * scale_x,
            y: atlas.y + (screen.y - rect.y).max(0.0) * scale_y,
            width: screen.width * scale_x,
            height: screen.height * scale_y,
        };
        self.draws.push(TextDraw { screen, atlas });
        self.labels += 1;
    }

    fn measure_label_cursor_x(
        &mut self,
        label: &str,
        rect: ViewRect,
        cursor: usize,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> f32 {
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return 0.0;
        }
        let max_label_width = self.width.saturating_sub(TEXT_PADDING * 2).max(1);
        let label_width = (rect.width.ceil().max(1.0) as u32).min(max_label_width);
        let label_height = rect.height.ceil().max(1.0) as u32;
        let attrs = Attrs::new().family(Family::SansSerif);
        let metrics =
            text_metrics_for_label_height(label_height, self.max_font_size, self.max_line_height);
        self.text_buffer.set_metrics(metrics);
        self.text_buffer.set_wrap(wrap.cosmic_wrap());
        self.text_buffer
            .set_size(Some(label_width as f32), Some(label_height as f32));
        self.text_buffer.set_text(
            label,
            &attrs,
            Shaping::Advanced,
            Some(alignment.cosmic_align()),
        );
        self.text_buffer.shape_until_scroll(self.font_system, false);
        let cursor = Cursor::new(0, normalized_text_cursor(label, cursor));
        let measured_x = self
            .text_buffer
            .cursor_position(&cursor)
            .map(|(x, _)| x)
            .or_else(|| self.text_buffer.layout_runs().next().map(|run| run.line_w))
            .unwrap_or(0.0);
        measured_x / (label_width as f32 / rect.width.max(1.0))
    }

    fn finish(self) -> TextFrame {
        let height = self.height.max(1);
        let mut vertices = Vec::with_capacity(self.draws.len() * 6);
        for draw in &self.draws {
            push_textured_rect(
                &mut vertices,
                draw.screen,
                draw.atlas,
                self.width,
                height,
                self.surface_size,
            );
        }
        let atlas_bytes = (self.width * height * 4) as usize;
        let cache_entries = self.label_cache.len();
        let cache_bytes = self.label_cache.bytes();
        TextFrame {
            vertices,
            pixels: self.pixels,
            width: self.width,
            height,
            stats: TextFrameStats {
                labels: self.labels,
                quads: self.draws.len(),
                atlas_width: self.width,
                atlas_height: height,
                atlas_bytes,
                cache_hits: self.cache_hits,
                cache_misses: self.cache_misses,
                cache_entries,
                cache_bytes,
                raster_us: self.raster_us,
            },
        }
    }

    fn rasterize_label(
        &mut self,
        label: &str,
        label_width: u32,
        label_height: u32,
        color: TextColor,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> Vec<u8> {
        let mut pixels = vec![0; (label_width * label_height * 4) as usize];
        let attrs = Attrs::new().family(Family::SansSerif);
        let metrics =
            text_metrics_for_label_height(label_height, self.max_font_size, self.max_line_height);
        self.text_buffer.set_metrics(metrics);
        self.text_buffer.set_wrap(wrap.cosmic_wrap());
        self.text_buffer
            .set_size(Some(label_width as f32), Some(label_height as f32));
        self.text_buffer.set_text(
            label,
            &attrs,
            Shaping::Advanced,
            Some(alignment.cosmic_align()),
        );
        self.text_buffer.draw(
            self.font_system,
            self.swash_cache,
            color,
            |x, y, w, h, glyph_color| {
                fill_text_pixels(
                    &mut pixels,
                    label_width,
                    label_height,
                    x,
                    y,
                    w,
                    h,
                    glyph_color,
                );
            },
        );
        pixels
    }

    fn allocate(&mut self, label_width: u32, label_height: u32) -> AtlasRect {
        if self.cursor_x + label_width + TEXT_PADDING > self.width {
            self.cursor_x = TEXT_PADDING;
            self.cursor_y += self.row_height.max(1);
            self.row_height = 0;
        }

        let x = self.cursor_x;
        let y = self.cursor_y;
        self.cursor_x += label_width + TEXT_PADDING;
        self.row_height = self.row_height.max(label_height + TEXT_PADDING);
        self.ensure_height(y + label_height + TEXT_PADDING);

        AtlasRect {
            x: x as f32,
            y: y as f32,
            width: label_width as f32,
            height: label_height as f32,
        }
    }

    fn ensure_height(&mut self, needed_height: u32) {
        if needed_height <= self.height {
            return;
        }
        self.height = needed_height.next_power_of_two();
        self.pixels
            .resize((self.width * self.height * 4) as usize, 0);
    }

    fn copy_label_pixels(
        &mut self,
        atlas: AtlasRect,
        label_width: u32,
        label_height: u32,
        label_pixels: &[u8],
    ) {
        let atlas_x = atlas.x as u32;
        let atlas_y = atlas.y as u32;
        for row in 0..label_height {
            let src_start = (row * label_width * 4) as usize;
            let src_end = src_start + (label_width * 4) as usize;
            let dst_start = (((atlas_y + row) * self.width + atlas_x) * 4) as usize;
            let dst_end = dst_start + (label_width * 4) as usize;
            self.pixels[dst_start..dst_end].copy_from_slice(&label_pixels[src_start..src_end]);
        }
    }
}

struct TextRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    texture_width: u32,
    texture_height: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_buffer: Buffer,
    label_cache: LabelRasterCache,
}

impl TextRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-text-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-wgpu-text-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture = create_text_texture(device, 1, 1);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_text_bind_group(device, &bind_group_layout, &texture_view, &sampler);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-text-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-text-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-text-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TextVertex::layout()],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut font_system = FontSystem::new();
        let mut text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT),
        );
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        let swash_cache = SwashCache::new();
        let label_cache = LabelRasterCache::new(TEXT_LABEL_CACHE_MAX_BYTES);
        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            texture_width: 1,
            texture_height: 1,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &TextFrame) {
        if frame.width != self.texture_width || frame.height != self.texture_height {
            self.texture = create_text_texture(device, frame.width, frame.height);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bind_group = create_text_bind_group(
                device,
                &self.bind_group_layout,
                &self.texture_view,
                &self.sampler,
            );
            self.texture_width = frame.width;
            self.texture_height = frame.height;
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );

        if frame.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = frame.vertices.len().next_power_of_two();
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
        }
        self.vertex_count = frame.vertices.len();
        if !frame.vertices.is_empty() {
            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&frame.vertices),
            );
        }
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0)
    }
}

fn prepare_scene_frame(
    text_renderer: &mut TextRenderer,
    overlay_text_renderer: &mut TextRenderer,
    icon_renderer: &mut IconRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &mut ShellScene,
    size: PhysicalSize<u32>,
) -> SceneFrame {
    text_renderer.label_cache.begin_frame();
    overlay_text_renderer.label_cache.begin_frame();
    icon_renderer.raster_cache.begin_frame();

    let (mut scene_frame, text_frame, overlay_text_frame, icon_frame) = {
        let mut text_builder = TextFrameBuilder::new(
            &mut text_renderer.font_system,
            &mut text_renderer.swash_cache,
            &mut text_renderer.text_buffer,
            &mut text_renderer.label_cache,
            size,
            scene.ui_scale() * scene.zoom_factor(),
        );
        let mut overlay_text_builder = TextFrameBuilder::new(
            &mut overlay_text_renderer.font_system,
            &mut overlay_text_renderer.swash_cache,
            &mut overlay_text_renderer.text_buffer,
            &mut overlay_text_renderer.label_cache,
            size,
            scene.ui_scale(),
        );
        let mut icon_builder = IconFrameBuilder::new(
            &mut icon_renderer.resolver,
            &mut icon_renderer.thumbnails,
            &mut icon_renderer.raster_cache,
            size,
        );
        let scene_frame = scene.build_frame(
            size,
            &mut text_builder,
            &mut icon_builder,
            &mut overlay_text_builder,
        );
        let text_frame = text_builder.finish();
        let overlay_text_frame = overlay_text_builder.finish();
        let icon_frame = icon_builder.finish();
        (scene_frame, text_frame, overlay_text_frame, icon_frame)
    };

    icon_renderer.upload(device, queue, &icon_frame);
    text_renderer.upload(device, queue, &text_frame);
    overlay_text_renderer.upload(device, queue, &overlay_text_frame);
    scene_frame.icon_stats = icon_frame.stats;
    scene_frame.text_stats = text_frame.stats.merged(overlay_text_frame.stats);
    scene_frame
}

fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-wgpu-quad-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<QuadVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct QuadVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl QuadVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

fn create_text_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-wgpu-text-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<TextVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_text_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-wgpu-text-atlas"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_text_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-wgpu-text-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_icon_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-wgpu-icon-atlas"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_icon_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-wgpu-icon-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn push_clipped_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if let Some(rect) = intersect_rect(rect, clip) {
        push_rect(vertices, rect, color, size);
    }
}

fn push_clipped_rounded_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    if radius <= 1.0 {
        push_clipped_rect(vertices, rect, clip, color, size);
        return;
    }

    let middle_height = (rect.height - radius * 2.0).max(0.0);
    if middle_height > 0.0 {
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y + radius,
                width: rect.width,
                height: middle_height,
            },
            clip,
            color,
            size,
        );
    }

    let steps = radius.ceil().clamp(4.0, 16.0) as usize;
    let step_height = radius / steps as f32;
    for step in 0..steps {
        let y = rect.y + step as f32 * step_height;
        let midpoint_y = y + step_height / 2.0;
        let dy = rect.y + radius - midpoint_y;
        let inset = radius - (radius * radius - dy * dy).max(0.0).sqrt();
        let strip_width = rect.width - inset * 2.0;
        if strip_width <= 0.0 {
            continue;
        }
        let top = ViewRect {
            x: rect.x + inset,
            y,
            width: strip_width,
            height: step_height,
        };
        let bottom = ViewRect {
            x: rect.x + inset,
            y: rect.bottom() - (step + 1) as f32 * step_height,
            width: strip_width,
            height: step_height,
        };
        push_clipped_rect(vertices, top, clip, color, size);
        push_clipped_rect(vertices, bottom, clip, color, size);
    }
}

fn push_clipped_rect_outline(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    thickness: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    let thickness = thickness.max(1.0).min(rect.width.min(rect.height) / 2.0);
    let top = ViewRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: thickness,
    };
    let bottom = ViewRect {
        x: rect.x,
        y: rect.bottom() - thickness,
        width: rect.width,
        height: thickness,
    };
    let left = ViewRect {
        x: rect.x,
        y: rect.y + thickness,
        width: thickness,
        height: (rect.height - thickness * 2.0).max(0.0),
    };
    let right = ViewRect {
        x: rect.right() - thickness,
        y: rect.y + thickness,
        width: thickness,
        height: (rect.height - thickness * 2.0).max(0.0),
    };
    push_clipped_rect(vertices, top, clip, color, size);
    push_clipped_rect(vertices, bottom, clip, color, size);
    push_clipped_rect(vertices, left, clip, color, size);
    push_clipped_rect(vertices, right, clip, color, size);
}

fn push_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let left = rect.x / width * 2.0 - 1.0;
    let right = rect.right() / width * 2.0 - 1.0;
    let top = 1.0 - rect.y / height * 2.0;
    let bottom = 1.0 - rect.bottom() / height * 2.0;

    vertices.extend_from_slice(&[
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [left, bottom],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [right, top],
            color,
        },
    ]);
}

fn push_textured_rect(
    vertices: &mut Vec<TextVertex>,
    rect: ViewRect,
    atlas: AtlasRect,
    atlas_width: u32,
    atlas_height: u32,
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || atlas.width <= 0.0 || atlas.height <= 0.0 {
        return;
    }
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let left = rect.x / width * 2.0 - 1.0;
    let right = rect.right() / width * 2.0 - 1.0;
    let top = 1.0 - rect.y / height * 2.0;
    let bottom = 1.0 - rect.bottom() / height * 2.0;

    let atlas_width = atlas_width.max(1) as f32;
    let atlas_height = atlas_height.max(1) as f32;
    let u0 = atlas.x / atlas_width;
    let v0 = atlas.y / atlas_height;
    let u1 = (atlas.x + atlas.width) / atlas_width;
    let v1 = (atlas.y + atlas.height) / atlas_height;

    vertices.extend_from_slice(&[
        TextVertex {
            position: [left, top],
            uv: [u0, v0],
        },
        TextVertex {
            position: [left, bottom],
            uv: [u0, v1],
        },
        TextVertex {
            position: [right, bottom],
            uv: [u1, v1],
        },
        TextVertex {
            position: [left, top],
            uv: [u0, v0],
        },
        TextVertex {
            position: [right, bottom],
            uv: [u1, v1],
        },
        TextVertex {
            position: [right, top],
            uv: [u1, v0],
        },
    ]);
}

fn fill_text_pixels(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: TextColor,
) {
    if color.a() == 0 || w == 0 || h == 0 {
        return;
    }
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = (x.saturating_add(w as i32)).clamp(0, width as i32) as u32;
    let y1 = (y.saturating_add(h as i32)).clamp(0, height as i32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }

    let rgba = color.as_rgba();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let offset = ((yy * width + xx) * 4) as usize;
            blend_pixel(&mut pixels[offset..offset + 4], rgba);
        }
    }
}

fn blend_pixel(dst: &mut [u8], src: [u8; 4]) {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_c = src[channel] as f32 / 255.0;
        let dst_c = dst[channel] as f32 / 255.0;
        let out_c = (src_c * src_a + dst_c * dst_a * (1.0 - src_a)) / out_a;
        dst[channel] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FileIconKind {
    Directory,
    Mime {
        mime: Arc<str>,
        extension: Option<String>,
    },
    PreliminaryFile {
        extension: Option<String>,
    },
    File {
        extension: Option<String>,
    },
    Named {
        icon_name: String,
        fallback: NamedIconFallback,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum NamedIconFallback {
    Service,
    Application,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FileIconRoleCacheKey {
    kind: FileIconKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FileIconPathCacheKey {
    role: FileIconRoleCacheKey,
    size_px: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedFileIcon {
    path: Option<PathBuf>,
}

struct FileIconResolver {
    cached: HashMap<FileIconPathCacheKey, ResolvedFileIcon>,
    pending: HashSet<FileIconPathCacheKey>,
    request_tx: Option<Sender<IconResolveRequest>>,
    result_rx: Receiver<IconResolveResult>,
}

#[derive(Clone, Debug)]
struct IconResolveRequest {
    key: FileIconPathCacheKey,
}

#[derive(Clone, Debug)]
struct IconResolveResult {
    key: FileIconPathCacheKey,
    icon: ResolvedFileIcon,
}

impl FileIconResolver {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-icon-resolver".to_string())
            .spawn(move || icon_resolve_worker(request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            cached: HashMap::new(),
            pending: HashSet::new(),
            request_tx,
            result_rx,
        }
    }

    fn resolve_entry(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        self.drain_results();
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        if let Some(icon) = self.cached.get(&key) {
            return Some(icon.clone());
        }

        if self.pending.insert(key.clone())
            && self
                .request_tx
                .as_ref()
                .is_none_or(|tx| tx.send(IconResolveRequest { key }).is_err())
        {
            self.pending.clear();
        }
        None
    }

    fn resolve_named(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        self.drain_results();
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return None;
        }
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: icon_name.to_string(),
                    fallback,
                },
            },
            size_px: icon_cache_size(icon_size),
        };
        if let Some(icon) = self.cached.get(&key) {
            return Some(icon.clone());
        }

        if self.pending.insert(key.clone())
            && self
                .request_tx
                .as_ref()
                .is_none_or(|tx| tx.send(IconResolveRequest { key }).is_err())
        {
            self.pending.clear();
        }
        None
    }

    fn drain_results(&mut self) -> usize {
        let mut changed = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&result.key);
            self.cached.insert(result.key, result.icon);
            changed += 1;
        }
        changed
    }

    fn has_pending(&mut self) -> bool {
        self.drain_results();
        !self.pending.is_empty()
    }
}

fn icon_resolve_worker(
    request_rx: Receiver<IconResolveRequest>,
    result_tx: Sender<IconResolveResult>,
) {
    let mut theme = IconThemeResolver::default();
    let mime = fika_core::MimeDatabase::shared();
    let mut roles = HashMap::<FileIconRoleCacheKey, FileIconProfile>::new();
    while let Ok(request) = request_rx.recv() {
        let profile = roles
            .entry(request.key.role.clone())
            .or_insert_with(|| file_icon_profile(&request.key.role.kind, mime));
        let icon = file_icon_snapshot(profile, request.key.size_px, &mut theme);
        if result_tx
            .send(IconResolveResult {
                key: request.key,
                icon,
            })
            .is_err()
        {
            break;
        }
    }
}

#[derive(Clone, Debug)]
struct IconThemeResolver {
    roots: Vec<PathBuf>,
    themes: Vec<String>,
    search_order: Option<Vec<String>>,
    inherits_cache: HashMap<String, Vec<String>>,
    path_cache: HashMap<(String, u16), Option<PathBuf>>,
    dir_exists_cache: HashMap<PathBuf, bool>,
    renderable_file_cache: HashMap<PathBuf, bool>,
}

impl Default for IconThemeResolver {
    fn default() -> Self {
        Self {
            roots: icon_theme_roots(),
            themes: icon_theme_names(),
            search_order: None,
            inherits_cache: HashMap::new(),
            path_cache: HashMap::new(),
            dir_exists_cache: HashMap::new(),
            renderable_file_cache: HashMap::new(),
        }
    }
}

impl IconThemeResolver {
    fn find(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        let key = (icon_name.to_string(), desired_size);
        if let Some(path) = self.path_cache.get(&key) {
            return path.clone();
        }

        let path = self.find_uncached(icon_name, desired_size);
        self.path_cache.insert(key, path.clone());
        path
    }

    fn first_existing(
        &mut self,
        icon_names: &[String],
        desired_size: u16,
    ) -> Option<(String, PathBuf)> {
        icon_names.iter().find_map(|name| {
            self.find(name, desired_size)
                .map(|path| (name.clone(), path))
        })
    }

    fn find_uncached(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        if let Some(path) = absolute_icon_candidate(icon_name)
            && self.is_renderable_icon_file(&path)
        {
            return Some(path);
        }

        let roots = self.roots.clone();
        for theme in self.theme_search_order() {
            for root in &roots {
                let theme_root = root.join(&theme);
                if let Some(path) = self.find_icon_in_theme(&theme_root, icon_name, desired_size) {
                    return Some(path);
                }
            }
        }

        [
            Path::new("/usr/share/pixmaps"),
            Path::new("/usr/local/share/pixmaps"),
        ]
        .into_iter()
        .find_map(|root| self.find_icon_direct(root, icon_name))
    }

    fn theme_search_order(&mut self) -> Vec<String> {
        if let Some(search_order) = &self.search_order {
            return search_order.clone();
        }
        let mut themes = Vec::new();
        for theme in self.themes.clone() {
            self.push_theme_and_inherits(theme, &mut themes, 0);
        }
        self.search_order = Some(themes.clone());
        themes
    }

    fn push_theme_and_inherits(&mut self, theme: String, themes: &mut Vec<String>, depth: usize) {
        if depth > 8 || theme.is_empty() {
            return;
        }
        let already_seen = themes.iter().any(|existing| existing == &theme);
        push_unique_icon_theme(themes, &theme);
        if already_seen {
            return;
        }
        for inherited in self.inherited_themes(&theme) {
            self.push_theme_and_inherits(inherited, themes, depth + 1);
        }
    }

    fn inherited_themes(&mut self, theme: &str) -> Vec<String> {
        if let Some(inherited) = self.inherits_cache.get(theme) {
            return inherited.clone();
        }
        let mut inherited = Vec::new();
        for root in &self.roots {
            let Ok(contents) = fs::read_to_string(root.join(theme).join("index.theme")) else {
                continue;
            };
            for theme in parse_icon_theme_inherits(&contents) {
                push_unique_icon_theme(&mut inherited, &theme);
            }
        }
        self.inherits_cache
            .insert(theme.to_string(), inherited.clone());
        inherited
    }

    fn find_icon_in_theme(
        &mut self,
        theme_root: &Path,
        icon_name: &str,
        desired_size: u16,
    ) -> Option<PathBuf> {
        const CATEGORIES: &[&str] = &[
            "places",
            "mimetypes",
            "apps",
            "actions",
            "devices",
            "status",
        ];
        if !self.dir_exists(theme_root) {
            return None;
        }
        if let Some(path) = self.find_icon_direct(theme_root, icon_name) {
            return Some(path);
        }
        for size in preferred_icon_size_dirs(desired_size) {
            for category in CATEGORIES {
                for base in [
                    theme_root.join(&size).join(category),
                    theme_root.join(category).join(&size),
                ] {
                    if let Some(path) = self.find_icon_direct(&base, icon_name) {
                        return Some(path);
                    }
                }
            }
        }
        for category in CATEGORIES {
            if let Some(path) = self.find_icon_direct(&theme_root.join(category), icon_name) {
                return Some(path);
            }
        }
        None
    }

    fn find_icon_direct(&mut self, root: &Path, icon_name: &str) -> Option<PathBuf> {
        if !self.dir_exists(root) {
            return None;
        }
        ["png", "svg", "webp", "jpg", "jpeg", "bmp", "gif", "ico"]
            .into_iter()
            .map(|extension| root.join(format!("{icon_name}.{extension}")))
            .find(|path| self.is_renderable_icon_file(path))
    }

    fn dir_exists(&mut self, path: &Path) -> bool {
        if let Some(exists) = self.dir_exists_cache.get(path) {
            return *exists;
        }
        let exists = path.is_dir();
        self.dir_exists_cache.insert(path.to_path_buf(), exists);
        exists
    }

    fn is_renderable_icon_file(&mut self, path: &Path) -> bool {
        if let Some(is_renderable) = self.renderable_file_cache.get(path) {
            return *is_renderable;
        }
        let is_renderable = is_renderable_icon_file(path);
        self.renderable_file_cache
            .insert(path.to_path_buf(), is_renderable);
        is_renderable
    }
}

fn file_icon_path_cache_key(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size: f32,
) -> FileIconPathCacheKey {
    FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: file_icon_kind(path, is_dir, mime_type, mime_magic_checked),
        },
        size_px: icon_cache_size(icon_size),
    }
}

fn file_icon_kind(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
) -> FileIconKind {
    if is_dir {
        return FileIconKind::Directory;
    }
    let extension = file_extension(path);
    if !mime_magic_checked && mime_type.as_deref() == Some(fika_core::GENERIC_BINARY_MIME) {
        return FileIconKind::PreliminaryFile { extension };
    }
    match mime_type {
        Some(mime) => FileIconKind::Mime { mime, extension },
        None => FileIconKind::File { extension },
    }
}

fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn icon_cache_size(icon_size: f32) -> u16 {
    icon_size.round().clamp(16.0, 256.0) as u16
}

fn file_icon_snapshot(
    profile: &FileIconProfile,
    desired_size: u16,
    theme: &mut IconThemeResolver,
) -> ResolvedFileIcon {
    let path = theme
        .first_existing(&profile.icon_candidates, desired_size)
        .or_else(|| theme.first_existing(&profile.generic_candidates, desired_size))
        .or_else(|| {
            theme.first_existing(
                &[
                    "unknown".to_string(),
                    "application-octet-stream".to_string(),
                ],
                desired_size,
            )
        })
        .map(|(_, path)| path);

    ResolvedFileIcon { path }
}

struct FileIconProfile {
    icon_candidates: Vec<String>,
    generic_candidates: Vec<String>,
}

fn file_icon_profile(kind: &FileIconKind, mime: &fika_core::MimeDatabase) -> FileIconProfile {
    let (icon_candidates, generic_candidates) = match kind {
        FileIconKind::Directory => (
            vec!["folder".to_string(), "inode-directory".to_string()],
            Vec::new(),
        ),
        FileIconKind::Mime {
            mime: mime_name,
            extension,
        } => (
            mime_icon_candidates(mime_name, extension.as_deref(), mime),
            mime_generic_icon_candidates(mime_name, mime),
        ),
        FileIconKind::PreliminaryFile { extension } => (
            preliminary_file_icon_candidates(extension.as_deref(), mime),
            Vec::new(),
        ),
        FileIconKind::File { extension } => (
            fallback_file_icon_candidates(extension.as_deref()),
            mime_generic_icon_candidates(fika_core::GENERIC_BINARY_MIME, mime),
        ),
        FileIconKind::Named {
            icon_name,
            fallback,
        } => named_icon_candidates(icon_name, *fallback),
    };

    FileIconProfile {
        icon_candidates,
        generic_candidates,
    }
}

fn mime_icon_candidates(
    mime_name: &str,
    extension: Option<&str>,
    mime: &fika_core::MimeDatabase,
) -> Vec<String> {
    let mut candidates = Vec::new();

    if mime_name == fika_core::GENERIC_BINARY_MIME {
        for icon_name in fallback_file_icon_candidates(extension) {
            push_icon_candidate(&mut candidates, icon_name);
        }
        return candidates;
    }

    for icon_name in mime_theme_icon_candidates(mime_name, extension) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some(icon_name) = mime.icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_generic_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = mime.generic_icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn mime_theme_icon_candidates(mime_name: &str, extension: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = fika_core::mime_icon_name(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some((family, subtype)) = mime_name.split_once('/')
        && family == "text"
    {
        let subtype = subtype.strip_prefix("x-").unwrap_or(subtype);
        if !subtype.is_empty() {
            push_icon_candidate(&mut candidates, format!("text-x-{subtype}"));
        }
        if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
            push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        }
    }
    candidates
}

fn fallback_file_icon_candidates(extension: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
        push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        push_icon_candidate(&mut candidates, format!("application-x-{extension}"));
    }
    push_icon_candidate(&mut candidates, "application-octet-stream");
    candidates
}

fn preliminary_file_icon_candidates(
    extension: Option<&str>,
    mime: &fika_core::MimeDatabase,
) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(extension) = extension.filter(|extension| !extension.is_empty()) {
        if let Some(mime_name) = mime.mime_for_extension(extension) {
            for icon_name in mime_theme_icon_candidates(mime_name, Some(extension)) {
                push_icon_candidate(&mut candidates, icon_name);
            }
        }
        push_icon_candidate(&mut candidates, format!("text-x-{extension}"));
        push_icon_candidate(&mut candidates, format!("application-x-{extension}"));
    }
    push_icon_candidate(&mut candidates, "text-x-generic");
    push_icon_candidate(&mut candidates, "unknown");
    candidates
}

fn push_icon_candidate(candidates: &mut Vec<String>, icon_name: impl Into<String>) {
    let icon_name = icon_name.into();
    if !candidates.iter().any(|existing| existing == &icon_name) {
        candidates.push(icon_name);
    }
}

fn named_icon_candidates(
    icon_name: &str,
    fallback: NamedIconFallback,
) -> (Vec<String>, Vec<String>) {
    let mut candidates = Vec::new();
    push_icon_candidate(&mut candidates, icon_name.trim());
    let generic = match fallback {
        NamedIconFallback::Service => ["configure", "preferences-system", "system-run"].as_slice(),
        NamedIconFallback::Application => [
            "application-x-executable",
            "system-run",
            "application-default-icon",
        ]
        .as_slice(),
    }
    .iter()
    .map(|candidate| (*candidate).to_string())
    .collect();
    (candidates, generic)
}

fn absolute_icon_candidate(icon_name: &str) -> Option<PathBuf> {
    let path = Path::new(icon_name);
    path.is_absolute().then(|| path.to_path_buf())
}

fn icon_theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(home).join(".local/share/icons"));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(data_home).join("icons"));
    }

    let data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        push_unique_icon_path(&mut roots, Path::new(dir).join("icons"));
    }
    push_unique_icon_path(&mut roots, PathBuf::from("/usr/share/icons"));
    roots
}

fn icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for theme in configured_icon_theme_names() {
        push_unique_icon_theme(&mut themes, &theme);
    }
    if env::var_os("KDE_FULL_SESSION").is_some()
        || env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| desktop.to_ascii_lowercase().contains("kde"))
            .unwrap_or(false)
    {
        push_unique_icon_theme(&mut themes, "breeze");
        push_unique_icon_theme(&mut themes, "breeze-dark");
    }
    for key in [
        "GTK_THEME",
        "ICON_THEME",
        "DESKTOP_SESSION",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(value) = env::var(key) {
            for part in value.split([':', ';']) {
                let theme = part.trim();
                if !theme.is_empty() {
                    push_unique_icon_theme(&mut themes, theme);
                }
            }
        }
    }
    for fallback in [
        "breeze",
        "breeze-dark",
        "Papirus",
        "Papirus-Dark",
        "Papirus-Light",
        "Adwaita",
        "hicolor",
    ] {
        push_unique_icon_theme(&mut themes, fallback);
    }
    themes
}

fn configured_icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for path in icon_theme_config_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for theme in parse_configured_icon_theme_names(&contents) {
            push_unique_icon_theme(&mut themes, &theme);
        }
    }
    themes
}

fn icon_theme_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        let config_home = PathBuf::from(config_home);
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
    }
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        let home = PathBuf::from(home);
        let config_home = home.join(".config");
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
        push_unique_icon_path(&mut paths, home.join(".gtkrc-2.0"));
    }
    paths
}

fn parse_configured_icon_theme_names(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    let mut in_icons_section = false;
    let mut in_icon_theme_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            in_icons_section = section.eq_ignore_ascii_case("Icons");
            in_icon_theme_section = section.eq_ignore_ascii_case("Icon Theme");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("gtk-icon-theme-name")
            || (in_icons_section && key.eq_ignore_ascii_case("Theme"))
            || (in_icon_theme_section && key.eq_ignore_ascii_case("Name"))
        {
            let theme = value.trim().trim_matches('"');
            if !theme.is_empty() {
                push_unique_icon_theme(&mut themes, theme);
            }
        }
    }
    themes
}

fn parse_icon_theme_inherits(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Inherits" {
            continue;
        }
        for theme in value
            .split(',')
            .map(str::trim)
            .filter(|theme| !theme.is_empty())
        {
            push_unique_icon_theme(&mut themes, theme);
        }
    }
    themes
}

fn preferred_icon_size_dirs(desired_size: u16) -> Vec<String> {
    let mut dirs = Vec::new();
    let fixed_sizes = [256u16, 128, 96, 64, 48, 32, 24, 22, 16];
    let desired = desired_size.max(16);
    let mut ordered = fixed_sizes.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|size| size.abs_diff(desired));
    for size in ordered {
        push_icon_size_dir(&mut dirs, format!("{size}x{size}"));
        push_icon_size_dir(&mut dirs, size.to_string());
    }
    push_icon_size_dir(&mut dirs, "scalable".to_string());
    push_icon_size_dir(&mut dirs, "symbolic".to_string());
    dirs
}

fn push_icon_size_dir(dirs: &mut Vec<String>, value: String) {
    if !dirs.iter().any(|existing| existing == &value) {
        dirs.push(value);
    }
}

fn is_renderable_icon_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }

    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "webp" | "jpg" | "jpeg" | "bmp" | "gif" | "ico")
    )
}

fn push_unique_icon_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn push_unique_icon_theme(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn rasterize_icon(path: &Path, target_size: u32) -> Option<IconRaster> {
    let target_size = target_size.clamp(16, 256);
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => rasterize_svg_icon(path, target_size),
        _ => rasterize_bitmap_icon(path, target_size),
    }
}

fn rasterize_bitmap_icon(path: &Path, target_size: u32) -> Option<IconRaster> {
    let image = image::open(path).ok()?.into_rgba8();
    let source_width = image.width();
    let source_height = image.height();
    if source_width == 0 || source_height == 0 {
        return None;
    }

    let (draw_width, draw_height) = fit_size(source_width, source_height, target_size);
    let resized = image::imageops::resize(
        &image,
        draw_width,
        draw_height,
        image::imageops::FilterType::Lanczos3,
    );
    let mut pixels = vec![0; (target_size * target_size * 4) as usize];
    let x = (target_size - draw_width) / 2;
    let y = (target_size - draw_height) / 2;
    copy_rgba_into(
        resized.as_raw(),
        draw_width,
        draw_height,
        &mut pixels,
        target_size,
        x,
        y,
    );
    Some(IconRaster {
        pixels: Arc::from(pixels),
        width: target_size,
        height: target_size,
    })
}

fn rasterize_svg_icon(path: &Path, target_size: u32) -> Option<IconRaster> {
    let data = fs::read(path).ok()?;
    let options = usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(&data, &options).ok()?;
    let size = tree.size();
    let source_width = size.width();
    let source_height = size.height();
    if source_width <= 0.0 || source_height <= 0.0 {
        return None;
    }

    let scale = (target_size as f32 / source_width).min(target_size as f32 / source_height);
    let draw_width = ((source_width * scale).ceil() as u32).clamp(1, target_size);
    let draw_height = ((source_height * scale).ceil() as u32).clamp(1, target_size);
    let mut pixmap = tiny_skia::Pixmap::new(draw_width, draw_height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let mut pixels = vec![0; (target_size * target_size * 4) as usize];
    let mut source = pixmap.take();
    unpremultiply_rgba(&mut source);
    let x = (target_size - draw_width) / 2;
    let y = (target_size - draw_height) / 2;
    copy_rgba_into(
        &source,
        draw_width,
        draw_height,
        &mut pixels,
        target_size,
        x,
        y,
    );
    Some(IconRaster {
        pixels: Arc::from(pixels),
        width: target_size,
        height: target_size,
    })
}

fn fit_size(source_width: u32, source_height: u32, target_size: u32) -> (u32, u32) {
    let scale =
        (target_size as f32 / source_width as f32).min(target_size as f32 / source_height as f32);
    let width = ((source_width as f32 * scale).round() as u32).clamp(1, target_size);
    let height = ((source_height as f32 * scale).round() as u32).clamp(1, target_size);
    (width, height)
}

fn copy_rgba_into(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    target: &mut [u8],
    target_width: u32,
    target_x: u32,
    target_y: u32,
) {
    for row in 0..source_height {
        let src_start = (row * source_width * 4) as usize;
        let src_end = src_start + (source_width * 4) as usize;
        let dst_start = (((target_y + row) * target_width + target_x) * 4) as usize;
        let dst_end = dst_start + (source_width * 4) as usize;
        target[dst_start..dst_end].copy_from_slice(&source[src_start..src_end]);
    }
}

fn unpremultiply_rgba(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u16 * 255) / alpha as u16).min(255) as u8;
        }
    }
}

fn intersect_rect(rect: ViewRect, clip: ViewRect) -> Option<ViewRect> {
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    (right > x && bottom > y).then_some(ViewRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

fn inset_rect(rect: ViewRect, inset: f32) -> Option<ViewRect> {
    let inset = inset.max(0.0);
    let width = rect.width - inset * 2.0;
    let height = rect.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: rect.x + inset,
        y: rect.y + inset,
        width,
        height,
    })
}

fn inset_content_scrollbar_slot(slot: ViewRect, scale_factor: f32) -> Option<ViewRect> {
    let inset = (CONTENT_SCROLLBAR_PADDING * scale_factor).round().max(1.0);
    let width = slot.width - inset * 2.0;
    let height = slot.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: slot.x + inset,
        y: slot.y + inset,
        width,
        height,
    })
}

fn scrollbar_scroll_from_pointer(
    pointer_axis: f32,
    grab_offset: f32,
    track_origin: f32,
    track_extent: f32,
    thumb_extent: f32,
    max_scroll: f32,
) -> f32 {
    if max_scroll <= f32::EPSILON {
        return 0.0;
    }
    let travel = (track_extent - thumb_extent).max(0.0);
    if travel <= f32::EPSILON {
        return 0.0;
    }
    let thumb_origin = (pointer_axis - grab_offset).clamp(track_origin, track_origin + travel);
    ((thumb_origin - track_origin) / travel * max_scroll).clamp(0.0, max_scroll)
}

fn screen_to_content_point(
    point: ViewPoint,
    scroll_offset: ViewPoint,
    content_rect: ViewRect,
) -> Option<ViewPoint> {
    if !content_rect.contains(point) {
        return None;
    }
    Some(ViewPoint {
        x: point.x - content_rect.x + scroll_offset.x,
        y: point.y - content_rect.y + scroll_offset.y,
    })
}

fn clamped_screen_to_content_point(
    point: ViewPoint,
    scroll_offset: ViewPoint,
    content_rect: ViewRect,
) -> ViewPoint {
    let y = point.y.clamp(content_rect.y, content_rect.bottom());
    let x = point.x.clamp(content_rect.x, content_rect.right());
    ViewPoint {
        x: x - content_rect.x + scroll_offset.x,
        y: y - content_rect.y + scroll_offset.y,
    }
}

fn pane_content_rect_to_screen(rect: ViewRect, projection: &ShellPaneProjection<'_>) -> ViewRect {
    ViewRect {
        x: rect.x - projection.view.scroll_x + projection.geometry.content.x,
        y: rect.y - projection.view.scroll_y + projection.geometry.content.y,
        width: rect.width,
        height: rect.height,
    }
}

fn point_distance(left: ViewPoint, right: ViewPoint) -> f32 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}

fn place_row_background_color(active: bool, hovered: bool) -> [f32; 4] {
    match (active, hovered) {
        (true, true) => [0.918, 0.945, 1.000, 1.0],
        (true, false) => [0.918, 0.945, 1.000, 1.0],
        (false, true) => [0.933, 0.953, 0.973, 1.0],
        (false, false) => [0.0, 0.0, 0.0, 0.0],
    }
}

fn push_scrollbar(
    vertices: &mut Vec<QuadVertex>,
    track: ViewRect,
    thumb: ViewRect,
    clip: ViewRect,
    size: PhysicalSize<u32>,
) {
    let track_radius = track.width.min(track.height) / 2.0;
    let thumb_radius = thumb.width.min(thumb.height) / 2.0;
    push_clipped_rounded_rect(
        vertices,
        track,
        clip,
        track_radius,
        [0.902, 0.922, 0.945, 1.0],
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        thumb,
        clip,
        thumb_radius,
        [0.596, 0.647, 0.714, 1.0],
        size,
    );
}

fn push_context_menu_shadow(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let scale = scale_factor.max(1.0);
    let radius = (6.0 * scale).round().max(1.0);
    for (dy, spread, alpha) in [(1.0, 1.0, 0.10), (3.0, 3.0, 0.08), (7.0, 8.0, 0.05)] {
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: rect.x - (spread * scale).round(),
                y: rect.y + (dy * scale).round() - (spread * scale).round(),
                width: rect.width + (spread * 2.0 * scale).round(),
                height: rect.height + (spread * 2.0 * scale).round(),
            },
            clip,
            radius + (spread * scale).round(),
            [0.000, 0.000, 0.000, alpha],
            size,
        );
    }
}

fn push_context_menu_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    glyph: ContextMenuGlyph,
    fg: [f32; 4],
    bg: [f32; 4],
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    match glyph {
        ContextMenuGlyph::Open => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 6.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 7.0, 10.0, 7.0, 2.0, fg, size);
        }
        ContextMenuGlyph::OpenWith => {
            for (x, y) in [(5.0, 5.0), (10.0, 5.0), (5.0, 10.0), (10.0, 10.0)] {
                push_context_icon_piece(vertices, rect, clip, x, y, 3.0, 3.0, 1.0, fg, size);
            }
        }
        ContextMenuGlyph::Pane => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 4.0, 10.0, 10.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 5.0, 1.0, 8.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 8.0, 8.0, 1.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Hidden => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 6.0, 4.0, 7.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 3.0, 1.0, bg, size);
        }
        ContextMenuGlyph::Copy => {
            push_context_icon_piece(vertices, rect, clip, 6.0, 4.0, 7.0, 9.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 6.0, 7.0, 9.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 7.0, 5.0, 7.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Cut => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 5.0, 3.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 11.0, 3.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 6.0, 6.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 11.0, 6.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Location => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 4.0, 8.0, 8.0, 4.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 7.0, 2.0, 2.0, 1.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 11.0, 2.0, 4.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Rename => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 10.0, 8.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 8.0, 3.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 14.0, 9.0, 1.0, 0.0, fg, size);
        }
        ContextMenuGlyph::Trash => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 6.0, 8.0, 6.0, 7.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 9.0, 1.0, 5.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 10.0, 9.0, 1.0, 5.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Restore => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 6.0, 11.0, 7.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 8.0, 2.0, 4.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 9.0, 7.0, 5.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Delete => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 11.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 5.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 2.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Place => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 4.0, 8.0, 11.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 11.0, 4.0, 4.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Create => {
            push_context_icon_piece(vertices, rect, clip, 8.0, 4.0, 2.0, 10.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Paste => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 10.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 4.0, 4.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 9.0, 4.0, 1.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 12.0, 4.0, 1.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Select => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 5.0, 2.0, 8.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Refresh => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 9.0, 2.0, 4.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 10.0, 4.0, 4.0, 4.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Properties => {
            push_context_icon_piece(vertices, rect, clip, 8.0, 4.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 6.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 14.0, 4.0, 1.0, 0.0, fg, size);
        }
        ContextMenuGlyph::Remove => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 2.0, 1.0, fg, size);
        }
    }
}

fn push_context_icon_piece(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    let unit = bounds.width.min(bounds.height) / CONTEXT_MENU_ICON_SIZE;
    let piece = ViewRect {
        x: bounds.x + (x * unit).round(),
        y: bounds.y + (y * unit).round(),
        width: (width * unit).round().max(1.0),
        height: (height * unit).round().max(1.0),
    };
    push_clipped_rounded_rect(vertices, piece, clip, (radius * unit).round(), color, size);
}

fn push_location_bar_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    active: bool,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let fg = if active {
        [0.122, 0.310, 0.749, 1.0]
    } else {
        [0.294, 0.318, 0.357, 1.0]
    };
    let bg = if active {
        [0.918, 0.945, 1.000, 1.0]
    } else {
        [0.933, 0.945, 0.961, 1.0]
    };
    push_clipped_rounded_rect(
        vertices,
        bounds,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    let s = |value: f32| {
        (value * bounds.width.min(bounds.height) / 18.0)
            .round()
            .max(1.0)
    };
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(6.0),
            width: s(7.0),
            height: s(3.0),
        },
        clip,
        s(1.0),
        fg,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(4.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(8.0),
            height: bounds.height - s(11.0),
        },
        clip,
        s(2.0),
        fg,
        size,
    );
}

fn push_place_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    place: &ShellPlace,
    active: bool,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let (fg, bg) = place_icon_colors(place, active);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (6.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    if place.trash {
        push_place_trash_icon(vertices, rect, clip, fg, scale_factor, size);
    } else if place.root || place.network || place.marker == "D" || place.marker == "/" {
        push_place_drive_icon(vertices, rect, clip, fg, scale_factor, size);
    } else {
        push_place_folder_icon(vertices, rect, clip, fg, scale_factor, size);
    }
}

fn place_icon_colors(place: &ShellPlace, active: bool) -> ([f32; 4], [f32; 4]) {
    if active {
        return ([0.122, 0.310, 0.749, 1.0], [0.918, 0.945, 1.000, 1.0]);
    }
    if place.trash {
        ([0.690, 0.282, 0.282, 1.0], [1.000, 0.922, 0.922, 1.0])
    } else if place.network {
        ([0.184, 0.435, 0.929, 1.0], [0.918, 0.945, 1.000, 1.0])
    } else if place.root {
        ([0.294, 0.318, 0.357, 1.0], [0.902, 0.922, 0.945, 1.0])
    } else if place.editable {
        ([0.192, 0.486, 0.310, 1.0], [0.910, 0.973, 0.925, 1.0])
    } else {
        ([0.749, 0.435, 0.047, 1.0], [1.000, 0.953, 0.855, 1.0])
    }
}

fn place_icon_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor).round().max(1.0)
}

fn push_place_folder_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: [f32; 4],
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(6.0),
            width: s(7.0),
            height: s(3.0),
        },
        clip,
        s(1.0),
        fg,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(4.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(8.0),
            height: bounds.height - s(11.0),
        },
        clip,
        s(2.0),
        fg,
        size,
    );
}

fn push_place_drive_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: [f32; 4],
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    let body = ViewRect {
        x: bounds.x + s(4.0),
        y: bounds.y + s(5.0),
        width: bounds.width - s(8.0),
        height: bounds.height - s(10.0),
    };
    push_clipped_rounded_rect(vertices, body, clip, s(2.0), fg, size);
    push_clipped_rect(
        vertices,
        ViewRect {
            x: body.x + s(3.0),
            y: body.bottom() - s(4.0),
            width: body.width - s(6.0),
            height: s(1.0),
        },
        clip,
        [1.000, 1.000, 1.000, 0.75],
        size,
    );
}

fn push_place_trash_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: [f32; 4],
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    push_clipped_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(6.0),
            y: bounds.y + s(5.0),
            width: bounds.width - s(12.0),
            height: s(2.0),
        },
        clip,
        fg,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(10.0),
            height: bounds.height - s(12.0),
        },
        clip,
        s(2.0),
        fg,
        size,
    );
}

fn item_background_color(selected: bool, hovered: bool) -> [f32; 4] {
    match (selected, hovered) {
        (true, true) => [0.812, 0.890, 1.000, 1.0],
        (true, false) => [0.859, 0.918, 0.996, 1.0],
        (false, true) => [0.918, 0.945, 1.000, 1.0],
        (false, false) => [0.0, 0.0, 0.0, 0.0],
    }
}

fn details_row_background_color(selected: bool, hovered: bool, index: usize) -> [f32; 4] {
    match (selected, hovered, index % 2 == 0) {
        (true, true, _) => [0.812, 0.890, 1.000, 1.0],
        (true, false, _) => [0.859, 0.918, 0.996, 1.0],
        (false, true, _) => [0.918, 0.945, 1.000, 1.0],
        (false, false, true) => [1.000, 1.000, 1.000, 1.0],
        (false, false, false) => [0.973, 0.980, 0.988, 1.0],
    }
}

fn view_mode_clear_color(view_mode: ShellViewMode) -> wgpu::Color {
    let [r, g, b, a] = view_mode_surface_color(view_mode);
    wgpu::Color {
        r: r as f64,
        g: g as f64,
        b: b as f64,
        a: a as f64,
    }
}

fn view_mode_surface_color(view_mode: ShellViewMode) -> [f32; 4] {
    let _ = view_mode;
    [0.973, 0.976, 0.984, 1.0]
}

fn view_mode_content_color(view_mode: ShellViewMode) -> [f32; 4] {
    let _ = view_mode;
    [1.000, 1.000, 1.000, 1.0]
}

fn view_mode_badge_color(view_mode: ShellViewMode) -> [f32; 4] {
    let _ = view_mode;
    [0.184, 0.435, 0.929, 1.0]
}

fn chrome_color() -> [f32; 4] {
    [0.973, 0.976, 0.984, 1.0]
}

fn sidebar_color() -> [f32; 4] {
    [0.973, 0.976, 0.984, 1.0]
}

fn details_size_label(entry: &Entry) -> String {
    if entry.is_dir {
        "Folder".to_string()
    } else if !entry.metadata_complete && entry.size_bytes == 0 && entry.modified_secs.is_none() {
        "-".to_string()
    } else {
        format_size(entry.size_bytes)
    }
}

fn push_fallback_icon(
    vertices: &mut Vec<QuadVertex>,
    entry: &Entry,
    icon_rect: ViewRect,
    content_clip: ViewRect,
    size: PhysicalSize<u32>,
) {
    if entry.is_dir {
        let tab = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.12,
            y: icon_rect.y + icon_rect.height * 0.16,
            width: icon_rect.width * 0.42,
            height: icon_rect.height * 0.18,
        };
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.08,
            y: icon_rect.y + icon_rect.height * 0.28,
            width: icon_rect.width * 0.84,
            height: icon_rect.height * 0.56,
        };
        push_clipped_rect(vertices, tab, content_clip, [0.96, 0.70, 0.26, 1.0], size);
        push_clipped_rect(vertices, body, content_clip, [0.90, 0.58, 0.18, 1.0], size);
    } else {
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.18,
            y: icon_rect.y + icon_rect.height * 0.10,
            width: icon_rect.width * 0.64,
            height: icon_rect.height * 0.78,
        };
        let stripe = ViewRect {
            x: body.x,
            y: body.y,
            width: body.width,
            height: body.height * 0.22,
        };
        push_clipped_rect(vertices, body, content_clip, file_color(entry), size);
        push_clipped_rect(
            vertices,
            stripe,
            content_clip,
            [0.76, 0.80, 0.86, 1.0],
            size,
        );
    }
}

fn file_color(entry: &Entry) -> [f32; 4] {
    let mime = entry.mime_type.as_deref().unwrap_or_default();
    if mime.starts_with("image/") {
        [0.50, 0.70, 0.56, 1.0]
    } else if mime.starts_with("video/") || mime.starts_with("audio/") {
        [0.69, 0.55, 0.82, 1.0]
    } else if mime.contains("text") || mime.contains("json") || mime.contains("xml") {
        [0.38, 0.60, 0.84, 1.0]
    } else {
        [0.55, 0.60, 0.68, 1.0]
    }
}

fn required_compact_item_width(options: CompactLayoutOptions, text_width: f32) -> f32 {
    (options.padding * 2.0 + options.icon_size + options.text_gap + text_width).round()
}

fn compact_entry_text_width(entry: &Entry, scale_factor: f32) -> f32 {
    let unit_width = f32::from(entry.name_width_units) * 8.5;
    let estimated_width = entry
        .name
        .chars()
        .map(estimated_name_char_width)
        .sum::<f32>();
    unit_width.max(estimated_width) * scale_factor
}

fn estimated_name_char_width(ch: char) -> f32 {
    match ch {
        '\u{200B}' => 0.0,
        '\u{2026}' => 8.0,
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' | '`' | '|' => 4.0,
        ' ' | '-' | '_' => 5.0,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | '#' => 11.0,
        'A'..='Z' => 9.0,
        '0'..='9' => 8.0,
        ch if ch.is_ascii() => 7.5,
        _ => 14.0,
    }
}

#[cfg(test)]
fn context_menu_rect(menu: &ShellContextMenu, size: PhysicalSize<u32>) -> ViewRect {
    context_menu_rect_scaled(menu, size, 1.0)
}

fn context_menu_rect_scaled(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let menu_width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let menu_height = (vertical_padding * 2.0 + context_menu_items(menu).len() as f32 * row_height)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: popup_menu_axis(menu.position.x, menu_width, width, margin),
        y: popup_menu_axis(menu.position.y, menu_height, height, margin),
        width: menu_width,
        height: menu_height,
    }
}

fn context_menu_row_at_screen_point(
    menu: &ShellContextMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<usize> {
    let rect = context_menu_rect_scaled(menu, size, scale_factor);
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < context_menu_items(menu).len()).then_some(row)
}

fn context_menu_submenu_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<ViewRect> {
    let submenu = menu.active_submenu?;
    let parent_row = menu.hovered_row?;
    let submenu_len = context_submenu_actions(submenu, menu).len();
    if submenu_len == 0 {
        return None;
    }
    let root = context_menu_rect_scaled(menu, size, scale_factor);
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let submenu_width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let submenu_height = (vertical_padding * 2.0 + submenu_len as f32 * row_height)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    let preferred_x = root.right() - 1.0;
    let x = if preferred_x + submenu_width <= width - margin {
        preferred_x
    } else {
        (root.x - submenu_width + 1.0).max(margin.min((width - submenu_width).max(0.0)))
    };
    let anchor_y = root.y + vertical_padding + parent_row as f32 * row_height;
    Some(ViewRect {
        x,
        y: popup_menu_axis(anchor_y, submenu_height, height, margin),
        width: submenu_width,
        height: submenu_height,
    })
}

fn context_submenu_row_at_screen_point(
    menu: &ShellContextMenu,
    submenu: ShellContextSubmenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<usize> {
    let rect = context_menu_submenu_rect(menu, size, scale_factor)?;
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < context_submenu_actions(submenu, menu).len()).then_some(row)
}

fn scaled_context_menu_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor.max(1.0)).round().max(1.0)
}

fn popup_menu_axis(anchor: f32, size: f32, viewport_size: f32, margin: f32) -> f32 {
    let min = margin.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - margin).max(min);
    let forward = anchor.clamp(min, max);
    if anchor + size <= viewport_size - margin {
        return forward;
    }
    let flipped = anchor - size;
    if flipped >= min {
        return flipped.min(max);
    }
    forward
}

#[cfg(test)]
fn properties_overlay_rect(overlay: &ShellPropertiesOverlay, size: PhysicalSize<u32>) -> ViewRect {
    properties_overlay_rect_scaled(overlay, size, 1.0)
}

fn properties_overlay_rect_scaled(
    overlay: &ShellPropertiesOverlay,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(PROPERTIES_OVERLAY_MARGIN, scale_factor);
    let overlay_width = scaled_dialog_metric(PROPERTIES_OVERLAY_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let overlay_height = (scaled_dialog_metric(PROPERTIES_TITLE_HEIGHT, scale_factor)
        + scaled_dialog_metric(22.0, scale_factor)
        + overlay.rows.len() as f32 * scaled_dialog_metric(PROPERTIES_ROW_HEIGHT, scale_factor))
    .min((height - margin * 2.0).max(1.0))
    .max(1.0);
    ViewRect {
        x: ((width - overlay_width) / 2.0).max(margin),
        y: ((height - overlay_height) / 2.0).max(margin),
        width: overlay_width,
        height: overlay_height,
    }
}

#[cfg(test)]
fn create_dialog_rect(_dialog: &ShellCreateDialog, size: PhysicalSize<u32>) -> ViewRect {
    create_dialog_rect_scaled(_dialog, size, 1.0)
}

fn create_dialog_rect_scaled(
    _dialog: &ShellCreateDialog,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(CREATE_DIALOG_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(CREATE_DIALOG_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = scaled_dialog_metric(CREATE_DIALOG_HEIGHT, scale_factor)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn create_kind_button_rect(dialog_rect: ViewRect, kind: CreateEntryKind) -> ViewRect {
    create_kind_button_rect_scaled(dialog_rect, kind, 1.0)
}

fn create_kind_button_rect_scaled(
    dialog_rect: ViewRect,
    kind: CreateEntryKind,
    scale_factor: f32,
) -> ViewRect {
    let x = match kind {
        CreateEntryKind::Folder => dialog_rect.x + scaled_dialog_metric(16.0, scale_factor),
        CreateEntryKind::File => {
            dialog_rect.x
                + scaled_dialog_metric(16.0, scale_factor)
                + scaled_dialog_metric(96.0, scale_factor)
        }
    };
    ViewRect {
        x,
        y: dialog_rect.y
            + scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(14.0, scale_factor),
        width: scaled_dialog_metric(88.0, scale_factor),
        height: scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn create_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_input_rect_scaled(dialog_rect, 1.0)
}

fn create_dialog_input_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(60.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(30.0, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn create_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, 1.0)
}

fn create_dialog_cancel_button_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(CREATE_DIALOG_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right
            - button_width * 2.0
            - scaled_dialog_metric(CREATE_DIALOG_BUTTON_GAP, scale_factor),
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
fn create_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, 1.0)
}

fn create_dialog_commit_button_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(CREATE_DIALOG_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right - button_width,
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
fn rename_dialog_rect(_dialog: &ShellRenameDialog, size: PhysicalSize<u32>) -> ViewRect {
    rename_dialog_rect_scaled(_dialog, size, 1.0)
}

fn rename_dialog_rect_scaled(
    _dialog: &ShellRenameDialog,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(RENAME_DIALOG_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(RENAME_DIALOG_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = scaled_dialog_metric(RENAME_DIALOG_HEIGHT, scale_factor)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn rename_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    rename_dialog_input_rect_scaled(dialog_rect, 1.0)
}

fn rename_dialog_input_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(RENAME_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(18.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(30.0, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn rename_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect(dialog_rect)
}

fn rename_dialog_cancel_button_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, scale_factor)
}

#[cfg(test)]
fn rename_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect(dialog_rect)
}

fn rename_dialog_commit_button_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, scale_factor)
}

#[cfg(test)]
fn open_with_chooser_rect(chooser: &ShellOpenWithChooser, size: PhysicalSize<u32>) -> ViewRect {
    open_with_chooser_rect_scaled(chooser, size, 1.0)
}

fn open_with_chooser_rect_scaled(
    chooser: &ShellOpenWithChooser,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(OPEN_WITH_CHOOSER_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(OPEN_WITH_CHOOSER_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let rows = open_with_chooser_visible_row_count(chooser).max(1);
    let error_height = if chooser.error.is_some() {
        scaled_dialog_metric(26.0, scale_factor)
    } else {
        0.0
    };
    let dialog_height = (scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale_factor)
        + scaled_dialog_metric(16.0, scale_factor)
        + scaled_dialog_metric(OPEN_WITH_CHOOSER_QUERY_HEIGHT, scale_factor)
        + scaled_dialog_metric(12.0, scale_factor)
        + rows as f32 * scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor)
        + error_height
        + scaled_dialog_metric(54.0, scale_factor))
    .min((height - margin * 2.0).max(1.0))
    .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}

fn open_with_chooser_visible_row_count(chooser: &ShellOpenWithChooser) -> usize {
    chooser
        .filtered_count()
        .min(OPEN_WITH_CHOOSER_MAX_ROWS)
        .max(1)
}

#[cfg(test)]
#[allow(dead_code)]
fn open_with_chooser_query_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_query_rect_scaled(dialog_rect, 1.0)
}

fn open_with_chooser_query_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale_factor)
            + margin,
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(OPEN_WITH_CHOOSER_QUERY_HEIGHT, scale_factor),
    }
}

#[cfg(test)]
fn open_with_chooser_list_rect(dialog_rect: ViewRect, chooser: &ShellOpenWithChooser) -> ViewRect {
    open_with_chooser_list_rect_scaled(dialog_rect, chooser, 1.0)
}

fn open_with_chooser_list_rect_scaled(
    dialog_rect: ViewRect,
    chooser: &ShellOpenWithChooser,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    let query = open_with_chooser_query_rect_scaled(dialog_rect, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: query.bottom() + scaled_dialog_metric(12.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: open_with_chooser_visible_row_count(chooser) as f32
            * scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn open_with_chooser_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_cancel_button_rect_scaled(dialog_rect, 1.0)
}

fn open_with_chooser_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right
            - button_width * 2.0
            - scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_GAP, scale_factor),
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
fn open_with_chooser_open_button_rect(dialog_rect: ViewRect) -> ViewRect {
    open_with_chooser_open_button_rect_scaled(dialog_rect, 1.0)
}

fn open_with_chooser_open_button_rect_scaled(dialog_rect: ViewRect, scale_factor: f32) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right - button_width,
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
fn trash_conflict_dialog_rect(
    _dialog: &ShellTrashConflictDialog,
    size: PhysicalSize<u32>,
) -> ViewRect {
    trash_conflict_dialog_rect_scaled(_dialog, size, 1.0)
}

fn trash_conflict_dialog_rect_scaled(
    _dialog: &ShellTrashConflictDialog,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_HEIGHT, scale_factor)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn trash_conflict_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect(dialog_rect)
}

fn trash_conflict_dialog_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, scale_factor)
}

#[cfg(test)]
fn trash_conflict_dialog_replace_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect(dialog_rect)
}

fn trash_conflict_dialog_replace_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, scale_factor)
}

fn scaled_dialog_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor.max(1.0)).round().max(1.0)
}

fn property_row(label: &'static str, value: String) -> ShellPropertyRow {
    ShellPropertyRow { label, value }
}

fn open_with_applications_for_mime(
    cache: &MimeApplicationCache,
    mime: Option<&str>,
) -> Vec<MimeApplication> {
    let mut applications = Vec::new();
    let mut seen = BTreeSet::new();
    let associated = mime
        .map(|mime| cache.applications_for_mime(mime))
        .unwrap_or_default();
    for application in associated.into_iter().chain(cache.all_applications()) {
        if seen.insert(application.id.clone()) {
            applications.push(application);
        }
    }
    applications
}

fn open_with_filtered_application_indexes(
    applications: &[MimeApplication],
    query: &str,
) -> Vec<usize> {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    applications
        .iter()
        .enumerate()
        .filter(|(_, application)| open_with_application_matches_terms(application, &terms))
        .map(|(index, _)| index)
        .collect()
}

fn open_with_application_matches_terms(application: &MimeApplication, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystacks = [
        application.name.to_ascii_lowercase(),
        application.id.to_ascii_lowercase(),
        application.exec.to_ascii_lowercase(),
        application
            .desktop_file
            .display()
            .to_string()
            .to_ascii_lowercase(),
        application
            .icon
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    ];
    terms.iter().all(|term| {
        haystacks
            .iter()
            .any(|haystack| haystack.contains(term.as_str()))
    })
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.to_string()
}

fn launch_uri_for_path(path: &Path) -> String {
    network_uri_from_path(path).unwrap_or_else(|| gio::File::for_path(path).uri().to_string())
}

fn scrollbar_axis_for_view_mode(view_mode: ShellViewMode) -> ContentScrollbarAxis {
    match view_mode {
        ShellViewMode::Compact => ContentScrollbarAxis::Horizontal,
        ShellViewMode::Icons | ShellViewMode::Details => ContentScrollbarAxis::Vertical,
    }
}

fn launch_file_with_default_app(request: &OpenFileRequest) -> Result<(), String> {
    gio::AppInfo::launch_default_for_uri(&request.uri, None::<&gio::AppLaunchContext>).map_err(
        |error| {
            format!(
                "launch default app for {} ({}): {error}",
                request.path.display(),
                request.uri
            )
        },
    )
}

fn copy_location_text_for_path(path: &Path) -> String {
    path.display().to_string()
}

fn file_clipboard_role_as_str(role: FileClipboardRole) -> &'static str {
    match role {
        FileClipboardRole::Copy => "copy",
        FileClipboardRole::Cut => "cut",
    }
}

fn create_entry_on_disk(request: &CreateEntryRequest) -> Result<(), String> {
    match request.kind {
        CreateEntryKind::Folder => fs::create_dir(&request.path)
            .map_err(|error| format!("create folder {}: {error}", request.path.display())),
        CreateEntryKind::File => fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&request.path)
            .map(drop)
            .map_err(|error| format!("create file {}: {error}", request.path.display())),
    }
}

fn rename_entry_on_disk(request: &RenameEntryRequest) -> Result<(), String> {
    fs::rename(&request.source, &request.target).map_err(|error| {
        format!(
            "rename {} to {}: {error}",
            request.source.display(),
            request.target.display()
        )
    })
}

fn validate_create_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    if name == "." || name == ".." {
        return Err("name must not be . or ..".to_string());
    }
    if name.contains('/') {
        return Err("name must not contain /".to_string());
    }
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    Ok(())
}

fn unique_child_name(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
    }
    for suffix in 2..1000 {
        let candidate = format!("{base} {suffix}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    base.to_string()
}

fn path_name_or_display(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn entry_index_by_name(entries: &[Entry], name: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.name.as_ref() == name)
}

fn build_shell_places() -> Vec<ShellPlace> {
    let user_places_path = default_user_places_path();
    build_shell_places_from_current_devices(&user_places_path)
}

fn build_shell_places_from(user_places_path: &Path) -> Vec<ShellPlace> {
    build_shell_places_from_with_devices(user_places_path, &[])
}

fn rebuild_shell_places_for_user_path(user_places_path: &Path) -> Vec<ShellPlace> {
    if user_places_path == default_user_places_path().as_path() {
        build_shell_places_from_current_devices(user_places_path)
    } else {
        build_shell_places_from(user_places_path)
    }
}

fn build_shell_places_from_current_devices(user_places_path: &Path) -> Vec<ShellPlace> {
    let devices = read_gio_devices().unwrap_or_default();
    build_shell_places_from_with_devices(user_places_path, &devices)
}

fn build_shell_places_from_with_devices(
    user_places_path: &Path,
    devices: &[DeviceInfo],
) -> Vec<ShellPlace> {
    const NETWORK_GROUP: &str = "Network";
    const DEVICES_GROUP: &str = "Devices";

    let home = home_dir();
    let mut places = Vec::new();
    push_shell_place(&mut places, "", "H", "Home", home.clone(), false);
    push_existing_shell_place(&mut places, "", "Desk", "Desktop", home.join("Desktop"));
    push_existing_shell_place(&mut places, "", "Doc", "Documents", home.join("Documents"));
    push_existing_shell_place(&mut places, "", "Down", "Downloads", home.join("Downloads"));
    push_existing_shell_place(&mut places, "", "Mus", "Music", home.join("Music"));
    push_existing_shell_place(&mut places, "", "Pic", "Pictures", home.join("Pictures"));
    push_existing_shell_place(&mut places, "", "Vid", "Videos", home.join("Videos"));
    push_shell_place(
        &mut places,
        "",
        "Tr",
        "Trash",
        file_ops::trash_files_dir(),
        false,
    );

    let built_in_paths = places
        .iter()
        .map(|place| place.path.clone())
        .chain(std::iter::once(PathBuf::from("/")))
        .chain(std::iter::once(network_root_path()))
        .collect::<BTreeSet<_>>();
    let mut network_places = Vec::new();
    for place in load_user_places(user_places_path).unwrap_or_default() {
        if built_in_paths.contains(&place.path) {
            continue;
        }
        if is_network_path(&place.path) {
            network_places.push(place);
        } else {
            push_user_shell_place(&mut places, "", place);
        }
    }
    let place_order_path = place_order_path_for_user_places_path(user_places_path);
    let place_order = load_place_order(&place_order_path).unwrap_or_default();
    apply_primary_shell_place_order(&mut places, &place_order);

    push_shell_place(
        &mut places,
        NETWORK_GROUP,
        "Net",
        NETWORK_ROOT_LABEL,
        network_root_path(),
        false,
    );
    for place in network_places {
        push_user_shell_place(&mut places, NETWORK_GROUP, place);
    }
    push_shell_place(
        &mut places,
        DEVICES_GROUP,
        "/",
        "Root",
        PathBuf::from("/"),
        false,
    );
    push_device_shell_places(&mut places, DEVICES_GROUP, devices);
    places
}

fn push_device_shell_places(
    places: &mut Vec<ShellPlace>,
    devices_group: &'static str,
    devices: &[DeviceInfo],
) {
    for device in devices {
        if device.mounted
            && !device
                .mount_point
                .as_ref()
                .is_some_and(|path| path.is_dir())
        {
            continue;
        }
        if !device.mounted && !device.ejectable && !device.can_power_off {
            continue;
        }
        let path = device
            .mount_point
            .clone()
            .unwrap_or_else(|| PathBuf::from(&device.id));
        let label = device
            .label
            .clone()
            .unwrap_or_else(|| path_name_or_display(&path));
        let place =
            ShellPlace::new(devices_group, "D", label, path, false).with_device(ShellDevicePlace {
                id: device.id.clone(),
                mounted: device.mounted,
                ejectable: device.ejectable,
                can_power_off: device.can_power_off,
            });
        places.push(place);
    }
}

fn apply_primary_shell_place_order(places: &mut Vec<ShellPlace>, order: &[PathBuf]) {
    if order.is_empty() {
        return;
    }

    let first_grouped = places
        .iter()
        .position(|place| !place.group.is_empty())
        .unwrap_or(places.len());
    let mut primary_places = places.drain(..first_grouped).collect::<Vec<_>>();
    let mut ordered_places = Vec::with_capacity(primary_places.len());

    for path in order {
        if let Some(index) = primary_places
            .iter()
            .position(|place| place.path.as_path() == path.as_path())
        {
            ordered_places.push(primary_places.remove(index));
        }
    }
    ordered_places.append(&mut primary_places);
    places.splice(0..0, ordered_places);
}

fn save_shell_primary_place_order(
    user_places_path: &Path,
    places: &[ShellPlace],
) -> Result<(), String> {
    let order = places
        .iter()
        .filter(|place| place.group.is_empty())
        .map(|place| place.path.clone())
        .collect::<Vec<_>>();
    save_place_order(
        &place_order_path_for_user_places_path(user_places_path),
        &order,
    )
}

fn add_user_place_at_path(
    user_places_path: &Path,
    path: &Path,
    label: String,
) -> Result<bool, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("place label cannot be empty".to_string());
    }
    let mut places = load_user_places(user_places_path)?;
    if places.iter().any(|place| place.path.as_path() == path) {
        return Ok(false);
    }
    places.push(UserPlace::new(label.to_string(), path.to_path_buf()));
    save_user_places(user_places_path, &places)?;
    Ok(true)
}

fn remove_user_place_at_path(user_places_path: &Path, path: &Path) -> Result<bool, String> {
    let mut places = load_user_places(user_places_path)?;
    let old_len = places.len();
    places.retain(|place| place.path.as_path() != path);
    if places.len() == old_len {
        return Ok(false);
    }
    save_user_places(user_places_path, &places)?;

    let order_path = place_order_path_for_user_places_path(user_places_path);
    let mut order = load_place_order(&order_path)?;
    let old_order_len = order.len();
    order.retain(|ordered_path| ordered_path.as_path() != path);
    if order.len() != old_order_len {
        save_place_order(&order_path, &order)?;
    }
    Ok(true)
}

fn default_shell_place_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn push_existing_shell_place(
    places: &mut Vec<ShellPlace>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if path.is_dir() {
        push_shell_place(places, group, marker, label, path, false);
    }
}

fn push_user_shell_place(places: &mut Vec<ShellPlace>, group: &'static str, place: UserPlace) {
    push_shell_place(places, group, "B", place.label, place.path, true);
}

fn push_shell_place(
    places: &mut Vec<ShellPlace>,
    group: &'static str,
    marker: &'static str,
    label: impl Into<String>,
    path: PathBuf,
    editable: bool,
) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(ShellPlace::new(group, marker, label, path, editable));
}

fn active_shell_place_index(places: &[ShellPlace], current_path: &Path) -> Option<usize> {
    let mut best = None;
    let mut best_components = 0usize;
    for (index, place) in places.iter().enumerate() {
        if !shell_place_matches_current(place, current_path) {
            continue;
        }
        let components = place.path.components().count();
        if best.is_none() || components > best_components {
            best = Some(index);
            best_components = components;
        }
    }
    best
}

fn shell_place_matches_current(place: &ShellPlace, current_path: &Path) -> bool {
    current_path == place.path || current_path.starts_with(&place.path)
}

fn filtered_indexes_for_entries(entries: &[Entry], show_hidden: bool, pattern: &str) -> Vec<usize> {
    let filter = (!pattern.is_empty()).then(|| NameFilter::plain_text(pattern.to_string()));
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let visible = show_hidden || !is_hidden_entry(entry);
            let matches_filter = filter
                .as_ref()
                .is_none_or(|filter| filter.matches_name(entry.name.as_ref()));
            (visible && matches_filter).then_some(index)
        })
        .collect()
}

fn is_hidden_entry(entry: &Entry) -> bool {
    entry.name.as_ref().starts_with('.')
}

#[cfg(test)]
fn content_height(size: PhysicalSize<u32>) -> f32 {
    (size.height as f32 - TOP_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(1.0)
}

fn nonzero_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn normalized_scale_factor(scale_factor: f32) -> f32 {
    if scale_factor.is_finite() {
        scale_factor.clamp(0.5, 4.0)
    } else {
        1.0
    }
}

fn visible_layout_range_for_projection(
    projection: &ShellPaneProjection<'_>,
) -> Option<Range<usize>> {
    let start = projection
        .visible_items
        .iter()
        .map(|item| item.layout.model_index)
        .min()?;
    let end = projection
        .visible_items
        .iter()
        .map(|item| item.layout.model_index)
        .max()?
        + 1;
    (start < end).then_some(start..end)
}

fn shell_dolphin_read_ahead_indexes(
    visible_indexes: Range<usize>,
    item_count: usize,
    maximum_visible_items: usize,
) -> Vec<usize> {
    if item_count == 0 || visible_indexes.is_empty() {
        return Vec::new();
    }

    let visible_start = visible_indexes.start.min(item_count);
    let visible_end = visible_indexes.end.min(item_count).max(visible_start);
    if visible_start >= visible_end {
        return Vec::new();
    }

    let maximum_visible_items = maximum_visible_items.max(1);
    let read_ahead_items = (THUMBNAIL_READ_AHEAD_PAGES * maximum_visible_items)
        .min(THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT / 2);
    let last_visible = visible_end - 1;
    let end_extended = (last_visible + read_ahead_items).min(item_count - 1);
    let begin_extended = visible_start.saturating_sub(read_ahead_items);

    let mut result = Vec::new();
    result.extend(visible_end..end_extended + 1);
    result.extend((begin_extended..visible_start).rev());

    let last_page_start = (end_extended + 1).max(item_count.saturating_sub(maximum_visible_items));
    result.extend(last_page_start..item_count);

    let first_page_end = begin_extended.min(maximum_visible_items);
    result.extend(0..first_page_end);

    let mut remaining = THUMBNAIL_READ_AHEAD_RESOLVE_LIMIT.saturating_sub(result.len());
    let rest_after_visible = (end_extended + 1)..last_page_start;
    let rest_after_len = rest_after_visible.len().min(remaining);
    result.extend(rest_after_visible.take(rest_after_len));
    remaining = remaining.saturating_sub(rest_after_len);

    result.extend((first_page_end..begin_extended).rev().take(remaining));
    result
}

fn text_metrics_for_label_height(
    label_height: u32,
    max_font_size: f32,
    max_line_height: f32,
) -> Metrics {
    let line_height = (label_height as f32).max(1.0).min(max_line_height.max(1.0));
    let font_size = (max_font_size * line_height / max_line_height.max(1.0)).clamp(8.0, 64.0);
    Metrics::new(font_size, line_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entry(name: &str, is_dir: bool) -> Entry {
        test_entry_with_mime(
            name,
            is_dir,
            if is_dir {
                "inode/directory"
            } else {
                "text/plain"
            },
        )
    }

    fn test_entry_with_mime(name: &str, is_dir: bool, mime_type: &'static str) -> Entry {
        test_entry_with_mime_and_modified(name, is_dir, mime_type, None)
    }

    fn test_entry_with_mime_and_modified(
        name: &str,
        is_dir: bool,
        mime_type: &'static str,
        modified_secs: Option<u64>,
    ) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs,
            metadata_complete: true,
            mime_type: Some(Arc::from(mime_type)),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn test_entry_with_target(name: &str, is_dir: bool, target_path: PathBuf) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: Some(target_path),
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: Some(Arc::from(if is_dir {
                "inode/directory"
            } else {
                "text/plain"
            })),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fika-wgpu-{name}-{unique}"))
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

    fn test_scene(entries: Vec<Entry>, view_mode: ShellViewMode) -> ShellScene {
        ShellScene {
            primary_pane: ShellPaneState::from_entries(
                PathBuf::from("/tmp"),
                view_mode,
                entries,
                false,
                "",
            ),
            places: vec![
                ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
                ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ],
            location_draft: None,
            filter_active: false,
            filter_pattern: String::new(),
            show_hidden: false,
            zoom_step: 0,
            places_visible: true,
            places_width: PLACES_SIDEBAR_WIDTH,
            places_scroll_y: 0.0,
            scrollbar_drag: None,
            pointer: None,
            hovered_index: None,
            hovered_place: None,
            last_primary_click: None,
            history: PathHistory::default(),
            selection: ShellSelection::default(),
            context_target: None,
            context_menu: None,
            properties_overlay: None,
            create_dialog: None,
            rename_dialog: None,
            open_with_chooser: None,
            trash_conflict_dialog: None,
            split_pane: None,
            split_pane_left_fraction: 0.5,
            primary_visible_slots: ShellVisibleItemSlotPool::default(),
            split_visible_slots: ShellVisibleItemSlotPool::default(),
            visible_slot_stats: ShellVisibleItemSlotStats::default(),
            internal_drag: None,
            dnd_hover_target: None,
            pending_drop_request: None,
            rubber_band: None,
            scale_factor: 1.0,
            hit_tests: 0,
            selection_changes: 0,
            context_target_changes: 0,
            context_menu_actions: 0,
            properties_changes: 0,
            create_changes: 0,
            rename_changes: 0,
            open_with_changes: 0,
            open_changes: 0,
            copy_location_changes: 0,
            file_clipboard_changes: 0,
            paste_changes: 0,
            trash_changes: 0,
            places_changes: 0,
            places_resize_changes: 0,
            places_scroll_changes: 0,
            keyboard_navigation: 0,
            rubber_band_updates: 0,
            view_switches: 0,
            path_changes: 0,
            directory_reloads: 0,
            location_changes: 0,
            filter_changes: 0,
            hidden_changes: 0,
            zoom_changes: 0,
            split_pane_changes: 0,
            dnd_hover_changes: 0,
            dnd_drop_requests: 0,
        }
    }

    #[test]
    fn places_hit_testing_is_separate_from_file_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let place_row = scene.place_row_rects(size)[0].1;
        let place_point = ViewPoint {
            x: place_row.x + 4.0,
            y: place_row.y + 4.0,
        };

        assert_eq!(
            scene.place_index_at_screen_point(place_point, size),
            Some(0)
        );
        assert_eq!(scene.hit_test_screen_point(place_point, size), None);
        assert!(scene.set_pointer(place_point, size));
        assert_eq!(scene.hovered_place, Some(0));
        assert_eq!(scene.hovered_index, None);

        let item = scene.layout(size).item(0).expect("item should layout");
        let item_point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: scene.content_origin_y() + item.visual_rect.y + 2.0,
        };
        assert!(scene.set_pointer(item_point, size));
        assert_eq!(scene.hovered_place, None);
        assert_eq!(scene.hovered_index, Some(0));
    }

    #[test]
    fn places_chrome_starts_at_pane_origin_below_app_toolbar() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let pane = scene.pane_rect(size);
        let sidebar = scene.places_sidebar_rect(size);
        let panel = scene.places_panel_rect(size);

        assert_eq!(sidebar.y, pane.y);
        assert_eq!(panel.y, pane.y);
        assert_eq!(sidebar.bottom(), pane.bottom());
        assert!(
            scene.content_origin_x(size) > sidebar.right(),
            "file pane should leave a visual gap after Places"
        );
        assert_eq!(
            scene.content_origin_x(size) - sidebar.right(),
            scene.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH)
                + scene.scale_metric(PLACES_TO_PANE_GAP)
        );
        assert!(
            panel.right() < sidebar.right(),
            "Places panel should keep right padding inside the sidebar"
        );
        assert!(scene.app_toolbar_height() < pane.y);
        assert!(!sidebar.contains(ViewPoint {
            x: sidebar.x + 8.0,
            y: scene.app_toolbar_height() / 2.0,
        }));
    }

    #[test]
    fn places_toggle_hides_sidebar_and_reclaims_pane_width() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let before_origin = scene.content_origin_x(size);
        let before_width = scene.pane_width(size);
        let place_row = scene.place_row_rects(size)[0].1;
        let place_point = ViewPoint {
            x: place_row.x + 4.0,
            y: place_row.y + 4.0,
        };
        let toggle = scene.places_toggle_rect(size);

        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: toggle.x + 2.0,
                    y: toggle.y + 2.0,
                },
                size,
            ),
            Some(true)
        );
        assert!(!scene.places_visible);
        assert_eq!(scene.places_sidebar_width(size), 0.0);
        assert_eq!(scene.content_origin_x(size), 0.0);
        assert!(scene.pane_width(size) > before_width);
        assert_eq!(scene.place_index_at_screen_point(place_point, size), None);
        assert_eq!(scene.places_changes, 1);

        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: toggle.x + 2.0,
                    y: toggle.y + 2.0,
                },
                size,
            ),
            Some(true)
        );
        assert!(scene.places_visible);
        assert_eq!(scene.content_origin_x(size), before_origin);
        assert!(scene.places_sidebar_width(size) > 0.0);
    }

    #[test]
    fn places_resize_drag_updates_sidebar_width_and_content_origin() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let before_width = scene.places_sidebar_width(size);
        let before_origin = scene.content_origin_x(size);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let start = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 12.0,
        };

        assert!(scene.begin_scrollbar_drag(start, size).is_some());
        assert!(scene.is_scrollbar_dragging());
        assert!(scene.set_pointer(
            ViewPoint {
                x: start.x + 48.0,
                y: start.y,
            },
            size
        ));
        assert!(scene.places_sidebar_width(size) > before_width);
        assert!(scene.content_origin_x(size) > before_origin);
        assert_eq!(scene.places_resize_changes, 1);

        assert!(scene.set_pointer(ViewPoint { x: 0.0, y: start.y }, size));
        assert_eq!(
            scene.places_sidebar_width(size),
            scene.places_sidebar_width_bounds(size).0
        );
        assert!(scene.places_resize_changes >= 2);
        let _ = scene.end_scrollbar_drag(ViewPoint { x: 0.0, y: start.y }, size);
        assert!(!scene.is_scrollbar_dragging());
    }

    #[test]
    fn places_resize_handle_is_left_biased_and_easier_to_hit() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let sidebar = scene.places_sidebar_rect(size);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let expected_left = sidebar.right() - scene.scale_metric(PLACES_RESIZE_HANDLE_WIDTH);

        assert!(handle.x <= expected_left + 0.5);
        assert!(handle.right() > sidebar.right());

        let left_edge_point = ViewPoint {
            x: handle.x + 0.25,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(left_edge_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);

        let outside_left = ViewPoint {
            x: handle.x - 1.0,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(outside_left, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
    }

    #[test]
    fn address_bar_uses_text_cursor_without_overriding_resize_cursor() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let path_bar = scene.path_bar_rect(size).unwrap();
        let path_point = ViewPoint {
            x: path_bar.x + path_bar.width / 2.0,
            y: path_bar.y + path_bar.height / 2.0,
        };

        let _ = scene.set_pointer(path_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Text);

        let handle = scene.places_resize_handle_rect(size).unwrap();
        let handle_point = ViewPoint {
            x: handle.x + 1.0,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(handle_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(handle_point, size).is_some());
        let _ = scene.set_pointer(path_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
    }

    #[test]
    fn splitter_cursor_hints_follow_hover_and_drag_state() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let point = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 10.0,
        };

        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
        let _ = scene.set_pointer(point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(point, size).is_some());
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.set_pointer(
            ViewPoint {
                x: point.x + 30.0,
                y: point.y,
            },
            size
        ));
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        let _ = scene.end_scrollbar_drag(
            ViewPoint {
                x: point.x + 30.0,
                y: point.y,
            },
            size,
        );
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        let _ = scene.set_pointer(
            ViewPoint {
                x: scene.content_origin_x(size) + scene.content_width(size) - 8.0,
                y: scene.content_origin_y() + 8.0,
            },
            size,
        );
        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
    }

    #[test]
    fn split_pane_divider_drag_updates_left_width_and_cursor_hint() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true)];
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Icons,
            dir_count: 1,
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let size = PhysicalSize::new(900, 360);
        let before_left = scene.split_pane_metrics(size).unwrap().left_width;
        let handle = scene.split_pane_resize_handle_rect(size).unwrap();
        let start = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 20.0,
        };

        let _ = scene.set_pointer(start, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(start, size).is_some());
        assert!(scene.set_pointer(
            ViewPoint {
                x: start.x + 70.0,
                y: start.y,
            },
            size
        ));
        let after_left = scene.split_pane_metrics(size).unwrap().left_width;
        assert!(after_left > before_left);
        assert!(scene.split_pane_left_fraction > 0.5);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);

        assert!(scene.set_pointer(ViewPoint { x: 0.0, y: start.y }, size));
        let min_left = scene.split_pane_width_bounds(size).unwrap().1;
        assert_eq!(scene.split_pane_metrics(size).unwrap().left_width, min_left);
        let _ = scene.end_scrollbar_drag(ViewPoint { x: 0.0, y: start.y }, size);
        assert!(!scene.is_scrollbar_dragging());
    }

    #[test]
    fn hidden_places_do_not_capture_scroll() {
        let entries = (0..80)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        let mut scene = test_scene(entries, ShellViewMode::Icons);
        scene.places = (0..28)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        assert!(scene.max_places_scroll_y(size) > 0.0);
        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: scene.places_toggle_rect(size).x + 2.0,
                    y: scene.places_toggle_rect(size).y + 2.0,
                },
                size,
            ),
            Some(true)
        );

        scene.pointer = Some(ViewPoint {
            x: PLACES_SIDEBAR_PADDING_X + 2.0,
            y: scene.content_origin_y() + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert_eq!(scene.places_scroll_y, 0.0);
        assert!(scene.primary_pane.scroll_y > 0.0);
    }

    #[test]
    fn drop_target_lookup_resolves_places_primary_blank_and_split_items() {
        let mut scene = test_scene(
            vec![test_entry("alpha", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let split_entries = vec![test_entry("right", true)];
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Icons,
            dir_count: 1,
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let size = PhysicalSize::new(900, 360);

        let place_row = scene.place_row_rects(size)[0].1;
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: place_row.x + 4.0,
                    y: place_row.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::Place {
                index: 0,
                path: PathBuf::from("/tmp"),
            })
        );

        let places_panel = scene.places_panel_rect(size);
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: places_panel.x + 4.0,
                    y: places_panel.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PlacesBlank)
        );

        let primary_geometry = scene.primary_pane_geometry(size);
        let primary_item = scene.layout(size).item(0).unwrap();
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: primary_geometry.content.x + primary_item.visual_rect.x + 4.0,
                    y: primary_geometry.content.y + primary_item.visual_rect.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneKind::Primary,
                index: 0,
                path: PathBuf::from("/tmp/alpha"),
                is_dir: true,
            })
        );
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: primary_geometry.content.right() - 2.0,
                    y: primary_geometry.content.bottom() - 2.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneBlank {
                pane: ShellPaneKind::Primary,
                path: PathBuf::from("/tmp"),
            })
        );

        let split_geometry = scene.split_pane_geometry(size).unwrap();
        let split_view = scene.pane_view(ShellPaneKind::Split).unwrap();
        let split_layout = scene.pane_layout(
            split_view,
            split_geometry.content.width,
            split_geometry.content.height,
        );
        let split_item = split_layout.item(0).unwrap();
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: split_geometry.content.x + split_item.visual_rect.x + 4.0,
                    y: split_geometry.content.y + split_item.visual_rect.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneKind::Split,
                index: 0,
                path: PathBuf::from("/right-root/right"),
                is_dir: true,
            })
        );
    }

    #[test]
    fn dnd_hover_target_can_be_updated_and_cleared_from_retained_hit_testing() {
        let mut scene = test_scene(
            vec![test_entry("alpha", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);
        let content = projection.geometry.content;
        let item = projection.visible_items[0];
        let item_point = ViewPoint {
            x: content.x + item.layout.visual_rect.x + 4.0,
            y: content.y + item.layout.visual_rect.y + 4.0,
        };

        assert!(scene.update_dnd_hover_target(item_point, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            Some("pane-item")
        );
        assert_eq!(scene.dnd_hover_changes, 1);
        assert!(!scene.update_dnd_hover_target(item_point, size));
        assert_eq!(scene.dnd_hover_changes, 1);

        let blank_point = ViewPoint {
            x: content.right() - 2.0,
            y: content.bottom() - 2.0,
        };
        assert!(scene.update_dnd_hover_target(blank_point, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            Some("pane-blank")
        );
        assert_eq!(scene.dnd_hover_changes, 2);
        assert!(scene.clear_dnd_hover_target());
        assert_eq!(scene.dnd_hover_target, None);
        assert_eq!(scene.dnd_hover_changes, 3);
    }

    #[test]
    fn internal_drag_to_primary_blank_creates_copy_drop_request() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("beta.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let blank = ViewPoint {
            x: projection.geometry.content.right() - 4.0,
            y: projection.geometry.content.bottom() - 4.0,
        };

        assert!(scene.begin_primary_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(blank, size));
        assert!(scene.internal_drag.as_ref().is_some_and(|drag| drag.active));
        assert!(scene.end_primary_pointer(blank, size));

        let request = scene
            .pending_drop_request
            .as_ref()
            .expect("active internal drag should create a drop request");
        assert_eq!(request.sources, vec![PathBuf::from("/tmp/alpha.txt")]);
        assert_eq!(request.target_dir, PathBuf::from("/tmp"));
        assert_eq!(
            request.target,
            ShellDropTarget::PaneBlank {
                pane: ShellPaneKind::Primary,
                path: PathBuf::from("/tmp"),
            }
        );
        assert_eq!(request.mode, FileTransferMode::Copy);
        assert_eq!(scene.dnd_drop_requests, 1);
        assert!(scene.internal_drag.is_none());
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn internal_drag_to_directory_item_targets_that_directory() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);
        let folder = projection.visible_items[0];
        let note = projection.visible_items[1];
        let start = ViewPoint {
            x: projection.geometry.content.x + note.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + note.layout.visual_rect.y + 6.0,
        };
        let target = ViewPoint {
            x: projection.geometry.content.x + folder.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + folder.layout.visual_rect.y + 6.0,
        };

        assert!(scene.begin_primary_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(target, size));
        assert!(scene.end_primary_pointer(target, size));

        let request = scene
            .pending_drop_request
            .as_ref()
            .expect("directory target should accept a drop request");
        assert_eq!(request.sources, vec![PathBuf::from("/tmp/note.txt")]);
        assert_eq!(request.target_dir, PathBuf::from("/tmp/folder"));
        assert_eq!(
            request.target,
            ShellDropTarget::PaneItem {
                pane: ShellPaneKind::Primary,
                index: 0,
                path: PathBuf::from("/tmp/folder"),
                is_dir: true,
            }
        );
        assert_eq!(scene.dnd_drop_requests, 1);
    }

    #[test]
    fn internal_drag_below_threshold_finishes_as_plain_click() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let end = ViewPoint {
            x: start.x + 1.0,
            y: start.y + 1.0,
        };

        assert!(scene.begin_primary_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(end, size));
        assert!(!scene.internal_drag.as_ref().is_some_and(|drag| drag.active));
        assert!(!scene.end_primary_pointer(end, size));
        assert!(scene.pending_drop_request.is_none());
        assert_eq!(scene.dnd_drop_requests, 0);
        assert!(scene.selection.contains(0));
    }

    #[test]
    fn internal_drag_to_plain_file_clears_hover_without_drop_request() {
        let mut scene = test_scene(
            vec![
                test_entry("source.txt", false),
                test_entry("target.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);
        let source = projection.visible_items[0];
        let target = projection.visible_items[1];
        let start = ViewPoint {
            x: projection.geometry.content.x + source.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + source.layout.visual_rect.y + 6.0,
        };
        let end = ViewPoint {
            x: projection.geometry.content.x + target.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + target.layout.visual_rect.y + 6.0,
        };

        assert!(scene.begin_primary_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(end, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            Some("pane-item")
        );
        assert!(scene.end_primary_pointer(end, size));
        assert!(scene.pending_drop_request.is_none());
        assert_eq!(scene.dnd_drop_requests, 0);
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn primary_and_split_panes_share_reusable_state_accessors() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true)];
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Details,
            dir_count: 1,
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });

        assert_eq!(
            scene.pane_state(ShellPaneKind::Primary).unwrap().path,
            PathBuf::from("/tmp")
        );
        assert_eq!(
            scene.pane_state(ShellPaneKind::Split).unwrap().path,
            PathBuf::from("/right-root")
        );

        scene
            .pane_state_mut(ShellPaneKind::Primary)
            .unwrap()
            .scroll_y = 42.0;
        scene.pane_state_mut(ShellPaneKind::Split).unwrap().scroll_y = 24.0;

        assert_eq!(
            scene.pane_scroll_offset(ShellPaneKind::Primary),
            Some((0.0, 42.0))
        );
        assert_eq!(
            scene.pane_scroll_offset(ShellPaneKind::Split),
            Some((0.0, 24.0))
        );
    }

    #[test]
    fn pane_projection_shares_visible_items_and_scroll_metrics_across_panes() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let split_entries = (0..24)
            .map(|index| test_entry(&format!("right-{index:02}"), index % 2 == 0))
            .collect::<Vec<_>>();
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Details,
            dir_count: split_entries.iter().filter(|entry| entry.is_dir).count(),
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let size = PhysicalSize::new(900, 360);

        let primary = scene.primary_pane_projection(size);
        assert_eq!(primary.geometry.kind, ShellPaneKind::Primary);
        assert_eq!(
            primary.visible_items.len(),
            scene.layout(size).visible_items().len()
        );
        assert_eq!(
            primary.scroll_metrics.max_scroll_x,
            scene.max_scroll_x(size)
        );
        assert_eq!(
            primary.scroll_metrics.max_scroll_y,
            scene.max_scroll_y(size)
        );

        let split = scene.pane_projection(ShellPaneKind::Split, size).unwrap();
        assert_eq!(split.geometry.kind, ShellPaneKind::Split);
        assert_eq!(split.view.path, Path::new("/right-root"));
        assert!(!split.visible_items.is_empty());
        assert!(split.scroll_metrics.content_size.height >= split.geometry.content.height);
    }

    #[test]
    fn pane_projection_assigns_reused_visible_slots() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);

        let first_stats = scene.update_visible_slot_pools(size);
        assert!(first_stats.active > 0);
        assert_eq!(first_stats.allocated, first_stats.active);
        let first_projection = scene.primary_pane_projection(size);
        let first_slot = first_projection.visible_items[0].slot_id;
        assert_ne!(first_slot, 0);
        assert!(
            first_projection
                .visible_items
                .iter()
                .all(|item| item.slot_id != 0)
        );

        let second_stats = scene.update_visible_slot_pools(size);
        assert_eq!(second_stats.reused, second_stats.active);
        let second_projection = scene.primary_pane_projection(size);
        assert_eq!(second_projection.visible_items[0].slot_id, first_slot);
    }

    #[test]
    fn thumbnail_candidates_are_projected_from_visible_previewable_files() {
        let scene = test_scene(
            vec![
                test_entry_with_mime_and_modified("photo.png", false, "image/png", Some(42)),
                test_entry_with_mime_and_modified("notes.txt", false, "text/plain", Some(42)),
                test_entry("folder", true),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.primary_pane_projection(size);

        assert_eq!(
            scene.thumbnail_candidate_count_for_projection(&projection),
            1
        );
    }

    #[test]
    fn thumbnail_read_ahead_indexes_follow_dolphin_order() {
        let indexes = shell_dolphin_read_ahead_indexes(4..7, 16, 3);

        assert_eq!(&indexes[..6], &[7, 8, 9, 10, 11, 12]);
        assert!(!indexes.iter().any(|index| (4..7).contains(index)));
        assert_eq!(
            indexes.iter().copied().collect::<BTreeSet<_>>().len(),
            indexes.len()
        );
    }

    #[test]
    fn thumbnail_worker_promotes_visible_request_over_deferred() {
        let mut visible = VecDeque::new();
        let mut deferred = VecDeque::new();
        let mut queued = HashMap::new();
        let first = test_thumbnail_raster_request("first.png", ThumbnailRequestPriority::Deferred);
        let second =
            test_thumbnail_raster_request("second.png", ThumbnailRequestPriority::Deferred);
        let promoted =
            test_thumbnail_raster_request("first.png", ThumbnailRequestPriority::Visible);

        thumbnail_worker_queue_request(first.clone(), &mut visible, &mut deferred, &mut queued);
        thumbnail_worker_queue_request(second.clone(), &mut visible, &mut deferred, &mut queued);
        thumbnail_worker_queue_request(promoted.clone(), &mut visible, &mut deferred, &mut queued);

        assert_eq!(visible.len(), 1);
        assert_eq!(deferred.len(), 1);
        assert_eq!(visible.front().unwrap().key, promoted.key);
        assert_eq!(deferred.front().unwrap().key, second.key);
        assert_eq!(
            queued.get(&promoted.key),
            Some(&ThumbnailRequestPriority::Visible)
        );
    }

    #[test]
    fn thumbnail_ready_cache_evicts_old_read_ahead_results() {
        let cache_root = test_dir("thumbnail-ready-cache-root");
        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
        resolver.ready_max_bytes = 32;
        let first = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/first.png"), 8, 1);
        let second = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/second.png"), 8, 1);
        let third = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/third.png"), 8, 1);

        resolver.insert_ready(first.clone(), test_icon_raster(2, 1));
        resolver.insert_ready(second.clone(), test_icon_raster(2, 2));
        resolver.insert_ready(third.clone(), test_icon_raster(2, 3));

        assert_eq!(resolver.ready_len(), 2);
        assert_eq!(resolver.ready_bytes(), 32);
        assert!(!resolver.ready.contains_key(&first));
        assert!(resolver.ready.contains_key(&second));
        assert!(resolver.ready.contains_key(&third));

        assert!(matches!(
            resolver.resolve(&second.path, 1, Some("image/png".to_string()), 8),
            ThumbnailResolveState::Ready(_)
        ));
        assert_eq!(resolver.ready_len(), 1);
        assert_eq!(resolver.ready_bytes(), 16);

        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn thumbnail_resolver_uses_freedesktop_cache_hit() {
        let cache_root = test_dir("thumbnail-cache-root");
        let source_root = test_dir("thumbnail-source-root");
        fs::create_dir_all(&source_root).unwrap();
        let source = source_root.join("photo.png");
        fs::write(&source, b"source").unwrap();
        let modified_secs = 42;
        let uri = fika_core::thumbnail_uri_for_path(&source).unwrap();
        let thumbnail =
            fika_core::thumbnail_cache_path(&cache_root, fika_core::ThumbnailSize::Normal, &uri);
        write_test_thumbnail_png(&thumbnail, &uri, modified_secs);

        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
        assert!(matches!(
            resolver.resolve(&source, modified_secs, Some("image/png".to_string()), 48),
            ThumbnailResolveState::Pending
        ));

        match wait_for_thumbnail_state(&mut resolver, &source, modified_secs, Some("image/png"), 48)
        {
            ThumbnailResolveState::Ready(raster) => {
                assert_eq!(raster.width, 48);
                assert_eq!(raster.height, 48);
                assert!(raster.pixels.iter().any(|channel| *channel != 0));
            }
            state => panic!("expected ready thumbnail raster, got {state:?}"),
        }

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn thumbnail_resolver_caches_failed_probe_result() {
        let cache_root = test_dir("thumbnail-failed-cache-root");
        let source_root = test_dir("thumbnail-failed-source-root");
        fs::create_dir_all(&source_root).unwrap();
        let source = source_root.join("payload.bin");
        fs::write(&source, b"source").unwrap();
        let modified_secs = 42;

        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
        assert!(matches!(
            resolver.resolve(&source, modified_secs, None, 48),
            ThumbnailResolveState::Pending
        ));
        assert!(matches!(
            wait_for_thumbnail_state(&mut resolver, &source, modified_secs, None, 48),
            ThumbnailResolveState::Failed
        ));
        let pending_after_failure = resolver.pending.len();
        assert!(
            resolver
                .failed
                .contains(&ThumbnailProbeCacheKey::new(source.clone(), modified_secs))
        );
        assert!(matches!(
            resolver.resolve(&source, modified_secs, None, 48),
            ThumbnailResolveState::Failed
        ));
        assert_eq!(resolver.pending.len(), pending_after_failure);

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(source_root);
    }

    fn test_thumbnail_raster_request(
        name: &str,
        priority: ThumbnailRequestPriority,
    ) -> ThumbnailRasterRequest {
        ThumbnailRasterRequest {
            key: IconRasterCacheKey::thumbnail(PathBuf::from(format!("/tmp/{name}")), 48, 1),
            mime_type: Some("image/png".to_string()),
            priority,
        }
    }

    fn test_icon_raster(size: u32, seed: u8) -> IconRaster {
        IconRaster {
            pixels: vec![seed; (size * size * 4) as usize].into(),
            width: size,
            height: size,
        }
    }

    fn wait_for_thumbnail_state(
        resolver: &mut ThumbnailRasterResolver,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<&str>,
        size_px: u16,
    ) -> ThumbnailResolveState {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state =
                resolver.resolve(path, modified_secs, mime_type.map(str::to_string), size_px);
            if !matches!(state, ThumbnailResolveState::Pending) || Instant::now() >= deadline {
                return state;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn write_test_thumbnail_png(path: &Path, uri: &str, modified_secs: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([32, 96, 192, 255]))
            .save(path)
            .unwrap();
        fika_core::write_thumbnail_metadata(path, uri, modified_secs).unwrap();
    }

    #[test]
    fn split_pane_mouse_wheel_scrolls_the_target_pane_only() {
        let mut scene = test_scene(
            (0..12)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let split_entries = (0..80)
            .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Icons,
            dir_count: 0,
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let size = PhysicalSize::new(760, 260);
        let split_content = scene.split_pane_geometry(size).unwrap().content;
        scene.pointer = Some(ViewPoint {
            x: split_content.x + 8.0,
            y: split_content.y + 8.0,
        });

        assert!(scene.scroll_by(120.0, size));
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        assert!(scene.split_pane.as_ref().unwrap().scroll_y > 0.0);
    }

    #[test]
    fn split_pane_scrollbar_drag_updates_split_scroll_offset() {
        let mut scene = test_scene(
            (0..12)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let split_entries = (0..80)
            .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        scene.split_pane = Some(ShellPaneState {
            path: PathBuf::from("/right-root"),
            view_mode: ShellViewMode::Icons,
            dir_count: 0,
            filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
            entries: split_entries,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let size = PhysicalSize::new(760, 260);
        let (track, thumb) = scene
            .pane_content_scrollbar_rects(ShellPaneKind::Split, size)
            .expect("split pane should need its own scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert_eq!(
            scene.scrollbar_drag.map(|drag| drag.target),
            Some(ScrollbarDragTarget::Content {
                pane: ShellPaneKind::Split,
                axis: ContentScrollbarAxis::Vertical,
            })
        );
        assert!(scene.set_pointer(drag_to, size));
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        assert!(scene.split_pane.as_ref().unwrap().scroll_y > 0.0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn places_sidebar_scroll_is_independent_from_file_content_scroll() {
        let entries = (0..80)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        let mut scene = test_scene(entries, ShellViewMode::Icons);
        scene.places = (0..28)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        assert!(scene.max_places_scroll_y(size) > 0.0);
        assert!(scene.max_scroll_y(size) > 0.0);

        scene.pointer = Some(ViewPoint {
            x: PLACES_SIDEBAR_PADDING_X + 2.0,
            y: TOP_BAR_HEIGHT + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert!(scene.places_scroll_y > 0.0);
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        assert_eq!(scene.places_scroll_changes, 1);

        scene.pointer = Some(ViewPoint {
            x: scene.content_origin_x(size) + 10.0,
            y: scene.content_origin_y() + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert!(scene.primary_pane.scroll_y > 0.0);
        assert_eq!(scene.places_scroll_changes, 1);
    }

    #[test]
    fn places_row_hit_testing_follows_sidebar_scroll_offset() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..16)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 160);
        let first_row = scene.place_row_rects(size)[0].1;
        let point = ViewPoint {
            x: first_row.x + 6.0,
            y: first_row.y + 6.0,
        };

        assert_eq!(scene.place_index_at_screen_point(point, size), Some(0));
        assert!(scene.scroll_places_by(PLACES_ROW_HEIGHT + PLACES_ROW_GAP, size));
        assert_eq!(scene.place_index_at_screen_point(point, size), Some(1));
    }

    #[test]
    fn places_scrollbar_thumb_moves_with_sidebar_scroll() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..24)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        let before = scene
            .places_scrollbar_thumb_rect(size)
            .expect("overflowing places should show a scrollbar thumb");

        assert!(scene.scroll_places_by(96.0, size));
        let after = scene
            .places_scrollbar_thumb_rect(size)
            .expect("scrollbar thumb should remain visible");

        assert!(after.y > before.y);
        assert_eq!(after.x, before.x);
        assert_eq!(after.width, before.width);
    }

    #[test]
    fn content_scrollbar_reserves_vertical_track_for_icons() {
        let scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let content = scene.content_screen_rect(size);
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("icons view should need vertical scrollbar");

        assert_eq!(
            scene.content_scrollbar_axis(),
            ContentScrollbarAxis::Vertical
        );
        assert!(track.x >= content.right());
        assert!(track.width > 0.0);
        assert!(thumb.height >= CONTENT_SCROLLBAR_MIN_THUMB_SIZE.min(track.height));
        assert!(!content.contains(ViewPoint {
            x: track.x,
            y: track.y,
        }));
    }

    #[test]
    fn compact_content_scrollbar_uses_horizontal_offset() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(420, 260);
        let (start_track, start_thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should need horizontal scrollbar");
        scene.primary_pane.scroll_x = scene.max_scroll_x(size) / 2.0;
        let (middle_track, middle_thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should keep horizontal scrollbar");

        assert_eq!(
            scene.content_scrollbar_axis(),
            ContentScrollbarAxis::Horizontal
        );
        assert_eq!(start_track.y, middle_track.y);
        assert!(middle_thumb.x > start_thumb.x);
        assert_eq!(start_thumb.height, middle_thumb.height);
    }

    #[test]
    fn content_scrollbar_thumb_drag_updates_vertical_scroll() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("icons view should need vertical scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.scrollbar_drag.is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.primary_pane.scroll_y > 0.0);
        assert_eq!(scene.primary_pane.scroll_x, 0.0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn content_scrollbar_thumb_drag_updates_horizontal_scroll() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(420, 260);
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should need horizontal scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: track.right() - thumb.width / 2.0,
            y: press.y,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.primary_pane.scroll_x > 0.0);
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn places_scrollbar_thumb_drag_updates_sidebar_scroll() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..24)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        let (track, thumb) = scene
            .places_scrollbar_rects(size)
            .expect("overflowing places should show a scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.places_scroll_y > 0.0);
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        assert!(scene.places_scroll_changes > 0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn place_activation_records_target_path_and_hover() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
            ShellPlace::new("", "P", "Projects", PathBuf::from("/tmp/projects"), true),
        ];
        let size = PhysicalSize::new(700, 320);
        let projects_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: projects_row.x + 6.0,
            y: projects_row.y + 6.0,
        };

        assert_eq!(
            scene.place_activation_for_primary_press(point, size),
            Some(PathBuf::from("/tmp/projects"))
        );
        assert_eq!(scene.hovered_place, Some(1));
        assert_eq!(scene.hovered_index, None);
        assert_eq!(scene.places_changes, 1);
    }

    #[test]
    fn active_place_prefers_longest_matching_prefix() {
        let places = vec![
            ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ShellPlace::new("", "H", "Home", PathBuf::from("/home/yk"), false),
            ShellPlace::new("", "D", "Docs", PathBuf::from("/home/yk/Documents"), true),
        ];

        assert_eq!(
            active_shell_place_index(&places, Path::new("/home/yk/Documents/fika")),
            Some(2)
        );
        assert_eq!(
            active_shell_place_index(&places, Path::new("/home/yk/Code")),
            Some(1)
        );
        assert_eq!(
            active_shell_place_index(&places, Path::new("/etc")),
            Some(0)
        );
    }

    #[test]
    fn places_context_menu_opens_row_actions_without_selecting_items() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        scene.selection.apply_navigation(0, false);
        let size = PhysicalSize::new(700, 320);
        let root_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: root_row.x + 5.0,
            y: root_row.y + 5.0,
        };

        assert!(scene.open_context_menu(point, size));
        assert_eq!(scene.selection.len(), 1);
        assert_eq!(scene.hovered_place, Some(1));
        assert_eq!(scene.hovered_index, None);
        assert_eq!(scene.context_target_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Place {
                index: 1,
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                group: "Devices",
                device: None,
                network: false,
                trash: false,
                root: true,
                editable: false,
            })
        );

        let menu = scene.context_menu.as_ref().expect("menu should open");
        assert_eq!(
            context_menu_actions(&menu.target),
            &[
                ShellContextMenuAction::Open,
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );
        assert_eq!(
            scene.context_target_directory_path(),
            Some(PathBuf::from("/"))
        );

        let rect = context_menu_rect(menu, size);
        let copy_location_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 2.0 + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(copy_location_row, size),
            Some(ShellContextMenuAction::CopyLocation)
        );
        assert_eq!(scene.context_menu_actions, 1);
        assert!(scene.context_menu.is_none());
    }

    #[test]
    fn editable_places_context_menu_includes_remove_action() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
            ShellPlace::new("", "B", "Project", PathBuf::from("/tmp/project"), true),
        ];
        let size = PhysicalSize::new(700, 320);
        let project_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: project_row.x + 6.0,
            y: project_row.y + 6.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene.context_menu.as_ref().expect("menu should open");
        assert_eq!(
            context_menu_actions(&menu.target),
            &[
                ShellContextMenuAction::Open,
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::RemovePlace,
                ShellContextMenuAction::Properties,
            ]
        );
        let rect = context_menu_rect(menu, size);
        let remove_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 3.0 + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(remove_row, size),
            Some(ShellContextMenuAction::RemovePlace)
        );
    }

    #[test]
    fn directory_context_menu_includes_add_to_places_action() {
        let folder_target = ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/folder"),
            is_dir: true,
            selection_count: 1,
        };
        assert!(
            context_menu_actions(&folder_target).contains(&ShellContextMenuAction::AddToPlaces)
        );

        let file_target = ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        };
        assert!(!context_menu_actions(&file_target).contains(&ShellContextMenuAction::AddToPlaces));
        assert!(context_menu_actions(&file_target).contains(&ShellContextMenuAction::OpenWith));

        let blank_target = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::AddToPlaces));
        assert!(
            context_menu_actions(&blank_target)
                .contains(&ShellContextMenuAction::ToggleHiddenFiles)
        );
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::SplitPane));
        assert_eq!(
            ShellContextMenuAction::ToggleHiddenFiles.label_for_hidden_state(false),
            "Show Hidden Files"
        );
        assert_eq!(
            ShellContextMenuAction::ToggleHiddenFiles.label_for_hidden_state(true),
            "Hide Hidden Files"
        );
    }

    #[test]
    fn context_menu_items_offer_open_with_submenu_applications() {
        let target = ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        };
        let menu = ShellContextMenu::with_dynamic(
            target,
            ViewPoint { x: 20.0, y: 20.0 },
            vec![MimeApplication {
                id: "org.example.Editor.desktop".to_string(),
                desktop_file: PathBuf::from("/usr/share/applications/org.example.Editor.desktop"),
                name: "Editor".to_string(),
                exec: "editor %F".to_string(),
                icon: Some("accessories-text-editor".to_string()),
                is_default: true,
            }],
            Vec::new(),
        );

        assert!(
            context_menu_items(&menu)
                .iter()
                .any(|item| item.submenu == Some(ShellContextSubmenu::OpenWith))
        );
        let submenu = context_submenu_actions(ShellContextSubmenu::OpenWith, &menu);
        assert!(matches!(
            submenu.first().map(|item| &item.command),
            Some(ShellContextMenuCommand::OpenWithApplication { desktop_id })
                if desktop_id == "org.example.Editor.desktop"
        ));
        assert!(matches!(
            submenu.last().map(|item| &item.command),
            Some(ShellContextMenuCommand::Builtin(
                ShellContextMenuAction::OpenWith
            ))
        ));
    }

    #[test]
    fn context_menu_items_offer_service_root_more_and_group_submenus() {
        let target = ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/archive.zip"),
            is_dir: false,
            selection_count: 1,
        };
        let mut service_actions = Vec::new();
        service_actions.push(ServiceMenuAction {
            id: "compress.desktop::compress".to_string(),
            label: "Compress".to_string(),
            source_name: "Ark".to_string(),
            icon: Some("ark".to_string()),
            submenu: None,
            priority: ServiceMenuPriority::Normal,
        });
        service_actions.push(ServiceMenuAction {
            id: "tools.desktop::checksum".to_string(),
            label: "Checksum".to_string(),
            source_name: "Tools".to_string(),
            icon: None,
            submenu: Some("Tools".to_string()),
            priority: ServiceMenuPriority::Normal,
        });
        for index in 0..4 {
            service_actions.push(ServiceMenuAction {
                id: format!("extra.desktop::action{index}"),
                label: format!("Extra {index}"),
                source_name: "Extra".to_string(),
                icon: None,
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }
        let menu = ShellContextMenu::with_dynamic(
            target,
            ViewPoint { x: 20.0, y: 20.0 },
            Vec::new(),
            service_actions,
        );

        let root = context_menu_items(&menu);
        assert!(root.iter().any(|item| matches!(
            item.command,
            ShellContextMenuCommand::RunServiceMenuAction { .. }
        )));
        assert!(root.iter().any(|item| {
            item.submenu == Some(ShellContextSubmenu::ServiceMenu) && item.label == "More Actions"
        }));
        let more = context_submenu_actions(ShellContextSubmenu::ServiceMenu, &menu);
        assert!(more.iter().any(|item| {
            item.submenu == Some(ShellContextSubmenu::ServiceMenuGroup(0)) && item.label == "Tools"
        }));
        let tools = context_submenu_actions(ShellContextSubmenu::ServiceMenuGroup(0), &menu);
        assert!(tools.iter().any(|item| item.label == "Checksum"));
    }

    #[test]
    fn service_menu_named_icon_request_preserves_icon_name() {
        let action = ServiceMenuAction {
            id: "archive.desktop::compress".to_string(),
            label: "Compress".to_string(),
            source_name: "Archive".to_string(),
            icon: Some("archive-insert".to_string()),
            submenu: None,
            priority: ServiceMenuPriority::TopLevel,
        };
        let item = service_menu_action_item(&action);

        assert_eq!(
            context_menu_named_icon_request(&item),
            Some(("archive-insert", NamedIconFallback::Service))
        );
    }

    #[test]
    fn named_service_icon_candidates_prefer_service_icon() {
        let profile = file_icon_profile(
            &FileIconKind::Named {
                icon_name: "tools-checksum".to_string(),
                fallback: NamedIconFallback::Service,
            },
            fika_core::MimeDatabase::shared(),
        );

        assert_eq!(
            profile.icon_candidates.first().map(String::as_str),
            Some("tools-checksum")
        );
        assert!(
            profile
                .generic_candidates
                .iter()
                .any(|name| name == "configure")
        );
        assert!(
            profile
                .generic_candidates
                .iter()
                .any(|name| name == "system-run")
        );
    }

    #[test]
    fn icon_frame_keeps_overlay_vertices_separate() {
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut builder = IconFrameBuilder::new(
            &mut resolver,
            &mut thumbnails,
            &mut raster_cache,
            PhysicalSize::new(128, 96),
        );
        let raster = test_icon_raster(2, 7);
        builder.copy_raster_to_atlas(
            raster.clone(),
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            IconDrawLayer::Content,
        );
        builder.copy_raster_to_atlas(
            raster,
            ViewRect {
                x: 24.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            ViewRect {
                x: 24.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            IconDrawLayer::Overlay,
        );

        let frame = builder.finish();

        assert_eq!(frame.vertices.len(), 6);
        assert_eq!(frame.overlay_vertices.len(), 6);
        assert_eq!(frame.stats.quads, 2);
    }

    #[test]
    fn dialog_rects_scale_with_window_dpi() {
        let chooser = ShellOpenWithChooser::new(
            PathBuf::from("/tmp/plain.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "org.example.Editor.desktop".to_string(),
                desktop_file: PathBuf::from("/usr/share/applications/org.example.Editor.desktop"),
                name: "Editor".to_string(),
                exec: "editor %F".to_string(),
                icon: None,
                is_default: false,
            }],
        );
        let size = PhysicalSize::new(1200, 900);
        let base = open_with_chooser_rect(&chooser, size);
        let scaled = open_with_chooser_rect_scaled(&chooser, size, 1.5);
        assert!(scaled.width > base.width);
        assert!(scaled.height > base.height);
        assert_eq!(
            open_with_chooser_list_rect_scaled(scaled, &chooser, 1.5).height,
            scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, 1.5)
        );
    }

    #[test]
    fn file_icon_path_cache_keys_share_dolphin_role_across_sizes() {
        let path = Path::new("/tmp/plain.txt");
        let small =
            file_icon_path_cache_key(path, false, Some(Arc::from("text/plain")), true, 18.0);
        let large =
            file_icon_path_cache_key(path, false, Some(Arc::from("text/plain")), true, 48.0);

        assert_eq!(small.role, large.role);
        assert_ne!(small.size_px, large.size_px);
    }

    #[test]
    fn open_in_new_pane_loads_reusable_pane_state() {
        let root = test_dir("split-pane");
        let right = root.join("right");
        fs::create_dir_all(&right).unwrap();
        fs::write(right.join("child.txt"), "split").unwrap();

        let mut scene = test_scene(vec![test_entry("right", true)], ShellViewMode::Icons);
        scene.primary_pane.path = root.clone();
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: right.clone(),
            is_dir: true,
            selection_count: 1,
        });
        let size = PhysicalSize::new(900, 420);

        assert!(scene.open_split_pane_from_context(size).unwrap());
        let pane = scene.split_pane.as_ref().expect("split pane should load");
        assert_eq!(pane.path, right);
        assert_eq!(pane.view_mode, ShellViewMode::Icons);
        assert_eq!(pane.entries.len(), 1);
        assert_eq!(pane.entries[0].name.as_ref(), "child.txt");
        assert_eq!(pane.filtered_entry_count(), 1);
        assert_eq!(scene.split_pane_changes, 1);
        let metrics = scene
            .split_pane_metrics(size)
            .expect("split pane should expose geometry");
        assert!(scene.pane_width(size) < (size.width as f32 - scene.content_origin_x(size)));
        assert!(metrics.right_pane.x > scene.pane_rect(size).x);

        {
            let pane = scene.split_pane.as_mut().expect("split pane should load");
            pane.view_mode = ShellViewMode::Compact;
            pane.scroll_x = 12.0;
        }
        let pane = scene.split_pane.as_ref().expect("split pane should load");
        let view = ShellPaneView::from_state(pane);
        assert_eq!(view.path, right.as_path());
        assert_eq!(view.dir_count, 0);
        assert_eq!(view.scroll_x, 12.0);
        let layout = scene.pane_layout(view, metrics.right_pane.width, metrics.right_pane.height);
        match layout {
            ShellLayout::Compact(layout) => {
                assert_eq!(layout.visible_items().len(), 1);
                assert!(layout.content_size().width > 0.0);
            }
            _ => panic!("split pane view mode should drive reusable compact layout"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_with_chooser_opens_from_file_context_and_filters_applications() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/note.txt"),
            is_dir: false,
            selection_count: 1,
        });
        let list = fika_core::parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=writer.desktop;\n",
        );
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                test_desktop_application("writer.desktop", "Writer", "writer %f", &["text/plain"]),
                test_desktop_application("paint.desktop", "Paint", "paint %f", &["image/png"]),
            ],
            &[list],
        );

        assert!(scene.open_open_with_chooser_from_context(&cache));
        let chooser = scene
            .open_with_chooser
            .as_ref()
            .expect("chooser should open");
        assert_eq!(chooser.path, PathBuf::from("/tmp/note.txt"));
        assert_eq!(chooser.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(
            chooser
                .applications
                .iter()
                .map(|application| application.id.as_str())
                .collect::<Vec<_>>(),
            vec!["writer.desktop", "viewer.desktop", "paint.desktop"]
        );
        assert_eq!(
            chooser.selected_application().map(|app| app.id.as_str()),
            Some("writer.desktop")
        );

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("paint".to_string())));
        let chooser = scene.open_with_chooser.as_ref().unwrap();
        assert_eq!(chooser.filtered_count(), 1);
        assert_eq!(
            chooser.selected_application().map(|app| app.id.as_str()),
            Some("paint.desktop")
        );
    }

    #[test]
    fn open_with_chooser_click_selects_row_and_buttons_close_or_commit() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![
                MimeApplication {
                    id: "viewer.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/viewer.desktop"),
                    name: "Viewer".to_string(),
                    exec: "viewer %f".to_string(),
                    icon: None,
                    is_default: false,
                },
                MimeApplication {
                    id: "writer.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/writer.desktop"),
                    name: "Writer".to_string(),
                    exec: "writer %f".to_string(),
                    icon: None,
                    is_default: false,
                },
            ],
        ));
        let size = PhysicalSize::new(640, 420);
        let rect = open_with_chooser_rect(scene.open_with_chooser.as_ref().unwrap(), size);
        let list = open_with_chooser_list_rect(rect, scene.open_with_chooser.as_ref().unwrap());

        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: list.x + 4.0,
                    y: list.y + OPEN_WITH_CHOOSER_ROW_HEIGHT + 4.0,
                },
                size,
            ),
            OpenWithChooserClick::Row(1)
        );
        assert!(scene.select_open_with_filtered_row(1));
        assert_eq!(
            scene
                .open_with_chooser
                .as_ref()
                .unwrap()
                .selected_application()
                .map(|application| application.id.as_str()),
            Some("writer.desktop")
        );
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: open_with_chooser_open_button_rect(rect).x + 2.0,
                    y: open_with_chooser_open_button_rect(rect).y + 2.0,
                },
                size,
            ),
            OpenWithChooserClick::Open
        );
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            OpenWithChooserClick::Outside
        );
    }

    #[test]
    fn open_with_chooser_builds_launch_plan_for_selected_application() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "writer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/writer.desktop"),
                name: "Writer".to_string(),
                exec: "writer %f".to_string(),
                icon: None,
                is_default: true,
            }],
        ));
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "writer.desktop",
                "Writer",
                "writer --line %f",
                &["text/plain"],
            )],
            &[],
        );

        let request = scene.open_with_launch_request(&cache).unwrap();

        assert_eq!(request.path, PathBuf::from("/tmp/note.txt"));
        assert_eq!(request.app_name, "Writer");
        assert_eq!(request.plan.commands[0].program, "writer");
        assert_eq!(
            request.plan.commands[0].args,
            vec!["--line", "/tmp/note.txt"]
        );
    }

    #[test]
    fn trash_context_menu_uses_restore_delete_and_empty_actions() {
        let trash_item = ShellContextTarget::Item {
            index: 0,
            path: file_ops::trash_files_dir().join("trashed.txt"),
            is_dir: false,
            selection_count: 1,
        };
        assert_eq!(
            context_menu_actions(&trash_item),
            &[
                ShellContextMenuAction::RestoreFromTrash,
                ShellContextMenuAction::Copy,
                ShellContextMenuAction::DeletePermanently,
                ShellContextMenuAction::Properties,
            ]
        );

        let trash_blank = ShellContextTarget::Blank {
            path: file_ops::trash_files_dir(),
        };
        assert_eq!(
            context_menu_actions(&trash_blank),
            &[
                ShellContextMenuAction::EmptyTrash,
                ShellContextMenuAction::SelectAll,
                ShellContextMenuAction::Refresh,
                ShellContextMenuAction::Properties,
            ]
        );

        let trash_place = ShellContextTarget::Place {
            index: 0,
            label: "Trash".to_string(),
            path: file_ops::trash_files_dir(),
            group: "",
            device: None,
            network: false,
            trash: true,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&trash_place),
            &[
                ShellContextMenuAction::Open,
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::EmptyTrash,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );

        let normal_blank = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        assert!(!context_menu_actions(&normal_blank).contains(&ShellContextMenuAction::EmptyTrash));
    }

    #[test]
    fn build_shell_places_applies_persistent_primary_order() {
        let root = test_dir("build-shell-places-order");
        let places_path = root.join("places.xbel");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&beta).unwrap();
        save_user_places(
            &places_path,
            &[
                UserPlace::new("Alpha".to_string(), alpha.clone()),
                UserPlace::new("Beta".to_string(), beta.clone()),
            ],
        )
        .unwrap();
        save_place_order(
            &place_order_path_for_user_places_path(&places_path),
            &[beta.clone(), alpha.clone()],
        )
        .unwrap();

        let places = build_shell_places_from(&places_path);
        let beta_index = places
            .iter()
            .position(|place| place.path == beta)
            .expect("beta place should be loaded");
        let alpha_index = places
            .iter()
            .position(|place| place.path == alpha)
            .expect("alpha place should be loaded");

        assert!(beta_index < alpha_index);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_shell_places_projects_mounted_devices() {
        let root = test_dir("build-shell-places-devices");
        let places_path = root.join("places.xbel");
        let usb = root.join("USB");
        fs::create_dir_all(&usb).unwrap();
        let devices = vec![
            DeviceInfo {
                id: "mounted-usb".to_string(),
                mount_point: Some(usb.clone()),
                uri: Some("file:///run/media/yk/USB".to_string()),
                filesystem_type: Some("vfat".to_string()),
                label: Some("USB Drive".to_string()),
                capacity_bytes: Some(16 * 1024 * 1024),
                removable: true,
                mounted: true,
                ejectable: true,
                can_power_off: false,
            },
            DeviceInfo {
                id: "unmounted".to_string(),
                mount_point: None,
                uri: Some("file:///dev/sdb1".to_string()),
                filesystem_type: None,
                label: Some("Unmounted".to_string()),
                capacity_bytes: None,
                removable: true,
                mounted: false,
                ejectable: true,
                can_power_off: false,
            },
        ];

        let places = build_shell_places_from_with_devices(&places_path, &devices);

        assert!(places.iter().any(|place| {
            place.group == "Devices"
                && place.marker == "D"
                && place.label == "USB Drive"
                && place.path == usb
                && place.device.as_ref().is_some_and(|device| device.mounted)
        }));
        assert!(places.iter().any(|place| {
            place.group == "Devices"
                && place.marker == "D"
                && place.label == "Unmounted"
                && place.path == PathBuf::from("unmounted")
                && place
                    .device
                    .as_ref()
                    .is_some_and(|device| !device.mounted && device.ejectable)
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn device_place_context_menu_uses_mount_and_eject_actions() {
        let mounted = ShellContextTarget::Place {
            index: 0,
            label: "USB Drive".to_string(),
            path: PathBuf::from("/run/media/USB"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "mounted-usb".to_string(),
                mounted: true,
                ejectable: true,
                can_power_off: true,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&mounted),
            &[
                ShellContextMenuAction::Open,
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::UnmountDevice,
                ShellContextMenuAction::EjectDevice,
                ShellContextMenuAction::SafelyRemoveDevice,
                ShellContextMenuAction::Properties,
            ]
        );

        let unmounted = ShellContextTarget::Place {
            index: 1,
            label: "USB Drive".to_string(),
            path: PathBuf::from("gio:volume:usb"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "gio:volume:usb".to_string(),
                mounted: false,
                ejectable: true,
                can_power_off: false,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&unmounted),
            &[
                ShellContextMenuAction::MountDevice,
                ShellContextMenuAction::EjectDevice,
                ShellContextMenuAction::Properties,
            ]
        );
    }

    #[test]
    fn context_target_device_action_preserves_device_id() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "USB Drive".to_string(),
            path: PathBuf::from("gio:volume:usb"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "gio:volume:usb".to_string(),
                mounted: false,
                ejectable: true,
                can_power_off: false,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        });

        assert_eq!(
            scene.context_target_device_action(ShellContextMenuAction::MountDevice),
            Some(DeviceActionRequest {
                id: "gio:volume:usb".to_string(),
                label: "USB Drive".to_string(),
                action: ShellContextMenuAction::MountDevice,
            })
        );
    }

    #[test]
    fn add_user_place_at_path_updates_xbel_and_rejects_duplicates() {
        let root = test_dir("add-user-place");
        let places_path = root.join("places.xbel");
        let target = root.join("project");
        fs::create_dir_all(&target).unwrap();

        assert!(add_user_place_at_path(&places_path, &target, "Project".to_string()).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Project".to_string(), target.clone())]
        );
        assert!(!add_user_place_at_path(&places_path, &target, "Again".to_string()).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Project".to_string(), target.clone())]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn add_context_target_to_places_reloads_places_and_persists_order() {
        let root = test_dir("add-context-place");
        let places_path = root.join("places.xbel");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let size = PhysicalSize::new(700, 320);
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            path: project.clone(),
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "stale".to_string(),
            rows: Vec::new(),
        });

        assert!(
            scene
                .add_context_target_to_places(&places_path, size)
                .unwrap()
        );
        assert!(scene.places.iter().any(|place| place.path == project));
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.places_changes, 1);
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("project".to_string(), project.clone())]
        );
        assert!(
            load_place_order(&place_order_path_for_user_places_path(&places_path))
                .unwrap()
                .iter()
                .any(|path| path == &project)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_user_place_at_path_updates_xbel_and_order() {
        let root = test_dir("remove-user-place");
        let places_path = root.join("places.xbel");
        let keep = PathBuf::from("/tmp/keep-place");
        let remove = PathBuf::from("/tmp/remove-place");
        save_user_places(
            &places_path,
            &[
                UserPlace::new("Keep".to_string(), keep.clone()),
                UserPlace::new("Remove".to_string(), remove.clone()),
            ],
        )
        .unwrap();
        let order_path = place_order_path_for_user_places_path(&places_path);
        save_place_order(&order_path, &[remove.clone(), keep.clone()]).unwrap();

        assert!(remove_user_place_at_path(&places_path, &remove).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Keep".to_string(), keep.clone())]
        );
        assert_eq!(load_place_order(&order_path).unwrap(), vec![keep]);
        assert!(!remove_user_place_at_path(&places_path, &remove).unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_context_place_reloads_places_and_clears_context_state() {
        let root = test_dir("remove-context-place");
        let places_path = root.join("places.xbel");
        let remove = PathBuf::from("/tmp/remove-context-place-target");
        save_user_places(
            &places_path,
            &[UserPlace::new("Remove Me".to_string(), remove.clone())],
        )
        .unwrap();
        let size = PhysicalSize::new(700, 320);
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = build_shell_places_from(&places_path);
        assert!(scene.places.iter().any(|place| place.path == remove));
        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Remove Me".to_string(),
            path: remove.clone(),
            group: "",
            device: None,
            network: false,
            trash: false,
            root: false,
            editable: true,
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "stale".to_string(),
            rows: Vec::new(),
        });

        assert!(scene.remove_context_place(&places_path, size).unwrap());
        assert!(!scene.places.iter().any(|place| place.path == remove));
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.places_changes, 1);
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            Vec::<UserPlace>::new()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_click_supports_single_toggle_and_range() {
        let mut selection = ShellSelection::default();

        assert!(selection.apply_click(Some(2), false, false));
        assert!(selection.contains(2));
        assert_eq!(selection.anchor, Some(2));

        assert!(selection.apply_click(Some(4), false, true));
        assert!(selection.contains(2));
        assert!(selection.contains(4));
        assert_eq!(selection.anchor, Some(4));

        assert!(selection.apply_click(Some(1), true, false));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(selection.anchor, Some(4));

        assert!(selection.apply_click(None, false, false));
        assert_eq!(selection.len(), 0);
        assert_eq!(selection.anchor, None);
    }

    #[test]
    fn selection_commands_select_all_and_clear_scene_state() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );

        assert!(scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(scene.selection.len(), 3);
        assert_eq!(scene.selection.anchor, Some(0));
        assert_eq!(scene.selection.focus, Some(2));
        assert_eq!(scene.selection_changes, 1);
        assert!(!scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(scene.selection_changes, 1);

        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            scene.selection.clone(),
        ));
        assert!(scene.apply_selection_command(SelectionCommand::Clear));
        assert_eq!(scene.selection.len(), 0);
        assert_eq!(scene.selection.anchor, None);
        assert_eq!(scene.selection.focus, None);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.selection_changes, 2);
        assert!(!scene.apply_selection_command(SelectionCommand::Clear));
    }

    #[test]
    fn context_target_selects_unselected_item_from_retained_hit_test() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", true),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 340);
        let item = scene
            .layout(size)
            .item(1)
            .expect("second item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(
            scene.selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(scene.selection.focus, Some(1));
        assert_eq!(scene.hovered_index, Some(1));
        assert_eq!(scene.selection_changes, 1);
        assert_eq!(scene.context_target_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Item {
                index: 1,
                path: PathBuf::from("/tmp/bravo.txt"),
                is_dir: false,
                selection_count: 1,
            })
        );
    }

    #[test]
    fn context_target_preserves_multi_selection_for_selected_item() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        assert!(scene.selection.select_indexes(&[0, 2]));
        let item = scene
            .layout(size)
            .item(0)
            .expect("first item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(
            scene.selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(scene.selection.focus, Some(0));
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Item {
                index: 0,
                path: PathBuf::from("/tmp/alpha.txt"),
                is_dir: false,
                selection_count: 2,
            })
        );
    }

    #[test]
    fn context_target_uses_blank_content_without_rubber_band() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        assert!(scene.selection.apply_navigation(0, false));
        scene.rubber_band = Some(RubberBand {
            start: ViewPoint { x: 0.0, y: 0.0 },
            current: ViewPoint { x: 12.0, y: 12.0 },
            active: true,
            mode: RubberBandMode::Replace,
            base_selection: scene.selection.clone(),
        });
        let content = scene.content_screen_rect(size);
        let point = ViewPoint {
            x: content.right() - 4.0,
            y: scene.content_origin_y() + 4.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(scene.selection.len(), 1);
        assert!(scene.selection.contains(0));
        assert_eq!(scene.hovered_index, None);
        assert!(scene.rubber_band.is_none());
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Blank {
                path: PathBuf::from("/tmp"),
            })
        );

        let status_point = ViewPoint {
            x: scene.status_bar_rect(size).x + 10.0,
            y: scene.status_bar_rect(size).y + 2.0,
        };
        assert!(scene.open_context_target(status_point, size));
        assert_eq!(scene.context_target, None);
    }

    #[test]
    fn context_menu_opens_item_actions_and_records_action_hits() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let item = scene.layout(size).item(0).expect("item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene
            .context_menu
            .as_ref()
            .expect("context menu should open");
        assert!(matches!(
            menu.target,
            ShellContextTarget::Item {
                index: 0,
                is_dir: true,
                ..
            }
        ));
        assert_eq!(
            context_menu_actions(&menu.target).first().copied(),
            Some(ShellContextMenuAction::Open)
        );

        let rect = context_menu_rect(menu, size);
        let first_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + 8.0,
        };
        assert_eq!(
            scene.context_menu_action_at_screen_point(first_row, size),
            Some(ShellContextMenuAction::Open)
        );
        assert!(scene.set_pointer(first_row, size));
        assert_eq!(
            scene
                .context_menu
                .as_ref()
                .and_then(|menu| menu.hovered_row),
            Some(0)
        );
        assert_eq!(
            scene.activate_or_close_context_menu(first_row, size),
            Some(ShellContextMenuAction::Open)
        );
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.context_menu_actions, 1);
        assert_eq!(
            scene.context_target_directory_path(),
            Some(PathBuf::from("/tmp/folder"))
        );
    }

    #[test]
    fn context_menu_clamps_blank_actions_inside_window() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        let size = PhysicalSize::new(240, 180);
        let content = scene.content_screen_rect(size);
        let point = ViewPoint {
            x: content.right() - 2.0,
            y: content.bottom() - 2.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene
            .context_menu
            .as_ref()
            .expect("blank context menu should open");
        assert!(matches!(menu.target, ShellContextTarget::Blank { .. }));
        assert_eq!(
            context_menu_actions(&menu.target).first().copied(),
            Some(ShellContextMenuAction::CreateNew)
        );
        let rect = context_menu_rect(menu, size);
        assert!(rect.x >= CONTEXT_MENU_VIEWPORT_MARGIN);
        assert!(rect.y >= CONTEXT_MENU_VIEWPORT_MARGIN);
        assert!(rect.right() <= size.width as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
        assert!(rect.bottom() <= size.height as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);

        assert_eq!(
            scene.activate_or_close_context_menu(
                ViewPoint {
                    x: CONTEXT_MENU_VIEWPORT_MARGIN,
                    y: CONTEXT_MENU_VIEWPORT_MARGIN,
                },
                size,
            ),
            None
        );
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.context_menu_actions, 0);
    }

    #[test]
    fn context_menu_uses_original_metrics_and_flips_near_edges() {
        let target = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        let menu = ShellContextMenu::new(target, ViewPoint { x: 390.0, y: 280.0 });
        let size = PhysicalSize::new(420, 320);
        let rect = context_menu_rect(&menu, size);

        assert_eq!(rect.width, 196.0);
        assert_eq!(
            rect.height,
            CONTEXT_MENU_VERTICAL_PADDING * 2.0
                + context_menu_actions(&menu.target).len() as f32 * CONTEXT_MENU_ROW_HEIGHT
        );
        assert!(rect.x < menu.position.x);
        assert!(rect.y < menu.position.y);
        assert!(rect.right() <= size.width as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
        assert!(rect.bottom() <= size.height as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
    }

    #[test]
    fn context_menu_hit_testing_respects_vertical_padding() {
        let target = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        let menu = ShellContextMenu::new(target, ViewPoint { x: 40.0, y: 40.0 });
        let size = PhysicalSize::new(420, 320);
        let rect = context_menu_rect(&menu, size);

        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.y + 2.0
                },
                size,
                1.0,
            ),
            None
        );
        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0
                },
                size,
                1.0,
            ),
            Some(0)
        );
        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.bottom() - 2.0
                },
                size,
                1.0,
            ),
            None
        );
    }

    #[test]
    fn context_menu_separator_rows_match_original_grouping() {
        let blank = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        let blank_actions = context_menu_actions(&blank);
        let paste_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Paste)
            .unwrap();
        let select_all_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::SelectAll)
            .unwrap();
        let toggle_hidden_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::ToggleHiddenFiles)
            .unwrap();
        let split_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::SplitPane)
            .unwrap();
        let properties_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Properties)
            .unwrap();

        assert!(!context_menu_separator_before(&blank, 0));
        assert!(context_menu_separator_before(&blank, paste_row));
        assert!(context_menu_separator_before(&blank, select_all_row));
        assert!(context_menu_separator_before(&blank, toggle_hidden_row));
        assert!(!context_menu_separator_before(&blank, split_row));
        assert!(context_menu_separator_before(&blank, properties_row));

        let item = ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/file.txt"),
            is_dir: false,
            selection_count: 1,
        };
        let item_actions = context_menu_actions(&item);
        let copy_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Copy)
            .unwrap();
        let rename_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Rename)
            .unwrap();
        let properties_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Properties)
            .unwrap();

        assert!(!context_menu_separator_before(&item, 0));
        assert!(context_menu_separator_before(&item, copy_row));
        assert!(context_menu_separator_before(&item, rename_row));
        assert!(context_menu_separator_before(&item, properties_row));
    }

    #[test]
    fn context_menu_blank_actions_can_hit_select_all_and_refresh() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let content = scene.content_screen_rect(size);
        let point = ViewPoint {
            x: content.right() - 4.0,
            y: scene.content_origin_y() + 4.0,
        };

        assert!(scene.open_context_menu(point, size));
        let rect = context_menu_rect(scene.context_menu.as_ref().unwrap(), size);
        let select_all_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 3.0 + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(select_all_row, size),
            Some(ShellContextMenuAction::SelectAll)
        );
        assert_eq!(scene.context_menu_actions, 1);

        assert!(scene.open_context_menu(point, size));
        let rect = context_menu_rect(scene.context_menu.as_ref().unwrap(), size);
        let refresh_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 6.0 + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(refresh_row, size),
            Some(ShellContextMenuAction::Refresh)
        );
        assert_eq!(scene.context_menu_actions, 2);
    }

    #[test]
    fn context_target_directory_path_only_resolves_directory_items() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/folder"),
            is_dir: true,
            selection_count: 1,
        });
        assert_eq!(
            scene.context_target_directory_path(),
            Some(PathBuf::from("/tmp/folder"))
        );

        scene.context_target = Some(ShellContextTarget::Item {
            index: 1,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert_eq!(scene.context_target_directory_path(), None);

        scene.context_target = Some(ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        });
        assert_eq!(scene.context_target_directory_path(), None);

        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            group: "Devices",
            device: None,
            network: false,
            trash: false,
            root: true,
            editable: false,
        });
        assert_eq!(
            scene.context_target_directory_path(),
            Some(PathBuf::from("/"))
        );
    }

    #[test]
    fn open_file_request_only_resolves_file_context_targets() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        });
        assert_eq!(scene.context_target_open_file_request(), None);

        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/folder"),
            is_dir: true,
            selection_count: 1,
        });
        assert_eq!(scene.context_target_open_file_request(), None);

        scene.context_target = Some(ShellContextTarget::Item {
            index: 1,
            path: PathBuf::from("/tmp/Fika Test/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert_eq!(
            scene.context_target_open_file_request(),
            Some(OpenFileRequest {
                path: PathBuf::from("/tmp/Fika Test/plain.txt"),
                uri: "file:///tmp/Fika%20Test/plain.txt".to_string(),
            })
        );
        assert_eq!(scene.open_changes, 0);
    }

    #[test]
    fn open_file_request_preserves_network_uri_targets() {
        let mut scene = test_scene(vec![test_entry("remote.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("sftp://example.test/home/yk/remote.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert_eq!(
            scene.context_target_open_file_request(),
            Some(OpenFileRequest {
                path: PathBuf::from("sftp://example.test/home/yk/remote.txt"),
                uri: "sftp://example.test/home/yk/remote.txt".to_string(),
            })
        );
    }

    #[test]
    fn copy_location_request_uses_target_display_path() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        });
        assert_eq!(scene.context_target_copy_location_request(), None);

        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/Fika Test/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        let request = scene
            .context_target_copy_location_request()
            .expect("item target should produce copy location request");
        assert_eq!(
            request,
            CopyLocationRequest {
                path: PathBuf::from("/tmp/Fika Test/plain.txt"),
                text: "/tmp/Fika Test/plain.txt".to_string(),
            }
        );

        scene.record_copy_location(&request);
        assert_eq!(scene.copy_location_changes, 1);

        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            group: "Devices",
            device: None,
            network: false,
            trash: false,
            root: true,
            editable: false,
        });
        assert_eq!(
            scene.context_target_copy_location_request(),
            Some(CopyLocationRequest {
                path: PathBuf::from("/"),
                text: "/".to_string(),
            })
        );
    }

    #[test]
    fn file_clipboard_request_uses_multi_selection_and_rejects_remote_cut() {
        let mut scene = test_scene(
            vec![
                test_entry("one.txt", false),
                test_entry("two.txt", false),
                test_entry("remote.txt", false),
            ],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            index: 1,
            path: PathBuf::from("/tmp/two.txt"),
            is_dir: false,
            selection_count: 2,
        });
        scene.selection.select_indexes(&[0, 1]);

        let request = scene
            .context_target_file_clipboard_request(ShellContextMenuAction::Copy)
            .unwrap()
            .expect("selected item target should produce clipboard request");
        assert_eq!(request.role, FileClipboardRole::Copy);
        assert_eq!(
            request.paths,
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );
        assert_eq!(
            request.text,
            encode_file_clipboard_text(FileClipboardRole::Copy, &request.paths)
        );

        scene.record_file_clipboard_export(&request);
        assert_eq!(scene.file_clipboard_changes, 1);

        scene.context_target = Some(ShellContextTarget::Item {
            index: 2,
            path: PathBuf::from("sftp://example.test/home/yk/remote.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .context_target_file_clipboard_request(ShellContextMenuAction::Cut)
                .unwrap_err()
                .contains("remote cut")
        );
    }

    #[test]
    fn paste_file_clipboard_copies_files_reloads_and_keeps_clipboard() {
        let source_root = test_dir("paste-file-source");
        let target_root = test_dir("paste-file-target");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        fs::write(source_root.join("source.txt"), b"source").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(target_root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            path: target_root.clone(),
        });
        let clipboard_text =
            encode_file_clipboard_text(FileClipboardRole::Copy, &[source_root.join("source.txt")]);

        let result = scene
            .paste_clipboard_text_from_context(&clipboard_text, size)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!result.clear_clipboard);
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert!(target_root.join("source.txt").is_file());
        assert_eq!(fs::read(target_root.join("source.txt")).unwrap(), b"source");
        assert!(entry_index_by_name(&scene.primary_pane.entries, "source.txt").is_some());

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn paste_cut_file_moves_file_and_requests_clipboard_clear() {
        let source_root = test_dir("paste-cut-source");
        let target_root = test_dir("paste-cut-target");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        fs::write(source_root.join("move.txt"), b"move").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(target_root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            path: target_root.clone(),
        });
        let clipboard_text =
            encode_file_clipboard_text(FileClipboardRole::Cut, &[source_root.join("move.txt")]);

        let result = scene
            .paste_clipboard_text_from_context(&clipboard_text, size)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Move);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(result.clear_clipboard);
        assert!(!source_root.join("move.txt").exists());
        assert!(target_root.join("move.txt").is_file());
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn paste_plain_text_creates_unique_text_file() {
        let root = test_dir("paste-text");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank { path: root.clone() });

        let result = scene
            .paste_clipboard_text_from_context("hello from clipboard", size)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!result.clear_clipboard);
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert_eq!(
            fs::read_to_string(root.join("Pasted Text.txt")).unwrap(),
            "hello from clipboard"
        );
        assert!(entry_index_by_name(&scene.primary_pane.entries, "Pasted Text.txt").is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn properties_overlay_builds_item_metadata_from_context_target() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            index: 1,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 2,
        });

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - plain.txt");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Name" && row.value == "plain.txt")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "File")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Selection" && row.value == "2 items")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Path" && row.value == "/tmp/plain.txt")
        );
        assert_eq!(scene.properties_changes, 1);
    }

    #[test]
    fn properties_overlay_builds_blank_directory_summary_and_closes_outside() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        });
        let size = PhysicalSize::new(360, 240);

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - /tmp");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Entries" && row.value == "2")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Folders" && row.value == "1")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Files" && row.value == "1")
        );
        let rect = properties_overlay_rect(overlay, size);
        assert!(rect.x >= PROPERTIES_OVERLAY_MARGIN);
        assert!(rect.y >= PROPERTIES_OVERLAY_MARGIN);
        assert!(!scene.close_properties_overlay_if_outside(
            ViewPoint {
                x: rect.x + 2.0,
                y: rect.y + 2.0,
            },
            size,
        ));
        assert!(scene.close_properties_overlay_if_outside(ViewPoint { x: 1.0, y: 1.0 }, size,));
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.properties_changes, 2);
    }

    #[test]
    fn properties_overlay_builds_place_metadata_from_context_target() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            group: "Devices",
            device: None,
            network: false,
            trash: false,
            root: true,
            editable: false,
        });

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - Root");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "Place")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Section" && row.value == "Devices")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Root" && row.value == "Yes")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Path" && row.value == "/")
        );
        assert_eq!(scene.properties_changes, 1);
    }

    #[test]
    fn create_dialog_opens_from_blank_context_and_accepts_text() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let root = test_dir("create-dialog");
        scene.context_target = Some(ShellContextTarget::Blank { path: root });

        assert!(scene.open_create_dialog_from_context());
        let dialog = scene
            .create_dialog
            .as_ref()
            .expect("create dialog should open");
        assert_eq!(dialog.kind, CreateEntryKind::Folder);
        assert_eq!(dialog.name, "New Folder");
        assert_eq!(scene.create_changes, 1);

        assert!(scene.apply_create_command(CreateCommand::Insert("custom".to_string()), size));
        assert_eq!(scene.create_dialog.as_ref().unwrap().name, "custom");
        assert!(scene.apply_create_command(CreateCommand::SetKind(CreateEntryKind::File), size));
        let dialog = scene.create_dialog.as_ref().unwrap();
        assert_eq!(dialog.kind, CreateEntryKind::File);
        assert_eq!(dialog.name, "New File");
        assert_eq!(
            scene.create_dialog_click_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            CreateDialogClick::Outside
        );
        let rect = create_dialog_rect(dialog, size);
        assert_eq!(
            scene.create_dialog_click_at_screen_point(
                ViewPoint {
                    x: create_dialog_commit_button_rect(rect).x + 2.0,
                    y: create_dialog_commit_button_rect(rect).y + 2.0,
                },
                size,
            ),
            CreateDialogClick::Commit
        );
    }

    #[test]
    fn create_entry_request_rejects_invalid_names_and_records_error() {
        let mut scene = test_scene(vec![], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        });
        assert!(scene.open_create_dialog_from_context());
        scene.create_dialog.as_mut().unwrap().name = "../bad".to_string();

        let error = scene.create_entry_request().unwrap_err();
        assert!(error.contains('/'));
        assert!(scene.set_create_dialog_error(error));
        let dialog = scene.create_dialog.as_ref().unwrap();
        assert!(dialog.error.as_ref().unwrap().contains('/'));
        assert_eq!(scene.create_changes, 2);
    }

    #[test]
    fn create_new_folder_creates_on_disk_reloads_and_selects_entry() {
        let root = test_dir("create-folder");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank { path: root.clone() });

        assert!(scene.open_create_dialog_from_context());
        scene.create_dialog.as_mut().unwrap().name = "made".to_string();
        scene.create_dialog.as_mut().unwrap().replace_on_insert = false;
        let request = scene.create_entry_request().unwrap();
        assert_eq!(request.kind, CreateEntryKind::Folder);
        create_entry_on_disk(&request).unwrap();
        assert!(root.join("made").is_dir());
        assert!(scene.close_create_dialog_after_success(&request));
        assert!(scene.reload_current_path(size).unwrap());
        assert!(scene.select_entry_by_name("made", size));

        let index = entry_index_by_name(&scene.primary_pane.entries, "made").unwrap();
        assert!(scene.primary_pane.entries[index].is_dir);
        assert!(scene.selection.contains(index));
        assert!(scene.create_dialog.is_none());
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_new_file_uses_create_new_and_unique_default_name() {
        let root = test_dir("create-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("New File"), b"existing").unwrap();
        let mut dialog = ShellCreateDialog::new(root.clone(), CreateEntryKind::File);
        assert_eq!(dialog.name, "New File 2");
        dialog.name = "note.txt".to_string();
        let request = CreateEntryRequest {
            parent: root.clone(),
            path: root.join("note.txt"),
            kind: CreateEntryKind::File,
            name: "note.txt".to_string(),
        };

        create_entry_on_disk(&request).unwrap();
        assert!(root.join("note.txt").is_file());
        assert!(
            create_entry_on_disk(&request)
                .unwrap_err()
                .contains("create file")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_dialog_opens_from_item_context_and_accepts_text() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert!(scene.open_rename_dialog_from_context());
        let dialog = scene
            .rename_dialog
            .as_ref()
            .expect("rename dialog should open");
        assert_eq!(dialog.original_name, "plain.txt");
        assert_eq!(dialog.name, "plain.txt");
        assert!(!dialog.is_dir);
        assert_eq!(scene.rename_changes, 1);

        assert!(scene.apply_rename_command(RenameCommand::Insert("renamed.txt".to_string())));
        assert_eq!(scene.rename_dialog.as_ref().unwrap().name, "renamed.txt");
        let rect = rename_dialog_rect(scene.rename_dialog.as_ref().unwrap(), size);
        assert_eq!(
            scene.rename_dialog_click_at_screen_point(
                ViewPoint {
                    x: rename_dialog_commit_button_rect(rect).x + 2.0,
                    y: rename_dialog_commit_button_rect(rect).y + 2.0,
                },
                size,
            ),
            RenameDialogClick::Commit
        );
        assert_eq!(
            scene.rename_dialog_click_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            RenameDialogClick::Outside
        );
    }

    #[test]
    fn rename_entry_request_rejects_unchanged_and_invalid_names() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(scene.open_rename_dialog_from_context());

        let unchanged = scene.rename_entry_request().unwrap_err();
        assert!(unchanged.contains("unchanged"));
        assert!(scene.set_rename_dialog_error(unchanged));
        assert!(scene.apply_rename_command(RenameCommand::Insert("../bad".to_string())));
        let invalid = scene.rename_entry_request().unwrap_err();
        assert!(invalid.contains('/'));
        assert!(scene.set_rename_dialog_error(invalid));
        assert!(scene.rename_dialog.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn rename_file_creates_request_renames_on_disk_reloads_and_selects_entry() {
        let root = test_dir("rename-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old.txt"), b"old").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        let old_index = entry_index_by_name(&scene.primary_pane.entries, "old.txt").unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            index: old_index,
            path: root.join("old.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert!(scene.open_rename_dialog_from_context());
        scene.rename_dialog.as_mut().unwrap().name = "new.txt".to_string();
        scene.rename_dialog.as_mut().unwrap().replace_on_insert = false;
        let request = scene.rename_entry_request().unwrap();
        assert_eq!(request.original_name, "old.txt");
        assert_eq!(request.name, "new.txt");
        rename_entry_on_disk(&request).unwrap();
        assert!(!root.join("old.txt").exists());
        assert!(root.join("new.txt").is_file());
        assert!(scene.close_rename_dialog_after_success(&request));
        assert!(scene.reload_current_path(size).unwrap());
        assert!(scene.select_entry_by_name("new.txt", size));

        let new_index = entry_index_by_name(&scene.primary_pane.entries, "new.txt").unwrap();
        assert!(scene.selection.contains(new_index));
        assert!(scene.rename_dialog.is_none());
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trash_context_target_uses_multi_selection_and_rejects_remote_paths() {
        let mut scene = test_scene(
            vec![
                test_entry("one.txt", false),
                test_entry("two.txt", false),
                test_entry("remote", false),
            ],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            index: 1,
            path: PathBuf::from("/tmp/two.txt"),
            is_dir: false,
            selection_count: 2,
        });
        scene.selection.select_indexes(&[0, 1]);

        assert_eq!(
            scene.context_target_trash_paths().unwrap(),
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );

        scene.context_target = Some(ShellContextTarget::Item {
            index: 2,
            path: PathBuf::from("sftp://example.test/home/remote"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .move_context_target_to_trash(PhysicalSize::new(420, 260))
                .unwrap_err()
                .contains("remote trash")
        );
        assert_eq!(scene.trash_changes, 0);
    }

    #[test]
    fn trash_view_operation_requests_validate_context_targets() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        let trash_path = file_ops::trash_files_dir().join("plain.txt");
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });
        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::RestoreFromTrash)
            .unwrap();
        assert!(matches!(operation, TrashViewOperation::Restore { .. }));
        assert_eq!(paths, vec![trash_path.clone()]);

        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::DeletePermanently)
            .unwrap();
        assert_eq!(operation, TrashViewOperation::DeletePermanently);
        assert_eq!(paths, vec![trash_path]);

        scene.context_target = Some(ShellContextTarget::Blank {
            path: file_ops::trash_files_dir(),
        });
        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::EmptyTrash)
            .unwrap();
        assert_eq!(operation, TrashViewOperation::Empty);
        assert!(paths.is_empty());

        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .context_target_trash_view_operation(ShellContextMenuAction::RestoreFromTrash)
                .unwrap_err()
                .contains("inside Trash")
        );
    }

    #[test]
    fn restore_trash_view_action_restores_test_file_and_reloads() {
        let root = test_dir("restore-trash-view");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("restore-me.txt");
        fs::write(&source, b"restore").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        let size = PhysicalSize::new(420, 260);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.selection.select_indexes(&[0]);

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::RestoreFromTrash, size)
            .unwrap();

        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(source.is_file());
        assert!(!trash_path.exists());
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.selection.len(), 0);
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trash_restore_conflict_dialog_replaces_existing_destination() {
        let root = test_dir("trash-restore-conflict");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("conflict.txt");
        fs::write(&source, b"old").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        fs::write(&source, b"new").unwrap();

        let size = PhysicalSize::new(520, 300);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::RestoreFromTrash, size)
            .unwrap();

        assert_eq!(result.success_count, 0);
        assert_eq!(result.restore_conflicts.len(), 1);
        assert_eq!(fs::read(&source).unwrap(), b"new");
        assert!(trash_path.exists());
        assert!(scene.trash_conflict_dialog.is_some());
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 0);

        let rect = trash_conflict_dialog_rect(scene.trash_conflict_dialog.as_ref().unwrap(), size);
        assert_eq!(
            scene.trash_conflict_dialog_click_at_screen_point(
                ViewPoint {
                    x: trash_conflict_dialog_replace_button_rect(rect).x + 2.0,
                    y: trash_conflict_dialog_replace_button_rect(rect).y + 2.0,
                },
                size,
            ),
            TrashConflictDialogClick::Replace
        );
        assert_eq!(
            scene.trash_conflict_dialog_click_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            TrashConflictDialogClick::Outside
        );

        let replace = scene.replace_trash_restore_conflicts(size).unwrap();

        assert_eq!(replace.success_count, 1);
        assert_eq!(replace.failure_count, 0);
        assert_eq!(fs::read(&source).unwrap(), b"old");
        assert!(!trash_path.exists());
        assert!(scene.trash_conflict_dialog.is_none());
        assert_eq!(scene.trash_changes, 2);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_permanently_trash_view_action_deletes_test_file_and_reloads() {
        let root = test_dir("delete-trash-view");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("delete-me.txt");
        fs::write(&source, b"delete").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        let size = PhysicalSize::new(420, 260);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::DeletePermanently, size)
            .unwrap();

        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!source.exists());
        assert!(!trash_path.exists());
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn move_to_trash_moves_context_target_reloads_and_clears_selection() {
        let root = test_dir("trash-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("remove.txt"), b"remove").unwrap();
        fs::write(root.join("keep.txt"), b"keep").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        let remove_index = entry_index_by_name(&scene.primary_pane.entries, "remove.txt").unwrap();
        scene.selection.select_indexes(&[remove_index]);
        scene.context_target = Some(ShellContextTarget::Item {
            index: remove_index,
            path: root.join("remove.txt"),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene.move_context_target_to_trash(size).unwrap();
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert!(!root.join("remove.txt").exists());
        assert!(root.join("keep.txt").exists());
        assert!(entry_index_by_name(&scene.primary_pane.entries, "remove.txt").is_none());
        assert_eq!(scene.selection.len(), 0);
        assert!(scene.context_target.is_none());

        file_ops::undo_trash(&result.trash_pairs).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_directory_path_uses_focus_and_target_path() {
        let target = PathBuf::from("/run/user/1000/gvfs/sftp:host=example");
        let mut scene = test_scene(
            vec![
                test_entry_with_target("remote", true, target.clone()),
                test_entry("plain.txt", false),
            ],
            ShellViewMode::Icons,
        );

        assert_eq!(scene.selected_directory_path(), None);
        assert!(scene.selection.apply_navigation(0, false));
        assert_eq!(scene.selected_directory_path(), Some(target));
        assert!(scene.selection.apply_navigation(1, false));
        assert_eq!(scene.selected_directory_path(), None);
    }

    #[test]
    fn double_click_directory_activation_uses_retained_hit_test() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let item = scene.layout(size).item(0).expect("test item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 4.0,
            y: item.visual_rect.y + scene.content_origin_y() + 4.0,
        };
        let now = Instant::now();

        assert_eq!(
            scene.directory_activation_for_primary_press(point, size, now),
            None
        );
        assert_eq!(
            scene.directory_activation_for_primary_press(
                point,
                size,
                now + Duration::from_millis(120)
            ),
            Some(PathBuf::from("/tmp/folder"))
        );
    }

    #[test]
    fn load_path_replaces_entries_and_resets_transient_state() {
        let unique = format!(
            "fika-wgpu-load-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("nested.txt"), b"nested").unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        scene.primary_pane.scroll_x = 128.0;
        scene.primary_pane.scroll_y = 64.0;
        scene.pointer = Some(ViewPoint {
            x: 12.0,
            y: TOP_BAR_HEIGHT + 12.0,
        });
        assert!(scene.selection.apply_navigation(0, false));
        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            ShellSelection::default(),
        ));

        scene.load_path(child.clone(), size).unwrap();

        assert_eq!(scene.primary_pane.path, child);
        assert_eq!(scene.primary_pane.entries.len(), 1);
        assert_eq!(scene.primary_pane.entries[0].name.as_ref(), "nested.txt");
        assert_eq!(scene.primary_pane.scroll_x, 0.0);
        assert_eq!(scene.primary_pane.scroll_y, 0.0);
        assert_eq!(scene.selection.len(), 0);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.path_changes, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn path_history_tracks_back_forward_and_clears_forward_on_new_navigation() {
        let unique = format!(
            "fika-wgpu-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        let first = root.join("first");
        let second = first.join("second");
        let sibling = root.join("sibling");
        fs::create_dir_all(&second).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();

        assert!(scene.load_path(first.clone(), size).unwrap());
        assert_eq!(scene.primary_pane.path, first);
        assert_eq!(scene.history.back, vec![root.clone()]);
        assert!(scene.history.forward.is_empty());

        assert!(scene.load_path(second.clone(), size).unwrap());
        assert_eq!(scene.primary_pane.path, second);
        assert_eq!(scene.history.back, vec![root.clone(), first.clone()]);

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.primary_pane.path, first);
        assert_eq!(scene.history.back, vec![root.clone()]);
        assert_eq!(scene.history.forward, vec![second.clone()]);

        assert!(scene.go_history_forward(size).unwrap());
        assert_eq!(scene.primary_pane.path, second);
        assert_eq!(scene.history.back, vec![root.clone(), first.clone()]);
        assert!(scene.history.forward.is_empty());

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.primary_pane.path, first);
        assert!(scene.load_path(sibling.clone(), size).unwrap());
        assert_eq!(scene.primary_pane.path, sibling);
        assert!(scene.history.forward.is_empty());
        assert_eq!(scene.history.back, vec![root.clone(), first]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reload_current_path_preserves_history_and_selection_by_name() {
        let unique = format!(
            "fika-wgpu-reload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("keep.txt"), b"keep").unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        let keep_index = entry_index_by_name(&scene.primary_pane.entries, "keep.txt").unwrap();
        assert!(scene.selection.apply_navigation(keep_index, false));
        scene.history.push_back(PathBuf::from("/tmp/previous"));
        scene.history.push_forward(PathBuf::from("/tmp/next"));

        fs::write(root.join("aaa.txt"), b"new").unwrap();
        assert!(scene.reload_current_path(size).unwrap());

        let new_keep_index = entry_index_by_name(&scene.primary_pane.entries, "keep.txt").unwrap();
        assert!(scene.selection.contains(new_keep_index));
        assert_eq!(scene.selection.len(), 1);
        assert_eq!(scene.selection.focus, Some(new_keep_index));
        assert_eq!(scene.history.back, vec![PathBuf::from("/tmp/previous")]);
        assert_eq!(scene.history.forward, vec![PathBuf::from("/tmp/next")]);
        assert_eq!(scene.primary_pane.path, root);
        assert_eq!(scene.path_changes, 0);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(scene.primary_pane.path).unwrap();
    }

    #[test]
    fn screen_to_content_point_rejects_top_bar() {
        let offset = ViewPoint { x: 0.0, y: 40.0 };
        let content_rect = ViewRect {
            x: 180.0,
            y: TOP_BAR_HEIGHT,
            width: 320.0,
            height: 160.0,
        };
        assert_eq!(
            screen_to_content_point(ViewPoint { x: 190.0, y: 10.0 }, offset, content_rect),
            None
        );
        assert_eq!(
            screen_to_content_point(
                ViewPoint {
                    x: content_rect.x - 1.0,
                    y: TOP_BAR_HEIGHT + 5.0
                },
                offset,
                content_rect
            ),
            None
        );
        assert_eq!(
            screen_to_content_point(
                ViewPoint {
                    x: 192.0,
                    y: TOP_BAR_HEIGHT + 5.0
                },
                offset,
                content_rect
            ),
            Some(ViewPoint { x: 12.0, y: 45.0 })
        );
    }

    #[test]
    fn view_mode_shortcuts_accept_digits_and_function_key_fallbacks() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            view_mode_for_key_parts(true, &PhysicalKey::Code(KeyCode::Digit1), &no_key, &no_key,),
            Some(ShellViewMode::Icons)
        );
        assert_eq!(
            view_mode_for_key_parts(true, &PhysicalKey::Code(KeyCode::Numpad2), &no_key, &no_key,),
            Some(ShellViewMode::Compact)
        );
        assert_eq!(
            view_mode_for_key_parts(true, &unidentified, &Key::Character("3".into()), &no_key,),
            Some(ShellViewMode::Details)
        );
        assert_eq!(
            view_mode_for_key_parts(
                true,
                &unidentified,
                &Key::Character("!".into()),
                &Key::Character("1".into()),
            ),
            Some(ShellViewMode::Icons)
        );
        assert_eq!(
            view_mode_for_key_parts(false, &PhysicalKey::Code(KeyCode::F3), &no_key, &no_key,),
            Some(ShellViewMode::Details)
        );
        assert_eq!(
            view_mode_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Digit1),
                &Key::Character("1".into()),
                &Key::Character("1".into()),
            ),
            Some(ShellViewMode::Icons)
        );
    }

    #[test]
    fn path_navigation_shortcuts_use_back_forward_and_parent_keys() {
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::Backspace), false),
            Some(PathNavigationAction::Parent)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowLeft), true),
            Some(PathNavigationAction::Back)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowRight), true),
            Some(PathNavigationAction::Forward)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowUp), true),
            Some(PathNavigationAction::Parent)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowLeft), false),
            None
        );
    }

    #[test]
    fn reload_shortcuts_accept_f5_and_ctrl_r() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert!(reload_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::F5),
            &no_key,
            &no_key,
        ));
        assert!(reload_requested_for_key_parts(
            true,
            &PhysicalKey::Code(KeyCode::KeyR),
            &no_key,
            &no_key,
        ));
        assert!(reload_requested_for_key_parts(
            true,
            &unidentified,
            &Key::Character("R".into()),
            &no_key,
        ));
        assert!(!reload_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::KeyR),
            &Key::Character("r".into()),
            &Key::Character("r".into()),
        ));
    }

    #[test]
    fn hidden_shortcut_requires_ctrl_h() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert!(hidden_toggle_requested_for_key_parts(
            true,
            &PhysicalKey::Code(KeyCode::KeyH),
            &no_key,
            &no_key,
        ));
        assert!(hidden_toggle_requested_for_key_parts(
            true,
            &unidentified,
            &Key::Character("H".into()),
            &no_key,
        ));
        assert!(!hidden_toggle_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::KeyH),
            &Key::Character("h".into()),
            &Key::Character("h".into()),
        ));
    }

    #[test]
    fn location_shortcuts_activate_and_capture_text_when_active() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            location_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyL),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyD),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                false,
                &PhysicalKey::Code(KeyCode::F6),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &unidentified,
                &Key::Character("/".into()),
                &no_key,
            ),
            Some(LocationCommand::Insert("/".to_string()))
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Tab),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Complete)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Delete),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Delete)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::ArrowLeft),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveLeft)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::ArrowRight),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveRight)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Home),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveHome)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::End),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveEnd)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Cancel)
        );
        assert_eq!(
            location_command_for_key_parts(
                true,
                true,
                &PhysicalKey::Code(KeyCode::KeyR),
                &Key::Character("r".into()),
                &Key::Character("r".into()),
            ),
            Some(LocationCommand::Ignore)
        );
    }

    #[test]
    fn location_draft_replaces_completes_and_cancels() {
        let temp = test_dir("location-draft");
        std::fs::create_dir_all(temp.join("alpha")).unwrap();

        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        scene.primary_pane.path = temp.clone();
        let size = PhysicalSize::new(420, 260);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let initial_value = temp.display().to_string();
        assert_eq!(scene.location_draft_value(), Some(initial_value.as_str()));

        assert!(scene.apply_location_command(LocationCommand::Insert("a".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("a"));

        assert!(scene.apply_location_command(LocationCommand::Complete, size));
        assert_eq!(scene.location_draft_value(), Some("alpha/"));
        assert_eq!(scene.resolved_location_draft(), Some(temp.join("alpha/")));

        assert!(scene.apply_location_command(LocationCommand::Cancel, size));
        assert_eq!(scene.location_draft_value(), None);
        assert!(!scene.is_location_editing());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn location_draft_cursor_edits_at_caret() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.primary_pane.path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(420, 260);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let draft = scene.location_draft.as_ref().unwrap();
        assert_eq!(draft.cursor, draft.value.len());

        assert!(scene.apply_location_command(LocationCommand::Insert("abc".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("abc"));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 3);

        assert!(scene.apply_location_command(LocationCommand::MoveLeft, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 2);
        assert!(scene.apply_location_command(LocationCommand::Backspace, size));
        assert_eq!(scene.location_draft_value(), Some("ac"));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 1);

        assert!(scene.apply_location_command(LocationCommand::Insert("β".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("aβc"));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, "aβ".len());

        assert!(scene.apply_location_command(LocationCommand::MoveLeft, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, "a".len());
        assert!(scene.apply_location_command(LocationCommand::Delete, size));
        assert_eq!(scene.location_draft_value(), Some("ac"));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 1);

        assert!(scene.apply_location_command(LocationCommand::MoveEnd, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 2);
        assert!(scene.apply_location_command(LocationCommand::MoveHome, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().cursor, 0);
    }

    #[test]
    fn location_draft_blurs_outside_path_bar_without_committing() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.primary_pane.path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(600, 320);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        assert!(
            scene.apply_location_command(
                LocationCommand::Insert("/does-not-exist".to_string()),
                size
            )
        );
        assert_eq!(scene.location_draft_value(), Some("/does-not-exist"));

        let path_bar = scene.path_bar_rect(size).unwrap();
        assert!(!scene.close_location_draft_if_outside(
            ViewPoint {
                x: path_bar.x + 4.0,
                y: path_bar.y + 4.0,
            },
            size
        ));
        assert!(scene.is_location_editing());

        let blank = ViewPoint {
            x: scene.content_origin_x(size) + scene.content_width(size) - 4.0,
            y: scene.content_origin_y() + 4.0,
        };
        assert!(scene.close_location_draft_if_outside(blank, size));
        assert_eq!(scene.location_draft_value(), None);
        assert_eq!(scene.primary_pane.path, PathBuf::from("/tmp"));
        assert_eq!(scene.location_changes, 3);
    }

    #[test]
    fn shaped_label_cursor_measurement_tracks_glyph_layout() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut text = TextFrameBuilder::new(
            &mut font_system,
            &mut swash_cache,
            &mut text_buffer,
            &mut label_cache,
            PhysicalSize::new(320, 120),
            1.0,
        );
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: TEXT_LINE_HEIGHT,
        };

        let one =
            text.measure_label_cursor_x("abcdef", rect, 1, LabelAlignment::Start, LabelWrap::None);
        let end = text.measure_label_cursor_x(
            "abcdef",
            rect,
            "abcdef".len(),
            LabelAlignment::Start,
            LabelWrap::None,
        );
        let wide = text.measure_label_cursor_x(
            "mmmmmm",
            rect,
            "mmmmmm".len(),
            LabelAlignment::Start,
            LabelWrap::None,
        );

        assert!(one > 0.0);
        assert!(end > one);
        assert!(wide > end);
    }

    #[test]
    fn location_bar_keeps_full_width_hit_target_when_editing() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.primary_pane.path = PathBuf::from("/x");
        let size = PhysicalSize::new(900, 360);

        let inactive = scene
            .path_bar_rect(size)
            .expect("inactive path bar should be visible");
        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let active = scene
            .path_bar_rect(size)
            .expect("active path bar should be visible");

        assert_eq!(active.width, inactive.width);
        assert_eq!(active.height, 28.0);
        assert!(scene.path_bar_contains_screen_point(
            ViewPoint {
                x: active.right() - 2.0,
                y: active.y + 2.0,
            },
            size
        ));
    }

    #[test]
    fn zoom_shortcuts_accept_common_characters() {
        assert_eq!(
            zoom_action_for_key(&Key::Character("+".into())),
            Some(ZoomAction::In)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("=".into())),
            Some(ZoomAction::In)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("-".into())),
            Some(ZoomAction::Out)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("0".into())),
            Some(ZoomAction::Reset)
        );
        assert_eq!(zoom_action_for_key(&Key::Character("x".into())), None);
        assert_eq!(zoom_action_for_scroll_delta(-1.0), Some(ZoomAction::In));
        assert_eq!(zoom_action_for_scroll_delta(1.0), Some(ZoomAction::Out));
        assert_eq!(zoom_action_for_scroll_delta(0.0), None);
    }

    #[test]
    fn selection_shortcuts_accept_ctrl_a_and_escape() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            selection_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &no_key,
                &no_key,
            ),
            Some(SelectionCommand::SelectAll)
        );
        assert_eq!(
            selection_command_for_key_parts(
                true,
                &unidentified,
                &Key::Character("A".into()),
                &no_key,
            ),
            Some(SelectionCommand::SelectAll)
        );
        assert_eq!(
            selection_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            None
        );
        assert_eq!(
            selection_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(SelectionCommand::Clear)
        );
    }

    #[test]
    fn filter_shortcuts_activate_and_capture_text_when_active() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            filter_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyF),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::Activate)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &unidentified,
                &Key::Character("1".into()),
                &no_key,
            ),
            Some(FilterCommand::Insert("1".to_string()))
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::Backspace)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::ClearAndDeactivate)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                false,
                &unidentified,
                &Key::Character("a".into()),
                &no_key,
            ),
            None
        );
    }

    #[test]
    fn create_dialog_key_input_captures_text_and_commit_controls() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            create_command_for_key_parts(
                false,
                &unidentified,
                &Key::Character("x".into()),
                &no_key,
            ),
            CreateCommand::Insert("x".to_string())
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            CreateCommand::Backspace
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &unidentified,
                &Key::Named(NamedKey::Enter),
                &no_key,
            ),
            CreateCommand::Commit
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            CreateCommand::Cancel
        );
        assert_eq!(
            create_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            CreateCommand::Ignore
        );
    }

    #[test]
    fn rename_dialog_key_input_captures_text_and_commit_controls() {
        let unidentified = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);

        assert_eq!(
            rename_command_for_key_parts(
                false,
                &unidentified,
                &Key::Character("x".into()),
                &no_key,
            ),
            RenameCommand::Insert("x".to_string())
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            RenameCommand::Backspace
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &unidentified,
                &Key::Named(NamedKey::Enter),
                &no_key,
            ),
            RenameCommand::Commit
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            RenameCommand::Cancel
        );
        assert_eq!(
            rename_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            RenameCommand::Ignore
        );
    }

    #[test]
    fn filter_updates_layout_hit_testing_and_select_all() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("beta.txt", false),
                test_entry("alphabet.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 340);

        assert!(scene.apply_filter_command(FilterCommand::Activate, size));
        assert!(scene.apply_filter_command(FilterCommand::Insert("alp".to_string()), size));
        assert_eq!(scene.primary_pane.filtered_indexes, vec![0, 2]);
        assert_eq!(scene.filtered_entry_count(), 2);
        assert_eq!(scene.filter_changes, 2);

        let layout = scene.layout(size);
        assert!(layout.item(2).is_none());
        let second = layout.item(1).expect("second filtered item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + second.visual_rect.x + 2.0,
            y: second.visual_rect.y + scene.content_origin_y() + 2.0,
        };
        assert_eq!(scene.hit_test_screen_point(point, size), Some(2));

        assert!(scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(
            scene.selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![0, 2]
        );

        assert!(scene.apply_filter_command(FilterCommand::Backspace, size));
        assert_eq!(scene.filter_pattern, "al");
        assert!(scene.apply_filter_command(FilterCommand::Deactivate, size));
        assert!(!scene.filter_active);
        assert_eq!(scene.filter_pattern, "al");
        assert_eq!(scene.filtered_entry_count(), 2);
        assert!(scene.apply_filter_command(FilterCommand::ClearAndDeactivate, size));
        assert!(!scene.filter_active);
        assert!(scene.filter_pattern.is_empty());
        assert_eq!(scene.filtered_entry_count(), 3);
    }

    #[test]
    fn hidden_toggle_updates_filtered_projection_and_prunes_selection() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry(".secret", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);

        assert_eq!(scene.primary_pane.filtered_indexes, vec![0, 2]);
        assert_eq!(scene.filtered_entry_count(), 2);
        assert!(scene.selection.apply_navigation(1, false));

        assert!(scene.toggle_hidden_visibility(size));
        assert!(scene.show_hidden);
        assert_eq!(scene.primary_pane.filtered_indexes, vec![0, 1, 2]);
        assert_eq!(scene.hidden_changes, 1);
        assert!(scene.selection.contains(1));

        assert!(scene.toggle_hidden_visibility(size));
        assert!(!scene.show_hidden);
        assert_eq!(scene.primary_pane.filtered_indexes, vec![0, 2]);
        assert_eq!(scene.selection.len(), 0);
        assert_eq!(scene.selection_changes, 1);
    }

    #[test]
    fn app_toolbar_does_not_expose_temporary_mouse_buttons() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 240);
        let toolbar_point = ViewPoint {
            x: 18.0,
            y: scene.app_toolbar_y() + 18.0,
        };

        assert_eq!(scene.view_mode_at_screen_point(toolbar_point, size), None);
        assert_eq!(
            scene.path_navigation_action_at_screen_point(toolbar_point, size),
            None
        );
    }

    #[test]
    fn switching_view_modes_clamps_scroll_and_refreshes_layout_axis() {
        let mut scene = test_scene(
            (0..30)
                .map(|index| test_entry(&format!("entry-{index}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(260, 180);
        scene.primary_pane.scroll_x = 10_000.0;
        scene.primary_pane.scroll_y = 500.0;
        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            ShellSelection::default(),
        ));

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(scene.primary_pane.view_mode, ShellViewMode::Details);
        assert_eq!(scene.primary_pane.scroll_x, 0.0);
        assert!(scene.primary_pane.scroll_y >= 0.0);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.view_switches, 1);

        assert!(!scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(scene.view_switches, 1);
    }

    #[test]
    fn zoom_updates_layout_metrics_for_all_view_modes() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index}.txt"), index % 5 == 0))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let icons_before = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };

        assert!(scene.zoom(ZoomAction::In, size));
        assert_eq!(scene.zoom_changes, 1);
        assert!(scene.zoom_percent() > 100);
        let icons_after = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(icons_after.icon_rect.width > icons_before.icon_rect.width);
        assert!(icons_after.item_rect.height > icons_before.item_rect.height);

        assert!(scene.set_view_mode(ShellViewMode::Compact, size));
        let compact_zoomed = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(scene.zoom(ZoomAction::Reset, size));
        let compact_reset = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(compact_zoomed.icon_rect.width > compact_reset.icon_rect.width);
        assert!(compact_zoomed.item_rect.height > compact_reset.item_rect.height);

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        let details_before = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(scene.zoom(ZoomAction::Out, size));
        let details_after = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(details_after.item_rect.height < details_before.item_rect.height);
        assert!(details_after.icon_rect.width <= details_before.icon_rect.width);
    }

    #[test]
    fn window_scale_factor_scales_default_shell_metrics() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(900, 600);

        assert!(scene.set_scale_factor(1.5, size));
        assert_eq!(scene.top_bar_height(), 54.0);
        assert_eq!(scene.text_line_height(), 27.0);

        let icons_item = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(scene.icons_options(size).text_height, 27.0);
        assert_eq!(icons_item.icon_rect.width, 72.0);
        assert!(icons_item.text_rect.height >= 27.0);

        assert!(scene.set_view_mode(ShellViewMode::Compact, size));
        let compact_item = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(scene.compact_options(size).text_height, 27.0);
        assert_eq!(compact_item.icon_rect.width, 42.0);
        assert!(compact_item.text_rect.height >= 27.0);

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        let details_item = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(details_item.icon_rect.width, 27.0);
        assert_eq!(details_item.text_rect.height, 27.0);
    }

    #[test]
    fn shell_hit_test_uses_content_coordinates_below_top_bar() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let layout = scene.layout(size);
        let item = layout.item(0).expect("test item should layout");

        let visual_point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 1.0,
            y: item.visual_rect.y + scene.content_origin_y() + 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(visual_point, size), Some(0));

        let top_bar_point = ViewPoint {
            x: scene.content_origin_x(size) + item.item_rect.x + 1.0,
            y: scene.content_origin_y() - 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(top_bar_point, size), None);
    }

    #[test]
    fn status_bar_reserves_viewport_and_blocks_selection_hits() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(360, 240);
        let status_bar = scene.status_bar_rect(size);

        assert_eq!(scene.viewport_height(size), 124.0);
        assert_eq!(status_bar.y, 204.0);
        assert_eq!(
            scene.hit_test_screen_point(
                ViewPoint {
                    x: status_bar.x + 16.0,
                    y: status_bar.y + 4.0,
                },
                size,
            ),
            None
        );

        assert!(scene.selection.apply_navigation(0, false));
        assert!(!scene.begin_primary_pointer(
            SelectionClick {
                point: ViewPoint {
                    x: status_bar.x + 16.0,
                    y: status_bar.y + 4.0,
                },
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert_eq!(scene.selection.len(), 1);
        assert!(scene.selection.contains(0));
    }

    #[test]
    fn keyboard_navigation_uses_icons_columns_and_page_stride() {
        let size = PhysicalSize::new(360, 240);
        let layout = IconsLayout::new(
            20,
            IconsLayoutOptions {
                viewport_width: size.width as f32,
                viewport_height: content_height(size),
                reserved_bottom: 0.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                padding: 8.0,
                gap: 12.0,
                item_width: ICONS_ITEM_WIDTH,
                item_height: ICONS_ITEM_HEIGHT,
                icon_size: ICONS_ICON_SIZE,
                text_height: 18.0,
            },
        );
        let layout = ShellLayout::Icons(layout);

        assert_eq!(
            navigation_target(NavigationAction::Right, 0, 20, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 20, &layout),
            Some(match &layout {
                ShellLayout::Icons(layout) => layout.columns_per_row(),
                _ => unreachable!(),
            })
        );
        assert_eq!(
            navigation_target(NavigationAction::Up, 1, 20, &layout),
            Some(0)
        );
        assert_eq!(
            navigation_target(NavigationAction::End, 0, 20, &layout),
            Some(19)
        );
        assert_eq!(
            navigation_target(NavigationAction::PageDown, 0, 20, &layout),
            Some(layout.visible_items().len())
        );
    }

    #[test]
    fn compact_navigation_uses_column_major_rows() {
        let size = PhysicalSize::new(320, 180);
        let compact = CompactLayout::new(
            20,
            CompactLayoutOptions {
                viewport_width: size.width as f32,
                viewport_height: content_height(size),
                reserved_bottom: 0.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                padding: 6.0,
                side_padding: 8.0,
                gap: 8.0,
                text_gap: 8.0,
                item_width: 236.0,
                item_height: COMPACT_ITEM_HEIGHT,
                icon_size: COMPACT_ICON_SIZE,
                text_height: 18.0,
            },
        );
        let layout = ShellLayout::Compact(ShellCompactLayout::new(compact, vec![0.0; 20]));
        let rows = match &layout {
            ShellLayout::Compact(layout) => layout.rows_per_column(),
            _ => unreachable!(),
        };

        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 20, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::Right, 0, 20, &layout),
            Some(rows)
        );
        assert_eq!(
            navigation_target(NavigationAction::Left, rows, 20, &layout),
            Some(0)
        );
    }

    #[test]
    fn compact_layout_uses_longest_name_per_column_and_per_item_visual_width() {
        let scene = test_scene(
            vec![
                test_entry("a", false),
                test_entry("very-wide-filename.txt", false),
                test_entry("b", false),
                test_entry("c", false),
            ],
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(700, 250);
        let layout = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout,
            _ => unreachable!(),
        };

        assert_eq!(layout.rows_per_column(), 2);
        let short_first_column = layout.item(0).unwrap();
        let long_first_column = layout.item(1).unwrap();
        let short_second_column = layout.item(2).unwrap();

        assert_eq!(
            short_first_column.item_rect.width,
            long_first_column.item_rect.width
        );
        assert!(long_first_column.item_rect.width > short_second_column.item_rect.width);
        assert!(short_first_column.visual_rect.width < long_first_column.visual_rect.width);
        assert!(short_first_column.visual_rect.width < short_first_column.item_rect.width);
    }

    #[test]
    fn details_navigation_and_header_hit_test_are_row_based() {
        let scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(420, 220);
        let header_point = ViewPoint {
            x: scene.content_origin_x(size) + 12.0,
            y: scene.details_header_y() + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(header_point, size), None);

        let row_point = ViewPoint {
            x: scene.content_origin_x(size) + 12.0,
            y: scene.content_origin_y() + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(row_point, size), Some(0));

        let layout = scene.layout(size);
        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 3, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::PageDown, 0, 3, &layout),
            Some(2)
        );
    }

    #[test]
    fn keyboard_navigation_updates_focus_and_shift_range() {
        let mut selection = ShellSelection::default();

        assert!(selection.apply_navigation(3, false));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(3));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3]
        );

        assert!(selection.apply_navigation(7, true));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(7));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3, 4, 5, 6, 7]
        );

        assert!(selection.apply_navigation(5, true));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(5));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
    }

    #[test]
    fn rubber_band_selection_supports_replace_extend_and_toggle() {
        let mut base = ShellSelection::default();
        assert!(base.apply_navigation(2, false));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[4, 5], RubberBandMode::Replace));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(selection.anchor, Some(4));
        assert_eq!(selection.focus, Some(5));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[4, 5], RubberBandMode::Extend));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![2, 4, 5]
        );
        assert_eq!(selection.anchor, Some(2));
        assert_eq!(selection.focus, Some(5));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[2, 3], RubberBandMode::Toggle));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(selection.focus, Some(3));
    }

    #[test]
    fn rubber_band_drag_from_blank_space_selects_intersecting_visual_rects() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let layout = scene.layout(size);
        let item = layout.item(0).expect("test item should layout");
        let start = ViewPoint {
            x: scene.content_origin_x(size) + scene.content_width(size) - 2.0,
            y: scene.content_origin_y() + 1.0,
        };
        let current = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.right() - 1.0,
            y: item.visual_rect.bottom() - scene.primary_pane.scroll_y + scene.content_origin_y()
                - 1.0,
        };

        assert!(!scene.begin_primary_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(current, size));
        assert!(scene.selection.contains(0));
        assert!(scene.rubber_band.as_ref().is_some_and(|band| band.active));
        assert!(scene.end_primary_pointer(current, size));
        assert!(scene.rubber_band.is_none());
    }

    #[test]
    fn clamped_screen_to_content_point_stays_inside_content_viewport() {
        let content_rect = ViewRect {
            x: 0.0,
            y: TOP_BAR_HEIGHT,
            width: 320.0,
            height: 160.0,
        };
        assert_eq!(
            clamped_screen_to_content_point(
                ViewPoint {
                    x: -10.0,
                    y: TOP_BAR_HEIGHT - 20.0,
                },
                ViewPoint { x: 0.0, y: 40.0 },
                content_rect,
            ),
            ViewPoint { x: 0.0, y: 40.0 }
        );
        assert_eq!(
            clamped_screen_to_content_point(
                ViewPoint { x: 500.0, y: 500.0 },
                ViewPoint { x: 0.0, y: 40.0 },
                content_rect,
            ),
            ViewPoint { x: 320.0, y: 200.0 }
        );
    }
}

const QUAD_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const TEXT_SHADER: &str = r#"
@group(0) @binding(0)
var text_atlas: texture_2d<f32>;

@group(0) @binding(1)
var text_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(text_atlas, text_sampler, input.uv);
}
"#;
