use std::time::{Duration, Instant};

use wayland_client_runtime::{
    Event, Runtime, RuntimeOptions, SurfaceEvent, ToplevelAttributes, ToplevelIcon,
    ToplevelIconBuffer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    if !runtime.capabilities().xdg_toplevel_icon_v1 {
        return Err("compositor does not advertise xdg-toplevel-icon-v1".into());
    }

    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "wayland-client-runtime icon smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimeIconSmoke".into(),
        ..Default::default()
    })?;
    let size = runtime
        .preferred_toplevel_icon_sizes()
        .into_iter()
        .filter(|size| *size <= 128)
        .max()
        .unwrap_or(64);
    let mut rgba = vec![0; size as usize * size as usize * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0x45, 0x85, 0xf4, 0xff]);
    }
    let buffer = ToplevelIconBuffer::new(rgba, size, size, 1)?;
    let icon = ToplevelIcon::new(Some("fika".to_string()), vec![buffer])?;
    runtime.set_toplevel_icon(surface, Some(icon))?;
    runtime.commit(surface)?;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(100)))?;
        let events = runtime.drain_events().collect::<Vec<_>>();
        for event in events {
            if let Event::Surface(SurfaceEvent::Configure {
                surface: configured,
                ..
            }) = event
                && configured == surface
            {
                println!(
                    "set toplevel icon surface={} size={} preferred_sizes={:?}",
                    surface.get(),
                    size,
                    runtime.preferred_toplevel_icon_sizes()
                );
                return Ok(());
            }
        }
    }

    Err("timed out waiting for icon smoke configure".into())
}
