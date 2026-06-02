use super::systemd_launch;
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

const PREFERRED_TERMINAL_DESKTOP_IDS: &[&str] = &["com.system76.CosmicTerm.desktop"];

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalCommand {
    label: String,
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Default)]
struct TerminalDesktopEntry {
    name: Option<String>,
    exec: Option<String>,
    terminal_emulator: bool,
    hidden: bool,
    no_display: bool,
}

pub(crate) fn open_terminal_here(dir: &Path) -> Result<systemd_launch::LaunchResult, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }

    let mut attempted = Vec::new();
    for terminal in terminal_candidates() {
        attempted.push(terminal.label.clone());
        match systemd_launch::spawn_in_user_scope_with_dir(
            &terminal.program,
            &terminal.args,
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

fn terminal_candidates() -> Vec<TerminalCommand> {
    let mut candidates = Vec::new();
    for key in ["FIKA_TERMINAL", "TERMINAL"] {
        if let Ok(value) = env::var(key) {
            if let Some(command) = terminal_command_from_env_value(key, &value) {
                push_unique_command(&mut candidates, command);
            }
        }
    }

    if let Some(default_desktop_id) = default_terminal_desktop_id()
        && let Some(command) = terminal_command_from_desktop_id(&default_desktop_id)
    {
        push_unique_command(&mut candidates, command);
    }

    for desktop_id in PREFERRED_TERMINAL_DESKTOP_IDS {
        if let Some(command) = terminal_command_from_desktop_id(desktop_id) {
            push_unique_command(&mut candidates, command);
        }
    }

    for command in terminal_commands_from_desktop_entries() {
        push_unique_command(&mut candidates, command);
    }

    for terminal in FALLBACK_TERMINALS {
        push_unique_command(&mut candidates, terminal_command_from_program(terminal));
    }
    candidates
}

fn terminal_command_from_env_value(key: &str, value: &str) -> Option<TerminalCommand> {
    let argv = parse_terminal_exec(value).ok()?;
    let (program, args) = argv.split_first()?;
    let mut args = args.to_vec();
    if args.is_empty() {
        args = terminal_args(program);
    }

    Some(TerminalCommand {
        label: format!("{key}={value}"),
        program: program.clone(),
        args,
    })
}

fn terminal_command_from_program(program: &str) -> TerminalCommand {
    TerminalCommand {
        label: program.to_string(),
        program: program.to_string(),
        args: terminal_args(program),
    }
}

fn terminal_args(terminal: &str) -> Vec<String> {
    let name = terminal.rsplit('/').next().unwrap_or(terminal);
    match name {
        "wezterm" => vec!["start".to_string()],
        _ => Vec::new(),
    }
}

fn default_terminal_desktop_id() -> Option<String> {
    let output = Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/terminal"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    default_terminal_desktop_id_from_stdout(&output.stdout)
}

fn default_terminal_desktop_id_from_stdout(stdout: &[u8]) -> Option<String> {
    let id = std::str::from_utf8(stdout).ok()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn terminal_command_from_desktop_id(desktop_id: &str) -> Option<TerminalCommand> {
    let path = find_desktop_file(desktop_id)?;
    let entry = parse_terminal_desktop_file(&path).ok()?;
    terminal_command_from_desktop_entry(desktop_id, &entry)
}

fn terminal_commands_from_desktop_entries() -> Vec<TerminalCommand> {
    let mut commands = Vec::new();
    for data_dir in data_dirs() {
        collect_terminal_commands(&data_dir.join("applications"), 4, &mut commands);
    }
    commands
}

fn collect_terminal_commands(dir: &Path, depth: usize, commands: &mut Vec<TerminalCommand>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_terminal_commands(&path, depth - 1, commands);
            continue;
        }
        if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("desktop") {
            continue;
        }
        let Some(desktop_id) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let Ok(entry) = parse_terminal_desktop_file(&path) else {
            continue;
        };
        if let Some(command) = terminal_command_from_desktop_entry(desktop_id, &entry) {
            push_unique_command(commands, command);
        }
    }
}

fn terminal_command_from_desktop_entry(
    desktop_id: &str,
    entry: &TerminalDesktopEntry,
) -> Option<TerminalCommand> {
    if !entry.terminal_emulator || entry.hidden || entry.no_display {
        return None;
    }
    let exec = entry.exec.as_deref()?;
    let argv = parse_terminal_exec(exec).ok()?;
    let argv: Vec<_> = argv
        .into_iter()
        .filter_map(|arg| expand_terminal_exec_arg(&arg))
        .collect();
    let (program, args) = argv.split_first()?;
    let mut args = args.to_vec();
    if args.is_empty() {
        args = terminal_args(program);
    }

    Some(TerminalCommand {
        label: entry
            .name
            .clone()
            .unwrap_or_else(|| desktop_id.trim_end_matches(".desktop").to_string()),
        program: program.clone(),
        args,
    })
}

fn parse_terminal_desktop_file(path: &Path) -> Result<TerminalDesktopEntry, String> {
    let file = fs::read_to_string(path).map_err(|err| err.to_string())?;
    parse_terminal_desktop_entry(&file)
}

fn parse_terminal_desktop_entry(content: &str) -> Result<TerminalDesktopEntry, String> {
    let sections = parse_ini_sections(content);
    let section = sections
        .get("Desktop Entry")
        .ok_or_else(|| "desktop file has no Desktop Entry section".to_string())?;
    Ok(TerminalDesktopEntry {
        name: section.get("Name").cloned(),
        exec: section.get("Exec").cloned(),
        terminal_emulator: section.get("Categories").is_some_and(|value| {
            desktop_list(value).any(|category| category == "TerminalEmulator")
        }),
        hidden: section.get("Hidden").is_some_and(|value| value == "true"),
        no_display: section
            .get("NoDisplay")
            .is_some_and(|value| value == "true"),
    })
}

fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = section.trim().to_string();
            sections.entry(current.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !current.is_empty() {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    sections
}

fn desktop_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').filter_map(|entry| {
        let entry = entry.trim();
        if entry.is_empty() { None } else { Some(entry) }
    })
}

