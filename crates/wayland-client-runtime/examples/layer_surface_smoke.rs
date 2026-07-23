use std::time::{Duration, Instant};

use wayland_client_runtime::{
    Event, LayerAnchor, LayerEdge, LayerSurfaceAttributes, LayerSurfaceEvent, LayerSurfaceState,
    LogicalSize, Runtime, RuntimeOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    if !capabilities.layer_shell_v1 {
        println!("layer-shell-v1 is unavailable");
        return Ok(());
    }

    let surface = runtime.create_layer_surface(LayerSurfaceAttributes {
        namespace: "dev.fika.WaylandClientRuntimeLayerSmoke".into(),
        state: LayerSurfaceState {
            size: LogicalSize::new(0, 32),
            anchor: LayerAnchor::TOP | LayerAnchor::LEFT | LayerAnchor::RIGHT,
            exclusive_zone: 32,
            exclusive_edge: capabilities
                .layer_shell_exclusive_edge
                .then_some(LayerEdge::Top),
            ..Default::default()
        },
        ..Default::default()
    })?;

    println!("waiting for layer configure/closed events for five seconds");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(250)))?;
        for event in runtime.drain_events() {
            match event {
                Event::LayerSurface(LayerSurfaceEvent::Configure {
                    surface: configured,
                    suggested_size,
                    serial,
                }) if configured == surface => {
                    println!("configure serial={serial} size={suggested_size:?}");
                }
                Event::LayerSurface(LayerSurfaceEvent::Closed { surface: closed })
                    if closed == surface =>
                {
                    println!("closed");
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
