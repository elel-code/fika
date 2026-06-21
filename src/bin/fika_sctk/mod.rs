use std::error::Error;

mod app;
mod metrics;
mod options;
mod quad;
mod renderer;
mod scene;
mod wayland;

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    let Some(options) = options::StartupOptions::parse()? else {
        return Ok(());
    };
    app::run(options)
}
