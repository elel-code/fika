use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopApplication {
    pub id: String,
    pub desktop_file: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub mime_types: Vec<String>,
    pub actions: Vec<DesktopAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MimeApplication {
    pub id: String,
    pub desktop_file: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub is_default: bool,
}

impl From<(&DesktopApplication, bool)> for MimeApplication {
    fn from((app, is_default): (&DesktopApplication, bool)) -> Self {
        Self {
            id: app.id.clone(),
            desktop_file: app.desktop_file.clone(),
            name: app.name.clone(),
            exec: app.exec.clone(),
            icon: app.icon.clone(),
            is_default,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MimeAppsList {
    pub default_apps: HashMap<String, Vec<String>>,
    pub added_associations: HashMap<String, Vec<String>>,
    pub removed_associations: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct MimeApplicationCache {
    apps: Vec<DesktopApplication>,
    apps_by_id: HashMap<String, usize>,
    apps_by_filename: HashMap<String, usize>,
    apps_by_mime: HashMap<String, Vec<usize>>,
    default_by_mime: HashMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopLaunchPlan {
    pub desktop_id: String,
    pub desktop_file: PathBuf,
    pub app_name: String,
    pub commands: Vec<DesktopLaunchCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemdLaunchUnit {
    pub unit_name: String,
    pub description: String,
    pub command: DesktopLaunchCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemdLaunchResult {
    pub units: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LauncherError {
    EmptyLaunchPlan {
        app_name: String,
    },
    EmptyCommand {
        app_name: String,
    },
    ProgramNotFound {
        program: String,
    },
    InvalidSystemdProperty {
        property: &'static str,
        message: String,
    },
    SessionBus {
        message: String,
    },
    SystemdManager {
        message: String,
    },
    StartTransientUnit {
        unit_name: String,
        message: String,
    },
}

impl fmt::Display for LauncherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LauncherError::EmptyLaunchPlan { app_name } => {
                write!(f, "{app_name} did not produce a launch command")
            }
            LauncherError::EmptyCommand { app_name } => {
                write!(f, "{app_name} produced an empty launch command")
            }
            LauncherError::ProgramNotFound { program } => {
                write!(f, "cannot find executable {program}")
            }
            LauncherError::InvalidSystemdProperty { property, message } => {
                write!(f, "cannot build systemd property {property}: {message}")
            }
            LauncherError::SessionBus { message } => {
                write!(f, "cannot connect to session bus: {message}")
            }
            LauncherError::SystemdManager { message } => {
                write!(f, "cannot create systemd user manager proxy: {message}")
            }
            LauncherError::StartTransientUnit { unit_name, message } => {
                write!(f, "cannot start {unit_name}: {message}")
            }
        }
    }
}

impl Error for LauncherError {}

impl DesktopApplication {
    pub fn launch_plan(&self, paths: &[PathBuf]) -> Option<DesktopLaunchPlan> {
        let commands = exec_to_launch_commands(&self.exec, &self.name, &self.desktop_file, paths)?;
        Some(DesktopLaunchPlan {
            desktop_id: self.id.clone(),
            desktop_file: self.desktop_file.clone(),
            app_name: self.name.clone(),
            commands,
        })
    }
}

pub fn systemd_units_for_launch_plan(
    plan: &DesktopLaunchPlan,
) -> Result<Vec<SystemdLaunchUnit>, LauncherError> {
    let nonce = systemd_launch_nonce();
    systemd_units_for_launch_plan_with_nonce(plan, nonce)
}

pub async fn launch_with_systemd_user(
    plan: DesktopLaunchPlan,
) -> Result<SystemdLaunchResult, LauncherError> {
    let units = systemd_units_for_launch_plan(&plan)?;
    let connection =
        zbus::Connection::session()
            .await
            .map_err(|err| LauncherError::SessionBus {
                message: err.to_string(),
            })?;
    let manager = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|err| LauncherError::SystemdManager {
        message: err.to_string(),
    })?;

    let mut started = Vec::with_capacity(units.len());
    for unit in &units {
        start_systemd_launch_unit(&manager, unit).await?;
        started.push(unit.unit_name.clone());
    }
    Ok(SystemdLaunchResult { units: started })
}

impl MimeApplicationCache {
    pub fn load() -> Self {
        let applications = load_desktop_applications();
        let lists = mimeapps_list_paths()
            .into_iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .map(|contents| parse_mimeapps_list(&contents))
            .collect::<Vec<_>>();
        Self::from_applications_and_mimeapps(applications, &lists)
    }

    pub fn shared() -> &'static Self {
        static CACHE: OnceLock<MimeApplicationCache> = OnceLock::new();
        CACHE.get_or_init(Self::load)
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_applications_and_mimeapps(
        mut apps: Vec<DesktopApplication>,
        lists: &[MimeAppsList],
    ) -> Self {
        apps.sort_by(desktop_application_cmp);
        let mut cache = Self::default();
        for app in apps {
            cache.insert_application(app);
        }
        cache.apply_mimeapps_lists(lists);
        cache
    }

    pub fn applications_for_mime(&self, mime: &str) -> Vec<MimeApplication> {
        let mime = mime.trim();
        if mime.is_empty() {
            return Vec::new();
        }
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        let default_id = self.default_by_mime.get(mime);

        if let Some(default_id) = default_id
            && let Some(index) = self.application_index(default_id)
            && seen.insert(index)
        {
            ordered.push(MimeApplication::from((&self.apps[index], true)));
        }

        if let Some(indexes) = self.apps_by_mime.get(mime) {
            for index in indexes {
                if seen.insert(*index) {
                    let is_default =
                        default_id.is_some_and(|id| self.application_matches(*index, id));
                    ordered.push(MimeApplication::from((&self.apps[*index], is_default)));
                }
            }
        }

        ordered
    }

    pub fn application(&self, desktop_id: &str) -> Option<&DesktopApplication> {
        self.application_index(desktop_id)
            .and_then(|index| self.apps.get(index))
    }

    fn insert_application(&mut self, app: DesktopApplication) {
        let index = self.apps.len();
        self.apps_by_id.insert(app.id.clone(), index);
        if let Some(filename) = app.desktop_file.file_name().and_then(|name| name.to_str()) {
            self.apps_by_filename
                .entry(filename.to_string())
                .or_insert(index);
        }
        for mime in &app.mime_types {
            self.apps_by_mime
                .entry(mime.clone())
                .or_default()
                .push(index);
        }
        self.apps.push(app);
    }

    fn apply_mimeapps_lists(&mut self, lists: &[MimeAppsList]) {
        let mut removed_by_mime: HashMap<String, HashSet<String>> = HashMap::new();
        for list in lists {
            for (mime, ids) in &list.removed_associations {
                let removed = removed_by_mime.entry(mime.clone()).or_default();
                removed.extend(ids.iter().cloned());
            }
        }

        let mut added_offsets: HashMap<String, usize> = HashMap::new();
        for list in lists {
            for (mime, ids) in &list.default_apps {
                if self.default_by_mime.contains_key(mime) {
                    continue;
                }
                if let Some(id) = ids.iter().find(|id| self.application_index(id).is_some()) {
                    self.default_by_mime.insert(mime.clone(), id.clone());
                    self.prepend_mime_application(mime, id);
                }
            }
            for (mime, ids) in &list.added_associations {
                for id in ids {
                    if removed_by_mime
                        .get(mime)
                        .is_some_and(|removed| removed.contains(id))
                    {
                        continue;
                    }
                    let offset = *added_offsets.get(mime).unwrap_or(&0);
                    if self.insert_added_mime_application(mime, id, offset) {
                        *added_offsets.entry(mime.clone()).or_default() += 1;
                    }
                }
            }
        }

        for (mime, removed) in removed_by_mime {
            let removed_indexes = removed
                .iter()
                .filter_map(|id| self.application_index(id))
                .collect::<HashSet<_>>();
            if let Some(indexes) = self.apps_by_mime.get_mut(&mime) {
                indexes.retain(|index| !removed_indexes.contains(index));
            }
        }
    }

    fn prepend_mime_application(&mut self, mime: &str, desktop_id: &str) {
        let Some(index) = self.application_index(desktop_id) else {
            return;
        };
        let indexes = self.apps_by_mime.entry(mime.to_string()).or_default();
        indexes.retain(|existing| *existing != index);
        indexes.insert(0, index);
    }

    fn insert_added_mime_application(
        &mut self,
        mime: &str,
        desktop_id: &str,
        added_offset: usize,
    ) -> bool {
        let Some(index) = self.application_index(desktop_id) else {
            return false;
        };
        if self
            .default_by_mime
            .get(mime)
            .is_some_and(|default_id| self.application_matches(index, default_id))
        {
            return false;
        }
        let default_slots = self.default_by_mime.get(mime).map_or(0, |default_id| {
            usize::from(
                self.apps_by_mime
                    .get(mime)
                    .and_then(|indexes| indexes.first())
                    .is_some_and(|first| self.application_matches(*first, default_id)),
            )
        });
        let indexes = self.apps_by_mime.entry(mime.to_string()).or_default();
        indexes.retain(|existing| *existing != index);
        let insert_at = (default_slots + added_offset).min(indexes.len());
        indexes.insert(insert_at, index);
        true
    }

    fn application_index(&self, desktop_id: &str) -> Option<usize> {
        self.apps_by_id
            .get(desktop_id)
            .copied()
            .or_else(|| self.apps_by_filename.get(desktop_id).copied())
    }

    fn application_matches(&self, index: usize, desktop_id: &str) -> bool {
        self.application_index(desktop_id) == Some(index)
    }
}

pub fn parse_desktop_application(
    id: impl Into<String>,
    desktop_file: impl Into<PathBuf>,
    contents: &str,
) -> Option<DesktopApplication> {
    let sections = parse_desktop_sections(contents);
    let entry = sections.get("Desktop Entry")?;
    if entry.get("Hidden").is_some_and(|value| desktop_bool(value)) {
        return None;
    }
    if entry.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    let name = entry.get("Name")?.trim();
    let exec = entry.get("Exec")?.trim();
    if name.is_empty() || exec.is_empty() {
        return None;
    }

    let action_ids = entry
        .get("Actions")
        .map(|value| desktop_list(value))
        .unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|action_id| {
            let section = sections.get(&format!("Desktop Action {action_id}"))?;
            let name = section.get("Name")?.trim();
            let exec = section.get("Exec")?.trim();
            (!name.is_empty() && !exec.is_empty()).then(|| DesktopAction {
                id: action_id,
                name: name.to_string(),
                exec: exec.to_string(),
            })
        })
        .collect();

    Some(DesktopApplication {
        id: id.into(),
        desktop_file: desktop_file.into(),
        name: name.to_string(),
        exec: exec.to_string(),
        icon: entry.get("Icon").filter(|icon| !icon.is_empty()).cloned(),
        mime_types: entry
            .get("MimeType")
            .map(|value| desktop_list(value))
            .unwrap_or_default(),
        actions,
    })
}

pub fn parse_mimeapps_list(contents: &str) -> MimeAppsList {
    let mut list = MimeAppsList::default();
    let mut section = "";
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((mime, value)) = line.split_once('=') else {
            continue;
        };
        let apps = desktop_list(value);
        if apps.is_empty() {
            continue;
        }
        match section {
            "Default Applications" => append_mimeapps(&mut list.default_apps, mime, apps),
            "Added Associations" => append_mimeapps(&mut list.added_associations, mime, apps),
            "Removed Associations" => append_mimeapps(&mut list.removed_associations, mime, apps),
            _ => {}
        }
    }
    list
}

pub fn exec_to_launch_commands(
    exec: &str,
    app_name: &str,
    desktop_file: &Path,
    paths: &[PathBuf],
) -> Option<Vec<DesktopLaunchCommand>> {
    let argv = split_exec_line(exec)?;
    let program = argv.first()?.clone();
    let mut args = Vec::new();
    let mut file_code_used = false;
    for token in argv.into_iter().skip(1) {
        match token.as_str() {
            "%F" | "%U" => {
                file_code_used = true;
                args.extend(paths.iter().map(|path| path.display().to_string()));
            }
            _ => {
                if let Some(argument) =
                    expand_exec_token(&token, app_name, desktop_file, paths.first())
                {
                    if token.contains("%f") || token.contains("%u") {
                        file_code_used = true;
                    }
                    args.push(argument);
                }
            }
        }
    }

    if !file_code_used && paths.is_empty() {
        return Some(vec![DesktopLaunchCommand { program, args }]);
    }
    Some(vec![DesktopLaunchCommand { program, args }])
}

fn systemd_units_for_launch_plan_with_nonce(
    plan: &DesktopLaunchPlan,
    nonce: u128,
) -> Result<Vec<SystemdLaunchUnit>, LauncherError> {
    if plan.commands.is_empty() {
        return Err(LauncherError::EmptyLaunchPlan {
            app_name: plan.app_name.clone(),
        });
    }

    plan.commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let command = systemd_launch_command(command, &plan.app_name)?;
            Ok(SystemdLaunchUnit {
                unit_name: systemd_launch_unit_name(&plan.desktop_id, index, nonce),
                description: format!("Fika Open With {}", plan.app_name),
                command,
            })
        })
        .collect()
}

fn systemd_launch_command(
    command: &DesktopLaunchCommand,
    app_name: &str,
) -> Result<DesktopLaunchCommand, LauncherError> {
    if command.program.trim().is_empty() {
        return Err(LauncherError::EmptyCommand {
            app_name: app_name.to_string(),
        });
    }
    let program = executable_path_for_systemd(&command.program)?;
    Ok(DesktopLaunchCommand {
        program: program.display().to_string(),
        args: command.args.clone(),
    })
}

pub fn systemd_launch_unit_name(desktop_id: &str, index: usize, nonce: u128) -> String {
    let component = sanitize_systemd_unit_component(desktop_id);
    format!("fika-open-with-{component}-{index}-{nonce:x}.service")
}

fn sanitize_systemd_unit_component(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len().min(48));
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            ch
        } else {
            '-'
        };
        if sanitized.ends_with('-') && next == '-' {
            continue;
        }
        sanitized.push(next);
        if sanitized.len() >= 48 {
            break;
        }
    }
    let sanitized = sanitized.trim_matches('-').trim_matches('.').to_string();
    if sanitized.is_empty() {
        "application".to_string()
    } else {
        sanitized
    }
}

