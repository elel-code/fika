fn main() {
    // FlexboxLayout is still gated behind Slint's experimental compiler registry.
    // The UI uses it only for local responsive control rows, not for the main file view.
    unsafe {
        std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    }
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");
}
