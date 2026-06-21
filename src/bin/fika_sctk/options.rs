use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use fika_core::ViewMode;

#[derive(Debug)]
pub(crate) struct StartupOptions {
    pub(crate) path: PathBuf,
    pub(crate) split_path: Option<PathBuf>,
    pub(crate) view_mode: ViewMode,
}

impl StartupOptions {
    pub(crate) fn parse() -> Result<Option<Self>, Box<dyn Error>> {
        let mut args = env::args_os();
        let program = args
            .next()
            .and_then(|value| value.into_string().ok())
            .unwrap_or_else(|| "fika-sctk".to_string());

        parse_from(program, args).map_err(|error| {
            Box::new(io::Error::new(io::ErrorKind::InvalidInput, error)) as Box<dyn Error>
        })
    }
}

fn parse_from(
    program: String,
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<StartupOptions>, String> {
    let mut args = args.into_iter();
    let mut view_mode = ViewMode::Icons;
    let mut path = None;
    let mut split_requested = false;
    let mut split_path = None;
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Usage: {program} [--view icons|compact|details] [--split|--split-path PATH] [PATH]"
            );
            return Ok(None);
        }
        if arg == "--view" {
            let Some(value) = args.next() else {
                return Err(format!(
                    "usage: {program} [--view icons|compact|details] [PATH]"
                ));
            };
            let value = value
                .to_str()
                .ok_or_else(|| "--view value must be valid UTF-8".to_string())?;
            view_mode = ViewMode::parse(value)?;
            continue;
        }
        if let Some(value) = arg.to_str().and_then(|arg| arg.strip_prefix("--view=")) {
            view_mode = ViewMode::parse(value)?;
            continue;
        }
        if arg == "--split" {
            split_requested = true;
            continue;
        }
        if arg == "--split-path" {
            let Some(value) = args.next() else {
                return Err(format!(
                    "usage: {program} [--view icons|compact|details] [--split|--split-path PATH] [PATH]"
                ));
            };
            split_path = Some(PathBuf::from(value));
            split_requested = true;
            continue;
        }
        if let Some(value) = arg
            .to_str()
            .and_then(|arg| arg.strip_prefix("--split-path="))
        {
            split_path = Some(PathBuf::from(value));
            split_requested = true;
            continue;
        }
        if arg.to_str().is_some_and(|arg| arg.starts_with("--")) {
            return Err(format!("unknown option: {}", arg.to_string_lossy()));
        }
        if path.replace(PathBuf::from(arg)).is_some() {
            return Err(format!(
                "usage: {program} [--view icons|compact|details] [PATH]"
            ));
        }
    }

    let path = match path {
        Some(path) => path,
        None => env::current_dir().map_err(|error| format!("current directory: {error}"))?,
    };
    let split_path = split_path.or_else(|| split_requested.then(|| path.clone()));
    Ok(Some(StartupOptions {
        path,
        split_path,
        view_mode,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_view_mode_and_path() {
        let options = parse_from(
            "fika-sctk".to_string(),
            [
                OsString::from("--view"),
                OsString::from("details"),
                OsString::from("/etc"),
            ],
        )
        .unwrap()
        .unwrap();

        assert_eq!(options.path, PathBuf::from("/etc"));
        assert_eq!(options.split_path, None);
        assert_eq!(options.view_mode, ViewMode::Details);
    }

    #[test]
    fn parses_split_same_path() {
        let options = parse_from(
            "fika-sctk".to_string(),
            [OsString::from("--split"), OsString::from("/etc")],
        )
        .unwrap()
        .unwrap();

        assert_eq!(options.path, PathBuf::from("/etc"));
        assert_eq!(options.split_path, Some(PathBuf::from("/etc")));
    }

    #[test]
    fn parses_split_path() {
        let options = parse_from(
            "fika-sctk".to_string(),
            [
                OsString::from("--split-path"),
                OsString::from("/tmp"),
                OsString::from("/etc"),
            ],
        )
        .unwrap()
        .unwrap();

        assert_eq!(options.path, PathBuf::from("/etc"));
        assert_eq!(options.split_path, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn rejects_unknown_option() {
        assert_eq!(
            parse_from("fika-sctk".to_string(), [OsString::from("--bad")]).unwrap_err(),
            "unknown option: --bad"
        );
    }
}
