#![allow(dead_code)]

use super::bus::{BusController, BusKind, with_bus_tokio_context};
use super::file_ops;
use super::network::is_network_path;
use futures_lite::StreamExt;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zbus::fdo::{self, DBusProxy};
use zbus::message::Header;
use zbus::names::BusName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, proxy};
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const SERVICE_NAME: &str = "org.fika.FileManager1.Privileged";
const OBJECT_PATH: &str = "/org/fika/FileManager1/Privileged";
const ACTION_ID: &str = "org.fika.FileManager.privileged-helper";
const POLICY_FILE: &str = "org.fika.FileManager.policy";
const HELPER_IDLE_SECONDS: u64 = 180;
const EXTERNAL_EDIT_TTL_SECONDS: u64 = 24 * 60 * 60;
static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub enum HelperBus {
    System,
    Session { session_bus_address: Option<String> },
}

#[derive(Clone)]
enum ServiceMode {
    System,
    SessionPkexec { allowed_uid: u32 },
}

#[derive(Clone, Debug)]
pub(crate) enum PrivilegedCommand {
    CreateFolder {
        parent: PathBuf,
        name: String,
    },
    CreateFile {
        parent: PathBuf,
        name: String,
    },
    Rename {
        path: PathBuf,
        new_name: String,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    Transfer {
        operation: String,
        source: PathBuf,
        target_dir: PathBuf,
    },
}

#[derive(Debug)]
pub(crate) struct PrivilegedOperationResult {
    pub(crate) label: String,
    pub(crate) affected_dirs: Vec<PathBuf>,
    pub(crate) result: Result<String, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalEditSession {
    pub(crate) original_path: PathBuf,
    pub(crate) scratch_path: PathBuf,
    pub(crate) token: String,
    pub(crate) unit: Option<String>,
}

impl PrivilegedCommand {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::CreateFolder { .. } => "Create folder",
            Self::CreateFile { .. } => "Create file",
            Self::Rename { .. } => "Rename",
            Self::Trash { .. } => "Move to Trash",
            Self::Transfer { operation, .. } => match operation.as_str() {
                "move" => "Move",
                "copy" => "Copy",
                "link" => "Link",
                _ => "File operation",
            },
        }
    }

    pub(crate) fn summary(&self) -> String {
        match self {
            Self::CreateFolder { parent, name } => {
                format!("Create '{name}' in {}", parent.display())
            }
            Self::CreateFile { parent, name } => {
                format!("Create file '{name}' in {}", parent.display())
            }
            Self::Rename { path, new_name } => {
                format!("Rename {} to '{new_name}'", path.display())
            }
            Self::Trash { paths } => match paths.as_slice() {
                [path] => format!("Move {} to Trash", path.display()),
                _ => format!("Move {} items to Trash", paths.len()),
            },
            Self::Transfer {
                operation,
                source,
                target_dir,
            } => {
                let verb = match operation.as_str() {
                    "move" => "Move",
                    "copy" => "Copy",
                    "link" => "Link",
                    _ => "Transfer",
                };
                format!("{verb} {} to {}", source.display(), target_dir.display())
            }
        }
    }

    pub(crate) fn affected_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        match self {
            Self::CreateFolder { parent, .. } | Self::CreateFile { parent, .. } => {
                dirs.push(parent.clone());
            }
            Self::Rename { path, .. } => {
                if let Some(parent) = path.parent() {
                    dirs.push(parent.to_path_buf());
                }
            }
            Self::Trash { paths } => {
                for path in paths {
                    if let Some(parent) = path.parent() {
                        push_unique(&mut dirs, parent.to_path_buf());
                    }
                }
            }
            Self::Transfer {
                source, target_dir, ..
            } => {
                dirs.push(target_dir.clone());
                if let Some(parent) = source.parent() {
                    push_unique(&mut dirs, parent.to_path_buf());
                }
            }
        }
        dirs
    }

    fn validate_local_paths(&self) -> Result<(), String> {
        match self {
            Self::CreateFolder { parent, .. } | Self::CreateFile { parent, .. } => {
                ensure_privileged_local_path(parent)
            }
            Self::Rename { path, .. } => ensure_privileged_local_path(path),
            Self::Trash { paths } => {
                for path in paths {
                    ensure_privileged_local_path(path)?;
                }
                Ok(())
            }
            Self::Transfer {
                source, target_dir, ..
            } => {
                ensure_privileged_local_path(source)?;
                ensure_privileged_local_path(target_dir)
            }
        }
    }
}

