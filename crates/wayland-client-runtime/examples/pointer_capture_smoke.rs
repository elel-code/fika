use std::time::{Duration, Instant};

use wayland_client_runtime::{
    Event, LogicalRect, PointerCaptureState, PointerConstraint, PointerConstraintRegion, Runtime,
    RuntimeOptions, ToplevelAttributes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    if !capabilities.pointer_constraints_v1 {
        println!("zwp_pointer_constraints_v1 is unavailable");
        return Ok(());
    }

    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "pointer-constraints-v1 smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimePointerCaptureSmoke".into(),
        ..Default::default()
    })?;
    runtime.set_pointer_capture_state(
        surface,
        PointerCaptureState {
            constraint: PointerConstraint::Confined,
            relative_motion: capabilities.relative_pointer_v1,
            region: PointerConstraintRegion::Rectangles(vec![LogicalRect::new(16, 16, 768, 568)]),
        },
    )?;
    runtime.commit(surface)?;

    println!("focus the window; observing confinement and relative motion for 15 seconds");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(250)))?;
        for event in runtime.drain_events() {
            match event {
                Event::PointerConstraint(event) if event.surface == surface => {
                    println!("constraint={:?} active={}", event.constraint, event.active);
                }
                Event::RelativePointer(event) if event.surface == surface => {
                    println!(
                        "relative time={} delta={:?} unaccelerated={:?}",
                        event.time_micros, event.delta, event.delta_unaccelerated
                    );
                }
                _ => {}
            }
        }
    }

    runtime.set_pointer_constraint(surface, PointerConstraint::None)?;
    runtime.commit(surface)?;
    Ok(())
}
