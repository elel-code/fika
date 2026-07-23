use std::time::{Duration, Instant};

use wayland_client_runtime::{
    Event, LogicalRect, Runtime, RuntimeOptions, TextInputContentHint, TextInputContentPurpose,
    TextInputContentType, TextInputEvent, TextInputState, TextInputSurroundingText,
    ToplevelAttributes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::connect(RuntimeOptions::default())?;
    if !runtime.capabilities().text_input_v3 {
        println!("zwp_text_input_v3 is unavailable");
        return Ok(());
    }

    let surface = runtime.create_toplevel(ToplevelAttributes {
        title: "text-input-v3 smoke".into(),
        app_id: "dev.fika.WaylandClientRuntimeTextInputSmoke".into(),
        ..Default::default()
    })?;
    let state = TextInputState::new()
        .with_surrounding_text(TextInputSurroundingText::new("", 0, 0)?)
        .with_content_type(TextInputContentType {
            hints: TextInputContentHint::COMPLETION,
            purpose: TextInputContentPurpose::Normal,
        })
        .with_cursor_rectangle(LogicalRect::new(16, 16, 1, 20))?;
    runtime.set_text_input_state(surface, Some(state))?;

    println!("focus the window and type; waiting for text-input-v3 events");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        runtime.dispatch(Some(Duration::from_millis(250)))?;
        for event in runtime.drain_events() {
            match event {
                Event::TextInput(TextInputEvent::Entered { surface: entered })
                    if entered == surface =>
                {
                    println!("entered surface={}", surface.get());
                }
                Event::TextInput(TextInputEvent::Left { surface: left }) if left == surface => {
                    println!("left surface={}", surface.get());
                }
                Event::TextInput(TextInputEvent::Done(done)) if done.surface == surface => {
                    println!(
                        "done serial={} delete={:?} commit={:?} preedit={:?}",
                        done.serial, done.delete_surrounding, done.commit, done.preedit
                    );
                }
                _ => {}
            }
        }
    }
    Ok(())
}
