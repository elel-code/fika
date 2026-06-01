use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use zbus::fdo;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

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
}

#[derive(Debug, Default)]
struct ChooserResult {
    paths: Vec<PathBuf>,
    filter_index: Option<usize>,
    choices: Vec<(String, String)>,
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
    Cancelled,
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
        _handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let directory = option_bool(&options, "directory").unwrap_or(false);
        let multiple = option_bool(&options, "multiple").unwrap_or(false);
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let args = chooser_args(ChooserArgs {
            start_dir: current_folder(&options),
            directory,
            multiple,
            title: portal_title(title),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: chooser_choice_specs(&choices),
            return_choices: !choices.is_empty(),
            parent_window: portal_parent_window(parent_window),
            ..ChooserArgs::default()
        });
        match run_chooser(args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_file(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let (start_dir, name) = save_file_start_and_name(&options);
        let Some(name) = name else {
            return Ok(cancelled());
        };
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let args = chooser_args(ChooserArgs {
            start_dir,
            title: portal_title(title),
            save_name: Some(name),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: chooser_choice_specs(&choices),
            return_choices: !choices.is_empty(),
            parent_window: portal_parent_window(parent_window),
            ..ChooserArgs::default()
        });
        match run_chooser(args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled) => Ok(cancelled()),
            Err(err) => Err(fdo::Error::Failed(err)),
        }
    }

    #[zbus(out_args("response", "results"))]
    async fn save_files(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let files = save_files(&options);
        if files.is_empty() {
            return Ok(cancelled());
        }
        let filters = portal_filters(&options);
        let filter_map = chooser_filter_map(&options, filters);
        let choices = portal_choices(&options);
        let args = chooser_args(ChooserArgs {
            start_dir: current_folder(&options),
            directory: true,
            title: portal_title(title),
            save_files: Some(files),
            accept_label: option_string(&options, "accept_label"),
            filters: filter_map.chooser_specs.clone(),
            filter_index: filter_map.initial_chooser_index,
            return_filter: !filter_map.chooser_specs.is_empty(),
            choices: chooser_choice_specs(&choices),
            return_choices: !choices.is_empty(),
            parent_window: portal_parent_window(parent_window),
            ..ChooserArgs::default()
        });
        match run_chooser(args).await {
            Ok(ChooserRun::Selected(result)) => {
                Ok((0, results_for_paths(result, &options, &filter_map)))
            }
            Ok(ChooserRun::Cancelled) => Ok(cancelled()),
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

async fn run_chooser(args: Vec<String>) -> Result<ChooserRun, String> {
    let mut command = chooser_command(gui_executable()?, args);

    let output = command
        .output()
        .await
        .map_err(|err| format!("cannot launch fika chooser: {err}"))?;
    if !output.status.success() {
        if output.status.code() == Some(fika::chooser::CHOOSER_CANCEL_EXIT_CODE) {
            return Ok(ChooserRun::Cancelled);
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
        Ok(ChooserRun::Cancelled)
    } else {
        Ok(ChooserRun::Selected(result))
    }
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
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
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
    nul_terminated_bytes_to_path(Vec::<u8>::try_from(value.clone()).ok()?)
}

fn save_file_start_and_name(
    options: &HashMap<String, OwnedValue>,
) -> (Option<PathBuf>, Option<String>) {
    if let Some(current_file) = options
        .get("current_file")
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .and_then(nul_terminated_bytes_to_path)
    {
        let start_dir = current_file.parent().map(Path::to_path_buf);
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
        let Some(spec) = chooser_filter_spec(filter) else {
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
    }

    map
}

fn chooser_filter_spec((label, patterns): &PortalFilter) -> Option<String> {
    let globs = patterns
        .iter()
        .filter(|(kind, pattern)| *kind == 0 && !pattern.is_empty())
        .map(|(_kind, pattern)| pattern.as_str())
        .collect::<Vec<_>>();
    if globs.is_empty() {
        return None;
    }
    Some(format!("{label}\t{}", globs.join(";")))
}

fn chooser_choice_specs(choices: &[PortalChoice]) -> Vec<String> {
    choices
        .iter()
        .map(|(id, label, items, default)| {
            let items = items
                .iter()
                .map(|(item_id, item_label)| format!("{item_id}={item_label}"))
                .collect::<Vec<_>>()
                .join(";");
            format!("{id}\t{label}\t{default}\t{items}")
        })
        .collect()
}

fn nul_terminated_bytes_to_path(bytes: Vec<u8>) -> Option<PathBuf> {
    nul_terminated_bytes_to_string(bytes).map(PathBuf::from)
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

fn portal_parent_window(parent_window: String) -> Option<String> {
    let parent_window = parent_window.trim();
    let (scheme, handle) = parent_window.split_once(':')?;
    if handle.is_empty() {
        return None;
    }
    matches!(scheme, "wayland" | "x11").then(|| parent_window.to_string())
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
        .map(|path| path_to_file_uri(path))
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
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
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
        assert_eq!(portal_parent_window(String::new()), None);
        assert_eq!(portal_title(String::new()), None);
        assert_eq!(
            chooser_args(ChooserArgs {
                parent_window: portal_parent_window(String::new()),
                title: portal_title(String::new()),
                ..ChooserArgs::default()
            }),
            vec!["--chooser"]
        );
    }

    #[test]
    fn portal_parent_window_accepts_only_known_platform_handles() {
        assert_eq!(
            portal_parent_window("wayland:1_42".to_string()).as_deref(),
            Some("wayland:1_42")
        );
        assert_eq!(
            portal_parent_window(" x11:0x4600007 ".to_string()).as_deref(),
            Some("x11:0x4600007")
        );
        assert_eq!(portal_parent_window("wayland:".to_string()), None);
        assert_eq!(portal_parent_window("x11:".to_string()), None);
        assert_eq!(portal_parent_window("malformed".to_string()), None);
        assert_eq!(portal_parent_window("unsupported:window".to_string()), None);
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
            vec!["Text\t*.txt".to_string(), "Images\t*.png".to_string()]
        );

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
    fn portal_mime_only_filters_are_not_exposed_as_empty_chooser_filters() {
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

        assert_eq!(filter_map.chooser_specs, vec!["Images\t*.png".to_string()]);
        assert_eq!(filter_map.portal_indices, vec![1]);
        assert_eq!(filter_map.initial_chooser_index, None);

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.png")],
                filter_index: None,
                choices: Vec::new(),
            },
            &options,
            &filter_map,
        );
        assert!(!result.contains_key("current_filter"));
    }

    #[test]
    fn portal_result_filter_maps_only_supported_chooser_indices() {
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
            vec!["Text\t*.txt".to_string(), "Images\t*.png".to_string()]
        );
        assert_eq!(filter_map.portal_indices, vec![1, 2]);

        let result = results_for_paths(
            ChooserResult {
                paths: vec![PathBuf::from("/tmp/a.txt")],
                filter_index: Some(0),
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
    fn chooser_process_is_killed_if_portal_request_is_dropped() {
        let command = chooser_command(PathBuf::from("/bin/true"), vec!["--chooser".to_string()]);
        assert!(command.get_kill_on_drop());
    }
}
