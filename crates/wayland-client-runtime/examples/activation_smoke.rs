use std::time::{Duration, Instant};

use wayland_client_runtime::{
    ActivationEvent, ActivationTokenAttributes, Event, Runtime, RuntimeOptions, ToplevelAttributes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    if !runtime.capabilities().xdg_activation_v1 {
        return Err("compositor does not advertise xdg-activation-v1".into());
    }

    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "wayland-client-runtime activation smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimeActivationSmoke".into(),
        ..Default::default()
    })?;
    runtime.request_user_attention(surface)?;
    runtime.request_user_attention(surface)?;
    let request = runtime.request_activation_token(
        surface,
        ActivationTokenAttributes {
            app_id: Some("dev.fika.WaylandClientRuntimeActivationSmoke".into()),
            serial: None,
        },
    )?;
    let deadline = Instant::now() + Duration::from_secs(2);

    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(100)))?;
        let events = runtime.drain_events().collect::<Vec<_>>();
        for event in events {
            let Event::Activation(ActivationEvent::TokenDone {
                request: completed,
                requesting_surface,
                token,
            }) = event
            else {
                continue;
            };
            if completed == request && requesting_surface == surface {
                runtime.activate_surface(surface, token)?;
                println!(
                    "activation token completed request={} surface={}",
                    request.get(),
                    surface.get()
                );
                return Ok(());
            }
        }
    }

    Err("timed out waiting for xdg-activation token".into())
}