fn systemd_launch_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn executable_path_for_systemd(program: &str) -> Result<PathBuf, LauncherError> {
    let program_path = Path::new(program);
    if program_path.is_absolute() {
        return executable_file_path(program_path).ok_or_else(|| LauncherError::ProgramNotFound {
            program: program.to_string(),
        });
    }
    if program.contains('/') {
        return Err(LauncherError::ProgramNotFound {
            program: program.to_string(),
        });
    }

    for dir in env::var_os("PATH")
        .filter(|path| !path.is_empty())
        .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        })
    {
        let candidate = dir.join(program);
        if let Some(path) = executable_file_path(&candidate) {
            return Ok(path);
        }
    }

    Err(LauncherError::ProgramNotFound {
        program: program.to_string(),
    })
}

fn executable_file_path(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if executable_permissions(&metadata) {
        Some(path.to_path_buf())
    } else {
        None
    }
}

#[cfg(unix)]
fn executable_permissions(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_permissions(_metadata: &fs::Metadata) -> bool {
    true
}

type SystemdProperty = (String, OwnedValue);
type SystemdAuxUnit = (String, Vec<SystemdProperty>);
type SystemdExecCommand = (String, Vec<String>, bool);

fn systemd_properties_for_launch_unit(
    unit: &SystemdLaunchUnit,
) -> Result<Vec<SystemdProperty>, LauncherError> {
    let mut argv = Vec::with_capacity(unit.command.args.len() + 1);
    argv.push(unit.command.program.clone());
    argv.extend(unit.command.args.iter().cloned());
    let exec_start: Vec<SystemdExecCommand> = vec![(unit.command.program.clone(), argv, false)];

    let mut properties = vec![
        systemd_property("Description", unit.description.clone())?,
        systemd_property("Type", "exec".to_string())?,
        systemd_property("ExecStart", exec_start)?,
    ];
    let environment = systemd_launch_environment();
    if !environment.is_empty() {
        properties.push(systemd_property("Environment", environment)?);
    }
    Ok(properties)
}

fn systemd_property<T>(name: &'static str, value: T) -> Result<SystemdProperty, LauncherError>
where
    T: zbus::zvariant::DynamicType + Into<Value<'static>>,
{
    let value = OwnedValue::try_from(Value::new(value)).map_err(|err| {
        LauncherError::InvalidSystemdProperty {
            property: name,
            message: err.to_string(),
        }
    })?;
    Ok((name.to_string(), value))
}

fn systemd_launch_environment() -> Vec<String> {
    const KEYS: &[&str] = &[
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "DBUS_SESSION_BUS_ADDRESS",
        "SSH_AUTH_SOCK",
        "LANG",
        "LC_ALL",
    ];
    KEYS.iter()
        .filter_map(|key| env::var(key).ok().map(|value| format!("{key}={value}")))
        .collect()
}

async fn start_systemd_launch_unit(
    manager: &zbus::Proxy<'_>,
    unit: &SystemdLaunchUnit,
) -> Result<OwnedObjectPath, LauncherError> {
    let properties = systemd_properties_for_launch_unit(unit)?;
    let aux: Vec<SystemdAuxUnit> = Vec::new();
    manager
        .call(
            "StartTransientUnit",
            &(unit.unit_name.as_str(), "fail", properties, aux),
        )
        .await
        .map_err(|err| LauncherError::StartTransientUnit {
            unit_name: unit.unit_name.clone(),
            message: err.to_string(),
        })
}

fn load_desktop_applications() -> Vec<DesktopApplication> {
    let mut applications = Vec::new();
    for applications_dir in desktop_application_dirs() {
        collect_desktop_applications(&applications_dir, &applications_dir, &mut applications);
    }
    applications
}

fn collect_desktop_applications(
    root: &Path,
    dir: &Path,
    applications: &mut Vec<DesktopApplication>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_desktop_applications(root, &path, applications);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
            continue;
        }
        let Some(id) = desktop_id_for_path(root, &path) else {
            continue;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(application) = parse_desktop_application(id, path, &contents) {
            applications.push(application);
        }
    }
}

fn desktop_application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(&mut dirs, PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut dirs,
            PathBuf::from(home).join(".local/share/applications"),
        );
    }
    for data_dir in env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .map(|paths| {
            env::split_paths(&paths)
                .map(|path| path.join("applications"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        })
    {
        push_unique_path(&mut dirs, data_dir);
    }
    dirs
}

fn mimeapps_list_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(&mut paths, PathBuf::from(config_home).join("mimeapps.list"));
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(home).join(".config/mimeapps.list"),
        );
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(data_home).join("applications/mimeapps.list"),
        );
    } else if let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) {
        push_unique_path(
            &mut paths,
            PathBuf::from(home).join(".local/share/applications/mimeapps.list"),
        );
    }
    for data_dir in env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .map(|paths| {
            env::split_paths(&paths)
                .map(|path| path.join("applications/mimeapps.list"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications/mimeapps.list"),
                PathBuf::from("/usr/share/applications/mimeapps.list"),
            ]
        })
    {
        push_unique_path(&mut paths, data_dir);
    }
    paths
}

