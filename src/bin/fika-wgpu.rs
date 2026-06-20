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
    CompactLayout, CompactLayoutOptions, Entry, IconsLayout, IconsLayoutOptions, ViewPoint,
    ViewRect, ViewSize, format_modified_secs, format_size, read_entries_sync,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const TOP_BAR_HEIGHT: f32 = 52.0;
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
const NAV_BUTTON_HEIGHT: f32 = 24.0;
const NAV_BUTTON_GAP: f32 = 6.0;
const PATH_HISTORY_LIMIT: usize = 128;
const ZOOM_STEP_MIN: i32 = -3;
const ZOOM_STEP_MAX: i32 = 4;
const ZOOM_STEP_SCALE: f32 = 0.12;
const AUTO_CYCLE_INTERVAL: Duration = Duration::from_secs(1);
const DOUBLE_CLICK_MAX_INTERVAL: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_MAX_DISTANCE: f32 = 6.0;

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
}

impl PathNavigationAction {
    fn label(self) -> &'static str {
        match self {
            Self::Back => "<",
            Self::Forward => ">",
            Self::Parent => "Up",
            Self::Reload => "Reload",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Back => "history-back",
            Self::Forward => "history-forward",
            Self::Parent => "parent-directory",
            Self::Reload => "reload-directory",
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

fn window_title(scene: &ShellScene) -> String {
    format!(
        "Fika wgpu shell [{}] - {}",
        scene.view_mode.as_str(),
        scene.path.display()
    )
}

struct FikaWgpuApp {
    scene: ShellScene,
    modifiers: Modifiers,
    // Drop order matters: the surface borrows the window handle, so renderer
    // must be dropped before the window.
    renderer: Option<WgpuState>,
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

        eprintln!(
            "[fika-wgpu] shell-ready size={}x{} scale={:.2}",
            renderer.size.width,
            renderer.size.height,
            window.scale_factor()
        );

        self.scene.clamp_scroll(renderer.size);
        self.renderer = Some(renderer);
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
                if button.mouse_button() != Some(MouseButton::Left) {
                    return;
                }
                let Some(renderer) = self.renderer.as_ref() else {
                    return;
                };
                let point = ViewPoint {
                    x: position.x as f32,
                    y: position.y as f32,
                };
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
            Ok(false) => {}
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

struct ShellScene {
    path: PathBuf,
    view_mode: ShellViewMode,
    entries: Vec<Entry>,
    dir_count: usize,
    zoom_step: i32,
    scroll_x: f32,
    scroll_y: f32,
    pointer: Option<ViewPoint>,
    hovered_index: Option<usize>,
    last_primary_click: Option<PrimaryClick>,
    history: PathHistory,
    selection: ShellSelection,
    rubber_band: Option<RubberBand>,
    hit_tests: u64,
    selection_changes: u64,
    keyboard_navigation: u64,
    rubber_band_updates: u64,
    view_switches: u64,
    path_changes: u64,
    directory_reloads: u64,
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

        Ok(Self {
            path,
            view_mode,
            entries,
            dir_count,
            zoom_step: 0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            pointer: None,
            hovered_index: None,
            last_primary_click: None,
            history: PathHistory::default(),
            selection: ShellSelection::default(),
            rubber_band: None,
            hit_tests: 0,
            selection_changes: 0,
            keyboard_navigation: 0,
            rubber_band_updates: 0,
            view_switches: 0,
            path_changes: 0,
            directory_reloads: 0,
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

        let previous_selection = self.selection.clone();
        let remapped_selection = self.selection_for_reloaded_entries(&entries);
        let selection_changed = previous_selection != remapped_selection;

        self.entries = entries;
        self.dir_count = dir_count;
        self.selection = remapped_selection;
        self.rubber_band = None;
        self.last_primary_click = None;
        self.directory_reloads += 1;
        if selection_changed {
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
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.selection = ShellSelection::default();
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
            SelectionCommand::SelectAll => self.selection.select_all(self.entries.len()),
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
        }
    }

    fn selected_directory_path(&self) -> Option<PathBuf> {
        self.selection
            .focus_or_first_selected()
            .and_then(|index| self.directory_path_for_index(index))
    }

    fn parent_directory_path(&self) -> Option<PathBuf> {
        self.path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }

    fn directory_path_for_index(&self, index: usize) -> Option<PathBuf> {
        let entry = self.entries.get(index)?;
        entry.is_dir.then(|| {
            entry
                .target_path
                .clone()
                .unwrap_or_else(|| self.path.join(entry.name.as_ref()))
        })
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
        match self.view_mode {
            ShellViewMode::Icons => ShellLayout::Icons(IconsLayout::new(
                self.entries.len(),
                self.icons_options(size),
            )),
            ShellViewMode::Compact => ShellLayout::Compact(CompactLayout::new(
                self.entries.len(),
                self.compact_options(size),
            )),
            ShellViewMode::Details => ShellLayout::Details(DetailsLayout::new(
                self.entries.len(),
                size.width as f32,
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
            viewport_width: size.width as f32,
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
            viewport_width: size.width as f32,
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
        let content_origin = self.content_origin_y();
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
        let path_x = path_bar_start_x();
        let path_rect = ViewRect {
            x: path_x,
            y: 14.0,
            width: path_placeholder_width(&self.path, width, path_x),
            height: 24.0,
        };
        if path_rect.width > 24.0 {
            push_rect(&mut vertices, path_rect, [0.170, 0.184, 0.198, 1.0], size);
            text.push_label(
                &self.path.display().to_string(),
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
        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: TOP_BAR_HEIGHT,
                width,
                height: (height - TOP_BAR_HEIGHT).max(1.0),
            },
            view_mode_content_color(self.view_mode),
            size,
        );
        push_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
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
                x: 0.0,
                y: content_origin,
                width,
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
            x: 0.0,
            y: content_origin,
            width,
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
            push_rect(
                vertices,
                rect,
                if enabled {
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
                if enabled {
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
        let width = size.width.max(1) as f32;
        let header = ViewRect {
            x: 0.0,
            y: TOP_BAR_HEIGHT,
            width,
            height: DETAILS_HEADER_HEIGHT,
        };
        push_rect(vertices, header, [0.100, 0.108, 0.117, 1.0], size);
        push_rect(
            vertices,
            ViewRect {
                x: 0.0,
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
                    x,
                    y: TOP_BAR_HEIGHT + 6.0,
                    width: width.max(1.0),
                    height: 18.0,
                },
                header,
                TextColor::rgb(170, 181, 192),
            );
        }
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
        let Some(entry) = self.entries.get(item.model_index) else {
            return;
        };
        let item_rect = self.content_to_screen(item.item_rect);
        let visual_rect = self.content_to_screen(item.visual_rect);
        let icon_rect = self.content_to_screen(item.icon_rect);
        let text_rect = self.content_to_screen(item.text_rect);
        let selected = self.selection.contains(item.model_index);
        let hovered = self.hovered_index == Some(item.model_index);

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
        let Some(entry) = self.entries.get(item.model_index) else {
            return;
        };
        let row_rect = self.content_to_screen(item.item_rect);
        let icon_rect = self.content_to_screen(item.icon_rect);
        let name_rect = self.content_to_screen(item.text_rect);
        let selected = self.selection.contains(item.model_index);
        let hovered = self.hovered_index == Some(item.model_index);

        push_clipped_rect(
            vertices,
            row_rect,
            content_clip,
            details_row_background_color(selected, hovered, item.model_index),
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
                x: DETAILS_NAME_WIDTH + 8.0 - self.scroll_x,
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
                x: DETAILS_NAME_WIDTH + DETAILS_SIZE_WIDTH + 8.0 - self.scroll_x,
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
        let rect = self.content_to_screen(rect);
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

        let status = format!(
            "{} entries ({} dirs, {} files) | {} selected | {} visible | {} | {}%",
            self.entries.len(),
            self.dir_count,
            self.entries.len().saturating_sub(self.dir_count),
            self.selection.len(),
            visible_items,
            self.view_mode.label(),
            self.zoom_percent()
        );
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

    fn content_to_screen(&self, rect: ViewRect) -> ViewRect {
        ViewRect {
            x: rect.x - self.scroll_x,
            y: rect.y - self.scroll_y + self.content_origin_y(),
            width: rect.width,
            height: rect.height,
        }
    }

    fn content_origin_y(&self) -> f32 {
        TOP_BAR_HEIGHT
            + if self.view_mode == ShellViewMode::Details {
                DETAILS_HEADER_HEIGHT
            } else {
                0.0
            }
    }

    fn viewport_height(&self, size: PhysicalSize<u32>) -> f32 {
        (size.height as f32 - self.content_origin_y() - STATUS_BAR_HEIGHT).max(1.0)
    }

    fn content_screen_rect(&self, size: PhysicalSize<u32>) -> ViewRect {
        ViewRect {
            x: 0.0,
            y: self.content_origin_y(),
            width: size.width.max(1) as f32,
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
        self.refresh_hover(size);
    }

    fn scroll_by(&mut self, delta_y: f32, size: PhysicalSize<u32>) -> bool {
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

    fn max_scroll_x(&self, size: PhysicalSize<u32>) -> f32 {
        let layout = self.layout(size);
        (layout.content_size().width - size.width as f32).max(0.0)
    }

    fn max_scroll_y(&self, size: PhysicalSize<u32>) -> f32 {
        let layout = self.layout(size);
        (layout.content_size().height - self.viewport_height(size)).max(0.0)
    }

    fn set_pointer(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        if self.rubber_band.is_some() {
            return self.update_rubber_band(point, size);
        }
        self.refresh_hover(size)
    }

    fn clear_pointer(&mut self) -> bool {
        self.pointer = None;
        let changed = self.hovered_index.take().is_some();
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

        let Some(start) =
            screen_to_content_point(click.point, self.scroll_offset(), self.content_origin_y())
        else {
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
            .copied()
            .filter(|index| {
                layout
                    .item(*index)
                    .is_some_and(|item| item.visual_rect.intersects(rect))
            })
            .collect()
    }

    fn navigate(
        &mut self,
        action: NavigationAction,
        extend: bool,
        size: PhysicalSize<u32>,
    ) -> bool {
        if self.entries.is_empty() {
            return false;
        }

        let old_scroll_y = self.scroll_y;
        let old_hovered = self.hovered_index;
        let current = self.selection.focus_or_first_selected().unwrap_or(0);
        let layout = self.layout(size);
        let Some(target) = navigation_target(action, current, self.entries.len(), &layout) else {
            return false;
        };

        let selection_changed = self.selection.apply_navigation(target, extend);
        if selection_changed {
            self.selection_changes += 1;
        }
        self.keyboard_navigation += 1;
        self.ensure_index_visible(target, size);
        self.hovered_index = self
            .pointer
            .and_then(|point| self.hit_test_screen_point(point, size));

        selection_changed
            || (self.scroll_y - old_scroll_y).abs() > f32::EPSILON
            || self.hovered_index != old_hovered
    }

    fn refresh_hover(&mut self, size: PhysicalSize<u32>) -> bool {
        let hit = self
            .pointer
            .and_then(|point| self.hit_test_screen_point(point, size));
        self.set_hovered_index(hit)
    }

    fn set_hovered_index(&mut self, hovered_index: Option<usize>) -> bool {
        self.hit_tests += 1;
        let changed = self.hovered_index != hovered_index;
        self.hovered_index = hovered_index;
        changed
    }

    fn hit_test_screen_point(&self, point: ViewPoint, size: PhysicalSize<u32>) -> Option<usize> {
        if !self.content_screen_rect(size).contains(point) {
            return None;
        }
        let content_point =
            screen_to_content_point(point, self.scroll_offset(), self.content_origin_y())?;
        let layout = self.layout(size);
        let index = layout.hit_test_content_point(content_point)?;
        let item = layout.item(index)?;
        item.visual_rect.contains(content_point).then_some(index)
    }

    fn ensure_index_visible(&mut self, index: usize, size: PhysicalSize<u32>) {
        let layout = self.layout(size);
        let Some(item) = layout.item(index) else {
            return;
        };
        let viewport_h = self.viewport_height(size);
        let padding = 8.0;
        match self.view_mode {
            ShellViewMode::Compact => {
                if item.visual_rect.x < self.scroll_x + padding {
                    self.scroll_x = (item.visual_rect.x - padding).max(0.0);
                } else if item.visual_rect.right() > self.scroll_x + size.width as f32 - padding {
                    self.scroll_x = item.visual_rect.right() - size.width as f32 + padding;
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

    fn select_all(&mut self, item_count: usize) -> bool {
        let old_selected = self.selected.clone();
        let old_anchor = self.anchor;
        let old_focus = self.focus;

        self.selected = (0..item_count).collect();
        self.anchor = (item_count > 0).then_some(0);
        self.focus = item_count.checked_sub(1);

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
                "[fika-wgpu] frame={} reason={} view={} zoom={} zoom_changes={} path={} entries={} visible={} selected={} hover={} rubber_band={} hit_tests={} selection_changes={} keyboard_nav={} rubber_band_updates={} view_switches={} path_changes={} reloads={} quads={} layout_content={:.1}x{:.1} first_item={:.1},{:.1},{:.1},{:.1} icons={} icon_quads={} icon_fallbacks={} icon_cache={}/{} entries={} bytes={} icon_atlas={}x{}:{}b icon_resolve={}us icon_raster={}us text_labels={} text_quads={} text_cache={}/{} entries={} bytes={} batches={} scroll_x={:.1} scroll_y={:.1} layout={}us text_raster={}us text_atlas={}x{}:{}b render={}us",
                self.frame_count,
                reason,
                scene.view_mode.as_str(),
                scene.zoom_percent(),
                scene.zoom_changes,
                scene.path.display(),
                scene.entries.len(),
                scene_frame.visible_items,
                scene.selection.len(),
                scene.hovered_index.map(|index| index as i64).unwrap_or(-1),
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
    content_origin_y: f32,
) -> Option<ViewPoint> {
    if point.y < content_origin_y {
        return None;
    }
    Some(ViewPoint {
        x: point.x + scroll_offset.x,
        y: point.y - content_origin_y + scroll_offset.y,
    })
}

fn clamped_screen_to_content_point(
    point: ViewPoint,
    scroll_offset: ViewPoint,
    content_rect: ViewRect,
) -> ViewPoint {
    let y = point.y.clamp(content_rect.y, content_rect.bottom());
    ViewPoint {
        x: point.x.clamp(content_rect.x, content_rect.right()) + scroll_offset.x,
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

fn path_navigation_button_rects() -> [(PathNavigationAction, ViewRect); 4] {
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
    ]
}

fn path_bar_start_x() -> f32 {
    let reload = path_navigation_button_rects()[3].1;
    reload.right() + 10.0
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
    let first_button_x = view_mode_button_rects(surface_width)[0].1.x;
    let max_width = (first_button_x - path_x - 10.0).max(0.0);
    display_width.min(max_width)
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

fn entry_index_by_name(entries: &[Entry], name: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.name.as_ref() == name)
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

    fn test_scene(entries: Vec<Entry>, view_mode: ShellViewMode) -> ShellScene {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        ShellScene {
            path: PathBuf::from("/tmp"),
            view_mode,
            entries,
            dir_count,
            zoom_step: 0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            pointer: None,
            hovered_index: None,
            last_primary_click: None,
            history: PathHistory::default(),
            selection: ShellSelection::default(),
            rubber_band: None,
            hit_tests: 0,
            selection_changes: 0,
            keyboard_navigation: 0,
            rubber_band_updates: 0,
            view_switches: 0,
            path_changes: 0,
            directory_reloads: 0,
            zoom_changes: 0,
        }
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
            x: item.visual_rect.x + 4.0,
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
        assert_eq!(
            screen_to_content_point(ViewPoint { x: 12.0, y: 10.0 }, offset, TOP_BAR_HEIGHT),
            None
        );
        assert_eq!(
            screen_to_content_point(
                ViewPoint {
                    x: 12.0,
                    y: TOP_BAR_HEIGHT + 5.0
                },
                offset,
                TOP_BAR_HEIGHT
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
            x: item.visual_rect.x + 1.0,
            y: item.visual_rect.y + TOP_BAR_HEIGHT + 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(visual_point, size), Some(0));

        let top_bar_point = ViewPoint {
            x: item.item_rect.x + 1.0,
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
            x: 12.0,
            y: TOP_BAR_HEIGHT + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(header_point, size), None);

        let row_point = ViewPoint {
            x: 12.0,
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
            x: 0.0,
            y: TOP_BAR_HEIGHT + 1.0,
        };
        let current = ViewPoint {
            x: item.visual_rect.right() - 1.0,
            y: item.visual_rect.bottom() - scene.scroll_y + TOP_BAR_HEIGHT - 1.0,
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
