use std::error::Error;
use std::num::NonZeroU32;

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::CompositorState,
    globals::GlobalData,
    output::OutputState,
    reexports::protocols::wp::{
        fractional_scale::v1::client::{
            wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
            wp_fractional_scale_v1::WpFractionalScaleV1,
        },
        viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
    },
    registry::RegistryState,
    seat::{
        SeatState,
        keyboard::{KeyEvent, Keysym, Modifiers},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell, XdgSurface,
            window::{Window, WindowConfigure, WindowDecorations},
        },
    },
};
use wayland_client::Connection;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_surface};

use super::options::StartupOptions;
use super::pane::PaneSelectionMove;
use super::renderer::{WgpuRenderer, surface_extent};
use super::scene::{SceneCommand, SctkScene};

const DEFAULT_WIDTH: u32 = 1100;
const DEFAULT_HEIGHT: u32 = 720;

pub(crate) fn run(options: StartupOptions) -> Result<(), Box<dyn Error>> {
    let scene = SctkScene::load(options.path, options.view_mode, options.split_path)?;
    scene.log_startup();

    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)?;
    let xdg_shell = XdgShell::bind(&globals, &qh)?;

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::ServerDefault, &qh);
    window.set_title("Fika SCTK");
    window.set_app_id("io.github.elel-code.fika.sctk");
    window.set_min_size(Some((360, 240)));
    window.commit();

    let exact_fractional_viewport =
        std::env::var_os("FIKA_SCTK_EXACT_FRACTIONAL_VIEWPORT").is_some();
    let viewporter: Option<WpViewporter> = exact_fractional_viewport
        .then(|| globals.bind(&qh, 1..=1, GlobalData).ok())
        .flatten();
    let viewport = viewporter
        .as_ref()
        .map(|viewporter| viewporter.get_viewport(window.wl_surface(), &qh, GlobalData));
    let fractional_scale_manager: Option<WpFractionalScaleManagerV1> =
        globals.bind(&qh, 1..=1, GlobalData).ok();
    let fractional_scale = fractional_scale_manager
        .as_ref()
        .zip(viewport.as_ref())
        .map(|(manager, _)| {
            manager.get_fractional_scale(
                window.wl_surface(),
                &qh,
                FractionalScaleData {
                    surface: window.wl_surface().clone(),
                },
            )
        });

    let renderer = WgpuRenderer::new(&conn, &window)?;
    let mut app = FikaSctkApp {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        pointer: None,
        keyboard: None,
        modifiers: Modifiers::default(),
        keyboard_focus: false,
        exit: false,
        ready_logged: false,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        scale_factor: 1.0,
        viewporter,
        viewport,
        fractional_scale_manager,
        fractional_scale,
        // Drop order matters: the wgpu surface must be destroyed before the
        // underlying Wayland window.
        renderer,
        window,
        scene,
    };

    let mut event_loop = EventLoop::<FikaSctkApp>::try_new()?;
    WaylandSource::new(conn.clone(), event_queue).insert(event_loop.handle())?;
    while !app.exit {
        event_loop.dispatch(None, &mut app)?;
    }

    Ok(())
}

pub(crate) struct FikaSctkApp {
    pub(crate) registry_state: RegistryState,
    pub(crate) seat_state: SeatState,
    pub(crate) output_state: OutputState,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) modifiers: Modifiers,
    pub(crate) keyboard_focus: bool,
    pub(crate) exit: bool,
    ready_logged: bool,
    width: u32,
    height: u32,
    scale_factor: f32,
    viewporter: Option<WpViewporter>,
    pub(crate) viewport: Option<WpViewport>,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub(crate) fractional_scale: Option<WpFractionalScaleV1>,
    renderer: WgpuRenderer,
    pub(crate) window: Window,
    scene: SctkScene,
}

#[derive(Clone)]
pub(crate) struct FractionalScaleData {
    pub(crate) surface: wl_surface::WlSurface,
}

impl FikaSctkApp {
    pub(crate) fn request_exit(&mut self) {
        self.exit = true;
    }

