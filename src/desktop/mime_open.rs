use super::systemd_launch;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct DesktopEntry {
    name: Option<String>,
    exec: Option<String>,
    mime_types: Vec<String>,
    no_display: bool,
    terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateApp {
    pub desktop_id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenLaunch {
    pub(crate) mime_type: String,
    pub(crate) unit: Option<String>,
    pub(crate) launch_diagnostic: Option<String>,
}

pub fn open_file_with_default_app(path: &Path) -> Result<OpenLaunch, String> {
    let mime_type = guess_mime_type(path)?;
    let desktop_id = find_default_app(&mime_type)
        .ok_or_else(|| format!("no default application for MIME type {mime_type}"))?;
    let mut launch = open_file_with_app(path, &desktop_id)?;
    launch.mime_type = mime_type;
    Ok(launch)
}

pub fn list_apps_for_file(path: &Path) -> Result<(String, Vec<CandidateApp>), String> {
    let mime_type = guess_mime_type(path)?;
    let default_app = find_default_app(&mime_type);
    let mut ids = Vec::new();

    if let Some(default_app) = &default_app {
        push_unique(&mut ids, default_app);
    }

    for desktop_id in associated_apps(&mime_type) {
        push_unique(&mut ids, &desktop_id);
    }

    let apps = ids
        .into_iter()
        .filter_map(|desktop_id| {
            candidate_for_desktop_id(&desktop_id, default_app.as_deref(), Some(&mime_type))
        })
        .collect();

    Ok((mime_type, apps))
}

pub fn list_other_apps_for_file(path: &Path) -> Result<(String, Vec<CandidateApp>), String> {
    let mime_type = guess_mime_type(path)?;
    let default_app = find_default_app(&mime_type);
    let mut ids = Vec::new();

    for desktop_id in associated_apps(&mime_type) {
        push_unique(&mut ids, &desktop_id);
    }
    for desktop_id in all_desktop_apps() {
        push_unique(&mut ids, &desktop_id);
    }

    let apps = ids
        .into_iter()
        .filter_map(|desktop_id| {
            candidate_for_desktop_id(&desktop_id, default_app.as_deref(), None)
        })
        .collect();

    Ok((mime_type, apps))
}

pub fn open_file_with_app(path: &Path, desktop_id: &str) -> Result<OpenLaunch, String> {
    let mime_type = guess_mime_type(path)?;
    let desktop_path = find_desktop_file(desktop_id)
        .ok_or_else(|| format!("desktop file not found: {desktop_id}"))?;
    let entry = parse_desktop_file(&desktop_path)?;
    let launch = spawn_desktop_entry(desktop_id, &entry, path)?;
    Ok(OpenLaunch {
        mime_type,
        unit: launch.unit,
        launch_diagnostic: launch.diagnostic,
    })
}

pub fn open_file_with_custom_command(
    path: &Path,
    command: &str,
) -> Result<systemd_launch::LaunchResult, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("custom command cannot be empty".to_string());
    }
    spawn_desktop_exec(command, path, Some(command))
}

pub fn set_default_app_for_file(path: &Path, desktop_id: &str) -> Result<String, String> {
    let mime_type = guess_mime_type(path)?;
    let desktop_path = find_desktop_file(desktop_id)
        .ok_or_else(|| format!("desktop file not found: {desktop_id}"))?;
    let entry = parse_desktop_file(&desktop_path)?;
    if entry.no_display {
        return Err(format!("{desktop_id} is hidden from desktop launchers"));
    }
    if entry.terminal {
        return Err(format!("{desktop_id} requires a terminal launcher"));
    }
    write_user_default_app(&mime_type, desktop_id)?;
    Ok(mime_type)
}

pub fn guess_mime_type(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|err| err.to_string())?;
    if metadata.is_dir() {
        return Ok("inode/directory".to_string());
    }

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
            if let Some(mime) = guess_mime_from_extension(path.extension()) {
                return Ok(mime.to_string());
            }
            return Err(err.to_string());
        }
        Err(err) => return Err(err.to_string()),
    };
    let mut buffer = [0_u8; 8192];
    let read = file.read(&mut buffer).map_err(|err| err.to_string())?;
    let bytes = &buffer[..read];

    if let Some(mime) = guess_mime_from_magic(bytes) {
        return Ok(mime.to_string());
    }

    if let Some(mime) = guess_mime_from_extension(path.extension()) {
        return Ok(mime.to_string());
    }

    if looks_like_text(bytes) {
        return Ok("text/plain".to_string());
    }

    Ok("application/octet-stream".to_string())
}

