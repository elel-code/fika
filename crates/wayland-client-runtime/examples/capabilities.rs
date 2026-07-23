use wayland_client_runtime::{Runtime, RuntimeOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    println!(
        "xdg_dialog_v1={} popup_reposition={} ext_background_effect={} cursor_shape={}",
        capabilities.xdg_dialog_v1,
        capabilities.popup_reposition,
        capabilities.ext_background_effect,
        capabilities.cursor_shape
    );
    Ok(())
}