fn ensure_privileged_local_path(path: &Path) -> Result<(), String> {
    if is_network_path(path) {
        Err(format!(
            "network locations are not supported by the privileged helper: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

#[proxy(
    interface = "org.fika.FileManager1.Privileged",
    default_service = "org.fika.FileManager1.Privileged",
    default_path = "/org/fika/FileManager1/Privileged"
)]
trait Privileged {
    #[zbus(name = "CreateFolder")]
    async fn create_folder(&self, parent: &str, name: &str) -> zbus::Result<String>;

    #[zbus(name = "CreateFile")]
    async fn create_file(&self, parent: &str, name: &str) -> zbus::Result<String>;

    #[zbus(name = "Rename")]
    async fn rename(&self, path: &str, new_name: &str) -> zbus::Result<String>;

    #[zbus(name = "Trash")]
    async fn trash(&self, paths: Vec<String>) -> zbus::Result<String>;

    #[zbus(name = "Transfer")]
    async fn transfer(
        &self,
        operation: &str,
        source: &str,
        target_dir: &str,
    ) -> zbus::Result<String>;

    #[zbus(name = "PrepareExternalEdit")]
    async fn prepare_external_edit(&self, path: &str) -> zbus::Result<(String, String)>;

    #[zbus(name = "CommitExternalEdit")]
    async fn commit_external_edit(&self, token: &str, scratch_path: &str) -> zbus::Result<String>;

    #[zbus(name = "DiscardExternalEdit")]
    async fn discard_external_edit(&self, token: &str) -> zbus::Result<()>;

    #[zbus(name = "AssociateExternalEditUnit")]
    async fn associate_external_edit_unit(
        &self,
        token: &str,
        unit: &str,
        session_bus_address: &str,
    ) -> zbus::Result<()>;
}

pub(crate) fn is_permission_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("permission denied")
        || error.contains("os error 13")
        || error.contains("operation not permitted")
        || error.contains("os error 1")
}

pub(crate) async fn run_via_dbus(command: PrivilegedCommand) -> PrivilegedOperationResult {
    let label = command.label().to_string();
    let affected_dirs = command.affected_dirs();
    let result = match command.validate_local_paths() {
        Ok(()) => run_via_dbus_inner(&command).await.map(|message| {
            if message.is_empty() {
                "completed with administrator privileges".to_string()
            } else {
                message
            }
        }),
        Err(err) => Err(err),
    };
    PrivilegedOperationResult {
        label,
        affected_dirs,
        result,
    }
}

async fn run_via_dbus_inner(command: &PrivilegedCommand) -> Result<String, String> {
    match call_dbus_command_on_system_bus(command).await {
        Ok(message) => Ok(message),
        Err(system_error) => match call_dbus_command_on_session_bus(command).await {
            Ok(message) => Ok(message),
            Err(session_error) => {
                start_session_helper_and_call(command, system_error, session_error).await
            }
        },
    }
}

async fn start_session_helper_and_call(
    command: &PrivilegedCommand,
    system_error: String,
    session_error: String,
) -> Result<String, String> {
    let mut helper = start_dbus_helper().map_err(|err| {
        privileged_helper_start_failed_message(&system_error, &session_error, &err)
    })?;
    let wait_result = wait_for_service().await;
    if wait_result.is_err() {
        let _ = helper.try_wait();
    }
    wait_result.map_err(|err| {
        privileged_helper_start_failed_message(&system_error, &session_error, &err)
    })?;
    call_dbus_command_on_session_bus(command).await
}

async fn call_dbus_command_on_system_bus(command: &PrivilegedCommand) -> Result<String, String> {
    let connection = privileged_bus_connection(BusKind::System).await?;
    call_dbus_command(command, &connection).await
}

async fn call_dbus_command_on_session_bus(command: &PrivilegedCommand) -> Result<String, String> {
    let connection = privileged_bus_connection(BusKind::Session).await?;
    call_dbus_command(command, &connection).await
}

async fn call_dbus_command(
    command: &PrivilegedCommand,
    connection: &Connection,
) -> Result<String, String> {
    command.validate_local_paths()?;
    with_bus_tokio_context(async move {
        let proxy = PrivilegedProxy::new(connection)
            .await
            .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;

        match command {
            PrivilegedCommand::CreateFolder { parent, name } => proxy
                .create_folder(&parent.display().to_string(), name)
                .await
                .map_err(|err| err.to_string()),
            PrivilegedCommand::CreateFile { parent, name } => proxy
                .create_file(&parent.display().to_string(), name)
                .await
                .map_err(|err| err.to_string()),
            PrivilegedCommand::Rename { path, new_name } => proxy
                .rename(&path.display().to_string(), new_name)
                .await
                .map_err(|err| err.to_string()),
            PrivilegedCommand::Trash { paths } => proxy
                .trash(
                    paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect(),
                )
                .await
                .map_err(|err| err.to_string()),
            PrivilegedCommand::Transfer {
                operation,
                source,
                target_dir,
            } => proxy
                .transfer(
                    operation,
                    &source.display().to_string(),
                    &target_dir.display().to_string(),
                )
                .await
                .map_err(|err| err.to_string()),
        }
    })
    .await
}

async fn prepare_external_edit_via_system_bus(path: &Path) -> Result<ExternalEditSession, String> {
    let connection = privileged_bus_connection(BusKind::System).await?;
    prepare_external_edit_call(&connection, path).await
}

async fn prepare_external_edit_via_session_bus(path: &Path) -> Result<ExternalEditSession, String> {
    let connection = privileged_bus_connection(BusKind::Session).await?;
    prepare_external_edit_call(&connection, path).await
}

pub(crate) async fn prepare_external_edit_via_dbus(
    path: PathBuf,
) -> Result<ExternalEditSession, String> {
    ensure_privileged_local_path(&path)?;
    match prepare_external_edit_via_system_bus(&path).await {
        Ok(session) => Ok(session),
        Err(system_error) => match prepare_external_edit_via_session_bus(&path).await {
            Ok(session) => Ok(session),
            Err(session_error) => {
                let mut helper = start_dbus_helper().map_err(|err| {
                    privileged_helper_start_failed_message(&system_error, &session_error, &err)
                })?;
                let wait_result = wait_for_service().await;
                if wait_result.is_err() {
                    let _ = helper.try_wait();
                }
                wait_result.map_err(|err| {
                    privileged_helper_start_failed_message(&system_error, &session_error, &err)
                })?;
                prepare_external_edit_via_session_bus(&path).await
            }
        },
    }
}

pub(crate) async fn commit_external_edit_via_dbus(
    session: &ExternalEditSession,
) -> Result<PathBuf, String> {
    let system_result = async {
        let connection = privileged_bus_connection(BusKind::System).await?;
        commit_external_edit_call(session, &connection).await
    }
    .await;
    match system_result {
        Ok(path) => Ok(path),
        Err(system_error) => {
            let connection = privileged_bus_connection(BusKind::Session)
                .await
                .map_err(|err| format!("{err}; system bus call failed: {system_error}"))?;
            commit_external_edit_call(session, &connection).await
        }
    }
}

pub(crate) async fn discard_external_edit_via_dbus(
    session: &ExternalEditSession,
) -> Result<PathBuf, String> {
    let system_result = async {
        let connection = privileged_bus_connection(BusKind::System).await?;
        discard_external_edit_call(session, &connection).await
    }
    .await;
    match system_result {
        Ok(path) => Ok(path),
        Err(system_error) => {
            let connection = privileged_bus_connection(BusKind::Session)
                .await
                .map_err(|err| format!("{err}; system bus call failed: {system_error}"))?;
            discard_external_edit_call(session, &connection).await
        }
    }
}

pub(crate) async fn associate_external_edit_unit_via_dbus(
    session: &ExternalEditSession,
) -> Result<(), String> {
    let Some(unit) = session.unit.as_deref() else {
        return Ok(());
    };
    let session_bus_address = env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    let system_result = async {
        let connection = privileged_bus_connection(BusKind::System).await?;
        associate_external_edit_unit_call(session, unit, &session_bus_address, &connection).await
    }
    .await;
    match system_result {
        Ok(()) => Ok(()),
        Err(system_error) => {
            let connection = privileged_bus_connection(BusKind::Session)
                .await
                .map_err(|err| format!("{err}; system bus call failed: {system_error}"))?;
            associate_external_edit_unit_call(session, unit, &session_bus_address, &connection)
                .await
        }
    }
}

async fn commit_external_edit_call(
    session: &ExternalEditSession,
    connection: &Connection,
) -> Result<PathBuf, String> {
    with_bus_tokio_context(async move {
        let proxy = PrivilegedProxy::new(connection)
            .await
            .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;
        proxy
            .commit_external_edit(
                &session.token,
                session.scratch_path.display().to_string().as_str(),
            )
            .await
            .map(PathBuf::from)
            .map_err(|err| err.to_string())
    })
    .await
}

async fn discard_external_edit_call(
    session: &ExternalEditSession,
    connection: &Connection,
) -> Result<PathBuf, String> {
    with_bus_tokio_context(async move {
        let proxy = PrivilegedProxy::new(connection)
            .await
            .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;
        proxy
            .discard_external_edit(&session.token)
            .await
            .map(|()| session.original_path.clone())
            .map_err(|err| err.to_string())
    })
    .await
}

async fn associate_external_edit_unit_call(
    session: &ExternalEditSession,
    unit: &str,
    session_bus_address: &str,
    connection: &Connection,
) -> Result<(), String> {
    with_bus_tokio_context(async move {
        let proxy = PrivilegedProxy::new(connection)
            .await
            .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;
        proxy
            .associate_external_edit_unit(&session.token, unit, session_bus_address)
            .await
            .map_err(|err| err.to_string())
    })
    .await
}

async fn prepare_external_edit_call(
    connection: &Connection,
    path: &Path,
) -> Result<ExternalEditSession, String> {
    ensure_privileged_local_path(path)?;
    with_bus_tokio_context(async move {
        let proxy = PrivilegedProxy::new(connection)
            .await
            .map_err(|err| format!("cannot create privileged helper proxy: {err}"))?;
        let (scratch_path, token) = proxy
            .prepare_external_edit(path.display().to_string().as_str())
            .await
            .map_err(|err| err.to_string())?;
        Ok(ExternalEditSession {
            original_path: path.to_path_buf(),
            scratch_path: PathBuf::from(scratch_path),
            token,
            unit: None,
        })
    })
    .await
}

async fn wait_for_service() -> Result<(), String> {
    with_bus_tokio_context(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(75);
        let service_name: BusName<'_> = SERVICE_NAME
            .try_into()
            .map_err(|err| format!("invalid privileged helper bus name: {err}"))?;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err("timed out waiting for privileged D-Bus helper".to_string());
            }
            if let Ok(connection) = privileged_bus_connection(BusKind::Session).await
                && let Ok(dbus) = DBusProxy::new(&connection).await
                && dbus.get_name_owner(service_name.clone()).await.is_ok()
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
}

async fn privileged_bus_connection(kind: BusKind) -> Result<Connection, String> {
    BusController::shared()
        .connection(kind)
        .await
        .map_err(|err| err.to_string())
}

fn start_dbus_helper() -> Result<Child, String> {
    let exe = helper_executable()?;
    let mut command = Command::new("pkexec");
    command.arg("--disable-internal-agent").arg(exe);
    if let Ok(address) = env::var("DBUS_SESSION_BUS_ADDRESS") {
        command.arg("--session-bus").arg(address);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("cannot start pkexec: {err}"))
}

fn helper_executable() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("FIKA_PRIVILEGED_HELPER") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().map_err(|err| format!("cannot locate executable: {err}"))?;
    let Some(dir) = exe.parent() else {
        return Err(format!(
            "cannot locate helper executable next to {}",
            exe.display()
        ));
    };
    Ok(dir.join("fika-privileged-helper"))
}

