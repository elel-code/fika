use super::{icons, mime_open, systemd_launch};
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const SERVICE_MENU_TYPE: &str = "Service";
const KONQ_POPUP_PLUGIN: &str = "KonqPopupMenu/Plugin";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMenuAction {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) icon_path: Option<PathBuf>,
    pub(crate) desktop_path: PathBuf,
    pub(crate) action_key: String,
    pub(crate) exec: String,
    pub(crate) argv: Vec<String>,
    pub(crate) top_level: bool,
    pub(crate) submenu: String,
}

#[derive(Debug)]
pub(crate) struct ServiceMenuActionsResult {
    pub(crate) generation: u64,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) result: Result<Vec<ServiceMenuAction>, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedServiceMenuItem {
    path: PathBuf,
    mime_type: String,
}

#[derive(Debug)]
struct ServiceMenuDesktopEntry {
    path: PathBuf,
    actions: Vec<String>,
    mime_types: Vec<String>,
    top_level: bool,
    submenu: String,
    sections: HashMap<String, HashMap<String, String>>,
}

pub(crate) fn list_actions_for_paths(paths: &[PathBuf]) -> Result<Vec<ServiceMenuAction>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let selected = selected_items(paths)?;
    Ok(actions_for_service_menu_dirs(
        &service_menu_dirs(),
        &selected,
    ))
}

pub(crate) fn launch_action(
    action: &ServiceMenuAction,
) -> Result<systemd_launch::LaunchResult, String> {
    let (program, args) = action
        .argv
        .split_first()
        .ok_or_else(|| format!("{} has an empty command", action.name))?;
    systemd_launch::spawn_in_user_scope(program, args, Some(&action.name))
}

fn expand_service_menu_exec(
    exec: &str,
    paths: &[PathBuf],
    action_name: &str,
    desktop_path: &Path,
) -> Result<Vec<String>, String> {
    let args = parse_desktop_exec(exec)?;
    let mut expanded = Vec::new();
    for arg in args {
        expanded.extend(expand_service_menu_exec_arg(
            &arg,
            paths,
            action_name,
            desktop_path,
        ));
    }
    if expanded.is_empty() {
        Err("service menu action has an empty Exec command".to_string())
    } else {
        Ok(expanded)
    }
}

fn selected_items(paths: &[PathBuf]) -> Result<Vec<SelectedServiceMenuItem>, String> {
    paths
        .iter()
        .map(|path| {
            Ok(SelectedServiceMenuItem {
                path: path.clone(),
                mime_type: mime_open::guess_mime_type(path)?,
            })
        })
        .collect()
}

fn service_menu_dirs() -> Vec<PathBuf> {
    service_menu_dirs_for_data_dirs(data_dirs())
}

fn service_menu_dirs_for_data_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter()
        .flat_map(|dir| {
            [
                dir.join("fika").join("servicemenus"),
                dir.join("kio").join("servicemenus"),
            ]
        })
        .collect()
}

fn actions_for_service_menu_dirs(
    dirs: &[PathBuf],
    selected: &[SelectedServiceMenuItem],
) -> Vec<ServiceMenuAction> {
    let mut actions = Vec::new();
    for dir in dirs {
        collect_service_menu_actions(dir, 3, selected, &mut actions);
    }
    actions.sort_by(|left, right| {
        right
            .top_level
            .cmp(&left.top_level)
            .then_with(|| left.submenu.cmp(&right.submenu))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    actions
}

fn collect_service_menu_actions(
    dir: &Path,
    depth: usize,
    selected: &[SelectedServiceMenuItem],
    actions: &mut Vec<ServiceMenuAction>,
) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_service_menu_actions(&path, depth - 1, selected, actions);
            continue;
        }
        if path.extension().and_then(OsStr::to_str) != Some("desktop") {
            continue;
        }
        actions.extend(actions_from_service_menu_file(&path, selected));
    }
}

fn actions_from_service_menu_file(
    path: &Path,
    selected: &[SelectedServiceMenuItem],
) -> Vec<ServiceMenuAction> {
    let Ok(entry) = parse_service_menu_file(path) else {
        return Vec::new();
    };
    if !service_menu_matches_selection(&entry, selected) {
        return Vec::new();
    }

    entry
        .actions
        .iter()
        .filter_map(|action_key| service_menu_action(&entry, action_key, selected))
        .collect()
}

