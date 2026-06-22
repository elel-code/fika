use futures_lite::StreamExt;
use std::collections::HashMap;
use std::env;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::process::Command;
use tokio::task::JoinHandle;
use zbus::fdo;
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.fika";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
type PortalChoice = (String, String, Vec<(String, String)>, String);
type PortalFilter = (String, Vec<(u32, String)>);

#[derive(Debug, Default)]
struct ChooserFilterMap {
    portal_filters: Vec<PortalFilter>,
    chooser_specs: Vec<String>,
    portal_indices: Vec<usize>,
    initial_chooser_index: Option<usize>,
    mime_mapped_filters: usize,
    hidden_filters: usize,
}

#[derive(Debug, Default)]
struct ChooserResult {
    paths: Vec<PathBuf>,
    filter_index: Option<usize>,
    choices: Vec<(String, String)>,
}

#[derive(Debug)]
struct ChooserProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct ChooserProcess {
    output_task: JoinHandle<Result<ChooserProcessOutput, String>>,
    terminate_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParentWindowStatus {
    Accepted,
    Empty,
    Malformed,
    EmptyHandle,
    UnsupportedScheme,
}

impl ParentWindowStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Empty => "empty",
            Self::Malformed => "malformed",
            Self::EmptyHandle => "empty-handle",
            Self::UnsupportedScheme => "unsupported-scheme",
        }
    }
}

fn parent_window_binding(status: ParentWindowStatus) -> &'static str {
    match status {
        ParentWindowStatus::Accepted => "metadata-only",
        ParentWindowStatus::Empty => "none",
        ParentWindowStatus::Malformed
        | ParentWindowStatus::EmptyHandle
        | ParentWindowStatus::UnsupportedScheme => "rejected",
    }
}

fn parent_window_binding_reason(status: ParentWindowStatus) -> &'static str {
    match status {
        ParentWindowStatus::Accepted => "parent-token-binding-unavailable",
        ParentWindowStatus::Empty => "no-parent-window",
        ParentWindowStatus::Malformed => "malformed-parent-window",
        ParentWindowStatus::EmptyHandle => "empty-parent-window-handle",
        ParentWindowStatus::UnsupportedScheme => "unsupported-parent-window-scheme",
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ParentWindowDecision {
    handle: Option<String>,
    status: ParentWindowStatus,
}

#[derive(Clone, Copy, Debug)]
struct PortalRequestDebug<'a> {
    method: &'a str,
    handle: &'a str,
    start_dir: Option<&'a Path>,
    directory: bool,
    multiple: bool,
    save_kind: &'a str,
    save_files: usize,
    portal_filters: usize,
    chooser_filters: usize,
    mime_mapped_filters: usize,
    hidden_filters: usize,
    initial_filter_index: Option<usize>,
    portal_choices: usize,
    parent_status: ParentWindowStatus,
    parent_forwarded: bool,
}

#[derive(Debug, Default)]
struct ChooserArgs {
    start_dir: Option<PathBuf>,
    directory: bool,
    multiple: bool,
    title: Option<String>,
    save_name: Option<String>,
    save_files: Option<Vec<String>>,
    accept_label: Option<String>,
    filters: Vec<String>,
    filter_index: Option<usize>,
    return_filter: bool,
    choices: Vec<String>,
    return_choices: bool,
    parent_window: Option<String>,
}

enum ChooserRun {
    Selected(ChooserResult),
    Cancelled(ChooserCancelReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChooserCancelReason {
    UserCancelled,
    EmptyOutput,
    RequestClose,
    CloseStreamEnded,
}

impl ChooserCancelReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::UserCancelled => "user-cancelled",
            Self::EmptyOutput => "empty-output",
            Self::RequestClose => "request-close",
            Self::CloseStreamEnded => "close-stream-ended",
        }
    }
}