pub async fn run_dbus_service(bus: HelperBus) -> Result<(), String> {
    let (service_mode, session_bus_address) = match bus {
        HelperBus::System => (ServiceMode::System, None),
        HelperBus::Session {
            session_bus_address,
        } => {
            let allowed_uid = env::var("PKEXEC_UID")
                .ok()
                .and_then(|uid| uid.parse::<u32>().ok())
                .ok_or_else(|| "refusing to start session helper without PKEXEC_UID".to_string())?;
            (
                ServiceMode::SessionPkexec { allowed_uid },
                session_bus_address,
            )
        }
    };
    privileged_debug_log(&helper_lifecycle_summary(
        "starting",
        &service_mode,
        &session_bus_address,
        0,
        0,
    ));
    let service = PrivilegedService::new(service_mode, session_bus_address.clone());
    let service_monitor = service.clone();
    let builder = match bus_connection_address(&session_bus_address, &service_monitor.mode) {
        BusConnection::System => zbus::connection::Builder::system()
            .map_err(|err| format!("cannot connect to system D-Bus: {err}"))?,
        BusConnection::SessionAddress(address) => zbus::connection::Builder::address(address)
            .map_err(|err| format!("cannot connect to provided session D-Bus address: {err}"))?,
        BusConnection::Session => zbus::connection::Builder::session()
            .map_err(|err| format!("cannot connect to session D-Bus: {err}"))?,
    };
    let _connection = builder
        .name(SERVICE_NAME)
        .map_err(|err| format!("cannot request privileged helper name: {err}"))?
        .serve_at(OBJECT_PATH, service)
        .map_err(|err| format!("cannot register privileged helper object: {err}"))?
        .build()
        .await
        .map_err(|err| format!("cannot build privileged helper D-Bus service: {err}"))?;

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        service_monitor.expire_stale_external_edits();
        if service_monitor.can_exit() {
            let (idle_for, active_edits) = service_monitor.exit_state();
            privileged_debug_log(&helper_lifecycle_summary(
                "exiting",
                &service_monitor.mode,
                &service_monitor.session_bus_address,
                idle_for,
                active_edits,
            ));
            break;
        }
    }
    Ok(())
}

enum BusConnection<'a> {
    System,
    SessionAddress(&'a str),
    Session,
}

fn bus_connection_address<'a>(
    session_bus_address: &'a Option<String>,
    mode: &ServiceMode,
) -> BusConnection<'a> {
    match mode {
        ServiceMode::System => BusConnection::System,
        ServiceMode::SessionPkexec { .. } => session_bus_address
            .as_deref()
            .map(BusConnection::SessionAddress)
            .unwrap_or(BusConnection::Session),
    }
}

fn helper_lifecycle_summary(
    phase: &str,
    mode: &ServiceMode,
    session_bus_address: &Option<String>,
    idle_for: u64,
    active_edits: usize,
) -> String {
    let (mode_label, authorized_subject) = match mode {
        ServiceMode::System => ("system-bus", "polkit".to_string()),
        ServiceMode::SessionPkexec { allowed_uid } => {
            ("session-bus-pkexec", format!("uid:{allowed_uid}"))
        }
    };
    let bus_connection = match bus_connection_address(session_bus_address, mode) {
        BusConnection::System => "system",
        BusConnection::SessionAddress(_) => "provided-session",
        BusConnection::Session => "session",
    };
    format!(
        "phase={phase} mode={mode_label} bus_connection={bus_connection} authorized_subject={authorized_subject} session_address={} idle_for={} active_external_edits={active_edits}",
        session_bus_address.is_some(),
        idle_for
    )
}

fn privileged_debug_log(message: &str) {
    static DEBUG_PRIVILEGE: OnceLock<bool> = OnceLock::new();
    if *DEBUG_PRIVILEGE.get_or_init(|| {
        env::var("FIKA_DEBUG_PRIVILEGE").is_ok_and(|value| env_flag_is_truthy(value.as_str()))
    }) {
        eprintln!("[fika privileged helper] {message}");
    }
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Clone, Debug)]
struct ExternalEdit {
    original_path: PathBuf,
    scratch_path: PathBuf,
    original_len: u64,
    original_modified: Option<SystemTime>,
    unit: Option<String>,
    session_bus_address: Option<String>,
    created_secs: u64,
}

