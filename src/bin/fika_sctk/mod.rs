use std::error::Error;

mod app;
mod context_menu;
mod metrics;
mod options;
mod pane;
mod quad;
mod renderer;
mod scene;
mod text;
mod wayland;

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    let Some(options) = options::StartupOptions::parse()? else {
        return Ok(());
    };
    app::run(options)
}
