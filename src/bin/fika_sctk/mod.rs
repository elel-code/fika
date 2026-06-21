use std::error::Error;

mod app;
mod options;
mod renderer;
mod scene;
mod wayland;

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    app::run(options::StartupOptions::parse()?)
}