fn desktop_id_for_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let id = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("-");
    (!id.is_empty()).then_some(id)
}

fn parse_desktop_sections(contents: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut section = String::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            sections.entry(section.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section.is_empty() || key.contains('[') {
            continue;
        }
        sections
            .entry(section.clone())
            .or_default()
            .insert(key.trim().to_string(), desktop_unescape(value.trim()));
    }
    sections
}

fn desktop_unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            match ch {
                's' => out.push(' '),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                _ => out.push(ch),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    if escaped {
        out.push('\\');
    }
    out
}

fn desktop_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn append_mimeapps(target: &mut HashMap<String, Vec<String>>, mime: &str, apps: Vec<String>) {
    target
        .entry(mime.trim().to_string())
        .or_default()
        .extend(apps);
}

fn desktop_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

fn desktop_application_cmp(
    left: &DesktopApplication,
    right: &DesktopApplication,
) -> std::cmp::Ordering {
    left.name
        .to_ascii_lowercase()
        .cmp(&right.name.to_ascii_lowercase())
        .then_with(|| left.id.cmp(&right.id))
}

fn split_exec_line(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        args.push(current);
    }
    (!args.is_empty()).then_some(args)
}

fn expand_exec_token(
    token: &str,
    app_name: &str,
    desktop_file: &Path,
    path: Option<&PathBuf>,
) -> Option<String> {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('c') => out.push_str(app_name),
            Some('k') => out.push_str(&desktop_file.display().to_string()),
            Some('f') | Some('u') => {
                let path = path?;
                out.push_str(&path.display().to_string());
            }
            Some('F') | Some('U') => return None,
            Some('i') | Some('d') | Some('D') | Some('n') | Some('N') | Some('v') | Some('m') => {}
            Some(_) | None => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher_test_executable(name: &str) -> (PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = env::temp_dir().join(format!("fika-launcher-test-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join(name);
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).unwrap();
        }
        (dir, executable)
    }

    fn desktop_app(id: &str, name: &str, mime_types: &[&str]) -> DesktopApplication {
        DesktopApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(format!("/apps/{id}")),
            name: name.to_string(),
            exec: format!("{name} %f"),
            icon: None,
            mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
            actions: Vec::new(),
        }
    }

    #[test]
    fn desktop_entry_parser_reads_application_mime_and_actions() {
        let entry = parse_desktop_application(
            "org.example.Viewer.desktop",
            "/apps/org.example.Viewer.desktop",
            "\
[Desktop Entry]\n\
Type=Application\n\
Name=Example Viewer\n\
Exec=viewer %f\n\
Icon=viewer\n\
MimeType=text/plain;image/png;\n\
Actions=print;\n\
\n\
[Desktop Action print]\n\
Name=Print\n\
Exec=viewer --print %f\n",
        )
        .unwrap();

        assert_eq!(entry.name, "Example Viewer");
        assert_eq!(entry.mime_types, vec!["text/plain", "image/png"]);
        assert_eq!(entry.actions[0].name, "Print");
    }

    #[test]
    fn hidden_desktop_entries_are_not_applications() {
        assert!(
            parse_desktop_application(
                "hidden.desktop",
                "/apps/hidden.desktop",
                "[Desktop Entry]\nType=Application\nHidden=true\nName=Hidden\nExec=hidden %f\n",
            )
            .is_none()
        );
    }

    #[test]
    fn mimeapps_parser_reads_default_added_and_removed_groups() {
        let list = parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=default.desktop;\n\
[Added Associations]\n\
text/plain=added.desktop;other.desktop;\n\
[Removed Associations]\n\
text/plain=other.desktop;\n",
        );

        assert_eq!(
            list.default_apps["text/plain"],
            vec!["default.desktop".to_string()]
        );
        assert_eq!(list.added_associations["text/plain"].len(), 2);
        assert_eq!(
            list.removed_associations["text/plain"],
            vec!["other.desktop".to_string()]
        );
    }

    #[test]
    fn mime_application_cache_orders_default_added_and_declared_apps() {
        let apps = vec![
            desktop_app("declared.desktop", "Declared", &["text/plain"]),
            desktop_app("default.desktop", "Default", &["text/plain"]),
            desktop_app("added.desktop", "Added", &[]),
            desktop_app("removed.desktop", "Removed", &["text/plain"]),
        ];
        let list = parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=default.desktop;\n\
[Added Associations]\n\
text/plain=added.desktop;\n\
[Removed Associations]\n\
text/plain=removed.desktop;\n",
        );

        let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[list]);
        let names = cache
            .applications_for_mime("text/plain")
            .into_iter()
            .map(|app| (app.name, app.is_default))
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                ("Default".to_string(), true),
                ("Added".to_string(), false),
                ("Declared".to_string(), false),
            ]
        );
    }

    #[test]
    fn exec_field_codes_expand_to_launch_command() {
        let command = exec_to_launch_commands(
            "viewer --name %c --desktop %k %f",
            "Viewer",
            Path::new("/apps/viewer.desktop"),
            &[PathBuf::from("/tmp/file.txt")],
        )
        .unwrap()
        .remove(0);

        assert_eq!(command.program, "viewer");
        assert_eq!(
            command.args,
            vec![
                "--name",
                "Viewer",
                "--desktop",
                "/apps/viewer.desktop",
                "/tmp/file.txt"
            ]
        );
    }

    #[test]
    fn systemd_launch_unit_name_sanitizes_desktop_id() {
        assert_eq!(
            systemd_launch_unit_name("org.example.Viewer.desktop", 2, 0x2a),
            "fika-open-with-org.example.Viewer.desktop-2-2a.service"
        );
        assert_eq!(
            systemd_launch_unit_name("///", 0, 0x2a),
            "fika-open-with-application-0-2a.service"
        );
    }

    #[test]
    fn systemd_units_for_launch_plan_resolves_executable_path() {
        let (dir, executable) = launcher_test_executable("viewer");
        let plan = DesktopLaunchPlan {
            desktop_id: "viewer.desktop".to_string(),
            desktop_file: PathBuf::from("/apps/viewer.desktop"),
            app_name: "Viewer".to_string(),
            commands: vec![DesktopLaunchCommand {
                program: executable.display().to_string(),
                args: vec!["/tmp/file.txt".to_string()],
            }],
        };

        let units = systemd_units_for_launch_plan_with_nonce(&plan, 0x2a).unwrap();

        assert_eq!(units.len(), 1);
        assert_eq!(
            units[0].unit_name,
            "fika-open-with-viewer.desktop-0-2a.service"
        );
        assert_eq!(units[0].command.program, executable.display().to_string());
        assert_eq!(units[0].command.args, vec!["/tmp/file.txt"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn systemd_properties_include_execstart_tuple() {
        let (dir, executable) = launcher_test_executable("viewer");
        let unit = SystemdLaunchUnit {
            unit_name: "fika-open-with-viewer-0.service".to_string(),
            description: "Fika Open With Viewer".to_string(),
            command: DesktopLaunchCommand {
                program: executable.display().to_string(),
                args: vec!["--flag".to_string(), "/tmp/file.txt".to_string()],
            },
        };

        let names = systemd_properties_for_launch_unit(&unit)
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"Description".to_string()));
        assert!(names.contains(&"Type".to_string()));
        assert!(names.contains(&"ExecStart".to_string()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn systemd_units_report_empty_plan_and_missing_program() {
        let empty = DesktopLaunchPlan {
            desktop_id: "empty.desktop".to_string(),
            desktop_file: PathBuf::from("/apps/empty.desktop"),
            app_name: "Empty".to_string(),
            commands: Vec::new(),
        };
        assert_eq!(
            systemd_units_for_launch_plan_with_nonce(&empty, 0x2a),
            Err(LauncherError::EmptyLaunchPlan {
                app_name: "Empty".to_string()
            })
        );

        let missing = DesktopLaunchPlan {
            desktop_id: "missing.desktop".to_string(),
            desktop_file: PathBuf::from("/apps/missing.desktop"),
            app_name: "Missing".to_string(),
            commands: vec![DesktopLaunchCommand {
                program: "/definitely/missing/fika-viewer".to_string(),
                args: Vec::new(),
            }],
        };
        assert_eq!(
            systemd_units_for_launch_plan_with_nonce(&missing, 0x2a),
            Err(LauncherError::ProgramNotFound {
                program: "/definitely/missing/fika-viewer".to_string()
            })
        );
    }
}