#[derive(Clone)]
struct PrivilegedService {
    mode: ServiceMode,
    external_edits: Arc<Mutex<std::collections::HashMap<String, ExternalEdit>>>,
    external_edit_watchers:
        Arc<Mutex<std::collections::HashMap<String, notify::RecommendedWatcher>>>,
    last_activity_secs: Arc<AtomicU64>,
    scratch_root_override: Option<PathBuf>,
    session_bus_address: Option<String>,
}

impl PrivilegedService {
    fn new(mode: ServiceMode, session_bus_address: Option<String>) -> Self {
        Self {
            mode,
            external_edits: Arc::new(Mutex::new(std::collections::HashMap::new())),
            external_edit_watchers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_activity_secs: Arc::new(AtomicU64::new(now_secs())),
            scratch_root_override: None,
            session_bus_address,
        }
    }

    #[cfg(test)]
    fn new_for_tests(scratch_root: PathBuf) -> Self {
        Self {
            mode: ServiceMode::SessionPkexec { allowed_uid: 0 },
            external_edits: Arc::new(Mutex::new(std::collections::HashMap::new())),
            external_edit_watchers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_activity_secs: Arc::new(AtomicU64::new(now_secs())),
            scratch_root_override: Some(scratch_root),
            session_bus_address: None,
        }
    }

    async fn authorize(&self, connection: &Connection, header: Header<'_>) -> fdo::Result<u32> {
        match &self.mode {
            ServiceMode::System => self.authorize_with_polkit(connection, header).await,
            ServiceMode::SessionPkexec { allowed_uid } => {
                let caller_uid = caller_uid(connection, &header).await?;
                if caller_uid == *allowed_uid {
                    self.mark_activity();
                    Ok(caller_uid)
                } else {
                    Err(fdo::Error::AccessDenied(format!(
                        "caller uid {caller_uid} does not match authorized uid {allowed_uid}"
                    )))
                }
            }
        }
    }

    async fn authorize_with_polkit(
        &self,
        connection: &Connection,
        header: Header<'_>,
    ) -> fdo::Result<u32> {
        let caller_uid = caller_uid(connection, &header).await?;
        let subject = Subject::new_for_message_header(&header).map_err(|err| {
            fdo::Error::AccessDenied(format!("cannot create polkit subject: {err}"))
        })?;
        let authority = AuthorityProxy::new(connection).await.map_err(|err| {
            fdo::Error::Failed(polkit_authority_unavailable_message(&err.to_string()))
        })?;
        let details = std::collections::HashMap::new();
        let result = authority
            .check_authorization(
                &subject,
                ACTION_ID,
                &details,
                CheckAuthorizationFlags::AllowUserInteraction.into(),
                "",
            )
            .await
            .map_err(|err| {
                fdo::Error::AccessDenied(polkit_check_failed_message(&err.to_string()))
            })?;
        if result.is_authorized {
            self.mark_activity();
            Ok(caller_uid)
        } else {
            Err(fdo::Error::AccessDenied(polkit_denied_message()))
        }
    }

    fn map_result<T>(result: Result<T, String>) -> fdo::Result<T> {
        result.map_err(fdo::Error::Failed)
    }

    fn mark_activity(&self) {
        self.last_activity_secs.store(now_secs(), Ordering::Relaxed);
    }

    fn can_exit(&self) -> bool {
        let (idle_for, active_edits) = self.exit_state();
        idle_for >= HELPER_IDLE_SECONDS && active_edits == 0
    }

    fn exit_state(&self) -> (u64, usize) {
        let idle_for = now_secs().saturating_sub(self.last_activity_secs.load(Ordering::Relaxed));
        let active_edits = self.external_edits.lock().map_or(0, |edits| edits.len());
        (idle_for, active_edits)
    }

    fn prepare_external_edit_inner(
        &self,
        path: PathBuf,
        authorized_uid: u32,
    ) -> Result<(String, String), String> {
        if !path.is_file() {
            return Err("external edit target is not a regular file".to_string());
        }
        let metadata = fs::metadata(&path).map_err(|err| err.to_string())?;
        let token = new_token();
        let scratch_dir = self.scratch_root(authorized_uid)?.join(&token);
        fs::create_dir_all(&scratch_dir).map_err(|err| err.to_string())?;
        self.chown_for_authorized_user(&scratch_dir, authorized_uid)?;

        let file_name = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("edit"));
        let scratch_path = scratch_dir.join(file_name);
        fs::copy(&path, &scratch_path).map_err(|err| err.to_string())?;
        self.set_private_user_file(&scratch_path, authorized_uid)?;

        let edit = ExternalEdit {
            original_path: path,
            scratch_path: scratch_path.clone(),
            original_len: metadata.len(),
            original_modified: metadata.modified().ok(),
            unit: None,
            session_bus_address: None,
            created_secs: now_secs(),
        };
        self.external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .insert(token.clone(), edit);
        if self.scratch_root_override.is_none() {
            self.watch_external_edit(token.clone(), scratch_path.clone())?;
        }