    pub(crate) fn handle_configure(&mut self, configure: WindowConfigure) {
        let (width, height) = configure.new_size;
        self.width = width.map(NonZeroU32::get).unwrap_or(DEFAULT_WIDTH);
        self.height = height.map(NonZeroU32::get).unwrap_or(DEFAULT_HEIGHT);
        self.apply_surface_state();
        let config = self
            .renderer
            .configure_surface(self.width, self.height, self.ui_scale());
        if !self.ready_logged {
            eprintln!(
                "[fika-sctk] shell-ready size={}x{} scale={:.2} scale_mode={} viewporter={} fractional_protocol={} surface={}x{} format={:?} path={} view={} entries={} dirs={} files={} split_pane={}",
                self.width,
                self.height,
                self.ui_scale(),
                self.scale_mode(),
                self.viewporter.is_some() as u8,
                self.fractional_scale_manager.is_some() as u8,
                config.width,
                config.height,
                config.format,
                self.scene.path().display(),
                self.scene.view_mode().as_str(),
                self.scene.entry_count(),
                self.scene.dir_count(),
                self.scene.file_count(),
                self.scene.split_enabled() as u8
            );
            self.ready_logged = true;
        } else {
            eprintln!(
                "[fika-sctk] configure size={}x{} scale={:.2} scale_mode={} surface={}x{}",
                self.width,
                self.height,
                self.ui_scale(),
                self.scale_mode(),
                config.width,
                config.height
            );
        }
        self.render_scene("configure");
    }

    pub(crate) fn set_legacy_scale_factor(&mut self, scale_factor: i32) {
        if self.fractional_scale.is_some() {
            return;
        }
        self.set_scale_factor(scale_factor.max(1) as f32, "legacy-scale");
    }

    pub(crate) fn set_fractional_scale(&mut self, scale_factor: f32) {
        let preferred_scale = scale_factor.max(1.0);
        let render_scale = if self.viewport.is_some() {
            preferred_scale
        } else {
            preferred_scale.ceil()
        };
        eprintln!(
            "[fika-sctk] fractional-scale={preferred_scale:.3} render-scale={render_scale:.3}"
        );
        self.set_scale_factor(render_scale, "render-scale");
    }

    fn set_scale_factor(&mut self, scale_factor: f32, reason: &str) {
        let scale_factor = (scale_factor.max(1.0) * 120.0).round() / 120.0;
        if (self.scale_factor - scale_factor).abs() < f32::EPSILON {
            return;
        }
        self.scale_factor = scale_factor;
        eprintln!("[fika-sctk] {reason}={scale_factor:.3}");
        if self.ready_logged {
            self.apply_surface_state();
            let config = self
                .renderer
                .configure_surface(self.width, self.height, self.ui_scale());
            eprintln!(
                "[fika-sctk] reconfigure scale={:.2} scale_mode={} surface={}x{}",
                self.ui_scale(),
                self.scale_mode(),
                config.width,
                config.height
            );
            self.render_scene("scale-factor");
        }
    }

    pub(crate) fn render_scene(&mut self, reason: &str) {
        self.apply_surface_state();
        let frame = self
            .scene
            .render_frame(self.width, self.height, self.ui_scale());
        self.renderer.render_scene_frame(frame, reason);
    }

    pub(crate) fn set_pointer(&mut self, x: f64, y: f64) {
        if self
            .scene
            .set_pointer(crate::fika_sctk::quad::point(x, y), self.width, self.height)
        {
            self.render_scene("pointer-hover");
        }
    }

    pub(crate) fn clear_pointer(&mut self) {
        if self.scene.clear_pointer() {
            self.render_scene("pointer-leave");
        }
    }

    pub(crate) fn set_keyboard_focus(&mut self, focused: bool) {
        self.keyboard_focus = focused;
    }

    pub(crate) fn update_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    pub(crate) fn press_key(&mut self, event: KeyEvent) {
        self.handle_key_event(event, "key-press");
    }

    pub(crate) fn repeat_key(&mut self, event: KeyEvent) {
        self.handle_key_event(event, "key-repeat");
    }

    fn handle_key_event(&mut self, event: KeyEvent, reason: &str) {
        if !self.keyboard_focus {
            return;
        }
        let Some(command) = key_command(event.keysym, self.modifiers) else {
            return;
        };
        match self.scene.handle_command(command, self.width, self.height) {
            Ok(true) => {
                eprintln!(
                    "[fika-sctk] command={command:?} reason={reason} active_pane={} path={} view={} show_hidden={} visible={}",
                    self.scene.active_pane_name(),
                    self.scene.active_path().display(),
                    self.scene.active_view_mode().as_str(),
                    self.scene.active_show_hidden() as u8,
                    self.scene.active_visible_entry_count()
                );
                self.render_scene(reason);
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!(
                    "[fika-sctk] command-error command={command:?} reason={reason} error={error}"
                );
            }
        }
    }

    pub(crate) fn press_primary(&mut self, x: f64, y: f64) {
        if self
            .scene
            .press_primary(crate::fika_sctk::quad::point(x, y), self.width, self.height)
        {
            self.render_scene("select");
        }
    }