fn parse_terminal_exec(exec: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '\\' if !in_single_quote => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() && !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if in_single_quote || in_double_quote {
        return Err("unterminated quote in terminal Exec command".to_string());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn expand_terminal_exec_arg(arg: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = arg.chars();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('%') => output.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k') => {}
            Some(_) | None => {}
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn find_desktop_file(desktop_id: &str) -> Option<PathBuf> {
    for data_dir in data_dirs() {
        let applications_dir = data_dir.join("applications");
        let direct = applications_dir.join(desktop_id);
        if direct.is_file() {
            return Some(direct);
        }

        if let Some(nested) = desktop_id_to_nested_path(desktop_id) {
            let path = applications_dir.join(nested);
            if path.is_file() {
                return Some(path);
            }
        }

        if let Some(found) = find_desktop_file_recursive(&applications_dir, desktop_id, 4) {
            return Some(found);
        }
    }

    None
}

fn desktop_id_to_nested_path(desktop_id: &str) -> Option<PathBuf> {
    let (vendor, rest) = desktop_id.split_once('-')?;
    Some(Path::new(vendor).join(rest))
}

fn find_desktop_file_recursive(dir: &Path, desktop_id: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }

    for entry in fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(OsStr::to_str) == Some(desktop_id) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_desktop_file_recursive(&path, desktop_id, depth - 1)
        {
            return Some(found);
        }
    }

    None
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".local/share")),
    );
    dirs.extend(split_paths_or_default(
        "XDG_DATA_DIRS",
        "/usr/local/share:/usr/share",
    ));
    dirs
}

fn split_paths_or_default(var: &str, default: &str) -> Vec<PathBuf> {
    let value = env::var_os(var).unwrap_or_else(|| default.into());
    env::split_paths(&value).collect()
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn is_missing_program(error: &str) -> bool {
    error.contains("No such file or directory") || error.contains("os error 2")
}

fn push_unique_command(values: &mut Vec<TerminalCommand>, value: TerminalCommand) {
    if !values
        .iter()
        .any(|existing| existing.program == value.program && existing.args == value.args)
    {
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

    #[test]
    fn default_terminal_desktop_id_trims_xdg_mime_stdout() {
        assert_eq!(
            default_terminal_desktop_id_from_stdout(b"org.example.Terminal.desktop\n"),
            Some("org.example.Terminal.desktop".to_string())
        );
        assert_eq!(default_terminal_desktop_id_from_stdout(b"\n"), None);
    }

    #[test]
    fn terminal_desktop_entry_requires_terminal_category() {
        let entry = parse_terminal_desktop_entry(
            "[Desktop Entry]\nName=Editor\nExec=editor %U\nCategories=Utility;\n",
        )
        .unwrap();
        assert_eq!(
            terminal_command_from_desktop_entry("editor.desktop", &entry),
            None
        );

        let terminal = parse_terminal_desktop_entry(
            "[Desktop Entry]\nName=Term\nExec=term %U\nCategories=System;TerminalEmulator;\n",
        )
        .unwrap();
        assert_eq!(
            terminal_command_from_desktop_entry("term.desktop", &terminal),
            Some(TerminalCommand {
                label: "Term".to_string(),
                program: "term".to_string(),
                args: Vec::new(),
            })
        );
    }

    #[test]
    fn terminal_desktop_exec_strips_field_codes_and_preserves_args() {
        let terminal = parse_terminal_desktop_entry(
            "[Desktop Entry]\nName=Term\nExec=terminal --new-window %U --title 'Project %%'\nCategories=TerminalEmulator;\n",
        )
        .unwrap();

        assert_eq!(
            terminal_command_from_desktop_entry("term.desktop", &terminal),
            Some(TerminalCommand {
                label: "Term".to_string(),
                program: "terminal".to_string(),
                args: vec![
                    "--new-window".to_string(),
                    "--title".to_string(),
                    "Project %".to_string()
                ],
            })
        );
    }

    #[test]
    fn env_terminal_value_accepts_command_arguments() {
        assert_eq!(
            terminal_command_from_env_value("FIKA_TERMINAL", "wezterm start --always-new-process"),
            Some(TerminalCommand {
                label: "FIKA_TERMINAL=wezterm start --always-new-process".to_string(),
                program: "wezterm".to_string(),
                args: vec!["start".to_string(), "--always-new-process".to_string()],
            })
        );
        assert_eq!(
            terminal_command_from_env_value("TERMINAL", "wezterm"),
            Some(TerminalCommand {
                label: "TERMINAL=wezterm".to_string(),
                program: "wezterm".to_string(),
                args: vec!["start".to_string()],
            })
        );
    }

    #[test]
    fn duplicate_terminal_commands_are_removed_by_program_and_args() {
        let mut commands = Vec::new();
        push_unique_command(&mut commands, terminal_command_from_program("kitty"));
        push_unique_command(
            &mut commands,
            TerminalCommand {
                label: "Kitty".to_string(),
                program: "kitty".to_string(),
                args: Vec::new(),
            },
        );
        push_unique_command(
            &mut commands,
            TerminalCommand {
                label: "Kitty holding".to_string(),
                program: "kitty".to_string(),
                args: vec!["--hold".to_string()],
            },
        );

        assert_eq!(commands.len(), 2);
    }
}