        Ok((scratch_path.display().to_string(), token))
    }

    fn commit_external_edit_inner(
        &self,
        token: &str,
        scratch_path: PathBuf,
    ) -> Result<String, String> {
        let mut edit = self
            .external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "unknown external edit token".to_string())?;

        if edit.scratch_path != scratch_path {
            return Err("scratch path does not match edit token".to_string());
        }
        let original_path = sync_external_edit(&mut edit)?;
        let _ = self
            .external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .remove(token);
        let _ = cleanup_scratch_token_dir(&scratch_path);
        Ok(original_path.display().to_string())
    }

    fn discard_external_edit_inner(&self, token: &str) -> Result<(), String> {
        let edit = self
            .external_edits
            .lock()
            .map_err(|_| "external edit state is poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "unknown external edit token".to_string())?;
        let _ = self
            .external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .remove(token);
        let _ = cleanup_scratch_token_dir(&edit.scratch_path);
        Ok(())
    }

    fn associate_external_edit_unit_inner(
        &self,
        token: &str,
        unit: String,
        session_bus_address: Option<String>,
    ) -> Result<(), String> {
        let session_bus_address = session_bus_address.or_else(|| self.session_bus_address.clone());
        {
            let mut edits = self
                .external_edits
                .lock()
                .map_err(|_| "external edit state is poisoned".to_string())?;
            let edit = edits
                .get_mut(token)
                .ok_or_else(|| "unknown external edit token".to_string())?;
            edit.unit = Some(unit.clone());
            edit.session_bus_address = session_bus_address.clone();
        }
        self.watch_external_edit_unit(token.to_string(), unit, session_bus_address);
        Ok(())
    }

    fn scratch_root(&self, authorized_uid: u32) -> Result<PathBuf, String> {
        if let Some(root) = &self.scratch_root_override {
            fs::create_dir_all(root).map_err(|err| err.to_string())?;
            return Ok(root.clone());
        }

        let root = PathBuf::from(format!("/run/user/{authorized_uid}"));
        let scratch_root = root.join("fika-edit");
        fs::create_dir_all(&scratch_root).map_err(|err| err.to_string())?;
        self.chown_for_authorized_user(&scratch_root, authorized_uid)?;
        Ok(scratch_root)
    }

    fn chown_for_authorized_user(&self, path: &Path, authorized_uid: u32) -> Result<(), String> {
        if self.scratch_root_override.is_some() {
            return Ok(());
        }
        set_owner_for_authorized_user(path, authorized_uid)
    }

    fn set_private_user_file(&self, path: &Path, authorized_uid: u32) -> Result<(), String> {
        if self.scratch_root_override.is_some() {
            return set_private_user_file(path);
        }
        set_owner_for_authorized_user(path, authorized_uid)?;
        set_private_user_file(path)
    }

    fn watch_external_edit(&self, token: String, scratch_path: PathBuf) -> Result<(), String> {
        use notify::Watcher;

        let edits = Arc::clone(&self.external_edits);
        let token_for_callback = token.clone();
        let scratch_for_callback = scratch_path.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                if !event.paths.iter().any(|path| path == &scratch_for_callback) {
                    return;
                }
                if !is_writeback_event(&event.kind) {
                    return;
                }

                let edits = Arc::clone(&edits);
                let token = token_for_callback.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(350));
                    let Ok(mut edits) = edits.lock() else {
                        return;
                    };
                    if let Some(edit) = edits.get_mut(&token)
                        && let Err(err) = sync_external_edit(edit)
                    {
                        eprintln!("[fika privileged helper] external edit writeback failed: {err}");
                    }
                });
            })
            .map_err(|err| err.to_string())?;
        watcher
            .watch(&scratch_path, notify::RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;
        self.external_edit_watchers
            .lock()
            .map_err(|_| "external edit watcher state is poisoned".to_string())?
            .insert(token, watcher);
        Ok(())
    }

    fn watch_external_edit_unit(
        &self,
        token: String,
        unit: String,
        session_bus_address: Option<String>,
    ) {
        let edits = Arc::clone(&self.external_edits);
        let watchers = Arc::clone(&self.external_edit_watchers);
        std::thread::spawn(move || {
            wait_for_user_unit_to_finish(&unit, session_bus_address.as_deref());

            let Ok(mut edits) = edits.lock() else {
                return;
            };
            let Some(mut edit) = edits.remove(&token) else {
                return;
            };
            if let Err(err) = sync_external_edit(&mut edit) {
                eprintln!("[fika privileged helper] final external edit writeback failed: {err}");
            }
            let _ = cleanup_scratch_token_dir(&edit.scratch_path);
            drop(edits);

            if let Ok(mut watchers) = watchers.lock() {
                let _ = watchers.remove(&token);
            }
        });
    }

    fn expire_stale_external_edits(&self) {
        let cutoff = now_secs().saturating_sub(EXTERNAL_EDIT_TTL_SECONDS);
        let expired = {
            let Ok(edits) = self.external_edits.lock() else {
                return;
            };
            edits
                .iter()
                .filter(|(_, edit)| edit.created_secs <= cutoff)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>()
        };

        for token in expired {
            let edit = {
                let Ok(mut edits) = self.external_edits.lock() else {
                    return;
                };
                edits.remove(&token)
            };
            let Some(mut edit) = edit else {
                continue;
            };
            if let Err(err) = sync_external_edit(&mut edit) {
                eprintln!("[fika privileged helper] expired edit final writeback failed: {err}");
            }
            let _ = cleanup_scratch_token_dir(&edit.scratch_path);
            if let Ok(mut watchers) = self.external_edit_watchers.lock() {
                let _ = watchers.remove(&token);
            }
        }
    }
}

async fn caller_uid(connection: &Connection, header: &Header<'_>) -> fdo::Result<u32> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("missing D-Bus sender".to_string()))?;
    let proxy = DBusProxy::new(connection)
        .await
        .map_err(|err| fdo::Error::Failed(format!("cannot query D-Bus credentials: {err}")))?;
    proxy
        .get_connection_unix_user(BusName::from(sender.clone()))
        .await
        .map_err(|err| fdo::Error::Failed(format!("cannot query caller uid: {err}")))
}

#[zbus::interface(name = "org.fika.FileManager1.Privileged")]
impl PrivilegedService {
    #[zbus(name = "CreateFolder")]
    async fn create_folder(
        &self,
        parent: String,
        name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&parent)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::create_folder(Path::new(&parent), &name)
                .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "CreateFile")]
    async fn create_file(
        &self,
        parent: String,
        name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&parent)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::create_file(Path::new(&parent), &name).map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "Rename")]
    async fn rename(
        &self,
        path: String,
        new_name: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&path)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::rename_path(Path::new(&path), &new_name)
                .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "Trash")]
    async fn trash(
        &self,
        paths: Vec<String>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        for path in &paths {
            ensure_privileged_local_path(path).map_err(fdo::Error::Failed)?;
        }
        Self::map_result(file_ops::trash_paths(&paths).to_result_message("moved to trash"))
    }

    #[zbus(name = "Transfer")]
    async fn transfer(
        &self,
        operation: String,
        source: String,
        target_dir: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&source)).map_err(fdo::Error::Failed)?;
        ensure_privileged_local_path(Path::new(&target_dir)).map_err(fdo::Error::Failed)?;
        Self::map_result(
            file_ops::perform_transfer_with_progress(
                &operation,
                Path::new(&source),
                Path::new(&target_dir),
                "keep-both",
                None,
                |_| {},
            )
            .map(|path| path.display().to_string()),
        )
    }

    #[zbus(name = "PrepareExternalEdit")]
    async fn prepare_external_edit(
        &self,
        path: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(String, String)> {
        let authorized_uid = self.authorize(connection, header).await?;
        ensure_privileged_local_path(Path::new(&path)).map_err(fdo::Error::Failed)?;
        Self::map_result(self.prepare_external_edit_inner(PathBuf::from(path), authorized_uid))
    }

    #[zbus(name = "CommitExternalEdit")]
    async fn commit_external_edit(
        &self,
        token: String,
        scratch_path: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let _authorized_uid = self.authorize(connection, header).await?;
        Self::map_result(self.commit_external_edit_inner(&token, PathBuf::from(scratch_path)))
    }

    #[zbus(name = "DiscardExternalEdit")]
    async fn discard_external_edit(
        &self,
        token: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _authorized_uid = self.authorize(connection, header).await?;
        Self::map_result(self.discard_external_edit_inner(&token))
    }

    #[zbus(name = "AssociateExternalEditUnit")]
    async fn associate_external_edit_unit(
        &self,
        token: String,
        unit: String,
        session_bus_address: String,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let _authorized_uid = self.authorize(connection, header).await?;
        let session_bus_address = if session_bus_address.is_empty() {
            None
        } else {
            Some(session_bus_address)
        };
        Self::map_result(self.associate_external_edit_unit_inner(&token, unit, session_bus_address))
    }
}

