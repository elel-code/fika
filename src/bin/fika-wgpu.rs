use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use cosmic_text::{
    Align, Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Wrap,
};
use fika_core::{
    CompactLayout, CompactLayoutOptions, Entry, FileClipboardRole, FileTransferMode, IconsLayout,
    IconsLayoutOptions, NETWORK_ROOT_LABEL, NameFilter, PaneId, TransferTaskResult,
    TrashViewOperation, TrashViewOperationResult, UserPlace, ViewPoint, ViewRect, ViewSize,
    complete_location_input, decode_file_clipboard_text, default_user_places_path,
    encode_file_clipboard_text, file_ops, format_modified_secs, format_size, home_dir,
    is_network_path, load_place_order, load_user_places, network_root_path, network_uri_from_path,
    paste_text_result, place_order_path_for_user_places_path, read_entries_sync,
    resolve_location_input, save_place_order, save_user_places, transfer_paths_result,
    trash_view_operation_result,
};
use gio::prelude::FileExt;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const TOP_BAR_HEIGHT: f32 = 52.0;
const FILTER_BAR_HEIGHT: f32 = 30.0;
const STATUS_BAR_HEIGHT: f32 = 28.0;
const ICONS_ITEM_WIDTH: f32 = 116.0;
const ICONS_ITEM_HEIGHT: f32 = 106.0;
const ICONS_ICON_SIZE: f32 = 48.0;
const COMPACT_ITEM_WIDTH: f32 = 236.0;
const COMPACT_ITEM_HEIGHT: f32 = 44.0;
const COMPACT_ICON_SIZE: f32 = 28.0;
const DETAILS_HEADER_HEIGHT: f32 = 28.0;
const DETAILS_ROW_HEIGHT: f32 = 26.0;
const DETAILS_ICON_SIZE: f32 = 18.0;
const DETAILS_NAME_WIDTH: f32 = 360.0;
const DETAILS_SIZE_WIDTH: f32 = 104.0;
const DETAILS_MODIFIED_WIDTH: f32 = 164.0;
const SCROLL_LINE_PX: f32 = 56.0;
const TEXT_ATLAS_WIDTH: u32 = 2048;
const TEXT_FONT_SIZE: f32 = 13.0;
const TEXT_LINE_HEIGHT: f32 = 16.0;
const TEXT_PADDING: u32 = 2;
const TEXT_LABEL_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
const ICON_ATLAS_WIDTH: u32 = 1024;
const ICON_PADDING: u32 = 2;
const ICON_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const RUBBER_BAND_START_THRESHOLD: f32 = 4.0;
const VIEW_SWITCH_REDRAW_FRAMES: u8 = 6;
const VIEW_MODE_BUTTON_WIDTH: f32 = 86.0;
const VIEW_MODE_BUTTON_HEIGHT: f32 = 24.0;
const VIEW_MODE_BUTTON_GAP: f32 = 6.0;
const VIEW_MODE_STRIPE_HEIGHT: f32 = 8.0;
const VIEW_MODE_RAIL_WIDTH: f32 = 6.0;
const NAV_BUTTON_WIDTH: f32 = 28.0;
const NAV_UP_BUTTON_WIDTH: f32 = 36.0;
const NAV_RELOAD_BUTTON_WIDTH: f32 = 58.0;
const NAV_HIDDEN_BUTTON_WIDTH: f32 = 58.0;
const NAV_BUTTON_HEIGHT: f32 = 24.0;
const NAV_BUTTON_GAP: f32 = 6.0;
const PLACES_SIDEBAR_WIDTH: f32 = 216.0;
const PLACES_SIDEBAR_SPLITTER_WIDTH: f32 = 1.0;
const PLACES_SIDEBAR_PADDING_X: f32 = 8.0;
const PLACES_SIDEBAR_TOP_PADDING: f32 = 8.0;
const PLACES_SECTION_HEIGHT: f32 = 24.0;
const PLACES_ROW_HEIGHT: f32 = 30.0;
const PLACES_ROW_GAP: f32 = 2.0;
const PLACES_ICON_SIZE: f32 = 18.0;
const PLACES_SCROLLBAR_WIDTH: f32 = 3.0;
const PLACES_SCROLLBAR_MARGIN: f32 = 4.0;
const PLACES_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 28.0;
const CONTEXT_MENU_WIDTH: f32 = 184.0;
const CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
const CONTEXT_MENU_MARGIN: f32 = 6.0;
const PROPERTIES_OVERLAY_WIDTH: f32 = 440.0;
const PROPERTIES_OVERLAY_MARGIN: f32 = 18.0;
const PROPERTIES_TITLE_HEIGHT: f32 = 42.0;
const PROPERTIES_ROW_HEIGHT: f32 = 24.0;
const CREATE_DIALOG_WIDTH: f32 = 420.0;
const CREATE_DIALOG_HEIGHT: f32 = 196.0;
const CREATE_DIALOG_MARGIN: f32 = 18.0;
const CREATE_DIALOG_TITLE_HEIGHT: f32 = 42.0;
const CREATE_DIALOG_BUTTON_WIDTH: f32 = 84.0;
const CREATE_DIALOG_BUTTON_HEIGHT: f32 = 24.0;
const CREATE_DIALOG_BUTTON_GAP: f32 = 8.0;
const RENAME_DIALOG_WIDTH: f32 = 420.0;
const RENAME_DIALOG_HEIGHT: f32 = 168.0;
const RENAME_DIALOG_MARGIN: f32 = 18.0;
const RENAME_DIALOG_TITLE_HEIGHT: f32 = 42.0;
const PATH_HISTORY_LIMIT: usize = 128;
const ZOOM_STEP_MIN: i32 = -3;
const ZOOM_STEP_MAX: i32 = 4;
const ZOOM_STEP_SCALE: f32 = 0.12;
const AUTO_CYCLE_INTERVAL: Duration = Duration::from_secs(1);
const DOUBLE_CLICK_MAX_INTERVAL: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_MAX_DISTANCE: f32 = 6.0;
const WGPU_SHELL_PANE_ID: PaneId = PaneId(1);

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

struct StartupOptions {
    path: PathBuf,
    view_mode: ShellViewMode,
    auto_cycle_views: bool,
}

fn parse_start_options() -> Result<Option<StartupOptions>, String> {
    let mut args = env::args_os();
    let program = args
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "fika-wgpu".to_string());

    let mut view_mode = ShellViewMode::Icons;
    let mut auto_cycle_views = false;
    let mut path = None;
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!("Usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]");
            return Ok(None);
        }
        if arg == "--auto-cycle-views" {
            auto_cycle_views = true;
            continue;
        }
        if arg == "--view" {
            let Some(value) = args.next() else {
                return Err(format!(
                    "usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]"
                ));
            };
            let value = value
                .to_str()
                .ok_or_else(|| "--view value must be valid UTF-8".to_string())?;
            view_mode = ShellViewMode::parse(value)?;
            continue;
        }
        if let Some(value) = arg.to_str().and_then(|arg| arg.strip_prefix("--view=")) {
            view_mode = ShellViewMode::parse(value)?;
            continue;
        }
        if arg.to_str().is_some_and(|arg| arg.starts_with("--")) {
            return Err(format!("unknown option: {}", arg.to_string_lossy()));
        }
        if path.replace(PathBuf::from(arg)).is_some() {
            return Err(format!(
                "usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]"
            ));
        }
    }

    let path = match path {
        Some(path) => path,
        None => env::current_dir().map_err(|error| format!("current directory: {error}"))?,
    };
    Ok(Some(StartupOptions {
        path,
        view_mode,
        auto_cycle_views,
    }))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ShellViewMode {
    #[default]
    Icons,
    Compact,
    Details,
}