fn guess_mime_from_magic(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(b"PK\x03\x04") {
        Some("application/zip")
    } else if bytes.starts_with(b"\x1f\x8b") {
        Some("application/gzip")
    } else if bytes.starts_with(b"\x7fELF") {
        Some("application/x-executable")
    } else if bytes.starts_with(b"SQLite format 3\0") {
        Some("application/vnd.sqlite3")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn guess_mime_from_extension(extension: Option<&OsStr>) -> Option<&'static str> {
    let extension = extension?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "txt" | "text" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rs" => "text/rust",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "cxx" | "hpp" => "text/x-c++src",
        "py" => "text/x-python",
        "js" | "mjs" | "cjs" => "application/javascript",
        "ts" | "tsx" => "application/typescript",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "xml" => "application/xml",
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        "7z" => "application/x-7z-compressed",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "desktop" => "application/x-desktop",
        _ => return None,
    })
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }
    std::str::from_utf8(bytes).is_ok()
}

fn find_default_app(mime_type: &str) -> Option<String> {
    let mut removed = HashSet::new();

    for path in mimeapps_paths() {
        let Ok(file) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse_ini_sections(&file);
        collect_removed(&parsed, mime_type, &mut removed);

        if let Some(app) = first_existing_app(&parsed, "Default Applications", mime_type, &removed)
        {
            return Some(app);
        }
    }

    for path in mimeapps_paths() {
        let Ok(file) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse_ini_sections(&file);
        if let Some(app) = first_existing_app(&parsed, "Added Associations", mime_type, &removed) {
            return Some(app);
        }
    }

    None
}

fn collect_removed(
    sections: &HashMap<String, HashMap<String, String>>,
    mime_type: &str,
    removed: &mut HashSet<String>,
) {
    if let Some(entries) = sections
        .get("Removed Associations")
        .and_then(|s| s.get(mime_type))
    {
        for desktop_id in desktop_list(entries) {
            removed.insert(desktop_id.to_string());
        }
    }
}

fn first_existing_app(
    sections: &HashMap<String, HashMap<String, String>>,
    section: &str,
    mime_type: &str,
    removed: &HashSet<String>,
) -> Option<String> {
    sections
        .get(section)?
        .get(mime_type)
        .into_iter()
        .flat_map(|value| desktop_list(value))
        .filter(|desktop_id| !removed.contains(*desktop_id))
        .find(|desktop_id| find_desktop_file(desktop_id).is_some())
        .map(str::to_string)
}

fn desktop_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').filter_map(|entry| {
        let entry = entry.trim();
        if entry.is_empty() { None } else { Some(entry) }
    })
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|candidate| candidate == value) {
        values.push(value.to_string());
    }
}

fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
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

fn parse_desktop_file(path: &Path) -> Result<DesktopEntry, String> {
    let file = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let sections = parse_ini_sections(&file);
    let section = sections
        .get("Desktop Entry")
        .ok_or_else(|| format!("{} has no Desktop Entry section", path.display()))?;

    Ok(DesktopEntry {
        name: section.get("Name").cloned(),
        exec: section.get("Exec").cloned(),
        mime_types: section
            .get("MimeType")
            .map(|value| desktop_list(value).map(str::to_string).collect())
            .unwrap_or_default(),
        no_display: section.get("NoDisplay").is_some_and(|v| v == "true"),
        terminal: section.get("Terminal").is_some_and(|v| v == "true"),
    })
}

fn associated_apps(mime_type: &str) -> Vec<String> {
    let mut removed = HashSet::new();
    let mut apps = Vec::new();

    for path in mimeapps_paths() {
        let Ok(file) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse_ini_sections(&file);
        collect_removed(&parsed, mime_type, &mut removed);

        for section in ["Default Applications", "Added Associations"] {
            if let Some(value) = parsed
                .get(section)
                .and_then(|section| section.get(mime_type))
            {
                for desktop_id in desktop_list(value) {
                    if !removed.contains(desktop_id) {
                        push_unique(&mut apps, desktop_id);
                    }
                }
            }
        }
    }

    for path in mimeinfo_cache_paths() {
        let Ok(file) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse_ini_sections(&file);
        if let Some(value) = parsed
            .get("MIME Cache")
            .and_then(|section| section.get(mime_type))
        {
            for desktop_id in desktop_list(value) {
                if !removed.contains(desktop_id) {
                    push_unique(&mut apps, desktop_id);
                }
            }
        }
    }

    apps
}

fn candidate_for_desktop_id(
    desktop_id: &str,
    default_app: Option<&str>,
    mime_type: Option<&str>,
) -> Option<CandidateApp> {
    let desktop_path = find_desktop_file(desktop_id)?;
    let entry = parse_desktop_file(&desktop_path).ok()?;
    if entry.no_display || entry.terminal || entry.exec.is_none() {
        return None;
    }
    if let Some(mime_type) = mime_type
        && !entry.mime_types.is_empty()
        && !entry
            .mime_types
            .iter()
            .any(|candidate| candidate == mime_type)
    {
        return None;
    }

    Some(CandidateApp {
        desktop_id: desktop_id.to_string(),
        name: entry.name.unwrap_or_else(|| desktop_id.to_string()),
        is_default: default_app.is_some_and(|default_app| default_app == desktop_id),
    })
}