fn new_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{counter:x}", std::process::id())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn set_owner_for_authorized_user(path: &Path, uid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let gid = fs::metadata(format!("/run/user/{uid}"))
            .map(|metadata| metadata.gid())
            .unwrap_or(uid);
        std::os::unix::fs::chown(path, Some(uid), Some(gid)).map_err(|err| err.to_string())?;
    }

    let _ = path;
    let _ = uid;
    Ok(())
}

fn set_private_user_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|err| err.to_string())?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn is_writeback_event(kind: &notify::EventKind) -> bool {
    matches!(
        kind,
        notify::EventKind::Modify(_) | notify::EventKind::Create(_) | notify::EventKind::Any
    )
}

fn sync_external_edit(edit: &mut ExternalEdit) -> Result<PathBuf, String> {
    if !edit.scratch_path.is_file() {
        return Err("scratch file no longer exists".to_string());
    }

    let current = fs::metadata(&edit.original_path).map_err(|err| err.to_string())?;
    if current.len() != edit.original_len || current.modified().ok() != edit.original_modified {
        return Err("original file changed outside this edit session".to_string());
    }

    let data = fs::read(&edit.scratch_path).map_err(|err| err.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&edit.original_path)
        .map_err(|err| err.to_string())?;
    file.write_all(&data).map_err(|err| err.to_string())?;
    file.sync_all().map_err(|err| err.to_string())?;

    let metadata = fs::metadata(&edit.original_path).map_err(|err| err.to_string())?;
    edit.original_len = metadata.len();
    edit.original_modified = metadata.modified().ok();
    Ok(edit.original_path.clone())
}

fn wait_for_user_unit_to_finish(unit: &str, session_bus_address: Option<&str>) {
    let unit = unit.to_string();
    let session_bus_address = session_bus_address.map(str::to_string);
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(wait_for_user_unit_to_finish_async(
            &unit,
            session_bus_address.as_deref(),
        )),
        Err(err) => eprintln!(
            "[fika privileged helper] cannot initialize Tokio unit watcher for {unit}: {err}"
        ),
    }
}

async fn wait_for_user_unit_to_finish_async(unit: &str, session_bus_address: Option<&str>) {
    if let Err(err) = wait_for_user_unit_to_finish_by_signal(unit, session_bus_address).await {
        eprintln!(
            "[fika privileged helper] cannot subscribe to unit {unit} lifecycle, falling back to polling: {err}"
        );
        wait_for_user_unit_to_finish_by_poll(unit, session_bus_address).await;
    }
}