fn parse_service_menu_file(path: &Path) -> Result<ServiceMenuDesktopEntry, String> {
    let content = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let sections = parse_ini_sections(&content);
    let desktop = sections
        .get("Desktop Entry")
        .ok_or_else(|| format!("{} has no Desktop Entry section", path.display()))?;

    if desktop.get("Type").map(String::as_str) != Some(SERVICE_MENU_TYPE) {
        return Err("not a service menu desktop entry".to_string());
    }
    if desktop.get("Hidden").is_some_and(|value| value == "true")
        || desktop
            .get("NoDisplay")
            .is_some_and(|value| value == "true")
    {
        return Err("service menu desktop entry is hidden".to_string());
    }

    let service_types = desktop
        .get("ServiceTypes")
        .or_else(|| desktop.get("X-KDE-ServiceTypes"))
        .map(|value| desktop_list(value).collect::<Vec<_>>())
        .unwrap_or_default();
    if !service_types.contains(&KONQ_POPUP_PLUGIN) {
        return Err("service menu is not a Konq popup plugin".to_string());
    }

    let actions = desktop
        .get("Actions")
        .map(|value| desktop_list(value).map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    if actions.is_empty() {
        return Err("service menu has no actions".to_string());
    }

    Ok(ServiceMenuDesktopEntry {
        path: path.to_path_buf(),
        actions,
        mime_types: desktop
            .get("MimeType")
            .map(|value| desktop_list(value).map(str::to_string).collect())
            .unwrap_or_default(),
        top_level: desktop
            .get("X-KDE-Priority")
            .is_some_and(|value| value == "TopLevel"),
        submenu: desktop.get("X-KDE-Submenu").cloned().unwrap_or_default(),
        sections,
    })
}

fn service_menu_matches_selection(
    entry: &ServiceMenuDesktopEntry,
    selected: &[SelectedServiceMenuItem],
) -> bool {
    !selected.is_empty()
        && selected.iter().all(|item| {
            entry.mime_types.is_empty()
                || entry
                    .mime_types
                    .iter()
                    .any(|pattern| mime_pattern_matches(pattern, &item.mime_type))
        })
}

fn service_menu_action(
    entry: &ServiceMenuDesktopEntry,
    action_key: &str,
    selected: &[SelectedServiceMenuItem],
) -> Option<ServiceMenuAction> {
    let section_name = format!("Desktop Action {action_key}");
    let section = entry.sections.get(&section_name)?;
    let exec = section.get("Exec")?;
    if selected.len() > 1 && !exec_supports_multiple(exec) {
        return None;
    }
    let name = section.get("Name")?.trim();
    if name.is_empty() {
        return None;
    }
    let paths = selected
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    let argv = expand_service_menu_exec(exec, &paths, name, &entry.path).ok()?;

    let icon = section.get("Icon").cloned().unwrap_or_default();
    let icon_path = icons::resolve_icon_path(&icon, 20);

    Some(ServiceMenuAction {
        id: format!("{}:{action_key}", entry.path.display()),
        name: name.to_string(),
        icon,
        icon_path,
        desktop_path: entry.path.clone(),
        action_key: action_key.to_string(),
        exec: exec.to_string(),
        argv,
        top_level: entry.top_level,
        submenu: entry.submenu.clone(),
    })
}

fn mime_pattern_matches(pattern: &str, mime_type: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == mime_type || pattern == "all/all" {
        return true;
    }
    if pattern == "all/allfiles" {
        return mime_type != "inode/directory";
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return mime_type
            .split_once('/')
            .is_some_and(|(top, _)| top == prefix);
    }
    false
}

fn exec_supports_multiple(exec: &str) -> bool {
    ["%F", "%U", "%D", "%N"]
        .iter()
        .any(|field| exec.contains(field))
}

fn expand_service_menu_exec_arg(
    arg: &str,
    paths: &[PathBuf],
    action_name: &str,
    desktop_path: &Path,
) -> Vec<String> {
    if matches!(arg, "%F" | "%U") {
        return paths.iter().map(|path| path_arg(path)).collect();
    }
    if arg == "%D" {
        return paths.iter().filter_map(|path| parent_arg(path)).collect();
    }
    if arg == "%N" {
        return paths
            .iter()
            .filter_map(|path| file_name_arg(path))
            .collect();
    }

    let first_path = paths.first();
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
                if let Some(path) = first_path {
                    output.push_str(&path_arg(path));
                }
            }
            Some('d' | 'D') => {
                if let Some(path) = first_path.and_then(|path| parent_arg(path)) {
                    output.push_str(&path);
                }
            }
            Some('n' | 'N') => {
                if let Some(name) = first_path.and_then(|path| file_name_arg(path)) {
                    output.push_str(&name);
                }
            }
            Some('c') => output.push_str(action_name),
            Some('k') => output.push_str(&desktop_path.to_string_lossy()),
            Some('i') => {}
            Some(_) | None => {}
        }
    }

    if output.is_empty() {
        Vec::new()
    } else {
        vec![output]
    }
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn parent_arg(path: &Path) -> Option<String> {
    path.parent()
        .map(|parent| parent.to_string_lossy().to_string())
}

