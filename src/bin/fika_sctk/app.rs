use std::error::Error;
use std::num::NonZeroU32;

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowDecorations},
        },
    },
};
use wayland_client::Connection;
use wayland_client::globals::registry_queue_init;

use super::options::StartupOptions;
use super::renderer::WgpuRenderer;
use super::scene::SctkScene;

const DEFAULT_WIDTH: u32 = 1100;
const DEFAULT_HEIGHT: u32 = 720;

pub(crate) fn run(options: StartupOptions) -> Result<(), Box<dyn Error>> {
    let scene = SctkScene::load(options.path, options.view_mode)?;
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

    let renderer = WgpuRenderer::new(&conn, &window)?;
    let mut app = FikaSctkApp {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        exit: false,
        ready_logged: false,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
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
    pub(crate) exit: bool,
    ready_logged: bool,
    width: u32,
    height: u32,
    renderer: WgpuRenderer,
    #[allow(dead_code)]
    window: Window,
    scene: SctkScene,
}

impl FikaSctkApp {
    pub(crate) fn request_exit(&mut self) {
        self.exit = true;
    }

    pub(crate) fn handle_configure(&mut self, configure: WindowConfigure) {
        let (width, height) = configure.new_size;
        self.width = width.map(NonZeroU32::get).unwrap_or(DEFAULT_WIDTH);
        self.height = height.map(NonZeroU32::get).unwrap_or(DEFAULT_HEIGHT);
        let config = self.renderer.configure_surface(self.width, self.height);
        if !self.ready_logged {
            eprintln!(
                "[fika-sctk] shell-ready size={}x{} format={:?} path={} view={} entries={} dirs={} files={}",
                self.width,
                self.height,
                config.format,
                self.scene.path().display(),
                self.scene.view_mode().as_str(),
                self.scene.entry_count(),
                self.scene.dir_count(),
                self.scene.file_count()
            );
            self.ready_logged = true;
        } else {
            eprintln!("[fika-sctk] configure size={}x{}", self.width, self.height);
        }
        self.renderer.render_clear_frame();
    }
}
