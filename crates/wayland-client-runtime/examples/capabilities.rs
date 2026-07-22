use wayland_client_runtime::{Runtime, RuntimeOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    println!(
        "xdg_dialog_v1={} popup_reposition={} kde_blur={} cursor_shape={}",
        capabilities.xdg_dialog_v1,
        capabilities.popup_reposition,
        capabilities.kde_blur,
        capabilities.cursor_shape
    );
    Ok(())
}