fn file_name_arg(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
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

fn desktop_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').filter_map(|entry| {
        let entry = entry.trim();
        if entry.is_empty() { None } else { Some(entry) }
    })
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
        return Err("unterminated quote in service menu Exec command".to_string());
    }

    if !current.is_empty() {
        args.push(current);
    }

    if args.is_empty() {
        Err("empty service menu Exec command".to_string())
    } else {
        Ok(args)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_menu_dirs_include_fika_first_and_kde_compatible_paths() {
        assert_eq!(
            service_menu_dirs_for_data_dirs([
                PathBuf::from("/home/yk/.local/share"),
                PathBuf::from("/usr/share"),
            ]),
            vec![
                PathBuf::from("/home/yk/.local/share/fika/servicemenus"),
                PathBuf::from("/home/yk/.local/share/kio/servicemenus"),
                PathBuf::from("/usr/share/fika/servicemenus"),
                PathBuf::from("/usr/share/kio/servicemenus"),
            ]
        );
    }

    #[test]
    fn service_menu_parser_filters_by_mime_and_multiselect_exec() {
        let temp = test_dir("mime-filter");
        let menu_dir = temp.join("kio").join("servicemenus");
        fs::create_dir_all(&menu_dir).unwrap();
        let menu_path = menu_dir.join("edit-with-zed.desktop");
        fs::write(
            &menu_path,
            "[Desktop Entry]\n\
             Type=Service\n\
             ServiceTypes=KonqPopupMenu/Plugin\n\
             MimeType=text/plain;application/octet-stream;\n\
             Actions=openzedfile;singlefile;\n\
             X-KDE-Priority=TopLevel\n\
             \n\
             [Desktop Action openzedfile]\n\
             Name=Edit with Zed\n\
             Icon=zed\n\
             Exec=zeditor %F\n\
             \n\
             [Desktop Action singlefile]\n\
             Name=Single File Only\n\
             Exec=zeditor %f\n",
        )
        .unwrap();

        let selected = vec![
            selected("/tmp/a.txt", "text/plain"),
            selected("/tmp/b.txt", "text/plain"),
        ];

        let actions = actions_for_service_menu_dirs(&[menu_dir], &selected);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Edit with Zed");
        assert_eq!(actions[0].icon, "zed");
        assert_eq!(actions[0].argv, vec!["zeditor", "/tmp/a.txt", "/tmp/b.txt"]);
        assert!(actions[0].top_level);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn service_menu_actions_sort_top_level_then_submenu_groups() {
        let temp = test_dir("submenu-sort");
        let menu_dir = temp.join("kio").join("servicemenus");
        fs::create_dir_all(&menu_dir).unwrap();
        for (file, submenu, action, name, priority) in [
            ("tools-b.desktop", "Tools", "toolsb", "Tools B", ""),
            ("edit.desktop", "Edit", "edit", "Edit A", ""),
            (
                "top.desktop",
                "",
                "top",
                "Top Action",
                "X-KDE-Priority=TopLevel\n",
            ),
            ("tools-a.desktop", "Tools", "toolsa", "Tools A", ""),
        ] {
            fs::write(
                menu_dir.join(file),
                format!(
                    "[Desktop Entry]\n\
                     Type=Service\n\
                     ServiceTypes=KonqPopupMenu/Plugin\n\
                     MimeType=text/plain\n\
                     X-KDE-Submenu={submenu}\n\
                     {priority}\
                     Actions={action}\n\
                     \n\
                     [Desktop Action {action}]\n\
                     Name={name}\n\
                     Exec=app %F\n"
                ),
            )
            .unwrap();
        }

        let actions =
            actions_for_service_menu_dirs(&[menu_dir], &[selected("/tmp/file.txt", "text/plain")]);

        assert_eq!(
            actions
                .iter()
                .map(|action| (
                    action.name.as_str(),
                    action.submenu.as_str(),
                    action.top_level
                ))
                .collect::<Vec<_>>(),
            vec![
                ("Top Action", "", true),
                ("Edit A", "Edit", false),
                ("Tools A", "Tools", false),
                ("Tools B", "Tools", false),
            ]
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn directory_service_menu_matches_inode_directory() {
        let temp = test_dir("directory-menu");
        let menu_dir = temp.join("kio").join("servicemenus");
        fs::create_dir_all(&menu_dir).unwrap();
        fs::write(
            menu_dir.join("ghostty.desktop"),
            "[Desktop Entry]\n\
             Type=Service\n\
             ServiceTypes=KonqPopupMenu/Plugin\n\
             MimeType=inode/directory\n\
             Actions=RunGhosttyDir\n\
             \n\
             [Desktop Action RunGhosttyDir]\n\
             Name=Open Ghostty Here\n\
             Icon=com.mitchellh.ghostty\n\
             Exec=ghostty --working-directory=%F --gtk-single-instance=false\n",
        )
        .unwrap();

        let actions = actions_for_service_menu_dirs(
            &[menu_dir],
            &[selected("/tmp/project", "inode/directory")],
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Open Ghostty Here");
        assert_eq!(
            actions[0].argv,
            vec![
                "ghostty",
                "--working-directory=/tmp/project",
                "--gtk-single-instance=false"
            ]
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn service_menu_discovery_ignores_non_service_desktop_entries() {
        let temp = test_dir("non-service");
        let menu_dir = temp.join("kio").join("servicemenus");
        fs::create_dir_all(&menu_dir).unwrap();
        fs::write(
            menu_dir.join("plain.desktop"),
            "[Desktop Entry]\n\
             Type=Application\n\
             Actions=open\n\
             \n\
             [Desktop Action open]\n\
             Name=Open\n\
             Exec=app %F\n",
        )
        .unwrap();

        let actions =
            actions_for_service_menu_dirs(&[menu_dir], &[selected("/tmp/file.txt", "text/plain")]);

        assert!(actions.is_empty());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn service_menu_exec_expands_kde_file_fields() {
        assert_eq!(
            expand_service_menu_exec(
                "ghostty --working-directory=%F --name %n --desktop %k",
                &[PathBuf::from("/tmp/project")],
                "Open Ghostty Here",
                Path::new("/tmp/ghostty.desktop")
            )
            .unwrap(),
            vec![
                "ghostty",
                "--working-directory=/tmp/project",
                "--name",
                "project",
                "--desktop",
                "/tmp/ghostty.desktop",
            ]
        );
    }

    #[test]
    fn service_menu_exec_expands_multi_file_argument() {
        assert_eq!(
            expand_service_menu_exec(
                "zeditor %F",
                &[PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
                "Edit with Zed",
                Path::new("/tmp/zed.desktop")
            )
            .unwrap(),
            vec!["zeditor", "/tmp/a.txt", "/tmp/b.txt"]
        );
    }

    fn selected(path: &str, mime_type: &str) -> SelectedServiceMenuItem {
        SelectedServiceMenuItem {
            path: PathBuf::from(path),
            mime_type: mime_type.to_string(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("fika-service-menu-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
