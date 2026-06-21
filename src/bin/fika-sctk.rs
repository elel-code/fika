use std::error::Error;

#[path = "fika_sctk/mod.rs"]
mod fika_sctk;

fn main() -> Result<(), Box<dyn Error>> {
    fika_sctk::run()
}
