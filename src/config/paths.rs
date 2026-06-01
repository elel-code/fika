use std::env;
use std::path::PathBuf;

pub fn normalize_start_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        path
    } else {
        path.parent()
            .map(|parent| {
                if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                }
            })
            .unwrap_or_else(home_dir)
    }
}

pub fn expand_user_path(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_file_path_to_parent() {
        let path = normalize_start_dir(PathBuf::from("Cargo.toml"));
        assert_eq!(path, PathBuf::from("."));
    }
}