async fn wait_for_user_unit_to_finish_by_signal(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<(), String> {
    let connection = user_bus_connection(session_bus_address).await?;
    let manager = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let _: () = manager
        .call("Subscribe", &())
        .await
        .map_err(|err| format!("Subscribe failed: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(()),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    let unit_proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .map_err(|err| format!("cannot create systemd unit proxy: {err}"))?;

    let deadline = unit_watch_deadline();
    let mut active_state_changes = unit_proxy
        .receive_property_changed::<String>("ActiveState")
        .await;
    loop {
        if SystemTime::now() >= deadline {
            eprintln!("[fika privileged helper] external edit unit watch timed out: {unit}");
            return Ok(());
        }

        let Some(change) = active_state_changes.next().await else {
            return Ok(());
        };
        match change.get().await {
            Ok(state) if is_finished_unit_state(&state) => return Ok(()),
            Ok(_) => {}
            Err(err) => return Err(format!("ActiveState signal read failed: {err}")),
        }
    }
}

async fn wait_for_user_unit_to_finish_by_poll(unit: &str, session_bus_address: Option<&str>) {
    let deadline = SystemTime::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or_else(SystemTime::now);
    loop {
        if SystemTime::now() >= deadline {
            eprintln!("[fika privileged helper] external edit unit watch timed out: {unit}");
            return;
        }

        match user_unit_active_state(unit, session_bus_address).await {
            Ok(Some(state)) if is_finished_unit_state(&state) => return,
            Ok(None) => return,
            Ok(Some(_)) => {}
            Err(err) => eprintln!("[fika privileged helper] cannot query unit {unit}: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn unit_watch_deadline() -> SystemTime {
    SystemTime::now()
        .checked_add(Duration::from_secs(24 * 60 * 60))
        .unwrap_or_else(SystemTime::now)
}

async fn user_unit_active_state(
    unit: &str,
    session_bus_address: Option<&str>,
) -> Result<Option<String>, String> {
    let connection = user_bus_connection(session_bus_address).await?;
    let manager = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|err| format!("cannot create systemd manager proxy: {err}"))?;
    let unit_path: OwnedObjectPath = match manager.call("GetUnit", &(unit)).await {
        Ok(path) => path,
        Err(err) if err.to_string().contains("NoSuchUnit") => return Ok(None),
        Err(err) => return Err(format!("GetUnit failed: {err}")),
    };
    let properties = zbus::Proxy::new(
        &connection,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(|err| format!("cannot create systemd unit properties proxy: {err}"))?;
    let value: OwnedValue = properties
        .call("Get", &("org.freedesktop.systemd1.Unit", "ActiveState"))
        .await
        .map_err(|err| format!("Get ActiveState failed: {err}"))?;
    String::try_from(value)
        .map(Some)
        .map_err(|err| format!("ActiveState was not a string: {err}"))
}

async fn user_bus_connection(session_bus_address: Option<&str>) -> Result<Connection, String> {
    match session_bus_address {
        Some(address) => zbus::connection::Builder::address(address)
            .map_err(|err| format!("cannot use provided session bus address: {err}"))?
            .build()
            .await
            .map_err(|err| format!("cannot connect to provided session bus: {err}")),
        None => Connection::session()
            .await
            .map_err(|err| format!("cannot connect to session bus: {err}")),
    }
}

fn is_finished_unit_state(state: &str) -> bool {
    matches!(state, "inactive" | "failed")
}

fn cleanup_scratch_token_dir(scratch_path: &Path) -> Result<(), String> {
    let Some(token_dir) = scratch_path.parent() else {
        return Err("scratch path has no token directory".to_string());
    };
    let Some(root_dir) = token_dir.parent() else {
        return Err("scratch token directory has no root".to_string());
    };
    if root_dir.file_name() != Some(std::ffi::OsStr::new("fika-edit")) {
        return Err("scratch path is outside fika-edit".to_string());
    }
    fs::remove_dir_all(token_dir).map_err(|err| err.to_string())
}

fn polkit_authority_unavailable_message(err: &str) -> String {
    format!(
        "cannot contact polkit authority for action {ACTION_ID}: {err}; ensure polkit is running, {POLICY_FILE} is installed, and a desktop polkit agent is available"
    )
}

fn polkit_check_failed_message(err: &str) -> String {
    format!(
        "polkit authorization failed for action {ACTION_ID}: {err}; ensure {POLICY_FILE} is installed in the polkit actions directory"
    )
}

fn polkit_denied_message() -> String {
    format!("polkit denied authorization for action {ACTION_ID}")
}

fn privileged_helper_start_failed_message(
    system_error: &str,
    session_error: &str,
    fallback_error: &str,
) -> String {
    format!(
        "cannot reach privileged helper. \
         System bus activation failed: {system_error}. \
         Development session-bus helper failed: {session_error}. \
         pkexec fallback failed: {fallback_error}. \
         Install Fika's D-Bus service and {POLICY_FILE}, then ensure a desktop polkit agent is running."
    )
}

pub(crate) fn run_helper(args: &[String]) -> Result<String, String> {
    let Some(operation) = args.first().map(String::as_str) else {
        return Err("missing privileged helper operation".to_string());
    };

    match operation {
        "create-folder" => {
            let [_, parent, name] = args else {
                return Err("create-folder expects parent and name".to_string());
            };
            ensure_privileged_local_path(Path::new(parent))?;
            file_ops::create_folder(Path::new(parent), name).map(|path| path.display().to_string())
        }
        "create-file" => {
            let [_, parent, name] = args else {
                return Err("create-file expects parent and name".to_string());
            };
            ensure_privileged_local_path(Path::new(parent))?;
            file_ops::create_file(Path::new(parent), name).map(|path| path.display().to_string())
        }
        "rename" => {
            let [_, path, new_name] = args else {
                return Err("rename expects path and new name".to_string());
            };
            ensure_privileged_local_path(Path::new(path))?;
            file_ops::rename_path(Path::new(path), new_name).map(|path| path.display().to_string())
        }
        "trash" => {
            if args.len() < 2 {
                return Err("trash expects at least one path".to_string());
            }
            let paths = args[1..]
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<PathBuf>>();
            for path in &paths {
                ensure_privileged_local_path(path)?;
            }
            file_ops::trash_paths(&paths).to_result_message("moved to trash")
        }
        "transfer" => {
            let [_, operation, source, target_dir] = args else {
                return Err("transfer expects operation, source and target directory".to_string());
            };
            ensure_privileged_local_path(Path::new(source))?;
            ensure_privileged_local_path(Path::new(target_dir))?;
            file_ops::perform_transfer_with_progress(
                operation,
                Path::new(source),
                Path::new(target_dir),
                "keep-both",
                None,
                |_| {},
            )
            .map(|path| path.display().to_string())
        }
        _ => Err(format!("unknown privileged helper operation: {operation}")),
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

trait SummaryMessage {
    fn to_result_message(self, success_label: &str) -> Result<String, String>;
}

impl SummaryMessage for file_ops::FileActionSummary {
    fn to_result_message(self, success_label: &str) -> Result<String, String> {
        match (self.successes.len(), self.failures.is_empty()) {
            (0, false) => Err(self.failures.join("; ")),
            (count, true) => Ok(format!("{count} item(s) {success_label}")),
            (count, false) => Ok(format!(
                "{count} item(s) {success_label}; {} failure(s): {}",
                self.failures.len(),
                self.failures.join("; ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_edit_commits_scratch_back_and_cleans_token_dir() {
        let temp = test_dir("commit");
        let original_dir = temp.join("original");
        let scratch_root = temp.join("fika-edit");
        fs::create_dir_all(&original_dir).unwrap();
        let original = original_dir.join("config.txt");
        fs::write(&original, "old").unwrap();

        let service = PrivilegedService::new_for_tests(scratch_root);
        let (scratch_path, token) = service
            .prepare_external_edit_inner(original.clone(), 0)
            .unwrap();
        let scratch_path = PathBuf::from(scratch_path);
        fs::write(&scratch_path, "new").unwrap();

        let committed = service
            .commit_external_edit_inner(&token, scratch_path.clone())
            .unwrap();

        assert_eq!(committed, original.display().to_string());
        assert_eq!(fs::read_to_string(&original).unwrap(), "new");
        assert!(!scratch_path.parent().unwrap().exists());
        assert!(service.external_edits.lock().unwrap().is_empty());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn external_edit_rejects_changed_original() {
        let temp = test_dir("changed-original");
        let original_dir = temp.join("original");
        let scratch_root = temp.join("fika-edit");
        fs::create_dir_all(&original_dir).unwrap();
        let original = original_dir.join("config.txt");
        fs::write(&original, "old").unwrap();

        let service = PrivilegedService::new_for_tests(scratch_root);
        let (scratch_path, token) = service
            .prepare_external_edit_inner(original.clone(), 0)
            .unwrap();
        fs::write(&original, "changed elsewhere").unwrap();

        let error = service
            .commit_external_edit_inner(&token, PathBuf::from(scratch_path))
            .unwrap_err();

        assert!(error.contains("outside this edit session"));
        assert_eq!(fs::read_to_string(&original).unwrap(), "changed elsewhere");

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn external_edit_discard_removes_scratch() {
        let temp = test_dir("discard");
        let original_dir = temp.join("original");
        let scratch_root = temp.join("fika-edit");
        fs::create_dir_all(&original_dir).unwrap();
        let original = original_dir.join("config.txt");
        fs::write(&original, "old").unwrap();

        let service = PrivilegedService::new_for_tests(scratch_root);
        let (scratch_path, token) = service
            .prepare_external_edit_inner(original.clone(), 0)
            .unwrap();
        let scratch_path = PathBuf::from(scratch_path);

        service.discard_external_edit_inner(&token).unwrap();

        assert_eq!(fs::read_to_string(&original).unwrap(), "old");
        assert!(!scratch_path.parent().unwrap().exists());
        assert!(service.external_edits.lock().unwrap().is_empty());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn external_edit_can_sync_multiple_saves_before_cleanup() {
        let temp = test_dir("multi-sync");
        let original_dir = temp.join("original");
        let scratch_root = temp.join("fika-edit");
        fs::create_dir_all(&original_dir).unwrap();
        let original = original_dir.join("config.txt");
        fs::write(&original, "old").unwrap();

        let service = PrivilegedService::new_for_tests(scratch_root);
        let (scratch_path, token) = service
            .prepare_external_edit_inner(original.clone(), 0)
            .unwrap();
        let scratch_path = PathBuf::from(scratch_path);

        {
            let mut edits = service.external_edits.lock().unwrap();
            let edit = edits.get_mut(&token).unwrap();
            fs::write(&scratch_path, "new").unwrap();
            sync_external_edit(edit).unwrap();
            fs::write(&scratch_path, "newer").unwrap();
            sync_external_edit(edit).unwrap();
        }

        assert_eq!(fs::read_to_string(&original).unwrap(), "newer");
        assert!(scratch_path.exists());

        service.discard_external_edit_inner(&token).unwrap();
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn stale_external_edit_expires_with_final_writeback() {
        let temp = test_dir("expire");
        let original_dir = temp.join("original");
        let scratch_root = temp.join("fika-edit");
        fs::create_dir_all(&original_dir).unwrap();
        let original = original_dir.join("config.txt");
        fs::write(&original, "old").unwrap();

        let service = PrivilegedService::new_for_tests(scratch_root);
        let (scratch_path, token) = service
            .prepare_external_edit_inner(original.clone(), 0)
            .unwrap();
        let scratch_path = PathBuf::from(scratch_path);
        fs::write(&scratch_path, "expired edit").unwrap();
        {
            let mut edits = service.external_edits.lock().unwrap();
            edits.get_mut(&token).unwrap().created_secs = 0;
        }

        service.expire_stale_external_edits();

        assert_eq!(fs::read_to_string(&original).unwrap(), "expired edit");
        assert!(!scratch_path.parent().unwrap().exists());
        assert!(service.external_edits.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn cleanup_refuses_paths_outside_fika_edit() {
        let temp = test_dir("cleanup");
        fs::create_dir_all(&temp).unwrap();
        let path = temp.join("token").join("file.txt");

        let error = cleanup_scratch_token_dir(&path).unwrap_err();

        assert!(error.contains("outside fika-edit"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn unit_finished_state_detection_is_conservative() {
        assert!(is_finished_unit_state("inactive"));
        assert!(is_finished_unit_state("failed"));
        assert!(!is_finished_unit_state("activating"));
        assert!(!is_finished_unit_state("active"));
        assert!(!is_finished_unit_state("reloading"));
    }

    #[test]
    fn service_mode_selects_expected_bus() {
        let address = Some("unix:path=/run/user/1000/bus".to_string());
        assert!(matches!(
            bus_connection_address(&address, &ServiceMode::System),
            BusConnection::System
        ));
        assert!(matches!(
            bus_connection_address(&address, &ServiceMode::SessionPkexec { allowed_uid: 1000 }),
            BusConnection::SessionAddress("unix:path=/run/user/1000/bus")
        ));
        assert!(matches!(
            bus_connection_address(&None, &ServiceMode::SessionPkexec { allowed_uid: 1000 }),
            BusConnection::Session
        ));
    }

    #[test]
    fn helper_lifecycle_summary_reports_mode_bus_and_activity() {
        assert_eq!(
            helper_lifecycle_summary("starting", &ServiceMode::System, &None, 0, 0),
            "phase=starting mode=system-bus bus_connection=system authorized_subject=polkit session_address=false idle_for=0 active_external_edits=0"
        );

        let address = Some("unix:path=/run/user/1000/bus".to_string());
        assert_eq!(
            helper_lifecycle_summary(
                "exiting",
                &ServiceMode::SessionPkexec { allowed_uid: 1000 },
                &address,
                180,
                2,
            ),
            "phase=exiting mode=session-bus-pkexec bus_connection=provided-session authorized_subject=uid:1000 session_address=true idle_for=180 active_external_edits=2"
        );

        assert_eq!(
            helper_lifecycle_summary(
                "starting",
                &ServiceMode::SessionPkexec { allowed_uid: 1000 },
                &None,
                0,
                0,
            ),
            "phase=starting mode=session-bus-pkexec bus_connection=session authorized_subject=uid:1000 session_address=false idle_for=0 active_external_edits=0"
        );
    }

    #[test]
    fn privilege_env_flag_truthy_values_are_explicit() {
        assert!(env_flag_is_truthy("1"));
        assert!(env_flag_is_truthy(" true "));
        assert!(env_flag_is_truthy("YES"));
        assert!(env_flag_is_truthy("on"));
        assert!(!env_flag_is_truthy(""));
        assert!(!env_flag_is_truthy("0"));
        assert!(!env_flag_is_truthy("false"));
        assert!(!env_flag_is_truthy("disabled"));
    }

    #[test]
    fn privileged_commands_reject_network_paths_before_dbus() {
        let commands = [
            PrivilegedCommand::CreateFolder {
                parent: PathBuf::from("smb://server/share/"),
                name: "folder".to_string(),
            },
            PrivilegedCommand::CreateFile {
                parent: PathBuf::from("smb://server/share/"),
                name: "file.txt".to_string(),
            },
            PrivilegedCommand::Rename {
                path: PathBuf::from("smb://server/share/file.txt"),
                new_name: "renamed.txt".to_string(),
            },
            PrivilegedCommand::Trash {
                paths: vec![PathBuf::from("smb://server/share/file.txt")],
            },
            PrivilegedCommand::Transfer {
                operation: "copy".to_string(),
                source: PathBuf::from("smb://server/share/file.txt"),
                target_dir: PathBuf::from("/tmp"),
            },
            PrivilegedCommand::Transfer {
                operation: "copy".to_string(),
                source: PathBuf::from("/tmp/file.txt"),
                target_dir: PathBuf::from("smb://server/share/"),
            },
        ];

        for command in commands {
            let error = command.validate_local_paths().unwrap_err();
            assert!(error.contains("network locations are not supported"));
            assert!(error.contains("smb://server/share"));
        }
    }

    #[test]
    fn privileged_helper_cli_rejects_network_paths() {
        let error = run_helper(&strings(&[
            "create-folder",
            "smb://server/share/",
            "folder",
        ]))
        .unwrap_err();
        assert!(error.contains("network locations are not supported"));

        let error = run_helper(&strings(&["trash", "smb://server/share/file.txt"])).unwrap_err();
        assert!(error.contains("network locations are not supported"));

        let error = run_helper(&strings(&[
            "transfer",
            "copy",
            "/tmp/file.txt",
            "smb://server/share/",
        ]))
        .unwrap_err();
        assert!(error.contains("network locations are not supported"));
    }

    #[test]
    fn polkit_diagnostics_include_action_and_install_hint() {
        let failed = polkit_check_failed_message("missing action");
        assert!(failed.contains(ACTION_ID));
        assert!(failed.contains(POLICY_FILE));
        assert!(failed.contains("missing action"));

        let denied = polkit_denied_message();
        assert!(denied.contains(ACTION_ID));

        let unavailable = polkit_authority_unavailable_message("no service");
        assert!(unavailable.contains("polkit"));
        assert!(unavailable.contains(ACTION_ID));
        assert!(unavailable.contains(POLICY_FILE));
        assert!(unavailable.contains("no service"));
        assert!(unavailable.contains("desktop polkit agent"));
    }

    #[test]
    fn privileged_helper_start_diagnostic_separates_activation_paths() {
        let message = privileged_helper_start_failed_message(
            "system service missing",
            "session service missing",
            "pkexec not installed",
        );

        assert!(message.contains("System bus activation failed: system service missing"));
        assert!(message.contains("Development session-bus helper failed: session service missing"));
        assert!(message.contains("pkexec fallback failed: pkexec not installed"));
        assert!(message.contains(POLICY_FILE));
        assert!(message.contains("desktop polkit agent"));
    }

    fn test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "fika-privilege-{name}-{}-{}",
            std::process::id(),
            TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }
}