fn spawn_desktop_entry(
    desktop_id: &str,
    entry: &DesktopEntry,
    path: &Path,
) -> Result<systemd_launch::LaunchResult, String> {
    if entry.no_display {
        return Err(format!("{desktop_id} is hidden from desktop launchers"));
    }
    if entry.terminal {
        return Err(format!("{desktop_id} requires a terminal launcher"));
    }

    let exec = entry
        .exec
        .as_deref()
        .ok_or_else(|| format!("{desktop_id} has no Exec entry"))?;

    spawn_desktop_exec(exec, path, entry.name.as_deref())
}

fn mimeapps_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let desktops = current_desktops();

    for base in config_dirs() {
        push_desktop_mimeapps(&mut paths, &base, "", &desktops);
    }

    for base in data_dirs() {
        push_desktop_mimeapps(&mut paths, &base, "applications", &desktops);
    }

    paths
}

fn push_desktop_mimeapps(paths: &mut Vec<PathBuf>, base: &Path, subdir: &str, desktops: &[String]) {
    let dir = if subdir.is_empty() {
        base.to_path_buf()
    } else {
        base.join(subdir)
    };

    for desktop in desktops {
        paths.push(dir.join(format!("{desktop}-mimeapps.list")));
    }
    paths.push(dir.join("mimeapps.list"));
}

fn mimeinfo_cache_paths() -> Vec<PathBuf> {
    data_dirs()
        .into_iter()
        .map(|base| base.join("applications").join("mimeinfo.cache"))
        .collect()
}

fn all_desktop_apps() -> Vec<String> {
    let mut ids = Vec::new();
    for data_dir in data_dirs() {
        collect_desktop_ids(&data_dir.join("applications"), "", 4, &mut ids);
    }
    ids
}

