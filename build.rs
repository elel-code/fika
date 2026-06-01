fn main() {
    // Slint 1.16.1 hides DragArea/DropArea behind this compiler flag. Keep it
    // enabled so DnD work can use the built-ins instead of more winit glue.
    unsafe {
        std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    }
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");
}
