use wayland_client_runtime::{Runtime, RuntimeOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::connect(RuntimeOptions::default())?;
    let capabilities = runtime.capabilities();
    println!(
        "xdg_dialog_v1={} xdg_activation_v1={} xdg_toplevel_icon_v1={} layer_shell_v1={} layer_shell_dynamic_layer={} layer_shell_on_demand_keyboard={} layer_shell_exclusive_edge={} text_input_v3={} pointer_constraints_v1={} relative_pointer_v1={} popup_reposition={} ext_background_effect={} fractional_scale={} cursor_shape={}",
        capabilities.xdg_dialog_v1,
        capabilities.xdg_activation_v1,
        capabilities.xdg_toplevel_icon_v1,
        capabilities.layer_shell_v1,
        capabilities.layer_shell_dynamic_layer,
        capabilities.layer_shell_on_demand_keyboard,
        capabilities.layer_shell_exclusive_edge,
        capabilities.text_input_v3,
        capabilities.pointer_constraints_v1,
        capabilities.relative_pointer_v1,
        capabilities.popup_reposition,
        capabilities.ext_background_effect,
        capabilities.fractional_scale,
        capabilities.cursor_shape
    );
    Ok(())
}
