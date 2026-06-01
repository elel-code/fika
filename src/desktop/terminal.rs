use super::systemd_launch;
use std::env;
use std::path::Path;

const FALLBACK_TERMINALS: &[&str] = &[
    "xdg-terminal-exec",
    "konsole",
    "kgx",
    "ptyxis",
    "gnome-terminal",
    "foot",
    "kitty",
    "alacritty",
    "wezterm",
    "xfce4-terminal",
    "xterm",
];

pub(crate) fn open_terminal_here(dir: &Path) -> Result<systemd_launch::LaunchResult, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }

    let mut attempted = Vec::new();
    for terminal in terminal_candidates() {
        attempted.push(terminal.clone());
        match systemd_launch::spawn_in_user_scope_with_dir(
            &terminal,
            &terminal_args(&terminal),
            Some("Terminal"),
            Some(dir),
        ) {
            Ok(unit) => return Ok(unit),
            Err(err) if is_missing_program(&err) => {}
            Err(err) => return Err(err),
        }
    }

    Err(format!(
        "no terminal emulator found; tried {}",
        attempted.join(", ")
    ))
}

fn terminal_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    for key in ["FIKA_TERMINAL", "TERMINAL"] {
        if let Ok(value) = env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                push_unique(&mut candidates, value.to_string());
            }
        }
    }
    for terminal in FALLBACK_TERMINALS {
        push_unique(&mut candidates, (*terminal).to_string());
    }
    candidates
}

fn terminal_args(terminal: &str) -> Vec<String> {
    let name = terminal.rsplit('/').next().unwrap_or(terminal);
    match name {
        "wezterm" => vec!["start".to_string()],
        _ => Vec::new(),
    }
}

fn is_missing_program(error: &str) -> bool {
    error.contains("No such file or directory") || error.contains("os error 2")
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wezterm_uses_start_subcommand() {
        assert_eq!(terminal_args("wezterm"), vec!["start"]);
        assert_eq!(terminal_args("/usr/bin/wezterm"), vec!["start"]);
        assert!(terminal_args("konsole").is_empty());
    }

    #[test]
    fn missing_program_detection_matches_os_errors() {
        assert!(is_missing_program("xterm: No such file or directory"));
        assert!(is_missing_program("xterm: os error 2"));
        assert!(!is_missing_program("permission denied"));
    }
}