fn main() {
    if env::args()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to initialize XDP backend runtime");

    if let Err(err) = runtime.block_on(run()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let _connection = zbus::connection::Builder::session()
        .map_err(|err| format!("cannot connect to session D-Bus: {err}"))?
        .name(BUS_NAME)
        .map_err(|err| format!("cannot request portal backend bus name: {err}"))?
        .serve_at(OBJECT_PATH, FileChooser)
        .map_err(|err| format!("cannot register FileChooser portal object: {err}"))?
        .build()
        .await
        .map_err(|err| format!("cannot build portal backend D-Bus service: {err}"))?;

    std::future::pending::<()>().await;
    Ok(())
}

struct FileChooser;

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    #[zbus(out_args("response", "results"))]
    async fn open_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let directory = option_bool(&options, "directory").unwrap_or(false);
        let multiple = option_bool(&options, "multiple").unwrap_or(false);
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        let start_dir = current_folder(&options);
        portal_debug_log_request(PortalRequestDebug {
            method: "OpenFile",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory,
            multiple,
            save_kind: "none",
            save_files: 0,
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            directory,
            multiple,
            title: portal_title(title),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let (start_dir, name) = save_file_start_and_name(&options);
        let Some(name) = name else {
            return Ok(cancelled());
        };
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        portal_debug_log_request(PortalRequestDebug {
            method: "SaveFile",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory: false,
            multiple: false,
            save_kind: "file",
            save_files: 0,
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            title: portal_title(title),
            save_name: Some(name),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_files(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let files = save_files(&options);
        if files.is_empty() {
            return Ok(cancelled());
        }
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let choice_specs = chooser_choice_specs(&choices);
        let return_choices = !choice_specs.is_empty();
        let parent_window = portal_parent_window(parent_window);
        let start_dir = current_folder(&options);
        portal_debug_log_request(PortalRequestDebug {
            method: "SaveFiles",
            handle: handle.as_str(),
            start_dir: start_dir.as_deref(),
            directory: true,
            multiple: false,
            save_kind: "files",
            save_files: files.len(),
            portal_filters: filter_map.portal_filters.len(),
            chooser_filters: filter_map.chooser_specs.len(),
            mime_mapped_filters: filter_map.mime_mapped_filters,
            hidden_filters: filter_map.hidden_filters,
            initial_filter_index: filter_map.initial_chooser_index,
            portal_choices: choices.len(),
            parent_status: parent_window.status,
            parent_forwarded: parent_window.handle.is_some(),
        });
        let args = chooser_args(ChooserArgs {
            start_dir,
            directory: true,
            title: portal_title(title),
            save_files: Some(files),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: choice_specs,
            return_choices,
            parent_window: parent_window.handle,
            ..ChooserArgs::default()
        });
        match run_chooser_for_request(connection, &handle, args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled(_)) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }
}

fn chooser_args(request: ChooserArgs) -> Vec<String> {
    let mut args = vec!["--chooser".to_string()];
    if request.directory {
        args.push("--chooser-directory".to_string());
    }
    if request.multiple {
        args.push("--chooser-multiple".to_string());
    }
    if let Some(save_name) = request.save_name {
        args.push("--chooser-save".to_string());
        args.push(save_name);
    }
    if let Some(save_files) = request.save_files {
        args.push("--chooser-save-files".to_string());
        args.push(save_files.join("\n"));
    }
    if let Some(title) = request.title {
        args.push("--chooser-title".to_string());
        args.push(title);
    }
    if let Some(accept_label) = request.accept_label {
        args.push("--chooser-accept-label".to_string());
        args.push(accept_label);
    }
    if !request.filters.is_empty() {
        args.push("--chooser-filters".to_string());
        args.push(request.filters.join("\n"));
    }
    if let Some(filter_index) = request.filter_index {
        args.push("--chooser-filter-index".to_string());
        args.push(filter_index.to_string());
    }
    if request.return_filter {
        args.push("--chooser-return-filter".to_string());
    }
    if !request.choices.is_empty() {
        args.push("--chooser-choices".to_string());
        args.push(request.choices.join("\n"));
    }
    if request.return_choices {
        args.push("--chooser-return-choices".to_string());
    }
    if let Some(parent_window) = request.parent_window {
        args.push("--chooser-parent-window".to_string());
        args.push(parent_window);
    }
    if let Some(start_dir) = request.start_dir {
        args.push(start_dir.display().to_string());
    }
    args
}

fn chooser_run_from_output(output: ChooserProcessOutput) -> Result<ChooserRun, String> {
    if !output.status.success() {
        if output.status.code() == Some(fika_core::CHOOSER_CANCEL_EXIT_CODE) {
            return Ok(ChooserRun::Cancelled(ChooserCancelReason::UserCancelled));
        }
        return Err(chooser_failure_message(
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("chooser stdout was not UTF-8: {err}"))?;
    let result = parse_chooser_output(&stdout);
    if result.paths.is_empty() {
        Ok(ChooserRun::Cancelled(ChooserCancelReason::EmptyOutput))
    } else {
        Ok(ChooserRun::Selected(result))
    }
}

async fn run_chooser_for_request(
    connection: &Connection,
    handle: &OwnedObjectPath,
    args: Vec<String>,
) -> Result<ChooserRun, String> {
    let mut close_stream = request_close_stream(connection, handle).await?;
    let ChooserProcess {
        mut output_task,
        terminate_tx,
    } = ChooserProcess::spawn(chooser_command(gui_executable()?, args))?;
    let outcome = tokio::select! {
        result = &mut output_task => chooser_output_from_task_result(result).and_then(chooser_run_from_output),
        close = close_stream.next() => {
            terminate_chooser_process(output_task, terminate_tx).await?;
            match close {
                Some(Ok(_)) => Ok(ChooserRun::Cancelled(ChooserCancelReason::RequestClose)),
                Some(Err(err)) => Err(format!("portal request Close signal failed: {err}")),
                None => Ok(ChooserRun::Cancelled(ChooserCancelReason::CloseStreamEnded)),
            }
        }
    };
    portal_debug_log_chooser_lifecycle(handle.as_str(), &outcome);
    outcome
}

impl ChooserProcess {
    fn spawn(mut command: Command) -> Result<Self, String> {
        let mut child = command
            .spawn()
            .map_err(|err| format!("cannot launch fika chooser: {err}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "cannot capture chooser stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "cannot capture chooser stderr".to_string())?;
        let stdout_task = read_pipe_task(stdout);
        let stderr_task = read_pipe_task(stderr);
        let (terminate_tx, terminate_rx) = tokio::sync::oneshot::channel();
        let output_task = tokio::spawn(async move {
            wait_chooser_child(child, stdout_task, stderr_task, terminate_rx).await
        });
        Ok(Self {
            output_task,
            terminate_tx: Some(terminate_tx),
        })
    }
}

async fn wait_chooser_child(
    mut child: Child,
    stdout_task: JoinHandle<io::Result<Vec<u8>>>,
    stderr_task: JoinHandle<io::Result<Vec<u8>>>,
    mut terminate_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<ChooserProcessOutput, String> {
    let status = tokio::select! {
        status = child.wait() => {
            status.map_err(|err| format!("cannot wait for fika chooser: {err}"))?
        }
        terminate = &mut terminate_rx => {
            if terminate.is_ok() {
                kill_chooser_child(&mut child).await?
            } else {
                child.wait().await.map_err(|err| format!("cannot wait for fika chooser after lifecycle channel closed: {err}"))?
            }
        }
    };
    let stdout = collect_pipe("stdout", stdout_task).await?;
    let stderr = collect_pipe("stderr", stderr_task).await?;
    Ok(ChooserProcessOutput {
        status,
        stdout,
        stderr,
    })
}

async fn kill_chooser_child(child: &mut Child) -> Result<ExitStatus, String> {
    match child.start_kill() {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::InvalidInput => {}
        Err(err) => {
            return Err(format!(
                "cannot terminate fika chooser after request Close: {err}"
            ));
        }
    }
    child
        .wait()
        .await
        .map_err(|err| format!("cannot wait for terminated fika chooser: {err}"))
}

fn read_pipe_task<R>(mut reader: R) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await?;
        Ok(output)
    })
}

async fn terminate_chooser_process(
    output_task: JoinHandle<Result<ChooserProcessOutput, String>>,
    terminate_tx: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<ChooserProcessOutput, String> {
    if let Some(terminate_tx) = terminate_tx {
        let _ = terminate_tx.send(());
    }
    chooser_output_from_task_result(output_task.await)
}

async fn collect_pipe(
    name: &'static str,
    task: JoinHandle<io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, String> {
    task.await
        .map_err(|err| format!("chooser {name} reader task failed: {err}"))?
        .map_err(|err| format!("cannot read chooser {name}: {err}"))
}

fn chooser_output_from_task_result(
    result: Result<Result<ChooserProcessOutput, String>, tokio::task::JoinError>,
) -> Result<ChooserProcessOutput, String> {
    result.map_err(|err| format!("chooser lifecycle task failed: {err}"))?
}

async fn request_close_stream(
    connection: &Connection,
    handle: &OwnedObjectPath,
) -> Result<MessageStream, String> {
    let rule = request_close_match_rule(handle.as_str())?;
    MessageStream::for_match_rule(rule, connection, Some(1))
        .await
        .map_err(|err| format!("cannot subscribe to portal request Close signal: {err}"))
}

fn request_close_match_rule(handle: &str) -> Result<zbus::OwnedMatchRule, String> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.impl.portal.Request")
        .map_err(|err| format!("cannot build portal request Close interface match: {err}"))?
        .member("Close")
        .map_err(|err| format!("cannot build portal request Close member match: {err}"))?
        .path(handle)
        .map_err(|err| format!("cannot build portal request Close path match: {err}"))?
        .build()
        .into())
}

fn chooser_command(program: PathBuf, args: Vec<String>) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command.stdin(Stdio::null());
    command.stderr(Stdio::piped());
    command.stdout(Stdio::piped());
    command.kill_on_drop(true);
    command
}

fn chooser_failure_message(code: Option<i32>, stderr: &str) -> String {
    let status = code.map_or_else(
        || "terminated by signal".to_string(),
        |code| format!("exit code {code}"),
    );
    if stderr.is_empty() {
        format!("fika chooser failed with {status}")
    } else {
        format!("fika chooser failed with {status}: {stderr}")
    }
}

fn parse_chooser_output(stdout: &str) -> ChooserResult {
    let mut result = ChooserResult::default();
    for line in stdout.lines().filter(|line| !line.is_empty()) {
        if let Some(index) = line
            .strip_prefix("FIKA_CHOOSER_FILTER\t")
            .and_then(|index| index.parse::<usize>().ok())
        {
            result.filter_index = Some(index);
        } else if let Some(choice) = line.strip_prefix("FIKA_CHOOSER_CHOICE\t") {
            let mut parts = choice.splitn(2, '\t');
            if let (Some(id), Some(selected)) = (parts.next(), parts.next()) {
                result.choices.push((id.to_string(), selected.to_string()));
            }
        } else {
            result.paths.push(PathBuf::from(line));
        }
    }
    result
}

fn gui_executable() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("FIKA_GUI") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().map_err(|err| format!("cannot locate executable: {err}"))?;
    let Some(dir) = exe.parent() else {
        return Err(format!("cannot locate fika next to {}", exe.display()));
    };
    Ok(dir.join("fika"))
}

fn current_folder(options: &HashMap<String, OwnedValue>) -> Option<PathBuf> {
    let value = options.get("current_folder")?;
    nul_terminated_bytes_to_location(Vec::<u8>::try_from(value.clone()).ok()?)
}

fn save_file_start_and_name(
    options: &HashMap<String, OwnedValue>,
) -> (Option<PathBuf>, Option<String>) {
    if let Some(current_file) = options
        .get("current_file")
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .and_then(nul_terminated_bytes_to_location)
    {
        let start_dir = fika_core::parent_location(&current_file);
        let name = current_file
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
        return (start_dir, name);
    }

    (
        current_folder(options),
        option_string(options, "current_name"),
    )
}

fn save_files(options: &HashMap<String, OwnedValue>) -> Vec<String> {
    options
        .get("files")
        .and_then(|value| Vec::<Vec<u8>>::try_from(value.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(nul_terminated_bytes_to_string)
        .collect()
}

fn portal_filters(options: &HashMap<String, OwnedValue>) -> Vec<PortalFilter> {
    options
        .get("filters")
        .and_then(|value| Vec::<PortalFilter>::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn portal_choices(options: &HashMap<String, OwnedValue>) -> Vec<PortalChoice> {
    options
        .get("choices")
        .and_then(|value| Vec::<PortalChoice>::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn chooser_filter_map(
    options: &HashMap<String, OwnedValue>,
    portal_filters: Vec<PortalFilter>,
) -> ChooserFilterMap {
    let current_filter = options
        .get("current_filter")
        .and_then(|value| PortalFilter::try_from(value.clone()).ok());
    let mut map = ChooserFilterMap {
        portal_filters,
        ..ChooserFilterMap::default()
    };

    for (portal_index, filter) in map.portal_filters.iter().enumerate() {
        let Some((spec, mime_mapped)) = chooser_filter_spec(filter, portal_index) else {
            map.hidden_filters += 1;
            continue;
        };
        if current_filter
            .as_ref()
            .is_some_and(|current| current == filter)
        {
            map.initial_chooser_index = Some(map.chooser_specs.len());
        }
        map.chooser_specs.push(spec);
        map.portal_indices.push(portal_index);
        if mime_mapped {
            map.mime_mapped_filters += 1;
        }
    }

    map
}

fn chooser_filter_spec(
    (label, patterns): &PortalFilter,
    portal_index: usize,
) -> Option<(String, bool)> {
    let (globs, mime_mapped) = chooser_filter_globs(patterns);
    if globs.is_empty() {
        return None;
    }
    let label = chooser_filter_label(label, portal_index);
    Some((format!("{label}\t{}", globs.join(";")), mime_mapped))
}

fn chooser_filter_label(label: &str, portal_index: usize) -> String {
    let label = label.trim();
    if label.is_empty() {
        format!("Filter {}", portal_index + 1)
    } else {
        label.to_string()
    }
}

fn chooser_filter_globs(patterns: &[(u32, String)]) -> (Vec<String>, bool) {
    let mut globs = Vec::new();
    let mut mime_mapped = false;
    for (kind, pattern) in patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        match kind {
            0 => push_unique_glob(&mut globs, pattern),
            1 => {
                let mime_globs = mime_filter_globs(pattern);
                if !mime_globs.is_empty() {
                    mime_mapped = true;
                }
                for glob in mime_globs {
                    push_unique_glob(&mut globs, glob);
                }
            }
            _ => {}
        }
    }
    (globs, mime_mapped)
}

fn push_unique_glob(globs: &mut Vec<String>, glob: &str) {
    if !globs.iter().any(|existing| existing == glob) {
        globs.push(glob.to_string());
    }
}

fn mime_filter_globs(mime_type: &str) -> &'static [&'static str] {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/*" => &[
            "*.avif", "*.avifs", "*.bmp", "*.gif", "*.heic", "*.heif", "*.jpeg", "*.jpg", "*.png",
            "*.svg", "*.webp",
        ],
        "image/avif" | "image/avif-sequence" => &["*.avif", "*.avifs"],
        "image/bmp" | "image/x-bmp" => &["*.bmp"],
        "image/gif" => &["*.gif"],
        "image/heic" => &["*.heic"],
        "image/heif" => &["*.heif"],
        "image/jpeg" | "image/pjpeg" => &["*.jpg", "*.jpeg"],
        "image/png" | "image/x-png" => &["*.png"],
        "image/svg+xml" => &["*.svg"],
        "image/webp" => &["*.webp"],
        "text/*" => &[
            "*.txt",
            "*.text",
            "*.md",
            "*.markdown",
            "*.rst",
            "*.csv",
            "*.tsv",
            "*.log",
            "*.ini",
            "*.conf",
            "*.toml",
            "*.json",
            "*.yaml",
            "*.yml",
            "*.xml",
            "*.html",
            "*.htm",
            "*.css",
            "*.js",
            "*.rs",
            "*.c",
            "*.h",
            "*.cpp",
            "*.hpp",
            "*.py",
            "*.sh",
        ],
        "text/plain" => &[
            "*.txt",
            "*.text",
            "*.md",
            "*.markdown",
            "*.rst",
            "*.log",
            "*.ini",
            "*.conf",
        ],
        "text/csv" => &["*.csv"],
        "text/html" => &["*.html", "*.htm"],
        "text/markdown" | "text/x-markdown" => &["*.md", "*.markdown"],
        "text/xml" | "application/xml" => &["*.xml"],
        "application/json" => &["*.json"],
        "application/pdf" => &["*.pdf"],
        "application/zip" => &["*.zip"],
        "application/x-zip-compressed" => &["*.zip"],
        "application/gzip" => &["*.gz"],
        "application/x-bzip2" => &["*.bz2"],
        "application/x-tar" => &["*.tar"],
        "application/x-xz" => &["*.xz"],
        "application/zstd" | "application/x-zstd" => &["*.zst"],
        "application/x-7z-compressed" => &["*.7z"],
        "application/vnd.rar" | "application/x-rar-compressed" => &["*.rar"],
        "audio/*" => &[
            "*.aac", "*.flac", "*.m4a", "*.mp3", "*.oga", "*.ogg", "*.opus", "*.wav",
        ],
        "audio/aac" => &["*.aac"],
        "audio/flac" | "audio/x-flac" => &["*.flac"],
        "audio/mp4" | "audio/x-m4a" => &["*.m4a"],
        "audio/mpeg" => &["*.mp3"],
        "audio/ogg" => &["*.oga", "*.ogg"],
        "audio/opus" => &["*.opus"],
        "audio/wav" | "audio/x-wav" => &["*.wav"],
        "video/*" => &[
            "*.avi", "*.m4v", "*.mkv", "*.mov", "*.mp4", "*.mpeg", "*.mpg", "*.ogv", "*.webm",
        ],
        "video/mp4" => &["*.mp4", "*.m4v"],
        "video/mpeg" => &["*.mpeg", "*.mpg"],
        "video/quicktime" => &["*.mov"],
        "video/webm" => &["*.webm"],
        "video/x-matroska" => &["*.mkv"],
        "video/x-msvideo" | "video/avi" => &["*.avi"],
        "application/msword" => &["*.doc"],
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => &["*.docx"],
        "application/vnd.oasis.opendocument.text" => &["*.odt"],
        "application/vnd.ms-excel" => &["*.xls"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => &["*.xlsx"],
        "application/vnd.oasis.opendocument.spreadsheet" => &["*.ods"],
        "application/vnd.ms-powerpoint" => &["*.ppt"],
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => &["*.pptx"],
        "application/vnd.oasis.opendocument.presentation" => &["*.odp"],
        _ => &[],
    }
}

fn chooser_choice_specs(choices: &[PortalChoice]) -> Vec<String> {
    choices
        .iter()
        .filter_map(|(id, label, items, default)| {
            let id = chooser_choice_token(id)?;
            let label = chooser_choice_label(label).unwrap_or_else(|| id.clone());
            let items = items
                .iter()
                .filter_map(|(item_id, item_label)| {
                    let item_id = chooser_choice_token(item_id)?;
                    let item_label =
                        chooser_choice_label(item_label).unwrap_or_else(|| item_id.clone());
                    Some((item_id, item_label))
                })
                .collect::<Vec<_>>();
            if items.is_empty() {
                return None;
            }
            let default = chooser_choice_token(default)
                .filter(|default| items.iter().any(|(item_id, _)| item_id == default))
                .unwrap_or_default();
            let items = items
                .into_iter()
                .map(|(item_id, item_label)| format!("{item_id}={item_label}"))
                .collect::<Vec<_>>()
                .join(";");
            Some(format!("{id}\t{label}\t{default}\t{items}"))
        })
        .collect()
}

fn chooser_choice_token(value: &str) -> Option<String> {
    (!value.trim().is_empty()
        && !value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ';' | '=')))
    .then(|| value.to_string())
}

fn chooser_choice_label(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, ';' | '=') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn nul_terminated_bytes_to_location(bytes: Vec<u8>) -> Option<PathBuf> {
    nul_terminated_bytes_to_string(bytes).map(|value| {
        fika_core::normalize_network_uri(&value)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(value))
    })
}

fn nul_terminated_bytes_to_string(bytes: Vec<u8>) -> Option<String> {
    let bytes = bytes
        .split(|byte| *byte == 0)
        .next()
        .filter(|bytes| !bytes.is_empty())?;
    Some(String::from_utf8_lossy(bytes).to_string())
}

fn option_bool(options: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    options
        .get(key)
        .and_then(|value| bool::try_from(value.clone()).ok())
}

fn option_string(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    options
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .filter(|value| !value.is_empty())
}

fn portal_debug_log_request(request: PortalRequestDebug<'_>) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!("[fika portal] {}", portal_request_summary(request));
}

fn portal_request_summary(request: PortalRequestDebug<'_>) -> String {
    format!(
        "request method={} handle={} start_dir={} directory={} multiple={} save_kind={} save_files={} portal_filters={} chooser_filters={} mime_mapped_filters={} hidden_filters={} initial_filter={} portal_choices={} parent_status={} parent_forwarded={} parent_binding={} parent_binding_reason={} native_transient=false",
        request.method,
        request.handle,
        request
            .start_dir
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        request.directory,
        request.multiple,
        request.save_kind,
        request.save_files,
        request.portal_filters,
        request.chooser_filters,
        request.mime_mapped_filters,
        request.hidden_filters,
        request
            .initial_filter_index
            .map(|index| index.to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        request.portal_choices,
        request.parent_status.as_str(),
        request.parent_forwarded,
        parent_window_binding(request.parent_status),
        parent_window_binding_reason(request.parent_status),
    )
}

fn portal_parent_window(parent_window: String) -> ParentWindowDecision {
    let parent_window = parent_window.trim();
    if parent_window.is_empty() {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Empty,
        };
        portal_debug_log_parent(&decision);
        return decision;
    }

    let Some((scheme, handle)) = parent_window.split_once(':') else {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Malformed,
        };
        portal_debug_log_parent(&decision);
        return decision;
    };

    if handle.is_empty() {
        let decision = ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::EmptyHandle,
        };
        portal_debug_log_parent(&decision);
        return decision;
    }

    let decision = if scheme == "wayland" {
        ParentWindowDecision {
            handle: Some(parent_window.to_string()),
            status: ParentWindowStatus::Accepted,
        }
    } else {
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::UnsupportedScheme,
        }
    };
    portal_debug_log_parent(&decision);
    decision
}

fn portal_debug_log_parent(decision: &ParentWindowDecision) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!(
        "[fika portal] parent_window status={} handle={} parent_binding={} parent_binding_reason={} native_transient=false",
        decision.status.as_str(),
        decision.handle.as_deref().unwrap_or(""),
        parent_window_binding(decision.status),
        parent_window_binding_reason(decision.status),
    );
}

fn portal_debug_log_chooser_lifecycle(handle: &str, outcome: &Result<ChooserRun, String>) {
    if !portal_debug_enabled() {
        return;
    }

    eprintln!(
        "[fika portal] {}",
        chooser_lifecycle_summary(handle, outcome)
    );
}

fn chooser_lifecycle_summary(handle: &str, outcome: &Result<ChooserRun, String>) -> String {
    match outcome {
        Ok(ChooserRun::Selected(result)) => format!(
            "chooser_finished handle={} outcome=selected paths={} filter={} choices={}",
            handle,
            result.paths.len(),
            result
                .filter_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            result.choices.len()
        ),
        Ok(ChooserRun::Cancelled(reason)) => format!(
            "chooser_finished handle={} outcome=cancelled reason={}",
            handle,
            reason.as_str()
        ),
        Err(err) => format!(
            "chooser_finished handle={} outcome=failed error={}",
            handle,
            first_log_line(err)
        ),
    }
}

fn first_log_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

fn portal_debug_enabled() -> bool {
    static DEBUG_PORTAL: OnceLock<bool> = OnceLock::new();
    *DEBUG_PORTAL.get_or_init(|| {
        env::var("FIKA_DEBUG_PORTAL").is_ok_and(|value| env_flag_is_truthy(value.as_str()))
    })
}

fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn portal_title(title: String) -> Option<String> {
    (!title.is_empty()).then_some(title)
}

fn cancelled() -> (u32, HashMap<String, OwnedValue>) {
    (1, HashMap::new())
}

fn results_for_paths(
    result: ChooserResult,
    options: &HashMap<String, OwnedValue>,
    filter_map: &ChooserFilterMap,
) -> HashMap<String, OwnedValue> {
    let uris = result
        .paths
        .iter()
        .map(|path| path_to_portal_uri(path))
        .collect::<Vec<_>>();
    let mut results = HashMap::new();
    if let Ok(value) = OwnedValue::try_from(Value::new(uris)) {
        results.insert("uris".to_string(), value);
    }
    if let Some(current_filter) = result
        .filter_index
        .and_then(|index| filter_map.portal_indices.get(index))
        .and_then(|portal_index| filter_map.portal_filters.get(*portal_index))
        .cloned()
        .and_then(|filter| OwnedValue::try_from(Value::new(filter)).ok())
    {
        results.insert("current_filter".to_string(), current_filter);
    }
    if let Some(choices) = selected_choices_for_options(options, &result.choices)
        && let Ok(value) = OwnedValue::try_from(Value::new(choices))
    {
        results.insert("choices".to_string(), value);
    }
    results
}

fn selected_choices_for_options(
    options: &HashMap<String, OwnedValue>,
    requested_choices: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let choices = options
        .get("choices")
        .and_then(|value| Vec::<PortalChoice>::try_from(value.clone()).ok())?;
    Some(
        choices
            .into_iter()
            .filter_map(|(id, _label, items, default)| {
                let selected = requested_choices
                    .iter()
                    .find(|(choice_id, selected)| {
                        choice_id == &id && items.iter().any(|(item_id, _)| item_id == selected)
                    })
                    .map(|(_, selected)| selected.clone())
                    .or_else(|| {
                        items
                            .iter()
                            .any(|(item_id, _)| item_id == &default)
                            .then_some(default)
                    })
                    .or_else(|| items.first().map(|(item_id, _)| item_id.clone()))?;
                Some((id, selected))
            })
            .collect(),
    )
}

fn path_to_file_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in text.as_bytes() {
        if is_uri_path_byte(*byte) {
            uri.push(*byte as char);
        } else {
            uri.push('%');
            uri.push(hex(byte >> 4));
            uri.push(hex(byte & 0x0f));
        }
    }
    uri
}

fn path_to_portal_uri(path: &Path) -> String {
    fika_core::network_uri_from_path(path).unwrap_or_else(|| path_to_file_uri(path))
}

fn is_uri_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~')
}

fn hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => unreachable!(),
    }
}

fn print_help() {
    println!(
        "Usage: fika-xdp-filechooser\n\n\
         Runs Fika's experimental xdg-desktop-portal FileChooser backend.\n\
         The backend owns {BUS_NAME} and launches `fika --chooser` for FileChooser requests."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_percent_encodes_non_ascii_and_spaces() {
        let uri = path_to_file_uri(Path::new("/tmp/Fika Test/数值.txt"));
        assert_eq!(uri, "file:///tmp/Fika%20Test/%E6%95%B0%E5%80%BC.txt");
    }

    #[cfg(unix)]
    #[test]
    fn file_uri_preserves_selected_symlink_path() {
        let temp =
            std::env::temp_dir().join(format!("fika-portal-uri-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let target = temp.join("target.txt");
        let link = temp.join("link.txt");
        std::fs::write(&target, "target").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(
            path_to_file_uri(&link),
            format!("file://{}", link.to_string_lossy())
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn chooser_args_include_portal_modes_before_start_dir() {
        let args = chooser_args(ChooserArgs {
            start_dir: Some(PathBuf::from("/tmp")),
            directory: true,
            multiple: true,
            save_name: Some("note.txt".to_string()),
            title: Some("Pick a note".to_string()),
            accept_label: Some("Select".to_string()),
            filters: vec!["Images\t*.png;*.jpg".to_string()],
            filter_index: Some(0),
            return_filter: true,
            choices: vec!["encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()],
            return_choices: true,
            parent_window: Some("wayland:1_42".to_string()),
            ..ChooserArgs::default()
        });

        assert_eq!(
            args,
            vec![
                "--chooser",
                "--chooser-directory",
                "--chooser-multiple",
                "--chooser-save",
                "note.txt",
                "--chooser-title",
                "Pick a note",
                "--chooser-accept-label",
                "Select",
                "--chooser-filters",
                "Images\t*.png;*.jpg",
                "--chooser-filter-index",
                "0",
                "--chooser-return-filter",
                "--chooser-choices",
                "encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1",
                "--chooser-return-choices",
                "--chooser-parent-window",
                "wayland:1_42",
                "/tmp"
            ]
        );
    }

    #[test]
    fn empty_portal_parent_window_is_not_forwarded() {
        assert_eq!(
            portal_parent_window(String::new()),
            ParentWindowDecision {
                handle: None,
                status: ParentWindowStatus::Empty,
            }
        );
        assert_eq!(portal_title(String::new()), None);
        assert_eq!(
            chooser_args(ChooserArgs {
                parent_window: portal_parent_window(String::new()).handle,
                title: portal_title(String::new()),
                ..ChooserArgs::default()
            }),
            vec!["--chooser"]
        );
    }

    #[test]
    fn portal_parent_window_accepts_wayland_handles() {
        assert_eq!(
            portal_parent_window("wayland:1_42".to_string()),
            ParentWindowDecision {
                handle: Some("wayland:1_42".to_string()),
                status: ParentWindowStatus::Accepted,
            }
        );
        assert_eq!(
            portal_parent_window("wayland:".to_string()),
            ParentWindowDecision {
                handle: None,
                status: ParentWindowStatus::EmptyHandle,
            }
        );
        assert_eq!(
            portal_parent_window("malformed".to_string()),
            ParentWindowDecision {
                handle: None,
                status: ParentWindowStatus::Malformed,
            }
        );
        assert_eq!(
            portal_parent_window("unsupported:window".to_string()),
            ParentWindowDecision {
                handle: None,
                status: ParentWindowStatus::UnsupportedScheme,
            }
        );
    }

    #[test]
    fn portal_request_summary_reports_request_shape() {
        let summary = portal_request_summary(PortalRequestDebug {
            method: "OpenFile",
            handle: "/org/freedesktop/portal/desktop/request/1_42/fika",
            start_dir: Some(Path::new("/home/yk")),
            directory: true,
            multiple: true,
            save_kind: "none",
            save_files: 0,
            portal_filters: 2,
            chooser_filters: 1,
            mime_mapped_filters: 1,
            hidden_filters: 1,
            initial_filter_index: Some(0),
            portal_choices: 1,
            parent_status: ParentWindowStatus::Accepted,
            parent_forwarded: true,
        });

        assert_eq!(
            summary,
            "request method=OpenFile handle=/org/freedesktop/portal/desktop/request/1_42/fika start_dir=/home/yk directory=true multiple=true save_kind=none save_files=0 portal_filters=2 chooser_filters=1 mime_mapped_filters=1 hidden_filters=1 initial_filter=0 portal_choices=1 parent_status=accepted parent_forwarded=true parent_binding=metadata-only parent_binding_reason=parent-token-binding-unavailable native_transient=false"
        );
    }

    #[test]
    fn portal_request_summary_reports_missing_optional_state() {
        let summary = portal_request_summary(PortalRequestDebug {
            method: "SaveFile",
            handle: "/org/freedesktop/portal/desktop/request/1_42/fika",
            start_dir: None,
            directory: false,
            multiple: false,
            save_kind: "file",
            save_files: 0,
            portal_filters: 0,
            chooser_filters: 0,
            mime_mapped_filters: 0,
            hidden_filters: 0,
            initial_filter_index: None,
            portal_choices: 0,
            parent_status: ParentWindowStatus::Empty,
            parent_forwarded: false,
        });

        assert_eq!(
            summary,
            "request method=SaveFile handle=/org/freedesktop/portal/desktop/request/1_42/fika start_dir=<none> directory=false multiple=false save_kind=file save_files=0 portal_filters=0 chooser_filters=0 mime_mapped_filters=0 hidden_filters=0 initial_filter=<none> portal_choices=0 parent_status=empty parent_forwarded=false parent_binding=none parent_binding_reason=no-parent-window native_transient=false"
        );
    }

    #[test]
    fn chooser_lifecycle_summary_reports_selected_metadata() {
        let outcome = Ok(ChooserRun::Selected(ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            filter_index: Some(1),
            choices: vec![("encoding".to_string(), "utf8".to_string())],
        }));

        assert_eq!(
            chooser_lifecycle_summary("/request", &outcome),
            "chooser_finished handle=/request outcome=selected paths=2 filter=1 choices=1"
        );
    }

    #[test]
    fn chooser_lifecycle_summary_reports_cancellation_reason() {
        let outcome = Ok(ChooserRun::Cancelled(ChooserCancelReason::RequestClose));

        assert_eq!(
            chooser_lifecycle_summary("/request", &outcome),
            "chooser_finished handle=/request outcome=cancelled reason=request-close"
        );
    }

    #[test]
    fn chooser_lifecycle_summary_reports_first_failure_line() {
        let outcome = Err("cannot launch chooser\nextra detail".to_string());

        assert_eq!(
            chooser_lifecycle_summary("/request", &outcome),
            "chooser_finished handle=/request outcome=failed error=cannot launch chooser"
        );
    }

    #[test]
    fn chooser_output_can_report_selected_filter_and_choices_without_breaking_paths() {
        let result = parse_chooser_output(
            "FIKA_CHOOSER_FILTER\t1\nFIKA_CHOOSER_CHOICE\tencoding\tlatin1\n/tmp/file.txt\n",
        );
        assert_eq!(result.filter_index, Some(1));
        assert_eq!(
            result.choices,
            vec![("encoding".to_string(), "latin1".to_string())]
        );
        assert_eq!(result.paths, vec![PathBuf::from("/tmp/file.txt")]);
    }

    #[test]
    fn chooser_output_preserves_path_whitespace() {
        let result =
            parse_chooser_output("FIKA_CHOOSER_FILTER\t0\n/tmp/ leading.txt\n/tmp/trailing.txt \n");

        assert_eq!(result.filter_index, Some(0));
        assert_eq!(
            result.paths,
            vec![
                PathBuf::from("/tmp/ leading.txt"),
                PathBuf::from("/tmp/trailing.txt "),
            ]
        );
    }

    #[test]
    fn portal_filters_map_to_chooser_specs_and_result_filter() {
        let filters = vec![
            (
                "Text".to_string(),
                vec![(0, "*.txt".to_string()), (1, "text/plain".to_string())],
            ),
            ("Images".to_string(), vec![(0, "*.png".to_string())]),
        ];
        let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());
        assert_eq!(
            filter_map.chooser_specs,
            vec![
                "Text\t*.txt;*.text;*.md;*.markdown;*.rst;*.log;*.ini;*.conf".to_string(),
                "Images\t*.png".to_string()
            ]
        );
        assert_eq!(filter_map.mime_mapped_filters, 1);
        assert_eq!(filter_map.hidden_filters, 0);

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.png")],
                filter_index: Some(1),
                choices: Vec::new(),
            },
            &HashMap::new(),
            &filter_map,
        );
        let current_filter = result.get("current_filter").cloned().unwrap();
        assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);
    }

    #[test]
    fn portal_mime_only_filters_map_to_chooser_specs() {
        let filters = vec![
            ("MIME only".to_string(), vec![(1, "image/png".to_string())]),
            ("Images".to_string(), vec![(0, "*.png".to_string())]),
        ];
        let mut options = HashMap::new();
        options.insert(
            "current_filter".to_string(),
            OwnedValue::try_from(Value::new(filters[0].clone())).unwrap(),
        );

        let filter_map = chooser_filter_map(&options, filters.clone());

        assert_eq!(
            filter_map.chooser_specs,
            vec!["MIME only\t*.png".to_string(), "Images\t*.png".to_string()]
        );
        assert_eq!(filter_map.portal_indices, vec![0, 1]);
        assert_eq!(filter_map.initial_chooser_index, Some(0));
        assert_eq!(filter_map.mime_mapped_filters, 1);
        assert_eq!(filter_map.hidden_filters, 0);

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.png")],
                filter_index: Some(0),
                choices: Vec::new(),
            },
            &options,
            &filter_map,
        );
        let current_filter = result.get("current_filter").cloned().unwrap();
        assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[0]);
    }

    #[test]
    fn portal_filters_with_empty_labels_get_stable_chooser_labels() {
        let filters = vec![
            ("".to_string(), vec![(1, "image/png".to_string())]),
            ("  ".to_string(), vec![(0, "*.txt".to_string())]),
        ];
        let mut options = HashMap::new();
        options.insert(
            "current_filter".to_string(),
            OwnedValue::try_from(Value::new(filters[1].clone())).unwrap(),
        );

        let filter_map = chooser_filter_map(&options, filters.clone());

        assert_eq!(
            filter_map.chooser_specs,
            vec!["Filter 1\t*.png".to_string(), "Filter 2\t*.txt".to_string()]
        );
        assert_eq!(filter_map.portal_indices, vec![0, 1]);
        assert_eq!(filter_map.initial_chooser_index, Some(1));

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: Some(1),
                choices: Vec::new(),
            },
            &options,
            &filter_map,
        );
        let current_filter = result.get("current_filter").cloned().unwrap();
        assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);
    }

    #[test]
    fn portal_unknown_mime_only_filters_stay_hidden() {
        let filters = vec![
            (
                "Unknown".to_string(),
                vec![(1, "application/x-fika-unknown".to_string())],
            ),
            (
                "Any text".to_string(),
                vec![(1, "text/*".to_string()), (1, "text/plain".to_string())],
            ),
        ];

        let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());

        assert_eq!(
            filter_map.chooser_specs,
            vec![
                "Any text\t*.txt;*.text;*.md;*.markdown;*.rst;*.csv;*.tsv;*.log;*.ini;*.conf;*.toml;*.json;*.yaml;*.yml;*.xml;*.html;*.htm;*.css;*.js;*.rs;*.c;*.h;*.cpp;*.hpp;*.py;*.sh"
                    .to_string()
            ]
        );
        assert_eq!(filter_map.portal_indices, vec![1]);
        assert_eq!(filter_map.mime_mapped_filters, 1);
        assert_eq!(filter_map.hidden_filters, 1);
    }

    #[test]
    fn portal_media_and_document_mime_filters_map_to_chooser_specs() {
        let filters = vec![
            ("Audio".to_string(), vec![(1, "audio/*".to_string())]),
            ("Video".to_string(), vec![(1, "video/mp4".to_string())]),
            (
                "AVIF".to_string(),
                vec![(1, "image/avif-sequence".to_string())],
            ),
            (
                "Documents".to_string(),
                vec![
                    (
                        1,
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            .to_string(),
                    ),
                    (1, "application/vnd.oasis.opendocument.text".to_string()),
                ],
            ),
            (
                "Sheets".to_string(),
                vec![
                    (
                        1,
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            .to_string(),
                    ),
                    (1, "application/vnd.ms-excel".to_string()),
                ],
            ),
        ];

        let filter_map = chooser_filter_map(&HashMap::new(), filters);

        assert_eq!(
            filter_map.chooser_specs,
            vec![
                "Audio\t*.aac;*.flac;*.m4a;*.mp3;*.oga;*.ogg;*.opus;*.wav".to_string(),
                "Video\t*.mp4;*.m4v".to_string(),
                "AVIF\t*.avif;*.avifs".to_string(),
                "Documents\t*.docx;*.odt".to_string(),
                "Sheets\t*.xlsx;*.xls".to_string(),
            ]
        );
        assert_eq!(filter_map.portal_indices, vec![0, 1, 2, 3, 4]);
        assert_eq!(filter_map.mime_mapped_filters, 5);
        assert_eq!(filter_map.hidden_filters, 0);
    }

    #[test]
    fn portal_result_filter_maps_supported_chooser_indices() {
        let filters = vec![
            ("MIME only".to_string(), vec![(1, "image/png".to_string())]),
            (
                "Text".to_string(),
                vec![(0, "*.txt".to_string()), (1, "text/plain".to_string())],
            ),
            ("Images".to_string(), vec![(0, "*.png".to_string())]),
        ];
        let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());
        assert_eq!(
            filter_map.chooser_specs,
            vec![
                "MIME only\t*.png".to_string(),
                "Text\t*.txt;*.text;*.md;*.markdown;*.rst;*.log;*.ini;*.conf".to_string(),
                "Images\t*.png".to_string()
            ]
        );
        assert_eq!(filter_map.portal_indices, vec![0, 1, 2]);
        assert_eq!(filter_map.mime_mapped_filters, 2);
        assert_eq!(filter_map.hidden_filters, 0);

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: Some(1),
                choices: Vec::new(),
            },
            &HashMap::new(),
            &filter_map,
        );
        let current_filter = result.get("current_filter").cloned().unwrap();
        assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);

        let out_of_range = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: Some(99),
                choices: Vec::new(),
            },
            &HashMap::new(),
            &filter_map,
        );
        assert!(!out_of_range.contains_key("current_filter"));
    }

    #[test]
    fn portal_choices_map_to_specs_and_result_choices() {
        let choices: Vec<PortalChoice> = vec![(
            "encoding".to_string(),
            "Encoding".to_string(),
            vec![
                ("utf8".to_string(), "UTF-8".to_string()),
                ("latin1".to_string(), "Latin-1".to_string()),
            ],
            "utf8".to_string(),
        )];
        assert_eq!(
            chooser_choice_specs(&choices),
            vec!["encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()]
        );

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: None,
                choices: vec![("encoding".to_string(), "latin1".to_string())],
            },
            &options_with_choices(choices.clone()),
            &ChooserFilterMap::default(),
        );
        let selected =
            Vec::<(String, String)>::try_from(result.get("choices").cloned().unwrap()).unwrap();
        assert_eq!(
            selected,
            vec![("encoding".to_string(), "latin1".to_string())]
        );
    }

    #[test]
    fn portal_choice_specs_sanitize_labels_and_drop_unsafe_ids() {
        let choices: Vec<PortalChoice> = vec![
            (
                "encoding".to_string(),
                "Encoding\tMode=Fast;Safe".to_string(),
                vec![
                    ("utf8".to_string(), "UTF-8\nDefault".to_string()),
                    ("latin;1".to_string(), "Latin-1".to_string()),
                    ("raw".to_string(), "Raw=Bytes;Exact".to_string()),
                ],
                "missing-default".to_string(),
            ),
            (
                "bad\tchoice".to_string(),
                "Broken".to_string(),
                vec![("ok".to_string(), "OK".to_string())],
                "ok".to_string(),
            ),
            (
                "empty-items".to_string(),
                "Empty Items".to_string(),
                vec![("bad=option".to_string(), "Bad".to_string())],
                "bad=option".to_string(),
            ),
        ];

        assert_eq!(
            chooser_choice_specs(&choices),
            vec!["encoding\tEncoding Mode Fast Safe\t\tutf8=UTF-8 Default;raw=Raw Bytes Exact"]
        );
    }

    #[test]
    fn portal_choice_specs_keep_original_safe_ids_for_result_mapping() {
        let choices: Vec<PortalChoice> = vec![(
            "output mode".to_string(),
            "Output Mode".to_string(),
            vec![("plain text".to_string(), "Plain Text".to_string())],
            "plain text".to_string(),
        )];

        assert_eq!(
            chooser_choice_specs(&choices),
            vec!["output mode\tOutput Mode\tplain text\tplain text=Plain Text"]
        );

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: None,
                choices: vec![("output mode".to_string(), "plain text".to_string())],
            },
            &options_with_choices(choices),
            &ChooserFilterMap::default(),
        );
        let selected =
            Vec::<(String, String)>::try_from(result.get("choices").cloned().unwrap()).unwrap();
        assert_eq!(
            selected,
            vec![("output mode".to_string(), "plain text".to_string())]
        );
    }

    #[test]
    fn portal_byte_arrays_decode_to_paths_and_file_names() {
        let mut options = HashMap::new();
        options.insert(
            "current_folder".to_string(),
            OwnedValue::try_from(Value::new(b"/tmp/fika\0".to_vec())).unwrap(),
        );
        options.insert(
            "files".to_string(),
            OwnedValue::try_from(Value::new(vec![
                b"one.txt\0".to_vec(),
                b"two.txt\0".to_vec(),
            ]))
            .unwrap(),
        );

        assert_eq!(current_folder(&options), Some(PathBuf::from("/tmp/fika")));
        assert_eq!(save_files(&options), vec!["one.txt", "two.txt"]);
    }

    #[test]
    fn portal_byte_arrays_decode_network_locations() {
        let mut options = HashMap::new();
        options.insert(
            "current_folder".to_string(),
            OwnedValue::try_from(Value::new(b"smb://server/share/\0".to_vec())).unwrap(),
        );
        options.insert(
            "current_file".to_string(),
            OwnedValue::try_from(Value::new(b"smb://server/share/report.txt\0".to_vec())).unwrap(),
        );

        assert_eq!(
            current_folder(&options),
            Some(PathBuf::from("smb://server/share/"))
        );
        assert_eq!(
            save_file_start_and_name(&options),
            (
                Some(PathBuf::from("smb://server/share/")),
                Some("report.txt".to_string())
            )
        );
    }

    #[test]
    fn portal_results_preserve_network_uris() {
        let result = results_for_paths(
            ChooserResult {
                paths: vec![
                    PathBuf::from("smb://server/share/report.txt"),
                    PathBuf::from("/tmp/local.txt"),
                ],
                filter_index: None,
                choices: Vec::new(),
            },
            &HashMap::new(),
            &ChooserFilterMap::default(),
        );
        let uris = Vec::<String>::try_from(result.get("uris").cloned().unwrap()).unwrap();

        assert_eq!(
            uris,
            vec![
                "smb://server/share/report.txt".to_string(),
                "file:///tmp/local.txt".to_string(),
            ]
        );
    }

    #[test]
    fn portal_choices_return_default_selection() {
        let choices: Vec<PortalChoice> = vec![(
            "encoding".to_string(),
            "Encoding".to_string(),
            vec![("utf8".to_string(), "UTF-8".to_string())],
            "utf8".to_string(),
        )];

        assert_eq!(
            selected_choices_for_options(&options_with_choices(choices), &[]),
            Some(vec![("encoding".to_string(), "utf8".to_string())])
        );
    }

    #[test]
    fn portal_choices_ignore_unknown_or_invalid_chooser_output() {
        let choices: Vec<PortalChoice> = vec![
            (
                "encoding".to_string(),
                "Encoding".to_string(),
                vec![
                    ("utf8".to_string(), "UTF-8".to_string()),
                    ("latin1".to_string(), "Latin-1".to_string()),
                ],
                "utf8".to_string(),
            ),
            (
                "mode".to_string(),
                "Mode".to_string(),
                vec![("read".to_string(), "Read".to_string())],
                "missing-default".to_string(),
            ),
        ];

        assert_eq!(
            selected_choices_for_options(
                &options_with_choices(choices),
                &[
                    ("encoding".to_string(), "unknown-option".to_string()),
                    ("unknown-choice".to_string(), "latin1".to_string()),
                ],
            ),
            Some(vec![
                ("encoding".to_string(), "utf8".to_string()),
                ("mode".to_string(), "read".to_string())
            ])
        );
    }

    fn options_with_choices(choices: Vec<PortalChoice>) -> HashMap<String, OwnedValue> {
        let mut options = HashMap::new();
        options.insert(
            "choices".to_string(),
            OwnedValue::try_from(Value::new(choices)).unwrap(),
        );
        options
    }

    #[test]
    fn chooser_failure_message_includes_exit_status_and_stderr() {
        assert_eq!(
            chooser_failure_message(Some(2), "bad filter"),
            "fika chooser failed with exit code 2: bad filter"
        );
        assert_eq!(
            chooser_failure_message(Some(3), ""),
            "fika chooser failed with exit code 3"
        );
        assert_eq!(
            chooser_failure_message(None, "killed"),
            "fika chooser failed with terminated by signal: killed"
        );
    }

    #[test]
    fn chooser_process_keeps_kill_on_drop_as_lifecycle_fallback() {
        let command = chooser_command(PathBuf::from("/bin/true"), vec!["--chooser".to_string()]);
        assert!(command.get_kill_on_drop());
    }

    #[tokio::test]
    async fn chooser_process_is_explicitly_terminated_for_request_close() {
        let process = ChooserProcess::spawn(chooser_command(
            PathBuf::from("/bin/sleep"),
            vec!["30".into()],
        ))
        .unwrap();
        let ChooserProcess {
            output_task,
            terminate_tx,
        } = process;

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            terminate_chooser_process(output_task, terminate_tx),
        )
        .await
        .expect("chooser termination should not wait for the full sleep")
        .expect("chooser termination should return process output");

        assert!(!output.status.success());
    }

    #[test]
    fn portal_request_close_match_targets_exact_request_handle() {
        let rule =
            request_close_match_rule("/org/freedesktop/portal/desktop/request/1_42/fika").unwrap();

        assert_eq!(
            rule.to_string(),
            "type='signal',interface='org.freedesktop.impl.portal.Request',member='Close',path='/org/freedesktop/portal/desktop/request/1_42/fika'"
        );
    }
}
