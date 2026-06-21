use std::env;
use std::error::Error;
use std::path::PathBuf;

pub(crate) struct StartupOptions {
    pub(crate) path: PathBuf,
}

impl StartupOptions {
    pub(crate) fn parse() -> Result<Self, Box<dyn Error>> {
        let path = env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or(env::current_dir()?);
        Ok(Self { path })
    }
}
