use std::time::{Duration, Instant};

use wayland_client_runtime::{Event, Runtime, RuntimeOptions, SurfaceEvent, ToplevelAttributes};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    if !capabilities.pointer_gestures_v1 {
        println!("zwp_pointer_gestures_v1 is unavailable");
        return Ok(());
    }

    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "pointer-gestures-v1 smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimePointerGesturesSmoke".into(),
        ..Default::default()
    })?;
    runtime.set_pointer_gestures_enabled(surface, true)?;
    assert!(runtime.pointer_gestures_enabled(surface)?);
    runtime.commit(surface)?;

    println!(
        "gesture subscription active; hold_supported={}",
        capabilities.pointer_gesture_hold_v1
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(100)))?;
        let configured = runtime.drain_events().any(|event| {
            matches!(
                event,
                Event::Surface(SurfaceEvent::Configure {
                    surface: configured,
                    ..
                }) if configured == surface
            )
        });
        if configured {
            runtime.set_pointer_gestures_enabled(surface, false)?;
            assert!(!runtime.pointer_gestures_enabled(surface)?);
            println!("gesture subscription detached cleanly");
            return Ok(());
        }
    }

    Err("timed out waiting for the initial xdg-toplevel configure".into())
}