    pub(crate) fn scroll_at(&mut self, x: f64, y: f64, horizontal: f32, vertical: f32) {
        if self.scene.scroll_at(
            crate::fika_sctk::quad::point(x, y),
            horizontal,
            vertical,
            self.width,
            self.height,
        ) {
            self.render_scene("scroll");
        }
    }

    fn ui_scale(&self) -> f32 {
        self.scale_factor.max(1.0)
    }

    fn apply_surface_state(&self) {
        self.window.xdg_surface().set_window_geometry(
            0,
            0,
            self.width.max(1) as i32,
            self.height.max(1) as i32,
        );
        if let Some(viewport) = &self.viewport {
            self.window.wl_surface().set_buffer_scale(1);
            viewport.set_source(
                0.0,
                0.0,
                surface_extent(self.width, self.ui_scale()) as f64,
                surface_extent(self.height, self.ui_scale()) as f64,
            );
            viewport.set_destination(self.width as i32, self.height as i32);
        } else {
            self.window
                .wl_surface()
                .set_buffer_scale(self.scale_factor.round().max(1.0) as i32);
        }
    }

    fn scale_mode(&self) -> &'static str {
        if self.viewport.is_some() {
            "viewport"
        } else {
            "buffer"
        }
    }
}

fn key_command(keysym: Keysym, modifiers: Modifiers) -> Option<SceneCommand> {
    let shortcut = modifiers.ctrl || modifiers.logo;
    if keysym == Keysym::F1 || keysym == Keysym::_1 || keysym == Keysym::KP_1 {
        return Some(SceneCommand::SetViewMode(fika_core::ViewMode::Icons));
    }
    if keysym == Keysym::F2 || keysym == Keysym::_2 || keysym == Keysym::KP_2 {
        return Some(SceneCommand::SetViewMode(fika_core::ViewMode::Compact));
    }
    if keysym == Keysym::F3 || keysym == Keysym::_3 || keysym == Keysym::KP_3 {
        return Some(SceneCommand::SetViewMode(fika_core::ViewMode::Details));
    }
    if shortcut && (keysym == Keysym::h || keysym == Keysym::H) {
        return Some(SceneCommand::ToggleHidden);
    }
    if keysym == Keysym::F5 || (shortcut && (keysym == Keysym::r || keysym == Keysym::R)) {
        return Some(SceneCommand::Reload);
    }
    match keysym {
        Keysym::Left => Some(SceneCommand::MoveSelection(PaneSelectionMove::Left)),
        Keysym::Right => Some(SceneCommand::MoveSelection(PaneSelectionMove::Right)),
        Keysym::Up => Some(SceneCommand::MoveSelection(PaneSelectionMove::Up)),
        Keysym::Down => Some(SceneCommand::MoveSelection(PaneSelectionMove::Down)),
        Keysym::Home => Some(SceneCommand::MoveSelection(PaneSelectionMove::First)),
        Keysym::End => Some(SceneCommand::MoveSelection(PaneSelectionMove::Last)),
        Keysym::Page_Up => Some(SceneCommand::MoveSelection(PaneSelectionMove::PageUp)),
        Keysym::Page_Down => Some(SceneCommand::MoveSelection(PaneSelectionMove::PageDown)),
        Keysym::Return | Keysym::KP_Enter => Some(SceneCommand::ActivateSelection),
        Keysym::Escape => Some(SceneCommand::ClearSelection),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_command_maps_view_mode_shortcuts() {
        assert_eq!(
            key_command(Keysym::F1, Modifiers::default()),
            Some(SceneCommand::SetViewMode(fika_core::ViewMode::Icons))
        );
        assert_eq!(
            key_command(Keysym::_2, Modifiers::default()),
            Some(SceneCommand::SetViewMode(fika_core::ViewMode::Compact))
        );
        assert_eq!(
            key_command(Keysym::KP_3, Modifiers::default()),
            Some(SceneCommand::SetViewMode(fika_core::ViewMode::Details))
        );
    }

    #[test]
    fn key_command_maps_pane_commands() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            key_command(Keysym::h, ctrl),
            Some(SceneCommand::ToggleHidden)
        );
        assert_eq!(
            key_command(Keysym::F5, Modifiers::default()),
            Some(SceneCommand::Reload)
        );
        assert_eq!(
            key_command(Keysym::Down, Modifiers::default()),
            Some(SceneCommand::MoveSelection(PaneSelectionMove::Down))
        );
        assert_eq!(
            key_command(Keysym::Return, Modifiers::default()),
            Some(SceneCommand::ActivateSelection)
        );
    }
}