impl ShellViewMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "icons" => Ok(Self::Icons),
            "compact" => Ok(Self::Compact),
            "details" => Ok(Self::Details),
            _ => Err(format!("unknown view mode: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Icons => "icons",
            Self::Compact => "compact",
            Self::Details => "details",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Icons => "Icons",
            Self::Compact => "Compact",
            Self::Details => "Details",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Icons => Self::Compact,
            Self::Compact => Self::Details,
            Self::Details => Self::Icons,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathNavigationAction {
    Back,
    Forward,
    Parent,
    Reload,
    ToggleHidden,
}

impl PathNavigationAction {
    fn label(self) -> &'static str {
        match self {
            Self::Back => "<",
            Self::Forward => ">",
            Self::Parent => "Up",
            Self::Reload => "Reload",
            Self::ToggleHidden => "Hidden",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Back => "history-back",
            Self::Forward => "history-forward",
            Self::Parent => "parent-directory",
            Self::Reload => "reload-directory",
            Self::ToggleHidden => "toggle-hidden",
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

fn window_title(scene: &ShellScene) -> String {
    format!(
        "Fika wgpu shell [{}] - {}",
        scene.view_mode.as_str(),
        scene.path.display()
    )
}

enum ShellClipboard {
    Wayland(smithay_clipboard::Clipboard),
}

impl ShellClipboard {
    fn from_window(window: &dyn Window) -> Result<Option<Self>, String> {
        let display = window
            .display_handle()
            .map_err(|error| format!("display handle: {error}"))?;
        match display.as_raw() {
            RawDisplayHandle::Wayland(handle) => {
                let clipboard = unsafe {
                    // The pointer comes from winit's live Wayland display handle.
                    // Fika drops ShellClipboard before dropping the window.
                    smithay_clipboard::Clipboard::new(handle.display.as_ptr())
                };
                Ok(Some(Self::Wayland(clipboard)))
            }
            _ => Ok(None),
        }
    }

    fn backend(&self) -> &'static str {
        match self {
            Self::Wayland(_) => "wayland",
        }
    }

    fn store_text(&self, text: &str) {
        match self {
            Self::Wayland(clipboard) => clipboard.store(text.to_string()),
        }
    }

    fn load_text(&self) -> Result<String, String> {
        match self {
            Self::Wayland(clipboard) => clipboard.load().map_err(|error| error.to_string()),
        }
    }
}

struct FikaWgpuApp {
    scene: ShellScene,
    modifiers: Modifiers,
    // Drop order matters: renderer and clipboard borrow display/window handles,
    // so they must be dropped before the window.
    renderer: Option<WgpuState>,
    clipboard: Option<ShellClipboard>,
    window: Option<Box<dyn Window>>,
    pending_redraw_frames: u8,
    auto_cycle_views: bool,
    next_auto_cycle: Instant,
}

impl FikaWgpuApp {
    fn new(scene: ShellScene, auto_cycle_views: bool) -> Self {
        Self {
            scene,
            modifiers: Modifiers::default(),
            renderer: None,
            clipboard: None,
            window: None,
            pending_redraw_frames: 0,
            auto_cycle_views,
            next_auto_cycle: Instant::now() + AUTO_CYCLE_INTERVAL,
        }
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
                let next = self.scene.view_mode.next();
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
                    self.scene.clamp_scroll(renderer.size);
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
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                if self.scene.is_rename_dialog_open() {
                    return;
                }
                if self.scene.is_create_dialog_open() {
                    return;
                }
                if self.scene.set_pointer(point, renderer.size)
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerLeft { .. } => {
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
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                let Some(mouse_button) = button.mouse_button() else {
                    return;
                };
                if self.scene.is_rename_dialog_open() {
                    if state == ElementState::Pressed && mouse_button == MouseButton::Left {
                        match self
                            .scene
                            .rename_dialog_click_at_screen_point(point, renderer.size)
                        {
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
                        match self
                            .scene
                            .create_dialog_click_at_screen_point(point, renderer.size)
                        {
                            CreateDialogClick::Outside | CreateDialogClick::Cancel => {
                                if self.scene.close_create_dialog()
                                    && let Some(window) = self.window.as_ref()
                                {
                                    window.request_redraw();
                                }
                            }
                            CreateDialogClick::Commit => self.commit_create_dialog(event_loop),
                            CreateDialogClick::Kind(kind) => {
                                if self.scene.apply_create_command(
                                    CreateCommand::SetKind(kind),
                                    renderer.size,
                                ) && let Some(window) = self.window.as_ref()
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
                        && self
                            .scene
                            .close_properties_overlay_if_outside(point, renderer.size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if mouse_button == MouseButton::Right {
                    if state == ElementState::Pressed
                        && self.scene.open_context_menu(point, renderer.size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if mouse_button != MouseButton::Left {
                    return;
                }
                if state == ElementState::Pressed && self.scene.is_context_menu_open() {
                    let action = self
                        .scene
                        .activate_or_close_context_menu(point, renderer.size);
                    if let Some(action) = action {
                        self.perform_context_menu_action(event_loop, action);
                    } else if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && self
                        .scene
                        .path_bar_contains_screen_point(point, renderer.size)
                {
                    if self
                        .scene
                        .apply_location_command(LocationCommand::Activate, renderer.size)
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(action) = self
                        .scene
                        .path_navigation_action_at_screen_point(point, renderer.size)
                {
                    self.perform_path_navigation(event_loop, action);
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(view_mode) =
                        self.scene.view_mode_at_screen_point(point, renderer.size)
                {
                    if self.scene.set_view_mode(view_mode, renderer.size) {
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
                    && let Some(path) = self
                        .scene
                        .place_activation_for_primary_press(point, renderer.size)
                {
                    self.load_scene_path(event_loop, path, "place-open");
                    return;
                }
                if state == ElementState::Pressed
                    && let Some(path) = self.scene.directory_activation_for_primary_press(
                        point,
                        renderer.size,
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
                        self.scene.begin_primary_pointer(selection, renderer.size)
                    }
                    ElementState::Released => self.scene.end_primary_pointer(point, renderer.size),
                };
                if changed && let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                if self.scene.scroll_by(scroll_delta_y(delta), renderer.size) {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
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
        action: ShellContextMenuAction,
    ) {
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
            ShellContextMenuAction::Refresh => self.reload_scene_path(event_loop),
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
            ShellContextMenuAction::Paste => self.paste_from_clipboard(event_loop),
            ShellContextMenuAction::OpenInNewPane => {
                eprintln!(
                    "[fika-wgpu] context-action-pending action={} target={}",
                    action.as_str(),
                    self.scene
                        .context_target
                        .as_ref()
                        .map(ShellContextTarget::kind)
                        .unwrap_or("none")
                );
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
            PathNavigationAction::Reload => self.scene.reload_current_path(size),
            PathNavigationAction::ToggleHidden => Ok(self.scene.toggle_hidden_visibility(size)),
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
        if renderer.render(window.as_ref(), event_loop, &self.scene, reason, force_log)
            && self.pending_redraw_frames > 0
        {
            self.pending_redraw_frames -= 1;
        }
    }
}

fn scroll_delta_y(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => -y * SCROLL_LINE_PX,
        MouseScrollDelta::PixelDelta(position) => -position.y as f32,
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

fn navigation_target(
    action: NavigationAction,
    current: usize,
    item_count: usize,
    layout: &ShellLayout,
) -> Option<usize> {
    if item_count == 0 {
        return None;
    }
    let last = item_count - 1;
    let current = current.min(last);
    match layout {
        ShellLayout::Icons(layout) => {
            let columns = layout.columns_per_row().max(1);
            let page_stride = layout.visible_items().count().max(columns).max(1);
            Some(match action {
                NavigationAction::Left => current.saturating_sub(1),
                NavigationAction::Right => (current + 1).min(last),
                NavigationAction::Up => current.saturating_sub(columns),
                NavigationAction::Down => (current + columns).min(last),
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
        ShellLayout::Compact(layout) => {
            let rows = layout.rows_per_column().max(1);
            let row = current % rows;
            let page_stride = layout.visible_items().count().max(rows).max(1);
            Some(match action {
                NavigationAction::Left => current.saturating_sub(rows),
                NavigationAction::Right => (current + rows).min(last),
                NavigationAction::Up => {
                    if row == 0 {
                        current
                    } else {
                        current - 1
                    }
                }
                NavigationAction::Down => {
                    if row + 1 >= rows {
                        current
                    } else {
                        (current + 1).min(last)
                    }
                }
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
        ShellLayout::Details(layout) => {
            let page_stride = layout.visible_items().len().max(1);
            Some(match action {
                NavigationAction::Left | NavigationAction::Up => current.saturating_sub(1),
                NavigationAction::Right | NavigationAction::Down => (current + 1).min(last),
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
    }
}

#[derive(Clone, Debug)]
enum ShellLayout {
    Icons(IconsLayout),
    Compact(CompactLayout),
    Details(DetailsLayout),
}

impl ShellLayout {
    fn content_size(&self) -> ViewSize {
        match self {
            Self::Icons(layout) => layout.content_size(),
            Self::Compact(layout) => layout.content_size(),
            Self::Details(layout) => layout.content_size(),
        }
    }

    fn item(&self, index: usize) -> Option<fika_core::ItemLayout> {
        match self {
            Self::Icons(layout) => layout.item(index),
            Self::Compact(layout) => layout.item(index),
            Self::Details(layout) => layout.item(index),
        }
    }

    fn visible_items(&self) -> Vec<fika_core::ItemLayout> {
        match self {
            Self::Icons(layout) => layout.visible_items().collect(),
            Self::Compact(layout) => layout.visible_items().collect(),
            Self::Details(layout) => layout.visible_items(),
        }
    }

    fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        match self {
            Self::Icons(layout) => layout.hit_test_content_point(point),
            Self::Compact(layout) => layout.hit_test_content_point(point),
            Self::Details(layout) => layout.hit_test_content_point(point),
        }
    }

    fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        match self {
            Self::Icons(layout) => layout.indexes_intersecting(rect).indexes().to_vec(),
            Self::Compact(layout) => layout.indexes_intersecting(rect).indexes().to_vec(),
            Self::Details(layout) => layout.indexes_intersecting(rect),
        }
    }
}

#[derive(Clone, Debug)]
struct DetailsLayout {
    item_count: usize,
    viewport_height: f32,
    scroll_y: f32,
    content_width: f32,
    row_height: f32,
    icon_size: f32,
}

impl DetailsLayout {
    fn new(
        item_count: usize,
        viewport_width: f32,
        viewport_height: f32,
        scroll_y: f32,
        row_height: f32,
        icon_size: f32,
    ) -> Self {
        Self {
            item_count,
            viewport_height,
            scroll_y,
            content_width: (DETAILS_NAME_WIDTH + DETAILS_SIZE_WIDTH + DETAILS_MODIFIED_WIDTH)
                .max(viewport_width),
            row_height,
            icon_size,
        }
    }

    fn content_size(&self) -> ViewSize {
        ViewSize {
            width: self.content_width,
            height: (self.item_count as f32 * self.row_height).max(1.0),
        }
    }

    fn item(&self, index: usize) -> Option<fika_core::ItemLayout> {
        if index >= self.item_count {
            return None;
        }
        let y = index as f32 * self.row_height;
        let item_rect = ViewRect {
            x: 0.0,
            y,
            width: self.content_width,
            height: self.row_height,
        };
        let icon_rect = ViewRect {
            x: 8.0,
            y: y + (self.row_height - self.icon_size) / 2.0,
            width: self.icon_size,
            height: self.icon_size,
        };
        Some(fika_core::ItemLayout {
            model_index: index,
            column: 0,
            row: index,
            item_rect,
            visual_rect: item_rect,
            icon_rect,
            text_rect: ViewRect {
                x: 14.0 + self.icon_size,
                y: y + (self.row_height - 18.0).max(0.0) / 2.0,
                width: (DETAILS_NAME_WIDTH - 42.0).max(1.0),
                height: 18.0,
            },
        })
    }

    fn visible_items(&self) -> Vec<fika_core::ItemLayout> {
        self.visible_row_range()
            .filter_map(|index| self.item(index))
            .collect()
    }

    fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        if point.x < 0.0 || point.x >= self.content_width || point.y < 0.0 {
            return None;
        }
        let index = (point.y / self.row_height).floor() as usize;
        (index < self.item_count).then_some(index)
    }

    fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        if self.item_count == 0 || rect.right() <= 0.0 || rect.x >= self.content_width {
            return Vec::new();
        }
        let start = (rect.y / self.row_height).floor().max(0.0) as usize;
        let end = (rect.bottom() / self.row_height).ceil().max(0.0) as usize;
        (start..end.min(self.item_count)).collect()
    }

    fn visible_row_range(&self) -> std::ops::Range<usize> {
        if self.item_count == 0 {
            return 0..0;
        }
        let start = (self.scroll_y / self.row_height).floor().max(0.0) as usize;
        let end = ((self.scroll_y + self.viewport_height) / self.row_height)
            .ceil()
            .max(0.0) as usize
            + 1;
        start.min(self.item_count)..end.min(self.item_count)
    }
}

#[derive(Clone, Copy, Debug)]
struct PrimaryClick {
    index: usize,
    point: ViewPoint,
    time: Instant,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PathHistory {
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl PathHistory {
    fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    fn push_back(&mut self, path: PathBuf) {
        push_limited_path(&mut self.back, path);
    }

    fn push_forward(&mut self, path: PathBuf) {
        push_limited_path(&mut self.forward, path);
    }

    fn clear_forward(&mut self) {
        self.forward.clear();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocationDraft {
    value: String,
    replace_on_insert: bool,
}

impl LocationDraft {
    fn new(value: String) -> Self {
        Self {
            value,
            replace_on_insert: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellPlace {
    group: &'static str,
    marker: &'static str,
    label: String,
    path: PathBuf,
    network: bool,
    trash: bool,
    root: bool,
    editable: bool,
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
            network,
            trash,
            root,
            editable,
        }
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
            Self::Item { path, .. } | Self::Blank { path } | Self::Place { path, .. } => path,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellContextMenuAction {
    Open,
    OpenInNewPane,
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
    Refresh,
    Properties,
    RemovePlace,
}

impl ShellContextMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::OpenInNewPane => "Open in New Pane",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::CopyLocation => "Copy Location",
            Self::Rename => "Rename",
            Self::MoveToTrash => "Move to Trash",
            Self::RestoreFromTrash => "Restore From Trash",
            Self::DeletePermanently => "Delete Permanently",
            Self::EmptyTrash => "Empty Trash",
            Self::AddToPlaces => "Add to Places",
            Self::CreateNew => "Create New",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
            Self::Refresh => "Refresh",
            Self::Properties => "Properties",
            Self::RemovePlace => "Remove",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenInNewPane => "open-in-new-pane",
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
            Self::Refresh => "refresh",
            Self::Properties => "properties",
            Self::RemovePlace => "remove-place",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ShellContextMenu {
    target: ShellContextTarget,
    position: ViewPoint,
    hovered_row: Option<usize>,
}

impl ShellContextMenu {
    fn new(target: ShellContextTarget, position: ViewPoint) -> Self {
        Self {
            target,
            position,
            hovered_row: None,
        }
    }
}

fn context_menu_actions(target: &ShellContextTarget) -> &'static [ShellContextMenuAction] {
    const ITEM_FILE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
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
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::EmptyTrash,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const EDITABLE_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::Open,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::RemovePlace,
        ShellContextMenuAction::Properties,
    ];
    match target {
        ShellContextTarget::Item { path, .. } if file_ops::is_in_trash_files_dir(path) => {
            TRASH_ITEM_ACTIONS
        }
        ShellContextTarget::Item { is_dir: true, .. } => ITEM_DIR_ACTIONS,
        ShellContextTarget::Item { .. } => ITEM_FILE_ACTIONS,
        ShellContextTarget::Blank { path } if file_ops::is_trash_files_dir(path) => {
            TRASH_BLANK_ACTIONS
        }
        ShellContextTarget::Blank { .. } => BLANK_ACTIONS,
        ShellContextTarget::Place { trash: true, .. } => TRASH_PLACE_ACTIONS,
        ShellContextTarget::Place { editable: true, .. } => EDITABLE_PLACE_ACTIONS,
        ShellContextTarget::Place { .. } => PLACE_ACTIONS,
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

struct ShellScene {
    path: PathBuf,
    view_mode: ShellViewMode,
    entries: Vec<Entry>,
    dir_count: usize,
    places: Vec<ShellPlace>,
    filtered_indexes: Vec<usize>,
    location_draft: Option<LocationDraft>,
    filter_active: bool,
    filter_pattern: String,
    show_hidden: bool,
    zoom_step: i32,
    scroll_x: f32,
    scroll_y: f32,
    places_scroll_y: f32,
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
    rubber_band: Option<RubberBand>,
    hit_tests: u64,
    selection_changes: u64,
    context_target_changes: u64,
    context_menu_actions: u64,
    properties_changes: u64,
    create_changes: u64,
    rename_changes: u64,
    open_changes: u64,
    copy_location_changes: u64,
    file_clipboard_changes: u64,
    paste_changes: u64,
    trash_changes: u64,
    places_changes: u64,
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

        let places = build_shell_places();
        eprintln!("[fika-wgpu] places entries={}", places.len());
        let filtered_indexes = filtered_indexes_for_entries(&entries, false, "");

        Ok(Self {
            path,
            view_mode,
            entries,
            dir_count,
            places,
            filtered_indexes,
            location_draft: None,
            filter_active: false,
            filter_pattern: String::new(),
            show_hidden: false,
            zoom_step: 0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            places_scroll_y: 0.0,
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
            rubber_band: None,
            hit_tests: 0,
            selection_changes: 0,
            context_target_changes: 0,
            context_menu_actions: 0,
            properties_changes: 0,
            create_changes: 0,
            rename_changes: 0,
            open_changes: 0,
            copy_location_changes: 0,
            file_clipboard_changes: 0,
            paste_changes: 0,
            trash_changes: 0,
            places_changes: 0,
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
        })
    }

    fn load_path(&mut self, path: PathBuf, size: PhysicalSize<u32>) -> Result<bool, String> {
        if path == self.path {
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

        let previous_path = self.path.clone();
        self.history.push_back(previous_path);
        self.history.clear_forward();
        self.apply_loaded_path(path, entries, dir_count, size);

        self.log_loaded_path(dir_count, &preview, elapsed);
        Ok(true)
    }

    fn reload_current_path(&mut self, size: PhysicalSize<u32>) -> Result<bool, String> {
        let load_start = Instant::now();
        let entries = read_entries_sync(&self.path)
            .map_err(|error| format!("read directory {}: {error}", self.path.display()))?;
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

        self.entries = entries;
        self.dir_count = dir_count;
        self.selection = remapped_selection;
        self.rebuild_filtered_indexes();
        let pruned_selection = self.selection.retain_indexes(&self.filtered_indexes);
        let selection_changed = previous_selection != self.selection;
        self.rubber_band = None;
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

        let current_path = self.path.clone();
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

        let current_path = self.path.clone();
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
        dir_count: usize,
        size: PhysicalSize<u32>,
    ) {
        self.path = path;
        self.entries = entries;
        self.dir_count = dir_count;
        self.rebuild_filtered_indexes();
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.location_draft = None;
        self.selection = ShellSelection::default();
        self.context_target = None;
        self.context_menu = None;
        self.properties_overlay = None;
        self.create_dialog = None;
        self.rename_dialog = None;
        self.rubber_band = None;
        self.last_primary_click = None;
        self.path_changes += 1;
        self.clamp_scroll(size);
    }

    fn log_loaded_path(&self, dir_count: usize, preview: &str, elapsed: Duration) {
        eprintln!(
            "[fika-wgpu] path={} entries={} dirs={} files={} load={}us changes={}",
            self.path.display(),
            self.entries.len(),
            dir_count,
            self.entries.len().saturating_sub(dir_count),
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
            self.path.display(),
            self.entries.len(),
            dir_count,
            self.entries.len().saturating_sub(dir_count),
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
        if self.view_mode == view_mode {
            return false;
        }
        self.view_mode = view_mode;
        self.rubber_band = None;
        self.view_switches += 1;
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] view-mode={} switches={} scroll_x={:.1} scroll_y={:.1}",
            self.view_mode.as_str(),
            self.view_switches,
            self.scroll_x,
            self.scroll_y
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
            self.scroll_x,
            self.scroll_y
        );
        true
    }

    fn apply_selection_command(&mut self, command: SelectionCommand) -> bool {
        let rubber_band_changed = self.rubber_band.take().is_some();
        let selection_changed = match command {
            SelectionCommand::SelectAll => self.selection.select_indexes(&self.filtered_indexes),
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
        resolve_location_input(&self.path, value)
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

    fn apply_location_command(
        &mut self,
        command: LocationCommand,
        size: PhysicalSize<u32>,
    ) -> bool {
        let old_draft = self.location_draft.clone();
        let old_filter_active = self.filter_active;

        match command {
            LocationCommand::Activate => {
                self.location_draft = Some(LocationDraft::new(self.path.display().to_string()));
                self.filter_active = false;
            }
            LocationCommand::Insert(value) => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                if draft.replace_on_insert {
                    draft.value.clear();
                    draft.replace_on_insert = false;
                }
                draft.value.push_str(&value);
            }
            LocationCommand::Backspace => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                if draft.replace_on_insert {
                    draft.value.clear();
                    draft.replace_on_insert = false;
                } else {
                    draft.value.pop();
                }
            }
            LocationCommand::Cancel => {
                self.location_draft = None;
            }
            LocationCommand::Complete => {
                let Some(draft) = self.location_draft.as_mut() else {
                    return false;
                };
                let Some(completed) = complete_location_input(&self.path, &draft.value) else {
                    return false;
                };
                draft.value = completed;
                draft.replace_on_insert = false;
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
        let selection_changed = self.selection.retain_indexes(&self.filtered_indexes);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.clamp_scroll(size);
        eprintln!(
            "[fika-wgpu] filter active={} pattern={:?} matches={} changes={} selection_changed={}",
            self.filter_active as u8,
            self.filter_pattern,
            self.filtered_indexes.len(),
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
        let selection_changed = self.selection.retain_indexes(&self.filtered_indexes);
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

    fn rebuild_filtered_indexes(&mut self) {
        self.filtered_indexes =
            filtered_indexes_for_entries(&self.entries, self.show_hidden, &self.filter_pattern);
    }

    fn filtered_entry_count(&self) -> usize {
        self.filtered_indexes.len()
    }

    fn model_index_for_layout_index(&self, layout_index: usize) -> Option<usize> {
        self.filtered_indexes.get(layout_index).copied()
    }

    fn layout_index_for_model_index(&self, model_index: usize) -> Option<usize> {
        self.filtered_indexes.binary_search(&model_index).ok()
    }

    fn selection_for_reloaded_entries(&self, entries: &[Entry]) -> ShellSelection {
        if self.selection.selected.is_empty() {
            return ShellSelection::default();
        }

        let selected_names = self
            .selection
            .selected
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .map(|entry| entry.name.to_string())
            .collect::<BTreeSet<_>>();
        let anchor_name = self
            .selection
            .anchor
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.name.to_string());
        let focus_name = self
            .selection
            .focus
            .and_then(|index| self.entries.get(index))
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
        view_mode_button_rects(size.width.max(1) as f32)
            .into_iter()
            .find_map(|(mode, rect)| rect.contains(point).then_some(mode))
    }

    fn path_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let width = size.width.max(1) as f32;
        let path_x = path_bar_start_x();
        let path_width = if self.is_location_editing() {
            path_bar_available_width(width, path_x)
        } else {
            path_placeholder_width(&self.path, width, path_x)
        };
        let rect = ViewRect {
            x: path_x,
            y: 14.0,
            width: path_width,
            height: 24.0,
        };
        (rect.width > 24.0).then_some(rect)
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
        path_navigation_button_rects()
            .into_iter()
            .find_map(|(action, rect)| {
                (rect.contains(point) && self.path_navigation_action_enabled(action))
                    .then_some(action)
            })
    }

    fn path_navigation_action_enabled(&self, action: PathNavigationAction) -> bool {
        match action {
            PathNavigationAction::Back => self.history.can_go_back(),
            PathNavigationAction::Forward => self.history.can_go_forward(),
            PathNavigationAction::Parent => self.parent_directory_path().is_some(),
            PathNavigationAction::Reload => true,
            PathNavigationAction::ToggleHidden => true,
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
        self.last_primary_click = None;
        self.context_target = None;
        self.context_menu = None;
        let place = self.places.get(index)?;
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
        let sidebar = self.places_sidebar_rect(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(self.places.len());
        let mut y = sidebar.y + PLACES_SIDEBAR_TOP_PADDING - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                y += PLACES_SECTION_HEIGHT;
            }
            let rect = ViewRect {
                x: sidebar.x + PLACES_SIDEBAR_PADDING_X,
                y,
                width: (sidebar.width - PLACES_SIDEBAR_PADDING_X * 2.0).max(1.0),
                height: PLACES_ROW_HEIGHT,
            };
            if rect.y < sidebar.bottom() && rect.bottom() > sidebar.y {
                rows.push((index, rect));
            }
            y += PLACES_ROW_HEIGHT + PLACES_ROW_GAP;
            previous_group = Some(place.group);
        }
        rows
    }

    fn places_sidebar_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        let width = places_sidebar_width(size);
        let height = (size.height as f32 - TOP_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(0.0);
        ViewRect {
            x: 0.0,
            y: TOP_BAR_HEIGHT,
            width,
            height,
        }
    }

    fn places_content_height(&self) -> f32 {
        if self.places.is_empty() {
            return PLACES_SIDEBAR_TOP_PADDING * 2.0;
        }

        let mut height = PLACES_SIDEBAR_TOP_PADDING;
        let mut previous_group = None;
        for place in &self.places {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                height += PLACES_SECTION_HEIGHT;
            }
            height += PLACES_ROW_HEIGHT + PLACES_ROW_GAP;
            previous_group = Some(place.group);
        }
        height - PLACES_ROW_GAP + PLACES_SIDEBAR_TOP_PADDING
    }

    fn max_places_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        let sidebar = self.places_sidebar_rect(size);
        (self.places_content_height() - sidebar.height).max(0.0)
    }

    fn places_scrollbar_thumb_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let sidebar = self.places_sidebar_rect(size);
        let max_scroll = self.max_places_scroll_y(size);
        if sidebar.width <= 0.0 || sidebar.height <= 0.0 || max_scroll <= f32::EPSILON {
            return None;
        }

        let track_height = (sidebar.height - PLACES_SCROLLBAR_MARGIN * 2.0).max(1.0);
        let content_height = self.places_content_height().max(sidebar.height);
        let thumb_height = (sidebar.height / content_height * track_height).clamp(
            PLACES_SCROLLBAR_MIN_THUMB_HEIGHT.min(track_height),
            track_height,
        );
        let travel = (track_height - thumb_height).max(0.0);
        let scroll_ratio = if max_scroll <= f32::EPSILON {
            0.0
        } else {
            (self.places_scroll_y / max_scroll).clamp(0.0, 1.0)
        };
        Some(ViewRect {
            x: sidebar.right() - PLACES_SCROLLBAR_MARGIN - PLACES_SCROLLBAR_WIDTH,
            y: sidebar.y + PLACES_SCROLLBAR_MARGIN + travel * scroll_ratio,
            width: PLACES_SCROLLBAR_WIDTH,
            height: thumb_height,
        })
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
            let entry = self.entries.get(index)?;
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
            path: self.path.clone(),
        })
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

    fn open_context_menu(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let changed = self.open_context_target(point, size);
        let old_menu = self.context_menu.clone();
        self.context_menu = self
            .context_target
            .clone()
            .map(|target| ShellContextMenu::new(target, point));
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
                    .map(|menu| context_menu_actions(&menu.target).len())
                    .unwrap_or(0)
            );
        }
        changed || menu_changed
    }

    fn close_context_menu(&mut self) -> bool {
        if self.context_menu.take().is_none() {
            return false;
        }
        eprintln!("[fika-wgpu] context-menu open=0");
        true
    }

    fn activate_or_close_context_menu(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        let action = self.context_menu_action_at_screen_point(point, size);
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

    fn context_menu_action_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellContextMenuAction> {
        let menu = self.context_menu.as_ref()?;
        let rect = context_menu_rect(menu, size);
        if !rect.contains(point) {
            return None;
        }
        let row = ((point.y - rect.y) / CONTEXT_MENU_ROW_HEIGHT).floor() as usize;
        context_menu_actions(&menu.target).get(row).copied()
    }

    fn update_context_menu_hover(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let hovered_row = self.context_menu.as_ref().and_then(|menu| {
            let rect = context_menu_rect(menu, size);
            rect.contains(point).then(|| {
                ((point.y - rect.y) / CONTEXT_MENU_ROW_HEIGHT)
                    .floor()
                    .max(0.0) as usize
            })
        });
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let row_count = context_menu_actions(&menu.target).len();
        let hovered_row = hovered_row.filter(|row| *row < row_count);
        let changed = menu.hovered_row != hovered_row;
        menu.hovered_row = hovered_row;
        changed
    }

    fn log_context_target(&self) {
        match self.context_target.as_ref() {
            Some(ShellContextTarget::Item {
                index,
                path,
                is_dir,
                selection_count,
            }) => eprintln!(
                "[fika-wgpu] context-target kind=item index={} dir={} selection={} path={} changes={}",
                index,
                *is_dir as u8,
                selection_count,
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Blank { path }) => eprintln!(
                "[fika-wgpu] context-target kind=blank path={} changes={}",
                path.display(),
                self.context_target_changes
            ),
            Some(ShellContextTarget::Place {
                index,
                label,
                path,
                network,
                trash,
                root,
                editable,
                ..
            }) => eprintln!(
                "[fika-wgpu] context-target kind=place index={} label={:?} network={} trash={} root={} editable={} path={} changes={}",
                index,
                label,
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
            ShellContextTarget::Place { path, .. } => Some(path.clone()),
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
            | Some(ShellContextTarget::Blank { path }) => {
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

        self.places = build_shell_places_from(user_places_path);
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

        self.places = build_shell_places_from(user_places_path);
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
            ShellContextTarget::Blank { path } => Some(path.clone()),
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
            .unwrap_or_else(|| self.path.clone());
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
            Some(ShellContextTarget::Blank { path }) => file_ops::is_trash_files_dir(path),
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
        self.trash_changes += 1;
        eprintln!(
            "[fika-wgpu] trash-view action={} success={} failure={} conflicts={} changes={}",
            action.as_str(),
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
        Ok(result)
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
        if properties_overlay_rect(overlay, size).contains(point) {
            return false;
        }
        self.close_properties_overlay()
    }

    fn is_create_dialog_open(&self) -> bool {
        self.create_dialog.is_some()
    }

    fn open_create_dialog_from_context(&mut self) -> bool {
        let Some(ShellContextTarget::Blank { path }) = self.context_target.as_ref() else {
            eprintln!(
                "[fika-wgpu] create-new-error target={}",
                self.context_target
                    .as_ref()
                    .map(ShellContextTarget::kind)
                    .unwrap_or("none")
            );
            return false;
        };
        let dialog = ShellCreateDialog::new(path.clone(), CreateEntryKind::Folder);
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
        let rect = create_dialog_rect(dialog, size);
        if !rect.contains(point) {
            return CreateDialogClick::Outside;
        }
        for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
            if create_kind_button_rect(rect, kind).contains(point) {
                return CreateDialogClick::Kind(kind);
            }
        }
        if create_dialog_cancel_button_rect(rect).contains(point) {
            return CreateDialogClick::Cancel;
        }
        if create_dialog_commit_button_rect(rect).contains(point) {
            return CreateDialogClick::Commit;
        }
        CreateDialogClick::Inside
    }

    fn select_entry_by_name(&mut self, name: &str, size: PhysicalSize<u32>) -> bool {
        let Some(index) = entry_index_by_name(&self.entries, name) else {
            return false;
        };
        if self.filtered_indexes.binary_search(&index).is_err() {
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
        let rect = rename_dialog_rect(dialog, size);
        if !rect.contains(point) {
            return RenameDialogClick::Outside;
        }
        if rename_dialog_cancel_button_rect(rect).contains(point) {
            return RenameDialogClick::Cancel;
        }
        if rename_dialog_commit_button_rect(rect).contains(point) {
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
            } => {
                let entry = self.entries.get(*index)?;
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
            ShellContextTarget::Blank { path } => Some(ShellPropertiesOverlay {
                title: format!("Properties - {}", path.display()),
                rows: vec![
                    property_row("Name", path_name_or_display(path)),
                    property_row("Type", "Folder".to_string()),
                    property_row("Entries", self.entries.len().to_string()),
                    property_row("Folders", self.dir_count.to_string()),
                    property_row(
                        "Files",
                        self.entries
                            .len()
                            .saturating_sub(self.dir_count)
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
        self.path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }

    fn directory_path_for_index(&self, index: usize) -> Option<PathBuf> {
        let entry = self.entries.get(index)?;
        entry
            .is_dir
            .then(|| self.entry_path_for_index(index))
            .flatten()
    }

    fn entry_path_for_index(&self, index: usize) -> Option<PathBuf> {
        let entry = self.entries.get(index)?;
        Some(
            entry
                .target_path
                .clone()
                .unwrap_or_else(|| self.path.join(entry.name.as_ref())),
        )
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

    fn layout(&self, size: PhysicalSize<u32>) -> ShellLayout {
        let item_count = self.filtered_entry_count();
        match self.view_mode {
            ShellViewMode::Icons => {
                ShellLayout::Icons(IconsLayout::new(item_count, self.icons_options(size)))
            }
            ShellViewMode::Compact => {
                ShellLayout::Compact(CompactLayout::new(item_count, self.compact_options(size)))
            }
            ShellViewMode::Details => ShellLayout::Details(DetailsLayout::new(
                item_count,
                self.content_width(size),
                self.viewport_height(size),
                self.scroll_y,
                self.details_row_height(),
                self.details_icon_size(),
            )),
        }
    }

    fn icons_options(&self, size: PhysicalSize<u32>) -> IconsLayoutOptions {
        let factor = self.zoom_factor();
        IconsLayoutOptions {
            viewport_width: self.content_width(size),
            viewport_height: self.viewport_height(size),
            reserved_bottom: 0.0,
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            padding: (8.0 * factor).round().clamp(6.0, 14.0),
            gap: (12.0 * factor).round().clamp(8.0, 22.0),
            item_width: (ICONS_ITEM_WIDTH * factor).round().clamp(82.0, 188.0),
            item_height: (ICONS_ITEM_HEIGHT * factor).round().clamp(76.0, 172.0),
            icon_size: (ICONS_ICON_SIZE * factor).round().clamp(28.0, 92.0),
            text_height: (18.0 * factor).round().clamp(16.0, 30.0),
        }
    }

    fn compact_options(&self, size: PhysicalSize<u32>) -> CompactLayoutOptions {
        let factor = self.zoom_factor();
        CompactLayoutOptions {
            viewport_width: self.content_width(size),
            viewport_height: self.viewport_height(size),
            reserved_bottom: 0.0,
            scroll_x: self.scroll_x,
            scroll_y: 0.0,
            padding: (6.0 * factor).round().clamp(4.0, 10.0),
            side_padding: (8.0 * factor).round().clamp(6.0, 14.0),
            gap: (8.0 * factor).round().clamp(6.0, 14.0),
            text_gap: (8.0 * factor).round().clamp(6.0, 14.0),
            item_width: (COMPACT_ITEM_WIDTH * factor).round().clamp(168.0, 360.0),
            item_height: (COMPACT_ITEM_HEIGHT * factor).round().clamp(34.0, 72.0),
            icon_size: (COMPACT_ICON_SIZE * factor).round().clamp(20.0, 56.0),
            text_height: (18.0 * factor).round().clamp(16.0, 26.0),
        }
    }

    fn zoom_factor(&self) -> f32 {
        (1.0 + self.zoom_step as f32 * ZOOM_STEP_SCALE).clamp(0.64, 1.64)
    }

    fn zoom_percent(&self) -> i32 {
        (self.zoom_factor() * 100.0).round() as i32
    }

    fn details_row_height(&self) -> f32 {
        (DETAILS_ROW_HEIGHT * self.zoom_factor())
            .round()
            .clamp(22.0, 44.0)
    }

    fn details_icon_size(&self) -> f32 {
        (DETAILS_ICON_SIZE * self.zoom_factor())
            .round()
            .clamp(16.0, 34.0)
    }

    fn build_frame(
        &self,
        size: PhysicalSize<u32>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
    ) -> SceneFrame {
        let layout_start = Instant::now();
        let mut vertices = Vec::with_capacity(64);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let content_origin_x = self.content_origin_x(size);
        let content_origin_y = self.content_origin_y();
        let content_width = self.content_width(size);
        let viewport_h = self.viewport_height(size);
        let status_bar = status_bar_rect(size);

        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            view_mode_surface_color(self.view_mode),
            size,
        );
        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width,
                height: TOP_BAR_HEIGHT,
            },
            [0.105, 0.112, 0.120, 1.0],
            size,
        );
        self.push_path_navigation_buttons(&mut vertices, text, size);
        if let Some(path_rect) = self.path_bar_rect(size) {
            let location_active = self.is_location_editing();
            push_rect(
                &mut vertices,
                path_rect,
                if location_active {
                    [0.190, 0.225, 0.252, 1.0]
                } else {
                    [0.170, 0.184, 0.198, 1.0]
                },
                size,
            );
            let path_label = self
                .location_draft
                .as_ref()
                .map(|draft| format!("{}|", draft.value))
                .unwrap_or_else(|| self.path.display().to_string());
            text.push_label(
                &path_label,
                ViewRect {
                    x: path_rect.x + 12.0,
                    y: path_rect.y + 3.0,
                    width: (path_rect.width - 24.0).max(1.0),
                    height: 18.0,
                },
                ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height: TOP_BAR_HEIGHT,
                },
                TextColor::rgb(222, 228, 232),
            );
        }
        self.push_view_mode_buttons(&mut vertices, text, size);
        self.push_places_sidebar(&mut vertices, text, size);
        push_rect(
            &mut vertices,
            ViewRect {
                x: content_origin_x,
                y: TOP_BAR_HEIGHT,
                width: content_width,
                height: (height - TOP_BAR_HEIGHT).max(1.0),
            },
            view_mode_content_color(self.view_mode),
            size,
        );
        self.push_filter_bar(&mut vertices, text, size);
        push_rect(
            &mut vertices,
            ViewRect {
                x: content_origin_x,
                y: TOP_BAR_HEIGHT,
                width: VIEW_MODE_RAIL_WIDTH,
                height: (height - TOP_BAR_HEIGHT).max(1.0),
            },
            view_mode_badge_color(self.view_mode),
            size,
        );
        push_rect(
            &mut vertices,
            ViewRect {
                x: content_origin_x,
                y: content_origin_y,
                width: content_width,
                height: VIEW_MODE_STRIPE_HEIGHT,
            },
            view_mode_badge_color(self.view_mode),
            size,
        );
        if self.view_mode == ShellViewMode::Details {
            self.push_details_header(&mut vertices, text, size);
        }

        let layout = self.layout(size);
        let content_size = layout.content_size();
        let visible_layout_items = layout.visible_items();
        let first_item_rect = visible_layout_items.first().map(|item| item.item_rect);
        let content_clip = ViewRect {
            x: content_origin_x,
            y: content_origin_y,
            width: content_width,
            height: viewport_h,
        };
        let mut visible_items = 0usize;
        for item in visible_layout_items {
            visible_items += 1;
            match self.view_mode {
                ShellViewMode::Icons | ShellViewMode::Compact => {
                    self.push_item(&mut vertices, text, icons, item, content_clip, size);
                }
                ShellViewMode::Details => {
                    self.push_details_item(&mut vertices, text, icons, item, content_clip, size);
                }
            }
        }
        self.push_rubber_band(&mut vertices, content_clip, size);
        self.push_status_bar(&mut vertices, text, size, visible_items, status_bar);
        self.push_context_menu_overlay(&mut vertices, text, size);
        self.push_properties_overlay(&mut vertices, text, size);
        self.push_create_dialog_overlay(&mut vertices, text, size);
        self.push_rename_dialog_overlay(&mut vertices, text, size);

        SceneFrame {
            layout_us: layout_start.elapsed().as_micros(),
            visible_items,
            quad_count: vertices.len() / 6,
            content_size,
            first_item_rect,
            vertices,
            text_stats: TextFrameStats::default(),
            icon_stats: IconFrameStats::default(),
        }
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
        push_rect(vertices, sidebar, [0.074, 0.082, 0.091, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: sidebar.right(),
                y: sidebar.y,
                width: PLACES_SIDEBAR_SPLITTER_WIDTH,
                height: sidebar.height,
            },
            [0.18, 0.20, 0.22, 1.0],
            size,
        );

        let active_place = active_shell_place_index(&self.places, &self.path);
        let mut y = sidebar.y + PLACES_SIDEBAR_TOP_PADDING - self.places_scroll_y;
        let mut previous_group = None;
        for (index, place) in self.places.iter().enumerate() {
            if !place.group.is_empty() && previous_group != Some(place.group) {
                let section = ViewRect {
                    x: sidebar.x + PLACES_SIDEBAR_PADDING_X + 4.0,
                    y: y + 4.0,
                    width: (sidebar.width - PLACES_SIDEBAR_PADDING_X * 2.0 - 8.0).max(1.0),
                    height: 16.0,
                };
                if section.y < sidebar.bottom() && section.bottom() > sidebar.y {
                    text.push_label(place.group, section, sidebar, TextColor::rgb(136, 148, 160));
                }
                y += PLACES_SECTION_HEIGHT;
            }

            let row = ViewRect {
                x: sidebar.x + PLACES_SIDEBAR_PADDING_X,
                y,
                width: (sidebar.width - PLACES_SIDEBAR_PADDING_X * 2.0).max(1.0),
                height: PLACES_ROW_HEIGHT,
            };
            if row.y < sidebar.bottom() && row.bottom() > sidebar.y {
                let active = active_place == Some(index);
                let hovered = self.hovered_place == Some(index);
                push_clipped_rect(
                    vertices,
                    row,
                    sidebar,
                    place_row_background_color(active, hovered),
                    size,
                );
                let icon = ViewRect {
                    x: row.x + 8.0,
                    y: row.y + (row.height - PLACES_ICON_SIZE) / 2.0,
                    width: PLACES_ICON_SIZE,
                    height: PLACES_ICON_SIZE,
                };
                push_clipped_rect(vertices, icon, sidebar, place_marker_color(place), size);
                text.push_label(
                    place.marker,
                    ViewRect {
                        x: icon.x + 3.0,
                        y: icon.y + 1.0,
                        width: (icon.width - 6.0).max(1.0),
                        height: 14.0,
                    },
                    sidebar,
                    TextColor::rgb(248, 250, 252),
                );
                text.push_label(
                    &place.label,
                    ViewRect {
                        x: row.x + 34.0,
                        y: row.y + 6.0,
                        width: (row.width - 42.0).max(1.0),
                        height: 18.0,
                    },
                    sidebar,
                    if active {
                        TextColor::rgb(244, 249, 252)
                    } else {
                        TextColor::rgb(194, 204, 214)
                    },
                );
            }

            y += PLACES_ROW_HEIGHT + PLACES_ROW_GAP;
            previous_group = Some(place.group);
        }

        if let Some(thumb) = self.places_scrollbar_thumb_rect(size) {
            let track = ViewRect {
                x: thumb.x,
                y: sidebar.y + PLACES_SCROLLBAR_MARGIN,
                width: thumb.width,
                height: (sidebar.height - PLACES_SCROLLBAR_MARGIN * 2.0).max(1.0),
            };
            push_rect(vertices, track, [0.12, 0.135, 0.15, 1.0], size);
            push_rect(vertices, thumb, [0.48, 0.54, 0.60, 1.0], size);
        }
    }

    fn push_path_navigation_buttons(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let width = size.width.max(1) as f32;
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width,
            height: TOP_BAR_HEIGHT,
        };
        for (action, rect) in path_navigation_button_rects() {
            let enabled = self.path_navigation_action_enabled(action);
            let active = matches!(action, PathNavigationAction::ToggleHidden) && self.show_hidden;
            push_rect(
                vertices,
                rect,
                if active {
                    view_mode_badge_color(self.view_mode)
                } else if enabled {
                    [0.145, 0.154, 0.164, 1.0]
                } else {
                    [0.095, 0.101, 0.108, 1.0]
                },
                size,
            );
            text.push_label(
                action.label(),
                ViewRect {
                    x: rect.x + 7.0,
                    y: rect.y + 3.0,
                    width: (rect.width - 14.0).max(1.0),
                    height: 18.0,
                },
                clip,
                if active {
                    TextColor::rgb(246, 250, 252)
                } else if enabled {
                    TextColor::rgb(222, 228, 232)
                } else {
                    TextColor::rgb(105, 115, 124)
                },
            );
        }
    }

    fn push_view_mode_buttons(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let width = size.width.max(1) as f32;
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width,
            height: TOP_BAR_HEIGHT,
        };
        for (mode, rect) in view_mode_button_rects(width) {
            let active = mode == self.view_mode;
            push_rect(
                vertices,
                rect,
                if active {
                    view_mode_badge_color(mode)
                } else {
                    [0.145, 0.154, 0.164, 1.0]
                },
                size,
            );
            text.push_label(
                mode.label(),
                ViewRect {
                    x: rect.x + 10.0,
                    y: rect.y + 3.0,
                    width: (rect.width - 20.0).max(1.0),
                    height: 18.0,
                },
                clip,
                if active {
                    TextColor::rgb(246, 250, 252)
                } else {
                    TextColor::rgb(176, 187, 198)
                },
            );
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
            height: DETAILS_HEADER_HEIGHT,
        };
        push_rect(vertices, header, [0.100, 0.108, 0.117, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x,
                y: header.bottom() - 1.0,
                width,
                height: 1.0,
            },
            [0.20, 0.22, 0.24, 1.0],
            size,
        );
        let columns = [
            ("Name", 34.0, DETAILS_NAME_WIDTH - 42.0),
            ("Size", DETAILS_NAME_WIDTH + 8.0, DETAILS_SIZE_WIDTH - 16.0),
            (
                "Modified",
                DETAILS_NAME_WIDTH + DETAILS_SIZE_WIDTH + 8.0,
                DETAILS_MODIFIED_WIDTH - 16.0,
            ),
        ];
        for (label, x, width) in columns {
            text.push_label(
                label,
                ViewRect {
                    x: header.x + x,
                    y: header.y + 6.0,
                    width: width.max(1.0),
                    height: 18.0,
                },
                header,
                TextColor::rgb(170, 181, 192),
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
        push_rect(vertices, rect, [0.112, 0.122, 0.132, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.bottom() - 1.0,
                width: rect.width,
                height: 1.0,
            },
            [0.22, 0.25, 0.28, 1.0],
            size,
        );
        text.push_label(
            "Filter:",
            ViewRect {
                x: rect.x + 12.0,
                y: rect.y + 6.0,
                width: 54.0,
                height: 18.0,
            },
            rect,
            TextColor::rgb(176, 187, 198),
        );
        let pattern = if self.filter_pattern.is_empty() {
            ""
        } else {
            self.filter_pattern.as_str()
        };
        text.push_label(
            pattern,
            ViewRect {
                x: rect.x + 66.0,
                y: rect.y + 6.0,
                width: (rect.width - 78.0).max(1.0),
                height: 18.0,
            },
            rect,
            TextColor::rgb(230, 236, 241),
        );
    }

    fn push_item(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        item: fika_core::ItemLayout,
        content_clip: ViewRect,
        size: PhysicalSize<u32>,
    ) {
        let Some(entry_index) = self.model_index_for_layout_index(item.model_index) else {
            return;
        };
        let Some(entry) = self.entries.get(entry_index) else {
            return;
        };
        let item_rect = self.content_to_screen(item.item_rect, size);
        let visual_rect = self.content_to_screen(item.visual_rect, size);
        let icon_rect = self.content_to_screen(item.icon_rect, size);
        let text_rect = self.content_to_screen(item.text_rect, size);
        let selected = self.selection.contains(entry_index);
        let hovered = self.hovered_index == Some(entry_index);

        push_clipped_rect(
            vertices,
            visual_rect,
            content_clip,
            item_background_color(selected, hovered),
            size,
        );
        if selected {
            push_clipped_rect_outline(
                vertices,
                visual_rect,
                content_clip,
                1.5,
                [0.38, 0.64, 0.92, 0.95],
                size,
            );
        }

        if icons.push_icon(&self.path, entry, icon_rect, content_clip) {
            // The atlas draw covers the icon slot; fallback geometry remains for misses.
        } else if entry.is_dir {
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

        let text_color = if selected {
            TextColor::rgb(242, 248, 252)
        } else if entry.is_dir {
            TextColor::rgb(222, 205, 163)
        } else {
            TextColor::rgb(194, 202, 212)
        };
        text.push_label(entry.name.as_ref(), text_rect, content_clip, text_color);

        let index_marker = ViewRect {
            x: item_rect.x + 7.0,
            y: item_rect.y + 7.0,
            width: 5.0,
            height: 5.0,
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

    fn push_details_item(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        item: fika_core::ItemLayout,
        content_clip: ViewRect,
        size: PhysicalSize<u32>,
    ) {
        let Some(entry_index) = self.model_index_for_layout_index(item.model_index) else {
            return;
        };
        let Some(entry) = self.entries.get(entry_index) else {
            return;
        };
        let row_rect = self.content_to_screen(item.item_rect, size);
        let icon_rect = self.content_to_screen(item.icon_rect, size);
        let name_rect = self.content_to_screen(item.text_rect, size);
        let selected = self.selection.contains(entry_index);
        let hovered = self.hovered_index == Some(entry_index);

        push_clipped_rect(
            vertices,
            row_rect,
            content_clip,
            details_row_background_color(selected, hovered, entry_index),
            size,
        );
        if selected {
            push_clipped_rect_outline(
                vertices,
                row_rect,
                content_clip,
                1.0,
                [0.38, 0.64, 0.92, 0.92],
                size,
            );
        }

        if !icons.push_icon(&self.path, entry, icon_rect, content_clip) {
            push_fallback_icon(vertices, entry, icon_rect, content_clip, size);
        }

        let text_color = if selected {
            TextColor::rgb(242, 248, 252)
        } else {
            TextColor::rgb(202, 211, 220)
        };
        text.push_label(entry.name.as_ref(), name_rect, content_clip, text_color);
        let metadata_y = row_rect.y + (row_rect.height - 18.0).max(0.0) / 2.0;
        text.push_label(
            &details_size_label(entry),
            ViewRect {
                x: self.content_origin_x(size) + DETAILS_NAME_WIDTH + 8.0 - self.scroll_x,
                y: metadata_y,
                width: DETAILS_SIZE_WIDTH - 16.0,
                height: 18.0,
            },
            content_clip,
            TextColor::rgb(170, 181, 192),
        );
        text.push_label(
            &format_modified_secs(entry.modified_secs),
            ViewRect {
                x: self.content_origin_x(size) + DETAILS_NAME_WIDTH + DETAILS_SIZE_WIDTH + 8.0
                    - self.scroll_x,
                y: metadata_y,
                width: DETAILS_MODIFIED_WIDTH - 16.0,
                height: 18.0,
            },
            content_clip,
            TextColor::rgb(170, 181, 192),
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
        push_rect(vertices, rect, [0.088, 0.096, 0.104, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: 0.0,
                y: rect.y,
                width: rect.width,
                height: 1.0,
            },
            [0.19, 0.21, 0.23, 1.0],
            size,
        );

        let mut status = format!(
            "{} entries ({} dirs, {} files) | {} selected | {} visible | {} | {}%",
            self.entries.len(),
            self.dir_count,
            self.entries.len().saturating_sub(self.dir_count),
            self.selection.len(),
            visible_items,
            self.view_mode.label(),
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
                x: 12.0,
                y: rect.y + 5.0,
                width: (rect.width - 24.0).max(1.0),
                height: 18.0,
            },
            rect,
            TextColor::rgb(178, 188, 198),
        );
    }

    fn push_context_menu_overlay(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        size: PhysicalSize<u32>,
    ) {
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let rect = context_menu_rect(menu, size);
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.98], size);
        push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.28, 0.32, 0.36, 1.0], size);

        for (row, action) in context_menu_actions(&menu.target).iter().enumerate() {
            let row_rect = ViewRect {
                x: rect.x,
                y: rect.y + row as f32 * CONTEXT_MENU_ROW_HEIGHT,
                width: rect.width,
                height: CONTEXT_MENU_ROW_HEIGHT,
            };
            if menu.hovered_row == Some(row) {
                push_rect(vertices, row_rect, [0.19, 0.33, 0.50, 0.88], size);
            } else if row % 2 == 1 {
                push_rect(vertices, row_rect, [0.105, 0.114, 0.126, 0.44], size);
            }
            text.push_label(
                action.label(),
                ViewRect {
                    x: row_rect.x + 12.0,
                    y: row_rect.y + 6.0,
                    width: (row_rect.width - 24.0).max(1.0),
                    height: 18.0,
                },
                rect,
                if menu.hovered_row == Some(row) {
                    TextColor::rgb(246, 250, 252)
                } else {
                    TextColor::rgb(218, 226, 234)
                },
            );
        }
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
        let rect = properties_overlay_rect(overlay, size);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: PROPERTIES_TITLE_HEIGHT,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            &overlay.title,
            ViewRect {
                x: rect.x + 16.0,
                y: rect.y + 12.0,
                width: (rect.width - 32.0).max(1.0),
                height: 18.0,
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        let rows_y = rect.y + PROPERTIES_TITLE_HEIGHT + 10.0;
        for (index, row) in overlay.rows.iter().enumerate() {
            let y = rows_y + index as f32 * PROPERTIES_ROW_HEIGHT;
            text.push_label(
                row.label,
                ViewRect {
                    x: rect.x + 16.0,
                    y,
                    width: 92.0,
                    height: 18.0,
                },
                rect,
                TextColor::rgb(164, 176, 188),
            );
            text.push_label(
                &row.value,
                ViewRect {
                    x: rect.x + 116.0,
                    y,
                    width: (rect.width - 132.0).max(1.0),
                    height: 18.0,
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
        let rect = create_dialog_rect(dialog, size);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: CREATE_DIALOG_TITLE_HEIGHT,
            },
            [0.145, 0.158, 0.174, 1.0],
            size,
        );
        text.push_label(
            "Create New",
            ViewRect {
                x: rect.x + 16.0,
                y: rect.y + 12.0,
                width: (rect.width - 32.0).max(1.0),
                height: 18.0,
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
            let button = create_kind_button_rect(rect, kind);
            let active = dialog.kind == kind;
            push_rect(
                vertices,
                button,
                if active {
                    view_mode_badge_color(self.view_mode)
                } else {
                    [0.150, 0.162, 0.176, 1.0]
                },
                size,
            );
            text.push_label(
                kind.label(),
                ViewRect {
                    x: button.x + 10.0,
                    y: button.y + 4.0,
                    width: (button.width - 20.0).max(1.0),
                    height: 18.0,
                },
                rect,
                if active {
                    TextColor::rgb(246, 250, 252)
                } else {
                    TextColor::rgb(196, 207, 218)
                },
            );
        }

        let input = create_dialog_input_rect(rect);
        push_rect(vertices, input, [0.078, 0.086, 0.096, 1.0], size);
        push_clipped_rect_outline(vertices, input, rect, 1.0, [0.26, 0.31, 0.36, 1.0], size);
        let draft = format!("{}|", dialog.name);
        text.push_label(
            &draft,
            ViewRect {
                x: input.x + 10.0,
                y: input.y + 7.0,
                width: (input.width - 20.0).max(1.0),
                height: 18.0,
            },
            input,
            TextColor::rgb(230, 236, 241),
        );

        if let Some(error) = dialog.error.as_ref() {
            text.push_label(
                error,
                ViewRect {
                    x: rect.x + 16.0,
                    y: input.bottom() + 8.0,
                    width: (rect.width - 32.0).max(1.0),
                    height: 18.0,
                },
                rect,
                TextColor::rgb(238, 132, 122),
            );
        }

        let cancel = create_dialog_cancel_button_rect(rect);
        let commit = create_dialog_commit_button_rect(rect);
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
                    x: button.x + 10.0,
                    y: button.y + 4.0,
                    width: (button.width - 20.0).max(1.0),
                    height: 18.0,
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
        let rect = rename_dialog_rect(dialog, size);
        push_rect(vertices, rect, [0.118, 0.128, 0.140, 0.99], size);
        push_clipped_rect_outline(vertices, rect, screen, 1.0, [0.34, 0.38, 0.43, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: RENAME_DIALOG_TITLE_HEIGHT,
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
                x: rect.x + 16.0,
                y: rect.y + 12.0,
                width: (rect.width - 32.0).max(1.0),
                height: 18.0,
            },
            rect,
            TextColor::rgb(238, 244, 249),
        );

        let input = rename_dialog_input_rect(rect);
        push_rect(vertices, input, [0.078, 0.086, 0.096, 1.0], size);
        push_clipped_rect_outline(vertices, input, rect, 1.0, [0.26, 0.31, 0.36, 1.0], size);
        let draft = format!("{}|", dialog.name);
        text.push_label(
            &draft,
            ViewRect {
                x: input.x + 10.0,
                y: input.y + 7.0,
                width: (input.width - 20.0).max(1.0),
                height: 18.0,
            },
            input,
            TextColor::rgb(230, 236, 241),
        );

        if let Some(error) = dialog.error.as_ref() {
            text.push_label(
                error,
                ViewRect {
                    x: rect.x + 16.0,
                    y: input.bottom() + 8.0,
                    width: (rect.width - 32.0).max(1.0),
                    height: 18.0,
                },
                rect,
                TextColor::rgb(238, 132, 122),
            );
        }

        let cancel = rename_dialog_cancel_button_rect(rect);
        let commit = rename_dialog_commit_button_rect(rect);
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
                    x: button.x + 10.0,
                    y: button.y + 4.0,
                    width: (button.width - 20.0).max(1.0),
                    height: 18.0,
                },
                rect,
                TextColor::rgb(238, 244, 249),
            );
        }
    }

    fn content_to_screen(&self, rect: ViewRect, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: rect.x - self.scroll_x + self.content_origin_x(size),
            y: rect.y - self.scroll_y + self.content_origin_y(),
            width: rect.width,
            height: rect.height,
        }
    }

    fn content_origin_x(&self, size: PhysicalSize<u32>) -> f32 {
        let sidebar_width = places_sidebar_width(size);
        if sidebar_width <= 0.0 {
            0.0
        } else {
            sidebar_width + PLACES_SIDEBAR_SPLITTER_WIDTH
        }
    }

    fn content_origin_y(&self) -> f32 {
        self.details_header_y()
            + if self.view_mode == ShellViewMode::Details {
                DETAILS_HEADER_HEIGHT
            } else {
                0.0
            }
    }

    fn details_header_y(&self) -> f32 {
        TOP_BAR_HEIGHT + self.filter_bar_height()
    }

    fn filter_bar_height(&self) -> f32 {
        if self.filter_active || !self.filter_pattern.is_empty() {
            FILTER_BAR_HEIGHT
        } else {
            0.0
        }
    }

    fn filter_bar_rect(&self, size: PhysicalSize<u32>) -> Option<ViewRect> {
        let height = self.filter_bar_height();
        (height > 0.0).then(|| ViewRect {
            x: self.content_origin_x(size),
            y: TOP_BAR_HEIGHT,
            width: self.content_width(size),
            height,
        })
    }

    fn content_width(&self, size: PhysicalSize<u32>) -> f32 {
        (size.width as f32 - self.content_origin_x(size)).max(1.0)
    }

    fn viewport_height(&self, size: PhysicalSize<u32>) -> f32 {
        (size.height as f32 - self.content_origin_y() - STATUS_BAR_HEIGHT).max(1.0)
    }

    fn content_screen_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: self.content_origin_x(size),
            y: self.content_origin_y(),
            width: self.content_width(size),
            height: self.viewport_height(size),
        }
    }

    fn clamp_scroll(&mut self, size: PhysicalSize<u32>) {
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x(size));
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y(size));
        if self.view_mode != ShellViewMode::Compact {
            self.scroll_x = 0.0;
        }
        if self.view_mode == ShellViewMode::Compact {
            self.scroll_y = 0.0;
        }
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

        let old_x = self.scroll_x;
        let old_y = self.scroll_y;
        match self.view_mode {
            ShellViewMode::Compact => {
                self.scroll_x = (self.scroll_x + delta_y).clamp(0.0, self.max_scroll_x(size));
                self.scroll_y = 0.0;
            }
            ShellViewMode::Icons | ShellViewMode::Details => {
                self.scroll_x = 0.0;
                self.scroll_y = (self.scroll_y + delta_y).clamp(0.0, self.max_scroll_y(size));
            }
        }
        let scrolled = (self.scroll_x - old_x).abs() > f32::EPSILON
            || (self.scroll_y - old_y).abs() > f32::EPSILON;
        let hover_changed = self.refresh_hover(size);
        scrolled || hover_changed
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

    fn max_scroll_x(&self, size: PhysicalSize<u32>) -> f32 {
        let layout = self.layout(size);
        (layout.content_size().width - self.content_width(size)).max(0.0)
    }

    fn max_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        let layout = self.layout(size);
        (layout.content_size().height - self.viewport_height(size)).max(0.0)
    }

    fn set_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if self.context_menu.is_some() {
            return self.update_context_menu_hover(point, size);
        }
        if self.rubber_band.is_some() {
            return self.update_rubber_band(point, size);
        }
        self.refresh_hover(size)
    }

    fn clear_pointer(&mut self) -> bool {
        self.pointer = None;
        let changed = self.hovered_index.take().is_some() || self.hovered_place.take().is_some();
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
        if hit.is_some() {
            let selection_changed = self.selection.apply_click(hit, click.extend, click.toggle);
            if selection_changed {
                self.selection_changes += 1;
            }
            return hover_changed || selection_changed;
        }
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

        let old_scroll_y = self.scroll_y;
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
            || (self.scroll_y - old_scroll_y).abs() > f32::EPSILON
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
        if !self.content_screen_rect(size).contains(point) {
            return None;
        }
        let content_point =
            screen_to_content_point(point, self.scroll_offset(), self.content_screen_rect(size))?;
        let layout = self.layout(size);
        let layout_index = layout.hit_test_content_point(content_point)?;
        let item = layout.item(layout_index)?;
        item.visual_rect
            .contains(content_point)
            .then(|| self.model_index_for_layout_index(layout_index))
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
        match self.view_mode {
            ShellViewMode::Compact => {
                if item.visual_rect.x < self.scroll_x + padding {
                    self.scroll_x = (item.visual_rect.x - padding).max(0.0);
                } else if item.visual_rect.right()
                    > self.scroll_x + self.content_width(size) - padding
                {
                    self.scroll_x = item.visual_rect.right() - self.content_width(size) + padding;
                }
            }
            ShellViewMode::Icons | ShellViewMode::Details => {
                if item.visual_rect.y < self.scroll_y + padding {
                    self.scroll_y = (item.visual_rect.y - padding).max(0.0);
                } else if item.visual_rect.bottom() > self.scroll_y + viewport_h - padding {
                    self.scroll_y = item.visual_rect.bottom() - viewport_h + padding;
                }
            }
        }
        self.clamp_scroll(size);
    }

    fn scroll_offset(&self) -> ViewPoint {
        ViewPoint {
            x: self.scroll_x,
            y: self.scroll_y,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavigationAction {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug)]
struct SelectionClick {
    point: ViewPoint,
    extend: bool,
    toggle: bool,
}

#[derive(Clone, Debug)]
struct RubberBand {
    start: ViewPoint,
    current: ViewPoint,
    active: bool,
    mode: RubberBandMode,
    base_selection: ShellSelection,
}

impl RubberBand {
    fn new(start: ViewPoint, mode: RubberBandMode, base_selection: ShellSelection) -> Self {
        Self {
            start,
            current: start,
            active: false,
            mode,
            base_selection,
        }
    }

    fn update(&mut self, current: ViewPoint) {
        self.current = current;
        if !self.active
            && ((self.current.x - self.start.x).abs() + (self.current.y - self.start.y).abs())
                >= RUBBER_BAND_START_THRESHOLD
        {
            self.active = true;
        }
    }

    fn active_rect(&self) -> Option<ViewRect> {
        self.active
            .then(|| rect_from_points(self.start, self.current))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RubberBandMode {
    Replace,
    Extend,
    Toggle,
}

impl RubberBandMode {
    fn from_modifiers(extend: bool, toggle: bool) -> Self {
        if toggle {
            Self::Toggle
        } else if extend {
            Self::Extend
        } else {
            Self::Replace
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ShellSelection {
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    focus: Option<usize>,
}

impl ShellSelection {
    fn contains(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    fn len(&self) -> usize {
        self.selected.len()
    }

    fn focus_or_first_selected(&self) -> Option<usize> {
        self.focus.or_else(|| self.selected.iter().next().copied())
    }

    fn select_indexes(&mut self, indexes: &[usize]) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected = indexes.iter().copied().collect();
        self.anchor = indexes.first().copied();
        self.focus = indexes.last().copied();

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    fn retain_indexes(&mut self, indexes: &[usize]) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected
            .retain(|index| indexes.binary_search(index).is_ok());
        self.anchor = self
            .anchor
            .filter(|index| self.selected.contains(index))
            .or_else(|| self.selected.iter().next().copied());
        self.focus = self
            .focus
            .filter(|index| self.selected.contains(index))
            .or_else(|| self.selected.iter().next_back().copied());

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    fn clear(&mut self) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected.clear();
        self.anchor = None;
        self.focus = None;

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    fn focus_selected(&mut self, index: usize) -> bool {
        if !self.selected.contains(&index) {
            return false;
        }
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        if self.anchor.is_none() {
            self.anchor = Some(index);
        }
        self.focus = Some(index);

        old_anchor != self.anchor || old_focus != self.focus
    }

    fn apply_click(&mut self, hit: Option<usize>, extend: bool, toggle: bool) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        match hit {
            Some(index) if extend => {
                let anchor = self.anchor.unwrap_or(index);
                self.selected.clear();
                for item in anchor.min(index)..=anchor.max(index) {
                    self.selected.insert(item);
                }
                self.anchor = Some(anchor);
                self.focus = Some(index);
            }
            Some(index) if toggle => {
                if !self.selected.remove(&index) {
                    self.selected.insert(index);
                    self.anchor = Some(index);
                    self.focus = Some(index);
                } else if self.anchor == Some(index) {
                    self.anchor = self.selected.iter().next().copied();
                    self.focus = self.anchor;
                } else {
                    self.focus = Some(index);
                }
            }
            Some(index) => {
                self.selected.clear();
                self.selected.insert(index);
                self.anchor = Some(index);
                self.focus = Some(index);
            }
            None if !extend && !toggle => {
                self.selected.clear();
                self.anchor = None;
                self.focus = None;
            }
            None => {}
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    fn apply_navigation(&mut self, target: usize, extend: bool) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        if extend {
            let anchor = self.anchor.or(self.focus).unwrap_or(target);
            self.selected.clear();
            for item in anchor.min(target)..=anchor.max(target) {
                self.selected.insert(item);
            }
            self.anchor = Some(anchor);
            self.focus = Some(target);
        } else {
            self.selected.clear();
            self.selected.insert(target);
            self.anchor = Some(target);
            self.focus = Some(target);
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }

    fn apply_rubber_band(
        &mut self,
        base: &ShellSelection,
        indexes: &[usize],
        mode: RubberBandMode,
    ) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        match mode {
            RubberBandMode::Replace => {
                self.selected.clear();
                self.selected.extend(indexes.iter().copied());
                self.anchor = indexes.first().copied();
                self.focus = indexes.last().copied();
            }
            RubberBandMode::Extend => {
                *self = base.clone();
                self.selected.extend(indexes.iter().copied());
                if let Some(last) = indexes.last().copied() {
                    self.anchor = self.anchor.or_else(|| indexes.first().copied());
                    self.focus = Some(last);
                }
            }
            RubberBandMode::Toggle => {
                *self = base.clone();
                for index in indexes {
                    if !self.selected.remove(index) {
                        self.selected.insert(*index);
                    }
                }
                if self.selected.is_empty() {
                    self.anchor = None;
                    self.focus = None;
                } else if let Some(last) = indexes.last().copied() {
                    self.focus = Some(last);
                    if self
                        .anchor
                        .is_none_or(|anchor| !self.selected.contains(&anchor))
                    {
                        self.anchor = self.selected.iter().next().copied();
                    }
                }
            }
        }

        old_selected != self.selected || old_anchor != self.anchor || old_focus != self.focus
    }
}

struct SceneFrame {
    vertices: Vec<QuadVertex>,
    visible_items: usize,
    quad_count: usize,
    content_size: ViewSize,
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
    icon_renderer: IconRenderer,
    text_renderer: TextRenderer,
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
            .find(|format| format.is_srgb())
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
        let icon_renderer = IconRenderer::new(&device, config.format);
        let text_renderer = TextRenderer::new(&device, config.format);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            size,
            quad_renderer,
            icon_renderer,
            text_renderer,
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
        scene: &ShellScene,
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
                        scene.view_mode.as_str()
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
                                scene.view_mode.as_str()
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
                                scene.view_mode.as_str()
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
                        scene.view_mode.as_str()
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
            &mut self.icon_renderer,
            &self.device,
            &self.queue,
            scene,
            self.size,
        );
        self.quad_renderer
            .upload(&self.device, &self.queue, &scene_frame.vertices);

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
                        load: wgpu::LoadOp::Clear(view_mode_clear_color(scene.view_mode)),
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
                "[fika-wgpu] frame={} reason={} view={} zoom={} zoom_changes={} path={} entries={} filtered={} show_hidden={} hidden_changes={} location_active={} location_changes={} filter_active={} filter_changes={} places={} place_hover={} places_changes={} places_scroll_y={:.1} places_scroll_changes={} visible={} selected={} hover={} context={} context_menu={} context_changes={} context_actions={} properties={} properties_changes={} create_dialog={} create_changes={} rename_dialog={} rename_changes={} open_changes={} copy_location_changes={} file_clipboard_changes={} paste_changes={} trash_changes={} rubber_band={} hit_tests={} selection_changes={} keyboard_nav={} rubber_band_updates={} view_switches={} path_changes={} reloads={} quads={} layout_content={:.1}x{:.1} first_item={:.1},{:.1},{:.1},{:.1} icons={} icon_quads={} icon_fallbacks={} icon_cache={}/{} entries={} bytes={} icon_atlas={}x{}:{}b icon_resolve={}us icon_raster={}us text_labels={} text_quads={} text_cache={}/{} entries={} bytes={} batches={} scroll_x={:.1} scroll_y={:.1} layout={}us text_raster={}us text_atlas={}x{}:{}b render={}us",
                self.frame_count,
                reason,
                scene.view_mode.as_str(),
                scene.zoom_percent(),
                scene.zoom_changes,
                scene.path.display(),
                scene.entries.len(),
                scene.filtered_entry_count(),
                scene.show_hidden as u8,
                scene.hidden_changes,
                scene.is_location_editing() as u8,
                scene.location_changes,
                scene.filter_active as u8,
                scene.filter_changes,
                scene.places.len(),
                scene.hovered_place.map(|index| index as i64).unwrap_or(-1),
                scene.places_changes,
                scene.places_scroll_y,
                scene.places_scroll_changes,
                scene_frame.visible_items,
                scene.selection.len(),
                scene.hovered_index.map(|index| index as i64).unwrap_or(-1),
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
                    + self.icon_renderer.batch_count()
                    + self.text_renderer.batch_count(),
                scene.scroll_x,
                scene.scroll_y,
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
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    cache_entries: usize,
    cache_bytes: usize,
    resolve_us: u128,
    raster_us: u128,
}

struct IconFrame {
    vertices: Vec<TextVertex>,
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

struct IconFrameBuilder<'a> {
    resolver: &'a mut FileIconResolver,
    raster_cache: &'a mut IconRasterCache,
    surface_size: PhysicalSize<u32>,
    pixels: Vec<u8>,
    draws: Vec<IconDraw>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    icons: usize,
    fallbacks: usize,
    cache_hits: usize,
    cache_misses: usize,
    resolve_us: u128,
    raster_us: u128,
}

impl<'a> IconFrameBuilder<'a> {
    fn new(
        resolver: &'a mut FileIconResolver,
        raster_cache: &'a mut IconRasterCache,
        surface_size: PhysicalSize<u32>,
    ) -> Self {
        Self {
            resolver,
            raster_cache,
            surface_size,
            pixels: vec![0; (ICON_ATLAS_WIDTH * 4) as usize],
            draws: Vec::with_capacity(64),
            width: ICON_ATLAS_WIDTH,
            height: 1,
            cursor_x: ICON_PADDING,
            cursor_y: ICON_PADDING,
            row_height: 0,
            icons: 0,
            fallbacks: 0,
            cache_hits: 0,
            cache_misses: 0,
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
        let snapshot = self.resolver.resolve_entry(directory, entry, icon_size);
        self.resolve_us += resolve_start.elapsed().as_micros();

        let Some(path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        let size_px = icon_cache_size(icon_size);
        let key = IconRasterCacheKey { path, size_px };
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            let raster_start = Instant::now();
            let Some(raster) = rasterize_icon(&key.path, size_px as u32) else {
                self.raster_us += raster_start.elapsed().as_micros();
                self.fallbacks += 1;
                return false;
            };
            self.raster_us += raster_start.elapsed().as_micros();
            self.raster_cache.insert(key, raster)
        };

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
        self.draws.push(IconDraw {
            screen,
            atlas,
            source,
        });
        true
    }

    fn finish(self) -> IconFrame {
        let height = self.height.max(1);
        let mut vertices = Vec::with_capacity(self.draws.len() * 6);
        for draw in &self.draws {
            push_textured_rect(
                &mut vertices,
                draw.screen,
                AtlasRect {
                    x: draw.atlas.x + draw.source.x,
                    y: draw.atlas.y + draw.source.y,
                    width: draw.source.width,
                    height: draw.source.height,
                },
                self.width,
                height,
                self.surface_size,
            );
        }
        let atlas_bytes = (self.width * height * 4) as usize;
        let cache_entries = self.raster_cache.len();
        let cache_bytes = self.raster_cache.bytes();
        IconFrame {
            vertices,
            pixels: self.pixels,
            width: self.width,
            height,
            stats: IconFrameStats {
                icons: self.icons,
                quads: self.draws.len(),
                fallbacks: self.fallbacks,
                atlas_width: self.width,
                atlas_height: height,
                atlas_bytes,
                cache_hits: self.cache_hits,
                cache_misses: self.cache_misses,
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
    resolver: FileIconResolver,
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
            resolver: FileIconResolver::new(),
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LabelCacheKey {
    text: String,
    width: u32,
    height: u32,
    color: TextColor,
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
    ) -> Self {
        text_buffer.set_metrics(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        Self {
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            surface_size,
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
        };
        let label_pixels = if let Some(pixels) = self.label_cache.get(&key) {
            self.cache_hits += 1;
            pixels
        } else {
            self.cache_misses += 1;
            let raster_start = Instant::now();
            let label_pixels = self.rasterize_label(label, label_width, label_height, color);
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
    ) -> Vec<u8> {
        let mut pixels = vec![0; (label_width * label_height * 4) as usize];
        let attrs = Attrs::new().family(Family::SansSerif);
        self.text_buffer
            .set_metrics(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        self.text_buffer.set_wrap(Wrap::WordOrGlyph);
        self.text_buffer
            .set_size(Some(label_width as f32), Some(label_height as f32));
        self.text_buffer
            .set_text(label, &attrs, Shaping::Advanced, Some(Align::Center));
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
    icon_renderer: &mut IconRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> SceneFrame {
    text_renderer.label_cache.begin_frame();
    icon_renderer.raster_cache.begin_frame();

    let (mut scene_frame, text_frame, icon_frame) = {
        let mut text_builder = TextFrameBuilder::new(
            &mut text_renderer.font_system,
            &mut text_renderer.swash_cache,
            &mut text_renderer.text_buffer,
            &mut text_renderer.label_cache,
            size,
        );
        let mut icon_builder = IconFrameBuilder::new(
            &mut icon_renderer.resolver,
            &mut icon_renderer.raster_cache,
            size,
        );
        let scene_frame = scene.build_frame(size, &mut text_builder, &mut icon_builder);
        let text_frame = text_builder.finish();
        let icon_frame = icon_builder.finish();
        (scene_frame, text_frame, icon_frame)
    };

    icon_renderer.upload(device, queue, &icon_frame);
    text_renderer.upload(device, queue, &text_frame);
    scene_frame.icon_stats = icon_frame.stats;
    scene_frame.text_stats = text_frame.stats;
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
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FileIconCacheKey {
    kind: FileIconKind,
    size_px: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedFileIcon {
    path: Option<PathBuf>,
}

#[derive(Debug)]
struct FileIconResolver {
    cached: HashMap<FileIconCacheKey, ResolvedFileIcon>,
    theme: IconThemeResolver,
}

impl FileIconResolver {
    fn new() -> Self {
        Self {
            cached: HashMap::new(),
            theme: IconThemeResolver::default(),
        }
    }

    fn resolve_entry(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> ResolvedFileIcon {
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        if let Some(icon) = self.cached.get(&key) {
            return icon.clone();
        }

        let icon = file_icon_snapshot(
            &key.kind,
            key.size_px,
            &mut self.theme,
            fika_core::MimeDatabase::shared(),
        );
        self.cached.insert(key, icon.clone());
        icon
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

fn file_icon_cache_key(
    path: &Path,
    is_dir: bool,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size: f32,
) -> FileIconCacheKey {
    FileIconCacheKey {
        kind: file_icon_kind(path, is_dir, mime_type, mime_magic_checked),
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
    kind: &FileIconKind,
    desired_size: u16,
    theme: &mut IconThemeResolver,
    mime: &fika_core::MimeDatabase,
) -> ResolvedFileIcon {
    let profile = file_icon_profile(kind, mime);
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

fn rect_from_points(start: ViewPoint, current: ViewPoint) -> ViewRect {
    let x = start.x.min(current.x);
    let y = start.y.min(current.y);
    ViewRect {
        x,
        y,
        width: start.x.max(current.x) - x,
        height: start.y.max(current.y) - y,
    }
}

fn point_distance(left: ViewPoint, right: ViewPoint) -> f32 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}

fn place_row_background_color(active: bool, hovered: bool) -> [f32; 4] {
    match (active, hovered) {
        (true, true) => [0.20, 0.37, 0.58, 0.96],
        (true, false) => [0.16, 0.30, 0.48, 0.90],
        (false, true) => [0.16, 0.18, 0.20, 0.90],
        (false, false) => [0.095, 0.104, 0.114, 0.22],
    }
}

fn place_marker_color(place: &ShellPlace) -> [f32; 4] {
    if place.trash {
        [0.68, 0.36, 0.32, 1.0]
    } else if place.network {
        [0.32, 0.48, 0.72, 1.0]
    } else if place.root {
        [0.45, 0.46, 0.50, 1.0]
    } else if place.editable {
        [0.34, 0.54, 0.42, 1.0]
    } else {
        [0.72, 0.50, 0.22, 1.0]
    }
}

fn item_background_color(selected: bool, hovered: bool) -> [f32; 4] {
    match (selected, hovered) {
        (true, true) => [0.20, 0.37, 0.58, 0.92],
        (true, false) => [0.16, 0.30, 0.49, 0.86],
        (false, true) => [0.19, 0.21, 0.23, 0.72],
        (false, false) => [0.135, 0.145, 0.155, 0.34],
    }
}

fn details_row_background_color(selected: bool, hovered: bool, index: usize) -> [f32; 4] {
    match (selected, hovered, index % 2 == 0) {
        (true, true, _) => [0.20, 0.37, 0.58, 0.92],
        (true, false, _) => [0.16, 0.30, 0.49, 0.86],
        (false, true, _) => [0.18, 0.20, 0.22, 0.82],
        (false, false, true) => [0.105, 0.112, 0.120, 0.55],
        (false, false, false) => [0.086, 0.092, 0.100, 0.55],
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
    match view_mode {
        ShellViewMode::Icons => [0.060, 0.073, 0.087, 1.0],
        ShellViewMode::Compact => [0.056, 0.082, 0.067, 1.0],
        ShellViewMode::Details => [0.076, 0.061, 0.086, 1.0],
    }
}

fn view_mode_content_color(view_mode: ShellViewMode) -> [f32; 4] {
    match view_mode {
        ShellViewMode::Icons => [0.080, 0.096, 0.113, 1.0],
        ShellViewMode::Compact => [0.070, 0.104, 0.083, 1.0],
        ShellViewMode::Details => [0.090, 0.076, 0.104, 1.0],
    }
}

fn view_mode_badge_color(view_mode: ShellViewMode) -> [f32; 4] {
    match view_mode {
        ShellViewMode::Icons => [0.20, 0.38, 0.58, 1.0],
        ShellViewMode::Compact => [0.28, 0.42, 0.30, 1.0],
        ShellViewMode::Details => [0.42, 0.30, 0.52, 1.0],
    }
}

fn view_mode_button_rects(surface_width: f32) -> [(ShellViewMode, ViewRect); 3] {
    let total_width = VIEW_MODE_BUTTON_WIDTH * 3.0 + VIEW_MODE_BUTTON_GAP * 2.0;
    let start_x = (surface_width - total_width - 16.0).max(16.0);
    [
        (
            ShellViewMode::Icons,
            ViewRect {
                x: start_x,
                y: 14.0,
                width: VIEW_MODE_BUTTON_WIDTH,
                height: VIEW_MODE_BUTTON_HEIGHT,
            },
        ),
        (
            ShellViewMode::Compact,
            ViewRect {
                x: start_x + VIEW_MODE_BUTTON_WIDTH + VIEW_MODE_BUTTON_GAP,
                y: 14.0,
                width: VIEW_MODE_BUTTON_WIDTH,
                height: VIEW_MODE_BUTTON_HEIGHT,
            },
        ),
        (
            ShellViewMode::Details,
            ViewRect {
                x: start_x + (VIEW_MODE_BUTTON_WIDTH + VIEW_MODE_BUTTON_GAP) * 2.0,
                y: 14.0,
                width: VIEW_MODE_BUTTON_WIDTH,
                height: VIEW_MODE_BUTTON_HEIGHT,
            },
        ),
    ]
}

fn path_navigation_button_rects() -> [(PathNavigationAction, ViewRect); 5] {
    let x = 16.0;
    [
        (
            PathNavigationAction::Back,
            ViewRect {
                x,
                y: 14.0,
                width: NAV_BUTTON_WIDTH,
                height: NAV_BUTTON_HEIGHT,
            },
        ),
        (
            PathNavigationAction::Forward,
            ViewRect {
                x: x + NAV_BUTTON_WIDTH + NAV_BUTTON_GAP,
                y: 14.0,
                width: NAV_BUTTON_WIDTH,
                height: NAV_BUTTON_HEIGHT,
            },
        ),
        (
            PathNavigationAction::Parent,
            ViewRect {
                x: x + (NAV_BUTTON_WIDTH + NAV_BUTTON_GAP) * 2.0,
                y: 14.0,
                width: NAV_UP_BUTTON_WIDTH,
                height: NAV_BUTTON_HEIGHT,
            },
        ),
        (
            PathNavigationAction::Reload,
            ViewRect {
                x: x + (NAV_BUTTON_WIDTH + NAV_BUTTON_GAP) * 2.0
                    + NAV_UP_BUTTON_WIDTH
                    + NAV_BUTTON_GAP,
                y: 14.0,
                width: NAV_RELOAD_BUTTON_WIDTH,
                height: NAV_BUTTON_HEIGHT,
            },
        ),
        (
            PathNavigationAction::ToggleHidden,
            ViewRect {
                x: x + (NAV_BUTTON_WIDTH + NAV_BUTTON_GAP) * 2.0
                    + NAV_UP_BUTTON_WIDTH
                    + NAV_BUTTON_GAP
                    + NAV_RELOAD_BUTTON_WIDTH
                    + NAV_BUTTON_GAP,
                y: 14.0,
                width: NAV_HIDDEN_BUTTON_WIDTH,
                height: NAV_BUTTON_HEIGHT,
            },
        ),
    ]
}

fn path_bar_start_x() -> f32 {
    let hidden = path_navigation_button_rects()[4].1;
    hidden.right() + 10.0
}

fn push_limited_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths.last().is_some_and(|existing| existing == &path) {
        return;
    }
    paths.push(path);
    if paths.len() > PATH_HISTORY_LIMIT {
        let overflow = paths.len() - PATH_HISTORY_LIMIT;
        paths.drain(0..overflow);
    }
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

fn path_placeholder_width(path: &std::path::Path, surface_width: f32, path_x: f32) -> f32 {
    let display_width = path.display().to_string().chars().count() as f32 * 7.5 + 28.0;
    display_width.min(path_bar_available_width(surface_width, path_x))
}

fn path_bar_available_width(surface_width: f32, path_x: f32) -> f32 {
    let first_button_x = view_mode_button_rects(surface_width)[0].1.x;
    (first_button_x - path_x - 10.0).max(0.0)
}

fn places_sidebar_width(size: PhysicalSize<u32>) -> f32 {
    let width = size.width.max(1) as f32;
    let responsive_width = (width * 0.42).max(128.0);
    PLACES_SIDEBAR_WIDTH
        .min(responsive_width)
        .min((width - 120.0).max(0.0))
}

fn status_bar_rect(size: PhysicalSize<u32>) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let bar_height = STATUS_BAR_HEIGHT.min(height);
    ViewRect {
        x: 0.0,
        y: height - bar_height,
        width,
        height: bar_height,
    }
}

fn context_menu_rect(menu: &ShellContextMenu, size: PhysicalSize<u32>) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let menu_width = CONTEXT_MENU_WIDTH
        .min((width - CONTEXT_MENU_MARGIN * 2.0).max(1.0))
        .max(1.0);
    let menu_height = (context_menu_actions(&menu.target).len() as f32 * CONTEXT_MENU_ROW_HEIGHT)
        .min((height - CONTEXT_MENU_MARGIN * 2.0).max(1.0))
        .max(1.0);
    let max_x = (width - menu_width - CONTEXT_MENU_MARGIN).max(CONTEXT_MENU_MARGIN);
    let max_y = (height - menu_height - CONTEXT_MENU_MARGIN).max(CONTEXT_MENU_MARGIN);
    ViewRect {
        x: menu.position.x.max(CONTEXT_MENU_MARGIN).min(max_x),
        y: menu.position.y.max(CONTEXT_MENU_MARGIN).min(max_y),
        width: menu_width,
        height: menu_height,
    }
}

fn properties_overlay_rect(overlay: &ShellPropertiesOverlay, size: PhysicalSize<u32>) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let overlay_width = PROPERTIES_OVERLAY_WIDTH
        .min((width - PROPERTIES_OVERLAY_MARGIN * 2.0).max(1.0))
        .max(1.0);
    let overlay_height =
        (PROPERTIES_TITLE_HEIGHT + 22.0 + overlay.rows.len() as f32 * PROPERTIES_ROW_HEIGHT)
            .min((height - PROPERTIES_OVERLAY_MARGIN * 2.0).max(1.0))
            .max(1.0);
    ViewRect {
        x: ((width - overlay_width) / 2.0).max(PROPERTIES_OVERLAY_MARGIN),
        y: ((height - overlay_height) / 2.0).max(PROPERTIES_OVERLAY_MARGIN),
        width: overlay_width,
        height: overlay_height,
    }
}

fn create_dialog_rect(_dialog: &ShellCreateDialog, size: PhysicalSize<u32>) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let dialog_width = CREATE_DIALOG_WIDTH
        .min((width - CREATE_DIALOG_MARGIN * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = CREATE_DIALOG_HEIGHT
        .min((height - CREATE_DIALOG_MARGIN * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(CREATE_DIALOG_MARGIN),
        y: ((height - dialog_height) / 2.0).max(CREATE_DIALOG_MARGIN),
        width: dialog_width,
        height: dialog_height,
    }
}

fn create_kind_button_rect(dialog_rect: ViewRect, kind: CreateEntryKind) -> ViewRect {
    let x = match kind {
        CreateEntryKind::Folder => dialog_rect.x + 16.0,
        CreateEntryKind::File => dialog_rect.x + 16.0 + 96.0,
    };
    ViewRect {
        x,
        y: dialog_rect.y + CREATE_DIALOG_TITLE_HEIGHT + 14.0,
        width: 88.0,
        height: CREATE_DIALOG_BUTTON_HEIGHT,
    }
}

fn create_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    ViewRect {
        x: dialog_rect.x + 16.0,
        y: dialog_rect.y + CREATE_DIALOG_TITLE_HEIGHT + 60.0,
        width: (dialog_rect.width - 32.0).max(1.0),
        height: 30.0,
    }
}

fn create_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    let right = dialog_rect.right() - 16.0;
    ViewRect {
        x: right - CREATE_DIALOG_BUTTON_WIDTH * 2.0 - CREATE_DIALOG_BUTTON_GAP,
        y: dialog_rect.bottom() - 16.0 - CREATE_DIALOG_BUTTON_HEIGHT,
        width: CREATE_DIALOG_BUTTON_WIDTH,
        height: CREATE_DIALOG_BUTTON_HEIGHT,
    }
}

fn create_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    let right = dialog_rect.right() - 16.0;
    ViewRect {
        x: right - CREATE_DIALOG_BUTTON_WIDTH,
        y: dialog_rect.bottom() - 16.0 - CREATE_DIALOG_BUTTON_HEIGHT,
        width: CREATE_DIALOG_BUTTON_WIDTH,
        height: CREATE_DIALOG_BUTTON_HEIGHT,
    }
}

fn rename_dialog_rect(_dialog: &ShellRenameDialog, size: PhysicalSize<u32>) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let dialog_width = RENAME_DIALOG_WIDTH
        .min((width - RENAME_DIALOG_MARGIN * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = RENAME_DIALOG_HEIGHT
        .min((height - RENAME_DIALOG_MARGIN * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(RENAME_DIALOG_MARGIN),
        y: ((height - dialog_height) / 2.0).max(RENAME_DIALOG_MARGIN),
        width: dialog_width,
        height: dialog_height,
    }
}

fn rename_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    ViewRect {
        x: dialog_rect.x + 16.0,
        y: dialog_rect.y + RENAME_DIALOG_TITLE_HEIGHT + 18.0,
        width: (dialog_rect.width - 32.0).max(1.0),
        height: 30.0,
    }
}

fn rename_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    let right = dialog_rect.right() - 16.0;
    ViewRect {
        x: right - CREATE_DIALOG_BUTTON_WIDTH * 2.0 - CREATE_DIALOG_BUTTON_GAP,
        y: dialog_rect.bottom() - 16.0 - CREATE_DIALOG_BUTTON_HEIGHT,
        width: CREATE_DIALOG_BUTTON_WIDTH,
        height: CREATE_DIALOG_BUTTON_HEIGHT,
    }
}

fn rename_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    let right = dialog_rect.right() - 16.0;
    ViewRect {
        x: right - CREATE_DIALOG_BUTTON_WIDTH,
        y: dialog_rect.bottom() - 16.0 - CREATE_DIALOG_BUTTON_HEIGHT,
        width: CREATE_DIALOG_BUTTON_WIDTH,
        height: CREATE_DIALOG_BUTTON_HEIGHT,
    }
}

fn property_row(label: &'static str, value: String) -> ShellPropertyRow {
    ShellPropertyRow { label, value }
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.to_string()
}

fn launch_uri_for_path(path: &Path) -> String {
    network_uri_from_path(path).unwrap_or_else(|| gio::File::for_path(path).uri().to_string())
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
    build_shell_places_from(&default_user_places_path())
}

fn build_shell_places_from(user_places_path: &Path) -> Vec<ShellPlace> {
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
    places
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entry(name: &str, is_dir: bool) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
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

    fn test_scene(entries: Vec<Entry>, view_mode: ShellViewMode) -> ShellScene {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(&entries, false, "");
        ShellScene {
            path: PathBuf::from("/tmp"),
            view_mode,
            entries,
            dir_count,
            places: vec![
                ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
                ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ],
            filtered_indexes,
            location_draft: None,
            filter_active: false,
            filter_pattern: String::new(),
            show_hidden: false,
            zoom_step: 0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            places_scroll_y: 0.0,
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
            rubber_band: None,
            hit_tests: 0,
            selection_changes: 0,
            context_target_changes: 0,
            context_menu_actions: 0,
            properties_changes: 0,
            create_changes: 0,
            rename_changes: 0,
            open_changes: 0,
            copy_location_changes: 0,
            file_clipboard_changes: 0,
            paste_changes: 0,
            trash_changes: 0,
            places_changes: 0,
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
        assert_eq!(scene.scroll_y, 0.0);
        assert_eq!(scene.places_scroll_changes, 1);

        scene.pointer = Some(ViewPoint {
            x: scene.content_origin_x(size) + 10.0,
            y: scene.content_origin_y() + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert!(scene.scroll_y > 0.0);
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
        let point = ViewPoint {
            x: PLACES_SIDEBAR_PADDING_X + 6.0,
            y: TOP_BAR_HEIGHT + PLACES_SIDEBAR_TOP_PADDING + 6.0,
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
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT + 8.0,
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
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::RemovePlace,
                ShellContextMenuAction::Properties,
            ]
        );
        let rect = context_menu_rect(menu, size);
        let remove_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 2.0 + 8.0,
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

        let blank_target = ShellContextTarget::Blank {
            path: PathBuf::from("/tmp"),
        };
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::AddToPlaces));
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
            network: false,
            trash: true,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&trash_place),
            &[
                ShellContextMenuAction::Open,
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
        let size = PhysicalSize::new(420, 260);
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
        let point = ViewPoint {
            x: size.width as f32 - 4.0,
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
            x: 10.0,
            y: status_bar_rect(size).y + 2.0,
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
        let point = ViewPoint {
            x: size.width as f32 - 2.0,
            y: status_bar_rect(size).y - 2.0,
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
        assert!(rect.x >= CONTEXT_MENU_MARGIN);
        assert!(rect.y >= CONTEXT_MENU_MARGIN);
        assert!(rect.right() <= size.width as f32 - CONTEXT_MENU_MARGIN + f32::EPSILON);
        assert!(rect.bottom() <= size.height as f32 - CONTEXT_MENU_MARGIN + f32::EPSILON);

        assert_eq!(
            scene.activate_or_close_context_menu(
                ViewPoint {
                    x: CONTEXT_MENU_MARGIN,
                    y: CONTEXT_MENU_MARGIN,
                },
                size,
            ),
            None
        );
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.context_menu_actions, 0);
    }

    #[test]
    fn context_menu_blank_actions_can_hit_select_all_and_refresh() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let point = ViewPoint {
            x: size.width as f32 - 4.0,
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
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 4.0 + 8.0,
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
        assert!(entry_index_by_name(&scene.entries, "source.txt").is_some());

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
        assert!(entry_index_by_name(&scene.entries, "Pasted Text.txt").is_some());

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

        let index = entry_index_by_name(&scene.entries, "made").unwrap();
        assert!(scene.entries[index].is_dir);
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
        let old_index = entry_index_by_name(&scene.entries, "old.txt").unwrap();
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

        let new_index = entry_index_by_name(&scene.entries, "new.txt").unwrap();
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
        let remove_index = entry_index_by_name(&scene.entries, "remove.txt").unwrap();
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
        assert!(entry_index_by_name(&scene.entries, "remove.txt").is_none());
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
            y: item.visual_rect.y + TOP_BAR_HEIGHT + 4.0,
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
        scene.scroll_x = 128.0;
        scene.scroll_y = 64.0;
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

        assert_eq!(scene.path, child);
        assert_eq!(scene.entries.len(), 1);
        assert_eq!(scene.entries[0].name.as_ref(), "nested.txt");
        assert_eq!(scene.scroll_x, 0.0);
        assert_eq!(scene.scroll_y, 0.0);
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
        assert_eq!(scene.path, first);
        assert_eq!(scene.history.back, vec![root.clone()]);
        assert!(scene.history.forward.is_empty());

        assert!(scene.load_path(second.clone(), size).unwrap());
        assert_eq!(scene.path, second);
        assert_eq!(scene.history.back, vec![root.clone(), first.clone()]);

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.path, first);
        assert_eq!(scene.history.back, vec![root.clone()]);
        assert_eq!(scene.history.forward, vec![second.clone()]);

        assert!(scene.go_history_forward(size).unwrap());
        assert_eq!(scene.path, second);
        assert_eq!(scene.history.back, vec![root.clone(), first.clone()]);
        assert!(scene.history.forward.is_empty());

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.path, first);
        assert!(scene.load_path(sibling.clone(), size).unwrap());
        assert_eq!(scene.path, sibling);
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
        let keep_index = entry_index_by_name(&scene.entries, "keep.txt").unwrap();
        assert!(scene.selection.apply_navigation(keep_index, false));
        scene.history.push_back(PathBuf::from("/tmp/previous"));
        scene.history.push_forward(PathBuf::from("/tmp/next"));

        fs::write(root.join("aaa.txt"), b"new").unwrap();
        assert!(scene.reload_current_path(size).unwrap());

        let new_keep_index = entry_index_by_name(&scene.entries, "keep.txt").unwrap();
        assert!(scene.selection.contains(new_keep_index));
        assert_eq!(scene.selection.len(), 1);
        assert_eq!(scene.selection.focus, Some(new_keep_index));
        assert_eq!(scene.history.back, vec![PathBuf::from("/tmp/previous")]);
        assert_eq!(scene.history.forward, vec![PathBuf::from("/tmp/next")]);
        assert_eq!(scene.path, root);
        assert_eq!(scene.path_changes, 0);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(scene.path).unwrap();
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
        scene.path = temp.clone();
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
    fn location_editing_expands_path_bar_hit_target() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.path = PathBuf::from("/x");
        let size = PhysicalSize::new(900, 360);

        let inactive = scene
            .path_bar_rect(size)
            .expect("inactive path bar should be visible");
        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let active = scene
            .path_bar_rect(size)
            .expect("active path bar should be visible");

        assert!(active.width > inactive.width);
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
        let size = PhysicalSize::new(420, 260);

        assert!(scene.apply_filter_command(FilterCommand::Activate, size));
        assert!(scene.apply_filter_command(FilterCommand::Insert("alp".to_string()), size));
        assert_eq!(scene.filtered_indexes, vec![0, 2]);
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

        assert_eq!(scene.filtered_indexes, vec![0, 2]);
        assert_eq!(scene.filtered_entry_count(), 2);
        assert!(scene.selection.apply_navigation(1, false));

        assert!(scene.toggle_hidden_visibility(size));
        assert!(scene.show_hidden);
        assert_eq!(scene.filtered_indexes, vec![0, 1, 2]);
        assert_eq!(scene.hidden_changes, 1);
        assert!(scene.selection.contains(1));

        assert!(scene.toggle_hidden_visibility(size));
        assert!(!scene.show_hidden);
        assert_eq!(scene.filtered_indexes, vec![0, 2]);
        assert_eq!(scene.selection.len(), 0);
        assert_eq!(scene.selection_changes, 1);
    }

    #[test]
    fn top_bar_view_mode_buttons_are_hit_tested() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 240);
        for (mode, rect) in view_mode_button_rects(size.width as f32) {
            assert_eq!(
                scene.view_mode_at_screen_point(
                    ViewPoint {
                        x: rect.x + 2.0,
                        y: rect.y + 2.0
                    },
                    size
                ),
                Some(mode)
            );
        }
        assert_eq!(
            scene.view_mode_at_screen_point(ViewPoint { x: 8.0, y: 8.0 }, size),
            None
        );
    }

    #[test]
    fn top_bar_path_navigation_buttons_respect_enabled_state() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 240);
        let back_rect = path_navigation_button_rects()[0].1;
        let forward_rect = path_navigation_button_rects()[1].1;
        let parent_rect = path_navigation_button_rects()[2].1;
        let reload_rect = path_navigation_button_rects()[3].1;
        let hidden_rect = path_navigation_button_rects()[4].1;

        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: back_rect.x + 2.0,
                    y: back_rect.y + 2.0
                },
                size
            ),
            None
        );
        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: parent_rect.x + 2.0,
                    y: parent_rect.y + 2.0
                },
                size
            ),
            Some(PathNavigationAction::Parent)
        );
        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: reload_rect.x + 2.0,
                    y: reload_rect.y + 2.0
                },
                size
            ),
            Some(PathNavigationAction::Reload)
        );
        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: hidden_rect.x + 2.0,
                    y: hidden_rect.y + 2.0
                },
                size
            ),
            Some(PathNavigationAction::ToggleHidden)
        );

        scene.history.push_back(PathBuf::from("/tmp/previous"));
        scene.history.push_forward(PathBuf::from("/tmp/next"));
        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: back_rect.x + 2.0,
                    y: back_rect.y + 2.0
                },
                size
            ),
            Some(PathNavigationAction::Back)
        );
        assert_eq!(
            scene.path_navigation_action_at_screen_point(
                ViewPoint {
                    x: forward_rect.x + 2.0,
                    y: forward_rect.y + 2.0
                },
                size
            ),
            Some(PathNavigationAction::Forward)
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
        scene.scroll_x = 10_000.0;
        scene.scroll_y = 500.0;
        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            ShellSelection::default(),
        ));

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(scene.view_mode, ShellViewMode::Details);
        assert_eq!(scene.scroll_x, 0.0);
        assert!(scene.scroll_y >= 0.0);
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
    fn shell_hit_test_uses_content_coordinates_below_top_bar() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let layout = scene.layout(size);
        let item = layout.item(0).expect("test item should layout");

        let visual_point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 1.0,
            y: item.visual_rect.y + TOP_BAR_HEIGHT + 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(visual_point, size), Some(0));

        let top_bar_point = ViewPoint {
            x: scene.content_origin_x(size) + item.item_rect.x + 1.0,
            y: TOP_BAR_HEIGHT - 1.0,
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
        let status_bar = status_bar_rect(size);

        assert_eq!(scene.viewport_height(size), 160.0);
        assert_eq!(status_bar.y, 212.0);
        assert_eq!(
            scene.hit_test_screen_point(
                ViewPoint {
                    x: 16.0,
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
                    x: 16.0,
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
        let layout = ShellLayout::Compact(CompactLayout::new(
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
                item_width: COMPACT_ITEM_WIDTH,
                item_height: COMPACT_ITEM_HEIGHT,
                icon_size: COMPACT_ICON_SIZE,
                text_height: 18.0,
            },
        ));
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
            y: TOP_BAR_HEIGHT + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(header_point, size), None);

        let row_point = ViewPoint {
            x: scene.content_origin_x(size) + 12.0,
            y: TOP_BAR_HEIGHT + DETAILS_HEADER_HEIGHT + 4.0,
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
            y: item.visual_rect.bottom() - scene.scroll_y + scene.content_origin_y() - 1.0,
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