fn collect_desktop_ids(dir: &Path, prefix: &str, depth: usize, ids: &mut Vec<String>) {
    if depth == 0 {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if path.is_file() && name.ends_with(".desktop") {
            push_unique(ids, &format!("{prefix}{name}"));
        } else if path.is_dir() {
            collect_desktop_ids(&path, &format!("{prefix}{name}-"), depth - 1, ids);
        }
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

fn config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config")),
    );
    dirs.extend(split_paths_or_default("XDG_CONFIG_DIRS", "/etc/xdg"));
    dirs
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

fn current_desktops() -> Vec<String> {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter_map(|desktop| {
            let desktop = desktop.trim().to_ascii_lowercase();
            if desktop.is_empty() {
                None
            } else {
                Some(desktop)
            }
        })
        .collect()
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn user_mimeapps_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("mimeapps.list")
}

fn write_user_default_app(mime_type: &str, desktop_id: &str) -> Result<(), String> {
    let path = user_mimeapps_path();
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut sections = parse_ini_sections(&content);

    sections
        .entry("Default Applications".to_string())
        .or_default()
        .insert(mime_type.to_string(), format!("{desktop_id};"));

    let added = sections
        .entry("Added Associations".to_string())
        .or_default()
        .entry(mime_type.to_string())
        .or_default()
        .clone();
    let mut added_apps = Vec::new();
    push_unique(&mut added_apps, desktop_id);
    for app in desktop_list(&added) {
        push_unique(&mut added_apps, app);
    }
    sections
        .entry("Added Associations".to_string())
        .or_default()
        .insert(mime_type.to_string(), desktop_list_value(&added_apps));

    if let Some(removed) = sections.get_mut("Removed Associations")
        && let Some(value) = removed.get_mut(mime_type)
    {
        let kept = desktop_list(value)
            .filter(|app| *app != desktop_id)
            .map(str::to_string)
            .collect::<Vec<_>>();
        *value = desktop_list_value(&kept);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let temporary_path = path.with_extension("list.tmp");
    fs::write(&temporary_path, serialize_mimeapps(&sections)).map_err(|err| err.to_string())?;
    fs::rename(&temporary_path, &path).map_err(|err| err.to_string())?;
    Ok(())
}

fn desktop_list_value(apps: &[String]) -> String {
    if apps.is_empty() {
        String::new()
    } else {
        format!("{};", apps.join(";"))
    }
}

fn serialize_mimeapps(sections: &HashMap<String, HashMap<String, String>>) -> String {
    let preferred = [
        "Default Applications",
        "Added Associations",
        "Removed Associations",
    ];
    let mut section_names = preferred
        .iter()
        .filter(|section| sections.contains_key(**section))
        .map(|section| (*section).to_string())
        .collect::<Vec<_>>();
    let mut remaining = sections
        .keys()
        .filter(|section| !preferred.contains(&section.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    remaining.sort();
    section_names.extend(remaining);

    let mut output = String::new();
    for section_name in section_names {
        let Some(entries) = sections.get(&section_name) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push('[');
        output.push_str(&section_name);
        output.push_str("]\n");

        let mut keys = entries.keys().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            output.push_str(key);
            output.push('=');
            output.push_str(&entries[key]);
            output.push('\n');
        }
    }

    output
}

fn spawn_desktop_exec(
    exec: &str,
    path: &Path,
    app_name: Option<&str>,
) -> Result<systemd_launch::LaunchResult, String> {
    let mut argv = parse_desktop_exec(exec)?;
    let file_arg = path.to_string_lossy();
    let mut consumed_file = false;

    argv = argv
        .into_iter()
        .filter_map(|arg| {
            expand_desktop_exec_arg(&arg, file_arg.as_ref(), app_name, &mut consumed_file)
        })
        .collect();

    if !consumed_file {
        argv.push(file_arg.into_owned());
    }

    let (program, args) = argv
        .split_first()
        .ok_or_else(|| "default application has an empty Exec command".to_string())?;

    systemd_launch::spawn_in_user_scope(program, args, app_name)
}

fn expand_desktop_exec_arg(
    arg: &str,
    file_arg: &str,
    app_name: Option<&str>,
    consumed_file: &mut bool,
) -> Option<String> {
    let mut output = String::new();
    let mut chars = arg.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('%') => output.push('%'),
            Some('f' | 'F' | 'u' | 'U') => {
                output.push_str(file_arg);
                *consumed_file = true;
            }
            Some('c') => output.push_str(app_name.unwrap_or_default()),
            Some('i' | 'k') => {}
            Some(_) | None => {}
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn parse_desktop_exec(exec: &str) -> Result<Vec<String>, String> {
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
        return Err("unterminated quote in default application Exec command".to_string());
    }

    if !current.is_empty() {
        args.push(current);
    }

    if args.is_empty() {
        Err("empty default application Exec command".to_string())
    } else {
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_common_mime_types() {
        assert_eq!(
            guess_mime_from_extension(Some(OsStr::new("png"))),
            Some("image/png")
        );
        assert_eq!(guess_mime_from_magic(b"%PDF-1.7"), Some("application/pdf"));
    }

    #[test]
    fn parses_mimeapps_sections() {
        let parsed =
            parse_ini_sections("[Default Applications]\ntext/plain=code.desktop;vim.desktop;\n");
        assert_eq!(
            parsed["Default Applications"]["text/plain"],
            "code.desktop;vim.desktop;"
        );
    }

    #[test]
    fn parses_mimeinfo_cache_section() {
        let parsed = parse_ini_sections("[MIME Cache]\nimage/png=org.kde.gwenview.desktop;\n");
        assert_eq!(
            desktop_list(&parsed["MIME Cache"]["image/png"]).collect::<Vec<_>>(),
            vec!["org.kde.gwenview.desktop"]
        );
    }

    #[test]
    fn parses_desktop_exec_with_quotes() {
        assert_eq!(
            parse_desktop_exec("code --reuse-window \"foo bar\"").unwrap(),
            vec!["code", "--reuse-window", "foo bar"]
        );
    }

    #[test]
    fn expands_desktop_exec_field_codes() {
        let mut consumed = false;

        assert_eq!(
            expand_desktop_exec_arg("--open=%f", "/tmp/example.txt", None, &mut consumed),
            Some("--open=/tmp/example.txt".to_string())
        );
        assert!(consumed);
    }

    #[test]
    fn serializes_user_mimeapps_defaults_first() {
        let mut sections = HashMap::new();
        sections.insert(
            "Added Associations".to_string(),
            HashMap::from([(
                "text/plain".to_string(),
                "code.desktop;vim.desktop;".to_string(),
            )]),
        );
        sections.insert(
            "Default Applications".to_string(),
            HashMap::from([("text/plain".to_string(), "code.desktop;".to_string())]),
        );

        assert_eq!(
            serialize_mimeapps(&sections),
            "[Default Applications]\ntext/plain=code.desktop;\n\n[Added Associations]\ntext/plain=code.desktop;vim.desktop;\n"
        );
    }

    #[test]
    fn desktop_list_value_uses_trailing_semicolon() {
        assert_eq!(
            desktop_list_value(&["code.desktop".to_string(), "vim.desktop".to_string()]),
            "code.desktop;vim.desktop;"
        );
    }
}
