use std::time::{Duration, Instant};

use wayland_client_runtime::{
    BlurRegion, BlurState, Event, Runtime, RuntimeOptions, SurfaceEvent, ToplevelAttributes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "wayland-client-runtime smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimeSmoke".into(),
        ..Default::default()
    })?;
    if runtime.capabilities().ext_background_effect {
        runtime.set_blur(surface, BlurState::Enabled(BlurRegion::EntireSurface))?;
        runtime.commit(surface)?;
    }
    let deadline = Instant::now() + Duration::from_secs(2);

    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(100)))?;
        for event in runtime.drain_events() {
            if let Event::Surface(SurfaceEvent::Configure {
                surface: configured,
                suggested_size,
                ..
            }) = event
                && configured == surface
            {
                println!(
                    "configured surface={} width={:?} height={:?}",
                    surface.get(),
                    suggested_size.width,
                    suggested_size.height
                );
                return Ok(());
            }
        }
    }

    Err("timed out waiting for the initial xdg-toplevel configure".into())
}
